use renews::auth::AuthProvider;
use renews::storage::{sqlite::SqliteStorage, Storage};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

mod common;

#[tokio::test]
async fn tls_quit() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let auth = Arc::new(
        renews::auth::sqlite::SqliteAuth::new("sqlite::memory:")
            .await
            .unwrap(),
    );
    let (addr, cert, _h) = common::setup_tls_server(storage, auth).await;
    let (mut reader, mut writer) = common::connect_tls(addr, cert).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("200"));
    line.clear();
    writer.write_all(b"QUIT\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("205"));
}

#[tokio::test]
async fn tls_mode_reader() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let auth = Arc::new(
        renews::auth::sqlite::SqliteAuth::new("sqlite::memory:")
            .await
            .unwrap(),
    );
    let (addr, cert, _h) = common::setup_tls_server(storage, auth).await;
    let (mut reader, mut writer) = common::connect_tls(addr, cert).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"MODE READER\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("200"));
}

#[tokio::test]
async fn tls_post_requires_auth() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let auth = Arc::new(
        renews::auth::sqlite::SqliteAuth::new("sqlite::memory:")
            .await
            .unwrap(),
    );
    storage.add_group("misc").await.unwrap();
    let (addr, cert, _h) = common::setup_tls_server(storage.clone(), auth).await;
    let (mut reader, mut writer) = common::connect_tls(addr, cert).await;
    let mut line = String::new();
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
    assert!(line.starts_with("480"));
    assert!(storage
        .get_article_by_id("<post@test>")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn tls_authinfo_and_post() {
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
    assert!(line.starts_with("381"));
    line.clear();
    writer.write_all(b"AUTHINFO PASS pass\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("281"));
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
        "Message-ID: <post@test>\r\n",
        "Newsgroups: misc\r\n",
        "\r\n",
        "Body\r\n",
        ".\r\n",
    );
    writer.write_all(article.as_bytes()).await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("240"));
    assert!(storage
        .get_article_by_id("<post@test>")
        .await
        .unwrap()
        .is_some());
    line.clear();
    writer.write_all(b"QUIT\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("205"));
}

#[tokio::test]
async fn tls_post_injects_date_header() {
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
    assert!(line.starts_with("381"));
    line.clear();
    writer.write_all(b"AUTHINFO PASS pass\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("281"));
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
        "Message-ID: <nodate@test>\r\n",
        "Newsgroups: misc\r\n",
        "\r\n",
        "Body\r\n",
        ".\r\n",
    );
    writer.write_all(article.as_bytes()).await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("240"));
    let stored = storage
        .get_article_by_id("<nodate@test>")
        .await
        .unwrap()
        .unwrap();
    assert!(stored
        .headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("Date")));
    line.clear();
    writer.write_all(b"QUIT\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("205"));
}
