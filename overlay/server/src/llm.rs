//! LLM client for the headless server: query embedding + chat streaming.
//!
//! Replaces the per-request `node tsx cmd-llm-stream.ts` subprocess with a
//! direct reqwest call. Only the OpenAI-compatible (`chat_completions`) wire
//! is supported; other `apiMode` values are rejected up front with a clear
//! error so the caller can fall back to the desktop app.
//!
//! The SSE wire format written to the browser is identical to the old Node
//! implementation (see `overlay/web/lib/llm-client.ts`): one JSON object per
//! `data:` line, `{event, data}`, events `token` / `reasoning` / `done` /
//! `error`.

use serde::Serialize;
use serde_json::{json, Value};

/// Parsed event from one upstream SSE `data:` line.
#[derive(Debug, PartialEq, Eq)]
pub enum StreamEvent {
    /// A content token (OpenAI `choices[0].delta.content`).
    Token(String),
    /// A chain-of-thought token (`choices[0].delta.reasoning_content` or
    /// `reasoning`, emitted by DeepSeek / Qwen-flavored endpoints).
    Reasoning(String),
    /// The terminal `[DONE]` sentinel.
    Done,
}

/// `<think>...</think>` content-stream splitter.
///
/// Some OpenAI-compatible models (MiniMax et al.) inline chain-of-thought as
/// literal `<think>` text inside the content stream rather than a structured
/// `reasoning_content` field. This state machine splits the token stream:
/// text inside `<think>` becomes `Reasoning`, everything else becomes
/// `Token`. Tags may straddle token boundaries — a hold-back buffer keeps the
/// tail that could be the start of a tag until more arrives.
#[derive(Debug)]
pub struct ThinkSplitter {
    in_think: bool,
    holdback: String,
}

impl ThinkSplitter {
    pub fn new() -> Self {
        Self {
            in_think: false,
            holdback: String::new(),
        }
    }

    /// Feed one content token; returns the `(token, reasoning)` fragments that
    /// can be emitted immediately. Some bytes may be retained in the hold-back
    /// buffer pending a tag-boundary decision.
    pub fn route(&mut self, token: &str) -> (Vec<String>, Vec<String>) {
        const OPEN: &str = "<think>";
        const CLOSE: &str = "</think>";
        self.holdback.push_str(token);
        let mut tokens: Vec<String> = Vec::new();
        let mut reasoning: Vec<String> = Vec::new();
        loop {
            let (tag, is_open) = if self.in_think { (CLOSE, false) } else { (OPEN, true) };
            match self.holdback.find(tag) {
                Some(idx) => {
                    let head: String = self.holdback.drain(..idx).collect();
                    if !head.is_empty() {
                        if self.in_think {
                            reasoning.push(head);
                        } else {
                            tokens.push(head);
                        }
                    }
                    let tag_len = tag.len();
                    self.holdback.drain(..tag_len);
                    self.in_think = is_open;
                    continue;
                }
                None => {
                    // No full tag present. Hold back only the longest suffix
                    // that is a prefix of the tag (could be the start of a tag
                    // split across tokens); emit the rest now.
                    let safe_len = self.holdback.len() - tag_prefix_overlap(&self.holdback, tag);
                    if safe_len > 0 {
                        let head: String = self.holdback.drain(..safe_len).collect();
                        if self.in_think {
                            reasoning.push(head);
                        } else {
                            tokens.push(head);
                        }
                    }
                    break;
                }
            }
        }
        (tokens, reasoning)
    }

    /// Flush at stream end: emit any buffered text according to the current
    /// mode. Returns the final `(token, reasoning)` fragment.
    pub fn flush(&mut self) -> (Vec<String>, Vec<String>) {
        if self.holdback.is_empty() {
            return (Vec::new(), Vec::new());
        }
        let remainder = std::mem::take(&mut self.holdback);
        if self.in_think {
            (Vec::new(), vec![remainder])
        } else {
            (vec![remainder], Vec::new())
        }
    }
}

impl Default for ThinkSplitter {
    fn default() -> Self {
        Self::new()
    }
}

/// Length of the longest suffix of `text` that is also a prefix of `tag`.
/// Used to decide how much to hold back when a tag might be split across
/// tokens. E.g. `tag_prefix_overlap("a<thi", "<think>")` = 4 (the whole tail
/// could be the start of `<think>`), while `tag_prefix_overlap("hidden", "</think>")` = 0.
fn tag_prefix_overlap(text: &str, tag: &str) -> usize {
    let text_bytes = text.as_bytes();
    let tag_bytes = tag.as_bytes();
    let max = text_bytes.len().min(tag_bytes.len() - 1);
    let mut best = 0;
    for k in (1..=max).rev() {
        if text_bytes[text_bytes.len() - k..] == tag_bytes[..k] {
            best = k;
            break;
        }
    }
    best
}

