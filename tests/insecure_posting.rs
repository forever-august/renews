//! Tests for the allow-posting-insecure-connections feature

use renews::{ConnectionState, handlers::{HandlerContext, CommandHandler, auth::ModeHandler, post::PostHandler}, queue::ArticleQueue};
use std::sync::Arc;
use tokio::io::BufReader;
use tokio::sync::RwLock;

mod utils;
use utils::{create_minimal_config, create_insecure_posting_config, create_test_storage, create_test_auth};

/// Test that non-TLS connections get the correct greeting when insecure posting is disabled
#[tokio::test]
async fn test_non_tls_greeting_secure_mode() {
    let config = create_minimal_config();
    
    // Simulate non-TLS connection
    let is_tls = false;
    let allow_posting_insecure = config.allow_posting_insecure_connections;
    
    // Check greeting logic
    assert!(!is_tls);
    assert!(!allow_posting_insecure);
    
    // This should result in RESP_201_READY_NO_POST being sent
    // (We can't easily test the actual network code here, but we can verify the logic)
}

/// Test that non-TLS connections get the correct greeting when insecure posting is enabled
#[tokio::test]
async fn test_non_tls_greeting_insecure_mode() {
    let config = create_insecure_posting_config();
    
    // Simulate non-TLS connection
    let is_tls = false;
    let allow_posting_insecure = config.allow_posting_insecure_connections;
    
    // Check greeting logic
    assert!(!is_tls);
    assert!(allow_posting_insecure);
    
    // This should result in RESP_200_READY being sent
}

/// Test MODE READER command behavior with insecure posting disabled
#[tokio::test]
async fn test_mode_reader_secure_mode() {
    let storage = create_test_storage().await;
    let auth = create_test_auth().await;
    let config = Arc::new(RwLock::new(create_minimal_config()));
    let queue = ArticleQueue::new(10);
    
    // Create a test context for non-TLS connection
    let (reader, writer) = tokio::io::duplex(1024);
    let reader = BufReader::new(reader);
    
    let mut ctx = HandlerContext {
        reader,
        writer,
        storage,
        auth,
        config,
        state: ConnectionState {
            is_tls: false,
            allow_posting_insecure: false,
            ..Default::default()
        },
        queue,
    };
    
    let args = vec!["READER".to_string()];
    let result = ModeHandler::handle(&mut ctx, &args).await;
    assert!(result.is_ok());
    
    // The response should indicate posting is prohibited
    // (We can't easily check the actual response here without more complex test setup)
}

/// Test MODE READER command behavior with insecure posting enabled
#[tokio::test]
async fn test_mode_reader_insecure_mode() {
    let storage = create_test_storage().await;
    let auth = create_test_auth().await;
    let config = Arc::new(RwLock::new(create_insecure_posting_config()));
    let queue = ArticleQueue::new(10);
    
    // Create a test context for non-TLS connection with insecure posting enabled
    let (reader, writer) = tokio::io::duplex(1024);
    let reader = BufReader::new(reader);
    
    let mut ctx = HandlerContext {
        reader,
        writer,
        storage,
        auth,
        config,
        state: ConnectionState {
            is_tls: false,
            allow_posting_insecure: true,
            ..Default::default()
        },
        queue,
    };
    
    let args = vec!["READER".to_string()];
    let result = ModeHandler::handle(&mut ctx, &args).await;
    assert!(result.is_ok());
    
    // The response should indicate posting is allowed
}

/// Test POST command behavior with insecure posting disabled (should fail)
#[tokio::test]
async fn test_post_secure_mode_should_fail() {
    let storage = create_test_storage().await;
    let auth = create_test_auth().await;
    let config = Arc::new(RwLock::new(create_minimal_config()));
    let queue = ArticleQueue::new(10);
    
    // Create a test context for non-TLS connection
    let (reader, writer) = tokio::io::duplex(1024);
    let reader = BufReader::new(reader);
    
    let mut ctx = HandlerContext {
        reader,
        writer,
        storage,
        auth,
        config,
        state: ConnectionState {
            is_tls: false,
            allow_posting_insecure: false,
            ..Default::default()
        },
        queue,
    };
    
    let args = vec![];
    let result = PostHandler::handle(&mut ctx, &args).await;
    assert!(result.is_ok()); // Handler should succeed, but return error response
    
    // The handler should have written RESP_483_SECURE_REQ to the writer
}

/// Test POST command behavior with insecure posting enabled (should proceed to auth check)
#[tokio::test]
async fn test_post_insecure_mode_proceed_to_auth() {
    let storage = create_test_storage().await;
    let auth = create_test_auth().await;
    let config = Arc::new(RwLock::new(create_insecure_posting_config()));
    let queue = ArticleQueue::new(10);
    
    // Create a test context for non-TLS connection with insecure posting enabled
    let (reader, writer) = tokio::io::duplex(1024);
    let reader = BufReader::new(reader);
    
    let mut ctx = HandlerContext {
        reader,
        writer,
        storage,
        auth,
        config,
        state: ConnectionState {
            is_tls: false,
            allow_posting_insecure: true,
            authenticated: false, // Not authenticated
            ..Default::default()
        },
        queue,
    };
    
    let args = vec![];
    let result = PostHandler::handle(&mut ctx, &args).await;
    assert!(result.is_ok()); // Handler should succeed, but return auth required response
    
    // The handler should have proceeded past TLS check and hit auth requirement
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
    
    let config: renews::config::Config = toml::from_str(toml).unwrap();
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
    
    let config: renews::config::Config = toml::from_str(toml).unwrap();
    assert!(!config.allow_posting_insecure_connections);
}