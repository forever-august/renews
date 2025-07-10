use crate::utils::{self as common, ClientMock};
use renews::auth::AuthProvider;
use renews::peers::{PeerConfig, PeerDb, build_peer_job};
use renews::storage::Storage;
use renews::storage::sqlite::SqliteStorage;
use serial_test::serial;
use std::fs;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio_cron_scheduler::JobScheduler;

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
        sync_schedule: Some("* * * * * *".into()), // Every second for testing
    };
    let mut scheduler = JobScheduler::new().await.unwrap();
    let job = build_peer_job(
        peer,
        "* * * * * *".to_string(),
        db.clone(),
        storage.clone(),
        "local".into(),
    )
    .unwrap();
    scheduler.add(job).await.unwrap();
    scheduler.start().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    scheduler.shutdown().await.unwrap();
    let last = db.get_last_sync("127.0.0.1:9").await.unwrap();
    assert!(last.is_some());
}

async fn peer_transfer_helper(schedule: &str) {
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
    auth.add_user("user", "pass").await.unwrap();

    let cfg_a: renews::config::Config = toml::from_str("addr=\":119\"\nsite_name='A'").unwrap();
    let cfg_b: renews::config::Config = toml::from_str("addr=\":119\"").unwrap();
    let (addr_b, cert_b, handle_b) =
        common::start_server(storage_b.clone(), auth.clone(), cfg_b.clone(), true).await;
    let ca_file = NamedTempFile::new().unwrap();
    if let Some((_, pem)) = &cert_b {
        fs::write(ca_file.path(), pem).unwrap();
        unsafe { std::env::set_var("SSL_CERT_FILE", ca_file.path()) };
    }
    let (addr_a, cert_a, handle_a) =
        common::start_server(storage_a.clone(), auth.clone(), cfg_a, true).await;

    let article = concat!(
        "Message-ID: <1@test>\r\n",
        "Newsgroups: misc.test\r\n",
        "From: a@test\r\n",
        "Subject: hello\r\n",
        "Date: Wed, 05 Oct 2022 00:00:00 GMT\r\n",
        "\r\n",
        "body\r\n",
        ".\r\n",
    );
    let cert_a = cert_a.unwrap().0;
    ClientMock::new()
        .expect("AUTHINFO USER user", "381 password required")
        .expect("AUTHINFO PASS pass", "281 authentication accepted")
        .expect(
            "POST",
            "340 send article to be posted. End with <CR-LF>.<CR-LF>",
        )
        .expect_request_multi(
            common::request_lines(article.trim_end_matches("\r\n")),
            vec!["240 article received"],
        )
        .run_tls_at(addr_a, cert_a)
        .await;
    handle_a.await.unwrap();

    let db = PeerDb::new("sqlite::memory:").await.unwrap();
    let peer_name = format!("localhost:{}", addr_b.port());
    db.sync_config(&[peer_name.clone()]).await.unwrap();
    let peer = PeerConfig {
        sitename: peer_name.clone(),
        patterns: vec!["*".into()],
        sync_schedule: Some(schedule.to_string()),
    };
    let mut scheduler = JobScheduler::new().await.unwrap();
    let job = build_peer_job(
        peer,
        "* * * * * *".to_string(),
        db.clone(),
        storage_a.clone(),
        "A".into(),
    )
    .unwrap();
    scheduler.add(job).await.unwrap();
    scheduler.start().await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
    scheduler.shutdown().await.unwrap();
    handle_b.await.unwrap();

    let (check_addr, check_cert, check_handle) =
        common::start_server(storage_b.clone(), auth.clone(), cfg_b.clone(), true).await;
    let check_cert = check_cert.unwrap().0;
    ClientMock::new()
        .expect_multi(
            "ARTICLE <1@test>",
            vec![
                "220 0 <1@test> article follows",
                "Message-ID: <1@test>",
                "Newsgroups: misc.test",
                "From: a@test",
                "Subject: hello",
                "Date: Wed, 05 Oct 2022 00:00:00 GMT",
                "Path: A",
                "",
                "body",
                ".",
            ],
        )
        .run_tls_at(check_addr, check_cert)
        .await;

    check_handle.await.unwrap();
}

#[tokio::test]
#[serial]
async fn peer_transfer_fast_schedule() {
    peer_transfer_helper("* * * * * *").await; // Every second
}

#[tokio::test]
#[serial]
async fn peer_transfer_default_schedule() {
    peer_transfer_helper("*/2 * * * * *").await; // Every 2 seconds
}
