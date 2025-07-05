use base64::{Engine as _, engine::general_purpose::STANDARD};
use renews::auth::sqlite::SqliteAuth;
use renews::parse_message;
use renews::storage::{Storage, sqlite::SqliteStorage};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

use test_utils as common;

#[tokio::test]
async fn cancel_key_allows_cancel() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let auth = Arc::new(SqliteAuth::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test", false).await.unwrap();

    let key = "secret";
    let key_b64 = STANDARD.encode(key);
    let lock_hash = Sha256::digest(key_b64.as_bytes());
    let lock_b64 = STANDARD.encode(lock_hash);
    let orig = format!(
        "Message-ID: <a@test>\r\nNewsgroups: misc.test\r\nCancel-Lock: sha256:{}\r\n\r\nBody",
        lock_b64
    );
    let (_, msg) = parse_message(&orig).unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();

    let (addr, _h) = common::setup_server(storage.clone(), auth.clone()).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();

    writer.write_all(b"IHAVE <c@test>\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();

    let cancel = format!(
        "Message-ID: <c@test>\r\nNewsgroups: misc.test\r\nControl: cancel <a@test>\r\nCancel-Key: sha256:{}\r\n\r\n.\r\n",
        key_b64
    );
    writer.write_all(cancel.as_bytes()).await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("235"));
    assert!(
        storage
            .get_article_by_id("<a@test>")
            .await
            .unwrap()
            .is_none()
    );
}
