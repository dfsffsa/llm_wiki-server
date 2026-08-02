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
}
