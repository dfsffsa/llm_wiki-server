use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::borrow::ToOwned;

use serde_json::Value;

use crate::config::{load_config_json, ServerConfig};

const APP_STATE_CACHE_TTL: Duration = Duration::from_secs(5);

#[derive(Clone)]
struct CachedAppState {
    loaded_at: Instant,
    value: Option<Value>,
}

/// Shared server state passed to API handlers (replaces Tauri `AppHandle`).
#[derive(Clone)]
pub struct ServerState {
    inner: Arc<ServerStateInner>,
}

struct ServerStateInner {
    project: PathBuf,
    config_path: Option<PathBuf>,
    token_override: Option<String>,
    config_cache: Mutex<Option<CachedAppState>>,
    auth: Option<Arc<llm_wiki_auth::AuthService>>,
    require_login: bool,
    daily_chat_limit: u32,
    public_landing_dir: Option<PathBuf>,
}

impl ServerState {
    pub fn from_config(config: &ServerConfig) -> Self {
        Self {
            inner: Arc::new(ServerStateInner {
                project: config.project.clone(),
                config_path: config.config_path.clone(),
                token_override: config.token_override.clone(),
                config_cache: Mutex::new(None),
                auth: None,
                require_login: false,
                daily_chat_limit: 50,
                public_landing_dir: config.public_landing_dir.clone(),
            }),
        }
    }

    pub fn project_path(&self) -> &PathBuf {
        &self.inner.project
    }

    pub fn config_path(&self) -> Option<PathBuf> {
        self.inner.config_path.clone()
    }

    pub fn invalidate_config_cache(&self) {
        if let Ok(mut cache) = self.inner.config_cache.lock() {
            *cache = None;
        }
    }

    /// Attach the AuthService (built at startup) and auth-mode flags. Consumes
    /// self and returns a new ServerState wrapping a fresh inner Arc that
    /// carries the auth values. Called once at startup.
    pub fn with_auth(
        self,
        auth: Option<Arc<llm_wiki_auth::AuthService>>,
        require_login: bool,
        daily_chat_limit: u32,
    ) -> Self {
        let inner = Arc::new(ServerStateInner {
            project: self.inner.project.clone(),
            config_path: self.inner.config_path.clone(),
            token_override: self.inner.token_override.clone(),
            config_cache: Mutex::new(None),
            auth,
            require_login,
            daily_chat_limit,
            public_landing_dir: self.inner.public_landing_dir.clone(),
        });
        Self { inner }
    }

    pub fn auth(&self) -> Option<&Arc<llm_wiki_auth::AuthService>> {
        self.inner.auth.as_ref()
    }

    pub fn require_login(&self) -> bool {
        self.inner.require_login
    }

    pub fn daily_chat_limit(&self) -> u32 {
        self.inner.daily_chat_limit
    }

    /// Public landing-page root, if configured (LLM_WIKI_PUBLIC_LANDING_DIR).
    pub fn public_landing_dir(&self) -> Option<&Path> {
        self.inner.public_landing_dir.as_deref()
    }

    pub fn load_app_state(&self) -> Option<Value> {
        let now = Instant::now();
        let mut previous = None;
        if let Ok(cache) = self.inner.config_cache.lock() {
            if let Some(cached) = cache.as_ref() {
                if now.duration_since(cached.loaded_at) < APP_STATE_CACHE_TTL {
                    return cached.value.clone();
                }
                previous = cached.value.clone();
            }
        }

        let loaded = self
            .inner
            .config_path
            .as_ref()
            .and_then(|path| load_config_json(path));

        let value = loaded.or(previous);

        if let Ok(mut cache) = self.inner.config_cache.lock() {
            *cache = Some(CachedAppState {
                loaded_at: now,
                value: value.clone(),
            });
        }
        value
    }

    pub fn api_token(&self) -> Option<String> {
        if let Some(token) = &self.inner.token_override {
            let trimmed = token.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        if let Ok(token) = std::env::var("LLM_WIKI_API_TOKEN") {
            let trimmed = token.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        self.load_app_state()
            .and_then(|parsed| {
                parsed
                    .get("apiConfig")
                    .and_then(|v| v.get("token"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned)
            })
    }

    pub fn api_token_source(&self) -> &'static str {
        if self
            .inner
            .token_override
            .as_ref()
            .map(|t| !t.trim().is_empty())
            .unwrap_or(false)
        {
            return "env";
        }
        if let Ok(token) = std::env::var("LLM_WIKI_API_TOKEN") {
            if !token.trim().is_empty() {
                return "env";
            }
        }
        if self
            .load_app_state()
            .and_then(|parsed| {
                parsed
                    .get("apiConfig")
                    .and_then(|v| v.get("token"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(|_| ())
            })
            .is_some()
        {
            return "config";
        }
        "none"
    }

    pub fn api_allow_unauthenticated(&self) -> bool {
        self.load_app_state()
            .and_then(|parsed| {
                parsed
                    .get("apiConfig")
                    .and_then(|v| v.get("allowUnauthenticated"))
                    .and_then(Value::as_bool)
            })
            .unwrap_or(false)
    }

    pub fn api_auth_required(&self) -> bool {
        !self.api_allow_unauthenticated()
    }

    pub fn api_enabled(&self) -> bool {
        self.load_app_state()
            .and_then(|parsed| {
                parsed
                    .get("apiConfig")
                    .and_then(|v| v.get("enabled"))
                    .and_then(Value::as_bool)
            })
            .unwrap_or(true)
    }

}