/// Build the OpenAI chat-completions URL from a config endpoint.
///
/// If the endpoint already ends with `/chat/completions` (case-insensitive),
/// use it verbatim; otherwise append `/chat/completions`. Trailing slashes on
/// the base are stripped first.
pub fn build_chat_url(endpoint: &str) -> String {
    let trimmed = endpoint.trim_end_matches('/');
    if trimmed.to_ascii_lowercase().ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/chat/completions")
    }
}

/// Build the OpenAI-compatible request body for a chat completion stream.
///
/// `messages` is the raw JSON `messages` array from the client (already in
/// `{role, content}` form). We set `stream: true` and inject the `model`.
pub fn build_openai_body(model: &str, messages: &Value) -> Value {
    json!({
        "model": model,
        "stream": true,
        "messages": messages,
    })
}

/// Parse one upstream SSE line into a `StreamEvent`.
///
/// Returns `None` for blank lines, comments, keep-alives, or lines that
/// don't carry a usable payload. `[DONE]` → `Done`. Otherwise the JSON
/// object's `choices[0].delta` is inspected for `content` (Token) and
/// `reasoning_content` / `reasoning` (Reasoning). When both are present,
/// reasoning is reported (the caller emits it before the content token to
/// match the Node ordering); here we return Reasoning if present, else Token.
pub fn parse_sse_line(line: &str) -> Option<StreamEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with(':') {
        return None;
    }
    let payload = trimmed.strip_prefix("data: ")?;
    let payload = payload.trim();
    if payload == "[DONE]" {
        return Some(StreamEvent::Done);
    }
    let parsed: Value = serde_json::from_str(payload).ok()?;
    let delta = parsed.get("choices")?.get(0)?.get("delta")?;
    if let Some(r) = delta.get("reasoning_content").and_then(Value::as_str) {
        if !r.is_empty() {
            return Some(StreamEvent::Reasoning(r.to_string()));
        }
    }
    if let Some(r) = delta.get("reasoning").and_then(Value::as_str) {
        if !r.is_empty() {
            return Some(StreamEvent::Reasoning(r.to_string()));
        }
    }
    if let Some(c) = delta.get("content").and_then(Value::as_str) {
        if !c.is_empty() {
            return Some(StreamEvent::Token(c.to_string()));
        }
    }
    None
}

#[derive(Debug, Clone, Serialize)]
pub struct LlmConfig {
    pub provider: String,
    pub api_key: String,
    pub model: String,
    pub custom_endpoint: String,
    pub api_mode: String,
}

#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
}

/// Extract an `EmbeddingConfig` from the parsed server config JSON
/// (`embeddingConfig` block), if present and enabled. Returns `None` when
/// embedding is disabled or unconfigured, so callers can treat `None` as
/// "keyword-only".
pub fn parse_embedding_config(app_state: &Value) -> Option<EmbeddingConfig> {
    let cfg = app_state.get("embeddingConfig")?;
    let enabled = cfg.get("enabled").and_then(Value::as_bool).unwrap_or(false);
    if !enabled {
        return None;
    }
    let endpoint = cfg.get("endpoint").and_then(Value::as_str)?.to_string();
    if endpoint.is_empty() {
        return None;
    }
    Some(EmbeddingConfig {
        endpoint,
        api_key: cfg.get("apiKey").and_then(Value::as_str).unwrap_or("").to_string(),
        model: cfg.get("model").and_then(Value::as_str).unwrap_or("").to_string(),
    })
}

/// Extract an `LlmConfig` from the parsed server config JSON (`llmConfig`).
pub fn parse_llm_config(app_state: &Value) -> Option<LlmConfig> {
    let cfg = app_state.get("llmConfig")?;
    let model = cfg.get("model").and_then(Value::as_str)?.to_string();
    if model.is_empty() {
        return None;
    }
    Some(LlmConfig {
        provider: cfg.get("provider").and_then(Value::as_str).unwrap_or("custom").to_string(),
        api_key: cfg.get("apiKey").and_then(Value::as_str).unwrap_or("").to_string(),
        model,
        custom_endpoint: cfg.get("customEndpoint").and_then(Value::as_str).unwrap_or("").to_string(),
        api_mode: cfg.get("apiMode").and_then(Value::as_str).unwrap_or("chat_completions").to_string(),
    })
}

