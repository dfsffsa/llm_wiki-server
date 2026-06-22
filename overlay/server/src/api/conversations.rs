//! /api/v1/conversations* — per-user chat history. Cookie auth required.
//!
//! These routes bypass `handle_request` (which returns ApiResponse and uses
//! is_authorized) because they need the Request handle to respond directly
//! and need the user_id from `authorize()` to scope history to the user.

use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};
use tiny_http::{Method, Request};
use uuid::Uuid;

use crate::api::{self, AuthOutcome};
use crate::state::ServerState;

pub fn handle(
    state: &ServerState,
    method: &Method,
    parts: &[&str],
    body: &str,
    outcome: AuthOutcome,
    request: Request,
) {
    let user_id = match outcome {
        AuthOutcome::Cookie(id) => id,
        _ => {
            // Bearer is allowed to call API endpoints generally, but
            // /conversations is per-user — without a user there is nothing
            // to return.
            return api::respond_json(
                request, 401,
                json!({ "error": { "code": "not_authenticated", "message": "需要登录" } }),
            );
        }
    };
    let Some(auth) = state.auth() else {
        return api::respond_json(
            request, 503,
            json!({ "error": { "code": "internal_error", "message": "auth disabled" } }),
        );
    };
    let store = auth.store();

    match (method, parts) {
        (&Method::Get, ["conversations"]) => {
            match store.list_conversations(user_id, 50) {
                Ok(list) => {
                    let arr: Vec<Value> = list.into_iter().map(|c| json!({
                        "id": c.id, "project_id": c.project_id, "title": c.title,
                        "created_at": c.created_at, "updated_at": c.updated_at,
                    })).collect();
                    api::respond_json(request, 200, json!({ "conversations": arr }))
                }
                Err(e) => server_err(request, e),
            }
        }
        (&Method::Post, ["conversations"]) => {
            let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
            let project_id = v.get("project_id").and_then(Value::as_str).unwrap_or("").to_string();
            let title = v.get("title").and_then(Value::as_str).unwrap_or("新对话").to_string();
            if project_id.is_empty() {
                return api::respond_json(request, 400, json!({
                    "error": { "code": "invalid_input", "message": "project_id required" }
                }));
            }
            let id = Uuid::new_v4().to_string();
            let now = now_secs();
            let title = trim_title(&title);
            match store.create_conversation(&id, user_id, &project_id, &title, now) {
                Ok(()) => api::respond_json(request, 200, json!({
                    "id": id, "project_id": project_id, "title": title,
                    "created_at": now, "updated_at": now,
                })),
                Err(e) => server_err(request, e),
            }
        }
        (&Method::Delete, ["conversations", id]) => {
            match store.find_conversation_owner(id) {
                Ok(Some(owner)) if owner == user_id => {
                    match store.delete_conversation(id) {
                        Ok(()) => api::respond_json(request, 200, json!({ "ok": true })),
                        Err(e) => server_err(request, e),
                    }
                }
                Ok(_) => api::respond_json(request, 404, json!({
                    "error": { "code": "not_found", "message": "conversation not found" }
                })),
                Err(e) => server_err(request, e),
            }
        }
        (&Method::Get, ["conversations", id, "messages"]) => {
            match store.find_conversation_owner(id) {
                Ok(Some(owner)) if owner == user_id => {
                    match store.list_messages(id) {
                        Ok(msgs) => {
                            let arr: Vec<Value> = msgs.into_iter().map(|m| json!({
                                "role": m.role, "content": m.content, "created_at": m.created_at,
                            })).collect();
                            api::respond_json(request, 200, json!({ "messages": arr }))
                        }
                        Err(e) => server_err(request, e),
                    }
                }
                Ok(_) => api::respond_json(request, 404, json!({
                    "error": { "code": "not_found", "message": "conversation not found" }
                })),
                Err(e) => server_err(request, e),
            }
        }
        (&Method::Post, ["conversations", id, "messages"]) => {
            let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
            let role = v.get("role").and_then(Value::as_str).unwrap_or("");
            let content = v.get("content").and_then(Value::as_str).unwrap_or("");
            if !matches!(role, "user" | "assistant") || content.is_empty() {
                return api::respond_json(request, 400, json!({
                    "error": { "code": "invalid_input", "message": "role/content required" }
                }));
            }
            match store.find_conversation_owner(id) {
                Ok(Some(owner)) if owner == user_id => {
                    match store.append_message(id, role, content, now_secs()) {
                        Ok(()) => api::respond_json(request, 200, json!({ "ok": true })),
                        Err(e) => server_err(request, e),
                    }
                }
                Ok(_) => api::respond_json(request, 404, json!({
                    "error": { "code": "not_found", "message": "conversation not found" }
                })),
                Err(e) => server_err(request, e),
            }
        }
        _ => api::respond_json(request, 404, json!({
            "error": { "code": "not_found", "message": "Not found" }
        })),
    }
}

fn trim_title(s: &str) -> String {
    s.chars().take(24).collect()
}

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn server_err(request: Request, e: llm_wiki_auth::AuthError) {
    api::respond_json(
        request, 500,
        json!({ "error": { "code": e.code(), "message": e.user_message() } }),
    );
}
