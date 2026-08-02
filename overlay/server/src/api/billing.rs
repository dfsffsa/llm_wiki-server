//! Waffo Pancake billing: RSA-SHA256 request signing, webhook verification,
//! subscription entitlement. Pure-Rust (no Node shim).
//! Reference: https://docs.waffo.ai/llms-full.txt

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rsa::pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey};
use rsa::pkcs8::LineEnding;
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::{Pkcs1v15Sign, RsaPrivateKey, RsaPublicKey};
use sha2::{Digest, Sha256};

pub fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
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
    let digest = Sha256::digest(canonical.as_bytes());
    let sig = key
        .sign(Pkcs1v15Sign::new::<Sha256>(), &digest)
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
        let part = part.trim();
        if let Some(v) = part.strip_prefix("t=") {
            ts = v.parse().ok();
        } else if let Some(v) = part.strip_prefix("v1=") {
            sig_b64 = Some(v);
        }
    }
    let ts = ts.ok_or("missing t=")?;
    let sig_b64 = sig_b64.ok_or("missing v1=")?;
    if ts > now + 60 || ts < now - 300 {
        return Err(format!(
            "webhook timestamp outside tolerance (now={now} ts={ts})"
        ));
    }
    let signed = format!("{ts}.{raw_body}");
    let digest = Sha256::digest(signed.as_bytes());
    let sig = B64.decode(sig_b64).map_err(|_| "bad base64 signature".to_string())?;
    let key = RsaPublicKey::from_public_key_pem(public_key_pem)
        .or_else(|_| DecodeRsaPublicKey::from_pkcs1_pem(public_key_pem))
        .map_err(|e| format!("invalid webhook public key: {e}"))?;
    key.verify(
        Pkcs1v15Sign::new::<Sha256>(),
        &digest,
        &sig,
    )
    .map_err(|_| "webhook signature verification failed".to_string())
}

/// "2026-03-10" (ISO date) -> unix seconds at 00:00 UTC (Howard Hinnant algorithm).
pub fn iso_date_to_epoch(s: &str) -> Option<i64> {
    let mut it = s.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    if !(1..=12).contains(&m) || !(1..=days_in_month(y, m)).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d) * 86_400)
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            let leap = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
            if leap { 29 } else { 28 }
        }
        _ => 0,
    }
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

/// Clamp a config numeric to u32, logging if it was absurdly large (typo guard).
fn u32_clamp(v: u64) -> u32 {
    match u32::try_from(v) {
        Ok(n) => n,
        Err(_) => {
            tracing::warn!("billing limit value {v} exceeds u32::MAX — clamping");
            u32::MAX
        }
    }
}

pub fn parse_billing_config(app_state: &serde_json::Value) -> Option<BillingConfig> {
    let b = app_state.get("billing")?;
    if b.get("enabled").and_then(serde_json::Value::as_bool) == Some(false) {
        return None;
    }
    let cfg = (|| {
        Some(BillingConfig {
            merchant_id: b.get("waffoMerchantId")?.as_str()?.trim().to_string(),
            private_key_pem: b.get("waffoPrivateKey")?.as_str()?.trim().to_string(),
            pro_product_id: b.get("proProductId")?.as_str()?.trim().to_string(),
            webhook_public_key_pem: b.get("webhookPublicKey")?.as_str()?.trim().to_string(),
            environment: b
                .get("environment")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("test")
                .trim()
                .to_string(),
            free_tier_daily_limit: b
                .get("freeTierDailyLimit")
                .and_then(serde_json::Value::as_u64)
                .map(u32_clamp)
                .unwrap_or(3),
            pro_tier_daily_limit: b
                .get("proTierDailyLimit")
                .and_then(serde_json::Value::as_u64)
                .map(u32_clamp)
                .unwrap_or(10_000),
            checkout_success_url: b
                .get("checkoutSuccessUrl")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string(),
            language: b
                .get("language")
                .and_then(serde_json::Value::as_str)
                .map(|s| s.trim().to_owned()),
        })
    })();
    if cfg.is_none() {
        tracing::warn!("billing block present but required field missing or wrong type — billing disabled");
    }
    cfg
}

