use serde_json::{json, Value};

use crate::api::{self, err, ok};
use crate::state::ServerState;

pub fn handle_runtime_config(state: &ServerState) -> api::ApiResponse {
    let Some(config_path) = state.config_path() else {
        return ok(json!({
            "ok": true,
            "chatEnabled": false,
            "reason": "LLM_WIKI_CONFIG not set",
            "llmConfig": null,
        }));
    };

    let parsed = match crate::config::load_config_json(&config_path) {
        Some(v) => v,
        None => {
            return err(500, "Failed to load server config");
        }
    };

    let llm = parsed.get("llmConfig");
    let chat_enabled = llm_config_usable(llm);

    let sanitized = llm.and_then(sanitize_llm_config);

    ok(json!({
        "ok": true,
        "chatEnabled": chat_enabled,
        "reason": if chat_enabled { Value::Null } else { json!("llmConfig.model and llmConfig.apiKey (or keyless provider) required in LLM_WIKI_CONFIG") },
        "llmConfig": sanitized,
    }))
}

fn llm_config_usable(llm: Option<&Value>) -> bool {
    let Some(llm) = llm else {
        return false;
    };
    let provider = llm
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("");
    let model = llm.get("model").and_then(Value::as_str).unwrap_or("");
    if model.trim().is_empty() {
        return false;
    }
    if matches!(provider, "ollama" | "custom" | "claude-code" | "codex-cli") {
        return true;
    }
    llm.get("apiKey")
        .and_then(Value::as_str)
        .map(|k| !k.trim().is_empty())
        .unwrap_or(false)
}

fn sanitize_llm_config(llm: &Value) -> Option<Value> {
    let mut out = llm.clone();
    if let Some(obj) = out.as_object_mut() {
        obj.remove("apiKey");
        obj.insert("apiKey".to_string(), json!(""));
        obj.insert("serverProxy".to_string(), json!(true));
    }
    Some(out)
}
