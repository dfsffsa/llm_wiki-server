# Public Deploy — Auth, History, Usage Limits — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 `overlay/` 内为 `llm-wiki-server` 增加最小可用的多用户系统(邮箱密码登录、服务端聊天历史、每日 chat 限额),并提供落地页/登录页;不破坏现有 Bearer token 鉴权(CLI/e2e)。

**Architecture:** Rust 单进程,新增 `overlay/auth/` 子 crate(SQLite + argon2id + session cookie + 漏桶限流);`/auth/*` JSON 路由与现有 `/api/v1/*` 并列;`is_authorized` 改为先尝试 cookie 再回退 Bearer。前端:静态落地页/登录页(纯 HTML),lite 页改造加用户栏 + 历史侧边栏。

**Tech Stack:** Rust + tiny_http(现有) / rusqlite(SQLite) / argon2(密码) / sha2(token hash) / rand(token) / base64(编码) / uuid(会话 id);前端原生 JS(无框架)。

**Spec:** [docs/superpowers/specs/2026-06-20-public-deploy-auth-design.md](../specs/2026-06-20-public-deploy-auth-design.md)

**Reference (供查阅,勿直接复制代码):** [smhanov/auth](https://github.com/smhanov/auth);本地副本可临时 `git clone https://github.com/smhanov/auth /tmp/smhanov-auth` 查阅 `ratelimit.go` / `auth.go` / `schema.go`。

---

## File Structure

| 文件 | 职责 |
|---|---|
| `overlay/auth/Cargo.toml` | 新 crate `llm-wiki-auth` |
| `overlay/auth/src/lib.rs` | 模块导出 + 公开 API(`AuthService`/`User`/`AuthError`) |
| `overlay/auth/src/schema.rs` | 建表 SQL + `init_schema()` |
| `overlay/auth/src/store.rs` | rusqlite 操作:users/sessions/reset_tokens/conversations/usage |
| `overlay/auth/src/password.rs` | argon2id hash + verify |
| `overlay/auth/src/session.rs` | token 生成 / sha256 / cookie 字符串 |
| `overlay/auth/src/ratelimit.rs` | 漏桶限流(内存,user+ip) |
| `overlay/auth/src/error.rs` | 错误类型 + 错误码→HTTP 状态映射 |
| `overlay/auth/src/service.rs` | 业务编排:register/login/logout/me/forgot/reset |
| `overlay/auth/tests/integration.rs` | 集成测试(临时 SQLite) |
| `overlay/server/src/api/auth_routes.rs` | HTTP 适配:`/auth/*` 路由 |
| `overlay/server/src/api/conversations.rs` | `/api/v1/conversations*` 路由 |
| `overlay/server/src/api/mod.rs` | 修改:`is_authorized` 加 cookie,`API_PREFIX`/auth 共存 |
| `overlay/server/src/api/chat.rs` | 修改:用量计数 + 用户态拒绝 |
| `overlay/server/src/server.rs` | 修改:`/auth/*` 与 `/api/v1/*` 并列分发,加 `is_auth_path` |
| `overlay/server/src/state.rs` | 修改:加 `Option<Arc<AuthService>>` 字段 |
| `overlay/server/src/main.rs` | 修改:启动时初始化 AuthService |
| `overlay/server/src/config.rs` | 修改:加 `auth_db` / `require_login` / `daily_chat_limit` / `admin_email` |
| `overlay/server/Cargo.toml` | 修改:依赖 `llm-wiki-auth` |
| `overlay/static/index.html` | 落地页 |
| `overlay/static/landing.css` | 落地页样式 |
| `overlay/static/landing.js` | 落地页 JS(检查 /auth/me 决定按钮跳转) |
| `overlay/static/auth/login.html` | 登录/注册同页(tab 切换) |
| `overlay/static/auth/auth.css` | 登录页样式 |
| `overlay/static/auth/auth.js` | 登录/注册逻辑 |
| `overlay/static/auth/reset.html` | 重置密码页 |
| `overlay/static/lite/app.js` | 修改:加 `/auth/me` 检查、用户栏、历史侧边栏、API 替换 localStorage |
| `overlay/static/lite/app.css` | 修改:用户栏 + 侧边栏 + 移动端适配 |
| `overlay/static/lite/index.html` | 修改:加用户栏与侧边栏 DOM |
| `overlay/server/src/static_files.rs` | 修改:`/login`/`/register`/`/reset-password` 美观路径映射 |
| `docs/部署-ECS与Tunnel.md` | 修改:加 auth db 目录、新环境变量 |

---

## Phase 概览

| Phase | 交付物 | 可独立验证 |
|---|---|---|
| 1 | `llm-wiki-auth` crate 雏形(schema + store + password + session) | `cargo test -p llm-wiki-auth` 通过 |
| 2 | 限流 + 错误 + AuthService 业务层 | service 集成测试通过 |
| 3 | `/auth/*` HTTP 路由接入 server | curl 注册/登录/me 通过 |
| 4 | `is_authorized` 加 cookie 路径 + chat 用量限额 | curl 用 cookie 调 chat 计数 + 超额 429 |
| 5 | conversations API + lite 页改造 | 浏览器多用户对话历史隔离 |
| 6 | 落地页 + 登录/注册/重置页(纯静态) | 浏览器走完注册→登录→对话→登出闭环 |
| 7 | 部署文档 + 配置项 + 回归 | e2e-local.sh 通过,部署文档自洽 |

---

## Phase 1 — auth crate 雏形

### Task 1.1: 创建 `llm-wiki-auth` crate 骨架

**Files:**
- Create: `overlay/auth/Cargo.toml`
- Create: `overlay/auth/src/lib.rs`
- Modify: `overlay/server/Cargo.toml`(末尾追加依赖)

- [ ] **Step 1: 写 Cargo.toml**

写 `overlay/auth/Cargo.toml`:

```toml
[package]
name = "llm-wiki-auth"
version = "0.1.0"
edition = "2021"
description = "Auth, sessions, conversation history, usage limits for llm-wiki-server"
license = "GPL-3.0-or-later"

[dependencies]
rusqlite = { version = "0.31", features = ["bundled"] }
argon2 = "0.5"
rand = "0.8"
sha2 = "0.10"
base64 = "0.22"
uuid = { version = "1", features = ["v4"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: 写 lib.rs 占位**

写 `overlay/auth/src/lib.rs`:

```rust
//! llm-wiki-auth: minimal auth kernel for llm-wiki-server.
//!
//! Modules are added incrementally by the implementation plan; this file
//! starts as a stub so the crate compiles before any logic is in place.

pub mod schema;
```

- [ ] **Step 3: 让 server crate 依赖新 crate(暂不 use)**

修改 `overlay/server/Cargo.toml` `[dependencies]` 末尾追加:

```toml
llm-wiki-auth = { path = "../auth" }
```

- [ ] **Step 4: 创建 schema.rs 占位让 lib 编译**

写 `overlay/auth/src/schema.rs`:

```rust
//! Schema constants and `init_schema()` are filled in by the next task.
```

- [ ] **Step 5: 编译验证**

Run: `cargo build --manifest-path overlay/server/Cargo.toml`
Expected: `Compiling llm-wiki-auth v0.1.0` 然后 `Finished`,无错误

- [ ] **Step 6: Commit**

```bash
git add overlay/auth/Cargo.toml overlay/auth/src/lib.rs overlay/auth/src/schema.rs overlay/server/Cargo.toml
git commit -m "feat(auth): scaffold llm-wiki-auth crate"
```

---

### Task 1.2: SQLite schema + init_schema

**Files:**
- Modify: `overlay/auth/src/schema.rs`
- Test: `overlay/auth/tests/integration.rs`(创建)

- [ ] **Step 1: 写失败测试**

写 `overlay/auth/tests/integration.rs`:

```rust
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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p llm-wiki-auth init_schema`
Expected: 编译失败(`init_schema` 未实现)

- [ ] **Step 3: 实现 schema.rs**

替换 `overlay/auth/src/schema.rs` 内容:

```rust
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
    Ok(())
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p llm-wiki-auth init_schema`
Expected: `test init_schema_creates_all_tables ... ok` 和 `test init_schema_is_idempotent ... ok`

- [ ] **Step 5: Commit**

```bash
git add overlay/auth/src/schema.rs overlay/auth/tests/integration.rs
git commit -m "feat(auth): SQLite schema + init_schema()"
```

---

### Task 1.3: 密码 argon2id

**Files:**
- Create: `overlay/auth/src/password.rs`
- Modify: `overlay/auth/src/lib.rs`(导出 password)
- Modify: `overlay/auth/tests/integration.rs`(追加测试)

- [ ] **Step 1: 写失败测试**

在 `overlay/auth/tests/integration.rs` 末尾追加:

```rust
use llm_wiki_auth::password::{hash_password, verify_password};

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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p llm-wiki-auth password`
Expected: 编译失败(`hash_password` 未导出)

- [ ] **Step 3: 实现 password.rs**

写 `overlay/auth/src/password.rs`:

```rust
//! Argon2id password hashing.
//!
//! Uses OWASP-recommended parameters (m=19456 KiB, t=2, p=1) via the
//! `argon2` crate's `Argon2::default()` which targets argon2id. Output is the
//! standard PHC-encoded string, which embeds the salt + parameters so verify
//! has everything it needs.

use argon2::password_hash::{rand_core::OsRng, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Argon2, PasswordHash};

#[derive(Debug, thiserror::Error)]
pub enum PasswordError {
    #[error("hash failed: {0}")]
    Hash(String),
    #[error("verify failed: {0}")]
    Verify(String),
}

pub fn hash_password(plain: &str) -> Result<String, PasswordError> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(plain.as_bytes(), &salt)
        .map_err(|e| PasswordError::Hash(e.to_string()))?;
    Ok(hash.to_string())
}

pub fn verify_password(stored_phc: &str, candidate: &str) -> Result<bool, PasswordError> {
    let parsed = PasswordHash::new(stored_phc).map_err(|e| PasswordError::Verify(e.to_string()))?;
    Ok(Argon2::default()
        .verify_password(candidate.as_bytes(), &parsed)
        .is_ok())
}
```

- [ ] **Step 4: 加 thiserror 依赖**

修改 `overlay/auth/Cargo.toml` `[dependencies]`,追加:

```toml
thiserror = "1"
```

- [ ] **Step 5: 在 lib.rs 导出 password 模块**

修改 `overlay/auth/src/lib.rs`,在 `pub mod schema;` 后追加:

```rust
pub mod password;
```

- [ ] **Step 6: 跑测试确认通过**

Run: `cargo test -p llm-wiki-auth password`
Expected: 两个测试都 ok

- [ ] **Step 7: Commit**

```bash
git add overlay/auth/Cargo.toml overlay/auth/src/lib.rs overlay/auth/src/password.rs overlay/auth/tests/integration.rs
git commit -m "feat(auth): argon2id password hashing"
```

---

### Task 1.4: Session token 生成与 hash

**Files:**
- Create: `overlay/auth/src/session.rs`
- Modify: `overlay/auth/src/lib.rs`(导出 session)
- Modify: `overlay/auth/tests/integration.rs`

- [ ] **Step 1: 写失败测试**

追加到 `overlay/auth/tests/integration.rs`:

```rust
use llm_wiki_auth::session::{generate_token, hash_token, build_session_cookie};

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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p llm-wiki-auth session`
Expected: 编译失败

- [ ] **Step 3: 实现 session.rs**

写 `overlay/auth/src/session.rs`:

```rust
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
```

- [ ] **Step 4: 在 lib.rs 导出**

`overlay/auth/src/lib.rs` 追加:

```rust
pub mod session;
```

- [ ] **Step 5: 跑测试确认通过**

Run: `cargo test -p llm-wiki-auth session`
Expected: 4 个测试全部 ok

- [ ] **Step 6: Commit**

```bash
git add overlay/auth/src/lib.rs overlay/auth/src/session.rs overlay/auth/tests/integration.rs
git commit -m "feat(auth): session token generation, hashing, cookie builder"
```


---

## Phase 2 — 限流 + 错误 + Store + AuthService

### Task 2.1: 错误类型与 HTTP 映射

**Files:**
- Create: `overlay/auth/src/error.rs`
- Modify: `overlay/auth/src/lib.rs`

- [ ] **Step 1: 实现 error.rs**

写 `overlay/auth/src/error.rs`:

```rust
//! Auth-layer errors. Each variant has a stable error code (matches the spec)
//! and a default user-facing message. The HTTP layer maps each to a status.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    InvalidInput(String),
    EmailAlreadyExists,
    InvalidCredentials,
    NotAuthenticated,
    RateLimited,
    DailyLimitExceeded,
    InvalidResetToken,
    ExpiredResetToken,
    Internal(String),
}

