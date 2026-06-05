use std::fs;
use std::io::Read;
use std::path::Path;

use tiny_http::{Header, Response, StatusCode};

const INDEX_HTML: &str = "index.html";

pub fn serve_static(root: &Path, url_path: &str) -> Option<Response<std::io::Cursor<Vec<u8>>>> {
    let path = url_path.split('?').next()?.trim_start_matches('/');
    let file_path = if path.is_empty() {
        root.join(INDEX_HTML)
    } else {
        let candidate = root.join(path);
        if candidate.is_file() {
            candidate
        } else if !path.contains('.') {
            root.join(INDEX_HTML)
        } else {
            candidate
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
        Some("js") => Some("application/javascript; charset=utf-8"),
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
