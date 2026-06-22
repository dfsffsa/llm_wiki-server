use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use tiny_http::{Method, Server};

use serde_json::json;

use crate::api::{self, API_PREFIX};
use crate::config::ServerConfig;
use crate::state::ServerState;
use crate::static_files;

pub fn run(
    config: ServerConfig,
    auth: Option<Arc<llm_wiki_auth::AuthService>>,
) -> Result<(), String> {
    let state = ServerState::from_config(&config)
        .with_auth(auth, config.require_login, config.daily_chat_limit);
    let static_dir = config.static_dir.clone();
    let bind = config.bind.clone();
    let project = config.project.display().to_string();

    eprintln!("llm-wiki-server listening on http://{bind}");
    eprintln!("  project: {project}");
    if let Some(ref dir) = static_dir {
        eprintln!("  static:  {}", dir.display());
    } else {
        eprintln!("  static:  (not configured — API only)");
    }
    eprintln!("  api:     http://{bind}{API_PREFIX}/health");

    let server = Server::http(&bind).map_err(|e| format!("Failed to bind {bind}: {e}"))?;
    let state = Arc::new(state);
    let static_dir = static_dir.map(Arc::new);

    for request in server.incoming_requests() {
        let method = request.method().clone();
        let url = request.url().to_string();
        if api::should_rate_limit(&method, &url) && !api::allow_request() {
            api::respond_error(request, 429, "Too many requests");
            continue;
        }
        let Some(slot) = api::try_acquire_request_slot() else {
            api::respond_error(request, 503, "API server is busy");
            continue;
        };
        let state = Arc::clone(&state);
        let static_dir = static_dir.clone();
        thread::spawn(move || {
            let _slot = slot;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                dispatch_request(state, static_dir, request);
            }));
            if let Err(payload) = result {
                eprintln!("[server] request handler panicked: {payload:?}");
            }
        });
    }
    Ok(())
}

fn dispatch_request(
    state: Arc<ServerState>,
    static_dir: Option<Arc<PathBuf>>,
    mut request: tiny_http::Request,
) {
    let method = request.method().clone();
    let url = request.url().to_string();

    if method == Method::Options {
        api::respond_options(request);
        return;
    }

    let (path, _) = api::split_url(&url);
    let is_api = path == "/health" || path.starts_with(API_PREFIX);
    // Auth static assets (GET /auth/*.css|js) are served from the public
    // landing dir, not the auth API. Exclude them so they fall through to
    // the landing branch below; everything else under /auth/ is the API.
    let is_auth_asset = method == Method::Get
        && path.starts_with("/auth/")
        && (path.ends_with(".css") || path.ends_with(".js"));
    let is_auth = path.starts_with("/auth/") && !is_auth_asset;

    if is_api || is_auth {
        let headers: Vec<(String, String)> = request
            .headers()
            .iter()
            .map(|header| {
                (
                    header.field.as_str().to_ascii_lowercase().to_string(),
                    header.value.as_str().to_string(),
                )
            })
            .collect();
        let body = match api::read_body(&mut request) {
            Ok(body) => body,
            Err(err) => {
                api::respond_error(request, 400, &err);
                return;
            }
        };

        if is_auth {
            api::auth_routes::handle(&state, &method, &path, &headers, &body, request);
            return;
        }

        let (path, _) = api::split_url(&url);
        let path_norm = path.trim_end_matches('/');
        let is_chat_post = method == Method::Post
            && path_norm.contains("/projects/")
            && path_norm.ends_with("/chat");
        if is_chat_post {
            api::chat::try_handle_chat_sse(&state, &url, &body, &headers, request);
            return;
        }

        // Per-user conversation history. Needs the Request handle + user_id
        // from authorize(), so it bypasses handle_request (like chat does).
        let conv_parts: Vec<&str> = path
            .trim_start_matches(API_PREFIX)
            .trim_start_matches('/')
            .split('/')
            .filter(|p| !p.is_empty())
            .collect();
        if conv_parts.first().copied() == Some("conversations") {
            let outcome = match api::authorize(&state, "", &headers) {
                Some(o) => o,
                None => {
                    api::respond_json(request, 401, json!({
                        "error": { "code": "not_authenticated", "message": "需要登录" }
                    }));
                    return;
                }
            };
            api::conversations::handle(&state, &method, &conv_parts, &body, outcome, request);
            return;
        }

        let response = api::handle_request(&state, &method, &url, &body, &headers);
        api::respond_json(request, response.status, response.body);
        return;
    }

    // Public landing pages take priority over upstream/dist for an allowlist
    // of paths when LLM_WIKI_PUBLIC_LANDING_DIR is configured. Falls through
    // (to static_dir / 404) if the file is absent, so local dev is unchanged.
    if let Some(landing_root) = state.public_landing_dir() {
        let landing_path = match path.as_str() {
            "/" => Some("index.html"),
            "/landing.css" => Some("landing.css"),
            "/landing.js" => Some("landing.js"),
            "/login" | "/register" => Some("auth/login.html"),
            "/reset-password" => Some("auth/reset.html"),
            // Auth-page static assets (GET /auth/*.css|js). Excluded from
            // is_auth above so they reach here; strip the leading "/" to get
            // the path relative to the landing dir (e.g. "auth/auth.css").
            other if other.starts_with("/auth/")
                && (other.ends_with(".css") || other.ends_with(".js")) =>
            {
                Some(&other[1..])
            }
            _ => None,
        };
        if let Some(rel) = landing_path {
            if let Some(response) = static_files::serve_file(landing_root, rel) {
                let _ = request.respond(response);
                return;
            }
        }
    }

    if let Some(ref root) = static_dir {
        if let Some(response) = static_files::serve_static(root, &path) {
            let _ = request.respond(response);
            return;
        }
    }

    let _ = request.respond(static_files::not_found_response());
}
