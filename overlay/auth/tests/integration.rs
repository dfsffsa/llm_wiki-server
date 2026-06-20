use llm_wiki_auth::password::{hash_password, verify_password};
use llm_wiki_auth::schema::init_schema;
use llm_wiki_auth::session::{build_session_cookie, generate_token, hash_token};
use rusqlite::Connection;

#[test]
fn init_schema_creates_all_tables() {
    let conn = Connection::open_in_memory().unwrap();
    init_schema(&conn).expect("init_schema ok");

    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap();
    let names: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    for required in [
        "conversation_messages",
        "conversations",
        "password_reset_tokens",
        "sessions",
        "usage_daily",
        "users",
    ] {
        assert!(
            names.iter().any(|n| n == required),
            "missing table {required}; got {names:?}"
        );
    }
}

#[test]
fn init_schema_is_idempotent() {
    let conn = Connection::open_in_memory().unwrap();
    init_schema(&conn).unwrap();
    init_schema(&conn).expect("second init must succeed (CREATE IF NOT EXISTS)");
}

#[test]
fn hash_then_verify_round_trip() {
    let h = hash_password("correct horse battery staple").unwrap();
    assert!(verify_password(&h, "correct horse battery staple").unwrap());
    assert!(!verify_password(&h, "wrong password").unwrap());
}

#[test]
fn hash_is_not_plaintext() {
    let h = hash_password("secret").unwrap();
    assert!(!h.contains("secret"));
    assert!(h.starts_with("$argon2"));
}

#[test]
fn token_is_random_and_long() {
    let a = generate_token();
    let b = generate_token();
    assert_ne!(a, b);
    // 32 bytes -> 43 base64url chars (no padding)
    assert!(a.len() >= 40);
}

#[test]
fn hash_is_deterministic() {
    let t = "any-token-string";
    assert_eq!(hash_token(t), hash_token(t));
    assert_ne!(hash_token(t), hash_token("other"));
    // sha256 hex = 64 chars
    assert_eq!(hash_token(t).len(), 64);
}

#[test]
fn cookie_has_required_attributes() {
    let c = build_session_cookie("abc", 30 * 24 * 3600, true);
    assert!(c.contains("session=abc"));
    assert!(c.contains("HttpOnly"));
    assert!(c.contains("Secure"));
    assert!(c.contains("SameSite=Lax"));
    assert!(c.contains("Path=/"));
    assert!(c.contains("Max-Age=2592000"));
}

#[test]
fn cookie_omits_secure_when_not_https() {
    let c = build_session_cookie("abc", 60, false);
    assert!(!c.contains("Secure"));
    assert!(c.contains("HttpOnly"));
}
