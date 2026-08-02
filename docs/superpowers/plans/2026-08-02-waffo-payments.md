# Waffo Pancake 付款实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 打通 DocuChat 付款：用户自助订阅 Pro（$19/月），Waffo 托管 checkout 收款，webhook 自动升级/降级聊天配额权益。

**Architecture:** 纯 Rust。在 auth crate 增加套餐列（`plan`/`waffo_order_id`/`pro_since`/`plan_period_end`）与 webhook 幂等表；在 server 新增 `api/billing.rs` 模块，用 `rsa`+`sha2`+`base64` 手写 API 签名与 webhook 验签；`chat.rs` 与 `/auth/me` 的日配额改为按用户套餐解析。

**Tech Stack:** Rust (tiny_http, reqwest, rusqlite, rsa, sha2, base64)，Waffo Pancake API（https://api.waffo.ai），静态 HTML（pricing/landing）。

**Spec:** `docs/superpowers/specs/2026-08-02-waffo-payments-design.md`

---

## 文件结构总览

| 文件 | 责任 |
|------|------|
| `overlay/auth/src/schema.rs` | 幂等迁移：users 加 4 列 + 新建 `waffo_webhook_events` 表 |
| `overlay/auth/src/store.rs` | User 结构加 plan 字段；新增 get_plan/set_plan/find_user_by_order_id/webhook 幂等方法 |
| `overlay/auth/src/service.rs` | 无改动（store 方法直达） |
| `overlay/server/Cargo.toml` | 加 `rsa`/`sha2`/`base64` 依赖；dev 加 `tempfile` |
| `overlay/server/src/api/billing.rs` | **新模块**：签名/验签/配置解析/权益状态机/webhook 处理/checkout 处理 |
| `overlay/server/src/api/mod.rs` | `pub(crate) mod billing;` |
| `overlay/server/src/server.rs` | dispatch 增加 `/api/v1/billing/*` 分支（绕过通用鉴权） |
| `overlay/server/src/api/chat.rs` | 配额改为 `billing::resolve_daily_limit` |
| `overlay/server/src/api/auth_routes.rs` | `/auth/me` 加 `plan`/`planPeriodEnd`，usage.limit 按用户解析 |
| `overlay/config/server.example.json` | 加 `billing` 配置块示例 |
| `overlay/static/pricing/index.html` | 「立即订阅」按钮接 checkout JS |
| `overlay/static/index.html` | 内嵌定价区「立即订阅」同样接线 |
| `scripts/e2e-billing.sh` | 新脚本：checkout→(手动付卡)→webhook→plan 校验 |
| `docs/付款-Waffo-Pancake.md` | 新文档：Dashboard 准备、配置、部署、排错 |

---

## Task 1: Auth store — 套餐列 + webhook 幂等表

**Files:**
- Modify: `overlay/auth/src/schema.rs`
- Modify: `overlay/auth/src/store.rs`
- Test: `overlay/auth/src/store.rs` (`#[cfg(test)]` module)

- [ ] **Step 1: 写失败的测试** — 在 `store.rs` 测试模块（文件底部 `#[cfg(test)]`）加以下测试：

```rust
#[test]
fn plan_defaults_to_free() {
    let store = test_store();
    let uid = store.create_user(NewUser {
        email: "p@example.com", password_hash: "h", display_name: None, is_admin: false, now: 1000,
    }).unwrap();
    assert_eq!(store.get_plan(uid).unwrap(), "free");
    let (plan, period_end) = store.get_plan_info(uid).unwrap();
    assert_eq!(plan, "free");
    assert_eq!(period_end, None);
}

#[test]
fn set_plan_roundtrip() {
    let store = test_store();
    let uid = store.create_user(NewUser {
        email: "p2@example.com", password_hash: "h", display_name: None, is_admin: false, now: 1000,
    }).unwrap();
    store.set_plan(uid, "pro", Some("ORD_abc"), Some(1_752_000_000), 2000).unwrap();
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
    let store = test_store();
    let uid = store.create_user(NewUser {
        email: "p3@example.com", password_hash: "h", display_name: None, is_admin: false, now: 1000,
    }).unwrap();
    store.set_plan(uid, "pro", Some("ORD_xyz"), None, 2000).unwrap();
    let u = store.find_user_by_order_id("ORD_xyz").unwrap().expect("found");
    assert_eq!(u.id, uid);
    assert_eq!(u.plan, "pro");
    assert!(store.find_user_by_order_id("ORD_nope").unwrap().is_none());
}

#[test]
fn webhook_event_dedup() {
    let store = test_store();
    assert!(!store.has_webhook_event("evt_1").unwrap());
    store.record_webhook_event("evt_1", "subscription.activated", 1000).unwrap();
    assert!(store.has_webhook_event("evt_1").unwrap());
    store.record_webhook_event("evt_1", "subscription.activated", 1000).unwrap(); // idempotent
    assert!(store.has_webhook_event("evt_1").unwrap());
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path overlay/auth/Cargo.toml plan_defaults_to_free set_plan_roundtrip find_user_by_order_id webhook_event_dedup`
Expected: FAIL — `get_plan` / `set_plan` / `find_user_by_order_id` / `has_webhook_event` 不存在（编译错误）。

- [ ] **Step 3: schema.rs 迁移** — 在 `SCHEMA_SQL` 末尾追加表，并在 `init_schema()` 加幂等 ALTER：

```rust
// SCHEMA_SQL 末尾（现有 `pending_email_changes` 表之后）追加：
CREATE TABLE IF NOT EXISTS waffo_webhook_events (
  event_id   TEXT PRIMARY KEY,
  event_type TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
```

```rust
// init_schema() 内，email_verified_at 迁移之后追加：
// Billing/subscription plan columns (idempotent ALTER, same pattern as above).
for ddl in [
    "ALTER TABLE users ADD COLUMN plan TEXT NOT NULL DEFAULT 'free'",
    "ALTER TABLE users ADD COLUMN waffo_order_id TEXT",
    "ALTER TABLE users ADD COLUMN pro_since INTEGER",
    "ALTER TABLE users ADD COLUMN plan_period_end INTEGER",
] {
    match conn.execute_batch(ddl) {
        Ok(_) => {}
        Err(e) if e.to_string().contains("duplicate column name") => {}
        Err(e) => return Err(e),
    }
}
```

- [ ] **Step 4: store.rs — User 结构 + SELECT + row_to_user** — 给 `User` 加两个字段，并同步两个 SELECT 与 `row_to_user`：

```rust
// User struct 追加字段：
pub plan: String,
pub plan_period_end: Option<i64>,
```

