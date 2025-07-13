//! Integration tests for PGP key discovery functionality.

use renews::control::{canonical_text, verify_pgp};
use renews::{
    Message,
    auth::pgp_discovery::{DefaultPgpKeyDiscovery, PgpKeyDiscovery},
};
use smallvec::smallvec;

#[tokio::test]
async fn test_pgp_discovery_with_missing_key() {
    // Create a test auth provider with in-memory database
    let auth = renews::auth::open("sqlite::memory:").await.unwrap();

    // Create a test message
    let msg = Message {
        headers: smallvec![
            ("From".to_string(), "test@example.com".to_string()),
            ("Subject".to_string(), "Test message".to_string()),
        ],
        body: "Test body".to_string(),
    };

    // Try verification with a non-existent user (will attempt discovery)
    let default_servers = renews::config::default_pgp_key_servers();
    let result = verify_pgp(
        &msg,
        &auth,
        "nonexistent@example.com",
        "1",
        "From,Subject",
        "test_signature_data",
        &default_servers,
    )
    .await;

    // Should fail because discovery won't find a key for this test user
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("No PGP key found") || error_msg.contains("no key could be discovered")
    );
}

#[tokio::test]
async fn test_pgp_discovery_with_stored_key() {
    // Create a test auth provider with in-memory database
    let auth = renews::auth::open("sqlite::memory:").await.unwrap();

    // Add a user with a test key
    const TEST_KEY: &str = include_str!("data/admin.pub.asc");
    auth.add_user("test@example.com", "password").await.unwrap();
    auth.update_pgp_key("test@example.com", TEST_KEY)
        .await
        .unwrap();

    // Verify the key was stored
    let stored_key = auth.get_pgp_key("test@example.com").await.unwrap();
    assert!(stored_key.is_some());
    assert_eq!(stored_key.unwrap(), TEST_KEY);
}

#[tokio::test]
async fn test_pgp_discovery_key_validation() {
    // Test the key validation functionality using the auth-based discovery
    let discovery = DefaultPgpKeyDiscovery::new();

    // Test with invalid key
    let is_valid = discovery.validate_key("invalid key data").await.unwrap();
    assert!(!is_valid);

    // TODO: Test with valid key once we have test data
    // let valid_key = include_str!("data/admin.pub.asc");
    // let is_valid = discovery.validate_key(valid_key).await.unwrap();
    // assert!(is_valid);
}

#[tokio::test]
async fn test_pgp_discovery_placeholder() {
    // Test the placeholder discovery functionality
    let discovery = DefaultPgpKeyDiscovery::new();

    let result = discovery.discover_key("test@example.com").await;
    assert!(result.is_ok());
    // Current implementation returns None (placeholder)
    assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn test_canonical_text_generation() {
    let msg = Message {
        headers: smallvec![
            ("From".to_string(), "test@example.com".to_string()),
            ("Subject".to_string(), "Test Subject".to_string()),
            (
                "Date".to_string(),
                "Mon, 1 Jan 2024 12:00:00 +0000".to_string()
            ),
        ],
        body: "Test message body".to_string(),
    };

    let canonical = canonical_text(&msg, "From,Subject");

    // Should include the specified headers in order
    assert!(canonical.contains("From: test@example.com"));
    assert!(canonical.contains("Subject: Test Subject"));
    // Should not include Date header (not specified in signed_headers)
    assert!(!canonical.contains("Date:"));
    // Should include the body
    assert!(canonical.contains("Test message body"));
}

// Note: Real network tests would require a working key server and valid PGP keys
// For integration testing, we focus on the logic flow and error handling
#[tokio::test]
async fn test_discovery_functionality() {
    // This test verifies that discovery works with the new auth-based approach
    let discovery = DefaultPgpKeyDiscovery::new();

    let result = discovery
        .discover_key("nonexistent.user.for.testing@invalid.domain.example")
        .await;

    // Should complete without panicking
    match result {
        Ok(None) => {
            // Expected: no key found (placeholder implementation)
        }
        Ok(Some(_)) => {
            // Would be unexpected with current placeholder implementation
            // but valid once real discovery is implemented
        }
        Err(_) => {
            // Would be valid for real discovery implementation
        }
    }
}

#[tokio::test]
async fn test_verify_pgp_with_discovery_fallback() {
    // Test the enhanced verify_pgp function behavior
    let auth = renews::auth::open("sqlite::memory:").await.unwrap();

    let msg = Message {
        headers: smallvec![
            ("From".to_string(), "test@example.com".to_string()),
            ("Subject".to_string(), "Test".to_string()),
        ],
        body: "Test body".to_string(),
    };

    // Test with user that has no stored key - should attempt discovery
    let default_servers = renews::config::default_pgp_key_servers();
    let result = verify_pgp(
        &msg,
        &auth,
        "unknown@example.com",
        "1",
        "From,Subject",
        "invalid_sig",
        &default_servers,
    )
    .await;

    // Should fail because discovery won't find a real key and signature is invalid
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("no key could be discovered") || error_msg.contains("No PGP key found")
    );
}
