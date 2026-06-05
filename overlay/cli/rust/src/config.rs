use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

pub fn load_config(path: &Path) -> Result<Value, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("Failed to read config: {e}"))?;
    let mut value: Value = serde_json::from_str(&raw).map_err(|e| format!("Invalid JSON: {e}"))?;
    expand_env_placeholders(&mut value);
    Ok(value)
}

pub fn default_config_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("LLM_WIKI_CONFIG") {
        if !path.trim().is_empty() {
            return Some(PathBuf::from(path));
        }
    }
    None
}

fn expand_env_placeholders(value: &mut Value) {
    match value {
        Value::String(s) => {
            if s.starts_with("${") && s.ends_with('}') {
                let key = &s[2..s.len() - 1];
                if let Ok(env) = std::env::var(key) {
                    *s = env;
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                expand_env_placeholders(item);
            }
        }
        Value::Object(map) => {
            for (_, v) in map {
                expand_env_placeholders(v);
            }
        }
        _ => {}
    }
}
