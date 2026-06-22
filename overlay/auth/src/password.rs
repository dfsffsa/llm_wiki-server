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