`find_user_by_email` 与 `find_user_by_id` 的 SELECT 从 8 列改为 10 列：

```rust
"SELECT id, email, password_hash, display_name, is_admin, created_at, last_seen_at,
        email_verified_at, plan, plan_period_end
 FROM users WHERE email = ?1",   // 以及 find_user_by_id 的 WHERE id = ?1
```

`row_to_user` 改为：

```rust
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
```

- [ ] **Step 5: store.rs — 新方法** — 在 `// --- users ---` 区段末尾（`find_user_by_id` 之后）追加：

```rust
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

pub fn record_webhook_event(&self, event_id: &str, event_type: &str, now: i64) -> Result<(), AuthError> {
    let conn = self.lock();
    conn.execute(
        "INSERT OR IGNORE INTO waffo_webhook_events (event_id, event_type, created_at)
         VALUES (?1, ?2, ?3)",
        params![event_id, event_type, now],
    )?;
    Ok(())
}
```

> `test_store()` helper 已存在于 `store.rs` 测试模块（`NamedTempFile` + `Store::open`）。若 `get_plan_info` 的 tuple 与既有测试断言冲突，先跑全量测试修正。

- [ ] **Step 6: 运行测试确认通过**

Run: `cargo test --manifest-path overlay/auth/Cargo.toml`
Expected: 全部 PASS（含既有测试——注意确认无其他构造 `User` 的代码需要补字段）。

- [ ] **Step 7: Commit**

```bash
git add overlay/auth/src/schema.rs overlay/auth/src/store.rs
git commit -m "feat(auth): plan columns + waffo_webhook_events dedup table + store methods"
```

---

## Task 2: Server 依赖 + 加密原语（签名/验签/日期）

**Files:**
- Modify: `overlay/server/Cargo.toml`
- Create: `overlay/server/src/api/billing.rs`
- Test: `overlay/server/src/api/billing.rs`

- [ ] **Step 1: Cargo.toml 加依赖**

```toml
# [dependencies] 区追加：
rsa = { version = "0.9", features = ["sha2", "pkcs1", "pkcs8"] }
sha2 = "0.10"
base64 = "0.22"

# [dev-dependencies] 区（新建）：
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: 声明模块 + 只写测试（不写实现）** — 创建 `overlay/server/src/api/billing.rs`：

```rust
//! Waffo Pancake billing: RSA-SHA256 request signing, webhook verification,
//! subscription entitlement. Pure-Rust (no Node shim).
//! Reference: https://docs.waffo.ai/llms-full.txt

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rsa::pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey};
use rsa::pkcs8::DecodePrivateKey;
use rsa::{Pkcs1v15Sign, RsaPrivateKey, RsaPublicKey};
use sha2::{Digest, Sha256};

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::rand_core::OsRng;

    fn keypair() -> (RsaPrivateKey, RsaPublicKey) {
        let privk = RsaPrivateKey::new(&mut OsRng, 2048).unwrap();
        let pubk = RsaPublicKey::from(&privk);
        (privk, pubk)
    }

    fn priv_pem(k: &RsaPrivateKey) -> String {
        rsa::pkcs8::EncodePrivateKey::to_pkcs8_pem(k).unwrap().to_string()
    }

    fn pub_pem(k: &RsaPublicKey) -> String {
        rsa::pkcs8::EncodePublicKey::to_public_key_pem(k).unwrap().to_string()
    }

    #[test]
    fn sign_headers_are_verifiable() {
        let (privk, pubk) = keypair();
        let headers = sign_headers("POST", "/v1/actions/checkout/create-session", b"{}", "MER_1", &priv_pem(&privk)).unwrap();
        let ts: i64 = headers.iter().find(|(k, _)| k == "X-Timestamp").unwrap().1.parse().unwrap();
        let sig = B64.decode(headers.iter().find(|(k, _)| k == "X-Signature").unwrap().1).unwrap();
        let canonical = format!("POST\n/v1/actions/checkout/create-session\n{ts}\n{}", B64.encode(Sha256::digest(b"{}")));
        pubk.verify(Pkcs1v15Sign::new::<Sha256>(), canonical.as_bytes(), &sig).unwrap();
    }

    #[test]
    fn verify_webhook_valid() {
        let (privk, pubk) = keypair();
        let body = r#"{"id":"evt_1","eventType":"subscription.activated","data":{}}"#;
        let ts = 1_700_000_000i64;
        let signed = format!("{ts}.{body}");
        let sig = privk.sign(Pkcs1v15Sign::new::<Sha256>(), signed.as_bytes()).unwrap();
        let header = format!("t={ts},v1={}", B64.encode(sig));
        verify_webhook_signature(body, &header, &pub_pem(&pubk), ts).unwrap();
    }

    #[test]
    fn verify_webhook_rejects_tampered_body() {
        let (privk, pubk) = keypair();
        let body = r#"{"id":"evt_1","eventType":"subscription.activated","data":{}}"#;
        let ts = 1_700_000_000i64;
        let signed = format!("{ts}.{body}");
        let sig = privk.sign(Pkcs1v15Sign::new::<Sha256>(), signed.as_bytes()).unwrap();
        let header = format!("t={ts},v1={}", B64.encode(sig));
        assert!(verify_webhook_signature("tampered", &header, &pub_pem(&pubk), ts).is_err());
    }

    #[test]
    fn verify_webhook_rejects_stale_timestamp() {
        let (privk, pubk) = keypair();
        let body = "{}";
        let ts = 1_700_000_000i64;
        let signed = format!("{ts}.{body}");
        let sig = privk.sign(Pkcs1v15Sign::new::<Sha256>(), signed.as_bytes()).unwrap();
        let header = format!("t={ts},v1={}", B64.encode(sig));
        // now is 10 minutes ahead of ts
        assert!(verify_webhook_signature(body, &header, &pub_pem(&pubk), ts + 600).is_err());
    }

    #[test]
    fn iso_date_to_epoch_known_values() {
        assert_eq!(iso_date_to_epoch("1970-01-01"), Some(0));
        assert_eq!(iso_date_to_epoch("1970-01-02"), Some(86_400));
        assert_eq!(iso_date_to_epoch("2026-03-10"), Some(1_772_352_000));
        assert_eq!(iso_date_to_epoch("2024-02-29"), Some(1_709_164_800)); // leap year
        assert_eq!(iso_date_to_epoch("2026-13-01"), None);
        assert_eq!(iso_date_to_epoch("garbage"), None);
    }
}
```

在 `overlay/server/src/api/mod.rs` 顶部加 `pub(crate) mod billing;`（放在 `mod runtime;` 之后）。

- [ ] **Step 3: 运行测试确认失败**

Run: `cargo test --manifest-path overlay/server/Cargo.toml billing`
Expected: FAIL — 编译错误：`sign_headers` / `verify_webhook_signature` / `iso_date_to_epoch` 未定义。

- [ ] **Step 4: 实现** — 在 `billing.rs` 测试模块之前（`use sha2...` 之后）插入：

```rust
pub fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub fn sign_headers(
    method: &str,
    path: &str,
    body: &[u8],
    merchant_id: &str,
    private_key_pem: &str,
) -> Result<Vec<(String, String)>, String> {
    let ts = now_secs();
    let body_hash = B64.encode(Sha256::digest(body));
    let canonical = format!("{method}\n{path}\n{ts}\n{body_hash}");
    let key = RsaPrivateKey::from_pkcs8_pem(private_key_pem)
        .or_else(|_| RsaPrivateKey::from_pkcs1_pem(private_key_pem))
        .map_err(|e| format!("invalid Waffo private key: {e}"))?;
    let sig = key
        .sign(Pkcs1v15Sign::new::<Sha256>(), canonical.as_bytes())
        .map_err(|e| format!("sign failed: {e}"))?;
    Ok(vec![
        ("X-Merchant-Id".to_string(), merchant_id.to_string()),
        ("X-Timestamp".to_string(), ts.to_string()),
        ("X-Signature".to_string(), B64.encode(sig)),
    ])
}

