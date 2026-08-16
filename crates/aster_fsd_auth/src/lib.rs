//! Authentication ports and password primitives for the network core.
//!
//! Passwords remain confined to the shortest decode-to-authentication path.
//! Expensive Argon2 verification runs on Tokio's blocking pool so a login does
//! not stall packet dispatch for unrelated connections.

#![cfg_attr(
    not(test),
    deny(
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented,
        clippy::unreachable,
        clippy::unwrap_used
    )
)]

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use aster_fsd_model::{AtcRating, AuthenticatedIdentity, PilotRating};
use async_trait::async_trait;
use thiserror::Error;

/// Authentication failures exposed to the network core.
#[derive(Debug, Error)]
pub enum AuthError {
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("client software is not authorized")]
    ClientNotAuthorized,
    #[error("identity is suspended")]
    Suspended,
    #[error("authentication backend failed: {0}")]
    Backend(String),
    #[error("password hash is invalid")]
    InvalidPasswordHash,
}

/// Port implemented by persistent or external identity providers.
#[async_trait]
pub trait Authenticator: Send + Sync {
    /// Verifies that a client-software identifier may enter the network.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::ClientNotAuthorized`] for an unknown or disabled
    /// client and [`AuthError::Backend`] when the provider fails.
    async fn authorize_client(&self, client_id: &str) -> Result<(), AuthError>;

    /// Authenticates one network identity without exposing credential details.
    ///
    /// # Errors
    ///
    /// Returns an [`AuthError`] for invalid credentials, suspension, malformed
    /// password data or provider failure.
    async fn authenticate(
        &self,
        network_id: &str,
        password: &str,
    ) -> Result<AuthenticatedIdentity, AuthError>;
}

/// Hashes a password with a new random Argon2 salt.
///
/// # Errors
///
/// Returns [`AuthError::InvalidPasswordHash`] when salt encoding or Argon2
/// hashing fails.
pub fn hash_password(password: &str) -> Result<String, AuthError> {
    let salt = SaltString::encode_b64(&rand::random::<[u8; 16]>())
        .map_err(|_| AuthError::InvalidPasswordHash)?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| AuthError::InvalidPasswordHash)
}

/// Verifies a password against an encoded Argon2 hash on the blocking pool.
///
/// # Errors
///
/// Returns [`AuthError::InvalidPasswordHash`] for malformed hashes and
/// [`AuthError::Backend`] when the blocking task does not complete normally.
pub async fn verify_password(password: String, hash: String) -> Result<bool, AuthError> {
    tokio::task::spawn_blocking(move || {
        let parsed = PasswordHash::new(&hash).map_err(|_| AuthError::InvalidPasswordHash)?;
        match Argon2::default().verify_password(password.as_bytes(), &parsed) {
            Ok(()) => Ok(true),
            Err(argon2::password_hash::Error::Password) => Ok(false),
            Err(_) => Err(AuthError::InvalidPasswordHash),
        }
    })
    .await
    .map_err(|error| AuthError::Backend(error.to_string()))?
}

/// Deterministic authenticator used only by unit and transport tests.
#[derive(Debug, Default)]
pub struct AllowAllAuthenticator;

#[async_trait]
impl Authenticator for AllowAllAuthenticator {
    async fn authorize_client(&self, _client_id: &str) -> Result<(), AuthError> {
        Ok(())
    }

    async fn authenticate(
        &self,
        network_id: &str,
        _password: &str,
    ) -> Result<AuthenticatedIdentity, AuthError> {
        Ok(AuthenticatedIdentity {
            network_id: network_id.to_string(),
            real_name: network_id.to_string(),
            atc_rating: AtcRating::Administrator,
            pilot_rating: PilotRating::FlightExaminer,
            suspended: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn password_verification_runs_through_blocking_boundary() {
        let hash = hash_password("correct horse").unwrap();
        assert!(
            verify_password("correct horse".to_string(), hash.clone())
                .await
                .unwrap()
        );
        assert!(!verify_password("wrong".to_string(), hash).await.unwrap());
    }
}
