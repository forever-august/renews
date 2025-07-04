use renews::auth::AuthProvider;
use renews::storage::{Storage, sqlite::SqliteStorage};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

mod common;

#[tokio::test]
async fn post_missing_headers_rejected() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let auth = Arc::new(
        renews::auth::sqlite::SqliteAuth::new("sqlite::memory:")
            .await
            .unwrap(),
    );
    storage.add_group("misc").await.unwrap();
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
    writer.write_all(b"GROUP misc\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"POST\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("340"));
    line.clear();
    let article = concat!(
        "Message-ID: <bad@test>\r\n",
        "Newsgroups: misc\r\n",
        "\r\n",
        "Body\r\n",
        ".\r\n",
    );
    writer.write_all(article.as_bytes()).await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("441"));
    assert!(
        storage
            .get_article_by_id("<bad@test>")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn ihave_missing_headers_rejected() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let auth = Arc::new(
        renews::auth::sqlite::SqliteAuth::new("sqlite::memory:")
            .await
            .unwrap(),
    );
    storage.add_group("misc.test").await.unwrap();
    let (addr, _h) = common::setup_server(storage.clone(), auth.clone()).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"IHAVE <missing@test>\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("335"));
    line.clear();
    let article = "Message-ID: <missing@test>\r\nNewsgroups: misc.test\r\n\r\nBody\r\n.\r\n";
    writer.write_all(article.as_bytes()).await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("437"));
    assert!(
        storage
            .get_article_by_id("<missing@test>")
            .await
            .unwrap()
            .is_none()
    );
}
