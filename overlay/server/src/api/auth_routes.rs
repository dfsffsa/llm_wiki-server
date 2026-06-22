//! HTTP handlers for /auth/*. Thin adapter over llm_wiki_auth::AuthService.
//!
//! Cookie-based: writes `Set-Cookie: session=...` on register/login,
//! clears it on logout. /auth/me returns the current user (cookie required).

use llm_wiki_auth::{
    session::{build_clear_cookie, build_session_cookie, parse_session_cookie},
    AuthError, LoginInput, RegisterInput,
};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};
use tiny_http::{Header, Method, Request, Response, StatusCode};

use crate::api::{self, cors_headers};
use crate::state::ServerState;

pub fn handle(
    state: &ServerState,
    method: &Method,
    path: &str,
    headers: &[(String, String)],
    body: &str,
    request: Request,
) {
    let Some(auth) = state.auth() else {
        respond_err(request, &AuthError::Internal("auth disabled".into()));
        return;
    };

    match (method, path) {
        (&Method::Post, "/auth/register") => handle_register(auth, headers, body, request),
        (&Method::Post, "/auth/login") => handle_login(auth, headers, body, request),
        (&Method::Post, "/auth/logout") => handle_logout(auth, headers, request),
        (&Method::Get, "/auth/me") => handle_me(state, auth, headers, request),
        (&Method::Post, "/auth/forgot-password") => {
            handle_forgot(auth, body, request)
        }
        (&Method::Post, "/auth/reset-password") => {
            handle_reset(auth, body, request)
        }
        _ => api::respond_json(
            request,
            404,
            json!({ "error": { "code": "not_found", "message": "Not found" } }),
        ),
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn parse_json(body: &str) -> Result<Value, AuthError> {
    serde_json::from_str(body).map_err(|e| AuthError::InvalidInput(format!("invalid json: {e}")))
}

fn json_str<'a>(v: &'a Value, key: &str) -> Result<&'a str, AuthError> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| AuthError::InvalidInput(format!("missing field: {key}")))
}

fn header_lookup<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

fn is_secure(headers: &[(String, String)]) -> bool {
    header_lookup(headers, "x-forwarded-proto")
        .map(|v| v.eq_ignore_ascii_case("https"))
        .unwrap_or(false)
}

fn cookie_token(headers: &[(String, String)]) -> Option<String> {
    header_lookup(headers, "cookie").and_then(parse_session_cookie)
}

fn user_to_json(u: &llm_wiki_auth::User) -> Value {
    json!({
        "id": u.id,
        "email": u.email,
        "display_name": u.display_name,
        "is_admin": u.is_admin,
    })
}

fn respond_with_cookie(request: Request, status: u16, body: Value, cookie: String) {
    let payload = body.to_string();
    let mut resp = Response::from_string(payload).with_status_code(StatusCode(status));
    for h in cors_headers() {
        resp.add_header(h);
    }
    resp.add_header(Header::from_bytes("Set-Cookie", cookie.as_bytes()).unwrap());
    let _ = request.respond(resp);
}

fn respond_err(request: Request, err: &AuthError) {
    api::respond_json(
        request,
        err.http_status(),
        json!({ "error": { "code": err.code(), "message": err.user_message() } }),
    );
}

fn handle_register(
    auth: &std::sync::Arc<llm_wiki_auth::AuthService>,
    headers: &[(String, String)],
    body: &str,
    request: Request,
) {
    let v = match parse_json(body) {
        Ok(v) => v,
        Err(e) => return respond_err(request, &e),
    };
    let email = match json_str(&v, "email") {
        Ok(s) => s,
        Err(e) => return respond_err(request, &e),
    };
    let password = match json_str(&v, "password") {
        Ok(s) => s,
        Err(e) => return respond_err(request, &e),
    };
    let secure = is_secure(headers);
    let now = now_secs();

    match auth.register(RegisterInput {
        email,
        password,
        now,
        ip: header_lookup(headers, "x-forwarded-for"),
        user_agent: header_lookup(headers, "user-agent"),
    }) {
        Ok(out) => {
            let cookie =
                build_session_cookie(&out.session_token, auth.config().session_ttl_secs, secure);
            respond_with_cookie(
                request,
                200,
                json!({ "user": user_to_json(&out.user) }),
                cookie,
            );
        }
        Err(e) => respond_err(request, &e),
    }
}

