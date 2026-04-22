//! # Peat Credentials
//!
//! Backend-agnostic credential container used for peer authentication.
//!
//! ## Environment Variables
//!
//! - `PEAT_APP_ID` — application / formation identifier (required)
//! - `PEAT_SECRET_KEY` — shared secret key, base64 encoded (optional)
//!   - `PEAT_SHARED_KEY` is accepted as an alias
//!
//! ## Usage
//!
//! ```ignore
//! use peat_protocol::credentials::PeatCredentials;
//!
//! let creds = PeatCredentials::from_env()?;
//! println!("App ID: {}", creds.app_id());
//! println!("Has secret key: {}", creds.secret_key().is_some());
//! ```

use std::env;

/// Peat credentials for backend authentication.
#[derive(Debug, Clone)]
pub struct PeatCredentials {
    /// Application / formation identifier (required).
    app_id: String,
    /// Shared secret key, base64 encoded (optional).
    secret_key: Option<String>,
}

impl PeatCredentials {
    /// Create credentials from explicit values.
    pub fn new(app_id: String, secret_key: Option<String>) -> Self {
        Self { app_id, secret_key }
    }

    /// Load credentials from environment variables.
    ///
    /// Reads `PEAT_APP_ID` (required) and `PEAT_SECRET_KEY` (or
    /// `PEAT_SHARED_KEY` as an alias, optional).
    pub fn from_env() -> Result<Self, CredentialsError> {
        let app_id = env::var("PEAT_APP_ID")
            .ok()
            .filter(|v| !v.is_empty())
            .ok_or(CredentialsError::MissingAppId)?;

        let secret_key = env::var("PEAT_SECRET_KEY")
            .ok()
            .filter(|v| !v.is_empty())
            .or_else(|| env::var("PEAT_SHARED_KEY").ok().filter(|v| !v.is_empty()));

        Ok(Self { app_id, secret_key })
    }

    /// Load credentials from environment, returning `None` if not configured.
    pub fn try_from_env() -> Option<Self> {
        Self::from_env().ok()
    }

    /// Whether `PEAT_APP_ID` is configured in the environment.
    pub fn is_configured() -> bool {
        env::var("PEAT_APP_ID").ok().is_some_and(|v| !v.is_empty())
    }

    /// Application identifier.
    pub fn app_id(&self) -> &str {
        &self.app_id
    }

    /// Shared secret key, if configured.
    pub fn secret_key(&self) -> Option<&str> {
        self.secret_key.as_deref()
    }

    /// Whether a secret key is configured.
    pub fn has_secret_key(&self) -> bool {
        self.secret_key.is_some()
    }

    /// Get the secret key, or return an error if missing.
    pub fn require_secret_key(&self) -> Result<&str, CredentialsError> {
        self.secret_key
            .as_deref()
            .ok_or(CredentialsError::MissingSecretKey)
    }
}

/// Errors that can occur when loading credentials.
#[derive(Debug, Clone, thiserror::Error)]
pub enum CredentialsError {
    #[error("PEAT_APP_ID not set")]
    MissingAppId,

    #[error("PEAT_SECRET_KEY not set")]
    MissingSecretKey,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credentials_new() {
        let creds = PeatCredentials::new("test-app".to_string(), Some("secret".to_string()));

        assert_eq!(creds.app_id(), "test-app");
        assert_eq!(creds.secret_key(), Some("secret"));
    }

    #[test]
    fn test_credentials_without_secret() {
        let creds = PeatCredentials::new("test-app".to_string(), None);

        assert_eq!(creds.app_id(), "test-app");
        assert!(creds.secret_key().is_none());
        assert!(!creds.has_secret_key());
    }

    #[test]
    fn test_require_secret_key_present() {
        let creds = PeatCredentials::new("test-app".to_string(), Some("secret".to_string()));

        assert_eq!(creds.require_secret_key().unwrap(), "secret");
    }

    #[test]
    fn test_require_secret_key_missing() {
        let creds = PeatCredentials::new("test-app".to_string(), None);

        assert!(creds.require_secret_key().is_err());
    }
}
