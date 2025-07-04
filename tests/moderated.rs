use renews::auth::{AuthProvider, sqlite::SqliteAuth};
use renews::storage::{Storage, sqlite::SqliteStorage};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

mod common;

#[tokio::test]
async fn post_requires_approval_for_moderated_group() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let auth = Arc::new(SqliteAuth::new("sqlite::memory:").await.unwrap());
    storage.add_group("mod.test", true).await.unwrap();
    auth.add_user("user", "pass").await.unwrap();
    let (addr, cert, _h) = common::setup_tls_server(storage.clone(), auth.clone()).await;
    let (mut reader, mut writer) = common::connect_tls(addr, cert).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"AUTHINFO USER user\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"AUTHINFO PASS pass\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"MODE READER\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"GROUP mod.test\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"POST\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("340"));
    line.clear();
    let article = concat!(
        "Message-ID: <p@test>\r\n",
        "Newsgroups: mod.test\r\n",
        "From: user@example.com\r\n",
        "Subject: t\r\n",
        "\r\n",
        "Body\r\n",
        ".\r\n",
    );
    writer.write_all(article.as_bytes()).await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("441"));
    assert!(
        storage
            .get_article_by_id("<p@test>")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn post_with_approval_succeeds() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let auth = Arc::new(SqliteAuth::new("sqlite::memory:").await.unwrap());
    storage.add_group("mod.test", true).await.unwrap();
    auth.add_user("user", "pass").await.unwrap();
    let (addr, cert, _h) = common::setup_tls_server(storage.clone(), auth.clone()).await;
    let (mut reader, mut writer) = common::connect_tls(addr, cert).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"AUTHINFO USER user\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"AUTHINFO PASS pass\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"MODE READER\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"GROUP mod.test\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"POST\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("340"));
    line.clear();
    let article = concat!(
        "Message-ID: <pa@test>\r\n",
        "Newsgroups: mod.test\r\n",
        "From: user@example.com\r\n",
        "Subject: t\r\n",
        "Approved: yes\r\n",
        "\r\n",
        "Body\r\n",
        ".\r\n",
    );
    writer.write_all(article.as_bytes()).await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("240"));
    assert!(
        storage
            .get_article_by_id("<pa@test>")
            .await
            .unwrap()
            .is_some()
    );
}
