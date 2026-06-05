mod files;
mod graph;
mod projects;
mod search;

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tiny_http::{Header, Method, StatusCode};

use crate::state::ServerState;

pub const API_PREFIX: &str = "/api/v1";
const MAX_BODY_BYTES: usize = 1024 * 1024;
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(1);
const RATE_LIMIT_MAX_REQUESTS: usize = 120;
const MAX_IN_FLIGHT_REQUESTS: usize = 64;

static IN_FLIGHT_REQUESTS: AtomicUsize = AtomicUsize::new(0);
static RATE_LIMIT: OnceLock<Mutex<VecDeque<Instant>>> = OnceLock::new();

pub struct ApiResponse {
    pub status: u16,
    pub body: Value,
}

pub fn ok(body: Value) -> ApiResponse {
    ApiResponse { status: 200, body }
}

pub fn err(status: u16, message: impl Into<String>) -> ApiResponse {
    ApiResponse {
        status,
        body: json!({ "ok": false, "error": message.into() }),
    }
}

pub fn handle_request(
    state: &ServerState,
    method: &Method,
    url: &str,
    body: &str,
    headers: &[(String, String)],
) -> ApiResponse {
    let (path, query) = split_url(url);
    if path == "/health" || path == format!("{API_PREFIX}/health") {
        return ok(json!({
            "ok": true,
            "status": "running",
            "mode": "headless",
            "version": env!("CARGO_PKG_VERSION"),
            "authRequired": state.api_auth_required(),
            "authConfigured": state.api_token().is_some(),
            "tokenSource": state.api_token_source(),
            "enabled": state.api_enabled(),
            "allowUnauthenticated": state.api_allow_unauthenticated(),
        }));
    }
    if !path.starts_with(API_PREFIX) {
        return err(404, "Not found");
    }
    if !state.api_enabled() {
        return err(503, "API server is disabled in configuration");
    }
    if !is_authorized(state, query, headers) {
        return err(401, "Unauthorized");
    }
    if !matches!(method, &Method::Get | &Method::Post) {
        return err(405, "Method not allowed");
    }

    let parts: Vec<&str> = path
        .trim_start_matches(API_PREFIX)
        .trim_start_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();

    match (method, parts.as_slice()) {
        (&Method::Get, ["projects"]) => projects::handle_projects(state),
        (&Method::Get, ["projects", project_id, "files"]) => {
            files::handle_files(state, project_id, query)
        }
        (&Method::Get, ["projects", project_id, "files", "content"]) => {
            files::handle_file_content(state, project_id, query)
        }
        (&Method::Post, ["projects", project_id, "search"]) => {
            search::handle_search(state, project_id, body)
        }
        (&Method::Get, ["projects", project_id, "graph"]) => {
            graph::handle_graph(state, project_id, query)
        }
        (&Method::Post, ["projects", _project_id, "sources", "rescan"]) => err(
            501,
            "Source rescan is not available in headless mode yet. Use the CLI (Phase 3) or the desktop app.",
        ),
        (&Method::Post, ["projects", project_id, "chat"]) => {
            let _ = project_id;
            err(
                501,
                "Chat API is not implemented in the headless server yet.",
            )
        }
        _ => err(404, "Not found"),
    }
}

pub fn should_rate_limit(method: &Method, url: &str) -> bool {
    if method == &Method::Options {
        return false;
    }
    let (path, _) = split_url(url);
    !(path == "/health" || path == format!("{API_PREFIX}/health"))
}

pub fn allow_request() -> bool {
    let now = Instant::now();
    let window_start = now - RATE_LIMIT_WINDOW;
    let lock = RATE_LIMIT.get_or_init(|| Mutex::new(VecDeque::new()));
    let Ok(mut hits) = lock.lock() else {
        return false;
    };
    while hits.front().map(|t| *t < window_start).unwrap_or(false) {
        hits.pop_front();
    }
    if hits.len() >= RATE_LIMIT_MAX_REQUESTS {
        return false;
    }
    hits.push_back(now);
    true
}

pub(crate) struct RequestSlot;

impl Drop for RequestSlot {
    fn drop(&mut self) {
        IN_FLIGHT_REQUESTS.fetch_sub(1, Ordering::Relaxed);
    }
}

