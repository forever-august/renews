use renews::peers::{PeerConfig, PeerDb, peer_task};
use renews::storage::Storage;
use renews::storage::sqlite::SqliteStorage;
use serial_test::serial;
use std::fs;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::sync::RwLock;

use test_utils as common;

#[tokio::test]
async fn add_and_remove_peers() {
    let db = PeerDb::new("sqlite::memory:").await.unwrap();
    db.sync_config(&["a".into(), "b".into()]).await.unwrap();
    let mut list = db.list_peers().await.unwrap();
    list.sort();
    assert_eq!(list, vec!["a", "b"]);
    db.sync_config(&["b".into()]).await.unwrap();
    let list = db.list_peers().await.unwrap();
    assert_eq!(list, vec!["b"]);
}

#[tokio::test]
async fn peer_task_updates_last_sync() {
    let db = PeerDb::new("sqlite::memory:").await.unwrap();
    db.sync_config(&["127.0.0.1:9".into()]).await.unwrap();
    let storage = SqliteStorage::new("sqlite::memory:").await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let peer = PeerConfig {
        sitename: "127.0.0.1:9".into(),
        patterns: vec![],
        sync_interval_secs: Some(1),
        username: None,
        password: None,
    };
    let db_clone = db.clone();
    let storage_clone = storage.clone();
    tokio::spawn(async move {
        peer_task(peer, 1, db_clone, storage_clone, "local".into()).await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    let last = db.get_last_sync("127.0.0.1:9").await.unwrap();
    assert!(last.is_some());
}

async fn peer_transfer_helper(interval: u64) {
    let storage_a: Arc<dyn Storage> =
        Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let storage_b: Arc<dyn Storage> =
        Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage_a.add_group("misc.test", false).await.unwrap();
    storage_b.add_group("misc.test", false).await.unwrap();
    let auth = Arc::new(
        renews::auth::sqlite::SqliteAuth::new("sqlite::memory:")
            .await
            .unwrap(),
    );

    let cfg_a: Arc<RwLock<renews::config::Config>> = Arc::new(RwLock::new(
        toml::from_str("port=119\nsite_name='A'").unwrap(),
    ));
    let (addr_b, _cert_b, pem, handle_b) =
        common::setup_tls_server(storage_b.clone(), auth.clone()).await;
    let ca_file = NamedTempFile::new().unwrap();
    fs::write(ca_file.path(), pem).unwrap();
    unsafe { std::env::set_var("SSL_CERT_FILE", ca_file.path()) };
    let (addr_a, handle_a) =
        common::setup_server_with_cfg(storage_a.clone(), auth.clone(), cfg_a).await;

    let (mut reader, mut writer) = common::connect(addr_a).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    writer.write_all(b"IHAVE <1@test>\r\n").await.unwrap();
    line.clear();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("335"));
    let article = "Message-ID: <1@test>\r\nNewsgroups: misc.test\r\nFrom: a@test\r\nSubject: hello\r\n\r\nbody\r\n.\r\n";
    line.clear();
    writer.write_all(article.as_bytes()).await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("235"));
    let _ = writer.shutdown().await;
    handle_a.await.unwrap();

    let db = PeerDb::new("sqlite::memory:").await.unwrap();
    let peer_name = format!("localhost:{}", addr_b.port());
    db.sync_config(&[peer_name.clone()]).await.unwrap();
    let peer = PeerConfig {
        sitename: peer_name.clone(),
        patterns: vec!["*".into()],
        sync_interval_secs: Some(interval),
        username: None,
        password: None,
    };
    let db_clone = db.clone();
    let storage_clone = storage_a.clone();
    let peer_handle = tokio::spawn(async move {
        peer_task(peer, 1, db_clone, storage_clone, "A".into()).await;
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
    peer_handle.abort();
    handle_b.await.unwrap();

    assert!(
        storage_b
            .get_article_by_id("<1@test>")
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
#[serial]
async fn peer_transfer_interval_zero() {
    peer_transfer_helper(0).await;
}

#[tokio::test]
#[serial]
async fn peer_transfer_interval_one() {
    peer_transfer_helper(1).await;
}