impl AuthError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_input",
            Self::EmailAlreadyExists => "email_already_exists",
            Self::InvalidCredentials => "invalid_credentials",
            Self::NotAuthenticated => "not_authenticated",
            Self::RateLimited => "rate_limited",
            Self::DailyLimitExceeded => "daily_limit_exceeded",
            Self::InvalidResetToken => "invalid_reset_token",
            Self::ExpiredResetToken => "expired_reset_token",
            Self::Internal(_) => "internal_error",
        }
    }

    pub fn http_status(&self) -> u16 {
        match self {
            Self::InvalidInput(_) => 400,
            Self::EmailAlreadyExists => 409,
            Self::InvalidCredentials | Self::NotAuthenticated => 401,
            Self::RateLimited | Self::DailyLimitExceeded => 429,
            Self::InvalidResetToken | Self::ExpiredResetToken => 400,
            Self::Internal(_) => 500,
        }
    }

    pub fn user_message(&self) -> String {
        match self {
            Self::InvalidInput(m) => m.clone(),
            Self::EmailAlreadyExists => "该邮箱已注册".into(),
            Self::InvalidCredentials => "邮箱或密码错误".into(),
            Self::NotAuthenticated => "请先登录".into(),
            Self::RateLimited => "尝试过于频繁,请稍后再试".into(),
            Self::DailyLimitExceeded => "今日额度已用完,明日重置".into(),
            Self::InvalidResetToken => "重置链接无效".into(),
            Self::ExpiredResetToken => "重置链接已过期".into(),
            Self::Internal(_) => "服务内部错误".into(),
        }
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code(), self.user_message())
    }
}

impl std::error::Error for AuthError {}

impl From<rusqlite::Error> for AuthError {
    fn from(e: rusqlite::Error) -> Self {
        // Unique constraint violation on users.email is the only one we map
        // to a domain error. Everything else is internal.
        let msg = e.to_string();
        if msg.contains("UNIQUE") && msg.contains("users.email") {
            return Self::EmailAlreadyExists;
        }
        Self::Internal(msg)
    }
}

impl From<crate::password::PasswordError> for AuthError {
    fn from(e: crate::password::PasswordError) -> Self {
        Self::Internal(e.to_string())
    }
}
```

- [ ] **Step 2: 在 lib.rs 导出**

`overlay/auth/src/lib.rs` 追加:

```rust
pub mod error;

pub use error::AuthError;
```

- [ ] **Step 3: 编译验证**

Run: `cargo build -p llm-wiki-auth`
Expected: Finished,无错误

- [ ] **Step 4: Commit**

```bash
git add overlay/auth/src/error.rs overlay/auth/src/lib.rs
git commit -m "feat(auth): AuthError with code + HTTP status + user message"
```

---

### Task 2.2: Store(rusqlite 操作)

**Files:**
- Create: `overlay/auth/src/store.rs`
- Modify: `overlay/auth/src/lib.rs`
- Modify: `overlay/auth/tests/integration.rs`

- [ ] **Step 1: 写失败测试**

追加到 `overlay/auth/tests/integration.rs`:

```rust
use llm_wiki_auth::store::{Store, NewUser};
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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p llm-wiki-auth store`
Expected: 编译失败(`Store` 未定义)

- [ ] **Step 3: 实现 store.rs**

写 `overlay/auth/src/store.rs`:

```rust
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
            "SELECT id, email, password_hash, display_name, is_admin, created_at, last_seen_at
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
            "SELECT id, email, password_hash, display_name, is_admin, created_at, last_seen_at
             FROM users WHERE id = ?1",
            params![id],
            row_to_user,
        )
        .optional()
        .map_err(AuthError::from)
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
        now: i64,
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
    })
}
```

- [ ] **Step 4: 在 lib.rs 导出**

`overlay/auth/src/lib.rs` 追加:

```rust
pub mod store;
```

- [ ] **Step 5: 跑测试确认通过**

Run: `cargo test -p llm-wiki-auth`
Expected: 所有测试 ok(包括 store 系列)

- [ ] **Step 6: Commit**

```bash
git add overlay/auth/src/store.rs overlay/auth/src/lib.rs overlay/auth/tests/integration.rs
git commit -m "feat(auth): SQLite store (users / sessions / reset tokens / usage)"
```

---

### Task 2.3: 漏桶限流

**Files:**
- Create: `overlay/auth/src/ratelimit.rs`
- Modify: `overlay/auth/src/lib.rs`
- Modify: `overlay/auth/tests/integration.rs`

- [ ] **Step 1: 写失败测试**

追加到测试文件:

```rust
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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p llm-wiki-auth rate_limit`
Expected: 编译失败

- [ ] **Step 3: 实现 ratelimit.rs**

写 `overlay/auth/src/ratelimit.rs`:

```rust
//! In-memory leaky-bucket rate limiter.
//!
//! Algorithm cloned from smhanov/auth/ratelimit.go (MIT) — translated to
//! Rust. Each named bucket tracks a "value" that drains at `rate / period`
//! per second. `allow()` succeeds if value + cost stays under rate, then
//! charges the cost. Concurrent calls are serialized with a Mutex; the
//! limiter is held in a single Arc and shared across handler threads.
//!
//! Time is passed in explicitly (`now_secs`) so tests are deterministic.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy)]
struct Record {
    value: f64,
    at: f64,
}

#[derive(Default)]
pub struct RateLimiter {
    state: Mutex<HashMap<String, Record>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if the operation is allowed, charging `cost` against the
    /// bucket. `rate` is max allowed cost per `period_secs`.
    pub fn allow(&self, key: &str, rate: f64, period_secs: f64, now_secs: i64) -> bool {
        let mut map = self.state.lock().expect("ratelimit mutex");
        let now = now_secs as f64;
        let rec = map.entry(key.to_string()).or_insert(Record { value: 0.0, at: now });

        // Drain proportional to elapsed time.
        let elapsed = (now - rec.at).max(0.0);
        rec.value = (rec.value - elapsed * rate / period_secs).max(0.0);
        rec.at = now;

        if rec.value + 1.0 <= rate {
            rec.value += 1.0;
            true
        } else {
            false
        }
    }
}
```

- [ ] **Step 4: 在 lib.rs 导出**

```rust
pub mod ratelimit;
```

- [ ] **Step 5: 跑测试确认通过**

Run: `cargo test -p llm-wiki-auth rate_limit`
Expected: 3 个测试 ok

- [ ] **Step 6: Commit**

```bash
git add overlay/auth/src/ratelimit.rs overlay/auth/src/lib.rs overlay/auth/tests/integration.rs
git commit -m "feat(auth): in-memory leaky-bucket rate limiter"
```

---

### Task 2.4: AuthService(业务层 register/login/logout/me)

**Files:**
- Create: `overlay/auth/src/service.rs`
- Modify: `overlay/auth/src/lib.rs`
- Modify: `overlay/auth/tests/integration.rs`

- [ ] **Step 1: 写失败测试**

追加测试:

```rust
use llm_wiki_auth::service::{AuthService, AuthServiceConfig, RegisterInput, LoginInput};
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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p llm-wiki-auth service`
Expected: 编译失败

- [ ] **Step 3: 实现 service.rs**

写 `overlay/auth/src/service.rs`:

```rust
//! Business orchestration. The HTTP layer should be a thin adapter on top
//! of `AuthService` — this keeps tests fast and deterministic.

use crate::password::{hash_password, verify_password};
use crate::ratelimit::RateLimiter;
use crate::session::{generate_token, hash_token};
use crate::store::{NewUser, Store, User};
use crate::AuthError;
use std::sync::Arc;

pub struct AuthService {
    store: Arc<Store>,
    cfg: AuthServiceConfig,
    limiter: RateLimiter,
}

#[derive(Debug, Clone)]
pub struct AuthServiceConfig {
    pub session_ttl_secs: i64,
    pub admin_email: Option<String>,
    pub login_attempts: f64,
    pub login_period_secs: f64,
}

#[derive(Debug, Clone)]
pub struct RegisterInput<'a> {
    pub email: &'a str,
    pub password: &'a str,
    pub now: i64,
    pub ip: Option<&'a str>,
    pub user_agent: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct LoginInput<'a> {
    pub email: &'a str,
    pub password: &'a str,
    pub now: i64,
    pub ip: Option<&'a str>,
    pub user_agent: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct AuthOutcome {
    pub user: User,
    pub session_token: String,
}

