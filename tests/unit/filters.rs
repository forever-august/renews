use renews::filters::header::HeaderFilter;
use renews::filters::size::SizeFilter;
use renews::filters::{ArticleFilter, FilterChain};
use renews::{Message, config::Config};
use smallvec::smallvec;
use std::sync::Arc;

#[tokio::test]
async fn test_header_filter_valid() {
    let filter = HeaderFilter;
    let storage = create_mock_storage().await;
    let auth = create_mock_auth().await;
    let cfg = create_test_config();

    let article = Message {
        headers: smallvec![
            ("From".to_string(), "test@example.com".to_string()),
            ("Subject".to_string(), "Test Article".to_string()),
            ("Newsgroups".to_string(), "alt.test".to_string()),
        ],
        body: "Test body".to_string(),
    };

    let result = filter.validate(&storage, &auth, &cfg, &article, 100).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_header_filter_missing_from() {
    let filter = HeaderFilter;
    let storage = create_mock_storage().await;
    let auth = create_mock_auth().await;
    let cfg = create_test_config();

    let article = Message {
        headers: smallvec![
            ("Subject".to_string(), "Test Article".to_string()),
            ("Newsgroups".to_string(), "alt.test".to_string()),
        ],
        body: "Test body".to_string(),
    };

    let result = filter.validate(&storage, &auth, &cfg, &article, 100).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "missing required headers");
}

#[tokio::test]
async fn test_size_filter_within_limit() {
    let filter = SizeFilter;
    let storage = create_mock_storage().await;
    let auth = create_mock_auth().await;
    let mut cfg = create_test_config();
    cfg.group_settings.push(renews::config::GroupRule {
        group: None,
        pattern: Some("*".to_string()),
        retention_days: None,
        max_article_bytes: Some(1000),
    });

    let article = Message {
        headers: smallvec![("Newsgroups".to_string(), "test.group".to_string())],
        body: "Test body".to_string(),
    };

    let result = filter.validate(&storage, &auth, &cfg, &article, 500).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_size_filter_exceeds_limit() {
    let filter = SizeFilter;
    let storage = create_mock_storage().await;
    let auth = create_mock_auth().await;
    let mut cfg = create_test_config();
    cfg.group_settings.push(renews::config::GroupRule {
        group: None,
        pattern: Some("*".to_string()),
        retention_days: None,
        max_article_bytes: Some(1000),
    });

    let article = Message {
        headers: smallvec![("Newsgroups".to_string(), "test.group".to_string())],
        body: "Test body".to_string(),
    };

    let result = filter.validate(&storage, &auth, &cfg, &article, 1500).await;
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().to_string(),
        "article too large for group test.group"
    );
}

#[tokio::test]
async fn test_filter_chain_default() {
    let chain = FilterChain::default();
    let names = chain.filter_names();

    assert_eq!(names.len(), 4);
    assert_eq!(names[0], "HeaderFilter");
    assert_eq!(names[1], "SizeFilter");
    assert_eq!(names[2], "GroupExistenceFilter");
    assert_eq!(names[3], "ModerationFilter");
}

#[tokio::test]
async fn test_filter_chain_custom() {
    let chain = FilterChain::new()
        .add_filter(Box::new(HeaderFilter))
        .add_filter(Box::new(SizeFilter));

    let names = chain.filter_names();

    assert_eq!(names.len(), 2);
    assert_eq!(names[0], "HeaderFilter");
    assert_eq!(names[1], "SizeFilter");
}

#[tokio::test]
async fn test_comprehensive_validation_compatibility() {
    // Test that the new filter-based comprehensive validation still works
    let storage = create_mock_storage().await;
    let auth = create_mock_auth().await;
    let cfg = create_test_config();

    let article = Message {
        headers: smallvec![
            ("From".to_string(), "test@example.com".to_string()),
            ("Subject".to_string(), "Test Article".to_string()),
            ("Newsgroups".to_string(), "alt.test".to_string()),
        ],
        body: "Test body".to_string(),
    };

    let result = renews::handlers::utils::comprehensive_validate_article(
        &storage, &auth, &cfg, &article, 100,
    )
    .await;

    // This might fail due to group not existing, but it should at least pass header/size validation
    // The actual result depends on mock storage implementation
    assert!(
        result.is_ok()
            || result
                .unwrap_err()
                .to_string()
                .contains("group does not exist")
    );
}

// Helper functions to create test objects
fn create_test_config() -> Config {
    // Create a minimal config for testing by parsing a TOML string
    let toml = r#"
addr = ":119"
db_path = "sqlite:///:memory:"
auth_db_path = "sqlite:///:memory:"
peer_db_path = "sqlite:///:memory:"
idle_timeout_secs = 600
article_queue_capacity = 100
article_worker_count = 1
site_name = "test.local"
"#;
    toml::from_str(toml).unwrap()
}

async fn create_mock_storage() -> renews::storage::DynStorage {
    // Create a simple in-memory storage for testing
    use renews::storage::sqlite::SqliteStorage;
    let storage = SqliteStorage::new(":memory:").await.unwrap();
    Arc::new(storage)
}

async fn create_mock_auth() -> renews::auth::DynAuth {
    // Create a simple in-memory auth for testing
    use renews::auth::sqlite::SqliteAuth;
    let auth = SqliteAuth::new(":memory:").await.unwrap();
    Arc::new(auth)
}
