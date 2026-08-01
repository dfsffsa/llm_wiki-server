//! Outbound email (SMTP) — multipart (HTML + plain text) delivery for all
//! notification types.
//!
//! Provider-agnostic: any SMTP server (Gmail, SES via SMTP, Mailgun, Postfix)
//! works via the `smtp` config block. Uses lettre's async transport on the
//! shared tokio runtime. When SMTP is unconfigured, the reset token falls
//! back to being logged (dev/operator flow), so the auth feature degrades
//! gracefully instead of breaking.

use lettre::message::MultiPart;
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
pub fn build_reset_plain(reset_url: &str) -> String {
    format!(
        "您请求重置 LLM Wiki 账户密码。\n\n\
         点击下方链接设置新密码（{ttl} 小时内有效，单次使用）：\n{url}\n\n\
         如果不是您本人操作，请忽略此邮件，您的密码不会更改。\n",
        ttl = RESET_TOKEN_TTL_HOURS,
        url = reset_url,
    )
}

// ---------------------------------------------------------------------------
// HTML template functions
// ---------------------------------------------------------------------------

pub fn build_verify_html(verify_url: &str) -> String {
    format!(r#"<!DOCTYPE html><html><body style="font-family:sans-serif;max-width:600px;margin:40px auto;padding:20px;line-height:1.6">
<h2 style="color:#333">验证您的邮箱</h2>
<p>感谢注册 LLM Wiki！请点击下方按钮验证您的邮箱：</p>
<a href="{verify_url}" style="display:inline-block;background:#4f46e5;color:#fff;padding:12px 24px;border-radius:6px;text-decoration:none;margin:16px 0">验证邮箱</a>
<p style="color:#666;font-size:14px">此链接 1 小时内有效。如果您没有注册，请忽略此邮件。</p>
</body></html>"#)
}

pub fn build_verify_plain(verify_url: &str) -> String {
    format!("感谢注册 LLM Wiki！请点击以下链接验证您的邮箱（1 小时内有效）：\n\n{verify_url}\n\n如果您没有注册，请忽略此邮件。")
}

pub fn build_welcome_html(display_name: &str) -> String {
    format!(r#"<!DOCTYPE html><html><body style="font-family:sans-serif;max-width:600px;margin:40px auto;padding:20px;line-height:1.6">
<h2 style="color:#333">欢迎使用 LLM Wiki！</h2>
<p>{display_name} 您好，</p>
<p>您的邮箱已验证成功。现在可以登录并开始使用 LLM Wiki 了。</p>
<p>如果您有任何问题，请联系管理员。</p>
</body></html>"#)
}

pub fn build_welcome_plain(display_name: &str) -> String {
    format!("{display_name} 您好，\n\n您的邮箱已验证成功。现在可以登录并开始使用 LLM Wiki 了。")
}

pub fn build_email_change_notice_html(confirm_url: &str) -> String {
    format!(r#"<!DOCTYPE html><html><body style="font-family:sans-serif;max-width:600px;margin:40px auto;padding:20px;line-height:1.6">
<h2 style="color:#333">邮箱变更确认</h2>
<p>您发起了修改邮箱的请求。请点击下方按钮确认此操作：</p>
<a href="{confirm_url}" style="display:inline-block;background:#4f46e5;color:#fff;padding:12px 24px;border-radius:6px;text-decoration:none;margin:16px 0">确认变更</a>
<p style="color:#666;font-size:14px">如果您没有发起此请求，请忽略此邮件并检查账户安全。</p>
</body></html>"#)
}

pub fn build_email_change_notice_plain(confirm_url: &str) -> String {
    format!("您发起了修改邮箱的请求。请点击以下链接确认（1 小时内有效）：\n\n{confirm_url}\n\n如果没有发起，请忽略此邮件。")
}

pub fn build_new_email_verify_html(verify_url: &str) -> String {
    format!(r#"<!DOCTYPE html><html><body style="font-family:sans-serif;max-width:600px;margin:40px auto;padding:20px;line-height:1.6">
<h2 style="color:#333">验证新邮箱</h2>
<p>请点击下方按钮验证您的新邮箱地址：</p>
<a href="{verify_url}" style="display:inline-block;background:#4f46e5;color:#fff;padding:12px 24px;border-radius:6px;text-decoration:none;margin:16px 0">验证新邮箱</a>
<p style="color:#666;font-size:14px">此链接 1 小时内有效。如非本人操作请忽略。</p>
</body></html>"#)
}

pub fn build_new_email_verify_plain(verify_url: &str) -> String {
    format!("请点击以下链接验证您的新邮箱（1 小时内有效）：\n\n{verify_url}")
}

pub fn build_reset_html(reset_url: &str) -> String {
    format!(r#"<!DOCTYPE html><html><body style="font-family:sans-serif;max-width:600px;margin:40px auto;padding:20px;line-height:1.6">
<h2 style="color:#333">重置您的 LLM Wiki 密码</h2>
<p>请点击下方按钮重置密码（1 小时内有效，单次使用）：</p>
<a href="{reset_url}" style="display:inline-block;background:#4f46e5;color:#fff;padding:12px 24px;border-radius:6px;text-decoration:none;margin:16px 0">重置密码</a>
<p style="color:#666;font-size:14px">如非本人操作请忽略，您的密码不会更改。</p>
</body></html>"#)
}

// ---------------------------------------------------------------------------
// Generic multipart send helper
// ---------------------------------------------------------------------------

/// Send a multipart (HTML + plain text) email via SMTP.
pub async fn send_email(
    cfg: &SmtpConfig,
    to: &str,
    subject: &str,
    html_body: &str,
    plain_body: &str,
) -> Result<(), String> {
    use lettre::message::header::ContentType;
    use lettre::message::{Mailbox, Message};
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};

    let from: Mailbox = cfg.from.parse()
        .map_err(|e| format!("invalid smtp.from: {e}"))?;
    let to: Mailbox = to.parse()
        .map_err(|e| format!("invalid recipient email: {e}"))?;

    let plain_part = lettre::message::SinglePart::builder()
        .header(ContentType::parse("text/plain; charset=utf-8").map_err(|e| format!("content-type: {e}"))?)
        .body(plain_body.to_string());

    let html_part = lettre::message::SinglePart::builder()
        .header(ContentType::parse("text/html; charset=utf-8").map_err(|e| format!("content-type: {e}"))?)
        .body(html_body.to_string());

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(subject)
        .multipart(MultiPart::alternative().singlepart(plain_part).singlepart(html_part))
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
    transport.send(email).await.map_err(|e| format!("smtp send: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// High-level send functions
// ---------------------------------------------------------------------------

/// Send an email verification link.
pub async fn send_verification_email(
    cfg: &SmtpConfig, to: &str, token: &str,
) -> Result<(), String> {
    let verify_url = format!("{}/auth/verify-email?token={token}", cfg.public_base_url.trim_end_matches('/'));
    send_email(cfg, to, "验证您的 LLM Wiki 邮箱",
        &build_verify_html(&verify_url),
        &build_verify_plain(&verify_url)).await
}

/// Send a welcome email after successful verification.
pub async fn send_welcome_email(
    cfg: &SmtpConfig, to: &str, display_name: &str,
) -> Result<(), String> {
    send_email(cfg, to, "欢迎使用 LLM Wiki",
        &build_welcome_html(display_name),
        &build_welcome_plain(display_name)).await
}

/// Send an email-change confirmation notice to the current email.
pub async fn send_email_change_notice(
    cfg: &SmtpConfig, to: &str, confirm_url: &str,
) -> Result<(), String> {
    send_email(cfg, to, "确认邮箱变更",
        &build_email_change_notice_html(confirm_url),
        &build_email_change_notice_plain(confirm_url)).await
}

/// Send a verification link to the *new* email address.
pub async fn send_new_email_verification(
    cfg: &SmtpConfig, to: &str, verify_url: &str,
) -> Result<(), String> {
    send_email(cfg, to, "验证新邮箱地址",
        &build_new_email_verify_html(verify_url),
        &build_new_email_verify_plain(verify_url)).await
}

/// Send the password-reset email. Uses the generic multipart helper.
pub async fn send_password_reset(
    cfg: &SmtpConfig,
    to_email: &str,
    reset_url: &str,
) -> Result<(), String> {
    send_email(cfg, to_email, "重置你的 LLM Wiki 密码",
        &build_reset_html(reset_url),
        &build_reset_plain(reset_url)).await
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

    // --- build_reset_plain ---

    #[test]
    fn email_body_contains_reset_url_and_expiry() {
        let url = "https://wiki.example.com/reset-password?token=abc123";
        let body = build_reset_plain(url);
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

    // --- build_verify_html / build_verify_plain ---

    #[test]
    fn verify_html_contains_link() {
        let html = build_verify_html("https://example.com/auth/verify-email?token=abc");
        assert!(html.contains("verify-email?token=abc"));
        assert!(html.contains("验证邮箱"));
    }

    #[test]
    fn verify_plain_contains_link() {
        let plain = build_verify_plain("https://example.com/auth/verify-email?token=abc");
        assert!(plain.contains("verify-email?token=abc"));
    }

    // --- build_welcome_html / build_welcome_plain ---

    #[test]
    fn welcome_html_contains_name() {
        let html = build_welcome_html("测试用户");
        assert!(html.contains("测试用户"));
        assert!(html.contains("已验证成功"));
    }

    #[test]
    fn welcome_plain_contains_name() {
        let plain = build_welcome_plain("测试用户");
        assert!(plain.contains("测试用户"));
        assert!(plain.contains("已验证成功"));
    }

    // --- build_email_change_notice_html / build_email_change_notice_plain ---

    #[test]
    fn email_change_notice_html_contains_link() {
        let html = build_email_change_notice_html("https://example.com/confirm?token=abc");
        assert!(html.contains("confirm?token=abc"));
        assert!(html.contains("确认变更"));
    }

    #[test]
    fn email_change_notice_plain_contains_link() {
        let plain = build_email_change_notice_plain("https://example.com/confirm?token=abc");
        assert!(plain.contains("confirm?token=abc"));
    }

    // --- build_new_email_verify_html / build_new_email_verify_plain ---

    #[test]
    fn new_email_verify_html_contains_link() {
        let html = build_new_email_verify_html("https://example.com/verify-new?token=abc");
        assert!(html.contains("verify-new?token=abc"));
        assert!(html.contains("验证新邮箱"));
    }

    #[test]
    fn new_email_verify_plain_contains_link() {
        let plain = build_new_email_verify_plain("https://example.com/verify-new?token=abc");
        assert!(plain.contains("verify-new?token=abc"));
    }

    // --- build_reset_html ---

    #[test]
    fn reset_html_contains_link() {
        let html = build_reset_html("https://example.com/reset-password?token=abc");
        assert!(html.contains("reset-password?token=abc"));
        assert!(html.contains("重置密码"));
    }
}
