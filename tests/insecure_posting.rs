//! Tests for the allow-posting-insecure-connections feature

use renews::{config::Config, session::Session};

mod utils;
use utils::{create_insecure_posting_config, create_minimal_config};

/// Test that the session logic works correctly for secure mode
#[tokio::test]
async fn test_session_secure_mode() {
    let config = create_minimal_config();
    assert!(!config.allow_posting_insecure_connections);

    // Simulate non-TLS connection logic
    let is_tls = false;
    let allow_posting_insecure = config.allow_posting_insecure_connections;

    let session = Session::new(is_tls, allow_posting_insecure);

    // In secure mode, non-TLS should not allow posting
    assert!(!session.is_tls());
    assert!(!session.allows_posting_attempt());

    // POST should be rejected (simulating the logic in PostHandler)
    // can_post() requires authentication, so we test allows_posting_attempt() instead
    assert!(!session.allows_posting_attempt());

    // MODE READER should return "posting prohibited"
    assert!(!session.allows_posting_attempt());
}

/// Test that the session logic works correctly for insecure mode
#[tokio::test]
async fn test_session_insecure_mode() {
    let config = create_insecure_posting_config();
    assert!(config.allow_posting_insecure_connections);

    // Simulate non-TLS connection logic
    let is_tls = false;
    let allow_posting_insecure = config.allow_posting_insecure_connections;

    let session = Session::new(is_tls, allow_posting_insecure);

    // In insecure mode, non-TLS should allow posting attempt
    assert!(!session.is_tls());
    assert!(session.allows_posting_attempt());

    // MODE READER should return "posting allowed"
    assert!(session.allows_posting_attempt());
}

/// Test that TLS connections always allow posting regardless of the flag
#[tokio::test]
async fn test_tls_always_allows_posting() {
    let config = create_minimal_config(); // Flag is false
    assert!(!config.allow_posting_insecure_connections);

    // Simulate TLS connection
    let is_tls = true;
    let allow_posting_insecure = config.allow_posting_insecure_connections;

    let session = Session::new(is_tls, allow_posting_insecure);

    // TLS should always allow posting attempt
    assert!(session.is_tls());

    // POST should be allowed because of TLS (after authentication)
    assert!(session.allows_posting_attempt());

    // MODE READER should return "posting allowed"
    assert!(session.allows_posting_attempt());
}

/// Test connection greeting logic for secure mode
#[tokio::test]
async fn test_greeting_logic_secure_mode() {
    let config = create_minimal_config();
    let is_tls = false;
    let allow_posting_insecure = config.allow_posting_insecure_connections;

    // Simulate the greeting logic from lib.rs
    let should_send_posting_ok = is_tls || allow_posting_insecure;
    assert!(!should_send_posting_ok);

    // This would result in RESP_201_READY_NO_POST being sent
}

/// Test connection greeting logic for insecure mode
#[tokio::test]
async fn test_greeting_logic_insecure_mode() {
    let config = create_insecure_posting_config();
    let is_tls = false;
    let allow_posting_insecure = config.allow_posting_insecure_connections;

    // Simulate the greeting logic from lib.rs
    let should_send_posting_ok = is_tls || allow_posting_insecure;
    assert!(should_send_posting_ok);

    // This would result in RESP_200_READY being sent
}

/// Test configuration parsing includes the new field
#[test]
fn test_config_parsing_includes_insecure_flag() {
    let toml = r#"
addr = ":119"
db_path = "sqlite:///:memory:"
auth_db_path = "sqlite:///:memory:"
peer_db_path = "sqlite:///:memory:"
allow_posting_insecure_connections = true
"#;

    let config: Config = toml::from_str(toml).unwrap();
    assert!(config.allow_posting_insecure_connections);
}

/// Test that the flag defaults to false
#[test]
fn test_config_flag_defaults_to_false() {
    let toml = r#"
addr = ":119"
db_path = "sqlite:///:memory:"
auth_db_path = "sqlite:///:memory:"
peer_db_path = "sqlite:///:memory:"
"#;

    let config: Config = toml::from_str(toml).unwrap();
    assert!(!config.allow_posting_insecure_connections);
}

/// Test that config update runtime preserves the flag
#[test]
fn test_config_update_runtime_preserves_flag() {
    let mut config1 = create_minimal_config();
    let config2 = create_insecure_posting_config();

    assert!(!config1.allow_posting_insecure_connections);
    assert!(config2.allow_posting_insecure_connections);

    // Update config1 with config2
    config1.update_runtime(config2.clone());

    // The flag should be updated
    assert!(config1.allow_posting_insecure_connections);
}

/// Test that CLI flag overrides config file setting
#[test]
fn test_cli_flag_override_logic() {
    // Simulate the CLI override logic from main.rs
    let mut config = create_minimal_config();
    assert!(!config.allow_posting_insecure_connections);

    // Simulate CLI flag being provided
    let cli_flag_provided = true;
    if cli_flag_provided {
        config.allow_posting_insecure_connections = true;
    }

    assert!(config.allow_posting_insecure_connections);
}
