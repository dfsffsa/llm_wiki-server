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
    dummy_hash: String,
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
        // Pre-compute a hash for timing equalization in `login` — see the
        // unknown-email branch for rationale.
        let dummy_hash = hash_password("dummy")
            .expect("argon2 hash of constant must succeed");
        Self { store, cfg, limiter: RateLimiter::new(), dummy_hash }
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
            None => {
                // Run a verify against a pre-computed hash so the
                // unknown-email path takes about the same time as the
                // wrong-password path. Without this, response timing leaks
                // whether the email is registered.
                let _ = verify_password(&self.dummy_hash, input.password);
                return Err(AuthError::InvalidCredentials);
            }
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
