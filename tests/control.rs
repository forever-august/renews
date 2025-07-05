use renews::auth::{AuthProvider, sqlite::SqliteAuth};
use renews::control::canonical_text;
use renews::parse_message;
use renews::storage::{sqlite::SqliteStorage, Storage};
use std::sync::Arc;

use test_utils::ClientMock;

async fn setup() -> (Arc<dyn Storage>, Arc<dyn AuthProvider>) {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let auth = Arc::new(SqliteAuth::new("sqlite::memory:").await.unwrap());
    (storage as _, auth as _)
}

const ADMIN_SEC: &str = include_str!("data/admin.sec.asc");
const ADMIN_PUB: &str = include_str!("data/admin.pub.asc");

fn build_sig(data: &str) -> (String, Vec<String>) {
    use pgp::composed::{Deserializable, SignedSecretKey, StandaloneSignature};
    use pgp::packet::SignatureConfig;
    use pgp::packet::SignatureType;
    use pgp::types::Password;
    use rand::thread_rng;

    let (key, _) = SignedSecretKey::from_string(ADMIN_SEC).unwrap();
    let cfg =
        SignatureConfig::from_key(thread_rng(), &key.primary_key, SignatureType::Binary).unwrap();
    let sig = cfg
        .sign(&key.primary_key, &Password::empty(), data.as_bytes())
        .unwrap();
    let armored = StandaloneSignature::new(sig)
        .to_armored_string(Default::default())
        .unwrap();
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

fn build_control_article(cmd: &str, body: &str) -> String {
    let headers = format!(
        "From: admin@example.org\r\nSubject: cmsg {c}\r\nControl: {c}\r\nMessage-ID: <ctrl@test>\r\nDate: Wed, 05 Oct 2022 00:00:00 GMT\r\n",
        c = cmd
    );
    let body = body.replace('\n', "\r\n");
    let article_text = format!("{}\r\n{}", headers, body);
    let (_, msg) = parse_message(&article_text).unwrap();
    let signed = "Subject,Control,Message-ID,Date,From,Sender";
    let data = canonical_text(&msg, signed);
    let (ver, lines) = build_sig(&data);
    let mut xhdr = format!("X-PGP-Sig: {} {}", ver, signed);
    for l in &lines {
        xhdr.push_str("\r\n ");
        xhdr.push_str(l);
    }
    let term = if body.ends_with("\r\n") {
        ".\r\n"
    } else {
        "\r\n.\r\n"
    };
    format!(
        "{}Newsgroups: test.group\r\n{}\r\n\r\n{}{}",
        headers, xhdr, body, term
    )
}

#[tokio::test]
async fn control_newgroup_and_rmgroup() {
    let (storage, auth) = setup().await;
    auth.add_user("admin@example.org", "x").await.unwrap();
    auth.add_admin("admin@example.org", ADMIN_PUB).await.unwrap();

    let article = build_control_article("newgroup test.group", "test group body\n");
    ClientMock::new()
        .expect("IHAVE <ctrl@test>", "335 Send it; end with <CR-LF>.<CR-LF>")
        .expect(article.trim_end_matches("\r\n"), "235 Article transferred OK")
        .run(storage.clone(), auth.clone())
        .await;
    assert!(
        storage
            .list_groups()
            .await
            .unwrap()
            .contains(&"test.group".to_string())
    );

    let article = build_control_article("rmgroup test.group", "rm body\n");
    ClientMock::new()
        .expect("IHAVE <ctrl2@test>", "335 Send it; end with <CR-LF>.<CR-LF>")
        .expect(article.trim_end_matches("\r\n"), "235 Article transferred OK")
        .run(storage.clone(), auth.clone())
        .await;
    assert!(
        !storage
            .list_groups()
            .await
            .unwrap()
            .contains(&"test.group".to_string())
    );
}

#[tokio::test]
async fn control_cancel_removes_article() {
    let (storage, auth) = setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, art) = parse_message(
        "Message-ID: <a@test>\r\nNewsgroups: misc.test\r\nFrom: u@test\r\nSubject: t\r\n\r\nBody",
    )
    .unwrap();
    storage.store_article("misc.test", &art).await.unwrap();
    auth.add_user("admin@example.org", "x").await.unwrap();
    auth.add_admin("admin@example.org", ADMIN_PUB).await.unwrap();
    let article = build_control_article("cancel <a@test>", "cancel\n");
    ClientMock::new()
        .expect("IHAVE <c@test>", "335 Send it; end with <CR-LF>.<CR-LF>")
        .expect(article.trim_end_matches("\r\n"), "235 Article transferred OK")
        .run(storage.clone(), auth)
        .await;
    assert!(
        storage
            .get_article_by_id("<a@test>")
            .await
            .unwrap()
            .is_none()
    );
}
#[tokio::test]
async fn admin_cancel_ignores_lock() {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use sha2::{Digest, Sha256};

    let (storage, auth) = setup().await;
    storage.add_group("misc.test", false).await.unwrap();

    // store an article with a cancel-lock
    let key = "secret";
    let key_b64 = STANDARD.encode(key);
    let lock_hash = Sha256::digest(key_b64.as_bytes());
    let lock_b64 = STANDARD.encode(lock_hash);
    let orig = format!(
        "Message-ID: <al@test>\r\nNewsgroups: misc.test\r\nCancel-Lock: sha256:{}\r\n\r\nBody",
        lock_b64
    );
    let (_, msg) = parse_message(&orig).unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();

    // admin setup
    auth.add_user("admin@example.org", "x").await.unwrap();
    auth.add_admin("admin@example.org", ADMIN_PUB).await.unwrap();

    let article = build_control_article("cancel <al@test>", "cancel\n");
    ClientMock::new()
        .expect("IHAVE <c2@test>", "335 Send it; end with <CR-LF>.<CR-LF>")
        .expect(article.trim_end_matches("\r\n"), "235 Article transferred OK")
        .run(storage.clone(), auth)
        .await;
    assert!(
        storage
            .get_article_by_id("<al@test>")
            .await
            .unwrap()
            .is_none()
    );
}
