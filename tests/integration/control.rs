use renews::control::canonical_text;
use renews::parse_message;

use crate::utils::{self, ClientMock, build_sig, collect_groups, store_test_article};

const ADMIN_PUB: &str = include_str!("../data/admin.pub.asc");

fn build_control_article(cmd: &str, body: &str) -> String {
    let headers = format!(
        "From: admin@example.org\r\nSubject: cmsg {cmd}\r\nControl: {cmd}\r\nMessage-ID: <ctrl@test>\r\nDate: Wed, 05 Oct 2022 00:00:00 GMT\r\n"
    );
    let body = body.replace('\n', "\r\n");
    let article_text = format!("{headers}\r\n{body}");
    let (_, msg) = parse_message(&article_text).unwrap();
    let signed = "Subject,Control,Message-ID,Date,From,Sender";
    let data = canonical_text(&msg, signed);
    let (ver, lines) = build_sig(&data);
    let mut xhdr = format!("X-PGP-Sig: {ver} {signed}");
    for l in &lines {
        xhdr.push_str("\r\n ");
        xhdr.push_str(l);
    }
    let term = if body.ends_with("\r\n") {
        ".\r\n"
    } else {
        "\r\n.\r\n"
    };
    format!("{headers}Newsgroups: test.group\r\n{xhdr}\r\n\r\n{body}{term}")
}

#[tokio::test]
async fn control_newgroup_and_rmgroup() {
    let (storage, auth) = utils::setup().await;
    auth.add_user("admin@example.org", "x").await.unwrap();
    auth.add_admin("admin@example.org", ADMIN_PUB)
        .await
        .unwrap();

    let article = build_control_article("newgroup test.group", "test group body\n");
    ClientMock::new()
        .expect("IHAVE <ctrl@test>", "335 Send it; end with <CR-LF>.<CR-LF>")
        .expect_request_multi(
            utils::request_lines(article.trim_end_matches("\r\n")),
            vec!["235 Article transferred OK"],
        )
        .run(storage.clone(), auth.clone())
        .await;
    let groups = collect_groups(&*storage).await;
    assert!(groups.contains(&"test.group".to_string()));

    let article = build_control_article("rmgroup test.group", "rm body\n");
    ClientMock::new()
        .expect(
            "IHAVE <ctrl2@test>",
            "335 Send it; end with <CR-LF>.<CR-LF>",
        )
        .expect_request_multi(
            utils::request_lines(article.trim_end_matches("\r\n")),
            vec!["235 Article transferred OK"],
        )
        .run(storage.clone(), auth.clone())
        .await;
    let groups = collect_groups(&*storage).await;
    assert!(!groups.contains(&"test.group".to_string()));
}

#[tokio::test]
async fn control_cancel_removes_article() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    store_test_article(
        &*storage,
        "Message-ID: <a@test>\r\nNewsgroups: misc.test\r\nFrom: u@test\r\nSubject: t\r\n\r\nBody",
    )
    .await;
    auth.add_user("admin@example.org", "x").await.unwrap();
    auth.add_admin("admin@example.org", ADMIN_PUB)
        .await
        .unwrap();
    let article = build_control_article("cancel <a@test>", "cancel\n");
    ClientMock::new()
        .expect("IHAVE <c@test>", "335 Send it; end with <CR-LF>.<CR-LF>")
        .expect_request_multi(
            utils::request_lines(article.trim_end_matches("\r\n")),
            vec!["235 Article transferred OK"],
        )
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
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use sha2::{Digest, Sha256};

    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();

    // store an article with a cancel-lock
    let key = "secret";
    let key_b64 = STANDARD.encode(key);
    let lock_hash = Sha256::digest(key_b64.as_bytes());
    let lock_b64 = STANDARD.encode(lock_hash);
    let orig = format!(
        "Message-ID: <al@test>\r\nNewsgroups: misc.test\r\nCancel-Lock: sha256:{lock_b64}\r\n\r\nBody"
    );
    store_test_article(&*storage, &orig).await;

    // admin setup
    auth.add_user("admin@example.org", "x").await.unwrap();
    auth.add_admin("admin@example.org", ADMIN_PUB)
        .await
        .unwrap();

    let article = build_control_article("cancel <al@test>", "cancel\n");
    ClientMock::new()
        .expect("IHAVE <c2@test>", "335 Send it; end with <CR-LF>.<CR-LF>")
        .expect_request_multi(
            utils::request_lines(article.trim_end_matches("\r\n")),
            vec!["235 Article transferred OK"],
        )
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
