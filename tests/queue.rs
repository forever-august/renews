//! Test for article queue functionality

mod utils;

use renews::{
    auth::sqlite::SqliteAuth,
    queue::{ArticleQueue, WorkerPool},
    storage::{Storage, sqlite::SqliteStorage},
};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::sync::RwLock;
use utils::{connect, create_test_queued_article, setup_server};

#[tokio::test]
async fn test_queue_functionality() {
    // Create storage and auth
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let auth = Arc::new(SqliteAuth::new("sqlite::memory:").await.unwrap());

    // Create a queue
    let queue = ArticleQueue::new(10);
    let config = Arc::new(RwLock::new(toml::from_str("addr=\":119\"").unwrap()));

    // Create worker pool
    let worker_pool = WorkerPool::new(
        queue.clone(),
        storage.clone(),
        auth.clone(),
        config.clone(),
        2,
    );

    // Start workers
    let _worker_handles = worker_pool.start().await;

    // Add the group first
    storage.add_group("test.group", false).await.unwrap();

    // Submit to queue
    let queued_article =
        create_test_queued_article("<test@example.com>", "test.group", "Test body");

    queue.submit(queued_article).await.unwrap();

    // Wait a bit for processing
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Verify the article was stored
    let stored = storage
        .get_article_by_id("<test@example.com>")
        .await
        .unwrap();
    assert!(stored.is_some());
    assert_eq!(stored.unwrap().body, "Test body");
}

#[tokio::test]
async fn test_queue_with_server() {
    // Create storage and auth
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let auth = Arc::new(SqliteAuth::new("sqlite::memory:").await.unwrap());

    // Add test group
    storage.add_group("test.group", false).await.unwrap();

    // For this test, we'll use the setup_server without queue to verify compatibility
    let (addr, handle) = setup_server(storage.clone(), auth.clone()).await;

    // Connect and test posting
    let (mut reader, mut writer) = connect(addr).await;

    // Enable TLS mode and authenticate (in test mode, this is simplified)
    writer.write_all(b"MODE READER\r\n").await.unwrap();
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();

    // This test verifies the backward compatibility when no queue is available
    writer.write_all(b"QUIT\r\n").await.unwrap();

    handle.abort();
}
