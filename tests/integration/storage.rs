use renews::{
    parse_message,
    storage::{Storage, sqlite::SqliteStorage},
};

#[tokio::test]
async fn store_and_retrieve_article() {
    let storage = SqliteStorage::new("sqlite::memory:").await.expect("init");
    let text = "Message-ID: <1@test>\r\nNewsgroups: group.test\r\nSubject: Hello\r\n\r\nBody";
    let (_, msg) = parse_message(text).unwrap();
    storage.store_article(&msg).await.unwrap();
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
    let (_, msg1) = parse_message("Message-ID: <1@test>\r\nNewsgroups: g1,g2\r\n\r\nA").unwrap();
    let (_, msg2) = parse_message("Message-ID: <2@test>\r\nNewsgroups: g1\r\n\r\nB").unwrap();

    // Store articles and verify numbering through retrieval
    storage.store_article(&msg1).await.unwrap();
    storage.store_article(&msg2).await.unwrap();

    // Verify numbering is per group
    assert!(
        storage
            .get_article_by_number("g1", 1)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        storage
            .get_article_by_number("g1", 2)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        storage
            .get_article_by_number("g2", 1)
            .await
            .unwrap()
            .is_some()
    );

    // Verify msg1 is at position 1 in both groups
    let g1_msg1 = storage
        .get_article_by_number("g1", 1)
        .await
        .unwrap()
        .unwrap();
    let g2_msg1 = storage
        .get_article_by_number("g2", 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(g1_msg1.body, "A");
    assert_eq!(g2_msg1.body, "A");

    // Verify msg2 is at position 2 in g1
    let g1_msg2 = storage
        .get_article_by_number("g1", 2)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(g1_msg2.body, "B");
}

#[tokio::test]
async fn add_and_list_groups() {
    let storage = SqliteStorage::new("sqlite::memory:").await.expect("init");
    assert!(storage.list_groups().await.unwrap().is_empty());
    storage.add_group("g1", false).await.unwrap();
    storage.add_group("g2", false).await.unwrap();
    let groups = storage.list_groups().await.unwrap();
    assert_eq!(groups, vec!["g1".to_string(), "g2".to_string()]);

    storage.remove_group("g1").await.unwrap();
    let groups = storage.list_groups().await.unwrap();
    assert_eq!(groups, vec!["g2".to_string()]);
}

#[tokio::test]
async fn purge_old_articles() {
    use chrono::Utc;
    use std::time::Duration as StdDuration;
    use tokio::time::sleep;

    let storage = SqliteStorage::new("sqlite::memory:").await.expect("init");
    storage.add_group("g1", false).await.unwrap();
    storage.add_group("g2", false).await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\nNewsgroups: g1,g2\r\n\r\nB").unwrap();
    storage.store_article(&msg).await.unwrap();

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

#[tokio::test]
async fn store_article_in_multiple_groups() {
    let storage = SqliteStorage::new("sqlite::memory:").await.expect("init");
    storage.add_group("group1", false).await.unwrap();
    storage.add_group("group2", false).await.unwrap();
    storage.add_group("group3", false).await.unwrap();

    // Create an article with multiple newsgroups
    let text = "Message-ID: <multi@test>\r\nNewsgroups: group1,group2,group3\r\nSubject: Multi\r\n\r\nBody";
    let (_, msg) = parse_message(text).unwrap();

    // Store the article - it should be automatically stored in all groups
    storage.store_article(&msg).await.unwrap();

    // Verify the article is in all three groups
    let article1 = storage
        .get_article_by_number("group1", 1)
        .await
        .unwrap()
        .expect("article in group1");
    let article2 = storage
        .get_article_by_number("group2", 1)
        .await
        .unwrap()
        .expect("article in group2");
    let article3 = storage
        .get_article_by_number("group3", 1)
        .await
        .unwrap()
        .expect("article in group3");

    assert_eq!(article1.body, "Body");
    assert_eq!(article2.body, "Body");
    assert_eq!(article3.body, "Body");

    // Verify they're the same message by checking Message-ID
    let msg_id1 = article1
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("Message-ID"))
        .unwrap()
        .1
        .clone();
    let msg_id2 = article2
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("Message-ID"))
        .unwrap()
        .1
        .clone();
    let msg_id3 = article3
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("Message-ID"))
        .unwrap()
        .1
        .clone();

    assert_eq!(msg_id1, "<multi@test>");
    assert_eq!(msg_id2, "<multi@test>");
    assert_eq!(msg_id3, "<multi@test>");
}

#[tokio::test]
async fn store_article_without_newsgroups_header() {
    let storage = SqliteStorage::new("sqlite::memory:").await.expect("init");
    storage.add_group("test", false).await.unwrap();

    // Create an article without Newsgroups header
    let text = "Message-ID: <no-groups@test>\r\nSubject: No Groups\r\n\r\nBody";
    let (_, msg) = parse_message(text).unwrap();

    // Store the article - should succeed but not be in any group
    storage.store_article(&msg).await.unwrap();

    // Verify the article is not retrievable by group (since it wasn't posted to any)
    let article = storage.get_article_by_number("test", 1).await.unwrap();
    assert!(article.is_none());

    // But should be retrievable by message ID
    let article = storage.get_article_by_id("<no-groups@test>").await.unwrap();
    assert!(article.is_some());
}

#[tokio::test]
async fn store_article_multiple_groups_comma_separated() {
    let storage = SqliteStorage::new("sqlite::memory:").await.expect("init");
    storage.add_group("group1", false).await.unwrap();
    storage.add_group("group2", false).await.unwrap();
    storage.add_group("group3", false).await.unwrap();

    // Create an article with multiple newsgroups in a single header (comma-separated)
    let text = "Message-ID: <multi@test>\r\nNewsgroups: group1,group2,group3\r\nSubject: Multi-post\r\n\r\nBody content";
    let (_, msg) = parse_message(text).unwrap();

    // Store the article - it should be automatically stored in all groups
    storage.store_article(&msg).await.unwrap();

    // Verify the article is in all three groups at position 1
    for group in ["group1", "group2", "group3"] {
        let article = storage
            .get_article_by_number(group, 1)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("article in {group}"));
        assert_eq!(article.body, "Body content");

        // Verify the Message-ID is consistent
        let msg_id = article
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Message-ID"))
            .unwrap()
            .1
            .clone();
        assert_eq!(msg_id, "<multi@test>");

        // Verify the Newsgroups header contains all groups
        let newsgroups = article
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Newsgroups"))
            .unwrap()
            .1
            .clone();
        assert_eq!(newsgroups, "group1,group2,group3");
    }
}
