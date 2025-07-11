use futures_util::StreamExt;
use renews::control::canonical_text;
use renews::parse_message;

use crate::utils::{self, ClientMock, build_sig};

const ADMIN_PUB: &str = include_str!("../data/admin.pub.asc");

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
    let article_text = format!("{headers}\r\n{body}");
    let (_, msg) = parse_message(&article_text).unwrap();
    let signed = "Message-ID,Newsgroups,From,Subject,Approved,Date";
    let data = canonical_text(&msg, signed);
    let (ver, lines) = build_sig(&data);
    let mut xhdr = format!("X-PGP-Sig: {ver} {signed}");
    for l in &lines {
        xhdr.push_str("\r\n ");
        xhdr.push_str(l);
    }
    format!("{headers}{xhdr}\r\n\r\nBody\r\n.\r\n")
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
    let article1 = format!("{base}Approved: mod1\r\n\r\nBody\n");
    let (_, msg1) = parse_message(&article1).unwrap();
    let signed = "Message-ID,Newsgroups,From,Subject,Approved,Date";
    let data1 = canonical_text(&msg1, signed);
    let (ver1, lines1) = build_sig(&data1);
    let mut x1 = format!("X-PGP-Sig: {ver1} {signed}");
    for l in &lines1 {
        x1.push_str("\r\n ");
        x1.push_str(l);
    }
    // build second signature for mod2
    let article2 = format!("{base}Approved: mod2\r\n\r\nBody\n");
    let (_, msg2) = parse_message(&article2).unwrap();
    let data2 = canonical_text(&msg2, signed);
    let (ver2, lines2) = build_sig(&data2);
    let mut x2 = format!("X-PGP-Sig: {ver2} {signed}");
    for l in &lines2 {
        x2.push_str("\r\n ");
        x2.push_str(l);
    }
    format!("{base}Approved: mod1\r\nApproved: mod2\r\n{x1}\r\n{x2}\r\n\r\nBody\r\n.\r\n")
}

#[tokio::test]
async fn post_requires_approval_for_moderated_group() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("mod.test", true).await.unwrap();
    auth.add_user("user", "pass").await.unwrap();
    ClientMock::new()
        .expect("AUTHINFO USER user", "381 password required")
        .expect("AUTHINFO PASS pass", "281 authentication accepted")
        .expect("MODE READER", "200 Posting allowed")
        .expect("GROUP mod.test", "211 0 0 0 mod.test")
        .expect(
            "POST",
            "340 send article to be posted. End with <CR-LF>.<CR-LF>",
        )
        .expect(
            concat!(
                "Message-ID: <p@test>\r\n",
                "Newsgroups: mod.test\r\n",
                "From: user@example.com\r\n",
                "Subject: t\r\n",
                "\r\n",
                "Body\r\n",
                ".",
            ),
            "441 posting failed",
        )
        .run_tls(storage.clone(), auth)
        .await;
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
    let (storage, auth) = utils::setup().await;
    storage.add_group("mod.test", true).await.unwrap();
    auth.add_user("user", "pass").await.unwrap();
    auth.update_pgp_key("user", ADMIN_PUB).await.unwrap();
    auth.add_moderator("user", "mod.*").await.unwrap();
    let article = build_article();
    ClientMock::new()
        .expect("AUTHINFO USER user", "381 password required")
        .expect("AUTHINFO PASS pass", "281 authentication accepted")
        .expect("MODE READER", "200 Posting allowed")
        .expect("GROUP mod.test", "211 0 0 0 mod.test")
        .expect(
            "POST",
            "340 send article to be posted. End with <CR-LF>.<CR-LF>",
        )
        .expect_request_multi(
            utils::request_lines(article.trim_end_matches("\r\n")),
            vec!["240 article received"],
        )
        .run_tls(storage.clone(), auth)
        .await;

    // Wait for queue processing
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

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
    let (storage, auth) = utils::setup().await;
    storage.add_group("mod.one", true).await.unwrap();
    storage.add_group("mod.two", true).await.unwrap();
    auth.add_user("poster", "pass").await.unwrap();
    auth.add_user("mod1", "x").await.unwrap();
    auth.add_user("mod2", "x").await.unwrap();
    auth.update_pgp_key("mod1", ADMIN_PUB).await.unwrap();
    auth.update_pgp_key("mod2", ADMIN_PUB).await.unwrap();
    auth.add_moderator("mod1", "mod.one").await.unwrap();
    auth.add_moderator("mod2", "mod.two").await.unwrap();
    let article = build_cross_article();
    ClientMock::new()
        .expect("AUTHINFO USER poster", "381 password required")
        .expect("AUTHINFO PASS pass", "281 authentication accepted")
        .expect("MODE READER", "200 Posting allowed")
        .expect("GROUP mod.one", "211 0 0 0 mod.one")
        .expect(
            "POST",
            "340 send article to be posted. End with <CR-LF>.<CR-LF>",
        )
        .expect_request_multi(
            utils::request_lines(article.trim_end_matches("\r\n")),
            vec!["240 article received"],
        )
        .run_tls(storage.clone(), auth)
        .await;

    // Wait for queue processing
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let mut mod_one_nums = Vec::new();
    let mut stream = storage.list_article_numbers("mod.one");
    while let Some(result) = stream.next().await {
        mod_one_nums.push(result.unwrap());
    }
    assert_eq!(mod_one_nums, vec![1]);
    
    let mut mod_two_nums = Vec::new();
    let mut stream = storage.list_article_numbers("mod.two");
    while let Some(result) = stream.next().await {
        mod_two_nums.push(result.unwrap());
    }
    assert_eq!(mod_two_nums, vec![1]);
}
