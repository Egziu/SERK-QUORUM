//! Credential primitives for SERK QUORUM.
//!
//! * Passwords for locally managed accounts are hashed with Argon2id using
//!   the library's default parameters (19 MiB memory, 2 iterations,
//!   1 lane — the OWASP Password Storage Cheat Sheet baseline).
//! * Session and invitation tokens are 256-bit values from the operating
//!   system CSPRNG. Only their SHA-256 digest is stored, so a database
//!   read does not yield usable bearer tokens.
//!
//! No algorithm here is home-grown; this crate only composes maintained
//! RustCrypto implementations.

use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("secure random generator unavailable")]
    Random,
    #[error("password hashing failed")]
    Hashing,
}

/// Hashes a password with Argon2id and returns a PHC string
/// (`$argon2id$v=19$m=19456,t=2,p=1$<salt>$<hash>`).
pub fn hash_password(password: &str) -> Result<String, AuthError> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|h| h.to_string())
        .map_err(|_| AuthError::Hashing)
}

/// Verifies a password against a stored PHC string. Returns `false` for a
/// mismatch and for a malformed stored hash; never panics.
pub fn verify_password(password: &str, phc: &str) -> bool {
    Argon2::default()
        .verify_password(password.as_bytes(), phc)
        .is_ok()
}

/// A dummy hash verified when an account does not exist, so that login
/// latency does not reveal whether an email address is registered.
pub fn equalize_timing(password: &str) {
    static DUMMY: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    if let Some(dummy) = DUMMY.get_or_init(|| hash_password("quorum-timing-equalizer").ok()) {
        let _ = verify_password(password, dummy);
    }
}

/// An opaque bearer token. `secret` is given to the client exactly once;
/// `digest` is what gets stored.
pub struct IssuedToken {
    pub secret: String,
    pub digest: [u8; 32],
}

impl std::fmt::Debug for IssuedToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssuedToken")
            .field("secret", &"<redacted>")
            .finish()
    }
}

/// Issues a new 256-bit random token.
pub fn issue_token() -> Result<IssuedToken, AuthError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| AuthError::Random)?;
    let secret = URL_SAFE_NO_PAD.encode(bytes);
    let digest = token_digest(&secret);
    Ok(IssuedToken { secret, digest })
}

/// SHA-256 digest of a presented token, used for lookup.
pub fn token_digest(secret: &str) -> [u8; 32] {
    Sha256::digest(secret.as_bytes()).into()
}

/// Structural check before hashing a presented token: rejects oversized or
/// non-base64url input cheaply.
pub fn plausible_token(secret: &str) -> bool {
    secret.len() == 43
        && secret
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Constant-time comparison for CSRF tokens and similar values.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && bool::from(a.as_bytes().ct_eq(b.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hash_round_trip() {
        let hash = hash_password("correct horse battery staple").expect("hash");
        assert!(hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert!(verify_password("correct horse battery staple", &hash));
        assert!(!verify_password("correct horse battery stapler", &hash));
    }

    #[test]
    fn hashes_are_salted() {
        let a = hash_password("same password here").expect("hash");
        let b = hash_password("same password here").expect("hash");
        assert_ne!(a, b);
    }

    #[test]
    fn malformed_hash_does_not_verify() {
        assert!(!verify_password("anything", "not-a-phc-string"));
        assert!(!verify_password("anything", ""));
        equalize_timing("anything");
    }

    #[test]
    fn tokens_are_unique_and_digest_matches() {
        let a = issue_token().expect("token");
        let b = issue_token().expect("token");
        assert_ne!(a.secret, b.secret);
        assert!(plausible_token(&a.secret));
        assert_eq!(token_digest(&a.secret), a.digest);
        assert!(!format!("{a:?}").contains(&a.secret));
    }

    #[test]
    fn sha256_known_answer() {
        // FIPS 180-2 test vector for "abc".
        let d = token_digest("abc");
        let hex: String = d.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn token_plausibility() {
        assert!(!plausible_token(""));
        assert!(!plausible_token(&"a".repeat(44)));
        assert!(!plausible_token(&format!("{}=", "a".repeat(42))));
    }

    #[test]
    fn constant_time_comparison() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "abcd"));
    }
}
