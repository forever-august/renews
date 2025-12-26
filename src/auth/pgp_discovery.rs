//! PGP key discovery functionality integrated with authentication.
//!
//! This module provides PGP key discovery capabilities that work with the
//! authentication system to automatically retrieve and validate public keys
//! from key servers when needed.

use anyhow::Result;
use async_trait::async_trait;
use pgp::native::{Deserializable, SignedPublicKey};

/// Trait for PGP key discovery from various sources.
#[async_trait]
pub trait PgpKeyDiscovery: Send + Sync {
    /// Discover a PGP public key for the given user identifier.
    ///
    /// This method attempts to find a PGP public key for the specified user
    /// by querying configured key sources (key servers, local keyring, etc.).
    ///
    /// # Arguments
    ///
    /// * `user` - The user identifier (typically an email address)
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(key_text))` if a valid key is found,
    /// `Ok(None)` if no key is found,
    /// or `Err(...)` if there's an error during the discovery process.
    async fn discover_key(&self, user: &str) -> Result<Option<String>>;

    /// Validate that a key block can be parsed as a valid PGP key.
    async fn validate_key(&self, key_text: &str) -> Result<bool>;
}

/// Default implementation of PGP key discovery using pgp-lib's key discovery features.
pub struct DefaultPgpKeyDiscovery {
    key_servers: Vec<String>,
}

impl DefaultPgpKeyDiscovery {
    /// Create a new instance with default key servers.
    pub fn new() -> Self {
        Self {
            key_servers: vec![
                "hkps://keys.openpgp.org/pks/lookup?op=get&search=<email>".to_string(),
                "hkps://pgp.mit.edu/pks/lookup?op=get&search=<email>".to_string(),
                "hkps://keyserver.ubuntu.com/pks/lookup?op=get&search=<email>".to_string(),
            ],
        }
    }

    /// Create a new instance with custom key servers.
    pub fn with_key_servers(key_servers: Vec<String>) -> Self {
        Self { key_servers }
    }
}

impl Default for DefaultPgpKeyDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PgpKeyDiscovery for DefaultPgpKeyDiscovery {
    async fn discover_key(&self, user: &str) -> Result<Option<String>> {
        tracing::debug!("Attempting key discovery");

        // Use pgp-lib's HTTP key discovery functionality
        match pgp::http::get_one(user.to_string(), self.key_servers.clone()).await {
            Ok(public_key) => {
                // Convert the SignedPublicKey to armored string format
                match public_key.to_armored_string(None) {
                    Ok(armored_key) => {
                        tracing::debug!("Successfully discovered key");
                        Ok(Some(armored_key))
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to serialize discovered key");
                        Err(anyhow::anyhow!("Failed to serialize discovered key: {e}"))
                    }
                }
            }
            Err(e) => {
                tracing::debug!(error = %e, "No key found during discovery");
                Ok(None)
            }
        }
    }

    async fn validate_key(&self, key_text: &str) -> Result<bool> {
        match SignedPublicKey::from_string(key_text) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

/// Create a default PGP key discovery instance.
pub fn create_default_discovery() -> Box<dyn PgpKeyDiscovery> {
    Box::new(DefaultPgpKeyDiscovery::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_validate_invalid_key() {
        let discovery = DefaultPgpKeyDiscovery::new();
        let result = discovery.validate_key("invalid key").await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_discover_key_returns_none_for_invalid_email() {
        let discovery = DefaultPgpKeyDiscovery::new();
        let result = discovery
            .discover_key("invalid-email-that-should-not-exist-anywhere@nonexistent.invalid")
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_custom_key_servers() {
        let custom_servers =
            vec!["hkps://custom.server.com/pks/lookup?op=get&search=<email>".to_string()];
        let discovery = DefaultPgpKeyDiscovery::with_key_servers(custom_servers.clone());
        assert_eq!(discovery.key_servers, custom_servers);
    }

    #[tokio::test]
    async fn test_key_servers_from_config() {
        let config_servers = vec![
            "hkps://config.example.com/pks/lookup?op=get&search=<email>".to_string(),
            "hkps://another.example.com/pks/lookup?op=get&search=<email>".to_string(),
        ];
        let discovery = DefaultPgpKeyDiscovery::with_key_servers(config_servers.clone());
        assert_eq!(discovery.key_servers, config_servers);
        assert_ne!(
            discovery.key_servers,
            crate::config::default_pgp_key_servers()
        );
    }

    #[tokio::test]
    async fn test_default_key_servers() {
        let discovery = DefaultPgpKeyDiscovery::default();
        assert!(!discovery.key_servers.is_empty());
        assert!(
            discovery
                .key_servers
                .iter()
                .any(|s| s.contains("keys.openpgp.org"))
        );
    }
}
