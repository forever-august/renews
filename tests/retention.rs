use renews::retention::cleanup_expired_articles;
use renews::{
    config::Config,
    parse_message,
    storage::{Storage, sqlite::SqliteStorage},
};
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tokio::time::sleep;

#[tokio::test]
async fn cleanup_removes_expired() {
    let cfg: Config = toml::from_str("port=1199\ndefault_retention_days=0").unwrap();
    let storage: Arc<dyn Storage> = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc", false).await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nB").unwrap();
    storage.store_article("misc", &msg).await.unwrap();
    sleep(StdDuration::from_secs(1)).await;
    cleanup_expired_articles(&*storage, &cfg).await.unwrap();
    assert!(
        storage
            .get_article_by_id("<1@test>")
            .await
            .unwrap()
            .is_none()
    );
}