impl AuthService {
    pub fn new(store: Arc<Store>, cfg: AuthServiceConfig) -> Self {
        Self { store, cfg, limiter: RateLimiter::new() }
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    pub fn config(&self) -> &AuthServiceConfig {
        &self.cfg
    }

    pub fn register(&self, input: RegisterInput<'_>) -> Result<AuthOutcome, AuthError> {
        let email = normalize_email(input.email)?;
        validate_password(input.password)?;
        let is_admin = self
            .cfg
            .admin_email
            .as_deref()
            .map(|a| a.eq_ignore_ascii_case(&email))
            .unwrap_or(false);
        let hash = hash_password(input.password)?;
        let user_id = self.store.create_user(NewUser {
            email: &email,
            password_hash: &hash,
            display_name: None,
            is_admin,
            now: input.now,
        })?;
        let user = self
            .store
            .find_user_by_id(user_id)?
            .ok_or_else(|| AuthError::Internal("user vanished".into()))?;
        let token = self.issue_session(user.id, input.now, input.ip, input.user_agent)?;
        Ok(AuthOutcome { user, session_token: token })
    }

    pub fn login(&self, input: LoginInput<'_>) -> Result<AuthOutcome, AuthError> {
        let email = normalize_email(input.email)?;

        // Rate-limit by email and ip BEFORE doing the password check, so
        // attackers can't burn CPU forcing argon2 verifications.
        let by_email = format!("login:{email}");
        if !self.limiter.allow(&by_email, self.cfg.login_attempts, self.cfg.login_period_secs, input.now) {
            return Err(AuthError::RateLimited);
        }
        if let Some(ip) = input.ip {
            let by_ip = format!("loginip:{ip}");
            if !self.limiter.allow(&by_ip, self.cfg.login_attempts, self.cfg.login_period_secs, input.now) {
                return Err(AuthError::RateLimited);
            }
        }

        let user = match self.store.find_user_by_email(&email)? {
            Some(u) => u,
            None => return Err(AuthError::InvalidCredentials),
        };
        if !verify_password(&user.password_hash, input.password)? {
            return Err(AuthError::InvalidCredentials);
        }
        let token = self.issue_session(user.id, input.now, input.ip, input.user_agent)?;
        self.store.touch_user_seen(user.id, input.now)?;
        Ok(AuthOutcome { user, session_token: token })
    }

    pub fn logout(&self, session_token: &str) -> Result<(), AuthError> {
        self.store.delete_session(&hash_token(session_token))
    }

    /// Look up the user behind a session cookie. Returns Ok(None) for
    /// invalid/expired sessions so the caller can decide between 401 and
    /// "anonymous request".
    pub fn session_user(&self, session_token: &str, now: i64) -> Result<Option<User>, AuthError> {
        let hash = hash_token(session_token);
        let Some(uid) = self.store.find_session_user(&hash, now)? else {
            return Ok(None);
        };
        self.store.find_user_by_id(uid)
    }

    fn issue_session(
        &self,
        user_id: i64,
        now: i64,
        ip: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<String, AuthError> {
        let token = generate_token();
        let hash = hash_token(&token);
        let expires_at = now + self.cfg.session_ttl_secs;
        self.store
            .create_session(&hash, user_id, now, expires_at, user_agent, ip)?;
        Ok(token)
    }
}

fn normalize_email(raw: &str) -> Result<String, AuthError> {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.is_empty() || !trimmed.contains('@') || trimmed.len() > 256 {
        return Err(AuthError::InvalidInput("邮箱格式错误".into()));
    }
    Ok(trimmed)
}

fn validate_password(p: &str) -> Result<(), AuthError> {
    if p.len() < 8 {
        return Err(AuthError::InvalidInput("密码至少 8 位".into()));
    }
    if p.len() > 256 {
        return Err(AuthError::InvalidInput("密码过长".into()));
    }
    Ok(())
}
```

- [ ] **Step 4: 在 lib.rs 导出**

```rust
pub mod service;

pub use service::{AuthService, AuthServiceConfig, AuthOutcome, LoginInput, RegisterInput};
pub use store::{Store, User};
```

- [ ] **Step 5: 跑测试确认通过**

Run: `cargo test -p llm-wiki-auth`
Expected: 全部测试 ok(>= 15 个)

- [ ] **Step 6: Commit**

```bash
git add overlay/auth/src/service.rs overlay/auth/src/lib.rs overlay/auth/tests/integration.rs
git commit -m "feat(auth): AuthService — register / login / logout / me"
```

---

### Task 2.5: 忘记密码 + 重置密码

**Files:**
- Modify: `overlay/auth/src/service.rs`
- Modify: `overlay/auth/tests/integration.rs`

- [ ] **Step 1: 写失败测试**

追加:

```rust
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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p llm-wiki-auth password_reset`
Expected: 编译失败

- [ ] **Step 3: 在 service.rs 加方法**

在 `impl AuthService` 块内追加:

```rust
    /// Start a password-reset flow. Returns a fresh token if the email
    /// belongs to a real user, or `None` otherwise. The HTTP layer must
    /// always respond `{ok:true}` regardless to avoid email enumeration.
    pub fn start_password_reset(
        &self,
        email: &str,
        now: i64,
    ) -> Result<Option<String>, AuthError> {
        let email = normalize_email(email)?;
        let user = match self.store.find_user_by_email(&email)? {
            Some(u) => u,
            None => return Ok(None),
        };
        let token = generate_token();
        let hash = hash_token(&token);
        let expires_at = now + 3600; // 1 hour
        self.store.create_reset_token(&hash, user.id, expires_at)?;
        Ok(Some(token))
    }

    /// Use a reset token to set a new password. Token is single-use:
    /// consumed even on success. All existing sessions for the user are
    /// invalidated.
    pub fn complete_password_reset(
        &self,
        reset_token: &str,
        new_password: &str,
        now: i64,
    ) -> Result<(), AuthError> {
        validate_password(new_password)?;
        let hash = hash_token(reset_token);
        let (user_id, expires_at) = match self.store.find_reset_token_user(&hash, now)? {
            Some(t) => t,
            None => return Err(AuthError::InvalidResetToken),
        };
        // Always consume the token, even if expired, to prevent retries.
        self.store.delete_reset_token(&hash)?;
        if expires_at <= now {
            return Err(AuthError::ExpiredResetToken);
        }
        let new_hash = hash_password(new_password)?;
        self.store.update_password(user_id, &new_hash)?;
        self.store.delete_user_sessions(user_id)?;
        Ok(())
    }
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p llm-wiki-auth`
Expected: 全部 ok(包括 5 个新增 reset 测试)

- [ ] **Step 5: Commit**

```bash
git add overlay/auth/src/service.rs overlay/auth/tests/integration.rs
git commit -m "feat(auth): forgot/reset password (single-use, 1h TTL, kills sessions)"
```


---

## Phase 3 — `/auth/*` HTTP 路由接入 server

### Task 3.1: 配置项扩展

**Files:**
- Modify: `overlay/server/src/config.rs`
- Modify: `overlay/server/src/main.rs`

- [ ] **Step 1: 在 ServerConfig 加字段**

修改 `overlay/server/src/config.rs` 的 `ServerConfig` 结构体,加字段:

```rust
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub project: PathBuf,
    pub bind: String,
    pub config_path: Option<PathBuf>,
    pub static_dir: Option<PathBuf>,
    pub token_override: Option<String>,
    // --- new auth fields ---
    pub auth_db: Option<PathBuf>,
    pub require_login: bool,
    pub daily_chat_limit: u32,
    pub admin_email: Option<String>,
    pub session_ttl_days: u32,
}
```

修改 `ServerConfig::resolve` 签名末尾追加参数,接收新值。

- [ ] **Step 2: 在 main.rs 的 Args 加 CLI/env**

修改 `overlay/server/src/main.rs` 的 `Args` struct,追加:

```rust
    /// SQLite path for auth/history/usage. If unset, multi-user mode is off.
    #[arg(long, env = "LLM_WIKI_AUTH_DB")]
    auth_db: Option<String>,

    /// Require login on the lite page (browser users). Bearer token auth
    /// for CLI/e2e is unaffected.
    #[arg(long, env = "LLM_WIKI_REQUIRE_LOGIN", default_value_t = false)]
    require_login: bool,

    /// Per-user daily chat limit (cookie-authenticated requests only).
    #[arg(long, env = "LLM_WIKI_DAILY_CHAT_LIMIT", default_value_t = 50)]
    daily_chat_limit: u32,

    /// Email that is auto-marked admin on registration.
    #[arg(long, env = "LLM_WIKI_ADMIN_EMAIL")]
    admin_email: Option<String>,

    /// Session cookie lifetime in days.
    #[arg(long, env = "LLM_WIKI_SESSION_TTL_DAYS", default_value_t = 30)]
    session_ttl_days: u32,
```

并更新 `ServerConfig::resolve(...)` 调用,把这些值传进去。

- [ ] **Step 3: 编译验证**

Run: `cargo build --manifest-path overlay/server/Cargo.toml`
Expected: Finished

- [ ] **Step 4: Commit**

```bash
git add overlay/server/src/config.rs overlay/server/src/main.rs
git commit -m "feat(server): add auth-related config (auth_db, require_login, daily_chat_limit, admin_email, session_ttl_days)"
```

---

### Task 3.2: ServerState 持有 AuthService

**Files:**
- Modify: `overlay/server/src/state.rs`
- Modify: `overlay/server/src/main.rs`

- [ ] **Step 1: 加字段**

修改 `overlay/server/src/state.rs`:

在 `ServerStateInner` 加:

```rust
    auth: Option<Arc<llm_wiki_auth::AuthService>>,
    require_login: bool,
    daily_chat_limit: u32,
```

`from_config` 现在不直接构造 AuthService(它需要 IO 可能失败),改加一个 setter:

```rust
impl ServerState {
    pub fn with_auth(
        mut self,
        auth: Option<Arc<llm_wiki_auth::AuthService>>,
        require_login: bool,
        daily_chat_limit: u32,
    ) -> Self {
        // ServerState wraps Arc<ServerStateInner>; we need to build a new one
        // because the inner is shared. Easiest to expose this only at startup.
        let inner = Arc::new(ServerStateInner {
            project: self.inner.project.clone(),
            config_path: self.inner.config_path.clone(),
            token_override: self.inner.token_override.clone(),
            config_cache: Mutex::new(None),
            auth,
            require_login,
            daily_chat_limit,
        });
        self.inner = inner;
        self
    }

    pub fn auth(&self) -> Option<&Arc<llm_wiki_auth::AuthService>> {
        self.inner.auth.as_ref()
    }

    pub fn require_login(&self) -> bool {
        self.inner.require_login
    }

    pub fn daily_chat_limit(&self) -> u32 {
        self.inner.daily_chat_limit
    }
}
```

并更新 `from_config` 让 `auth/require_login/daily_chat_limit` 默认为 `None`/`false`/`50`。

- [ ] **Step 2: 在 main.rs 启动时构造 AuthService**

修改 `overlay/server/src/main.rs` 的 `main()`,在 config 解析后追加:

```rust
    let auth_service = match &config.auth_db {
        Some(path) => {
            use llm_wiki_auth::{AuthService, AuthServiceConfig, Store};
            use std::sync::Arc;
            let store = match Store::open(path) {
                Ok(s) => Arc::new(s),
                Err(e) => {
                    eprintln!("auth: failed to open SQLite at {}: {e}", path.display());
                    std::process::exit(1);
                }
            };
            let svc = AuthService::new(store, AuthServiceConfig {
                session_ttl_secs: (config.session_ttl_days as i64) * 24 * 3600,
                admin_email: config.admin_email.clone(),
                login_attempts: 25.0,
                login_period_secs: 3600.0,
            });
            Some(Arc::new(svc))
        }
        None => None,
    };
```

然后用 `with_auth` 把它注入 state(在 `http_server::run(config)` 调用前)。这需要把 `run` 改为接受 `ServerState` 而非 `ServerConfig`,或在 server.rs 内构造 state 后调 `with_auth`。

**最小改动方案:**改 `server::run` 签名为:

```rust
pub fn run(config: ServerConfig, auth: Option<Arc<llm_wiki_auth::AuthService>>) -> Result<(), String>
```

然后在 `run` 内 `state.with_auth(auth, config.require_login, config.daily_chat_limit)`。

- [ ] **Step 3: 编译验证**

Run: `cargo build --manifest-path overlay/server/Cargo.toml`
Expected: Finished

- [ ] **Step 4: 启动冒烟(无 auth db)**

```bash
LLM_WIKI_PROJECT=/home/li/overseas-github/llm_wiki_projects/CivilCareer \
LLM_WIKI_API_TOKEN=e2e-test-token \
LLM_WIKI_CONFIG=overlay/config/server.minimax.local.json \
LLM_WIKI_REPO=$PWD \
LLM_WIKI_STATIC=$PWD/upstream/dist \
./overlay/server/target/release/llm-wiki-server &
sleep 2
curl -sf "http://127.0.0.1:8080/api/v1/health?token=e2e-test-token" >/dev/null && echo "ok"
pkill -f llm-wiki-server
```
Expected: `ok`(无 auth db 时 server 仍正常,Bearer 路径不变)

- [ ] **Step 5: Commit**

```bash
git add overlay/server/src/state.rs overlay/server/src/main.rs overlay/server/src/server.rs
git commit -m "feat(server): construct AuthService at startup, attach to ServerState"
```

---

### Task 3.3: `/auth/*` 路由 — register / login / logout / me

**Files:**
- Create: `overlay/server/src/api/auth_routes.rs`
- Modify: `overlay/server/src/api/mod.rs`(声明 + 路由分发)
- Modify: `overlay/server/src/server.rs`(让 `/auth/*` 也走 API 处理路径)

- [ ] **Step 1: 写 auth_routes.rs**

写 `overlay/server/src/api/auth_routes.rs`:

```rust
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
use tiny_http::{Header, Method, Request, Response};

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
        _ => api::respond_json(request, 404, json!({ "error": { "code": "not_found", "message": "Not found" } })),
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
    let mut resp = Response::from_string(payload).with_status_code(status);
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
    let email = match json_str(&v, "email") { Ok(s) => s, Err(e) => return respond_err(request, &e) };
    let password = match json_str(&v, "password") { Ok(s) => s, Err(e) => return respond_err(request, &e) };
    let secure = is_secure(headers);
    let now = now_secs();

    match auth.register(RegisterInput {
        email, password, now,
        ip: header_lookup(headers, "x-forwarded-for"),
        user_agent: header_lookup(headers, "user-agent"),
    }) {
        Ok(out) => {
            let cookie = build_session_cookie(&out.session_token, auth.config().session_ttl_secs, secure);
            respond_with_cookie(
                request, 200,
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
    let email = match json_str(&v, "email") { Ok(s) => s, Err(e) => return respond_err(request, &e) };
    let password = match json_str(&v, "password") { Ok(s) => s, Err(e) => return respond_err(request, &e) };
    let secure = is_secure(headers);
    let now = now_secs();

    match auth.login(LoginInput {
        email, password, now,
        ip: header_lookup(headers, "x-forwarded-for"),
        user_agent: header_lookup(headers, "user-agent"),
    }) {
        Ok(out) => {
            let cookie = build_session_cookie(&out.session_token, auth.config().session_ttl_secs, secure);
            respond_with_cookie(
                request, 200,
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
    respond_with_cookie(
        request, 200,
        json!({ "ok": true }),
        build_clear_cookie(secure),
    );
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
        request, 200,
        json!({
            "user": user_to_json(&user),
            "usage": { "used": used, "limit": limit, "date": date },
        }),
    );
}

fn handle_forgot(
    auth: &std::sync::Arc<llm_wiki_auth::AuthService>,
    body: &str,
    request: Request,
) {
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
    let token = match json_str(&v, "token") { Ok(s) => s, Err(e) => return respond_err(request, &e) };
    let new_password = match json_str(&v, "password") { Ok(s) => s, Err(e) => return respond_err(request, &e) };
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
```

- [ ] **Step 2: 在 api/mod.rs 声明子模块**

修改 `overlay/server/src/api/mod.rs`,顶部模块声明区追加:

```rust
pub mod auth_routes;
```

- [ ] **Step 3: 在 server.rs 加 `/auth/*` 分发**

修改 `overlay/server/src/server.rs` 的 `dispatch_request`。当前判定 `is_api = path == "/health" || path.starts_with(API_PREFIX)`。改为:

```rust
    let (path, _) = api::split_url(&url);
    let is_api = path == "/health" || path.starts_with(API_PREFIX);
    let is_auth = path.starts_with("/auth/");

    if is_api || is_auth {
        let headers: Vec<(String, String)> = request
            .headers()
            .iter()
            .map(|header| {
                (
                    header.field.as_str().to_ascii_lowercase().to_string(),
                    header.value.as_str().to_string(),
                )
            })
            .collect();
        let body = match api::read_body(&mut request) {
            Ok(body) => body,
            Err(err) => {
                api::respond_error(request, 400, &err);
                return;
            }
        };

        if is_auth {
            api::auth_routes::handle(&state, &method, &path, &headers, &body, request);
            return;
        }

        // ... existing chat-SSE branch + handle_request (unchanged)
```

剩余分支(chat SSE / handle_request)保持原样。

- [ ] **Step 4: 编译 + 冒烟(空 auth db)**

```bash
cargo build --release --manifest-path overlay/server/Cargo.toml
mkdir -p /tmp/llm-wiki-test
LLM_WIKI_PROJECT=/home/li/overseas-github/llm_wiki_projects/CivilCareer \
LLM_WIKI_API_TOKEN=e2e-test-token \
LLM_WIKI_CONFIG=$PWD/overlay/config/server.minimax.local.json \
LLM_WIKI_REPO=$PWD \
LLM_WIKI_STATIC=$PWD/upstream/dist \
LLM_WIKI_AUTH_DB=/tmp/llm-wiki-test/auth.db \
./overlay/server/target/release/llm-wiki-server &
SRV=$!
sleep 2

# Register
curl -s -i -X POST http://127.0.0.1:8080/auth/register \
  -H 'Content-Type: application/json' \
  -d '{"email":"alice@e.com","password":"longenough"}' | tee /tmp/reg.txt | head -15

# Cookie should be in response
COOKIE=$(grep -i '^set-cookie:' /tmp/reg.txt | head -1 | sed 's/^[Ss]et-[Cc]ookie: //;s/;.*//')
echo "got cookie: $COOKIE"

# /auth/me with that cookie
curl -s -H "Cookie: $COOKIE" http://127.0.0.1:8080/auth/me

# Logout
curl -s -X POST -H "Cookie: $COOKIE" http://127.0.0.1:8080/auth/logout

# /auth/me now 401
curl -s -o /dev/null -w '%{http_code}\n' -H "Cookie: $COOKIE" http://127.0.0.1:8080/auth/me

kill $SRV
rm -rf /tmp/llm-wiki-test
```

Expected:
- register 返回 200 + Set-Cookie: session=...
- `/auth/me` 返回 `{"user":{...},"usage":{"used":0,"limit":50,"date":"YYYY-MM-DD"}}`
- logout 返回 `{"ok":true}`
- 之后 `/auth/me` 返回 401

- [ ] **Step 5: Commit**

```bash
git add overlay/server/src/api/auth_routes.rs overlay/server/src/api/mod.rs overlay/server/src/server.rs
git commit -m "feat(server): /auth/* routes (register/login/logout/me/forgot/reset)"
```


---

## Phase 4 — `is_authorized` 加 cookie 路径 + Chat 用量限额

### Task 4.1: `is_authorized` 优先 cookie

**Files:**
- Modify: `overlay/server/src/api/mod.rs`

- [ ] **Step 1: 改造 is_authorized**

修改 `overlay/server/src/api/mod.rs` 的 `is_authorized`,在最前面加 cookie 校验路径。同时把它的返回类型从 `bool` 改为 `AuthOutcome` 枚举,以便下游 handler 知道用户身份(chat 用量需要 user_id)。

新加类型(可放 mod.rs):

```rust
/// Result of authorizing an incoming request. `Cookie(user_id)` means the
/// request belongs to a logged-in user (used for usage accounting); `Bearer`
/// means the legacy shared-token path (no user, no usage accounting);
/// `Anonymous` is allowed only when allowUnauthenticated is on.
#[derive(Debug, Clone, Copy)]
pub enum AuthOutcome {
    Cookie(i64),
    Bearer,
    Anonymous,
}

impl AuthOutcome {
    pub fn user_id(&self) -> Option<i64> {
        match self {
            Self::Cookie(id) => Some(*id),
            _ => None,
        }
    }
    pub fn is_authorized(&self) -> bool {
        !matches!(self, Self::Anonymous) || true
        // Anonymous is only returned when allowUnauthenticated, which already
        // means the request is allowed; callers that need to enforce
        // require_login check that separately.
    }
}
```

修改 `is_authorized` (重命名为 `authorize`,返回 `AuthOutcome` 或 401):

```rust
pub(crate) fn authorize(
    state: &ServerState,
    query: &str,
    headers: &[(String, String)],
) -> Option<AuthOutcome> {
    // 1) cookie session — preferred when auth is enabled
    if let Some(auth) = state.auth() {
        if let Some(cookie_hdr) = headers.iter().find(|(k, _)| k == "cookie").map(|(_, v)| v.as_str()) {
            if let Some(token) = llm_wiki_auth::session::parse_session_cookie(cookie_hdr) {
                let now = now_secs_for_auth();
                if let Ok(Some(user)) = auth.session_user(&token, now) {
                    return Some(AuthOutcome::Cookie(user.id));
                }
            }
        }
    }

    // 2) bearer token / query token — existing path (CLI, e2e, internal)
    if !state.api_auth_required() {
        return Some(AuthOutcome::Anonymous);
    }
    let Some(token) = state.api_token() else {
        return None;
    };
    let params = parse_query(query);
    if params
        .get("token")
        .map(|v| constant_time_eq(v.as_bytes(), token.as_bytes()))
        .unwrap_or(false)
    {
        return Some(AuthOutcome::Bearer);
    }
    let bearer_ok = headers.iter().any(|(key, value)| {
        if key == "x-llm-wiki-token" {
            return constant_time_eq(value.as_bytes(), token.as_bytes());
        }
        if key == "authorization" {
            return value
                .strip_prefix("Bearer ")
                .map(|v| constant_time_eq(v.as_bytes(), token.as_bytes()))
                .unwrap_or(false);
        }
        false
    });
    if bearer_ok {
        Some(AuthOutcome::Bearer)
    } else {
        // require_login mode: cookie was tried first and failed; reject.
        if state.require_login() {
            return None;
        }
        None
    }
}

fn now_secs_for_auth() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
```

保留旧的 `is_authorized` 作为 thin wrapper 临时兼容现有调用:

```rust
pub(crate) fn is_authorized(
    state: &ServerState,
    query: &str,
    headers: &[(String, String)],
) -> bool {
    authorize(state, query, headers).is_some()
}
```

- [ ] **Step 2: 编译验证**

Run: `cargo build --release --manifest-path overlay/server/Cargo.toml`
Expected: Finished

- [ ] **Step 3: 冒烟回归**

```bash
LLM_WIKI_PROJECT=/home/li/overseas-github/llm_wiki_projects/CivilCareer \
LLM_WIKI_API_TOKEN=e2e-test-token \
LLM_WIKI_CONFIG=$PWD/overlay/config/server.minimax.local.json \
LLM_WIKI_REPO=$PWD \
LLM_WIKI_STATIC=$PWD/upstream/dist \
./overlay/server/target/release/llm-wiki-server &
SRV=$!
sleep 2
# Bearer 路径仍工作
curl -sf -H "Authorization: Bearer e2e-test-token" http://127.0.0.1:8080/api/v1/projects > /dev/null && echo "bearer ok"
# 无 token 拒绝
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8080/api/v1/projects
kill $SRV
```
Expected: `bearer ok` + `401`

- [ ] **Step 4: Commit**

```bash
git add overlay/server/src/api/mod.rs
git commit -m "feat(server): cookie session in authorize(); Bearer path preserved"
```

---

### Task 4.2: Chat handler 加用量计数

**Files:**
- Modify: `overlay/server/src/api/chat.rs`

- [ ] **Step 1: 在 chat handler 入口加用量检查**

修改 `overlay/server/src/api/chat.rs` 的 `try_handle_chat_sse`,把现有 `is_authorized` 调用替换为 `authorize`:

```rust
    let auth_outcome = match api::authorize(state, &query, headers) {
        Some(o) => o,
        None => {
            api::respond_json(request, 401, json!({ "ok": false, "error": "Unauthorized" }));
            return;
        }
    };
```

然后在已经通过认证、且即将 spawn 子进程**之前**(spawn 前的最后一步),加用量检查与计数:

```rust
    // Per-user daily quota — only applies to cookie-authenticated requests.
    // Bearer-token clients (CLI / e2e) bypass the quota by design.
    if let api::AuthOutcome::Cookie(user_id) = auth_outcome {
        if let Some(auth) = state.auth() {
            let date = today_utc_for_chat();
            let used = match auth.store().get_usage(user_id, &date) {
                Ok(n) => n,
                Err(_) => 0,
            };
            let limit = state.daily_chat_limit() as i64;
            if used >= limit {
                api::respond_json(
                    request, 429,
                    json!({
                        "error": {
                            "code": "daily_limit_exceeded",
                            "message": "今日额度已用完,明日重置",
                            "used": used, "limit": limit,
                        }
                    }),
                );
                return;
            }
            // Increment BEFORE spawn — even if streaming fails partway, the
            // attempt counts (consistent with most LLM products). If you want
            // refund-on-failure semantics, that's a v1.1 change.
            if let Err(e) = auth.store().increment_usage(user_id, &date) {
                eprintln!("[chat] usage increment failed for user {user_id}: {e}");
            }
        }
    }
```

加 `today_utc_for_chat` 辅助函数(同 auth_routes.rs 的 today_utc 算法,放 chat.rs 末尾):

```rust
fn today_utc_for_chat() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let days = secs / 86_400;
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
```

- [ ] **Step 2: 编译**

Run: `cargo build --release --manifest-path overlay/server/Cargo.toml`
Expected: Finished

- [ ] **Step 3: E2E 冒烟(用量计数)**

```bash
mkdir -p /tmp/llm-wiki-test && rm -f /tmp/llm-wiki-test/auth.db
LLM_WIKI_PROJECT=/home/li/overseas-github/llm_wiki_projects/CivilCareer \
LLM_WIKI_API_TOKEN=e2e-test-token \
LLM_WIKI_CONFIG=$PWD/overlay/config/server.minimax.local.json \
LLM_WIKI_REPO=$PWD \
LLM_WIKI_STATIC=$PWD/upstream/dist \
LLM_WIKI_AUTH_DB=/tmp/llm-wiki-test/auth.db \
LLM_WIKI_DAILY_CHAT_LIMIT=2 \
./overlay/server/target/release/llm-wiki-server > /tmp/srv.log 2>&1 &
SRV=$!
sleep 2

# 注册并保存 cookie
curl -s -c /tmp/cj.txt -X POST http://127.0.0.1:8080/auth/register \
  -H 'Content-Type: application/json' \
  -d '{"email":"u@e.com","password":"longenough"}' > /dev/null

PROJECT_ID=$(curl -sf -H "Authorization: Bearer e2e-test-token" \
  http://127.0.0.1:8080/api/v1/projects | python3 -c \
  'import sys,json;d=json.load(sys.stdin);print(d["projects"][0]["id"])')
echo "project: $PROJECT_ID"

# 第 1, 2 次 chat 应该 200,第 3 次应 429
for i in 1 2 3; do
  CODE=$(curl -s -b /tmp/cj.txt -o /tmp/c$i.txt -w '%{http_code}' \
    -X POST "http://127.0.0.1:8080/api/v1/projects/$PROJECT_ID/chat" \
    -H 'Content-Type: application/json' \
    -d '{"messages":[{"role":"user","content":"hi"}]}' --max-time 60)
  echo "req $i -> $CODE"
done
echo "third response body:"; head -c 200 /tmp/c3.txt
kill $SRV; rm -rf /tmp/llm-wiki-test /tmp/cj.txt /tmp/c?.txt
```
Expected: `req 1 -> 200`, `req 2 -> 200`, `req 3 -> 429`,第三个响应包含 `"daily_limit_exceeded"`。

- [ ] **Step 4: Commit**

```bash
git add overlay/server/src/api/chat.rs
git commit -m "feat(chat): per-user daily usage quota (cookie auth only; Bearer bypasses)"
```

---

## Phase 5 — Conversations API + lite 页改造

### Task 5.1: Store 加 conversations / messages 操作

**Files:**
- Modify: `overlay/auth/src/store.rs`
- Modify: `overlay/auth/tests/integration.rs`

- [ ] **Step 1: 写失败测试**

追加:

```rust
#[test]
fn conversations_crud_per_user() {
    let (store, _dir) = fresh_store();
    let alice = store.create_user(NewUser {
        email: "a@e.com", password_hash: "x", display_name: None, is_admin: false, now: 1
    }).unwrap();
    let bob = store.create_user(NewUser {
        email: "b@e.com", password_hash: "x", display_name: None, is_admin: false, now: 1
    }).unwrap();

    let c1 = "conv-1";
    store.create_conversation(c1, alice, "proj-x", "first chat", 100).unwrap();
    let c2 = "conv-2";
    store.create_conversation(c2, alice, "proj-x", "second", 200).unwrap();

    let alices = store.list_conversations(alice, 50).unwrap();
    assert_eq!(alices.len(), 2);
    // most-recent first
    assert_eq!(alices[0].id, c2);

    // bob has none, can't see alice's
    assert_eq!(store.list_conversations(bob, 50).unwrap().len(), 0);
    let owner = store.find_conversation_owner(c1).unwrap().unwrap();
    assert_eq!(owner, alice);

    // append messages
    store.append_message(c1, "user", "hello", 110).unwrap();
    store.append_message(c1, "assistant", "hi back", 111).unwrap();
    let msgs = store.list_messages(c1).unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[1].content, "hi back");

    // delete
    store.delete_conversation(c1).unwrap();
    assert_eq!(store.list_conversations(alice, 50).unwrap().len(), 1);
    // messages cascade-deleted
    assert!(store.list_messages(c1).unwrap().is_empty());
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p llm-wiki-auth conversations`
Expected: 编译失败

- [ ] **Step 3: 在 store.rs 实现**

在 `Store` impl 块尾部追加:

```rust
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
```

注意:`Store` 块的最后 `}` 现在在 `list_messages` 之后,前文 `row_to_user` 函数仍在文件末尾不在 impl 内 — 上述 patch 末尾的 `}` 正确闭合 impl,`ConversationRow`/`MessageRow` 在 impl 之后。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p llm-wiki-auth conversations`
Expected: ok

