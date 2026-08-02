use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use tiny_http::{Method, Server};

use serde_json::json;

use crate::api::{self, API_PREFIX};
use crate::audit;
use crate::config::ServerConfig;
use crate::state::ServerState;
use crate::static_files;

pub fn run(
    config: ServerConfig,
    auth: Option<Arc<llm_wiki_auth::AuthService>>,
    runtime: Option<Arc<tokio::runtime::Runtime>>,
) -> Result<(), String> {
    let state = ServerState::from_config(&config)
        .with_auth(auth, config.require_login, config.disable_registration, config.daily_chat_limit, runtime);
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

    // In-house access audit. Disabled unless LLM_WIKI_AUDIT_DIR is set; a
    // failure to open the directory only disables auditing, never the server.
    // (Named `audit_log`, not `audit`, so the value doesn't shadow the module.)
    let audit_log: Option<Arc<audit::AuditLog>> = match config.audit_dir {
        Some(ref dir) => match audit::AuditLog::new(dir.clone(), config.audit_retention_days) {
            Ok(log) => Some(Arc::new(log)),
            Err(e) => {
                eprintln!("[audit] disabled: failed to open {}: {e}", dir.display());
                None
            }
        },
        None => None,
    };

    for request in server.incoming_requests() {
        // Metadata is extracted before `request` is moved into the worker
        // thread. `path` intentionally drops the query string so tokens in
        // URLs (`/auth/verify-email?token=...`) never reach the log.
        let started = std::time::Instant::now();
        let method = request.method().clone();
        let url = request.url().to_string();
        let (path, _) = api::split_url(&url);
        let ip = audit::client_ip(&request);
        let ua = request
            .headers()
            .iter()
            .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case("user-agent"))
            .map(|h| audit::truncate(h.value.as_str(), 256))
            .unwrap_or_default();
        let request_id = uuid::Uuid::new_v4().simple().to_string();

        // During shutdown, fast-reject new connections so in-flight work can
        // drain instead of queuing more.
        if api::is_shutting_down() {
            record_audit(&audit_log, &request_id, &method, &path, &ip, &ua, 503, started);
            api::respond_error(request, 503, "Server is shutting down");
            continue;
        }
        if api::should_rate_limit(&method, &url) && !api::allow_request() {
            record_audit(&audit_log, &request_id, &method, &path, &ip, &ua, 429, started);
            api::respond_error(request, 429, "Too many requests");
            continue;
        }
        let Some(slot) = api::try_acquire_request_slot() else {
            record_audit(&audit_log, &request_id, &method, &path, &ip, &ua, 503, started);
            api::respond_error(request, 503, "API server is busy");
            continue;
        };
        let state = Arc::clone(&state);
        let static_dir = static_dir.clone();
        let audit_log = audit_log.clone();
        thread::spawn(move || {
            let _slot = slot;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                dispatch_request(state, static_dir, request, &request_id);
            }));
            // The respond helpers record the final status into a thread-local;
            // read it back here (per-request thread, so it's unambiguous).
            let status = match result {
                Err(payload) => {
                    eprintln!("[server] request handler panicked: {payload:?}");
                    500
                }
                Ok(()) => audit::last_status().unwrap_or(200),
            };
            record_audit(&audit_log, &request_id, &method, &path, &ip, &ua, status, started);
        });
    }
    Ok(())
}

/// Append one request to the audit log, if enabled.
fn record_audit(
    audit_log: &Option<Arc<audit::AuditLog>>,
    request_id: &str,
    method: &Method,
    path: &str,
    ip: &str,
    ua: &str,
    status: u16,
    started: std::time::Instant,
) {
    if let Some(log) = audit_log {
        log.record(&audit::Entry {
            method: method.as_str().to_string(),
            path: path.to_string(),
            status,
            ip: ip.to_string(),
            ua: ua.to_string(),
            ms: started.elapsed().as_millis() as u64,
            req: request_id.to_string(),
        });
    }
}

fn dispatch_request(
    state: Arc<ServerState>,
    static_dir: Option<Arc<PathBuf>>,
    mut request: tiny_http::Request,
    request_id: &str,
) {
    let method = request.method().clone();
    let url = request.url().to_string();
    let (path, _) = api::split_url(&url);

    // Per-request tracing span. The request_id (generated in the accept loop,
    // shared with the audit log) propagates into every log line emitted while
    // handling this request, so a single request's trace is grep-able in the
    // journal and joinable to its audit record.
    let span = tracing::info_span!("request", request_id = %request_id, %method, path = %path);
    let _enter = span.enter();
    tracing::info!("dispatch");
    crate::metrics::inc_requests();

    if method == Method::Options {
        api::respond_options(request);
        return;
    }

    // /metrics: Prometheus text exposition. Public (no auth) — the metric
    // set is non-sensitive counts/gauges. Scrapers have no bearer token.
    if method == Method::Get && path == "/metrics" {
        let body = crate::metrics::render(
            api::in_flight_count(),
            api::chat::in_flight_chat_count(),
            api::is_shutting_down(),
        );
        let mut resp = tiny_http::Response::from_string(body)
            .with_status_code(tiny_http::StatusCode(200));
        resp.add_header(
            tiny_http::Header::from_bytes("Content-Type", "text/plain; version=0.0.4; charset=utf-8")
                .unwrap(),
        );
        crate::audit::set_status(200);
        let _ = request.respond(resp);
        return;
    }

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

        // Billing (Waffo Pancake): /api/v1/billing/checkout + /api/v1/billing/webhook.
        // Webhook is signature-verified (no cookie) and must bypass handle_request's
        // generic auth, so route it here alongside conversations.
        let billing_parts: Vec<&str> = path
            .trim_start_matches(API_PREFIX)
            .trim_start_matches('/')
            .split('/')
            .filter(|p| !p.is_empty())
            .collect();
        if billing_parts.first().copied() == Some("billing") {
            api::billing::handle(&state, &method, &billing_parts, &body, &headers, request);
            return;
        }

        let response = api::handle_request(&state, &method, &url, &body, &headers);
        if response.status >= 500 {
            crate::metrics::inc_errors();
            tracing::warn!(status = response.status, "request failed");
        }
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
            "/i18n.js" => Some("i18n.js"),
            "/login" => Some("auth/login.html"),
            "/register" => Some("auth/register.html"),
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
                crate::audit::set_status(200);
                let _ = request.respond(response);
                return;
            }
        }
    }

    if let Some(ref root) = static_dir {
        if let Some(response) = static_files::serve_static(root, &path) {
            crate::audit::set_status(200);
            let _ = request.respond(response);
            return;
        }
    }

    crate::audit::set_status(404);
    let _ = request.respond(static_files::not_found_response());
}
