use std::fs;
use std::io::Read;
use std::path::Path;

use tiny_http::{Header, Response, StatusCode};

const INDEX_HTML: &str = "index.html";

pub fn serve_static(root: &Path, url_path: &str) -> Option<Response<std::io::Cursor<Vec<u8>>>> {
    let path = url_path
        .split('?')
        .next()?
        .trim_start_matches('/')
        .trim_end_matches('/');
    let file_path = if path.is_empty() {
        root.join(INDEX_HTML)
    } else {
        let candidate = root.join(path);
        if candidate.is_file() {
            candidate
        } else {
            let dir_index = candidate.join(INDEX_HTML);
            if dir_index.is_file() {
                dir_index
            } else if !path.contains('.') {
                // SPA fallback for client-side routes (e.g. /settings), not subdirs like /lite/
                root.join(INDEX_HTML)
            } else {
                candidate
            }
        }
    };

    if !file_path.exists() || !file_path.is_file() {
        return None;
    }

    let bytes = fs::read(&file_path).ok()?;
    let mut response = Response::from_data(bytes).with_status_code(StatusCode(200));
    if let Some(mime) = mime_for_path(&file_path) {
        let _ = response.add_header(Header::from_bytes("Content-Type", mime).ok()?);
    }
    let _ = response.add_header(
        Header::from_bytes("Cache-Control", "public, max-age=3600")
            .ok()?,
    );
    Some(response)
}

pub fn not_found_response() -> Response<std::io::Cursor<Vec<u8>>> {
    let body = b"Not found".to_vec();
    Response::from_data(body).with_status_code(StatusCode(404))
}

/// Serve a single file by its path *relative* to `root` (e.g. `"index.html"`,
/// `"auth/login.html"`). Unlike `serve_static`, this does no SPA fallback —
/// the exact file must exist or `None` is returned. Used for the public
/// landing pages, which must take priority over upstream/dist without
/// shadowing it for other paths.
///
/// `rel` is sanitized: a leading `/` is stripped and any `..` path component
/// is rejected to prevent traversal outside `root`.
pub fn serve_file(root: &Path, rel: &str) -> Option<Response<std::io::Cursor<Vec<u8>>>> {
    let rel = rel.trim_start_matches('/');
    if rel.is_empty() || rel.split('/').any(|c| c == "..") {
        return None;
    }
    let file_path = root.join(rel);
    if !file_path.is_file() {
        return None;
    }
    let bytes = fs::read(&file_path).ok()?;
    let mut response = Response::from_data(bytes).with_status_code(StatusCode(200));
    if let Some(mime) = mime_for_path(&file_path) {
        let _ = response.add_header(Header::from_bytes("Content-Type", mime).ok()?);
    }
    let _ = response.add_header(
        Header::from_bytes("Cache-Control", "public, max-age=3600").ok()?,
    );
    Some(response)
}

pub fn read_body_limited(
    request: &mut tiny_http::Request,
    max_bytes: usize,
) -> Result<String, String> {
    let mut limited = request.as_reader().take(max_bytes as u64 + 1);
    let mut bytes = Vec::new();
    limited
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read body: {e}"))?;
    if bytes.len() > max_bytes {
        return Err("Request body too large".to_string());
    }
    String::from_utf8(bytes).map_err(|_| "Request body must be UTF-8".to_string())
}

fn mime_for_path(path: &Path) -> Option<&'static str> {
    match path.extension().and_then(|s| s.to_str()) {
        Some("html") => Some("text/html; charset=utf-8"),
        Some("js") | Some("mjs") => Some("application/javascript; charset=utf-8"),
        Some("css") => Some("text/css; charset=utf-8"),
        Some("json") => Some("application/json; charset=utf-8"),
        Some("svg") => Some("image/svg+xml"),
        Some("png") => Some("image/png"),
        Some("jpg") | Some("jpeg") => Some("image/jpeg"),
        Some("webp") => Some("image/webp"),
        Some("woff") => Some("font/woff"),
        Some("woff2") => Some("font/woff2"),
        Some("ico") => Some("image/x-icon"),
        Some("txt") => Some("text/plain; charset=utf-8"),
        _ => Some("application/octet-stream"),
    }
}