/// Embed a query string via the configured OpenAI-compatible embeddings
/// endpoint. Returns the embedding vector or an error describing why the
/// vector channel is unavailable. The HTTP search caller treats any `Err` as
/// "degrade to keyword-only".
pub async fn embed_query(cfg: &EmbeddingConfig, text: &str) -> Result<Vec<f32>, String> {
    if cfg.model.is_empty() {
        return Err("embeddingConfig.model is empty".to_string());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("http client build: {e}"))?;
    let body = json!({ "model": cfg.model, "input": text });
    let mut req = client.post(&cfg.endpoint).header("Content-Type", "application/json");
    if !cfg.api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", cfg.api_key));
    }
    let resp = req
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("embedding request: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("embedding endpoint {status}: {}", truncate(&text, 200)));
    }
    let parsed: Value = resp
        .json()
        .await
        .map_err(|e| format!("embedding response parse: {e}"))?;
    let arr = parsed
        .get("data")
        .and_then(|d| d.get(0))
        .and_then(|d| d.get("embedding"))
        .and_then(Value::as_array)
        .ok_or_else(|| "embedding response missing data[0].embedding".to_string())?;
    let vec: Vec<f32> = arr
        .iter()
        .filter_map(|v| v.as_f64().map(|f| f as f32))
        .collect();
    if vec.is_empty() {
        return Err("embedding response had empty vector".to_string());
    }
    Ok(vec)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

/// Callbacks the chat streamer drives. Each `&str` is one fragment to write
/// to the SSE wire (already routed into token vs reasoning by the splitter).
pub trait ChatSink: Send {
    fn write_token(&mut self, fragment: &str);
    fn write_reasoning(&mut self, fragment: &str);
    /// Returns `false` if the downstream client has disconnected and the
    /// stream should abort (so we stop pulling LLM tokens).
    fn is_alive(&mut self) -> bool;
}

/// Stream an OpenAI-compatible chat completion to `sink`. Reads the upstream
/// SSE response line by line, routes content through the `<think>` splitter,
/// and dispatches reasoning_content / reasoning deltas directly. Returns
/// `Ok(())` on clean completion (including `[DONE]`), `Err` on upstream
/// failure.
pub async fn stream_chat(
    cfg: &LlmConfig,
    messages: &Value,
    sink: &mut dyn ChatSink,
) -> Result<(), String> {
    if !cfg.api_mode.is_empty() && cfg.api_mode != "chat_completions" {
        return Err(format!(
            "chat via server only supports apiMode=chat_completions (got \"{}\"); \
             use the desktop app or switch the LLM endpoint",
            cfg.api_mode
        ));
    }
    if cfg.custom_endpoint.is_empty() {
        return Err("llmConfig.customEndpoint is empty".to_string());
    }
    let url = build_chat_url(&cfg.custom_endpoint);
    let body = build_openai_body(&cfg.model, messages);

    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| format!("http client build: {e}"))?;
    let mut req = client.post(&url).header("Content-Type", "application/json");
    if !cfg.api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", cfg.api_key));
    }
    let resp = req
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("chat request: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("chat endpoint {status}: {}", truncate(&text, 300)));
    }

    use futures::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut line_buf = String::new();
    let mut splitter = ThinkSplitter::new();
    let mut saw_done = false;

    while let Some(chunk) = stream.next().await {
        if !sink.is_alive() {
            // Client disconnected — stop pulling tokens.
            return Ok(());
        }
        let chunk = chunk.map_err(|e| format!("stream read: {e}"))?;
        line_buf.push_str(&String::from_utf8_lossy(&chunk));
        loop {
            let Some((line, rest)) = split_line(&line_buf) else {
                break;
            };
            line_buf = rest;
            match parse_sse_line(&line) {
                Some(StreamEvent::Token(t)) => {
                    let (toks, reas) = splitter.route(&t);
                    for r in reas {
                        sink.write_reasoning(&r);
                    }
                    for t in toks {
                        sink.write_token(&t);
                    }
                }
                Some(StreamEvent::Reasoning(r)) => sink.write_reasoning(&r),
                Some(StreamEvent::Done) => {
                    saw_done = true;
                }
                None => {}
            }
            if saw_done {
                break;
            }
        }
        if saw_done {
            break;
        }
    }

    // Flush any held-back content, then signal completion.
    let (toks, reas) = splitter.flush();
    for r in reas {
        sink.write_reasoning(&r);
    }
    for t in toks {
        sink.write_token(&t);
    }
    Ok(())
}