fn handle_login(
    auth: &std::sync::Arc<llm_wiki_auth::AuthService>,
    headers: &[(String, String)],
    body: &str,
    request: Request,
) {
    let v = match parse_json(body) {
        Ok(v) => v,
        Err(e) => return respond_err(request, &e),
    };
    let email = match json_str(&v, "email") {
        Ok(s) => s,
        Err(e) => return respond_err(request, &e),
    };
    let password = match json_str(&v, "password") {
        Ok(s) => s,
        Err(e) => return respond_err(request, &e),
    };
    let secure = is_secure(headers);
    let now = now_secs();

    match auth.login(LoginInput {
        email,
        password,
        now,
        ip: header_lookup(headers, "x-forwarded-for"),
        user_agent: header_lookup(headers, "user-agent"),
    }) {
        Ok(out) => {
            let cookie =
                build_session_cookie(&out.session_token, auth.config().session_ttl_secs, secure);
            respond_with_cookie(
                request,
                200,
                json!({ "user": user_to_json(&out.user) }),
                cookie,
            );
        }
        Err(e) => respond_err(request, &e),
    }
}

fn handle_logout(
    auth: &std::sync::Arc<llm_wiki_auth::AuthService>,
    headers: &[(String, String)],
    request: Request,
) {
    if let Some(token) = cookie_token(headers) {
        let _ = auth.logout(&token); // ignore errors — always clear cookie
    }
    let secure = is_secure(headers);
    respond_with_cookie(request, 200, json!({ "ok": true }), build_clear_cookie(secure));
}

fn handle_me(
    state: &ServerState,
    auth: &std::sync::Arc<llm_wiki_auth::AuthService>,
    headers: &[(String, String)],
    request: Request,
) {
    let token = match cookie_token(headers) {
        Some(t) => t,
        None => return respond_err(request, &AuthError::NotAuthenticated),
    };
    let now = now_secs();
    let user = match auth.session_user(&token, now) {
        Ok(Some(u)) => u,
        Ok(None) => return respond_err(request, &AuthError::NotAuthenticated),
        Err(e) => return respond_err(request, &e),
    };

    // Usage info (today, UTC).
    let limit = state.daily_chat_limit() as i64;
    let date = today_utc();
    let used = auth.store().get_usage(user.id, &date).unwrap_or(0);

    api::respond_json(
        request,
        200,
        json!({
            "user": user_to_json(&user),
            "usage": { "used": used, "limit": limit, "date": date },
        }),
    );
}

fn handle_forgot(auth: &std::sync::Arc<llm_wiki_auth::AuthService>, body: &str, request: Request) {
    let v = parse_json(body).unwrap_or(Value::Null);
    let email = v.get("email").and_then(Value::as_str).unwrap_or("");
    let now = now_secs();
    // Always return ok=true regardless, to prevent email enumeration.
    let token = auth.start_password_reset(email, now).ok().flatten();
    if let Some(t) = token {
        // Print the reset URL to server stderr — wiring email delivery is
        // out of scope for v1. Operators can read this from journalctl.
        eprintln!("[auth] password reset token for {email}: {t}");
    }
    api::respond_json(request, 200, json!({ "ok": true }));
}

fn handle_reset(
    auth: &std::sync::Arc<llm_wiki_auth::AuthService>,
    body: &str,
    request: Request,
) {
    let v = match parse_json(body) {
        Ok(v) => v,
        Err(e) => return respond_err(request, &e),
    };
    let token = match json_str(&v, "token") {
        Ok(s) => s,
        Err(e) => return respond_err(request, &e),
    };
    let new_password = match json_str(&v, "password") {
        Ok(s) => s,
        Err(e) => return respond_err(request, &e),
    };
    let now = now_secs();
    match auth.complete_password_reset(token, new_password, now) {
        Ok(()) => api::respond_json(request, 200, json!({ "ok": true })),
        Err(e) => respond_err(request, &e),
    }
}

fn today_utc() -> String {
    // YYYY-MM-DD in UTC, computed from now without bringing in chrono.
    let secs = now_secs();
    let days = secs / 86_400;
    // Civil-from-days (Howard Hinnant). Same algorithm as in chat.rs.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}
