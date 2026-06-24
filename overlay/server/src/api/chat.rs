use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{json, Value};
use tiny_http::Request;

use crate::api::{self, resolve_project, API_PREFIX};
use crate::llm::{parse_llm_config, stream_chat, ChatSink};
use crate::state::ServerState;

/// Maximum number of concurrent chat streams. Chat requests are long-lived
/// (up to the LLM's streaming duration, potentially minutes) and each holds a
/// worker thread. Without a separate bound, a burst of slow chats would
/// exhaust the global `MAX_IN_FLIGHT_REQUESTS` slots and starve fast
/// endpoints (health, search). Keep chat on its own, smaller leash.
const MAX_CONCURRENT_CHAT: usize = 8;
static IN_FLIGHT_CHAT: AtomicUsize = AtomicUsize::new(0);

/// RAII guard reserving one chat concurrency slot. Released on drop, which
/// happens after the stream finishes (or the client disconnects).
struct ChatSlot;
impl Drop for ChatSlot {
    fn drop(&mut self) {
        IN_FLIGHT_CHAT.fetch_sub(1, Ordering::Relaxed);
    }
}

fn try_acquire_chat_slot() -> Option<ChatSlot> {
    let mut current = IN_FLIGHT_CHAT.load(Ordering::Relaxed);
    loop {
        if current >= MAX_CONCURRENT_CHAT {
            return None;
        }
        match IN_FLIGHT_CHAT.compare_exchange_weak(
            current,
            current + 1,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return Some(ChatSlot),
            Err(next) => current = next,
        }
    }
}

pub fn try_handle_chat_sse(
    state: &ServerState,
    url: &str,
    body: &str,
    headers: &[(String, String)],
    request: Request,
) {
    let (path, query) = api::split_url(url);

    if !state.api_enabled() {
        api::respond_json(request, 503, json!({ "ok": false, "error": "API server is disabled" }));
        return;
    }
    let auth_outcome = match api::authorize(state, &query, headers) {
        Some(o) => o,
        None => {
            api::respond_json(request, 401, json!({ "ok": false, "error": "Unauthorized" }));
            return;
        }
    };

    let parts: Vec<&str> = path
        .trim_start_matches(API_PREFIX)
        .trim_start_matches('/')
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();

    if parts.len() != 3 || parts[0] != "projects" || parts[2] != "chat" {
        api::respond_json(request, 404, json!({ "ok": false, "error": "Not found" }));
        return;
    }

    let project_id = parts[1];
    let project = match resolve_project(state, project_id) {
        Ok(p) => p,
        Err(e) => {
            api::respond_json(request, 404, json!({ "ok": false, "error": e }));
            return;
        }
    };
    let _ = project;

    let config_path = match state.config_path() {
        Some(p) => p,
        None => {
            api::respond_json(
                request,
                503,
                json!({ "ok": false, "error": "Chat requires LLM_WIKI_CONFIG with llmConfig" }),
            );
            return;
        }
    };

    let parsed_body: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            api::respond_json(
                request,
                400,
                json!({ "ok": false, "error": format!("Invalid JSON body: {e}") }),
            );
            return;
        }
    };

    let messages = match parsed_body.get("messages") {
        Some(m) if m.is_array() => m.clone(),
        _ => {
            api::respond_json(
                request,
                400,
                json!({ "ok": false, "error": "Body must include messages array" }),
            );
            return;
        }
    };

    // Load llmConfig from the config file (expands ${VAR} placeholders).
    let app_state = crate::config::load_config_json(&config_path).unwrap_or(Value::Null);
    let llm_config = match parse_llm_config(&app_state) {
        Some(c) => c,
        None => {
            api::respond_json(
                request,
                503,
                json!({ "ok": false, "error": "config.llmConfig (with model) is required for chat" }),
            );
            return;
        }
    };

    let Some(runtime) = state.runtime() else {
        api::respond_json(
            request,
            503,
            json!({ "ok": false, "error": "async runtime unavailable" }),
        );
        return;
    };

    // Per-user daily quota — only applies to cookie-authenticated requests.
    // Bearer-token clients (CLI / e2e) bypass the quota by design.
    if let api::AuthOutcome::Cookie(user_id) = auth_outcome {
        if let Some(auth) = state.auth() {
            let date = today_utc_for_chat();
            // NOTE: get_usage + increment_usage are two separate Store mutex
            // acquisitions, so this is a TOCTOU window — concurrent chats
            // from the same user can both pass the check and both increment,
            // letting a user exceed the limit by up to (concurrency - 1).
            // Acceptable for v1 (single browser, sequential chats). A single
            // conditional UPSERT in store.rs would close it atomically.
            let used = match auth.store().get_usage(user_id, &date) {
                Ok(n) => n,
                Err(_) => 0,
            };
            let limit = state.daily_chat_limit() as i64;
            if used >= limit {
                api::respond_json(
                    request, 429,
                    json!({
                        "ok": false,
                        "error": {
                            "code": "daily_limit_exceeded",
                            "message": "今日额度已用完,明日重置",
                            "used": used, "limit": limit,
                        }
                    }),
                );
                return;
            }
            // Increment BEFORE streaming — even if streaming fails partway, the
            // attempt counts (consistent with most LLM products).
            if let Err(e) = auth.store().increment_usage(user_id, &date) {
                eprintln!("[chat] usage increment failed for user {user_id}: {e}");
            }
        }
    }

    // Reserve a chat concurrency slot. Held until the stream ends / client
    // disconnects (ChatSlot drops at end of scope). Reject before streaming
    // so a saturated server fails fast with 503 instead of queuing.
    let _chat_slot = match try_acquire_chat_slot() {
        Some(slot) => slot,
        None => {
            api::respond_json(
                request,
                503,
                json!({
                    "ok": false,
                    "error": format!(
                        "Too many concurrent chat requests (max {MAX_CONCURRENT_CHAT}). Try again shortly."
                    ),
                }),
            );
            return;
        }
    };

    eprintln!("[chat] streaming via reqwest (no node subprocess)");
    let mut responder = ChatResponder::new(request);
    // Stream the LLM completion. This blocks the worker thread for the
    // stream duration (bounded by MAX_CONCURRENT_CHAT), exactly as the old
    // node-subprocess version did — no thread-pool regression.
    let result = runtime.block_on(stream_chat(&llm_config, &messages, &mut responder));

    match result {
        Ok(()) => responder.finish_done(),
        Err(e) => {
            eprintln!("[chat] stream error: {e}");
            responder.finish_error(&e);
        }
    }
}

