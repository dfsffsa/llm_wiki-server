use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use tiny_http::{Method, Server};

use crate::api::{self, API_PREFIX};
use crate::config::ServerConfig;
use crate::state::ServerState;
use crate::static_files;

pub fn run(config: ServerConfig) -> Result<(), String> {
    let state = ServerState::from_config(&config);
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

    if is_api {
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
        let (path, _) = api::split_url(&url);
        let path_norm = path.trim_end_matches('/');
        let is_chat_post = method == Method::Post
            && path_norm.contains("/projects/")
            && path_norm.ends_with("/chat");
        if is_chat_post {
            api::chat::try_handle_chat_sse(&state, &url, &body, &headers, request);
            return;
        }
        let response = api::handle_request(&state, &method, &url, &body, &headers);
        api::respond_json(request, response.status, response.body);
        return;
    }

    if let Some(ref root) = static_dir {
        if let Some(response) = static_files::serve_static(root, &path) {
            let _ = request.respond(response);
            return;
        }
    }

    let _ = request.respond(static_files::not_found_response());
}
