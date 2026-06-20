use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use serde_json::{json, Value};
use tiny_http::Request;

use crate::api::{self, resolve_project, API_PREFIX};
use crate::state::ServerState;

/// Maximum number of concurrent chat streams. Chat requests are long-lived
/// (up to the LLM's streaming duration, potentially minutes) and each holds a
/// worker thread plus a Node subprocess. Without a separate bound, a burst of
/// slow chats would exhaust the global `MAX_IN_FLIGHT_REQUESTS` slots and
/// starve fast endpoints (health, search). Keep chat on its own, smaller leash.
const MAX_CONCURRENT_CHAT: usize = 8;
static IN_FLIGHT_CHAT: AtomicUsize = AtomicUsize::new(0);

/// RAII guard reserving one chat concurrency slot. Released on drop, which
/// happens after the stream finishes (or the client disconnects).
struct ChatSlot;
impl Drop for ChatSlot {
    fn drop(&mut self) {
        IN_FLIGHT_CHAT.fetch_sub(1, Ordering::Relaxed);
    }
}

fn try_acquire_chat_slot() -> Option<ChatSlot> {
    let mut current = IN_FLIGHT_CHAT.load(Ordering::Relaxed);
    loop {
        if current >= MAX_CONCURRENT_CHAT {
            return None;
        }
        match IN_FLIGHT_CHAT.compare_exchange_weak(
            current,
            current + 1,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return Some(ChatSlot),
            Err(next) => current = next,
        }
    }
}

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

    // Reserve a chat concurrency slot. Held until the stream ends / client
    // disconnects (ChatSlot drops at end of scope). Reject before spawning a
    // worker so a saturated server fails fast with 503 instead of queuing.
    let _chat_slot = match try_acquire_chat_slot() {
        Some(slot) => slot,
        None => {
            api::respond_json(
                request,
                503,
                json!({
                    "ok": false,
                    "error": format!(
                        "Too many concurrent chat requests (max {MAX_CONCURRENT_CHAT}). Try again shortly."
                    ),
                }),
            );
            return;
        }
    };

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
        // --no-warnings: Node's own deprecation/warning output fires before our
        // script can redirect stdout away from the SSE wire. Silence it at the
        // source so no startup noise corrupts the stream.
        .arg("--no-warnings")
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

    // Stream the child's stdout to the client with our own copy loop instead of
    // `request.respond()`. tiny_http's `respond()` path wraps the socket in a
    // `BufWriter` (1KB) and only flushes once at the end of `io::copy`, so SSE
    // events would batch up and the client saw the whole answer "jump out at
    // once" rather than stream. `into_writer()` gives us the raw socket writer;
    // we write the HTTP status line + headers + chunked body ourselves and
    // flush after every read, so each SSE event reaches the browser promptly.
    stream_chat_response(request, stdout);

    // The stream has ended — either the child finished cleanly (stdout EOF
    // after the `done` event) or the client disconnected mid-stream (tiny_http
    // surfaces that as a write error and returns from `respond`). Either way,
    // kill the child if it is still alive so a disconnected client does not
    // leave a Node process running and continuing to pull LLM tokens, then reap
    // it to avoid a zombie. `kill` on an already-exited child is a no-op error
    // we ignore. `_chat_slot` drops after this, releasing the concurrency slot.
    if let Err(e) = child.kill() {
        // ESRCH means the child already exited — expected for the normal path.
        if e.raw_os_error() != Some(3) {
            eprintln!("[chat] failed to kill child: {e}");
        }
    }
    if let Err(e) = child.wait() {
        eprintln!("[chat] failed to wait on child: {e}");
    }
}

/// Write a streaming SSE response directly to the request's raw writer, flushing
/// after every read so events aren't batched by tiny_http's internal buffer.
fn stream_chat_response<R: std::io::Read>(request: tiny_http::Request, mut body: R) {
    use std::io::Write;

    let mut writer = request.into_writer();

    // Build the response headers. We use chunked transfer-encoding (no
    // Content-Length) since the stream length is unknown up front.
    let mut header_lines: Vec<String> = vec![
        "HTTP/1.1 200 OK".to_string(),
        "Content-Type: text/event-stream".to_string(),
        "Cache-Control: no-cache".to_string(),
        "Connection: keep-alive".to_string(),
        "Transfer-Encoding: chunked".to_string(),
        "Access-Control-Allow-Origin: *".to_string(),
        "Access-Control-Allow-Methods: GET, POST, OPTIONS".to_string(),
        "Access-Control-Allow-Headers: Content-Type, Authorization, X-LLM-Wiki-Token".to_string(),
        "X-Accel-Buffering: no".to_string(), // hint proxies (nginx) not to buffer
        format!("Date: {}", http_date_now()),
        "Server: llm-wiki-server".to_string(),
    ];
    let header_blob = header_lines.join("\r\n") + "\r\n\r\n";

    if let Err(e) = writer.write_all(header_blob.as_bytes()) {
        eprintln!("[chat] failed to write headers: {e}");
        return;
    }
    let _ = writer.flush();

    // Copy the child's stdout in small chunks, flushing after each so the
    // client receives SSE events as they are produced. 64 bytes is small
    // enough to flush per-event (each SSE event is ~40-80 bytes) without
    // excessive syscalls.
    let mut buf = [0u8; 64];
    loop {
        match body.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let chunk = &buf[..n];
                // chunked-encoding frame: "<hexlen>\r\n<data>\r\n"
                if write!(writer, "{:X}\r\n", n).is_err() {
                    break;
                }
                if writer.write_all(chunk).is_err() {
                    break;
                }
                if writer.write_all(b"\r\n").is_err() {
                    break;
                }
                if writer.flush().is_err() {
                    break;
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                eprintln!("[chat] stdout read error: {e}");
                break;
            }
        }
    }

    // Terminating chunk + flush.
    let _ = writer.write_all(b"0\r\n\r\n");
    let _ = writer.flush();
    header_lines.clear();
}

/// RFC 7231 IMF-fixdate, e.g. "Sun, 06 Nov 1994 08:49:37 GMT".
fn http_date_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    epoch_to_http_date(secs)
}

/// Convert a Unix epoch second to an HTTP date without pulling in a time crate.
fn epoch_to_http_date(secs: u64) -> String {
    const DAY: u64 = 86_400;
    let days = secs / DAY;
    let sod = secs % DAY; // seconds of day
    let hour = sod / 3600;
    let min = (sod % 3600) / 60;
    let sec = sod % 60;

    // 1970-01-01 was a Thursday (weekday index 4 if Sun=0).
    let weekday = (4 + days) % 7;
    let wd = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][weekday as usize];

    // Civil-from-days algorithm (Howard Hinnant).
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][(m as usize) - 1];

    format!("{wd}, {d:02} {month} {y:04} {hour:02}:{min:02}:{sec:02} GMT")
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
