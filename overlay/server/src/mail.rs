//! Outbound email (SMTP) — currently only the password-reset link.
//!
//! Provider-agnostic: any SMTP server (Gmail, SES via SMTP, Mailgun, Postfix)
//! works via the `smtp` config block. Uses lettre's async transport on the
//! shared tokio runtime. When SMTP is unconfigured, the reset token falls
//! back to being logged (dev/operator flow), so the auth feature degrades
//! gracefully instead of breaking.

use serde_json::Value;

const RESET_TOKEN_TTL_HOURS: i64 = 1;

/// Parsed `smtp` config block. `None`-valued fields mean "send email but
/// without that knob" — see `enabled`.
#[derive(Debug, Clone)]
pub struct SmtpConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub from: String,
    /// Base URL of the public site, e.g. `https://wiki.example.com` (no
    /// trailing slash). The reset link is `<publicBaseUrl>/reset-password?token=...`.
    pub public_base_url: String,
}

/// Extract an `SmtpConfig` from the parsed server config JSON (`smtp` block).
/// Returns `None` when absent or disabled — callers then skip email delivery.
pub fn parse_smtp_config(app_state: &Value) -> Option<SmtpConfig> {
    let cfg = app_state.get("smtp")?;
    let enabled = cfg.get("enabled").and_then(Value::as_bool).unwrap_or(false);
    if !enabled {
        return None;
    }
    let host = cfg.get("host").and_then(Value::as_str)?.to_string();
    if host.is_empty() {
        return None;
    }
    Some(SmtpConfig {
        enabled: true,
        host,
        port: cfg.get("port").and_then(Value::as_u64).unwrap_or(587) as u16,
        user: cfg.get("user").and_then(Value::as_str).unwrap_or("").to_string(),
        password: cfg.get("pass").and_then(Value::as_str).unwrap_or("").to_string(),
        from: cfg.get("from").and_then(Value::as_str).unwrap_or("").to_string(),
        public_base_url: cfg
            .get("publicBaseUrl")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim_end_matches('/')
            .to_string(),
    })
}

/// Build the password-reset URL the user clicks in the email.
///
/// `base_url` may or may not have a trailing slash; the result is always
/// `<base>/reset-password?token=<token>`. Pure / IO-free so the URL shape is
/// regression-tested.
pub fn build_reset_url(base_url: &str, token: &str) -> String {
    let base = base_url.trim_end_matches('/');
    format!("{base}/reset-password?token={token}")
}

/// Build the plain-text body of the password-reset email. Pure.
pub fn build_reset_email_body(reset_url: &str) -> String {
    format!(
        "您请求重置 LLM Wiki 账户密码。\n\n\
         点击下方链接设置新密码（{ttl} 小时内有效，单次使用）：\n{url}\n\n\
         如果不是您本人操作，请忽略此邮件，您的密码不会更改。\n",
        ttl = RESET_TOKEN_TTL_HOURS,
        url = reset_url,
    )
}

/// Send the password-reset email. Returns `Ok(())` on accepted-by-SMTP-server.
/// Errors are descriptive strings (logged + collapsed to ok:true by the HTTP
/// layer to avoid leaking account existence).
pub async fn send_password_reset(
    cfg: &SmtpConfig,
    to_email: &str,
    reset_url: &str,
) -> Result<(), String> {
    use lettre::message::header::ContentType;
    use lettre::message::{Mailbox, Message};
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};

    let from: Mailbox = cfg
        .from
        .parse()
        .map_err(|e| format!("invalid smtp.from: {e}"))?;
    let to: Mailbox = to_email
        .parse()
        .map_err(|e| format!("invalid recipient email: {e}"))?;

    let body = build_reset_email_body(reset_url);
    let email = Message::builder()
        .from(from)
        .to(to)
        .subject("重置你的 LLM Wiki 密码")
        .header(ContentType::parse("text/plain; charset=utf-8").map_err(|e| format!("content-type: {e}"))?)
        .body(body)
        .map_err(|e| format!("build email: {e}"))?;

    let mut transport_builder = AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.host)
        .map_err(|e| format!("smtp relay: {e}"))?
        .port(cfg.port);
    if !cfg.user.is_empty() {
        transport_builder = transport_builder.credentials(Credentials::new(
            cfg.user.clone(),
            cfg.password.clone(),
        ));
    }
    let transport = transport_builder.build();
    transport
        .send(email)
        .await
        .map_err(|e| format!("smtp send: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- build_reset_url ---

    #[test]
    fn reset_url_appends_token_without_double_slash() {
        assert_eq!(
            build_reset_url("https://wiki.example.com", "abc123"),
            "https://wiki.example.com/reset-password?token=abc123"
        );
    }

    #[test]
    fn reset_url_strips_trailing_slash_from_base() {
        assert_eq!(
            build_reset_url("https://wiki.example.com/", "t"),
            "https://wiki.example.com/reset-password?token=t"
        );
    }

    // --- build_reset_email_body ---

    #[test]
    fn email_body_contains_reset_url_and_expiry() {
        let url = "https://wiki.example.com/reset-password?token=abc123";
        let body = build_reset_email_body(url);
        assert!(body.contains(url), "body must contain the reset URL");
        assert!(body.contains("1 小时"), "body must state the expiry");
    }

    // --- parse_smtp_config ---

    #[test]
    fn parse_smtp_config_returns_none_when_disabled() {
        let v = json!({ "smtp": { "enabled": false, "host": "smtp.x.com" } });
        assert!(parse_smtp_config(&v).is_none());
    }

    #[test]
    fn parse_smtp_config_returns_none_when_absent() {
        let v = json!({ "llmConfig": {} });
        assert!(parse_smtp_config(&v).is_none());
    }

    #[test]
    fn parse_smtp_config_parses_enabled_block() {
        let v = json!({
            "smtp": {
                "enabled": true, "host": "smtp.example.com", "port": 587,
                "user": "u", "pass": "p", "from": "noreply@example.com",
                "publicBaseUrl": "https://wiki.example.com/"
            }
        });
        let c = parse_smtp_config(&v).unwrap();
        assert_eq!(c.host, "smtp.example.com");
        assert_eq!(c.port, 587);
        assert_eq!(c.public_base_url, "https://wiki.example.com"); // trailing slash stripped
    }

    #[test]
    fn parse_smtp_config_defaults_port_to_587() {
        let v = json!({ "smtp": { "enabled": true, "host": "smtp.x.com" } });
        assert_eq!(parse_smtp_config(&v).unwrap().port, 587);
    }
}
