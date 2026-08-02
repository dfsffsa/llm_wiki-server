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
        (&Method::Post, "/auth/register") => {
            if state.disable_registration() {
                api::respond_json(
                    request,
                    403,
                    json!({ "error": { "code": "registration_closed", "message": "注册已关闭，请联系管理员开通账号" } }),
                );
                return;
            }
            handle_register(auth, state, headers, body, request)
        }
        (&Method::Post, "/auth/login") => handle_login(auth, headers, body, request),
        (&Method::Post, "/auth/logout") => handle_logout(auth, headers, request),
        (&Method::Get, "/auth/me") => handle_me(state, auth, headers, request),
        (&Method::Post, "/auth/forgot-password") => {
            handle_forgot(state, auth, headers, body, request)
        }
        (&Method::Post, "/auth/reset-password") => {
            handle_reset(auth, body, request)
        }
        (&Method::Get, "/auth/verify-email") => {
            handle_verify_email(state, auth, request)
        }
        (&Method::Post, "/auth/change-email") => {
            handle_change_email(state, auth, headers, body, request)
        }
        (&Method::Get, "/auth/confirm-email-change") => {
            handle_confirm_email_change(auth, request)
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
        "plan": u.plan,
        "plan_period_end": u.plan_period_end,
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
    state: &ServerState,
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
    let now = now_secs();

    match auth.register(RegisterInput {
        email,
        password,
        now,
        ip: header_lookup(headers, "x-forwarded-for"),
        user_agent: header_lookup(headers, "user-agent"),
    }) {
        Ok(user) => {
            // 发验证邮件
            if let Ok(token) = auth.start_verification(user.id, now) {
                let app_state = state.load_app_state().unwrap_or(serde_json::Value::Null);
                let smtp = crate::mail::parse_smtp_config(&app_state);
                match (smtp, state.runtime()) {
                    (Some(cfg), Some(rt)) if !cfg.public_base_url.is_empty() => {
                        let _ = rt.block_on(crate::mail::send_verification_email(&cfg, &user.email, &token));
                    }
                    _ => {
                        eprintln!("[auth] verification token for {}: {} (SMTP not configured)", user.email, token);
                    }
                }
            }
            // 始终返回 ok:true，防邮箱枚举
            api::respond_json(request, 200, json!({ "ok": true, "message": "验证邮件已发送，请检查邮箱" }));
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

    // `user` already carries plan + period_end from the session lookup;
    // avoid a second read of the same row.
    let plan = user.plan.clone();
    let period_end = user.plan_period_end;
    let limit = crate::api::billing::resolve_daily_limit(
        state.load_app_state().as_ref(),
        &plan,
        state.daily_chat_limit(),
    );
    let date = today_utc();
    let used = auth.store().get_usage(user.id, &date).unwrap_or(0);

    api::respond_json(
        request,
        200,
        json!({
            "user": user_to_json(&user),
            "plan": { "name": plan, "periodEnd": period_end },
            "usage": { "used": used, "limit": limit, "date": date },
        }),
    );
}

fn handle_forgot(
    state: &ServerState,
    auth: &std::sync::Arc<llm_wiki_auth::AuthService>,
    headers: &[(String, String)],
    body: &str,
    request: Request,
) {
    let v = parse_json(body).unwrap_or(Value::Null);
    let email = v.get("email").and_then(Value::as_str).unwrap_or("");
    let now = now_secs();
    // Always return ok=true regardless, to prevent email enumeration. A
    // RateLimited result also collapses to ok:true (no token emitted), so
    // the throttle works silently from the client's view.
    let token = auth
        .start_password_reset(email, now, header_lookup(headers, "x-forwarded-for"))
        .ok()
        .flatten();
    if let Some(t) = token {
        // Resolve SMTP config from the server config file. When configured,
        // email the reset link; otherwise fall back to logging the token
        // (dev/operator flow) so the feature still works without SMTP.
        let app_state = state.load_app_state().unwrap_or(Value::Null);
        let smtp = crate::mail::parse_smtp_config(&app_state);
        match (smtp, state.runtime()) {
            (Some(cfg), Some(rt)) if !cfg.public_base_url.is_empty() => {
                let reset_url = crate::mail::build_reset_url(&cfg.public_base_url, &t);
                // Send on the shared runtime. Errors are logged but never
                // surfaced to the client (ok:true either way).
                if let Err(e) = rt.block_on(crate::mail::send_password_reset(&cfg, email, &reset_url, &t)) {
                    eprintln!("[auth] password-reset email to {email} failed: {e}");
                }
            }
            _ => {
                // No SMTP / no public base URL — log the token for operator
                // out-of-band delivery. Read from journalctl.
                eprintln!("[auth] password reset token for {email}: {t} (SMTP not configured)");
            }
        }
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

fn handle_verify_email(
    state: &ServerState,
    auth: &std::sync::Arc<llm_wiki_auth::AuthService>,
    request: Request,
) {
    let now = now_secs();
    // 从 full URL 取 token（request.url() 含 query string，需 to_string() 避免借用冲突）
    let full_url = request.url().to_string();
    let token = full_url.split('?').nth(1).unwrap_or("")
        .split('&')
        .find(|p| p.starts_with("token="))
        .map(|p| &p[6..])
        .unwrap_or("")
        .to_string();

    if token.is_empty() {
        api::respond_json(request, 400, json!({ "error": { "code": "invalid_input", "message": "缺少 token" } }));
        return;
    }

    match auth.complete_verification(&token, now) {
        Ok(user) => {
            // 发欢迎邮件
            let app_state = state.load_app_state().unwrap_or(serde_json::Value::Null);
            let smtp = crate::mail::parse_smtp_config(&app_state);
            if let (Some(cfg), Some(rt)) = (smtp, state.runtime()) {
                let display_name = user.display_name.clone().unwrap_or_default();
                let _ = rt.block_on(crate::mail::send_welcome_email(&cfg, &user.email, &display_name));
            }
            // 302 跳到登录页
            let redirect = "/login?verified=true";
            let mut resp = tiny_http::Response::from_string("")
                .with_status_code(302);
            resp.add_header(
                tiny_http::Header::from_bytes("Location", redirect.as_bytes()).unwrap()
            );
            let _ = request.respond(resp);
        }
        Err(_e) => {
            let redirect = "/login?verified=failed";
            let mut resp = tiny_http::Response::from_string("")
                .with_status_code(302);
            resp.add_header(
                tiny_http::Header::from_bytes("Location", redirect.as_bytes()).unwrap()
            );
            let _ = request.respond(resp);
        }
    }
}

fn handle_change_email(
    state: &ServerState,
    auth: &std::sync::Arc<llm_wiki_auth::AuthService>,
    headers: &[(String, String)],
    body: &str,
    request: Request,
) {
    let now = now_secs();
    // 需要登录 cookie
    let token = match cookie_token(headers) {
        Some(t) => t,
        None => return respond_err(request, &AuthError::NotAuthenticated),
    };
    let user = match auth.session_user(&token, now) {
        Ok(Some(u)) => u,
        Ok(None) => return respond_err(request, &AuthError::NotAuthenticated),
        Err(e) => return respond_err(request, &e),
    };
    let v = match parse_json(body) {
        Ok(v) => v,
        Err(e) => return respond_err(request, &e),
    };
    let new_email = match json_str(&v, "email") {
        Ok(s) => s,
        Err(e) => return respond_err(request, &e),
    };

    match auth.start_email_change(user.id, new_email, now) {
        Ok((old_token, new_token)) => {
            let app_state = state.load_app_state().unwrap_or(serde_json::Value::Null);
            let smtp = crate::mail::parse_smtp_config(&app_state);
            if let (Some(cfg), Some(rt)) = (smtp, state.runtime()) {
                let base = cfg.public_base_url.trim_end_matches('/');
                let old_confirm_url = format!("{base}/auth/confirm-email-change?token={old_token}");
                let new_verify_url = format!("{base}/auth/confirm-email-change?token={new_token}");
                let _ = rt.block_on(crate::mail::send_email_change_notice(&cfg, &user.email, &old_confirm_url));
                let _ = rt.block_on(crate::mail::send_new_email_verification(&cfg, new_email, &new_verify_url));
            }
            api::respond_json(request, 200, json!({ "ok": true, "message": "确认邮件已发送" }));
        }
        Err(e) => respond_err(request, &e),
    }
}

fn handle_confirm_email_change(
    auth: &std::sync::Arc<llm_wiki_auth::AuthService>,
    request: Request,
) {
    let now = now_secs();
    let full_url = request.url().to_string();
    let token: String = full_url.split('?').nth(1).unwrap_or("")
        .split('&')
        .find(|p| p.starts_with("token="))
        .map(|p| p[6..].to_string())
        .unwrap_or_default();

    if token.is_empty() {
        api::respond_json(request, 400, json!({ "error": { "code": "invalid_input", "message": "缺少 token" } }));
        return;
    }

    match auth.confirm_email_change(&token, now) {
        Ok(llm_wiki_auth::EmailChangeStatus::Completed) => {
            let mut resp = tiny_http::Response::from_string("")
                .with_status_code(302);
            resp.add_header(
                tiny_http::Header::from_bytes("Location", b"/settings?email=changed").unwrap()
            );
            let _ = request.respond(resp);
        }
        Ok(llm_wiki_auth::EmailChangeStatus::PendingOneSide) => {
            let mut resp = tiny_http::Response::from_string("")
                .with_status_code(302);
            resp.add_header(
                tiny_http::Header::from_bytes("Location", b"/settings?email=pending").unwrap()
            );
            let _ = request.respond(resp);
        }
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