pub fn verify_webhook_signature(
    raw_body: &str,
    sig_header: &str,
    public_key_pem: &str,
    now: i64,
) -> Result<(), String> {
    let mut ts: Option<i64> = None;
    let mut sig_b64: Option<&str> = None;
    for part in sig_header.split(',') {
        if let Some(v) = part.strip_prefix("t=") {
            ts = v.parse().ok();
        } else if let Some(v) = part.strip_prefix("v1=") {
            sig_b64 = Some(v);
        }
    }
    let ts = ts.ok_or("missing t=")?;
    let sig_b64 = sig_b64.ok_or("missing v1=")?;
    if (now - ts).abs() > 300 {
        return Err(format!("webhook timestamp outside tolerance (now={now} ts={ts})"));
    }
    let signed = format!("{ts}.{raw_body}");
    let sig = B64.decode(sig_b64).map_err(|_| "bad base64 signature".to_string())?;
    let key = RsaPublicKey::from_public_key_pem(public_key_pem)
        .or_else(|_| DecodeRsaPublicKey::from_pkcs1_pem(public_key_pem))
        .map_err(|e| format!("invalid webhook public key: {e}"))?;
    key.verify(Pkcs1v15Sign::new::<Sha256>(), signed.as_bytes(), &sig)
        .map_err(|_| "webhook signature verification failed".to_string())
}

/// "2026-03-10" (ISO date) → unix seconds at 00:00 UTC (Howard Hinnant algorithm).
pub fn iso_date_to_epoch(s: &str) -> Option<i64> {
    let mut it = s.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some(days_from_civil(y, m, d) * 86_400)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}
```

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test --manifest-path overlay/server/Cargo.toml billing`
Expected: 5 个 crypto/date 测试全部 PASS。

- [ ] **Step 6: Commit**

```bash
git add overlay/server/Cargo.toml overlay/server/src/api/mod.rs overlay/server/src/api/billing.rs
git commit -m "feat(billing): RSA-SHA256 signing + webhook verification + iso date parsing"
```

---

## Task 3: Billing 配置解析 + 按套餐解析日配额

**Files:**
- Modify: `overlay/server/src/api/billing.rs`

- [ ] **Step 1: 写失败的测试** — 在 `billing.rs` 测试模块追加：

```rust
use serde_json::json;

fn billing_json() -> serde_json::Value {
    json!({
        "billing": {
            "waffoMerchantId": "MER_1",
            "waffoPrivateKey": "-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----",
            "proProductId": "PROD_1",
            "webhookPublicKey": "-----BEGIN PUBLIC KEY-----\ndef\n-----END PUBLIC KEY-----",
            "environment": "test",
            "freeTierDailyLimit": 3,
            "proTierDailyLimit": 10000,
            "checkoutSuccessUrl": "https://www.sship.online/pricing?upgraded=1",
            "language": "zh-Hans"
        }
    })
}

#[test]
fn parse_billing_config_happy_path() {
    let cfg = parse_billing_config(&billing_json()).expect("parsed");
    assert_eq!(cfg.merchant_id, "MER_1");
    assert_eq!(cfg.pro_product_id, "PROD_1");
    assert_eq!(cfg.environment, "test");
    assert_eq!(cfg.free_tier_daily_limit, 3);
    assert_eq!(cfg.pro_tier_daily_limit, 10000);
    assert_eq!(cfg.language.as_deref(), Some("zh-Hans"));
}

#[test]
fn parse_billing_config_disabled_returns_none() {
    let mut v = billing_json();
    v["billing"]["enabled"] = json!(false);
    assert!(parse_billing_config(&v).is_none());
}

#[test]
fn parse_billing_config_missing_block_returns_none() {
    assert!(parse_billing_config(&json!({"other": 1})).is_none());
}

#[test]
fn resolve_daily_limit_per_plan() {
    let app = billing_json();
    assert_eq!(resolve_daily_limit(Some(&app), "free", 50), 3);
    assert_eq!(resolve_daily_limit(Some(&app), "pro", 50), 10000);
}

#[test]
fn resolve_daily_limit_no_billing_uses_global() {
    assert_eq!(resolve_daily_limit(Some(&json!({"other": 1})), "free", 50), 50);
    assert_eq!(resolve_daily_limit(None, "pro", 50), 50);
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path overlay/server/Cargo.toml billing::tests::parse_billing_config billing::tests::resolve_daily_limit`
Expected: FAIL — `parse_billing_config` / `resolve_daily_limit` 未定义。

- [ ] **Step 3: 实现** — 在 `billing.rs` 的 `iso_date_to_epoch` 之后追加：

