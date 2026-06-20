use llm_wiki_auth::password::{hash_password, verify_password};
use llm_wiki_auth::schema::init_schema;
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