/// Drives the SSE response: writes the HTTP chunked stream and implements
/// `ChatSink` so `stream_chat` can push token/reasoning fragments. Tracks
/// liveness via write success so a disconnected client aborts the upstream
/// pull promptly.
struct ChatResponder {
    writer: Option<Box<dyn Write + Send>>,
    alive: bool,
    headers_sent: bool,
}

impl ChatResponder {
    fn new(request: Request) -> Self {
        let writer = request.into_writer();
        Self {
            writer: Some(writer),
            alive: true,
            headers_sent: false,
        }
    }

    fn ensure_headers(&mut self) {
        if self.headers_sent {
            return;
        }
        let Some(w) = self.writer.as_mut() else {
            self.alive = false;
            return;
        };
        let header_blob = [
            "HTTP/1.1 200 OK",
            "Content-Type: text/event-stream",
            "Cache-Control: no-cache",
            "Connection: keep-alive",
            "Transfer-Encoding: chunked",
            "Access-Control-Allow-Origin: *",
            "Access-Control-Allow-Methods: GET, POST, OPTIONS",
            "Access-Control-Allow-Headers: Content-Type, Authorization, X-LLM-Wiki-Token",
            "X-Accel-Buffering: no",
            &format!("Date: {}", http_date_now()),
            "Server: llm-wiki-server",
        ]
        .join("\r\n");
        let blob = format!("{header_blob}\r\n\r\n");
        if w.write_all(blob.as_bytes()).is_err() {
            self.alive = false;
            return;
        }
        let _ = w.flush();
        self.headers_sent = true;
    }

