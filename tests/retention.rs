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
async fn cleanup_retention_zero_keeps_articles() {
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
            .is_some()
    );
}

#[tokio::test]
async fn cleanup_expires_header() {
    use chrono::Duration as ChronoDuration;
    let cfg: Config = toml::from_str("port=1199\ndefault_retention_days=10").unwrap();
    let storage: Arc<dyn Storage> = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc", false).await.unwrap();
    let past = (chrono::Utc::now() - ChronoDuration::days(1)).to_rfc2822();
    let text = format!("Message-ID: <2@test>\r\nExpires: {}\r\n\r\nB", past);
    let (_, msg) = parse_message(&text).unwrap();
    storage.store_article("misc", &msg).await.unwrap();
    cleanup_expired_articles(&*storage, &cfg).await.unwrap();
    assert!(
        storage
            .get_article_by_id("<2@test>")
            .await
            .unwrap()
            .is_none()
    );
}
