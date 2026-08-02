//! SQLite-backed storage for users, sessions, reset tokens, conversations,
//! usage. Wraps a `rusqlite::Connection` behind a `Mutex` (single-writer is
//! fine — we already serialize all auth requests on a small thread pool, and
//! WAL allows concurrent reads).
//!
//! All methods take primitive `&str` / `i64` arguments and small structs;
//! the HTTP/service layer is responsible for the higher-level shape.

use crate::AuthError;
use crate::schema::init_schema;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

pub struct Store {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone)]
pub struct User {
    pub id: i64,
    pub email: String,
    pub password_hash: String,
    pub display_name: Option<String>,
    pub is_admin: bool,
    pub created_at: i64,
    pub last_seen_at: i64,
    pub email_verified_at: Option<i64>,
    pub plan: String,
    pub plan_period_end: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct NewUser<'a> {
    pub email: &'a str,
    pub password_hash: &'a str,
    pub display_name: Option<&'a str>,
    pub is_admin: bool,
    pub now: i64,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, AuthError> {
        let conn = Connection::open(path).map_err(AuthError::from)?;
        init_schema(&conn).map_err(AuthError::from)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Liveness probe for the SQLite connection: `SELECT 1`. Used by the deep
    /// `/health?deep=true` check. Returns the underlying rusqlite error on
    /// failure (e.g. DB file gone, disk full).
    pub fn ping(&self) -> Result<(), rusqlite::Error> {
        let conn = self.lock();
        conn.query_row("SELECT 1", [], |row| row.get::<_, i64>(0))?;
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("auth store mutex poisoned")
    }

    // --- users ---

    pub fn create_user(&self, n: NewUser<'_>) -> Result<i64, AuthError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO users (email, password_hash, display_name, is_admin, created_at, last_seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![
                n.email,
                n.password_hash,
                n.display_name,
                if n.is_admin { 1 } else { 0 },
                n.now
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn find_user_by_email(&self, email: &str) -> Result<Option<User>, AuthError> {
        let conn = self.lock();
        conn.query_row(
            "SELECT id, email, password_hash, display_name, is_admin, created_at, last_seen_at,
                    email_verified_at, plan, plan_period_end
             FROM users WHERE email = ?1",
            params![email],
            row_to_user,
        )
        .optional()
        .map_err(AuthError::from)
    }

    pub fn find_user_by_id(&self, id: i64) -> Result<Option<User>, AuthError> {
        let conn = self.lock();
        conn.query_row(
            "SELECT id, email, password_hash, display_name, is_admin, created_at, last_seen_at,
                    email_verified_at, plan, plan_period_end
             FROM users WHERE id = ?1",
            params![id],
            row_to_user,
        )
        .optional()
        .map_err(AuthError::from)
    }

    pub fn get_plan(&self, user_id: i64) -> Result<String, AuthError> {
        let conn = self.lock();
        Ok(conn.query_row(
            "SELECT COALESCE(plan, 'free') FROM users WHERE id = ?1",
            params![user_id],
            |row| row.get(0),
        )?)
    }

    pub fn get_plan_info(&self, user_id: i64) -> Result<(String, Option<i64>), AuthError> {
        let conn = self.lock();
        Ok(conn.query_row(
            "SELECT COALESCE(plan, 'free'), plan_period_end FROM users WHERE id = ?1",
            params![user_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    }

    pub fn set_plan(
        &self,
        user_id: i64,
        plan: &str,
        order_id: Option<&str>,
        period_end: Option<i64>,
        now: i64,
    ) -> Result<(), AuthError> {
        let conn = self.lock();
        conn.execute(
            "UPDATE users SET
               plan = ?1,
               waffo_order_id = ?2,
               plan_period_end = ?3,
               pro_since = CASE WHEN ?1 = 'pro' AND pro_since IS NULL THEN ?4 ELSE pro_since END
             WHERE id = ?5",
            params![plan, order_id, period_end, now, user_id],
        )?;
        Ok(())
    }

    pub fn find_user_by_order_id(&self, order_id: &str) -> Result<Option<User>, AuthError> {
        let conn = self.lock();
        conn.query_row(
            "SELECT id, email, password_hash, display_name, is_admin, created_at, last_seen_at,
                    email_verified_at, plan, plan_period_end
             FROM users WHERE waffo_order_id = ?1",
            params![order_id],
            row_to_user,
        )
        .optional()
        .map_err(AuthError::from)
    }

    pub fn has_webhook_event(&self, event_id: &str) -> Result<bool, AuthError> {
        let conn = self.lock();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM waffo_webhook_events WHERE event_id = ?1",
            params![event_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn record_webhook_event(
        &self,
        event_id: &str,
        event_type: &str,
        now: i64,
    ) -> Result<(), AuthError> {
        let conn = self.lock();
        conn.execute(
            "INSERT OR IGNORE INTO waffo_webhook_events (event_id, event_type, created_at)
             VALUES (?1, ?2, ?3)",
            params![event_id, event_type, now],
        )?;
        Ok(())
    }

    pub fn touch_user_seen(&self, id: i64, now: i64) -> Result<(), AuthError> {
        let conn = self.lock();
        conn.execute("UPDATE users SET last_seen_at = ?1 WHERE id = ?2", params![now, id])?;
        Ok(())
    }

    pub fn update_password(&self, id: i64, password_hash: &str) -> Result<(), AuthError> {
        let conn = self.lock();
        conn.execute(
            "UPDATE users SET password_hash = ?1 WHERE id = ?2",
            params![password_hash, id],
        )?;
        Ok(())
    }

    // --- sessions ---

    pub fn create_session(
        &self,
        token_hash: &str,
        user_id: i64,
        now: i64,
        expires_at: i64,
        user_agent: Option<&str>,
        ip: Option<&str>,
    ) -> Result<(), AuthError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO sessions (token_hash, user_id, created_at, expires_at, user_agent, ip)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![token_hash, user_id, now, expires_at, user_agent, ip],
        )?;
        Ok(())
    }

    /// Returns the user_id if the session exists and is not yet expired.
    pub fn find_session_user(&self, token_hash: &str, now: i64) -> Result<Option<i64>, AuthError> {
        let conn = self.lock();
        conn.query_row(
            "SELECT user_id FROM sessions
             WHERE token_hash = ?1 AND expires_at > ?2",
            params![token_hash, now],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(AuthError::from)
    }

    pub fn delete_session(&self, token_hash: &str) -> Result<(), AuthError> {
        let conn = self.lock();
        conn.execute("DELETE FROM sessions WHERE token_hash = ?1", params![token_hash])?;
        Ok(())
    }

    pub fn delete_user_sessions(&self, user_id: i64) -> Result<(), AuthError> {
        let conn = self.lock();
        conn.execute("DELETE FROM sessions WHERE user_id = ?1", params![user_id])?;
        Ok(())
    }

    // --- reset tokens ---

    pub fn create_reset_token(
        &self,
        token_hash: &str,
        user_id: i64,
        expires_at: i64,
    ) -> Result<(), AuthError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO password_reset_tokens (token_hash, user_id, expires_at)
             VALUES (?1, ?2, ?3)",
            params![token_hash, user_id, expires_at],
        )?;
        Ok(())
    }

    /// Look up the user_id for a reset token, if it exists and is not expired.
    pub fn find_reset_token_user(
        &self,
        token_hash: &str,
        _now: i64,
    ) -> Result<Option<(i64, i64)>, AuthError> {
        // returns (user_id, expires_at) so caller can distinguish "expired"
        // from "missing" if it wants different errors.
        let conn = self.lock();
        conn.query_row(
            "SELECT user_id, expires_at FROM password_reset_tokens WHERE token_hash = ?1",
            params![token_hash],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(AuthError::from)
    }

    pub fn delete_reset_token(&self, token_hash: &str) -> Result<(), AuthError> {
        let conn = self.lock();
        conn.execute(
            "DELETE FROM password_reset_tokens WHERE token_hash = ?1",
            params![token_hash],
        )?;
        Ok(())
    }

    // --- email verification ---

    pub fn set_email_verified(&self, user_id: i64, now: i64) -> Result<(), AuthError> {
        let conn = self.lock();
        conn.execute(
            "UPDATE users SET email_verified_at = ?1 WHERE id = ?2",
            params![now, user_id],
        )?;
        Ok(())
    }

    pub fn create_verification_token(
        &self,
        token_hash: &str,
        user_id: i64,
        expires_at: i64,
    ) -> Result<(), AuthError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO email_verification_tokens (token_hash, user_id, expires_at)
             VALUES (?1, ?2, ?3)",
            params![token_hash, user_id, expires_at],
        )?;
        Ok(())
    }

    pub fn find_verification_token_user(
        &self,
        token_hash: &str,
    ) -> Result<Option<(i64, i64)>, AuthError> {
        let conn = self.lock();
        conn.query_row(
            "SELECT user_id, expires_at FROM email_verification_tokens
             WHERE token_hash = ?1",
            params![token_hash],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(AuthError::from)
    }

    pub fn delete_verification_token(&self, token_hash: &str) -> Result<(), AuthError> {
        let conn = self.lock();
        conn.execute(
            "DELETE FROM email_verification_tokens WHERE token_hash = ?1",
            params![token_hash],
        )?;
        Ok(())
    }

    // --- email change ---

    pub fn create_pending_change(
        &self,
        user_id: i64,
        new_email: &str,
        old_token_hash: &str,
        old_expires_at: i64,
        new_token_hash: &str,
        new_expires_at: i64,
        now: i64,
    ) -> Result<i64, AuthError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO pending_email_changes
             (user_id, new_email, old_token_hash, old_expires_at,
              new_token_hash, new_expires_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                user_id,
                new_email,
                old_token_hash,
                old_expires_at,
                new_token_hash,
                new_expires_at,
                now
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn find_pending_change_by_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<PendingChangeRow>, AuthError> {
        let conn = self.lock();
        conn.query_row(
            "SELECT id, user_id, new_email, old_token_hash, old_expires_at,
                    old_confirmed, new_token_hash, new_expires_at, new_confirmed,
                    created_at
             FROM pending_email_changes
             WHERE old_token_hash = ?1 OR new_token_hash = ?1",
            params![token_hash],
            |row| {
                Ok(PendingChangeRow {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    new_email: row.get(2)?,
                    old_token_hash: row.get(3)?,
                    old_expires_at: row.get(4)?,
                    old_confirmed: row.get::<_, i64>(5)? != 0,
                    new_token_hash: row.get(6)?,
                    new_expires_at: row.get(7)?,
                    new_confirmed: row.get::<_, i64>(8)? != 0,
                    created_at: row.get(9)?,
                })
            },
        )
        .optional()
        .map_err(AuthError::from)
    }

    pub fn mark_old_email_confirmed(&self, change_id: i64) -> Result<(), AuthError> {
        let conn = self.lock();
        conn.execute(
            "UPDATE pending_email_changes SET old_confirmed = 1 WHERE id = ?1",
            params![change_id],
        )?;
        Ok(())
    }

    pub fn mark_new_email_confirmed(&self, change_id: i64) -> Result<(), AuthError> {
        let conn = self.lock();
        conn.execute(
            "UPDATE pending_email_changes SET new_confirmed = 1 WHERE id = ?1",
            params![change_id],
        )?;
        Ok(())
    }

    pub fn update_user_email(
        &self,
        user_id: i64,
        new_email: &str,
        now: i64,
    ) -> Result<(), AuthError> {
        let conn = self.lock();
        conn.execute(
            "UPDATE users SET email = ?1, email_verified_at = ?2 WHERE id = ?3",
            params![new_email, now, user_id],
        )?;
        Ok(())
    }

    pub fn delete_pending_change(&self, change_id: i64) -> Result<(), AuthError> {
        let conn = self.lock();
        conn.execute(
            "DELETE FROM pending_email_changes WHERE id = ?1",
            params![change_id],
        )?;
        Ok(())
    }

    pub fn verify_email_exists(&self, email: &str) -> Result<bool, AuthError> {
        let conn = self.lock();
        let exists: Option<i64> = conn
            .query_row("SELECT 1 FROM users WHERE email = ?1", params![email], |row| {
                row.get(0)
            })
            .optional()?;
        Ok(exists.is_some())
    }

    // --- conversations ---

    pub fn create_conversation(
        &self,
        id: &str,
        user_id: i64,
        project_id: &str,
        title: &str,
        now: i64,
    ) -> Result<(), AuthError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO conversations (id, user_id, project_id, title, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![id, user_id, project_id, title, now],
        )?;
        Ok(())
    }

    pub fn list_conversations(
        &self,
        user_id: i64,
        limit: u32,
    ) -> Result<Vec<ConversationRow>, AuthError> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, project_id, title, created_at, updated_at
             FROM conversations
             WHERE user_id = ?1
             ORDER BY updated_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![user_id, limit as i64], |row| {
            Ok(ConversationRow {
                id: row.get(0)?,
                project_id: row.get(1)?,
                title: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn find_conversation_owner(&self, id: &str) -> Result<Option<i64>, AuthError> {
        let conn = self.lock();
        conn.query_row(
            "SELECT user_id FROM conversations WHERE id = ?1",
            params![id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(AuthError::from)
    }

    pub fn delete_conversation(&self, id: &str) -> Result<(), AuthError> {
        let conn = self.lock();
        conn.execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn touch_conversation(&self, id: &str, now: i64) -> Result<(), AuthError> {
        let conn = self.lock();
        conn.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        Ok(())
    }

    // --- messages ---

    pub fn append_message(
        &self,
        conv_id: &str,
        role: &str,
        content: &str,
        now: i64,
    ) -> Result<(), AuthError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO conversation_messages (conversation_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![conv_id, role, content, now],
        )?;
        conn.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
            params![now, conv_id],
        )?;
        Ok(())
    }

    pub fn list_messages(&self, conv_id: &str) -> Result<Vec<MessageRow>, AuthError> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT role, content, created_at
             FROM conversation_messages
             WHERE conversation_id = ?1
             ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![conv_id], |row| {
            Ok(MessageRow {
                role: row.get(0)?,
                content: row.get(1)?,
                created_at: row.get(2)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    // --- usage ---

    pub fn get_usage(&self, user_id: i64, date: &str) -> Result<i64, AuthError> {
        let conn = self.lock();
        let count: Option<i64> = conn
            .query_row(
                "SELECT chat_count FROM usage_daily WHERE user_id = ?1 AND date = ?2",
                params![user_id, date],
                |row| row.get(0),
            )
            .optional()?;
        Ok(count.unwrap_or(0))
    }

    pub fn increment_usage(&self, user_id: i64, date: &str) -> Result<(), AuthError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO usage_daily (user_id, date, chat_count) VALUES (?1, ?2, 1)
             ON CONFLICT(user_id, date) DO UPDATE SET chat_count = chat_count + 1",
            params![user_id, date],
        )?;
        Ok(())
    }

    /// Atomically increment the daily chat counter **only if** it is below
    /// `limit`. Returns `Ok(true)` if the increment happened (quota remained),
    /// `Ok(false)` if the user is already at/over the limit (no row changed).
    ///
    /// Closes the TOCTOU window that `get_usage` + `increment_usage` had:
    /// those were two separate mutex acquisitions, so concurrent chats could
    /// both pass the check and both increment. This is one statement under
    /// the connection mutex, so the check-and-increment is atomic.
    ///
    /// `limit <= 0` denies immediately and writes no row.
    pub fn try_increment_usage(
        &self,
        user_id: i64,
        date: &str,
        limit: i64,
    ) -> Result<bool, AuthError> {
        if limit <= 0 {
            return Ok(false);
        }
        let conn = self.lock();
        conn.execute(
            "INSERT INTO usage_daily (user_id, date, chat_count) VALUES (?1, ?2, 1)
             ON CONFLICT(user_id, date) DO UPDATE SET chat_count = chat_count + 1
             WHERE chat_count < ?3",
            params![user_id, date, limit],
        )?;
        // changes(): 1 if the INSERT ran or the UPDATE's WHERE matched (quota
        // remained); 0 if the conflict path's WHERE was false (at/over limit).
        Ok(conn.changes() > 0)
    }
}

fn row_to_user(row: &rusqlite::Row<'_>) -> rusqlite::Result<User> {
    Ok(User {
        id: row.get(0)?,
        email: row.get(1)?,
        password_hash: row.get(2)?,
        display_name: row.get(3)?,
        is_admin: row.get::<_, i64>(4)? != 0,
        created_at: row.get(5)?,
        last_seen_at: row.get(6)?,
        email_verified_at: row.get::<_, Option<i64>>(7)?,
        plan: row.get::<_, String>(8)?,
        plan_period_end: row.get::<_, Option<i64>>(9)?,
    })
}

#[derive(Debug, Clone)]
pub struct PendingChangeRow {
    pub id: i64,
    pub user_id: i64,
    pub new_email: String,
    pub old_token_hash: String,
    pub old_expires_at: i64,
    pub old_confirmed: bool,
    pub new_token_hash: String,
    pub new_expires_at: i64,
    pub new_confirmed: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct ConversationRow {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct MessageRow {
    pub role: String,
    pub content: String,
    pub created_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn open_test_store() -> Store {
        let f = NamedTempFile::new().unwrap();
        Store::open(f.path()).unwrap()
    }

    fn make_user(store: &Store, email: &str) -> i64 {
        store
            .create_user(NewUser {
                email,
                password_hash: "x",
                display_name: None,
                is_admin: false,
                now: 1000,
            })
            .unwrap()
    }

    #[test]
    fn try_increment_allows_up_to_limit_then_denies() {
        let store = open_test_store();
        let uid = make_user(&store, "a@b.com");
        let date = "2026-06-26";
        // limit = 2: two increments succeed, third+ denied, count stays 2.
        assert_eq!(store.try_increment_usage(uid, date, 2).unwrap(), true);
        assert_eq!(store.try_increment_usage(uid, date, 2).unwrap(), true);
        assert_eq!(store.try_increment_usage(uid, date, 2).unwrap(), false);
        assert_eq!(store.try_increment_usage(uid, date, 2).unwrap(), false);
        assert_eq!(store.get_usage(uid, date).unwrap(), 2);
    }

    #[test]
    fn try_increment_limit_zero_denies_immediately() {
        let store = open_test_store();
        let uid = make_user(&store, "a@b.com");
        assert_eq!(store.try_increment_usage(uid, "2026-06-26", 0).unwrap(), false);
        // no row should have been written.
        assert_eq!(store.get_usage(uid, "2026-06-26").unwrap(), 0);
    }

    #[test]
    fn try_increment_is_independent_per_date() {
        let store = open_test_store();
        let uid = make_user(&store, "a@b.com");
        assert_eq!(store.try_increment_usage(uid, "2026-06-26", 1).unwrap(), true);
        assert_eq!(store.try_increment_usage(uid, "2026-06-27", 1).unwrap(), true);
        // 06-26 now at limit 1 → denied; 06-27 also at limit 1 → denied.
        assert_eq!(store.try_increment_usage(uid, "2026-06-26", 1).unwrap(), false);
        assert_eq!(store.try_increment_usage(uid, "2026-06-27", 1).unwrap(), false);
    }

    #[test]
    fn try_increment_does_not_overshoot_under_repeated_denials() {
        // Repeated denials at the limit must never bump the count.
        let store = open_test_store();
        let uid = make_user(&store, "a@b.com");
        let date = "2026-06-26";
        for _ in 0..5 {
            store.try_increment_usage(uid, date, 1).unwrap();
        }
        assert_eq!(store.get_usage(uid, date).unwrap(), 1);
    }

    #[test]
    fn set_email_verified_updates_field() {
        let store = open_test_store();
        let uid = make_user(&store, "verify@test.com");
        assert_eq!(
            store.find_user_by_id(uid).unwrap().unwrap().email_verified_at,
            None
        );
        store.set_email_verified(uid, 2000).unwrap();
        assert_eq!(
            store.find_user_by_id(uid).unwrap().unwrap().email_verified_at,
            Some(2000)
        );
    }

    #[test]
    fn create_and_find_verification_token() {
        let store = open_test_store();
        let uid = make_user(&store, "token@test.com");
        let hash = "testhash123";
        store.create_verification_token(hash, uid, 2000).unwrap();
        let found = store.find_verification_token_user(hash).unwrap().unwrap();
        assert_eq!(found.0, uid);
        assert_eq!(found.1, 2000);
    }

    #[test]
    fn delete_verification_token_removes_it() {
        let store = open_test_store();
        let uid = make_user(&store, "del@test.com");
        store.create_verification_token("h1", uid, 2000).unwrap();
        assert!(store.find_verification_token_user("h1").unwrap().is_some());
        store.delete_verification_token("h1").unwrap();
        assert!(store.find_verification_token_user("h1").unwrap().is_none());
    }

    #[test]
    fn create_and_complete_pending_change() {
        let store = open_test_store();
        let uid = make_user(&store, "old@test.com");
        let cid = store
            .create_pending_change(uid, "new@test.com", "oldh", 2000, "newh", 2000, 1000)
            .unwrap();
        let row = store
            .find_pending_change_by_hash("oldh")
            .unwrap()
            .unwrap();
        assert_eq!(row.new_email, "new@test.com");
        assert!(!row.old_confirmed);
        assert!(!row.new_confirmed);
        store.mark_old_email_confirmed(cid).unwrap();
        store.mark_new_email_confirmed(cid).unwrap();
        let row2 = store
            .find_pending_change_by_hash("oldh")
            .unwrap()
            .unwrap();
        assert!(row2.old_confirmed);
        assert!(row2.new_confirmed);
        // complete the change
        store.update_user_email(uid, "new@test.com", 1500).unwrap();
        store.delete_pending_change(cid).unwrap();
        let user = store.find_user_by_id(uid).unwrap().unwrap();
        assert_eq!(user.email, "new@test.com");
        assert_eq!(user.email_verified_at, Some(1500));
        assert!(store
            .find_pending_change_by_hash("oldh")
            .unwrap()
            .is_none());
    }

    #[test]
    fn create_and_verify_email_exists() {
        let store = open_test_store();
        make_user(&store, "exists@test.com");
        assert!(store.verify_email_exists("exists@test.com").unwrap());
        assert!(!store.verify_email_exists("nope@test.com").unwrap());
    }

    #[test]
    fn plan_defaults_to_free() {
        let store = open_test_store();
        let uid = store
            .create_user(NewUser {
                email: "p@example.com",
                password_hash: "h",
                display_name: None,
                is_admin: false,
                now: 1000,
            })
            .unwrap();
        assert_eq!(store.get_plan(uid).unwrap(), "free");
        let (plan, period_end) = store.get_plan_info(uid).unwrap();
        assert_eq!(plan, "free");
        assert_eq!(period_end, None);
    }

    #[test]
    fn set_plan_roundtrip() {
        let store = open_test_store();
        let uid = store
            .create_user(NewUser {
                email: "p2@example.com",
                password_hash: "h",
                display_name: None,
                is_admin: false,
                now: 1000,
            })
            .unwrap();
        store
            .set_plan(uid, "pro", Some("ORD_abc"), Some(1_752_000_000), 2000)
            .unwrap();
        let (plan, period_end) = store.get_plan_info(uid).unwrap();
        assert_eq!(plan, "pro");
        assert_eq!(period_end, Some(1_752_000_000));
        // downgrade clears order + period
        store.set_plan(uid, "free", None, None, 3000).unwrap();
        let (plan, period_end) = store.get_plan_info(uid).unwrap();
        assert_eq!(plan, "free");
        assert_eq!(period_end, None);
    }

    #[test]
    fn find_user_by_order_id() {
        let store = open_test_store();
        let uid = store
            .create_user(NewUser {
                email: "p3@example.com",
                password_hash: "h",
                display_name: None,
                is_admin: false,
                now: 1000,
            })
            .unwrap();
        store
            .set_plan(uid, "pro", Some("ORD_xyz"), None, 2000)
            .unwrap();
        let u = store.find_user_by_order_id("ORD_xyz").unwrap().expect("found");
        assert_eq!(u.id, uid);
        assert_eq!(u.plan, "pro");
        assert!(store.find_user_by_order_id("ORD_nope").unwrap().is_none());
    }

    #[test]
    fn webhook_event_dedup() {
        let store = open_test_store();
        assert!(!store.has_webhook_event("evt_1").unwrap());
        store
            .record_webhook_event("evt_1", "subscription.activated", 1000)
            .unwrap();
        assert!(store.has_webhook_event("evt_1").unwrap());
        store
            .record_webhook_event("evt_1", "subscription.activated", 1000)
            .unwrap(); // idempotent
        assert!(store.has_webhook_event("evt_1").unwrap());
    }
}
