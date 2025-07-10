use crate::utils::{self, ClientMock};

#[tokio::test]
async fn tls_quit() {
    let (storage, auth) = utils::setup().await;
    ClientMock::new()
        .expect("QUIT", "205 closing connection")
        .run_tls(storage, auth)
        .await;
}

#[tokio::test]
async fn tls_mode_reader() {
    let (storage, auth) = utils::setup().await;
    ClientMock::new()
        .expect("MODE READER", "200 Posting allowed")
        .run_tls(storage, auth)
        .await;
}

#[tokio::test]
async fn tls_post_requires_auth() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc", false).await.unwrap();
    ClientMock::new()
        .expect("MODE READER", "200 Posting allowed")
        .expect("GROUP misc", "211 0 0 0 misc")
        .expect("POST", "480 authentication required")
        .run_tls(storage.clone(), auth)
        .await;
    assert!(
        storage
            .get_article_by_id("<post@test>")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn tls_authinfo_and_post() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc", false).await.unwrap();
    auth.add_user("user", "pass").await.unwrap();
    let article = concat!(
        "Message-ID: <post@test>\r\n",
        "Newsgroups: misc\r\n",
        "From: user@example.com\r\n",
        "Subject: test\r\n",
        "\r\n",
        "Body\r\n",
        ".",
    );
    ClientMock::new()
        .expect("AUTHINFO USER user", "381 password required")
        .expect("AUTHINFO PASS pass", "281 authentication accepted")
        .expect("MODE READER", "200 Posting allowed")
        .expect("GROUP misc", "211 0 0 0 misc")
        .expect(
            "POST",
            "340 send article to be posted. End with <CR-LF>.<CR-LF>",
        )
        .expect_request_multi(
            utils::request_lines(article.trim_end_matches("\r\n")),
            vec!["240 article received"],
        )
        .expect("QUIT", "205 closing connection")
        .run_tls(storage.clone(), auth)
        .await;

    // Wait for queue processing
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    assert!(
        storage
            .get_article_by_id("<post@test>")
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn post_without_msgid_generates_one() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc", false).await.unwrap();
    auth.add_user("user", "pass").await.unwrap();
    let article = concat!(
        "Newsgroups: misc\r\n",
        "From: user@example.com\r\n",
        "Subject: test\r\n",
        "\r\n",
        "Body\r\n",
        ".",
    );
    ClientMock::new()
        .expect("AUTHINFO USER user", "381 password required")
        .expect("AUTHINFO PASS pass", "281 authentication accepted")
        .expect("MODE READER", "200 Posting allowed")
        .expect("GROUP misc", "211 0 0 0 misc")
        .expect(
            "POST",
            "340 send article to be posted. End with <CR-LF>.<CR-LF>",
        )
        .expect_request_multi(
            utils::request_lines(article.trim_end_matches("\r\n")),
            vec!["240 article received"],
        )
        .run_tls(storage.clone(), auth.clone())
        .await;

    // Wait for queue processing
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    use sha1::{Digest, Sha1};
    let hash = Sha1::digest(b"Body\r\n");
    let id = format!(
        "<{}>",
        hash.iter().map(|b| format!("{b:02x}")).collect::<String>()
    );
    assert!(storage.get_article_by_id(&id).await.unwrap().is_some());
}

#[tokio::test]
async fn post_without_date_adds_header() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc", false).await.unwrap();
    auth.add_user("user", "pass").await.unwrap();
    let article = concat!(
        "Newsgroups: misc\r\n",
        "From: user@example.com\r\n",
        "Subject: test\r\n",
        "\r\n",
        "Body\r\n",
        ".",
    );
    ClientMock::new()
        .expect("AUTHINFO USER user", "381 password required")
        .expect("AUTHINFO PASS pass", "281 authentication accepted")
        .expect("MODE READER", "200 Posting allowed")
        .expect("GROUP misc", "211 0 0 0 misc")
        .expect(
            "POST",
            "340 send article to be posted. End with <CR-LF>.<CR-LF>",
        )
        .expect_request_multi(
            utils::request_lines(article.trim_end_matches("\r\n")),
            vec!["240 article received"],
        )
        .run_tls(storage.clone(), auth.clone())
        .await;

    // Wait for queue processing
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    use sha1::{Digest, Sha1};
    let hash = Sha1::digest(b"Body\r\n");
    let id = format!(
        "<{}>",
        hash.iter().map(|b| format!("{b:02x}")).collect::<String>()
    );
    let msg = storage.get_article_by_id(&id).await.unwrap().unwrap();
    let date = msg
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("Date"))
        .map(|(_, v)| v.clone());
    assert!(date.is_some());
    chrono::DateTime::parse_from_rfc2822(&date.unwrap()).unwrap();
}
