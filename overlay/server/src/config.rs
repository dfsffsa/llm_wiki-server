use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Resolved server configuration from CLI args and environment.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub project: PathBuf,
    pub bind: String,
    pub config_path: Option<PathBuf>,
    pub static_dir: Option<PathBuf>,
    pub token_override: Option<String>,
    pub auth_db: Option<PathBuf>,
    pub require_login: bool,
    pub disable_registration: bool,
    pub daily_chat_limit: u32,
    pub admin_email: Option<String>,
    pub session_ttl_days: u32,
    /// Directory holding the public landing page (index.html, landing.css,
    /// landing.js, auth/login.html, auth/reset.html, ...). When set, requests
    /// to `/`, `/login`, `/register`, `/reset-password` and the landing
    /// assets are served from here instead of upstream/dist. `None` disables
    /// (local dev unchanged — `/` still shows the full React UI).
    pub public_landing_dir: Option<PathBuf>,
}

impl ServerConfig {
    pub fn resolve(
        project: Option<String>,
        bind: String,
        config: Option<String>,
        static_dir: Option<String>,
        token: Option<String>,
        auth_db: Option<String>,
        require_login: bool,
        disable_registration: bool,
        daily_chat_limit: u32,
        admin_email: Option<String>,
        session_ttl_days: u32,
        public_landing_dir: Option<String>,
    ) -> Result<Self, String> {
        let project = project
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or_else(|| {
                "Wiki project path is required (--project or LLM_WIKI_PROJECT)".to_string()
            })?;

        let project = project
            .canonicalize()
            .map_err(|e| format!("Failed to resolve project path: {e}"))?;

        if !project.is_dir() {
            return Err(format!(
                "Project path is not a directory: {}",
                project.display()
            ));
        }

        let config_path = config
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| {
                p.canonicalize()
                    .map_err(|e| format!("Failed to resolve config path: {e}"))
            })
            .transpose()?;

        if let Some(ref path) = config_path {
            if !path.is_file() {
                return Err(format!("Config file not found: {}", path.display()));
            }
        }

        let static_dir = static_dir
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
            .or_else(default_static_dir)
            .map(|p| {
                if p.is_dir() {
                    Ok(p.canonicalize().unwrap_or(p))
                } else {
                    Err(format!(
                        "Static UI directory not found: {} (run: npm run build --prefix upstream)",
                        p.display()
                    ))
                }
            })
            .transpose()?;

        let auth_db = auth_db
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty());

        let admin_email = admin_email
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let public_landing_dir = public_landing_dir
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| {
                if p.is_dir() {
                    Ok(p.canonicalize().unwrap_or(p))
                } else {
                    Err(format!(
                        "Public landing directory not found: {} (set LLM_WIKI_PUBLIC_LANDING_DIR to a directory containing index.html)",
                        p.display()
                    ))
                }
            })
            .transpose()?;

        Ok(Self {
            project,
            bind,
            config_path,
            static_dir,
            token_override: token.filter(|t| !t.trim().is_empty()),
            auth_db,
            require_login,
            disable_registration,
            daily_chat_limit,
            admin_email,
            session_ttl_days,
            public_landing_dir,
        })
    }
}

fn default_static_dir() -> Option<PathBuf> {
    for candidate in [
        PathBuf::from("../../upstream/dist"),
        PathBuf::from("upstream/dist"),
        PathBuf::from("/app/dist"),
    ] {
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

/// Load and optionally expand `${VAR}` placeholders in JSON string values.
pub fn load_config_json(path: &Path) -> Option<Value> {
    let raw = fs::read_to_string(path).ok()?;
    let mut value: Value = serde_json::from_str(&raw).ok()?;
    expand_env_placeholders(&mut value);
    Some(value)
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
