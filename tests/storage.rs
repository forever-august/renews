use renews::{parse_message, storage::{sqlite::SqliteStorage, Storage}};

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
