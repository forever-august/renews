use renews::{
    parse_message,
    storage::{Storage, sqlite::SqliteStorage},
};

#[tokio::test]
async fn store_and_retrieve_article() {
    let storage = SqliteStorage::new("sqlite::memory:").await.expect("init");
    let text = "Message-ID: <1@test>\r\nSubject: Hello\r\n\r\nBody";
    let (_, msg) = parse_message(text).unwrap();
    let n = storage.store_article("group.test", &msg).await.unwrap();
    assert_eq!(n, 1);
    let fetched = storage
        .get_article_by_number("group.test", 1)
        .await
        .unwrap()
        .expect("article by number");
    assert_eq!(fetched.body, "Body");
    let fetched_id = storage
        .get_article_by_id("<1@test>")
        .await
        .unwrap()
        .expect("article by id");
    assert_eq!(fetched_id.headers, fetched.headers);
    // drop storage to close connections
}

#[tokio::test]
async fn numbering_is_per_group() {
    let storage = SqliteStorage::new("sqlite::memory:").await.expect("init");
    let (_, msg1) = parse_message("Message-ID: <1@test>\r\n\r\nA").unwrap();
    let (_, msg2) = parse_message("Message-ID: <2@test>\r\n\r\nB").unwrap();
    assert_eq!(storage.store_article("g1", &msg1).await.unwrap(), 1);
    assert_eq!(storage.store_article("g1", &msg2).await.unwrap(), 2);
    assert_eq!(storage.store_article("g2", &msg1).await.unwrap(), 1);
}

#[tokio::test]
async fn add_and_list_groups() {
    let storage = SqliteStorage::new("sqlite::memory:").await.expect("init");
    assert!(storage.list_groups().await.unwrap().is_empty());
    storage.add_group("g1").await.unwrap();
    storage.add_group("g2").await.unwrap();
    let groups = storage.list_groups().await.unwrap();
    assert_eq!(groups, vec!["g1".to_string(), "g2".to_string()]);
}

#[tokio::test]
async fn purge_old_articles() {
    use chrono::Utc;
    use std::time::Duration as StdDuration;
    use tokio::time::sleep;

    let storage = SqliteStorage::new("sqlite::memory:").await.expect("init");
    storage.add_group("g1").await.unwrap();
    storage.add_group("g2").await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nB").unwrap();
    storage.store_article("g1", &msg).await.unwrap();
    storage.store_article("g2", &msg).await.unwrap();

    sleep(StdDuration::from_secs(1)).await;
    storage.purge_group_before("g1", Utc::now()).await.unwrap();
    storage.purge_orphan_messages().await.unwrap();
    assert!(
        storage
            .get_article_by_number("g1", 1)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .get_article_by_number("g2", 1)
            .await
            .unwrap()
            .is_some()
    );

    storage.purge_group_before("g2", Utc::now()).await.unwrap();
    storage.purge_orphan_messages().await.unwrap();
    assert!(
        storage
            .get_article_by_id("<1@test>")
            .await
            .unwrap()
            .is_none()
    );
}
