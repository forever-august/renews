use renews::config::Config;
use renews::storage::{Storage, sqlite::SqliteStorage};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

mod common;

#[tokio::test]
async fn ihave_rejects_large_article() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let auth = Arc::new(
        renews::auth::sqlite::SqliteAuth::new("sqlite::memory:")
            .await
            .unwrap(),
    );
    storage.add_group("misc.test").await.unwrap();
    let cfg: Config = toml::from_str("port=1199\ndefault_max_article_bytes=10\n").unwrap();
    let (addr, _h) = common::setup_server_with_cfg(storage.clone(), auth.clone(), cfg).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"IHAVE <1@test>\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("335"));
    line.clear();
    let article = "Message-ID: <1@test>\r\nFrom: a@test\r\nSubject: S\r\nNewsgroups: misc.test\r\n\r\n0123456789A\r\n.\r\n";
    writer.write_all(article.as_bytes()).await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("437"));
}

#[tokio::test]
async fn ihave_rejects_large_article_with_suffix() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let auth = Arc::new(
        renews::auth::sqlite::SqliteAuth::new("sqlite::memory:")
            .await
            .unwrap(),
    );
    storage.add_group("misc.test").await.unwrap();
    let cfg: Config = toml::from_str("port=1199\ndefault_max_article_bytes=\"1K\"\n").unwrap();
    let (addr, _h) = common::setup_server_with_cfg(storage.clone(), auth.clone(), cfg).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"IHAVE <2@test>\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("335"));
    line.clear();
    let body = "A".repeat(1100);
    let article = format!(
        "Message-ID: <2@test>\r\nFrom: b@test\r\nSubject: B\r\nNewsgroups: misc.test\r\n\r\n{}\r\n.\r\n",
        body
    );
    writer.write_all(article.as_bytes()).await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("437"));
}