pub fn try_acquire_request_slot() -> Option<RequestSlot> {
    let mut current = IN_FLIGHT_REQUESTS.load(Ordering::Relaxed);
    loop {
        if current >= MAX_IN_FLIGHT_REQUESTS {
            return None;
        }
        match IN_FLIGHT_REQUESTS.compare_exchange_weak(
            current,
            current + 1,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return Some(RequestSlot),
            Err(next) => current = next,
        }
    }
}

pub fn read_body(request: &mut tiny_http::Request) -> Result<String, String> {
    crate::static_files::read_body_limited(request, MAX_BODY_BYTES)
}

pub fn respond_error(request: tiny_http::Request, status: u16, message: &str) {
    respond_json(request, status, json!({ "ok": false, "error": message }));
}

pub fn respond_options(request: tiny_http::Request) {
    let mut response = tiny_http::Response::empty(StatusCode(204));
    for header in cors_headers() {
        response.add_header(header);
    }
    response.add_header(Header::from_bytes("Access-Control-Max-Age", "600").unwrap());
    let _ = request.respond(response);
}

pub fn respond_json(request: tiny_http::Request, status: u16, body: Value) {
    let mut response =
        tiny_http::Response::from_string(body.to_string()).with_status_code(StatusCode(status));
    for header in cors_headers() {
        response.add_header(header);
    }
    let _ = request.respond(response);
}

pub fn cors_headers() -> Vec<Header> {
    vec![
        Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap(),
        Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, OPTIONS").unwrap(),
        Header::from_bytes(
            "Access-Control-Allow-Headers",
            "Content-Type, Authorization, X-LLM-Wiki-Token",
        )
        .unwrap(),
        Header::from_bytes("Content-Type", "application/json").unwrap(),
    ]
}

pub fn split_url(url: &str) -> (String, &str) {
    match url.split_once('?') {
        Some((path, query)) => (path.to_string(), query),
        None => (url.to_string(), ""),
    }
}

pub fn parse_query(query: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for pair in query.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        out.insert(percent_decode(k), percent_decode(v));
    }
    out
}

pub fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&input[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn is_authorized(state: &ServerState, query: &str, headers: &[(String, String)]) -> bool {
    if !state.api_auth_required() {
        return true;
    }
    let Some(token) = state.api_token() else {
        return false;
    };
    let params = parse_query(query);
    if params
        .get("token")
        .map(|v| constant_time_eq(v.as_bytes(), token.as_bytes()))
        .unwrap_or(false)
    {
        return true;
    }
    headers.iter().any(|(key, value)| {
        if key == "x-llm-wiki-token" {
            return constant_time_eq(value.as_bytes(), token.as_bytes());
        }
        if key == "authorization" {
            return value
                .strip_prefix("Bearer ")
                .map(|v| constant_time_eq(v.as_bytes(), token.as_bytes()))
                .unwrap_or(false);
        }
        false
    })
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let max_len = left.len().max(right.len());
    let mut diff = left.len() ^ right.len();
    for i in 0..max_len {
        let a = left.get(i).copied().unwrap_or(0);
        let b = right.get(i).copied().unwrap_or(0);
        diff |= (a ^ b) as usize;
    }
    diff == 0
}

pub fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_string()
}

pub fn project_name_from_path(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("Project")
        .to_string()
}

pub fn read_project_id(path: &str) -> Option<String> {
    let raw =
        std::fs::read_to_string(std::path::Path::new(path).join(".llm-wiki/project.json")).ok()?;
    let parsed: Value = serde_json::from_str(&raw).ok()?;
    parsed
        .get("id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

pub fn project_path_matches(stored_path: &str, candidate: &str) -> bool {
    let stored = normalize_path(stored_path);
    let candidate = normalize_path(candidate);
    if cfg!(windows) {
        stored.eq_ignore_ascii_case(&candidate)
    } else {
        stored == candidate
    }
}

pub fn resolve_project(state: &ServerState, project_id: &str) -> Result<projects::ProjectEntry, String> {
    let project_id = percent_decode(project_id);
    let wants_current = project_id.eq_ignore_ascii_case("current");
    projects::load_projects(state)
        .into_iter()
        .find(|p| {
            p.id == project_id
                || project_path_matches(&p.path, &project_id)
                || (wants_current && p.current)
        })
        .ok_or_else(|| format!("Unknown project: {project_id}"))
}