/// Per-user daily chat limit: pro -> proTierDailyLimit, free -> freeTierDailyLimit.
/// No billing block / billing disabled -> falls back to the global limit.
pub fn resolve_daily_limit(
    app_state: Option<&serde_json::Value>,
    plan: &str,
    global_default: u32,
) -> i64 {
    let Some(cfg) = app_state.and_then(parse_billing_config) else {
        return global_default as i64;
    };
    if plan == "pro" {
        cfg.pro_tier_daily_limit as i64
    } else {
        cfg.free_tier_daily_limit as i64
    }
}

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
        rsa::pkcs8::EncodePrivateKey::to_pkcs8_pem(k, LineEnding::LF)
            .unwrap()
            .to_string()
    }

    fn pub_pem(k: &RsaPublicKey) -> String {
        rsa::pkcs8::EncodePublicKey::to_public_key_pem(k, LineEnding::LF)
            .unwrap()
            .to_string()
    }

    #[test]
    fn sign_headers_are_verifiable() {
        let (privk, pubk) = keypair();
        let headers = sign_headers(
            "POST",
            "/v1/actions/checkout/create-session",
            b"{}",
            "MER_1",
            &priv_pem(&privk),
        )
        .unwrap();
        let ts: i64 = headers
            .iter()
            .find(|(k, _)| k == "X-Timestamp")
            .unwrap()
            .1
            .parse()
            .unwrap();
        let sig = B64
            .decode(
                headers
                    .iter()
                    .find(|(k, _)| k == "X-Signature")
                    .unwrap()
                    .1
                    .clone(),
            )
            .unwrap();
        let canonical = format!(
            "POST\n/v1/actions/checkout/create-session\n{ts}\n{}",
            B64.encode(Sha256::digest(b"{}"))
        );
        let digest = Sha256::digest(canonical.as_bytes());
        pubk.verify(Pkcs1v15Sign::new::<Sha256>(), &digest, &sig)
            .unwrap();
    }

    #[test]
    fn verify_webhook_valid() {
        let (privk, pubk) = keypair();
        let body = r#"{"id":"evt_1","eventType":"subscription.activated","data":{}}"#;
        let ts = 1_700_000_000i64;
        let signed = format!("{ts}.{body}");
        let sig = privk
            .sign(Pkcs1v15Sign::new::<Sha256>(), &Sha256::digest(signed.as_bytes()))
            .unwrap();
        let header = format!("t={ts},v1={}", B64.encode(sig));
        verify_webhook_signature(body, &header, &pub_pem(&pubk), ts).unwrap();
    }

    #[test]
    fn verify_webhook_rejects_tampered_body() {
        let (privk, pubk) = keypair();
        let body = r#"{"id":"evt_1","eventType":"subscription.activated","data":{}}"#;
        let ts = 1_700_000_000i64;
        let signed = format!("{ts}.{body}");
        let sig = privk
            .sign(Pkcs1v15Sign::new::<Sha256>(), &Sha256::digest(signed.as_bytes()))
            .unwrap();
        let header = format!("t={ts},v1={}", B64.encode(sig));
        assert!(
            verify_webhook_signature("tampered", &header, &pub_pem(&pubk), ts).is_err()
        );
    }

    #[test]
    fn verify_webhook_rejects_stale_timestamp() {
        let (privk, pubk) = keypair();
        let body = "{}";
        let ts = 1_700_000_000i64;
        let signed = format!("{ts}.{body}");
        let sig = privk
            .sign(Pkcs1v15Sign::new::<Sha256>(), &Sha256::digest(signed.as_bytes()))
            .unwrap();
        let header = format!("t={ts},v1={}", B64.encode(sig));
        // now is 10 minutes ahead of ts
        assert!(
            verify_webhook_signature(body, &header, &pub_pem(&pubk), ts + 600).is_err()
        );
    }

    #[test]
    fn verify_webhook_rejects_future_timestamp() {
        let (privk, pubk) = keypair();
        let body = "{}";
        let ts = 1_700_000_000i64;
        let signed = format!("{ts}.{body}");
        let sig = privk
            .sign(Pkcs1v15Sign::new::<Sha256>(), &Sha256::digest(signed.as_bytes()))
            .unwrap();
        let header = format!("t={ts},v1={}", B64.encode(sig));
        // now is 2 minutes behind ts → ts is 2 min in the future → reject (1 min max)
        assert!(
            verify_webhook_signature(body, &header, &pub_pem(&pubk), ts - 120).is_err()
        );
    }

    #[test]
    fn sign_and_verify_with_pkcs1_pem() {
        let (privk, pubk) = keypair();
        let priv_pkcs1 = rsa::pkcs1::EncodeRsaPrivateKey::to_pkcs1_pem(&privk, rsa::pkcs1::LineEnding::LF)
            .unwrap()
            .to_string();
        let pub_pkcs1 = rsa::pkcs1::EncodeRsaPublicKey::to_pkcs1_pem(&pubk, rsa::pkcs1::LineEnding::LF)
            .unwrap()
            .to_string();
        // Private-key path: sign_headers must parse the PKCS#1 PEM via the or_else fallback.
        let headers = sign_headers("POST", "/p", b"{}", "MER_1", &priv_pkcs1).unwrap();
        assert!(headers.iter().any(|(k, _)| k == "X-Signature"));
        // Public-key path: verify_webhook_signature must parse the PKCS#1 public key PEM.
        let body = r#"{"id":"evt_1","eventType":"subscription.activated","data":{}}"#;
        let ts = 1_700_000_000i64;
        let sig = privk
            .sign(
                Pkcs1v15Sign::new::<Sha256>(),
                &Sha256::digest(format!("{ts}.{body}").as_bytes()),
            )
            .unwrap();
        let header = format!("t={ts},v1={}", B64.encode(sig));
        verify_webhook_signature(body, &header, &pub_pkcs1, ts).unwrap();
    }

    #[test]
    fn iso_date_to_epoch_known_values() {
        assert_eq!(iso_date_to_epoch("1970-01-01"), Some(0));
        assert_eq!(iso_date_to_epoch("1970-01-02"), Some(86_400));
        assert_eq!(iso_date_to_epoch("2026-03-10"), Some(1_773_100_800));
        assert_eq!(iso_date_to_epoch("2024-02-29"), Some(1_709_164_800)); // leap year
        assert_eq!(iso_date_to_epoch("2026-13-01"), None);
        assert_eq!(iso_date_to_epoch("2026-02-30"), None); // impossible date
        assert_eq!(iso_date_to_epoch("2024-02-30"), None); // leap-year Feb also capped at 29
        assert_eq!(iso_date_to_epoch("garbage"), None);
    }

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
}
