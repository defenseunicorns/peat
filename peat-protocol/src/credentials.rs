//! # PEAT Credentials - Backend-Agnostic Credential Management
//!
//! Provides unified credential handling for PEAT backends, abstracting
//! away backend-specific naming (e.g., Ditto's app_id/shared_key).
//!
//! ## Environment Variables
//!
//! Primary (recommended):
//! - `PEAT_APP_ID` - Application identifier
//! - `PEAT_SECRET_KEY` - Shared secret key (base64 encoded)
//! - `PEAT_OFFLINE_TOKEN` - Offline license token (for Ditto backend)
//!
//! Legacy (fallback, for backwards compatibility):
//! - `DITTO_APP_ID` → `PEAT_APP_ID`
//! - `DITTO_SHARED_KEY` → `PEAT_SECRET_KEY`
//! - `DITTO_OFFLINE_TOKEN` → `PEAT_OFFLINE_TOKEN`
//!
//! ## Usage
//!
//! ```ignore
//! use peat_protocol::credentials::PeatCredentials;
//!
//! // Load from environment (tries PEAT_*, falls back to DITTO_*)
//! let creds = PeatCredentials::from_env()?;
//!
//! // Access credentials
//! println!("App ID: {}", creds.app_id());
//! println!("Has secret key: {}", creds.secret_key().is_some());
//! ```

use std::env;

/// PEAT credentials for backend authentication
///
/// Provides a backend-agnostic interface to credentials, with automatic
/// fallback to legacy Ditto environment variable names.
#[derive(Debug, Clone)]
pub struct PeatCredentials {
    /// Application identifier (required)
    app_id: String,
    /// Shared secret key (optional, base64 encoded)
    secret_key: Option<String>,
    /// Offline license token (optional, for Ditto backend)
    offline_token: Option<String>,
}

impl PeatCredentials {
    /// Create credentials from explicit values
    pub fn new(app_id: String, secret_key: Option<String>, offline_token: Option<String>) -> Self {
        Self {
            app_id,
            secret_key,
            offline_token,
        }
    }

    /// Load credentials from environment variables
    ///
    /// Tries PEAT_* variables first, falls back to DITTO_* for backwards compatibility.
    ///
    /// # Returns
    ///
    /// `Ok(PeatCredentials)` if app_id is found, error otherwise
    ///
    /// # Environment Variables
    ///
    /// | Primary | Legacy Fallback |
    /// |---------|-----------------|
    /// | PEAT_APP_ID | DITTO_APP_ID |
    /// | PEAT_SECRET_KEY | DITTO_SHARED_KEY |
    /// | PEAT_OFFLINE_TOKEN | DITTO_OFFLINE_TOKEN |
    pub fn from_env() -> Result<Self, CredentialsError> {
        let app_id = Self::get_env_with_fallback("PEAT_APP_ID", "DITTO_APP_ID")
            .ok_or(CredentialsError::MissingAppId)?;

        if app_id.is_empty() {
            return Err(CredentialsError::EmptyAppId);
        }

        // Check PEAT_SECRET_KEY first, then PEAT_SHARED_KEY, then DITTO_SHARED_KEY
        let secret_key = env::var("PEAT_SECRET_KEY")
            .ok()
            .filter(|v| !v.is_empty())
            .or_else(|| env::var("PEAT_SHARED_KEY").ok().filter(|v| !v.is_empty()))
            .or_else(|| env::var("DITTO_SHARED_KEY").ok().filter(|v| !v.is_empty()));
        let offline_token =
            Self::get_env_with_fallback("PEAT_OFFLINE_TOKEN", "DITTO_OFFLINE_TOKEN");

        Ok(Self {
            app_id,
            secret_key,
            offline_token,
        })
    }

    /// Load credentials from environment, returning None if not configured
    ///
    /// Unlike `from_env()`, this doesn't error on missing credentials.
    /// Useful for optional credential loading in tests.
    pub fn try_from_env() -> Option<Self> {
        Self::from_env().ok()
    }