```rust
#[derive(Debug, Clone)]
pub struct BillingConfig {
    pub merchant_id: String,
    pub private_key_pem: String,
    pub pro_product_id: String,
    pub webhook_public_key_pem: String,
    pub environment: String,
    pub free_tier_daily_limit: u32,
    pub pro_tier_daily_limit: u32,
    pub checkout_success_url: String,
    pub language: Option<String>,
}

pub fn parse_billing_config(app_state: &serde_json::Value) -> Option<BillingConfig> {
    let b = app_state.get("billing")?;
    if b.get("enabled").and_then(serde_json::Value::as_bool) == Some(false) {
        return None;
    }
    Some(BillingConfig {
        merchant_id: b.get("waffoMerchantId")?.as_str()?.trim().to_string(),
        private_key_pem: b.get("waffoPrivateKey")?.as_str()?.trim().to_string(),
        pro_product_id: b.get("proProductId")?.as_str()?.trim().to_string(),
        webhook_public_key_pem: b.get("webhookPublicKey")?.as_str()?.trim().to_string(),
        environment: b.get("environment").and_then(serde_json::Value::as_str).unwrap_or("test").to_string(),
        free_tier_daily_limit: b.get("freeTierDailyLimit").and_then(serde_json::Value::as_u64).map(|v| v as u32).unwrap_or(3),
        pro_tier_daily_limit: b.get("proTierDailyLimit").and_then(serde_json::Value::as_u64).map(|v| v as u32).unwrap_or(10_000),
        checkout_success_url: b.get("checkoutSuccessUrl").and_then(serde_json::Value::as_str).unwrap_or("").to_string(),
        language: b.get("language").and_then(serde_json::Value::as_str).map(ToOwned::to_owned),
    })
}

/// Per-user daily chat limit: pro → proTierDailyLimit, free → freeTierDailyLimit.
/// No billing block / billing disabled → falls back to the global limit.
pub fn resolve_daily_limit(app_state: Option<&serde_json::Value>, plan: &str, global_default: u32) -> i64 {
    let Some(cfg) = app_state.and_then(parse_billing_config) else {
        return global_default as i64;
    };
    if plan == "pro" {
        cfg.pro_tier_daily_limit as i64
    } else {
        cfg.free_tier_daily_limit as i64
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test --manifest-path overlay/server/Cargo.toml billing::tests::parse_billing_config billing::tests::resolve_daily_limit`
Expected: 全部 PASS。

- [ ] **Step 5: Commit**

```bash
git add overlay/server/src/api/billing.rs
git commit -m "feat(billing): config parsing + per-plan daily limit resolution"
```

---

## Task 4: 权益状态机 + webhook 事件落库

**Files:**
- Modify: `overlay/server/src/api/billing.rs`

- [ ] **Step 1: 写失败的测试** — 追加：

```rust
use llm_wiki_auth::store::Store;

fn tmp_store() -> Store {
    let f = tempfile::NamedTempFile::new().unwrap();
    Store::open(f.path()).unwrap()
}

fn mk_data(oid: &str, period: Option<&str>, user_id: i64, email: &str) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    m.insert("orderId".into(), serde_json::Value::String(oid.into()));
    m.insert("buyerEmail".into(), serde_json::Value::String(email.into()));
    if let Some(p) = period {
        m.insert("currentPeriodEnd".into(), serde_json::Value::String(p.into()));
    }
    m.insert(
        "orderMetadata".into(),
        serde_json::json!({ "userId": user_id.to_string() }),
    );
    serde_json::Value::Object(m)
}

#[test]
fn apply_event_state_machine() {
    let d = mk_data("ORD_1", Some("2026-04-10"), 1, "a@b.com");
    assert_eq!(
        apply_event("subscription.activated", &d),
        PlanAction::GrantPro { order_id: "ORD_1".into(), period_end: iso_date_to_epoch("2026-04-10") }
    );
    assert_eq!(apply_event("subscription.canceling", &d), PlanAction::KeepPro);
    assert_eq!(apply_event("subscription.past_due", &d), PlanAction::KeepPro);
    assert_eq!(apply_event("subscription.canceled", &d), PlanAction::DowngradeToFree);
    assert_eq!(apply_event("order.completed", &d), PlanAction::Noop);
    assert_eq!(apply_event("refund.succeeded", &d), PlanAction::DowngradeToFree);
}

#[test]
fn process_webhook_activated_grants_pro() {
    let store = tmp_store();
    let uid = store
        .create_user(llm_wiki_auth::store::NewUser {
            email: "a@b.com", password_hash: "h", display_name: None, is_admin: false, now: 1000,
        })
        .unwrap();
    let data = mk_data("ORD_9", Some("2026-04-10"), uid, "a@b.com");
    process_webhook_event(&store, "subscription.activated", &data).unwrap();
    let (plan, period_end) = store.get_plan_info(uid).unwrap();
    assert_eq!(plan, "pro");
    assert_eq!(period_end, iso_date_to_epoch("2026-04-10"));
}

#[test]
fn process_webhook_canceled_downgrades() {
    let store = tmp_store();
    let uid = store
        .create_user(llm_wiki_auth::store::NewUser {
            email: "c@d.com", password_hash: "h", display_name: None, is_admin: false, now: 1000,
        })
        .unwrap();
    store.set_plan(uid, "pro", Some("ORD_7"), Some(1_752_000_000), 2000).unwrap();
    let data = mk_data("ORD_7", None, uid, "c@d.com");
    process_webhook_event(&store, "subscription.canceled", &data).unwrap();
    let (plan, _) = store.get_plan_info(uid).unwrap();
    assert_eq!(plan, "free");
}

#[test]
fn process_webhook_maps_by_order_id() {
    let store = tmp_store();
    let uid = store
        .create_user(llm_wiki_auth::store::NewUser {
            email: "e@f.com", password_hash: "h", display_name: None, is_admin: false, now: 1000,
        })
        .unwrap();
    store.set_plan(uid, "pro", Some("ORD_42"), None, 2000).unwrap();
    // No orderMetadata.userId; only orderId — must still map via waffo_order_id.
    let data = serde_json::json!({
        "orderId": "ORD_42",
        "buyerEmail": "e@f.com",
        "currentPeriodEnd": "2026-05-01",
    });
    process_webhook_event(&store, "subscription.payment_succeeded", &data).unwrap();
    let (plan, period_end) = store.get_plan_info(uid).unwrap();
    assert_eq!(plan, "pro");
    assert_eq!(period_end, iso_date_to_epoch("2026-05-01"));
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path overlay/server/Cargo.toml billing::tests::apply_event billing::tests::process_webhook`
Expected: FAIL — `PlanAction` / `apply_event` / `process_webhook_event` 未定义。

- [ ] **Step 3: 实现** — 在 `billing.rs` 的 `resolve_daily_limit` 之后追加：

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanAction {
    GrantPro { order_id: String, period_end: Option<i64> },
    KeepPro,
    DowngradeToFree,
    Noop,
}

