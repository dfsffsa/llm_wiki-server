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
        //
        // We match on the human-readable error string. This is intentionally
        // simple but brittle: if rusqlite/SQLite ever change the wording, the
        // duplicate-email path silently falls through to Internal (500 instead
        // of 409). A more robust v1.1 approach is to match on the extended
        // result code:
        //   matches!(&e, rusqlite::Error::SqliteFailure(err, _)
        //                if err.extended_code == 2067) // SQLITE_CONSTRAINT_UNIQUE
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
