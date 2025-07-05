use chrono::{Duration, Utc};
use renews::parse_message;
use renews::storage::{sqlite::SqliteStorage, Storage};
use std::sync::Arc;

use test_utils::ClientMock;

async fn setup() -> (Arc<dyn Storage>, Arc<dyn renews::auth::AuthProvider>) {
    use renews::auth::sqlite::SqliteAuth;
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let auth = Arc::new(SqliteAuth::new("sqlite::memory:").await.unwrap());
    (storage as _, auth as _)
}

fn capabilities_lines() -> Vec<String> {
    vec![
        "101 Capability list follows".into(),
        "VERSION 2".into(),
        format!("IMPLEMENTATION Renews {}", env!("CARGO_PKG_VERSION")),
        "READER".into(),
        "NEWNEWS".into(),
        "IHAVE".into(),
        "STREAMING".into(),
        "OVER MSGID".into(),
        "HDR".into(),
        "LIST ACTIVE NEWSGROUPS ACTIVE.TIMES OVERVIEW.FMT HEADERS".into(),
        ".".into(),
    ]
}

fn help_lines() -> Vec<String> {
    vec![
        "100 help text follows".into(),
        "CAPABILITIES".into(),
        "MODE READER".into(),
        "MODE STREAM".into(),
        "GROUP".into(),
        "LIST".into(),
        "LISTGROUP".into(),
        "ARTICLE".into(),
        "HEAD".into(),
        "BODY".into(),
        "STAT".into(),
        "HDR".into(),
        "OVER".into(),
        "NEXT".into(),
        "LAST".into(),
        "NEWGROUPS".into(),
        "NEWNEWS".into(),
        "IHAVE".into(),
        "CHECK".into(),
        "TAKETHIS".into(),
        "POST".into(),
        "DATE".into(),
        "HELP".into(),
        "QUIT".into(),
        ".".into(),
    ]
}

