//! Tests for posting and authentication security features

use renews::{config::Config, session::Session};

mod utils;
use utils::create_minimal_config;

/// Test that the session logic works correctly for secure mode (default config)
#[tokio::test]
async fn test_session_secure_mode() {
    let config = create_minimal_config();
    assert!(!config.allow_auth_insecure_connections);
    assert!(!config.allow_anonymous_posting);

    // Simulate non-TLS connection with default config
    let session = Session::new(
        false, // is_tls
        config.allow_auth_insecure_connections,
        config.allow_anonymous_posting,
    );

    // In secure mode, non-TLS should not allow auth
    assert!(!session.is_tls());
    assert!(!session.can_post()); // Not authenticated, no anonymous posting
    assert!(!session.can_authenticate());
}

/// Test that anonymous posting works when enabled
#[tokio::test]
async fn test_session_anonymous_posting() {
    let mut config = create_minimal_config();
    config.allow_anonymous_posting = true;

    // Simulate TLS connection with anonymous posting enabled
    let session = Session::new(
        true, // is_tls
        config.allow_auth_insecure_connections,
        config.allow_anonymous_posting,
    );

    // With TLS and anonymous posting, can_post() should be true without auth
    assert!(session.is_tls());
    assert!(session.can_post());
    assert!(!session.is_authenticated());
}

/// Test that anonymous posting on non-TLS connections works when enabled
#[tokio::test]
async fn test_session_anonymous_posting_non_tls() {
    let mut config = create_minimal_config();
    config.allow_anonymous_posting = true;

    // Simulate non-TLS connection with anonymous posting enabled
    let session = Session::new(
        false, // is_tls
        config.allow_auth_insecure_connections,
        config.allow_anonymous_posting,
    );

    // With anonymous posting enabled, can_post() should be true regardless of TLS
    assert!(!session.is_tls());
    assert!(session.can_post());
    assert!(!session.is_authenticated());
}

/// Test that TLS connections can authenticate but need auth to post (by default)
#[tokio::test]
async fn test_tls_requires_auth_to_post() {
    let config = create_minimal_config();
    assert!(!config.allow_anonymous_posting);

    // Simulate TLS connection
    let mut session = Session::new(
        true, // is_tls
        config.allow_auth_insecure_connections,
        config.allow_anonymous_posting,
    );

    // TLS allows authentication
    assert!(session.is_tls());
    assert!(session.can_authenticate());

    // But cannot post without authentication (anonymous posting disabled)
    assert!(!session.can_post());

    // After authentication, can post
    session.authenticate("testuser".to_string());
    assert!(session.can_post());
}

/// Test that insecure auth connections work when enabled
#[tokio::test]
async fn test_session_insecure_auth() {
    let mut config = create_minimal_config();
    config.allow_auth_insecure_connections = true;

    // Simulate non-TLS connection with insecure auth enabled
    let mut session = Session::new(
        false, // is_tls
        config.allow_auth_insecure_connections,
        config.allow_anonymous_posting,
    );

    // With insecure auth enabled, can authenticate on non-TLS
    assert!(!session.is_tls());
    assert!(session.can_authenticate());
    assert!(!session.can_post()); // Still need to authenticate

    // After authentication, can post
    session.authenticate("testuser".to_string());
    assert!(session.can_post());
}

/// Test connection greeting logic - default config (no anonymous posting)
#[tokio::test]
async fn test_greeting_logic_default() {
    let config = create_minimal_config();

    // Non-TLS connection with default config
    let session = Session::new(
        false, // is_tls
        config.allow_auth_insecure_connections,
        config.allow_anonymous_posting,
    );

    // Greeting should indicate no posting (201)
    assert!(!session.can_post());

    // TLS connection with default config
    let session_tls = Session::new(
        true, // is_tls
        config.allow_auth_insecure_connections,
        config.allow_anonymous_posting,
    );

    // Still no posting until authenticated (201)
    assert!(!session_tls.can_post());
}

/// Test connection greeting logic - anonymous posting enabled
#[tokio::test]
async fn test_greeting_logic_anonymous_posting() {
    let mut config = create_minimal_config();
    config.allow_anonymous_posting = true;

    // TLS connection with anonymous posting
    let session = Session::new(
        true, // is_tls
        config.allow_auth_insecure_connections,
        config.allow_anonymous_posting,
    );

    // Greeting should indicate posting allowed (200)
    assert!(session.can_post());
}

/// Test configuration parsing includes all security fields
#[test]
fn test_config_parsing_includes_security_flags() {
    let toml = r#"
addr = ":119"
db_path = "sqlite:///:memory:"
auth_db_path = "sqlite:///:memory:"
peer_db_path = "sqlite:///:memory:"
allow_auth_insecure_connections = true
allow_anonymous_posting = true
"#;

    let config: Config = toml::from_str(toml).unwrap();
    assert!(config.allow_auth_insecure_connections);
    assert!(config.allow_anonymous_posting);
}

/// Test that all flags default to false
#[test]
fn test_config_flags_default_to_false() {
    let toml = r#"
addr = ":119"
db_path = "sqlite:///:memory:"
auth_db_path = "sqlite:///:memory:"
peer_db_path = "sqlite:///:memory:"
"#;

    let config: Config = toml::from_str(toml).unwrap();
    assert!(!config.allow_auth_insecure_connections);
    assert!(!config.allow_anonymous_posting);
}

/// Test that config update runtime preserves all flags
#[test]
fn test_config_update_runtime_preserves_flags() {
    let mut config1 = create_minimal_config();
    let mut config2 = create_minimal_config();
    config2.allow_auth_insecure_connections = true;
    config2.allow_anonymous_posting = true;

    assert!(!config1.allow_auth_insecure_connections);
    assert!(!config1.allow_anonymous_posting);

    // Update config1 with config2
    config1.update_runtime(config2.clone());

    // All flags should be updated
    assert!(config1.allow_auth_insecure_connections);
    assert!(config1.allow_anonymous_posting);
}
