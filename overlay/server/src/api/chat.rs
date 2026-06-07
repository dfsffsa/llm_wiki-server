use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;

use serde_json::{json, Value};
use tiny_http::{Header, Request, Response, StatusCode};

use crate::api::{self, resolve_project, API_PREFIX};
use crate::state::ServerState;

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
    if !api::is_authorized(state, &query, headers) {
        api::respond_json(request, 401, json!({ "ok": false, "error": "Unauthorized" }));
        return;
    }

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

    if !parsed_body.get("messages").map(Value::is_array).unwrap_or(false) {
        api::respond_json(
            request,
            400,
            json!({ "ok": false, "error": "Body must include messages array" }),
        );
        return;
    }

    let repo_root = repo_root();
    let script = repo_root.join("overlay/cli/node/src/cmd-llm-stream.ts");
    if !script.is_file() {
        api::respond_json(
            request,
            500,
            json!({ "ok": false, "error": "Chat stream script not found" }),
        );
        return;
    }

    let mut child = match Command::new("npx")
        .args(["tsx", script.to_str().unwrap_or_default(), "--config"])
        .arg(&config_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(repo_root.join("overlay/cli/node"))
        .env("LLM_WIKI_REPO", &repo_root)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            api::respond_json(
                request,
                500,
                json!({ "ok": false, "error": format!("Failed to start chat stream (is Node installed?): {e}") }),
            );
            return;
        }
    };

    let mut stdin = match child.stdin.take() {
        Some(s) => s,
        None => {
            api::respond_json(request, 500, json!({ "ok": false, "error": "stdin unavailable" }));
            return;
        }
    };

    let body_owned = body.to_string();
    thread::spawn(move || {
        let _ = stdin.write_all(body_owned.as_bytes());
    });

    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            api::respond_json(request, 500, json!({ "ok": false, "error": "stdout unavailable" }));
            return;
        }
    };

    let mut response = Response::new(
        StatusCode(200),
        sse_headers(),
        stdout,
        None,
        None,
    );
    for header in api::cors_headers() {
        response.add_header(header);
    }
    if let Err(e) = request.respond(response) {
        eprintln!("[chat] respond error: {e}");
    }

    // Reap child in background (stdout already consumed by response).
    thread::spawn(move || {
        let _ = child.wait();
    });
}

fn sse_headers() -> Vec<Header> {
    vec![
        Header::from_bytes("Content-Type", "text/event-stream").unwrap(),
        Header::from_bytes("Cache-Control", "no-cache").unwrap(),
        Header::from_bytes("Connection", "keep-alive").unwrap(),
    ]
}

fn repo_root() -> PathBuf {
    if let Ok(root) = std::env::var("LLM_WIKI_REPO") {
        if !root.is_empty() {
            return PathBuf::from(root);
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}