#[tokio::test]
async fn head_and_list_commands() {
    let (storage, auth) = setup().await;
    storage.add_group("misc", false).await.unwrap();
    let (_, msg) =
        parse_message("Message-ID: <1@test>\r\nSubject: T\r\n\r\nBody").unwrap();
    storage.store_article("misc", &msg).await.unwrap();

    ClientMock::new()
        .expect("MODE READER", "201 Posting prohibited")
        .expect("GROUP misc", "211 1 1 1 misc")
        .expect_multi(
            "HEAD 1",
            vec![
                "221 1 <1@test> article headers follow",
                "Message-ID: <1@test>",
                "Subject: T",
                ".",
            ],
        )
        .expect_multi(
            "LIST",
            vec!["215 list of newsgroups follows", "misc 1 1 y", "."],
        )
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn listgroup_and_navigation_commands() {
    let (storage, auth) = setup().await;
    storage.add_group("misc", false).await.unwrap();
    let (_, m1) = parse_message("Message-ID: <1@test>\r\n\r\nA").unwrap();
    let (_, m2) = parse_message("Message-ID: <2@test>\r\n\r\nB").unwrap();
    storage.store_article("misc", &m1).await.unwrap();
    storage.store_article("misc", &m2).await.unwrap();

    let future = Utc::now() + Duration::seconds(1);
    let date = future.format("%Y%m%d");
    let time = future.format("%H%M%S");

    ClientMock::new()
        .expect("MODE READER", "201 Posting prohibited")
        .expect("GROUP misc", "211 2 1 2 misc")
        .expect_multi(
            "LISTGROUP",
            vec!["211 article numbers follow", "1", "2", "."],
        )
        .expect_multi(
            "HEAD 1",
            vec!["221 1 <1@test> article headers follow", "Message-ID: <1@test>", "."],
        )
        .expect("NEXT", "223 2 <2@test> article exists")
        .expect("LAST", "223 1 <1@test> article exists")
        .expect_multi(
            "NEWGROUPS 19700101 000000",
            vec!["231 list of new newsgroups follows", "misc", "."],
        )
        .expect_multi(
            &format!("NEWGROUPS {} {}", date, time),
            vec!["231 list of new newsgroups follows", "."],
        )
        .expect_multi(
            "NEWNEWS misc 19700101 000000",
            vec!["230 list of new articles follows", "<1@test>", "<2@test>", "."],
        )
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn capabilities_and_misc_commands() {
    let (storage, auth) = setup().await;
    storage.add_group("misc", false).await.unwrap();

    let date = Utc::now().format("%Y%m%d%H%M%S").to_string();

    ClientMock::new()
        .expect_multi("CAPABILITIES", capabilities_lines())
        .expect("DATE", &format!("111 {}", date))
        .expect_multi("HELP", help_lines())
        .expect_multi(
            "LIST NEWSGROUPS",
            vec!["215 descriptions follow", "misc ", "."],
        )
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn no_group_returns_412() {
    let (storage, auth) = setup().await;
    storage.add_group("misc", false).await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc", &msg).await.unwrap();

    ClientMock::new()
        .expect("MODE READER", "201 Posting prohibited")
        .expect("HEAD 1", "412 no newsgroup selected")
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn responses_include_number_and_id() {
    let (storage, auth) = setup().await;
    storage.add_group("misc", false).await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc", &msg).await.unwrap();

    ClientMock::new()
        .expect("MODE READER", "201 Posting prohibited")
        .expect("GROUP misc", "211 1 1 1 misc")
        .expect_multi(
            "HEAD 1",
            vec!["221 1 <1@test> article headers follow", "Message-ID: <1@test>", "."],
        )
        .expect_multi(
            "BODY 1",
            vec!["222 1 <1@test> article body follows", "Body", "."],
        )
        .expect("STAT 1", "223 1 <1@test> article exists")
        .expect_multi(
            "ARTICLE 1",
            vec![
                "220 1 <1@test> article follows",
                "Message-ID: <1@test>",
                "",
                "Body",
                ".",
            ],
        )
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn post_and_dot_stuffing() {
    let (storage, auth) = setup().await;
    storage.add_group("misc", false).await.unwrap();

    ClientMock::new()
        .expect("MODE READER", "201 Posting prohibited")
        .expect("GROUP misc", "211 0 0 0 misc")
        .expect("POST", "483 Secure connection required")
        .expect("QUIT", "205 closing connection")
        .run(storage.clone(), auth)
        .await;

    assert!(
        storage
            .get_article_by_id("<3@test>")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn body_returns_proper_crlf() {
    let (storage, auth) = setup().await;
    storage.add_group("misc", false).await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nline1\r\nline2\r\n").unwrap();
    storage.store_article("misc", &msg).await.unwrap();

    ClientMock::new()
        .expect("MODE READER", "201 Posting prohibited")
        .expect("GROUP misc", "211 1 1 1 misc")
        .expect_multi(
            "BODY 1",
            vec![
                "222 1 <1@test> article body follows",
                "line1",
                "line2",
                ".",
            ],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn newgroups_accepts_gmt_argument() {
    let (storage, auth) = setup().await;
    storage.add_group("misc", false).await.unwrap();

    ClientMock::new()
        .expect("MODE READER", "201 Posting prohibited")
        .expect_multi(
            "NEWGROUPS 19700101 000000 GMT",
            vec!["231 list of new newsgroups follows", "misc", "."],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn newnews_accepts_gmt_argument() {
    let (storage, auth) = setup().await;
    storage.add_group("misc", false).await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc", &msg).await.unwrap();

    ClientMock::new()
        .expect("MODE READER", "201 Posting prohibited")
        .expect_multi(
            "NEWNEWS misc 19700101 000000 GMT",
            vec!["230 list of new articles follows", "<1@test>", "."],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn post_without_selecting_group() {
    let (storage, auth) = setup().await;
    storage.add_group("misc", false).await.unwrap();

    ClientMock::new()
        .expect("MODE READER", "201 Posting prohibited")
        .expect("POST", "483 Secure connection required")
        .run(storage.clone(), auth)
        .await;

    assert!(
        storage
            .get_article_by_id("<post@test>")
            .await
            .unwrap()
            .is_none()
    );
}
