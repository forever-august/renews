//! Tests for resource exhaustion and queue failure modes

use crate::utils::{setup, create_test_queue_with_workers, ClientMock};
use renews::{
    config::Config,
    queue::{ArticleQueue, QueuedArticle},
    Message,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use smallvec::smallvec;

#[tokio::test]
async fn test_queue_capacity_exhaustion() {
    let (storage, auth) = setup().await;
    let config = Arc::new(RwLock::new(Config {
        addr: "127.0.0.1:0".to_string(),
        site_name: "test".to_string(),
        db_path: "sqlite::memory:".to_string(),
        auth_db_path: "sqlite::memory:".to_string(),
        peer_db_path: "sqlite::memory:".to_string(),
        peer_sync_schedule: "0 0 * * * *".to_string(),
        idle_timeout_secs: 600,
        peers: vec![],
        tls_addr: None,
        tls_cert: None,
        tls_key: None,
        ws_addr: None,
        default_retention_days: None,
        default_max_article_bytes: None,
        article_queue_capacity: 2, // Very small queue
        article_worker_count: 1,
        group_settings: vec![],
        filters: vec![],
        pgp_key_servers: renews::config::default_pgp_key_servers(),
    }));

    let queue = create_test_queue_with_workers(storage, auth, config).await;

    // Create test articles
    let article1 = QueuedArticle {
        message: Message {
            headers: smallvec![
                ("From".to_string(), "test1@example.com".to_string()),
                ("Subject".to_string(), "Test 1".to_string()),
                ("Message-ID".to_string(), "<test1@example.com>".to_string()),
            ],
            body: "Test body 1".to_string(),
        },
        size: 100,
        is_control: false,
        already_validated: false,
    };

    let article2 = QueuedArticle {
        message: Message {
            headers: smallvec![
                ("From".to_string(), "test2@example.com".to_string()),
                ("Subject".to_string(), "Test 2".to_string()),
                ("Message-ID".to_string(), "<test2@example.com>".to_string()),
            ],
            body: "Test body 2".to_string(),
        },
        size: 100,
        is_control: false,
        already_validated: false,
    };

    let article3 = QueuedArticle {
        message: Message {
            headers: smallvec![
                ("From".to_string(), "test3@example.com".to_string()),
                ("Subject".to_string(), "Test 3".to_string()),
                ("Message-ID".to_string(), "<test3@example.com>".to_string()),
            ],
            body: "Test body 3".to_string(),
        },
        size: 100,
        is_control: false,
        already_validated: false,
    };

    // Fill the queue to capacity
    assert!(queue.submit(article1).await.is_ok());
    assert!(queue.submit(article2).await.is_ok());

    // This should fail due to capacity
    assert!(queue.submit(article3).await.is_err());
}

#[tokio::test]
async fn test_empty_queue_operations() {
    let queue = ArticleQueue::new(10);

    // Get the receiver for the queue
    let receiver = queue.receiver();
    
    // Try to receive from empty queue (should not block, should fail immediately)
    let result = receiver.try_recv();

    // Should return error since queue is empty
    assert!(result.is_err());
}

#[tokio::test]
async fn test_zero_capacity_queue() {
    let queue = ArticleQueue::new(0);

    let article = QueuedArticle {
        message: Message {
            headers: smallvec![
                ("From".to_string(), "test@example.com".to_string()),
                ("Subject".to_string(), "Test".to_string()),
                ("Message-ID".to_string(), "<test@example.com>".to_string()),
            ],
            body: "Test body".to_string(),
        },
        size: 100,
        is_control: false,
        already_validated: false,
    };

    // Should fail immediately with zero capacity
    assert!(queue.submit(article).await.is_err());
}

#[tokio::test]
async fn test_large_article_submission() {
    let (storage, auth) = setup().await;
    storage.add_group("test.group", false).await.unwrap();
    auth.add_user("testuser", "password").await.unwrap();

    let config: Config = toml::from_str(r#"
addr = ":119"
default_max_article_bytes = 1000
"#).unwrap();

    // Create a very large article (exceeds limit)
    let large_body = "x".repeat(2000);
    let large_article = format!(
        "From: test@example.com\r\nSubject: Large Article\r\nNewsgroups: test.group\r\nMessage-ID: <large@example.com>\r\n\r\n{}\r\n.\r\n",
        large_body
    );

    ClientMock::new()
        .expect("AUTHINFO USER testuser", "381 more authentication information required")
        .expect("AUTHINFO PASS password", "281 authentication accepted")
        .expect("POST", "340 send article to be posted")
        .expect_request_multi(
            vec![large_article],
            vec!["441 posting failed"] // Should fail due to size limit
        )
        .expect("QUIT", "205 closing connection")
        .run_with_cfg(config, storage, auth)
        .await;
}

#[tokio::test]
async fn test_malformed_article_submission() {
    let (storage, auth) = setup().await;
    storage.add_group("test.group", false).await.unwrap();
    auth.add_user("testuser", "password").await.unwrap();

    let config: Config = toml::from_str(r#"addr = ":119""#).unwrap();

    // Create an article with malformed headers
    let malformed_article = "InvalidHeader\r\nSubject: Test\r\nNewsgroups: test.group\r\n\r\nBody\r\n.\r\n";

    ClientMock::new()
        .expect("AUTHINFO USER testuser", "381 more authentication information required")
        .expect("AUTHINFO PASS password", "281 authentication accepted")
        .expect("POST", "340 send article to be posted")
        .expect_request_multi(
            vec![malformed_article.to_string()],
            vec!["441 posting failed"] // Should fail due to malformed headers
        )
        .expect("QUIT", "205 closing connection")
        .run_with_cfg(config, storage, auth)
        .await;
}

#[tokio::test]
async fn test_missing_required_headers() {
    let (storage, auth) = setup().await;
    storage.add_group("test.group", false).await.unwrap();
    auth.add_user("testuser", "password").await.unwrap();

    let config: Config = toml::from_str(r#"addr = ":119""#).unwrap();

    // Article missing From header
    let article_no_from = "Subject: Test\r\nNewsgroups: test.group\r\n\r\nBody\r\n.\r\n";

    // Article missing Subject header
    let article_no_subject = "From: test@example.com\r\nNewsgroups: test.group\r\n\r\nBody\r\n.\r\n";

    // Article missing Newsgroups header
    let article_no_newsgroups = "From: test@example.com\r\nSubject: Test\r\n\r\nBody\r\n.\r\n";

    ClientMock::new()
        .expect("AUTHINFO USER testuser", "381 more authentication information required")
        .expect("AUTHINFO PASS password", "281 authentication accepted")
        .expect("POST", "340 send article to be posted")
        .expect_request_multi(
            vec![article_no_from.to_string()],
            vec!["441 posting failed"]
        )
        .expect("POST", "340 send article to be posted")
        .expect_request_multi(
            vec![article_no_subject.to_string()],
            vec!["441 posting failed"]
        )
        .expect("POST", "340 send article to be posted")
        .expect_request_multi(
            vec![article_no_newsgroups.to_string()],
            vec!["441 posting failed"]
        )
        .expect("QUIT", "205 closing connection")
        .run_with_cfg(config, storage, auth)
        .await;
}

#[tokio::test]
async fn test_concurrent_queue_access() {
    let queue = ArticleQueue::new(100);

    // Create multiple concurrent submission tasks
    let mut handles = Vec::new();
    
    for i in 0..10 {
        let queue_clone = queue.clone();
        let handle = tokio::spawn(async move {
            let article = QueuedArticle {
                message: Message {
                    headers: smallvec![
                        ("From".to_string(), format!("test{}@example.com", i)),
                        ("Subject".to_string(), format!("Test {}", i)),
                        ("Message-ID".to_string(), format!("<test{}@example.com>", i)),
                    ],
                    body: format!("Test body {}", i),
                },
                size: 100,
                is_control: false,
                already_validated: false,
            };
            
            queue_clone.submit(article).await
        });
        handles.push(handle);
    }

    // Wait for all submissions to complete
    let results = futures_util::future::join_all(handles).await;
    
    // All should succeed since queue capacity is sufficient
    for result in results {
        assert!(result.unwrap().is_ok());
    }
}

#[tokio::test]
async fn test_extremely_long_headers() {
    let (storage, auth) = setup().await;
    storage.add_group("test.group", false).await.unwrap();
    auth.add_user("testuser", "password").await.unwrap();

    let config: Config = toml::from_str(r#"addr = ":119""#).unwrap();

    // Create an article with extremely long header values
    let long_subject = "x".repeat(10000);
    let article_long_header = format!(
        "From: test@example.com\r\nSubject: {}\r\nNewsgroups: test.group\r\n\r\nBody\r\n.\r\n",
        long_subject
    );

    ClientMock::new()
        .expect("AUTHINFO USER testuser", "381 more authentication information required")
        .expect("AUTHINFO PASS password", "281 authentication accepted")
        .expect("POST", "340 send article to be posted")
        .expect_request_multi(
            vec![article_long_header],
            vec!["240 article received"] // Might succeed or fail depending on implementation
        )
        .expect("QUIT", "205 closing connection")
        .run_with_cfg(config, storage, auth)
        .await;
}

#[tokio::test]
async fn test_null_bytes_in_article() {
    let (storage, auth) = setup().await;
    storage.add_group("test.group", false).await.unwrap();
    auth.add_user("testuser", "password").await.unwrap();

    let config: Config = toml::from_str(r#"addr = ":119""#).unwrap();

    // Create an article with null bytes (binary content)
    let article_with_nulls = "From: test@example.com\r\nSubject: Test\r\nNewsgroups: test.group\r\n\r\nBody with \0 null byte\r\n.\r\n";

    ClientMock::new()
        .expect("AUTHINFO USER testuser", "381 more authentication information required")
        .expect("AUTHINFO PASS password", "281 authentication accepted")
        .expect("POST", "340 send article to be posted")
        .expect_request_multi(
            vec![article_with_nulls.to_string()],
            vec!["240 article received"] // Behavior depends on implementation
        )
        .expect("QUIT", "205 closing connection")
        .run_with_cfg(config, storage, auth)
        .await;
}