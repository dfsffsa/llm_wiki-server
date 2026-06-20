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

// --- AuthService tests (Task 2.4) ---

use llm_wiki_auth::service::{AuthService, AuthServiceConfig, LoginInput, RegisterInput};
use std::sync::Arc;

fn fresh_service() -> (Arc<AuthService>, TempDir) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("auth.db");
    let store = Arc::new(Store::open(&path).unwrap());
    let cfg = AuthServiceConfig {
        session_ttl_secs: 30 * 24 * 3600,
        admin_email: Some("admin@x.com".into()),
        login_attempts: 5.0,
        login_period_secs: 3600.0,
    };
    (Arc::new(AuthService::new(store, cfg)), dir)
}

#[test]
fn register_then_login_then_me() {
    let (svc, _dir) = fresh_service();
    let reg = svc.register(RegisterInput {
        email: "Alice@Example.Com",
        password: "supersecret",
        now: 1_000,
        ip: None,
        user_agent: None,
    }).unwrap();
    assert_eq!(reg.user.email, "alice@example.com"); // lowercased

    let token = reg.session_token.clone();
    let me = svc.session_user(&token, 2_000).unwrap().unwrap();
    assert_eq!(me.id, reg.user.id);

    // logout
    svc.logout(&token).unwrap();
    assert!(svc.session_user(&token, 3_000).unwrap().is_none());

    // re-login
    let lo = svc.login(LoginInput {
        email: "alice@example.com",
        password: "supersecret",
        now: 4_000,
        ip: None,
        user_agent: None,
    }).unwrap();
    assert_eq!(lo.user.id, reg.user.id);
}

#[test]
fn admin_email_marks_user_admin() {
    let (svc, _dir) = fresh_service();
    let reg = svc.register(RegisterInput {
        email: "admin@x.com",
        password: "p123abcd",
        now: 1,
        ip: None,
        user_agent: None,
    }).unwrap();
    assert!(reg.user.is_admin);
}

#[test]
fn duplicate_email_is_rejected() {
    let (svc, _dir) = fresh_service();
    svc.register(RegisterInput {
        email: "x@x.com", password: "p1234567", now: 1, ip: None, user_agent: None,
    }).unwrap();
    let err = svc.register(RegisterInput {
        email: "x@x.com", password: "p1234567", now: 2, ip: None, user_agent: None,
    }).unwrap_err();
    assert_eq!(err.code(), "email_already_exists");
}

#[test]
fn login_with_wrong_password_returns_invalid_credentials() {
    let (svc, _dir) = fresh_service();
    svc.register(RegisterInput {
        email: "y@x.com", password: "p1234567", now: 1, ip: None, user_agent: None,
    }).unwrap();
    let err = svc.login(LoginInput {
        email: "y@x.com", password: "wrong000", now: 2, ip: None, user_agent: None,
    }).unwrap_err();
    assert_eq!(err.code(), "invalid_credentials");
}

#[test]
fn login_unknown_email_also_invalid_credentials() {
    let (svc, _dir) = fresh_service();
    let err = svc.login(LoginInput {
        email: "nobody@x.com", password: "p1234567", now: 1, ip: None, user_agent: None,
    }).unwrap_err();
    // do NOT leak "no such user" — same error as wrong password
    assert_eq!(err.code(), "invalid_credentials");
}

#[test]
fn login_rate_limit_kicks_in() {
    let (svc, _dir) = fresh_service();
    svc.register(RegisterInput {
        email: "z@x.com", password: "p1234567", now: 1, ip: Some("1.2.3.4"), user_agent: None,
    }).unwrap();
    // config gives 5 attempts/hour. The 6th wrong-password attempt should hit
    // the rate limiter, not the credential check.
    for _ in 0..5 {
        let _ = svc.login(LoginInput {
            email: "z@x.com", password: "wrong000", now: 2, ip: Some("1.2.3.4"), user_agent: None,
        });
    }
    let err = svc.login(LoginInput {
        email: "z@x.com", password: "wrong000", now: 2, ip: Some("1.2.3.4"), user_agent: None,
    }).unwrap_err();
    assert_eq!(err.code(), "rate_limited");
}

#[test]
fn invalid_input_email_or_short_password() {
    let (svc, _dir) = fresh_service();
    let err = svc.register(RegisterInput {
        email: "not-an-email", password: "p1234567", now: 1, ip: None, user_agent: None,
    }).unwrap_err();
    assert_eq!(err.code(), "invalid_input");

    let err = svc.register(RegisterInput {
        email: "ok@e.com", password: "short", now: 1, ip: None, user_agent: None,
    }).unwrap_err();
    assert_eq!(err.code(), "invalid_input");
}

// --- Password reset tests (Task 2.5) ---

#[test]
fn forgot_password_returns_token_for_known_email() {
    let (svc, _dir) = fresh_service();
    svc.register(RegisterInput {
        email: "f@x.com", password: "p1234567", now: 1, ip: None, user_agent: None,
    }).unwrap();
    let res = svc.start_password_reset("f@x.com", 100).unwrap();
    assert!(res.is_some(), "should produce a token for an existing user");
}

#[test]
fn forgot_password_unknown_email_returns_none_silently() {
    let (svc, _dir) = fresh_service();
    let res = svc.start_password_reset("nobody@x.com", 100).unwrap();
    // Service signals "no token" but the HTTP layer must still return 200
    // to avoid email enumeration. The service doesn't fail.
    assert!(res.is_none());
}

#[test]
fn reset_password_works_then_old_sessions_die() {
    let (svc, _dir) = fresh_service();
    let reg = svc.register(RegisterInput {
        email: "r@x.com", password: "oldpassword", now: 1, ip: None, user_agent: None,
    }).unwrap();
    let token = svc.start_password_reset("r@x.com", 10).unwrap().unwrap();

    svc.complete_password_reset(&token, "newpassword", 20).unwrap();

    // Old session is dead.
    assert!(svc.session_user(&reg.session_token, 30).unwrap().is_none());

    // New password works, old does not.
    assert!(svc.login(LoginInput {
        email: "r@x.com", password: "newpassword", now: 40, ip: None, user_agent: None,
    }).is_ok());
    assert_eq!(
        svc.login(LoginInput {
            email: "r@x.com", password: "oldpassword", now: 41, ip: None, user_agent: None,
        }).unwrap_err().code(),
        "invalid_credentials"
    );
}

#[test]
fn reset_token_is_single_use() {
    let (svc, _dir) = fresh_service();
    svc.register(RegisterInput {
        email: "s@x.com", password: "p1234567", now: 1, ip: None, user_agent: None,
    }).unwrap();
    let token = svc.start_password_reset("s@x.com", 10).unwrap().unwrap();
    svc.complete_password_reset(&token, "newpassword", 20).unwrap();
    let err = svc.complete_password_reset(&token, "newer000", 30).unwrap_err();
    assert_eq!(err.code(), "invalid_reset_token");
}

#[test]
fn reset_token_expires() {
    let (svc, _dir) = fresh_service();
    svc.register(RegisterInput {
        email: "t@x.com", password: "p1234567", now: 1, ip: None, user_agent: None,
    }).unwrap();
    let token = svc.start_password_reset("t@x.com", 10).unwrap().unwrap();
    // 1 hour + 1 second later
    let err = svc.complete_password_reset(&token, "newpassword", 10 + 3601).unwrap_err();
    assert_eq!(err.code(), "expired_reset_token");
}
