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

// --- Store tests (Task 2.2) ---

use llm_wiki_auth::store::{NewUser, Store};
use tempfile::TempDir;

fn fresh_store() -> (Store, TempDir) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("auth.db");
    let store = Store::open(&path).expect("open store");
    (store, dir)
}

#[test]
fn create_user_and_find_by_email() {
    let (store, _dir) = fresh_store();
    let now = 1_700_000_000;
    let id = store
        .create_user(NewUser {
            email: "alice@example.com",
            password_hash: "$argon2id$dummy",
            display_name: None,
            is_admin: false,
            now,
        })
        .unwrap();
    assert!(id > 0);

    let user = store.find_user_by_email("alice@example.com").unwrap().unwrap();
    assert_eq!(user.id, id);
    assert_eq!(user.email, "alice@example.com");
    assert_eq!(user.password_hash, "$argon2id$dummy");
    assert!(!user.is_admin);
}

#[test]
fn duplicate_email_returns_email_already_exists() {
    let (store, _dir) = fresh_store();
    let n = NewUser {
        email: "bob@example.com",
        password_hash: "x",
        display_name: None,
        is_admin: false,
        now: 1,
    };
    store.create_user(n.clone()).unwrap();
    let err = store.create_user(n).unwrap_err();
    assert!(matches!(err, llm_wiki_auth::AuthError::EmailAlreadyExists));
}

#[test]
fn session_lifecycle() {
    let (store, _dir) = fresh_store();
    let uid = store
        .create_user(NewUser {
            email: "c@e.com",
            password_hash: "x",
            display_name: None,
            is_admin: false,
            now: 1,
        })
        .unwrap();
    store
        .create_session("hash1", uid, /*now*/ 100, /*expires*/ 1000, None, None)
        .unwrap();
    let found = store.find_session_user("hash1", /*now*/ 200).unwrap();
    assert_eq!(found, Some(uid));
    // expired
    let expired = store.find_session_user("hash1", /*now*/ 2000).unwrap();
    assert_eq!(expired, None);
}

#[test]
fn delete_session_clears_it() {
    let (store, _dir) = fresh_store();
    let uid = store
        .create_user(NewUser {
            email: "d@e.com",
            password_hash: "x",
            display_name: None,
            is_admin: false,
            now: 1,
        })
        .unwrap();
    store.create_session("h", uid, 1, 1000, None, None).unwrap();
    store.delete_session("h").unwrap();
    assert_eq!(store.find_session_user("h", 2).unwrap(), None);
}

#[test]
fn delete_user_sessions_clears_all() {
    let (store, _dir) = fresh_store();
    let uid = store
        .create_user(NewUser {
            email: "e@e.com",
            password_hash: "x",
            display_name: None,
            is_admin: false,
            now: 1,
        })
        .unwrap();
    store.create_session("h1", uid, 1, 1000, None, None).unwrap();
    store.create_session("h2", uid, 1, 1000, None, None).unwrap();
    store.delete_user_sessions(uid).unwrap();
    assert_eq!(store.find_session_user("h1", 2).unwrap(), None);
    assert_eq!(store.find_session_user("h2", 2).unwrap(), None);
}

#[test]
fn usage_increment_counts_per_day() {
    let (store, _dir) = fresh_store();
    let uid = store
        .create_user(NewUser {
            email: "u@e.com",
            password_hash: "x",
            display_name: None,
            is_admin: false,
            now: 1,
        })
        .unwrap();
    assert_eq!(store.get_usage(uid, "2026-06-20").unwrap(), 0);
    store.increment_usage(uid, "2026-06-20").unwrap();
    store.increment_usage(uid, "2026-06-20").unwrap();
    assert_eq!(store.get_usage(uid, "2026-06-20").unwrap(), 2);
    assert_eq!(store.get_usage(uid, "2026-06-21").unwrap(), 0);
}

use llm_wiki_auth::ratelimit::RateLimiter;

#[test]
fn rate_limit_blocks_after_quota() {
    let rl = RateLimiter::new();
    // 3 attempts per 60 seconds
    for _ in 0..3 {
        assert!(rl.allow("login:alice", 3.0, 60.0, 1_000));
    }
    assert!(!rl.allow("login:alice", 3.0, 60.0, 1_001));
}

#[test]
fn rate_limit_isolates_keys() {
    let rl = RateLimiter::new();
    for _ in 0..3 {
        rl.allow("login:alice", 3.0, 60.0, 1_000);
    }
    // bob still has full quota
    assert!(rl.allow("login:bob", 3.0, 60.0, 1_000));
}

#[test]
fn rate_limit_refills_over_time() {
    let rl = RateLimiter::new();
    for _ in 0..3 {
        rl.allow("k", 3.0, 60.0, 1_000);
    }
    assert!(!rl.allow("k", 3.0, 60.0, 1_000));
    // After 60s, the bucket has fully refilled.
    assert!(rl.allow("k", 3.0, 60.0, 1_060));
}