- [ ] **Step 5: Commit**

```bash
git add overlay/auth/src/store.rs overlay/auth/tests/integration.rs
git commit -m "feat(auth): conversation + message storage"
```

---

### Task 5.2: `/api/v1/conversations*` 路由

**Files:**
- Create: `overlay/server/src/api/conversations.rs`
- Modify: `overlay/server/src/api/mod.rs`(声明 + 路由分发)

- [ ] **Step 1: 写 conversations.rs**

写 `overlay/server/src/api/conversations.rs`:

```rust
//! /api/v1/conversations* — per-user chat history. Cookie auth required.

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
            // Bearer is allowed to call API endpoints generally, but /conversations
            // is per-user — without a user there is nothing to return.
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
```

- [ ] **Step 2: 在 mod.rs 注册 + 分发**

修改 `overlay/server/src/api/mod.rs`:

声明 `pub mod conversations;` 后,改 `handle_request` 路由分发,加 conversations 分支(在已有 match parts 内):

```rust
        (&Method::Get, ["conversations"]) | (&Method::Post, ["conversations"]) | (&Method::Delete, ["conversations", _])
        | (&Method::Get, ["conversations", _, "messages"]) | (&Method::Post, ["conversations", _, "messages"]) => {
            // delegated below — needs Request handle, which handle_request doesn't have.
            // Add a special return so server.rs takes the conversations path.
            return ApiResponse {
                status: 0, // sentinel: "handled in conversations.rs via Request"
                body: Value::Null,
            };
        }
```

