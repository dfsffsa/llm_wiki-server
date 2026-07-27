//! SQLite schema for the auth/history/usage tables.
//!
//! All `CREATE TABLE IF NOT EXISTS` so `init_schema()` is safe to call on
//! every startup. Pragmas (WAL, busy_timeout) are applied alongside.

use rusqlite::Connection;

pub const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS users (
  id            INTEGER PRIMARY KEY,
  email         TEXT UNIQUE NOT NULL,
  password_hash TEXT NOT NULL,
  display_name  TEXT,
  is_admin      INTEGER NOT NULL DEFAULT 0,
  created_at    INTEGER NOT NULL,
  last_seen_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
  token_hash    TEXT PRIMARY KEY,
  user_id       INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  created_at    INTEGER NOT NULL,
  expires_at    INTEGER NOT NULL,
  user_agent    TEXT,
  ip            TEXT
);
CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);

CREATE TABLE IF NOT EXISTS password_reset_tokens (
  token_hash    TEXT PRIMARY KEY,
  user_id       INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  expires_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS conversations (
  id            TEXT PRIMARY KEY,
  user_id       INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  project_id    TEXT NOT NULL,
  title         TEXT NOT NULL,
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_conv_user ON conversations(user_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS conversation_messages (
  id              INTEGER PRIMARY KEY,
  conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  role            TEXT NOT NULL,
  content         TEXT NOT NULL,
  created_at      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_msg_conv ON conversation_messages(conversation_id, id);

CREATE TABLE IF NOT EXISTS usage_daily (
  user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  date       TEXT NOT NULL,
  chat_count INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (user_id, date)
);

CREATE TABLE IF NOT EXISTS email_verification_tokens (
  token_hash    TEXT PRIMARY KEY,
  user_id       INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  expires_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS pending_email_changes (
  id                    INTEGER PRIMARY KEY,
  user_id               INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  new_email             TEXT NOT NULL,
  old_token_hash        TEXT NOT NULL UNIQUE,
  old_expires_at        INTEGER NOT NULL,
  old_confirmed         INTEGER NOT NULL DEFAULT 0,
  new_token_hash        TEXT NOT NULL UNIQUE,
  new_expires_at        INTEGER NOT NULL,
  new_confirmed         INTEGER NOT NULL DEFAULT 0,
  created_at            INTEGER NOT NULL
);
"#;

/// Apply pragmas + create all tables. Safe to call repeatedly.
pub fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;",
    )?;
    conn.execute_batch(SCHEMA_SQL)?;

    // Idempotent migration: add email_verified_at column (SQLite doesn't support
    // IF NOT EXISTS for ALTER TABLE, so we catch the "duplicate column" error).
    match conn.execute_batch("ALTER TABLE users ADD COLUMN email_verified_at INTEGER") {
        Ok(_) => {}
        Err(e) if e.to_string().contains("duplicate column name") => {}
        Err(e) => return Err(e),
    }
    // Populate email_verified_at for existing users who registered before this
    // migration was deployed (treat their original registration as verified).
    conn.execute_batch(
        "UPDATE users SET email_verified_at = created_at
         WHERE email_verified_at IS NULL AND created_at > 0",
    )?;

    Ok(())
}