pub fn apply_event(event_type: &str, data: &serde_json::Value) -> PlanAction {
    let order_id = data.get("orderId").and_then(serde_json::Value::as_str).unwrap_or("").to_string();
    let period_end = data
        .get("currentPeriodEnd")
        .and_then(serde_json::Value::as_str)
        .and_then(iso_date_to_epoch);
    match event_type {
        "subscription.activated" | "subscription.uncanceled" | "subscription.payment_succeeded" => {
            PlanAction::GrantPro { order_id, period_end }
        }
        "subscription.canceling" | "subscription.past_due" => PlanAction::KeepPro,
        "subscription.canceled" | "refund.succeeded" => PlanAction::DowngradeToFree,
        _ => PlanAction::Noop,
    }
}

fn resolve_user_id(store: &llm_wiki_auth::Store, data: &serde_json::Value) -> Option<i64> {
    // 1) orderMetadata.userId (set at checkout, string or number)
    if let Some(s) = data.get("orderMetadata").and_then(|m| m.get("userId")) {
        if let Some(id) = s.as_i64() {
            return Some(id);
        }
        if let Some(Ok(id)) = s.as_str().map(|x| x.parse::<i64>()) {
            return Some(id);
        }
    }
    // 2) orderId → users.waffo_order_id
    if let Some(oid) = data.get("orderId").and_then(serde_json::Value::as_str) {
        if let Ok(Some(u)) = store.find_user_by_order_id(oid) {
            return Some(u.id);
        }
    }
    // 3) buyerEmail (normalized lowercase in DB)
    if let Some(email) = data.get("buyerEmail").and_then(serde_json::Value::as_str) {
        if let Ok(Some(u)) = store.find_user_by_email(email.trim().to_lowercase().as_str()) {
            return Some(u.id);
        }
    }
    None
}

