//! Session tokens.
//!
//! - `generate_token()` returns a 32-byte random base64url string (no padding).
//!   This is what the browser stores in the cookie.
//! - `hash_token()` returns the sha256 hex digest. The DB only stores the
//!   hash so a leak of the sessions table cannot be replayed as cookies.
//! - `build_session_cookie()` produces a `Set-Cookie` value with the security
//!   attributes the spec requires: HttpOnly, SameSite=Lax, Secure (only when
//!   the request was over HTTPS — the caller decides via `secure`).

use base64::Engine as _;
use rand::RngCore;
use sha2::{Digest, Sha256};

const TOKEN_BYTES: usize = 32;

pub fn generate_token() -> String {
    let mut buf = [0u8; TOKEN_BYTES];
    rand::thread_rng().fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn build_session_cookie(token: &str, max_age_secs: i64, secure: bool) -> String {
    let mut attrs = vec![
        format!("session={token}"),
        format!("Max-Age={max_age_secs}"),
        "Path=/".to_string(),
        "HttpOnly".to_string(),
        "SameSite=Lax".to_string(),
    ];
    if secure {
        attrs.push("Secure".to_string());
    }
    attrs.join("; ")
}

/// Build the `Set-Cookie` value that clears the session.
pub fn build_clear_cookie(secure: bool) -> String {
    build_session_cookie("", 0, secure)
}

/// Parse a session token out of a `Cookie:` header value, if present.
/// Returns `None` if not present.
pub fn parse_session_cookie(cookie_header: &str) -> Option<String> {
    for part in cookie_header.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("session=") {
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}