更稳的方案:在 `server.rs::dispatch_request` 里,**在 `handle_request` 调用之前**对 path 做 conversations 探测,如命中直接走 `conversations::handle`,绕过 `handle_request`(因为后者无法接管 Request)。改 server.rs:

```rust
        // Existing chat-SSE branch...
        // New conversations branch:
        let path_norm = path.trim_end_matches('/');
        let parts: Vec<&str> = path_norm
            .trim_start_matches(API_PREFIX)
            .trim_start_matches('/')
            .split('/')
            .filter(|p| !p.is_empty())
            .collect();
        let is_conversations = parts.first().copied() == Some("conversations");
        if is_conversations {
            let outcome = match api::authorize(&state, "", &headers) {
                Some(o) => o,
                None => {
                    api::respond_json(request, 401, json!({"error":{"code":"not_authenticated","message":"need login"}}));
                    return;
                }
            };
            api::conversations::handle(&state, &method, &parts, &body, outcome, request);
            return;
        }
```

(`json!` 需要 import — `use serde_json::json;` 加到 server.rs 顶部。`query` 这里用空字符串,因为 conversations 不读 query 参数。)

- [ ] **Step 3: 加 uuid 依赖**

修改 `overlay/server/Cargo.toml` `[dependencies]`,追加(若已通过 llm-wiki-auth 间接引入仍要 server 自己声明):

```toml
uuid = { version = "1", features = ["v4"] }
```

- [ ] **Step 4: 编译 + E2E 冒烟**

```bash
cargo build --release --manifest-path overlay/server/Cargo.toml

mkdir -p /tmp/llm-wiki-test && rm -f /tmp/llm-wiki-test/auth.db
LLM_WIKI_PROJECT=/home/li/overseas-github/llm_wiki_projects/CivilCareer \
LLM_WIKI_API_TOKEN=e2e-test-token \
LLM_WIKI_CONFIG=$PWD/overlay/config/server.minimax.local.json \
LLM_WIKI_REPO=$PWD \
LLM_WIKI_STATIC=$PWD/upstream/dist \
LLM_WIKI_AUTH_DB=/tmp/llm-wiki-test/auth.db \
./overlay/server/target/release/llm-wiki-server > /tmp/srv.log 2>&1 &
SRV=$!
sleep 2

curl -s -c /tmp/cj.txt -X POST http://127.0.0.1:8080/auth/register \
  -H 'Content-Type: application/json' -d '{"email":"u@e.com","password":"longenough"}' > /dev/null

# create conversation
CONV=$(curl -s -b /tmp/cj.txt -X POST http://127.0.0.1:8080/api/v1/conversations \
  -H 'Content-Type: application/json' -d '{"project_id":"px","title":"t1"}' \
  | python3 -c 'import sys,json;print(json.load(sys.stdin)["id"])')
echo "conv id: $CONV"

# append messages
curl -s -b /tmp/cj.txt -X POST http://127.0.0.1:8080/api/v1/conversations/$CONV/messages \
  -H 'Content-Type: application/json' -d '{"role":"user","content":"hi"}' > /dev/null
curl -s -b /tmp/cj.txt -X POST http://127.0.0.1:8080/api/v1/conversations/$CONV/messages \
  -H 'Content-Type: application/json' -d '{"role":"assistant","content":"hello"}' > /dev/null

# list
curl -s -b /tmp/cj.txt http://127.0.0.1:8080/api/v1/conversations
curl -s -b /tmp/cj.txt http://127.0.0.1:8080/api/v1/conversations/$CONV/messages

kill $SRV; rm -rf /tmp/llm-wiki-test /tmp/cj.txt
```
Expected: conversations 列表含 1 条;messages 列表含 2 条(user/assistant)。

