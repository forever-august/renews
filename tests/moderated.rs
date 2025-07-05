use renews::auth::{AuthProvider, sqlite::SqliteAuth};
use renews::control::canonical_text;
use renews::parse_message;
use renews::storage::{Storage, sqlite::SqliteStorage};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

use test_utils as common;

const ADMIN_SEC: &str = include_str!("data/admin.sec.asc");
const ADMIN_PUB: &str = include_str!("data/admin.pub.asc");

fn build_sig(data: &str) -> (String, Vec<String>) {
    use pgp::composed::{Deserializable, SignedSecretKey, StandaloneSignature};
    use pgp::packet::SignatureConfig;
    use pgp::packet::SignatureType;
    use pgp::types::Password;
    use rand::thread_rng;

    let (key, _) = SignedSecretKey::from_string(ADMIN_SEC).unwrap();
    let cfg = SignatureConfig::from_key(thread_rng(), &key.primary_key, SignatureType::Binary).unwrap();
    let sig = cfg.sign(&key.primary_key, &Password::empty(), data.as_bytes()).unwrap();
    let armored = StandaloneSignature::new(sig).to_armored_string(Default::default()).unwrap();
    let version = "1".to_string();
    let mut lines = Vec::new();
    for line in armored.lines() {
        if line.starts_with("-----BEGIN") || line.starts_with("Version") || line.is_empty() {
            continue;
        }
        if line.starts_with("-----END") {
            break;
        }
        lines.push(line.to_string());
    }
    (version, lines)
}

fn build_article() -> String {
    let headers = concat!(
        "Message-ID: <pa@test>\r\n",
        "Newsgroups: mod.test\r\n",
        "From: user@example.com\r\n",
        "Subject: t\r\n",
        "Approved: user\r\n",
        "Date: Wed, 05 Oct 2022 00:00:00 GMT\r\n",
    );
    let body = "Body\n";
    let article_text = format!("{}\r\n{}", headers, body);
    let (_, msg) = parse_message(&article_text).unwrap();
    let signed = "Message-ID,Newsgroups,From,Subject,Approved,Date";
    let data = canonical_text(&msg, signed);
    let (ver, lines) = build_sig(&data);
    let mut xhdr = format!("X-PGP-Sig: {} {}", ver, signed);
    for l in &lines {
        xhdr.push_str("\r\n ");
        xhdr.push_str(l);
    }
    format!("{}{}\r\n\r\nBody\r\n.\r\n", headers, xhdr)
}

fn build_cross_article() -> String {
    let base = concat!(
        "Message-ID: <pa@test>\r\n",
        "Newsgroups: mod.one,mod.two\r\n",
        "From: poster@example.com\r\n",
        "Subject: t\r\n",
        "Date: Wed, 05 Oct 2022 00:00:00 GMT\r\n",
    );
    // build first signature for mod1
    let article1 = format!("{}Approved: mod1\r\n\r\nBody\n", base);
    let (_, msg1) = parse_message(&article1).unwrap();
    let signed = "Message-ID,Newsgroups,From,Subject,Approved,Date";
    let data1 = canonical_text(&msg1, signed);
    let (ver1, lines1) = build_sig(&data1);
    let mut x1 = format!("X-PGP-Sig: {} {}", ver1, signed);
    for l in &lines1 {
        x1.push_str("\r\n ");
        x1.push_str(l);
    }
    // build second signature for mod2
    let article2 = format!("{}Approved: mod2\r\n\r\nBody\n", base);
    let (_, msg2) = parse_message(&article2).unwrap();
    let data2 = canonical_text(&msg2, signed);
    let (ver2, lines2) = build_sig(&data2);
    let mut x2 = format!("X-PGP-Sig: {} {}", ver2, signed);
    for l in &lines2 {
        x2.push_str("\r\n ");
        x2.push_str(l);
    }
    format!(
        "{}Approved: mod1\r\nApproved: mod2\r\n{}\r\n{}\r\n\r\nBody\r\n.\r\n",
        base, x1, x2
    )
}

#[tokio::test]
async fn post_requires_approval_for_moderated_group() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let auth = Arc::new(SqliteAuth::new("sqlite::memory:").await.unwrap());
    storage.add_group("mod.test", true).await.unwrap();
    auth.add_user("user", "pass").await.unwrap();
    let (addr, cert, _pem, _h) = common::setup_tls_server(storage.clone(), auth.clone()).await;
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
    auth.update_pgp_key("user", ADMIN_PUB).await.unwrap();
    auth.add_moderator("user", "mod.*").await.unwrap();
    let (addr, cert, _pem, _h) = common::setup_tls_server(storage.clone(), auth.clone()).await;
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
    let article = build_article();
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

#[tokio::test]
async fn cross_post_different_moderators() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let auth = Arc::new(SqliteAuth::new("sqlite::memory:").await.unwrap());
    storage.add_group("mod.one", true).await.unwrap();
    storage.add_group("mod.two", true).await.unwrap();
    auth.add_user("poster", "pass").await.unwrap();
    auth.add_user("mod1", "x").await.unwrap();
    auth.add_user("mod2", "x").await.unwrap();
    auth.update_pgp_key("mod1", ADMIN_PUB).await.unwrap();
    auth.update_pgp_key("mod2", ADMIN_PUB).await.unwrap();
    auth.add_moderator("mod1", "mod.one").await.unwrap();
    auth.add_moderator("mod2", "mod.two").await.unwrap();
    let (addr, cert, _pem, _h) = common::setup_tls_server(storage.clone(), auth.clone()).await;
    let (mut reader, mut writer) = common::connect_tls(addr, cert).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"AUTHINFO USER poster\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"AUTHINFO PASS pass\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"MODE READER\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"GROUP mod.one\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"POST\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("340"));
    line.clear();
    let article = build_cross_article();
    writer.write_all(article.as_bytes()).await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("240"));
    assert_eq!(
        storage.list_article_numbers("mod.one").await.unwrap(),
        vec![1]
    );
    assert_eq!(
        storage.list_article_numbers("mod.two").await.unwrap(),
        vec![1]
    );
}
