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

    // Invoke tsx directly via `node <cli.mjs>` rather than `npx tsx`.
    // `npx`/`npm exec` spawns intermediate `npm exec` + `sh -c` processes that
    // inherit the child's stdout pipe and stay alive after the real worker
    // exits, so the pipe's write end is never closed and the server's response
    // copy never sees EOF — the request hangs forever. Driving `node` with
    // tsx's CLI module is a single process with no lingering parents.
    let cli_dir = repo_root.join("overlay/cli/node");
    let tsx_cli = cli_dir.join("node_modules/tsx/dist/cli.mjs");
    let node_bin = std::env::var("LLM_WIKI_NODE_BIN")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "node".to_string());

    eprintln!("[chat] spawning node tsx for chat stream");
    let mut child = match Command::new(&node_bin)
        .arg(&tsx_cli)
        .arg(&script)
        .arg("--config")
        .arg(&config_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // stderr must NOT be piped-and-unread: tsx/undici write startup noise to
        // stderr, and a full pipe buffer (64KB) deadlocks the child before it
        // ever streams on stdout. Inherit so it lands in the server log.
        .stderr(Stdio::inherit())
        .current_dir(&cli_dir)
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
        // stdin drops here, closing the pipe's write end so the child's
        // readStdin() sees EOF and proceeds to stream.
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