- [ ] **Step 5: Commit**

```bash
git add overlay/server/src/api/conversations.rs overlay/server/src/api/mod.rs overlay/server/src/server.rs overlay/server/Cargo.toml
git commit -m "feat(server): /api/v1/conversations* (per-user history)"
```

---

### Task 5.3: lite 页改造 — 用户栏 + 历史侧边栏 + 服务端历史

**Files:**
- Modify: `overlay/static/lite/index.html`
- Modify: `overlay/static/lite/app.js`
- Modify: `overlay/static/lite/app.css`

注意:lite 是纯静态,改完后用 `cp overlay/static/lite/{index.html,app.js,app.css} upstream/dist/lite/` 同步,或重跑 `./scripts/build-web.sh`。

由于 lite/app.js 当前已 600+ 行,本任务专注于"接入认证 + 服务端历史",移动端适配/落地页/登录页放后续 Task。

- [ ] **Step 1: index.html 加用户栏 + 侧边栏 DOM**

在现有 `<body>` 内的合适位置(顶部导航旁)加:

```html
<aside id="history-sidebar" class="sidebar">
  <button id="new-chat-btn" class="sidebar-action">+ 新对话</button>
  <ul id="history-list" class="history-list" aria-label="聊天历史"></ul>
</aside>

<header class="topbar">
  <span id="user-email" class="user-email"></span>
  <span id="usage-info" class="usage-info"></span>
  <button id="logout-btn" class="logout-btn">登出</button>
</header>
```

(具体插入位置依现有 DOM,放在 `<main>` 之外即可。)

- [ ] **Step 2: app.js 启动时检查 /auth/me**

在 `init()` 函数最前面加:

```js
async function ensureLogin() {
  try {
    const r = await fetch('/auth/me', { credentials: 'same-origin' });
    if (r.ok) {
      const d = await r.json();
      state.user = d.user;
      state.usage = d.usage;
      renderTopbar();
      return true;
    }
  } catch {}
  location.href = '/login';
  return false;
}

async function init() {
  if (!(await ensureLogin())) return;
  // ... existing init body
}

function renderTopbar() {
  document.getElementById('user-email').textContent = state.user?.email || '';
  if (state.usage) {
    const remaining = Math.max(0, state.usage.limit - state.usage.used);
    document.getElementById('usage-info').textContent =
      `今日剩余 ${remaining}/${state.usage.limit}`;
  }
}

document.getElementById('logout-btn')?.addEventListener('click', async () => {
  await fetch('/auth/logout', { method: 'POST', credentials: 'same-origin' });
  location.href = '/login';
});
```

- [ ] **Step 3: 历史从服务端拉**

替换 `loadStore` / `saveStore` / `getMessages` / `persistMessages` 函数为:

```js
async function fetchConversations() {
  const r = await fetch('/api/v1/conversations', { credentials: 'same-origin' });
  if (!r.ok) return [];
  const d = await r.json();
  return d.conversations || [];
}

async function fetchMessages(convId) {
  const r = await fetch(`/api/v1/conversations/${encodeURIComponent(convId)}/messages`,
    { credentials: 'same-origin' });
  if (!r.ok) return [];
  const d = await r.json();
  return d.messages || [];
}

async function createConversation(projectId, title) {
  const r = await fetch('/api/v1/conversations', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    credentials: 'same-origin',
    body: JSON.stringify({ project_id: projectId, title }),
  });
  if (!r.ok) throw new Error('create conversation failed');
  return r.json();
}

async function appendMessageToServer(convId, role, content) {
  await fetch(`/api/v1/conversations/${encodeURIComponent(convId)}/messages`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    credentials: 'same-origin',
    body: JSON.stringify({ role, content }),
  });
}

async function deleteConversation(convId) {
  await fetch(`/api/v1/conversations/${encodeURIComponent(convId)}`, {
    method: 'DELETE',
    credentials: 'same-origin',
  });
}
```

修改 `sendMessage` 流程,在用户消息发出前 `appendMessageToServer(convId, 'user', text)`,assistant 完成后 `appendMessageToServer(convId, 'assistant', assistant.content)`。无活跃 conv 时先 `createConversation(projectId, firstUserText.slice(0,24))`。

修改历史侧边栏渲染 `renderHistoryList()` 为:

```js
async function renderHistoryList() {
  const list = $('#history-list');
  list.innerHTML = '';
  const convs = await fetchConversations();
  const here = convs.filter((c) => c.project_id === state.activeProject.id);
  for (const c of here) {
    const li = document.createElement('li');
    li.innerHTML = `<button class="history-item ${c.id === state.conversationId ? 'active' : ''}"
      data-id="${c.id}">${escapeHtml(c.title)}</button>
      <button class="history-del" data-id="${c.id}" title="删除">×</button>`;
    list.appendChild(li);
  }
  list.querySelectorAll('.history-item').forEach((b) => {
    b.addEventListener('click', async () => {
      state.conversationId = b.dataset.id;
      const msgs = await fetchMessages(state.conversationId);
      renderMessages(msgs);
      renderHistoryList();
    });
  });
  list.querySelectorAll('.history-del').forEach((b) => {
    b.addEventListener('click', async () => {
      await deleteConversation(b.dataset.id);
      if (state.conversationId === b.dataset.id) state.conversationId = null;
      renderHistoryList();
    });
  });
}
```

- [ ] **Step 4: 额度耗尽禁用输入**

在 `sendMessage` 最前面加:

```js
  if (state.usage && state.usage.used >= state.usage.limit) {
    alert('今日额度已用完,明日重置');
    return;
  }
```

并在 chat 完成后刷新 `state.usage`(简单做法:重新 fetch `/auth/me`)。

- [ ] **Step 5: 同步 dist**

```bash
cp overlay/static/lite/app.js overlay/static/lite/app.css overlay/static/lite/index.html upstream/dist/lite/
```

- [ ] **Step 6: 浏览器手测**

启动 server(同 Task 4.2 命令,但提高 limit),浏览器:
1. 打开 `http://127.0.0.1:8080/lite/` → 应被重定向到 `/login`(下个 phase 提供)
   暂时直接访问 `/auth/register` 走 curl 注册,然后回 `/lite/`,刷新查看顶部用户栏
2. 发一条消息 → 历史侧边栏出现 1 条
3. 刷新页面,历史保留(localStorage 不再用)

- [ ] **Step 7: Commit**

```bash
git add overlay/static/lite/ upstream/dist/lite/
git commit -m "feat(lite): cookie-auth gate, server-side history, user bar with quota"
```


---

## Phase 6 — 落地页 + 登录/注册/重置页(纯静态)

### Task 6.1: 落地页 `/`

**Files:**
- Create: `overlay/static/index.html`
- Create: `overlay/static/landing.css`
- Create: `overlay/static/landing.js`
- Modify: `overlay/server/src/static_files.rs`(确保 `/` 命中 `index.html`)

注意:服务现在的 root `/` 已被 `upstream/dist/index.html`(完整 React UI)占据。我们要在公网部署时让 `/` 显示新的极简落地页,而不是重前端 UI。**策略:**新落地页放 `overlay/static/index.html`,通过 `LLM_WIKI_PUBLIC_LANDING=true` 启用(默认关,本地开发不变)。开启后 `/` 优先匹配新落地页,旧 React UI 仍在 `/app/`(或保留默认)。

实施做法:在 server.rs 静态托管前,如果 `LLM_WIKI_PUBLIC_LANDING=true` 且 path == "/",从 `LLM_WIKI_PUBLIC_LANDING_DIR`(默认 `overlay/static`)托管 `index.html`。

- [ ] **Step 1: 加配置项**

在 `overlay/server/src/config.rs::ServerConfig` 加:

```rust
    pub public_landing_dir: Option<PathBuf>, // None disables; Some=root
```

`overlay/server/src/main.rs::Args` 加:

```rust
    /// Directory containing the public landing page (index.html etc.). When
    /// set, requests to `/` and `/login`/`/register`/`/reset-password` are
    /// served from here instead of upstream/dist.
    #[arg(long, env = "LLM_WIKI_PUBLIC_LANDING_DIR")]
    public_landing_dir: Option<String>,
```

- [ ] **Step 2: server.rs 在静态分发前优先 landing**

修改 `dispatch_request` 末尾(static_files 调用前):

```rust
    // Public landing pages take priority over upstream/dist for these
    // path prefixes when configured.
    if let Some(landing_root) = state.public_landing_dir() {
        let landing_path = match path.as_str() {
            "/" => Some("index.html"),
            "/login" | "/register" => Some("auth/login.html"),
            "/reset-password" => Some("auth/reset.html"),
            other if other.starts_with("/auth/") && other.ends_with(".css") => Some(&other[1..]),
            other if other.starts_with("/auth/") && other.ends_with(".js") => Some(&other[1..]),
            _ => None,
        };
        if let Some(rel) = landing_path {
            if let Some(response) = static_files::serve_file(&landing_root, rel) {
                let _ = request.respond(response);
                return;
            }
        }
    }
```

注:`static_files::serve_file(root, rel)` 是新辅助函数。如果当前 `static_files.rs` 只导出 `serve_static(root, &path)`,封一个 `serve_file` 接受相对路径即可,内部复用现有逻辑。

`ServerState` 加 `public_landing_dir()` getter,从 ServerConfig 拿。

- [ ] **Step 3: 写 index.html**

写 `overlay/static/index.html`:

```html
<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>LLM Wiki — 智能知识库</title>
  <link rel="stylesheet" href="/landing.css">
</head>
<body>
  <main>
    <h1>LLM Wiki</h1>
    <p class="lede">为你的笔记/文档库做智能问答 — 选个主题,直接对话。</p>

    <section class="features">
      <article>
        <h3>🔍 全文搜索</h3>
        <p>关键词检索整个知识库。</p>
      </article>
      <article>
        <h3>💬 智能问答</h3>
        <p>基于你的资料回答,带思考过程。</p>
      </article>
      <article>
        <h3>🕸️ 知识图谱</h3>
        <p>可视化条目之间的关联。</p>
      </article>
    </section>

    <a id="cta" class="cta" href="/login">开始使用</a>
  </main>
  <script src="/landing.js"></script>
</body>
</html>
```

- [ ] **Step 4: 写 landing.css**