pub fn process_webhook_event(
    store: &llm_wiki_auth::Store,
    event_type: &str,
    data: &serde_json::Value,
) -> Result<(), String> {
    let now = now_secs();
    match apply_event(event_type, data) {
        PlanAction::Noop | PlanAction::KeepPro => Ok(()),
        PlanAction::GrantPro { order_id, period_end } => {
            let uid = resolve_user_id(store, data).ok_or("cannot map webhook to a user")?;
            store
                .set_plan(uid, "pro", Some(&order_id), period_end, now)
                .map_err(|e| e.to_string())
        }
        PlanAction::DowngradeToFree => {
            if let Some(uid) = resolve_user_id(store, data) {
                store.set_plan(uid, "free", None, None, now).map_err(|e| e.to_string())?;
            }
            Ok(())
        }
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test --manifest-path overlay/server/Cargo.toml billing::tests::apply_event billing::tests::process_webhook`
Expected: 全部 PASS。

- [ ] **Step 5: Commit**

```bash
git add overlay/server/src/api/billing.rs
git commit -m "feat(billing): entitlement state machine + webhook event persistence"
```

---

## Task 5: HTTP 路由 — checkout + webhook + dispatch 接线

**Files:**
- Modify: `overlay/server/src/api/billing.rs`
- Modify: `overlay/server/src/server.rs`

> 路由逻辑薄、难以单测（需要真实 `tiny_http::Request`）。用可单测的 `build_checkout_request_body` 覆盖核心；HTTP 层靠 Step 4 的手工 curl 验证。

- [ ] **Step 1: 写失败的测试（可单测部分）**

```rust
#[test]
fn build_checkout_request_body_shape() {
    let app = billing_json();
    let cfg = parse_billing_config(&app).unwrap();
    let body = build_checkout_request_body(&cfg, 42, "u@x.com", 1_700_000_000);
    assert_eq!(body["productId"], "PROD_1");
    assert_eq!(body["productType"], "subscription");
    assert_eq!(body["currency"], "USD");
    assert_eq!(body["buyerEmail"], "u@x.com");
    assert_eq!(body["successUrl"], "https://www.sship.online/pricing?upgraded=1");
    assert_eq!(body["metadata"]["userId"], "42");
    assert_eq!(body["language"], "zh-Hans");
    assert!(body["orderMerchantExternalId"].as_str().unwrap().starts_with("dllm-42-"));
}
```

- [ ] **Step 2: 实现 `build_checkout_request_body` 与 HTTP 处理器** — 在 `billing.rs` 追加：

```rust
use tiny_http::{Method, Request, StatusCode};
use crate::api::{self, AuthOutcome};
use crate::state::ServerState;
use serde_json::{json, Value};

pub fn build_checkout_request_body(cfg: &BillingConfig, user_id: i64, email: &str, now: i64) -> Value {
    let mut body = json!({
        "productId": cfg.pro_product_id,
        "productType": "subscription",
        "currency": "USD",
        "buyerEmail": email,
        "successUrl": cfg.checkout_success_url,
        "metadata": { "userId": user_id.to_string() },
        "orderMerchantExternalId": format!("dllm-{user_id}-{now}"),
    });
    if let Some(lang) = &cfg.language {
        body["language"] = Value::String(lang.clone());
    }
    body
}

/// Entry point from server.rs dispatch for `/api/v1/billing/*`.
pub fn handle(
    state: &ServerState,
    method: &Method,
    parts: &[&str],
    body: &str,
    headers: &[(String, String)],
    request: Request,
) {
    match (method, parts) {
        (&Method::Post, ["billing", "checkout"]) => handle_checkout(state, body, headers, request),
        (&Method::Post, ["billing", "webhook"]) => handle_webhook(state, body, headers, request),
        _ => api::respond_json(
            request,
            404,
            json!({ "error": { "code": "not_found", "message": "Not found" } }),
        ),
    }
}

fn handle_checkout(
    state: &ServerState,
    body: &str,
    headers: &[(String, String)],
    request: Request,
) {
    let Some(auth) = state.auth() else {
        api::respond_json(request, 503, json!({"ok": false, "error": "auth disabled"}));
        return;
    };
    let Some(cfg) = state.load_app_state().as_ref().and_then(parse_billing_config) else {
        api::respond_json(request, 503, json!({"ok": false, "error": "billing not configured"}));
        return;
    };
    let Some(AuthOutcome::Cookie(user_id)) = api::authorize(state, "", headers) else {
        api::respond_json(
            request,
            401,
            json!({ "error": { "code": "not_authenticated", "message": "需要登录" } }),
        );
        return;
    };
    // 只支持 pro 套餐
    let parsed: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    if parsed.get("plan").and_then(Value::as_str) != Some("pro") {
        api::respond_json(request, 400, json!({"ok": false, "error": "unsupported plan"}));
        return;
    }
    let (plan, period_end) = auth.store().get_plan_info(user_id).unwrap_or(("free".to_string(), None));
    if plan == "pro" && period_end.map(|pe| pe > now_secs()).unwrap_or(false) {
        api::respond_json(request, 409, json!({"ok": false, "error": "already subscribed"}));
        return;
    }
    let user = match auth.store().find_user_by_id(user_id) {
        Ok(Some(u)) => u,
        _ => {
            api::respond_json(request, 404, json!({"ok": false, "error": "user not found"}));
            return;
        }
    };
    let Some(runtime) = state.runtime() else {
        api::respond_json(request, 503, json!({"ok": false, "error": "async runtime unavailable"}));
        return;
    };
    let req_body = build_checkout_request_body(&cfg, user_id, &user.email, now_secs());
    let path = "/v1/actions/checkout/create-session".to_string();
    match runtime.block_on(post_signed_json(&cfg, &path, &req_body)) {
        Ok(checkout_url) => api::respond_json(request, 200, json!({"ok": true, "checkoutUrl": checkout_url})),
        Err(e) => api::respond_json(request, 502, json!({"ok": false, "error": format!("waffo: {e}")})),
    }
}

fn handle_webhook(
    state: &ServerState,
    raw_body: &str,
    headers: &[(String, String)],
    request: Request,
) {
    let Some(cfg) = state.load_app_state().as_ref().and_then(parse_billing_config) else {
        api::respond_json(request, 503, json!({"ok": false, "error": "billing not configured"}));
        return;
    };
    let sig_header = headers
        .iter()
        .find(|(k, _)| k == "x-waffo-signature")
        .map(|(_, v)| v.as_str())
        .unwrap_or("");
    if let Err(e) = verify_webhook_signature(raw_body, sig_header, &cfg.webhook_public_key_pem, now_secs()) {
        tracing::warn!(error = %e, "webhook signature verification failed");
        api::respond_json(request, 401, json!({"ok": false, "error": e}));
        return;
    }
    let parsed: Value = match serde_json::from_str(raw_body) {
        Ok(v) => v,
        Err(_) => {
            api::respond_json(request, 400, json!({"ok": false, "error": "invalid json"}));
            return;
        }
    };
    let event_id = parsed.get("id").and_then(Value::as_str).unwrap_or("").to_string();
    let event_type = parsed.get("eventType").and_then(Value::as_str).unwrap_or("").to_string();
    let mode = parsed.get("mode").and_then(Value::as_str).unwrap_or("");
    if !mode.is_empty() && mode != cfg.environment {
        tracing::info!(event = %event_type, mode = %mode, "webhook environment mismatch — ignored");
        respond_ok(request);
        return;
    }
    let Some(auth) = state.auth() else {
        api::respond_json(request, 503, json!({"ok": false, "error": "auth disabled"}));
        return;
    };
    let store = auth.store();
    if store.has_webhook_event(&event_id).unwrap_or(false) {
        respond_ok(request);
        return;
    }
    let data = parsed.get("data").cloned().unwrap_or(Value::Null);
    match process_webhook_event(store, &event_type, &data) {
        Ok(()) => {
            let _ = store.record_webhook_event(&event_id, &event_type, now_secs());
            tracing::info!(event = %event_type, "webhook processed");
            respond_ok(request);
        }
        Err(e) => {
            // 不记幂等 → Waffo 按退避重试
            tracing::warn!(event = %event_type, error = %e, "webhook processing failed");
            api::respond_json(request, 500, json!({"ok": false, "error": e}));
        }
    }
}

fn respond_ok(request: Request) {
    let resp = tiny_http::Response::from_string("OK").with_status_code(StatusCode(200));
    let _ = request.respond(resp);
}

async fn post_signed_json(cfg: &BillingConfig, path: &str, body: &Value) -> Result<String, String> {
    let body_bytes = serde_json::to_vec(body).map_err(|e| e.to_string())?;
    let headers = sign_headers("POST", path, &body_bytes, &cfg.merchant_id, &cfg.private_key_pem)?;
    let client = reqwest::Client::new();
    let mut req = client
        .post(format!("https://api.waffo.ai{path}"))
        .header("Content-Type", "application/json")
        .body(body_bytes);
    for (k, v) in headers {
        req = req.header(k, v);
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let parsed: Value = serde_json::from_str(&text).map_err(|_| format!("bad response {status}: {text}"))?;
    if status.is_success() {
        parsed
            .get("data")
            .and_then(|d| d.get("checkoutUrl"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| format!("missing checkoutUrl: {text}"))
    } else {
        let msg = parsed
            .get("errors")
            .and_then(|e| e.get(0))
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        Err(format!("{status}: {msg}"))
    }
}
```

- [ ] **Step 3: server.rs dispatch 接线** — 在 `dispatch_request` 中，`conversations` 分支之后、`let response = api::handle_request(...)` 之前插入：

```rust
// Billing (Waffo Pancake): /api/v1/billing/checkout + /api/v1/billing/webhook.
// Webhook is signature-verified (no cookie) and must bypass handle_request's
// generic auth, so route it here alongside conversations.
let billing_parts: Vec<&str> = path
    .trim_start_matches(API_PREFIX)
    .trim_start_matches('/')
    .split('/')
    .filter(|p| !p.is_empty())
    .collect();
if billing_parts.first().copied() == Some("billing") {
    api::billing::handle(&state, &method, &billing_parts, &body, &headers, request);
    return;
}
```

- [ ] **Step 4: 编译 + 手工冒烟验证**

Run: `cargo build --release --manifest-path overlay/server/Cargo.toml`
Expected: 编译通过。

再运行单测：
Run: `cargo test --manifest-path overlay/server/Cargo.toml billing`
Expected: 全部 PASS（含 build_checkout_request_body_shape）。

手工冒烟（需真实 billing 配置 + 登录态，见 Task 8 文档；此处至少验证路由存在）：
```bash
curl -sS -o /dev/null -w '%{http_code}\n' -X POST http://127.0.0.1:8080/api/v1/billing/checkout -H 'Content-Type: application/json' -d '{"plan":"pro"}'
# 预期：401（未登录）——证明路由已挂载而非 404
```

- [ ] **Step 5: Commit**

```bash
git add overlay/server/src/api/billing.rs overlay/server/src/server.rs
git commit -m "feat(billing): /api/v1/billing/checkout + /webhook routes wired into dispatch"
```

---

## Task 6: 配额强制 + `/auth/me` 套餐展示

**Files:**
- Modify: `overlay/server/src/api/chat.rs:153-158`
- Modify: `overlay/server/src/api/auth_routes.rs`（`handle_me` + `user_to_json`）

- [ ] **Step 1: chat.rs 改配额解析** — 把 `chat.rs` 中：

```rust
let date = today_utc_for_chat();
let limit = state.daily_chat_limit() as i64;
```

替换为：

```rust
let date = today_utc_for_chat();
let plan = auth
    .store()
    .get_plan(user_id)
    .unwrap_or_else(|_| "free".to_string());
let limit = crate::api::billing::resolve_daily_limit(
    state.load_app_state().as_ref(),
    &plan,
    state.daily_chat_limit(),
);
```

- [ ] **Step 2: auth_routes.rs 改 `/auth/me`** — `handle_me` 中：

```rust
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
```

替换为：

```rust
// Usage info (today, UTC) + subscription plan.
let (plan, period_end) = auth
    .store()
    .get_plan_info(user.id)
    .unwrap_or_else(|_| ("free".to_string(), None));
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
```

- [ ] **Step 3: user_to_json 暴露套餐字段**

```rust
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
```

- [ ] **Step 4: 编译 + 回归**

Run: `cargo build --release --manifest-path overlay/server/Cargo.toml`
Run: `cargo test --manifest-path overlay/server/Cargo.toml`
Run: `cargo test --manifest-path overlay/auth/Cargo.toml`
Expected: 全部通过，无警告新增。

- [ ] **Step 5: Commit**

```bash
git add overlay/server/src/api/chat.rs overlay/server/src/api/auth_routes.rs
git commit -m "feat(billing): per-plan daily quota in chat + plan in /auth/me"
```

---

## Task 7: 配置示例 + 前端接线

**Files:**
- Modify: `overlay/config/server.example.json`
- Modify: `overlay/static/pricing/index.html`
- Modify: `overlay/static/index.html`

- [ ] **Step 1: server.example.json 加 billing 块** — 在 `smtp` 块之后追加：

```json
"billing": {
  "enabled": true,
  "waffoMerchantId": "${WAFFO_MERCHANT_ID}",
  "waffoPrivateKey": "${WAFFO_PRIVATE_KEY}",
  "proProductId": "${WAFFO_PRO_PRODUCT_ID}",
  "webhookPublicKey": "${WAFFO_WEBHOOK_PUBLIC_KEY}",
  "environment": "test",
  "freeTierDailyLimit": 3,
  "proTierDailyLimit": 10000,
  "checkoutSuccessUrl": "https://www.sship.online/pricing?upgraded=1",
  "language": "zh-Hans"
}
```

- [ ] **Step 2: pricing/index.html 接线「立即订阅」** — 把 `专业版` 卡片里的链接（约 160-162 行）：

```html
<a href="/register" class="block text-center py-3 px-6 bg-primary-600 text-white rounded-xl font-semibold hover:bg-primary-700 transition-colors shadow-lg shadow-primary-600/30">
  立即订阅
</a>
```

替换为按钮 + 共享脚本（`</body>` 前）：

```html
<button id="btn-subscribe" class="block w-full text-center py-3 px-6 bg-primary-600 text-white rounded-xl font-semibold hover:bg-primary-700 transition-colors shadow-lg shadow-primary-600/30">
  立即订阅
</button>

<script>
document.getElementById('btn-subscribe')?.addEventListener('click', async (e) => {
  const btn = e.currentTarget;
  btn.disabled = true;
  btn.textContent = '正在跳转支付…';
  try {
    const r = await fetch('/api/v1/billing/checkout', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ plan: 'pro' }),
    });
    if (r.status === 401) { location.href = '/login?next=/pricing'; return; }
    const j = await r.json();
    if (r.ok && j.checkoutUrl) { window.open(j.checkoutUrl, '_blank', 'noopener,noreferrer'); }
    else { alert((j.error && typeof j.error === 'string' ? j.error : '无法创建订单，请稍后再试')); }
  } catch (err) { alert('网络错误，请稍后再试'); }
  btn.disabled = false;
  btn.textContent = '立即订阅';
});
</script>
```

- [ ] **Step 3: index.html 内嵌定价区同样接线** — 首页内嵌定价区的「立即订阅」（约 396 行，`<a href="/register">`）改为同款 `<button id="btn-subscribe">` + 同一段 `<script>`。

- [ ] **Step 4: 验证**

```bash
# 静态页语法抽查
node -e "const s=require('fs').readFileSync('overlay/static/pricing/index.html','utf8'); console.log(s.includes('btn-subscribe') ? 'pricing OK' : 'pricing MISSING')"
node -e "const s=require('fs').readFileSync('overlay/static/index.html','utf8'); console.log(s.includes('btn-subscribe') ? 'landing OK' : 'landing MISSING')"
```
Expected: 两行都 OK。

- [ ] **Step 5: Commit**

```bash
git add overlay/config/server.example.json overlay/static/pricing/index.html overlay/static/index.html
git commit -m "feat(billing): wire pricing/landing subscribe buttons to checkout API + config example"
```

---

## Task 8: E2E 脚本 + 文档

**Files:**
- Create: `scripts/e2e-billing.sh`
- Create: `docs/付款-Waffo-Pancake.md`

- [ ] **Step 1: 写 `scripts/e2e-billing.sh`**：

```bash
#!/usr/bin/env bash
# Waffo Pancake 付款 E2E（test mode）。
# 依赖：服务器已带 billing 配置启动（test 环境）、可访问公网收 webhook。
# 用法: ./scripts/e2e-billing.sh [BASE_URL] [EMAIL] [PASSWORD]
set -euo pipefail

BASE="${1:-http://127.0.0.1:8080}"
EMAIL="${2:-billing-e2e@test.com}"
PASSWORD="${3:-longenoughpass}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

say() { echo; echo "==> $*"; }
ok()  { echo "   ✅ $*"; }
fail(){ echo "   ❌ $*"; exit 1; }

say "Health check"
curl -sf "${BASE}/api/v1/health" >/dev/null || fail "server not up"
ok "server up"

say "Register + verify email (token 从服务器日志取)"
curl -sf -X POST "${BASE}/auth/register" -H 'Content-Type: application/json' \
  -d "{\"email\":\"${EMAIL}\",\"password\":\"${PASSWORD}\"}" >/dev/null \
  || fail "register"
# e2e-auth 的 verify 逻辑：token 出现在服务器 stderr 日志（SMTP 未配时）
say "请在上一步服务器日志中复制 verify token，然后:"
read -rp "  粘贴 verify token: " VT
curl -sf -o /dev/null "${BASE}/auth/verify-email?token=${VT}" || fail "verify-email"
ok "email verified"

say "登录拿 cookie"
curl -sf -c "${TMP}/c.txt" -X POST "${BASE}/auth/login" -H 'Content-Type: application/json' \
  -d "{\"email\":\"${EMAIL}\",\"password\":\"${PASSWORD}\"}" >/dev/null || fail "login"
ok "logged in"

say "创建 checkout session"
CHECKOUT="$(curl -sf -b "${TMP}/c.txt" -X POST "${BASE}/api/v1/billing/checkout" \
  -H 'Content-Type: application/json' -d '{"plan":"pro"}')"
echo "   checkoutUrl: $(echo "$CHECKOUT" | sed 's/.*"checkoutUrl":"\([^"]*\)".*/\1/')"
echo
echo "   👉 在浏览器打开上面的 checkoutUrl，用测试卡 4576 7500 0000 0110 完成支付"
echo "   👉 支付成功后 Waffo 会发 webhook 到服务器（需在 Dashboard 配好 test webhook URL）"
read -rp "   完成后按回车继续: " _

say "轮询 /auth/me 等待 plan=pro (最多 30s)"
for i in $(seq 1 15); do
  ME="$(curl -sf -b "${TMP}/c.txt" "${BASE}/auth/me")"
  PLAN="$(echo "$ME" | sed 's/.*"name":"\([^"]*\)".*/\1/')"
  LIMIT="$(echo "$ME" | sed 's/.*"limit":\([0-9]*\).*/\1/')"
  [ "$PLAN" = "pro" ] && break
  sleep 2
done
[ "$PLAN" = "pro" ] || fail "plan 未变 pro（当前: $PLAN）"
ok "plan=pro"
[ "$LIMIT" = "10000" ] && ok "limit=10000" || fail "limit 应为 10000, 实际 $LIMIT"

say "完成 ✅（取消订阅降级流程：Dashboard/客服取消 → webhook subscription.canceled → /auth/me plan=free，可手动复验）"
```

> 脚本用 sed 解析 JSON（与 `scripts/e2e-auth.sh` 风格一致），不引入 jq 依赖。脚本可执行：`chmod +x scripts/e2e-billing.sh`。

- [ ] **Step 2: 写 `docs/付款-Waffo-Pancake.md`** — 内容：

```markdown
# Waffo Pancake 付款接入（DocuChat Pro 订阅）

> Spec: [docs/superpowers/specs/2026-08-02-waffo-payments-design.md](../docs/superpowers/specs/2026-08-02-waffo-payments-design.md)
> 外部资料: https://docs.waffo.ai/llms-full.txt

## 1. Dashboard 准备（一次性）

1. 注册商户: https://pancake.waffo.ai/merchant/auth/signin
2. 建 store（或复用现有）
3. **API & Development → Create API Key（选 Test）→ 立即下载私钥**（只显示一次）
4. Products → 建订阅产品「Pro Monthly」: billingPeriod=monthly, USD 19.00, taxCategory=saas; 复制 Product ID
5. Settings → Webhooks → 复制 **Test Webhook Public Key**
6. Settings → Webhooks → Add Webhook: URL `https://<域名>/api/v1/billing/webhook`（test），订阅 subscription.* 事件

## 2. 服务器配置

`server.local.json`（或环境变量）:

| 变量 | 值 |
|------|-----|
| `WAFFO_MERCHANT_ID` | `MER_...` |
| `WAFFO_PRIVATE_KEY` | 私钥 PEM（base64 或转义换行） |
| `WAFFO_PRO_PRODUCT_ID` | `PROD_...` |
| `WAFFO_WEBHOOK_PUBLIC_KEY` | Dashboard 复制的 Test/Prod 公钥 |
| `WAFFO_ENVIRONMENT` | `test` 或 `prod` |

示例 `billing` 块见 `overlay/config/server.example.json`。密钥经 `deploy-ecs.sh` 的 sed 注入 `server.local.json`（chmod 600），**不进 git、不打日志**。

## 3. 本地联调（webhook 回环）

- 用 cloudflared 或 ngrok 把本机 8080 暴露成公网 HTTPS，把该 URL 配成 Dashboard 的 test webhook
- Dashboard → Send Test Event 验证签名（若 401，见 §5 第一行）
- 测试卡: 成功 `4576 7500 0000 0110` / 拒绝 `4576 7500 0000 0220`

## 4. 验证流程

`./scripts/e2e-billing.sh`（需登录态 + 真实支付一步手动完成）。

## 5. 排错

| 现象 | 原因 | 处理 |
|------|------|------|
| webhook 401 验签失败 | signed-payload 构造或公钥环境不对 | 用 Dashboard Send Test Event 抓真实 `X-Waffo-Signature` 对拍；确认 Test/Prod 公钥与事件 mode 一致 |
| checkout 502 | 私钥无效 / 时间戳超前 >1min | 检查 `WAFFO_PRIVATE_KEY` PEM 格式；服务器 NTP 校准 |
| 产品不可见 | 未 `.publish()` 到 prod | test 环境用 test key；上线前 publish + 换 prod key |
| checkout 401 未登录 | 无 session cookie | 先 `/auth/login` |
```

- [ ] **Step 3: 语法校验脚本**

Run: `bash -n scripts/e2e-billing.sh`
Expected: 无输出（语法 OK）。

- [ ] **Step 4: Commit**

```bash
git add scripts/e2e-billing.sh docs/付款-Waffo-Pancake.md
git commit -m "feat(billing): e2e-billing script + Waffo setup/runbook doc"
```

---

## 自检记录（写完后的自查）

- **Spec 覆盖**：DB 迁移（Task1）、配置（Task3/7）、checkout+webhook 路由（Task5）、事件状态机（Task4）、配额强制（Task6）、前端（Task7）、安全/幂等（Task1/4/5）、测试（各 Task + Task8）、Dashboard 准备（Task8 文档）——均已落任务。
- **无占位符**：所有代码步骤含完整实现；唯一"手工"步骤是真实支付（Waffo 托管页无法自动化），在 Task 8 脚本中明确标注。
- **类型一致**：`set_plan(user_id, plan, order_id, period_end, now)` 在 Task1 定义、Task4 使用，签名一致；`resolve_daily_limit(app_state, plan, global_default) -> i64` 在 Task3 定义、Task6 使用，一致。`PlanAction` 变体在 Task4 定义与断言一致。