    fn write_frame(&mut self, event: &str, data: &Value) {
        if !self.alive {
            return;
        }
        self.ensure_headers();
        if !self.alive {
            return;
        }
        // Wire format matches the old Node implementation exactly:
        // `data: {"event":"token","data":{"token":"..."}}\n\n`. The web client
        // (overlay/web/lib/llm-client.ts) parses `parsed.event` + `parsed.data`,
        // so both keys are required.
        let frame = build_sse_frame(event, data);
        let Some(w) = self.writer.as_mut() else {
            self.alive = false;
            return;
        };
        if w.write_all(frame.as_bytes()).is_err() {
            self.alive = false;
            return;
        }
        if w.flush().is_err() {
            self.alive = false;
        }
    }

    fn finish_done(&mut self) {
        self.write_frame("done", &json!({}));
        self.end_stream();
    }

    fn finish_error(&mut self, message: &str) {
        self.write_frame("error", &json!({ "message": message }));
        self.end_stream();
    }

    fn end_stream(&mut self) {
        if let Some(w) = self.writer.as_mut() {
            let _ = w.write_all(b"0\r\n\r\n");
            let _ = w.flush();
        }
    }
}

impl ChatSink for ChatResponder {
    fn write_token(&mut self, fragment: &str) {
        self.write_frame("token", &json!({ "token": fragment }));
    }
    fn write_reasoning(&mut self, fragment: &str) {
        self.write_frame("reasoning", &json!({ "token": fragment }));
    }
    fn is_alive(&mut self) -> bool {
        self.alive
    }
}

/// RFC 7231 IMF-fixdate, e.g. "Sun, 06 Nov 1994 08:49:37 GMT".
fn http_date_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    epoch_to_http_date(secs)
}

/// Convert a Unix epoch second to an HTTP date without pulling in a time crate.
fn epoch_to_http_date(secs: u64) -> String {
    const DAY: u64 = 86_400;
    let days = secs / DAY;
    let sod = secs % DAY; // seconds of day
    let hour = sod / 3600;
    let min = (sod % 3600) / 60;
    let sec = sod % 60;

    // 1970-01-01 was a Thursday (weekday index 4 if Sun=0).
    let weekday = (4 + days) % 7;
    let wd = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][weekday as usize];

    // Civil-from-days algorithm (Howard Hinnant).
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][(m as usize) - 1];

    format!("{wd}, {d:02} {month} {y:04} {hour:02}:{min:02}:{sec:02} GMT")
}

fn today_utc_for_chat() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs / 86_400;
    // Civil-from-days (Howard Hinnant). UTC date from unix epoch days.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Build one HTTP chunked-encoding frame carrying an SSE event.
///
/// The SSE payload is `data: {"event":<e>,"data":<d>}\n\n` (matching the
/// Node implementation and the web client's parser). The chunked frame wraps
/// it as `<hex-len>\r\n<payload>\r\n`. Pure / IO-free so the wire format is
/// regression-tested.
fn build_sse_frame(event: &str, data: &Value) -> String {
    let payload = format!("data: {}\n\n", json!({ "event": event, "data": data }));
    format!("{:X}\r\n{payload}\r\n", payload.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_frame_wraps_event_and_data_for_web_client() {
        let frame = build_sse_frame("token", &json!({ "token": "hi" }));
        // The web client parses `parsed.event` and `parsed.data.token`, so
        // both keys must be present in the JSON.
        assert!(frame.contains(r#""event":"token""#), "missing event: {frame}");
        assert!(frame.contains(r#""data":{"token":"hi"}"#), "missing data: {frame}");
        assert!(frame.contains("data: "), "missing SSE data: prefix: {frame}");
    }

    #[test]
    fn sse_frame_done_has_empty_data() {
        let frame = build_sse_frame("done", &json!({}));
        assert!(frame.contains(r#""event":"done""#));
        assert!(frame.contains(r#""data":{}"#));
    }

    #[test]
    fn sse_frame_uses_chunked_hex_length_prefix() {
        let frame = build_sse_frame("token", &json!({ "token": "x" }));
        // First line is the hex length of the payload, then CRLF.
        let first_line = frame.split("\r\n").next().unwrap();
        assert!(usize::from_str_radix(first_line, 16).is_ok(), "bad hex prefix: {first_line}");
    }
}