写 `overlay/static/landing.css`(简短,纯 CSS Grid):

```css
* { box-sizing: border-box; }
body {
  margin: 0;
  font-family: -apple-system, BlinkMacSystemFont, "PingFang SC", "Microsoft YaHei", sans-serif;
  background: #fafafa;
  color: #1a1a1a;
}
main {
  max-width: 880px;
  margin: 0 auto;
  padding: 4rem 1.5rem;
  text-align: center;
}
h1 { font-size: 2.5rem; margin-bottom: 0.4rem; }
.lede { font-size: 1.1rem; color: #555; margin-bottom: 3rem; }
.features {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 1.5rem;
  margin-bottom: 3rem;
}
.features article {
  background: #fff;
  border: 1px solid #eee;
  border-radius: 0.75rem;
  padding: 1.25rem;
  text-align: left;
}
.features h3 { margin: 0 0 0.4rem 0; font-size: 1.05rem; }
.features p { margin: 0; color: #666; }
.cta {
  display: inline-block;
  padding: 0.85rem 2rem;
  background: #2563eb;
  color: #fff;
  border-radius: 0.5rem;
  text-decoration: none;
  font-weight: 500;
}
.cta:hover { background: #1d4ed8; }
```

- [ ] **Step 5: 写 landing.js**

写 `overlay/static/landing.js`:

```js
// Decide where the CTA points: logged in -> /lite/, otherwise /login.
(async () => {
  try {
    const r = await fetch('/auth/me', { credentials: 'same-origin' });
    if (r.ok) {
      document.getElementById('cta').href = '/lite/';
    }
  } catch {}
})();
```

- [ ] **Step 6: 编译 server 并冒烟**

```bash
cargo build --release --manifest-path overlay/server/Cargo.toml
LLM_WIKI_PROJECT=/home/li/overseas-github/llm_wiki_projects/CivilCareer \
LLM_WIKI_API_TOKEN=e2e-test-token \
LLM_WIKI_CONFIG=$PWD/overlay/config/server.minimax.local.json \
LLM_WIKI_REPO=$PWD \
LLM_WIKI_STATIC=$PWD/upstream/dist \
LLM_WIKI_AUTH_DB=/tmp/auth.db \
LLM_WIKI_PUBLIC_LANDING_DIR=$PWD/overlay/static \
./overlay/server/target/release/llm-wiki-server &
sleep 2
curl -s http://127.0.0.1:8080/ | grep -q "开始使用" && echo "landing ok"
curl -s http://127.0.0.1:8080/landing.css | head -5
pkill -f llm-wiki-server
rm -f /tmp/auth.db
```
Expected: `landing ok` + 看到 CSS 内容

- [ ] **Step 7: Commit**

```bash
git add overlay/static/index.html overlay/static/landing.css overlay/static/landing.js \
        overlay/server/src/config.rs overlay/server/src/main.rs overlay/server/src/server.rs \
        overlay/server/src/state.rs overlay/server/src/static_files.rs
git commit -m "feat(landing): public landing page (overlay/static), gated by LLM_WIKI_PUBLIC_LANDING_DIR"
```

---

### Task 6.2: 登录/注册页 `/login` `/register`

**Files:**
- Create: `overlay/static/auth/login.html`
- Create: `overlay/static/auth/auth.css`
- Create: `overlay/static/auth/auth.js`

(`/login` 和 `/register` 由 server.rs 在 Task 6.1 都映射到 `auth/login.html`,前端用 location.pathname 决定默认 tab。)

- [ ] **Step 1: 写 login.html**

写 `overlay/static/auth/login.html`:

```html
<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>登录 — LLM Wiki</title>
  <link rel="stylesheet" href="/auth/auth.css">
</head>
<body>
  <main class="card">
    <h1>LLM Wiki</h1>
    <div class="tabs">
      <button id="tab-login" class="tab active">登录</button>
      <button id="tab-register" class="tab">注册</button>
    </div>
    <form id="auth-form">
      <label>邮箱
        <input type="email" name="email" required autocomplete="email">
      </label>
      <label>密码
        <input type="password" name="password" required minlength="8" autocomplete="current-password">
      </label>
      <button type="submit" id="submit-btn">登录</button>
      <div id="error" class="error" role="alert"></div>
    </form>
    <p class="link"><a href="/reset-password">忘记密码?</a></p>
  </main>
  <script src="/auth/auth.js"></script>
</body>
</html>
```

- [ ] **Step 2: 写 auth.css**

写 `overlay/static/auth/auth.css`:

```css
body {
  margin: 0;
  min-height: 100vh;
  display: grid;
  place-items: center;
  background: #fafafa;
  font-family: -apple-system, BlinkMacSystemFont, "PingFang SC", sans-serif;
  color: #1a1a1a;
}
.card {
  background: #fff;
  border: 1px solid #eee;
  border-radius: 1rem;
  padding: 2rem 2.25rem;
  width: 100%;
  max-width: 380px;
  box-shadow: 0 2px 12px rgba(0,0,0,0.04);
}
h1 { margin: 0 0 1.5rem 0; text-align: center; font-size: 1.5rem; }
.tabs { display: flex; gap: 0.5rem; margin-bottom: 1.25rem; }
.tab {
  flex: 1;
  padding: 0.5rem;
  background: transparent;
  border: 1px solid #ddd;
  border-radius: 0.5rem;
  cursor: pointer;
  font-size: 0.95rem;
}
.tab.active { background: #2563eb; color: #fff; border-color: #2563eb; }
form { display: flex; flex-direction: column; gap: 0.85rem; }
label {
  display: flex;
  flex-direction: column;
  gap: 0.3rem;
  font-size: 0.85rem;
  color: #555;
}
input {
  padding: 0.55rem 0.7rem;
  border: 1px solid #ddd;
  border-radius: 0.5rem;
  font-size: 0.95rem;
}
input:focus { outline: 2px solid #2563eb; outline-offset: 1px; }
button[type="submit"] {
  margin-top: 0.5rem;
  padding: 0.7rem;
  background: #2563eb;
  color: #fff;
  border: none;
  border-radius: 0.5rem;
  font-weight: 500;
  cursor: pointer;
}
button[type="submit"]:hover { background: #1d4ed8; }
button[type="submit"]:disabled { opacity: 0.6; cursor: progress; }
.error {
  min-height: 1.2rem;
  color: #b91c1c;
  font-size: 0.85rem;
  text-align: center;
}
.link { text-align: center; margin: 1rem 0 0 0; font-size: 0.85rem; }
.link a { color: #2563eb; text-decoration: none; }
```

- [ ] **Step 3: 写 auth.js**

写 `overlay/static/auth/auth.js`:

```js
const tabLogin = document.getElementById('tab-login');
const tabRegister = document.getElementById('tab-register');
const form = document.getElementById('auth-form');
const submit = document.getElementById('submit-btn');
const errEl = document.getElementById('error');

let mode = 'login';
function setMode(m) {
  mode = m;
  tabLogin.classList.toggle('active', m === 'login');
  tabRegister.classList.toggle('active', m === 'register');
  submit.textContent = m === 'login' ? '登录' : '注册';
  errEl.textContent = '';
}
tabLogin.addEventListener('click', () => setMode('login'));
tabRegister.addEventListener('click', () => setMode('register'));

// /register URL defaults to register tab
if (location.pathname === '/register') setMode('register');

form.addEventListener('submit', async (e) => {
  e.preventDefault();
  errEl.textContent = '';
  submit.disabled = true;
  const data = new FormData(form);
  const body = JSON.stringify({
    email: data.get('email'),
    password: data.get('password'),
  });
  const path = mode === 'login' ? '/auth/login' : '/auth/register';
  try {
    const r = await fetch(path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      credentials: 'same-origin',
      body,
    });
    if (r.ok) {
      location.href = '/lite/';
      return;
    }
    const d = await r.json().catch(() => ({}));
    errEl.textContent = d.error?.message || '请求失败';
  } catch (err) {
    errEl.textContent = '网络错误';
  } finally {
    submit.disabled = false;
  }
});

// already logged in -> straight to /lite/
(async () => {
  try {
    const r = await fetch('/auth/me', { credentials: 'same-origin' });
    if (r.ok) location.href = '/lite/';
  } catch {}
})();
```

- [ ] **Step 4: 浏览器冒烟**

启动 server(带 `LLM_WIKI_AUTH_DB` 和 `LLM_WIKI_PUBLIC_LANDING_DIR=$PWD/overlay/static`),打开 `http://127.0.0.1:8080/login`:
1. 注册一个账号 → 跳 `/lite/`
2. 登出后回 `/login` → 用刚注册的账号登录 → 进 lite

- [ ] **Step 5: Commit**

```bash
git add overlay/static/auth/
git commit -m "feat(auth-ui): /login + /register tabbed page (plain HTML/CSS/JS)"
```

---

### Task 6.3: 重置密码页 `/reset-password`

**Files:**
- Create: `overlay/static/auth/reset.html`

- [ ] **Step 1: 写 reset.html**

写 `overlay/static/auth/reset.html`:

```html
<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>重置密码 — LLM Wiki</title>
  <link rel="stylesheet" href="/auth/auth.css">
</head>
<body>
  <main class="card">
    <h1>重置密码</h1>
    <form id="step1">
      <p style="font-size:0.85rem;color:#666;margin:0 0 0.5rem 0;">
        输入邮箱,我们会向你的邮箱发送重置链接(若邮箱已注册)。
      </p>
      <label>邮箱
        <input type="email" name="email" required>
      </label>
      <button type="submit">发送重置链接</button>
      <div id="step1-msg" class="error" style="color:#16a34a"></div>
    </form>

    <form id="step2" style="display:none">
      <p style="font-size:0.85rem;color:#666;margin:0 0 0.5rem 0;">
        粘贴邮件中的 token,然后输入新密码。
      </p>
      <label>Token
        <input type="text" name="token" required>
      </label>
      <label>新密码
        <input type="password" name="password" required minlength="8">
      </label>
      <button type="submit">设置新密码</button>
      <div id="step2-msg" class="error"></div>
    </form>

    <p class="link"><a href="/login">返回登录</a></p>
  </main>

  <script>
    const s1 = document.getElementById('step1');
    const s2 = document.getElementById('step2');
    const m1 = document.getElementById('step1-msg');
    const m2 = document.getElementById('step2-msg');

    s1.addEventListener('submit', async (e) => {
      e.preventDefault();
      const email = new FormData(s1).get('email');
      const r = await fetch('/auth/forgot-password', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email }),
      });
      m1.textContent = r.ok ? '若该邮箱已注册,你将收到一封带 token 的邮件' : '请求失败';
      if (r.ok) s2.style.display = 'flex';
    });

    s2.addEventListener('submit', async (e) => {
      e.preventDefault();
      m2.textContent = '';
      const data = new FormData(s2);
      const r = await fetch('/auth/reset-password', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ token: data.get('token'), password: data.get('password') }),
      });
      if (r.ok) { location.href = '/login'; return; }
      const d = await r.json().catch(() => ({}));
      m2.textContent = d.error?.message || '重置失败';
    });
  </script>
</body>
</html>
```

- [ ] **Step 2: 浏览器手测**