/// Split `buf` at the first `\n`; returns `(line_without_newline, remainder)`.
/// Returns `None` if no newline is present (caller should wait for more).
fn split_line(buf: &str) -> Option<(String, String)> {
    let idx = buf.find('\n')?;
    let line = buf[..idx].trim_end_matches('\r').to_string();
    let rest = buf[idx + 1..].to_string();
    Some((line, rest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- build_chat_url ---

    #[test]
    fn chat_url_appends_when_bare_endpoint() {
        assert_eq!(
            build_chat_url("https://api.example.com/v1"),
            "https://api.example.com/v1/chat/completions"
        );
    }

    #[test]
    fn chat_url_does_not_double_append() {
        assert_eq!(
            build_chat_url("https://api.example.com/v1/chat/completions"),
            "https://api.example.com/v1/chat/completions"
        );
    }

    #[test]
    fn chat_url_strips_trailing_slash() {
        assert_eq!(
            build_chat_url("https://api.example.com/v1/"),
            "https://api.example.com/v1/chat/completions"
        );
    }

    // --- build_openai_body ---

    #[test]
    fn openai_body_sets_model_stream_and_messages() {
        let messages = json!([{ "role": "user", "content": "hi" }]);
        let body = build_openai_body("gpt-4o", &messages);
        assert_eq!(body["model"], "gpt-4o");
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"], messages);
    }

    // --- parse_sse_line ---

    #[test]
    fn parse_done_sentinel() {
        assert_eq!(parse_sse_line("data: [DONE]"), Some(StreamEvent::Done));
    }

    #[test]
    fn parse_content_delta() {
        let line = r#"data: {"choices":[{"delta":{"content":"hello"}}]}"#;
        assert_eq!(parse_sse_line(line), Some(StreamEvent::Token("hello".into())));
    }

    #[test]
    fn parse_reasoning_content_delta() {
        let line = r#"data: {"choices":[{"delta":{"reasoning_content":"thinking"}}]}"#;
        assert_eq!(
            parse_sse_line(line),
            Some(StreamEvent::Reasoning("thinking".into()))
        );
    }

    #[test]
    fn parse_blank_line_is_none() {
        assert_eq!(parse_sse_line(""), None);
        assert_eq!(parse_sse_line(": keepalive"), None);
    }

    #[test]
    fn parse_empty_delta_is_none() {
        // role-only delta or empty content: no token to emit.
        let line = r#"data: {"choices":[{"delta":{"role":"assistant"}}]}"#;
        assert_eq!(parse_sse_line(line), None);
    }

    // --- ThinkSplitter ---

    #[test]
    fn splitter_plain_text_emits_as_token() {
        let mut s = ThinkSplitter::new();
        let (tok, rea) = s.route("hello world");
        assert!(rea.is_empty());
        assert_eq!(tok.concat(), "hello world");
    }

    #[test]
    fn splitter_think_block_routes_to_reasoning() {
        let mut s = ThinkSplitter::new();
        let (t1, r1) = s.route("before<think>hidden");
        let (t2, r2) = s.route("</think>after");
        assert_eq!(t1.concat(), "before");
        assert_eq!(r1.concat(), "hidden");
        assert_eq!(t2.concat(), "after");
        assert!(r2.is_empty());
    }

    #[test]
    fn splitter_tag_split_across_tokens() {
        let mut s = ThinkSplitter::new();
        // "<thi" then "nk>" — the tag straddles the boundary.
        let (t1, _r1) = s.route("a<thi");
        let (t2, r2) = s.route("nk>secret");
        assert_eq!(t1.concat(), "a");
        assert!(t2.is_empty());
        assert_eq!(r2.concat(), "secret");
    }

    #[test]
    fn splitter_flush_emits_held_back_as_token() {
        let mut s = ThinkSplitter::new();
        // "hello" emitted now; "<" held (could be the start of <think>).
        s.route("hello<");
        let (t, r) = s.flush();
        assert!(r.is_empty());
        assert_eq!(t.concat(), "<");
    }

    #[test]
    fn splitter_flush_unfinished_think_emits_held_as_reasoning() {
        let mut s = ThinkSplitter::new();
        // Enter think; "secret" emitted as reasoning; "<" held (could be the
        // start of </think>). Stream ends before the tag completes.
        s.route("<think>secret<");
        let (t, r) = s.flush();
        assert!(t.is_empty());
        assert_eq!(r.concat(), "<");
    }
}
