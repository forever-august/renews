//! PGP key discovery from key servers.

use pgp::composed::{Deserializable, SignedPublicKey};
use std::error::Error;

/// Default key server URLs to try for key discovery.
const DEFAULT_KEY_SERVERS: &[&str] = &[
    "https://keys.openpgp.org",
    "https://pgp.mit.edu",
    "https://keyserver.ubuntu.com",
];

/// Discover a PGP public key for the given user from key servers.
///
/// This function attempts to find a PGP public key for the specified user
/// by querying multiple key servers. It returns the first valid key found.
///
/// # Arguments
///
/// * `user` - The user identifier (typically an email address)
///
/// # Returns
///
/// Returns `Ok(Some(key_text))` if a valid key is found,
/// `Ok(None)` if no key is found on any server,
/// or `Err(...)` if there's an error during the discovery process.
///
/// # Errors
///
/// Returns an error if there are issues with network communication
/// or key parsing during the discovery process.
pub async fn discover_pgp_key(user: &str) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
    let client = reqwest::Client::new();
    
    for server in DEFAULT_KEY_SERVERS {
        match try_discover_from_server(&client, server, user).await {
            Ok(Some(key)) => return Ok(Some(key)),
            Ok(None) => continue, // Try next server
            Err(e) => {
                // Log error but continue trying other servers
                tracing::debug!("Failed to query key server {}: {}", server, e);
                continue;
            }
        }
    }
    
    Ok(None)
}

/// Try to discover a key from a specific key server.
async fn try_discover_from_server(
    client: &reqwest::Client,
    server: &str,
    user: &str,
) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
    // Try HKP (HTTP Keyserver Protocol) lookup first
    let url = format!("{}/pks/lookup?op=get&search={}", server, urlencoding::encode(user));
    
    let response = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?;
    
    if !response.status().is_success() {
        return Ok(None);
    }
    
    let text = response.text().await?;
    
    // Look for PGP key block in the response
    if let Some(key_block) = extract_pgp_key_block(&text) {
        // Validate that this is a parseable PGP key
        if validate_pgp_key(&key_block).await? {
            return Ok(Some(key_block));
        }
    }
    
    Ok(None)
}

/// Extract a PGP key block from HTML or text response.
pub fn extract_pgp_key_block(text: &str) -> Option<String> {
    // Look for PGP PUBLIC KEY BLOCK
    let start_marker = "-----BEGIN PGP PUBLIC KEY BLOCK-----";
    let end_marker = "-----END PGP PUBLIC KEY BLOCK-----";
    
    if let Some(start) = text.find(start_marker) {
        if let Some(end) = text[start..].find(end_marker) {
            let key_block = &text[start..start + end + end_marker.len()];
            return Some(key_block.to_string());
        }
    }
    
    None
}

/// Validate that a key block can be parsed as a valid PGP key.
async fn validate_pgp_key(key_text: &str) -> Result<bool, Box<dyn Error + Send + Sync>> {
    match SignedPublicKey::from_string(key_text) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_extract_pgp_key_block() {
        let html_response = r#"
            <html>
            <body>
            <pre>
-----BEGIN PGP PUBLIC KEY BLOCK-----

mQENBF2QxBABCAC7...
-----END PGP PUBLIC KEY BLOCK-----
            </pre>
            </body>
            </html>
        "#;
        
        let key_block = extract_pgp_key_block(html_response);
        assert!(key_block.is_some());
        assert!(key_block.unwrap().contains("-----BEGIN PGP PUBLIC KEY BLOCK-----"));
    }
    
    #[test]
    fn test_extract_pgp_key_block_not_found() {
        let text = "No PGP key here";
        let key_block = extract_pgp_key_block(text);
        assert!(key_block.is_none());
    }
    
    #[tokio::test]
    async fn test_validate_invalid_key() {
        let result = validate_pgp_key("invalid key").await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }
}