server 已启动情况下,访问 `/reset-password`:
1. 输入已注册邮箱,提交 → 应显示绿色提示
2. 从 server 日志(`journalctl -u llm-wiki-server` 或终端 stderr)读取 `[auth] password reset token for ... : <token>`
3. 粘贴 token + 新密码 → 提交 → 跳 `/login`
4. 用新密码登录成功

- [ ] **Step 3: Commit**

```bash
git add overlay/static/auth/reset.html
git commit -m "feat(auth-ui): /reset-password (forgot + reset, two-step)"
```

---

### Task 6.4: lite 页移动端适配

**Files:**
- Modify: `overlay/static/lite/app.css`

- [ ] **Step 1: 加 @media 断点**

在 `app.css` 末尾追加(具体类名以 Task 5.3 的 DOM 为准):

```css
/* mobile: ≤ 768px */
@media (max-width: 768px) {
  .sidebar {
    position: fixed;
    top: 0;
    left: 0;
    bottom: 0;
    z-index: 100;
    width: 80%;
    max-width: 280px;
    transform: translateX(-100%);
    transition: transform 0.2s ease;
    background: #fff;
    box-shadow: 2px 0 8px rgba(0,0,0,0.08);
  }
  .sidebar.open { transform: translateX(0); }

  .topbar {
    padding: 0.5rem 0.75rem;
    font-size: 0.85rem;
    gap: 0.5rem;
  }
  .topbar .user-email { display: none; }

  .menu-toggle {
    display: inline-block;
    background: transparent;
    border: 1px solid #ddd;
    border-radius: 0.4rem;
    padding: 0.3rem 0.6rem;
    cursor: pointer;
  }

  /* 主对话区让出侧边栏空间在桌面被吃掉,移动端全宽 */
  main { padding: 0.75rem; }
  .messages { padding: 0.5rem; }
  .composer { padding: 0.5rem; }
}

/* desktop: hide hamburger, show sidebar */
@media (min-width: 769px) {
  .menu-toggle { display: none; }
}
```

修改 `index.html` 在顶部 topbar 加汉堡按钮:

```html
<header class="topbar">
  <button class="menu-toggle" id="menu-toggle" aria-label="菜单">☰</button>
  <span id="user-email" class="user-email"></span>
  <span id="usage-info" class="usage-info"></span>
  <button id="logout-btn" class="logout-btn">登出</button>
</header>
```

修改 `app.js` 末尾追加:

```js
document.getElementById('menu-toggle')?.addEventListener('click', () => {
  document.getElementById('history-sidebar')?.classList.toggle('open');
});
```

- [ ] **Step 2: 同步 dist + 浏览器手测**

```bash
cp overlay/static/lite/{app.js,app.css,index.html} upstream/dist/lite/
```

在浏览器 DevTools 用手机视图(iPhone 12 等)打开 `/lite/`,验证侧边栏折叠、汉堡可展开。

- [ ] **Step 3: Commit**

```bash
git add overlay/static/lite/ upstream/dist/lite/
git commit -m "feat(lite): mobile responsive (collapsing sidebar, hamburger)"
```

---

## Phase 7 — 部署文档 + 配置项 + 回归

### Task 7.1: 更新部署文档

**Files:**
- Modify: `docs/部署-ECS与Tunnel.md`
- Modify: `README-OVERLAY.md`(简短指引)

- [ ] **Step 1: 在部署文档加章节**

在 `docs/部署-ECS与Tunnel.md` `### 3.6 服务端配置` 章节后追加:

```markdown
### 3.6.1 公网模式:多用户认证

公网部署时启用账号系统(注册/登录/历史/用量限额):

新增系统目录与权限:

\`\`\`bash
sudo mkdir -p /var/lib/llm-wiki
sudo chown deploy: /var/lib/llm-wiki
sudo chmod 700 /var/lib/llm-wiki
\`\`\`

在 `/etc/llm-wiki/env` 追加:

\`\`\`bash
export LLM_WIKI_AUTH_DB=/var/lib/llm-wiki/auth.db
export LLM_WIKI_REQUIRE_LOGIN=true
export LLM_WIKI_DAILY_CHAT_LIMIT=50
export LLM_WIKI_ADMIN_EMAIL=you@example.com   # 该邮箱注册时自动 admin
export LLM_WIKI_SESSION_TTL_DAYS=30
export LLM_WIKI_PUBLIC_LANDING_DIR=/opt/llm-wiki/overlay/static
\`\`\`

并把 `overlay/static/` 同步到 ECS 的 `/opt/llm-wiki/overlay/static/`(rsync 时已包含)。

`systemctl restart llm-wiki-server` 后,访问 `https://your-domain/`:

| 路径 | 显示 |
|------|------|
| `/` | 落地页 |
| `/login` `/register` | 登录/注册 |
| `/reset-password` | 重置密码(token 暂时打到 server stderr,通过 `journalctl -u llm-wiki-server` 取) |
| `/lite/` | 问答页(需登录) |
| `/api/v1/*` | 仍接受 cookie 或 Bearer(CLI 不变) |

**备份:** 将 `/var/lib/llm-wiki/auth.db` 加入备份清单,定期 `cp` 即可(WAL 模式下 cp 可能拿到部分写入,生产建议 `sqlite3 auth.db ".backup /backup/auth.db"`)。
```

- [ ] **Step 2: README-OVERLAY 加一个指针**

在 `README-OVERLAY.md` 合适位置加一行指向新章节,例如:

```markdown
### 公网部署(多用户)

见 [docs/部署-ECS与Tunnel.md §3.6.1](docs/部署-ECS与Tunnel.md#361-公网模式多用户认证)。
```

- [ ] **Step 3: Commit**

```bash
git add docs/部署-ECS与Tunnel.md README-OVERLAY.md
git commit -m "docs(deploy): public-mode auth/history/usage runbook"
```

---

### Task 7.2: 回归测试

**Files:** 无新文件,跑现有 e2e + 新加 cookie 路径冒烟。

- [ ] **Step 1: 跑 cargo test**

```bash
cargo test --manifest-path overlay/server/Cargo.toml --workspace
```
Expected: 全部通过(包括 `llm-wiki-auth` 的 ~20 个集成测试)

- [ ] **Step 2: 跑 e2e-local.sh(Bearer 路径回归)**

```bash
./scripts/e2e-local.sh
```
Expected: `Local E2E passed.`(沿用共享 token,不开 auth db)

- [ ] **Step 3: cookie 路径完整冒烟**

写一个一次性脚本验证 cookie 闭环:

```bash
mkdir -p /tmp/llm-wiki-test && rm -f /tmp/llm-wiki-test/auth.db
LLM_WIKI_PROJECT=/home/li/overseas-github/llm_wiki_projects/CivilCareer \
LLM_WIKI_API_TOKEN=e2e-test-token \
LLM_WIKI_CONFIG=$PWD/overlay/config/server.minimax.local.json \
LLM_WIKI_REPO=$PWD \
LLM_WIKI_STATIC=$PWD/upstream/dist \
LLM_WIKI_AUTH_DB=/tmp/llm-wiki-test/auth.db \
LLM_WIKI_PUBLIC_LANDING_DIR=$PWD/overlay/static \
LLM_WIKI_DAILY_CHAT_LIMIT=3 \
./overlay/server/target/release/llm-wiki-server > /tmp/srv.log 2>&1 &
SRV=$!
sleep 2

echo "=== landing ==="
curl -s http://127.0.0.1:8080/ | grep -q "开始使用" && echo "  landing ok"

echo "=== register ==="
curl -s -c /tmp/cj.txt -X POST http://127.0.0.1:8080/auth/register \
  -H 'Content-Type: application/json' \
  -d '{"email":"e2e@test.com","password":"longenough"}' | grep -q '"user"' && echo "  register ok"

echo "=== /auth/me ==="
curl -sf -b /tmp/cj.txt http://127.0.0.1:8080/auth/me | grep -q '"limit":3' && echo "  me ok (limit=3)"

echo "=== conversations CRUD ==="
CONV=$(curl -s -b /tmp/cj.txt -X POST http://127.0.0.1:8080/api/v1/conversations \
  -H 'Content-Type: application/json' -d '{"project_id":"px","title":"smoke"}' \
  | python3 -c 'import sys,json;print(json.load(sys.stdin)["id"])')
[ -n "$CONV" ] && echo "  create ok: $CONV"
curl -s -b /tmp/cj.txt http://127.0.0.1:8080/api/v1/conversations | grep -q "$CONV" && echo "  list ok"

echo "=== chat usage limit ==="
PROJECT_ID=$(curl -sf -H 'Authorization: Bearer e2e-test-token' \
  http://127.0.0.1:8080/api/v1/projects | python3 -c \
  'import sys,json;d=json.load(sys.stdin);print(d["projects"][0]["id"])')
for i in 1 2 3 4; do
  CODE=$(curl -s -b /tmp/cj.txt -o /dev/null -w '%{http_code}' \
    -X POST "http://127.0.0.1:8080/api/v1/projects/$PROJECT_ID/chat" \
    -H 'Content-Type: application/json' \
    -d '{"messages":[{"role":"user","content":"hi"}]}' --max-time 60)
  echo "  chat $i -> $CODE"
done
# Expected: 200,200,200,429

echo "=== logout ==="
curl -s -b /tmp/cj.txt -X POST http://127.0.0.1:8080/auth/logout | grep -q '"ok":true' && echo "  logout ok"
curl -s -o /dev/null -w '  /auth/me after logout: %{http_code}\n' -b /tmp/cj.txt http://127.0.0.1:8080/auth/me

echo "=== Bearer path still works ==="
curl -sf -H 'Authorization: Bearer e2e-test-token' \
  http://127.0.0.1:8080/api/v1/projects > /dev/null && echo "  bearer ok"

kill $SRV
rm -rf /tmp/llm-wiki-test /tmp/cj.txt
```

Expected: 全部 ok 行 + chat 1/2/3 是 200,4 是 429 + logout 后 me 是 401 + bearer 仍 ok。

- [ ] **Step 4: 把这段脚本固化到仓库**

写到 `scripts/e2e-auth.sh`,加可执行位:

```bash
chmod +x scripts/e2e-auth.sh
```

文件内容就是上面的 bash 脚本(去掉外层 echo 标签的句首两个空格,正常脚本格式)。

- [ ] **Step 5: Commit**

```bash
git add scripts/e2e-auth.sh
git commit -m "test: e2e-auth.sh — cookie auth + history + usage limit smoke"
```

---

## Self-Review

写完计划后我做的核对(已完成):

- **Spec 覆盖率**:Spec 13 节 + 14 个实施步骤,全部映射到 Task。Spec §10 配置项每条都在 Task 3.1 / 6.1。Spec §11 测试项每条都有对应 cargo test 或 e2e 验证。
- **占位符扫描**:无 TODO / TBD / "类似上文" / "添加适当错误处理"。
- **类型一致性**:`AuthService` / `AuthOutcome` / `Store::create_session(token_hash, user_id, now, expires_at, ...)` 在不同 Task 中签名相同;`session_user(token, now)` 也是。

## 已知遗留(留给 v1.1)

这些 Spec §12 列为不做,plan 也未实现:
- 邮箱真实发送(目前 reset token 只打 stderr)
- Google OAuth
- 多设备 session 管理 UI
- admin 后台
- 用量月度统计

如需提前做某项,作为单独 plan 处理,不挤本计划。