    /// Check if credentials are available in the environment
    pub fn is_configured() -> bool {
        Self::get_env_with_fallback("PEAT_APP_ID", "DITTO_APP_ID").is_some()
    }

    /// Get the application ID
    pub fn app_id(&self) -> &str {
        &self.app_id
    }

    /// Get the secret key (if configured)
    pub fn secret_key(&self) -> Option<&str> {
        self.secret_key.as_deref()
    }

    /// Get the offline token (if configured)
    pub fn offline_token(&self) -> Option<&str> {
        self.offline_token.as_deref()
    }

    /// Check if secret key is configured
    pub fn has_secret_key(&self) -> bool {
        self.secret_key.is_some()
    }

    /// Check if offline token is configured
    pub fn has_offline_token(&self) -> bool {
        self.offline_token.is_some()
    }

    /// Helper: get env var with fallback to legacy name
    fn get_env_with_fallback(primary: &str, fallback: &str) -> Option<String> {
        env::var(primary)
            .ok()
            .filter(|v| !v.is_empty())
            .or_else(|| env::var(fallback).ok().filter(|v| !v.is_empty()))
    }

    /// Get required secret key or return error
    pub fn require_secret_key(&self) -> Result<&str, CredentialsError> {
        self.secret_key
            .as_deref()
            .ok_or(CredentialsError::MissingSecretKey)
    }

    /// Get required offline token or return error
    pub fn require_offline_token(&self) -> Result<&str, CredentialsError> {
        self.offline_token
            .as_deref()
            .ok_or(CredentialsError::MissingOfflineToken)
    }
}

/// Errors that can occur when loading credentials
#[derive(Debug, Clone, thiserror::Error)]
pub enum CredentialsError {
    /// PEAT_APP_ID (or DITTO_APP_ID) not set
    #[error("PEAT_APP_ID not set (also checked DITTO_APP_ID for backwards compatibility)")]
    MissingAppId,

    /// PEAT_APP_ID is empty
    #[error("PEAT_APP_ID cannot be empty")]
    EmptyAppId,

    /// PEAT_SECRET_KEY (or DITTO_SHARED_KEY) not set
    #[error("PEAT_SECRET_KEY not set (also checked DITTO_SHARED_KEY for backwards compatibility)")]
    MissingSecretKey,

    /// PEAT_OFFLINE_TOKEN (or DITTO_OFFLINE_TOKEN) not set
    #[error(
        "PEAT_OFFLINE_TOKEN not set (also checked DITTO_OFFLINE_TOKEN for backwards compatibility)"
    )]
    MissingOfflineToken,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests modify environment variables, so they should run in isolation
    // In practice, they're testing the fallback logic

    #[test]
    fn test_credentials_new() {
        let creds = PeatCredentials::new(
            "test-app".to_string(),
            Some("secret".to_string()),
            Some("token".to_string()),
        );

        assert_eq!(creds.app_id(), "test-app");
        assert_eq!(creds.secret_key(), Some("secret"));
        assert_eq!(creds.offline_token(), Some("token"));
    }

    #[test]
    fn test_credentials_without_optional() {
        let creds = PeatCredentials::new("test-app".to_string(), None, None);

        assert_eq!(creds.app_id(), "test-app");
        assert!(creds.secret_key().is_none());
        assert!(creds.offline_token().is_none());
        assert!(!creds.has_secret_key());
        assert!(!creds.has_offline_token());
    }

    #[test]
    fn test_require_secret_key_present() {
        let creds = PeatCredentials::new("test-app".to_string(), Some("secret".to_string()), None);

        assert!(creds.require_secret_key().is_ok());
        assert_eq!(creds.require_secret_key().unwrap(), "secret");
    }

    #[test]
    fn test_require_secret_key_missing() {
        let creds = PeatCredentials::new("test-app".to_string(), None, None);

        assert!(creds.require_secret_key().is_err());
    }

    #[test]
    fn test_get_env_with_fallback_primary() {
        // This test requires careful env var management
        // For now, just test the function signature works
        let result = PeatCredentials::get_env_with_fallback("NONEXISTENT_VAR", "ALSO_NONEXISTENT");
        assert!(result.is_none());
    }
}
