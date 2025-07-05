use crate::utils::{self, ClientMock};
use renews::{parse_command, parse_message, parse_response};

#[tokio::test]
async fn unknown_command_mail() {
    let (storage, auth) = utils::setup().await;
    ClientMock::new()
        .expect("MAIL", "500 command not recognized")
        .run(storage, auth)
        .await;
}

#[test]
fn parse_example_commands() {
    // Commands from the examples in RFC 3977 section 3.2.1.1 and the
    // introductory simple session.
    let examples = vec![
        ("MODE READER\r\n", "MODE", vec!["READER"]),
        ("GROUP misc.test\r\n", "GROUP", vec!["misc.test"]),
        ("ARTICLE 1\r\n", "ARTICLE", vec!["1"]),
        ("MAIL\r\n", "MAIL", vec![]),
        ("CAPABILITIES\r\n", "CAPABILITIES", vec![]),
        ("OVER\r\n", "OVER", vec![]),
        ("MODE POSTER\r\n", "MODE", vec!["POSTER"]),
        (
            "ARTICLE a.message.id@no.angle.brackets\r\n",
            "ARTICLE",
            vec!["a.message.id@no.angle.brackets"],
        ),
        ("HEAD 53 54 55\r\n", "HEAD", vec!["53", "54", "55"]),
        ("LIST ACTIVE u[ks].*\r\n", "LIST", vec!["ACTIVE", "u[ks].*"]),
        (
            "XENCRYPT RSA abcd=efg\r\n",
            "XENCRYPT",
            vec!["RSA", "abcd=efg"],
        ),
        (
            "IHAVE <i.am.an.article.you.will.want@example.com>\r\n",
            "IHAVE",
            vec!["<i.am.an.article.you.will.want@example.com>"],
        ),
        ("GROUP secret.group\r\n", "GROUP", vec!["secret.group"]),
        (
            "XSECRET fred flintstone\r\n",
            "XSECRET",
            vec!["fred", "flintstone"],
        ),
        ("XENCRYPT\r\n", "XENCRYPT", vec![]),
        ("GROUP binary.group\r\n", "GROUP", vec!["binary.group"]),
        (
            "XHOST binary.news.example.org\r\n",
            "XHOST",
            vec!["binary.news.example.org"],
        ),
        ("GROUP archive.local\r\n", "GROUP", vec!["archive.local"]),
        ("ARTICLE 123\r\n", "ARTICLE", vec!["123"]),
        ("QUIT\r\n", "QUIT", vec![]),
    ];

    for (input, name, args) in examples {
        let (_, cmd) = parse_command(input).unwrap();
        assert_eq!(cmd.name, name);
        assert_eq!(cmd.args, args);
    }
}

#[test]
fn parse_example_responses() {
    let examples = vec![
        (
            "200 news.example.com server ready (posting allowed)\r\n",
            200,
        ),
        ("211 1 1 1 misc.test\r\n", 211),
        (
            "340 send article to be posted. End with <CR-LF>.<CR-LF>\r\n",
            340,
        ),
        ("500 Unknown command\r\n", 500),
        ("101 Capability list:\r\n", 101),
        ("500 Unknown command\r\n", 500),
        ("501 Unknown MODE option\r\n", 501),
        ("501 Syntax error\r\n", 501),
        ("501 Too many arguments\r\n", 501),
        ("501 Syntax error\r\n", 501),
        ("504 Base64 encoding error\r\n", 504),
        ("200 Reader mode, posting permitted\r\n", 200),
        ("500 Permission denied\r\n", 500),
        ("480 Permission denied\r\n", 480),
        ("290 Password for fred accepted\r\n", 290),
        ("211 5 1 20 secret.group selected\r\n", 211),
        ("483 Secure connection required\r\n", 483),
        ("283 Encrypted link established\r\n", 283),
        ("211 5 1 20 secret.group selected\r\n", 211),
        ("401 XHOST Not on this virtual host\r\n", 401),
        ("290 binary.news.example.org virtual host selected\r\n", 290),
        ("211 5 1 77 binary.group selected\r\n", 211),
        ("403 Archive server temporarily offline\r\n", 403),
        ("400 Power supply failed, running on UPS\r\n", 400),
        ("205 closing connection\r\n", 205),
    ];

    for (input, code) in examples {
        let (_, resp) = parse_response(input).unwrap();
        assert_eq!(resp.code, code);
    }
}

#[test]
fn parse_example_article() {
    // Article used in the POST examples in RFC 3977 section 6.3.1.3
    let text = concat!(
        "From: \"Demo User\" <nobody@example.net>\r\n",
        "Newsgroups: misc.test\r\n",
        "Subject: I am just a test article\r\n",
        "Organization: An Example Net\r\n",
        "\r\n",
        "This is just a test article."
    );
    let (_, msg) = parse_message(text).unwrap();
    assert_eq!(msg.headers.len(), 4);
    assert_eq!(
        msg.headers[0],
        ("From".into(), "\"Demo User\" <nobody@example.net>".into())
    );
    assert_eq!(msg.headers[1], ("Newsgroups".into(), "misc.test".into()));
    assert_eq!(
        msg.headers[2],
        ("Subject".into(), "I am just a test article".into())
    );
    assert_eq!(
        msg.headers[3],
        ("Organization".into(), "An Example Net".into())
    );
    assert_eq!(msg.body, "This is just a test article.");
}

#[tokio::test]
async fn capabilities_and_unknown_command() {
    let (storage, auth) = utils::setup().await;
    ClientMock::new()
        .expect_multi("CAPABILITIES", utils::capabilities_lines())
        .expect("OVER", "412 no newsgroup selected")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn unsupported_mode_variant() {
    let (storage, auth) = utils::setup().await;
    ClientMock::new()
        .expect("MODE POSTER", "501 unknown mode")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn article_syntax_error() {
    let (storage, auth) = utils::setup().await;
    ClientMock::new()
        .expect("ARTICLE a.message.id@no.angle.brackets", "501 invalid id")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn head_without_group_returns_412() {
    let (storage, auth) = utils::setup().await;
    ClientMock::new()
        .expect("HEAD 1", "412 no newsgroup selected")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn list_unknown_keyword() {
    let (storage, auth) = utils::setup().await;
    ClientMock::new()
        .expect_multi(
            "LIST ACTIVE u[ks].*",
            vec![
                String::from("215 list of newsgroups follows"),
                String::from("."),
            ],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn list_distrib_pats_not_supported() {
    let (storage, auth) = utils::setup().await;
    ClientMock::new()
        .expect("LIST DISTRIB.PATS", "503 feature not supported")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn unknown_command_xencrypt() {
    let (storage, auth) = utils::setup().await;
    ClientMock::new()
        .expect("XENCRYPT RSA abcd=efg", "500 command not recognized")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn mode_reader_success() {
    let (storage, auth) = utils::setup().await;
    ClientMock::new()
        .expect("MODE READER", "201 Posting prohibited")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn commands_are_case_insensitive() {
    let (storage, auth) = utils::setup().await;
    ClientMock::new()
        .expect("mode reader", "201 Posting prohibited")
        .expect("quit", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn group_select_returns_211() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    ClientMock::new()
        .expect("GROUP misc.test", "211 0 0 0 misc.test")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn article_success_by_number() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    ClientMock::new()
        .expect("GROUP misc.test", "211 1 1 1 misc.test")
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
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn article_success_by_id() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    ClientMock::new()
        .expect_multi(
            "ARTICLE <1@test>",
            vec![
                "220 0 <1@test> article follows",
                "Message-ID: <1@test>",
                "",
                "Body",
                ".",
            ],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn article_id_not_found() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    ClientMock::new()
        .expect("ARTICLE <nope@id>", "430 no such article")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn article_number_no_group() {
    let (storage, auth) = utils::setup().await;
    ClientMock::new()
        .expect("ARTICLE 1", "412 no newsgroup selected")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn head_success_by_number() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    ClientMock::new()
        .expect("GROUP misc.test", "211 1 1 1 misc.test")
        .expect_multi(
            "HEAD 1",
            vec![
                "221 1 <1@test> article headers follow",
                "Message-ID: <1@test>",
                ".",
            ],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn head_success_by_id() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    ClientMock::new()
        .expect_multi(
            "HEAD <1@test>",
            vec![
                "221 0 <1@test> article headers follow",
                "Message-ID: <1@test>",
                ".",
            ],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn head_number_not_found() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    ClientMock::new()
        .expect("GROUP misc.test", "211 1 1 1 misc.test")
        .expect("HEAD 2", "423 no such article number in this group")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn head_id_not_found() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    ClientMock::new()
        .expect("HEAD <nope@id>", "430 no such article")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn head_no_current_article_selected() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    ClientMock::new()
        .expect("GROUP misc.test", "211 0 0 0 misc.test")
        .expect("HEAD", "420 no current article selected")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn body_success_by_number() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    ClientMock::new()
        .expect("GROUP misc.test", "211 1 1 1 misc.test")
        .expect_multi(
            "BODY 1",
            vec!["222 1 <1@test> article body follows", "Body", "."],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn body_success_by_id() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    ClientMock::new()
        .expect_multi(
            "BODY <1@test>",
            vec!["222 0 <1@test> article body follows", "Body", "."],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn body_number_not_found() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    ClientMock::new()
        .expect("GROUP misc.test", "211 1 1 1 misc.test")
        .expect("BODY 2", "423 no such article number in this group")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn body_id_not_found() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    ClientMock::new()
        .expect("BODY <nope@id>", "430 no such article")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn body_number_no_group() {
    let (storage, auth) = utils::setup().await;
    ClientMock::new()
        .expect("BODY 1", "412 no newsgroup selected")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn stat_success_by_number() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    ClientMock::new()
        .expect("GROUP misc.test", "211 1 1 1 misc.test")
        .expect("STAT 1", "223 1 <1@test> article exists")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn stat_success_by_id() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    ClientMock::new()
        .expect("STAT <1@test>", "223 0 <1@test> article exists")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn stat_number_not_found() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    ClientMock::new()
        .expect("GROUP misc.test", "211 1 1 1 misc.test")
        .expect("STAT 2", "423 no such article number in this group")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn stat_id_not_found() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    ClientMock::new()
        .expect("STAT <nope@id>", "430 no such article")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn stat_number_no_group() {
    let (storage, auth) = utils::setup().await;
    ClientMock::new()
        .expect("STAT 1", "412 no newsgroup selected")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn listgroup_returns_numbers() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    ClientMock::new()
        .expect_multi(
            "LISTGROUP misc.test",
            vec!["211 article numbers follow", "1", "."],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn listgroup_without_group_selected() {
    let (storage, auth) = utils::setup().await;
    ClientMock::new()
        .expect("LISTGROUP", "412 no newsgroup selected")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn list_newsgroups_returns_groups() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    storage.add_group("alt.test", false).await.unwrap();
    ClientMock::new()
        .expect_multi(
            "LIST NEWSGROUPS",
            vec!["215 descriptions follow", "alt.test ", "misc.test ", "."],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn list_all_keywords() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let ts = storage
        .list_groups_with_times()
        .await
        .unwrap()
        .into_iter()
        .find(|(g, _)| g == "misc.test")
        .unwrap()
        .1;
    ClientMock::new()
        .expect_multi(
            "LIST ACTIVE",
            vec!["215 list of newsgroups follows", "misc.test 0 0 y", "."],
        )
        .expect_multi(
            "LIST ACTIVE.TIMES",
            vec![
                "215 information follows".into(),
                format!("misc.test {} -", ts),
                ".".into(),
            ],
        )
        .expect_multi(
            "LIST OVERVIEW.FMT",
            vec![
                "215 Order of fields in overview database.",
                "Subject:",
                "From:",
                "Date:",
                "Message-ID:",
                "References:",
                ":bytes",
                ":lines",
                ".",
            ],
        )
        .expect_multi(
            "LIST HEADERS",
            vec![
                "215 metadata items supported:",
                ":",
                ":lines",
                ":bytes",
                ".",
            ],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn newnews_lists_recent_articles() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    ClientMock::new()
        .expect_multi(
            "NEWNEWS misc.test 19700101 000000",
            vec!["230 list of new articles follows", "<1@test>", "."],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn newnews_no_matches_returns_empty() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    use chrono::{Duration, Utc};
    let future = Utc::now() + Duration::seconds(1);
    let date = future.format("%Y%m%d");
    let time = future.format("%H%M%S");
    ClientMock::new()
        .expect_multi(
            &format!("NEWNEWS misc.test {} {}", date, time),
            vec!["230 list of new articles follows", "."],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn hdr_subject_by_message_id() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\nSubject: Hello\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    ClientMock::new()
        .expect_multi(
            "HDR Subject <1@test>",
            vec!["225 Headers follow", "0 Hello", "."],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn hdr_subject_range() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, m1) = parse_message("Message-ID: <1@test>\r\nSubject: A\r\n\r\nBody").unwrap();
    let (_, m2) = parse_message("Message-ID: <2@test>\r\nSubject: B\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &m1).await.unwrap();
    storage.store_article("misc.test", &m2).await.unwrap();
    ClientMock::new()
        .expect("GROUP misc.test", "211 2 1 2 misc.test")
        .expect_multi(
            "HDR Subject 1-2",
            vec!["225 Headers follow", "1 A", "2 B", "."],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn hdr_all_headers_message_id() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, msg) =
        parse_message("Message-ID: <1@test>\r\nSubject: Hello\r\nFrom: a@test\r\n\r\nBody")
            .unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    ClientMock::new()
        .expect_multi(
            "HDR : <1@test>",
            vec![
                "225 Headers follow",
                "0 Message-ID: <1@test>",
                "0 Subject: Hello",
                "0 From: a@test",
                ".",
            ],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn xpat_subject_message_id() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\nSubject: Hello\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    ClientMock::new()
        .expect_multi(
            "XPAT Subject <1@test> *ell*",
            vec!["221 Header follows", "0 Hello", "."],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn xpat_subject_range() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, m1) = parse_message("Message-ID: <1@test>\r\nSubject: apple\r\n\r\nBody").unwrap();
    let (_, m2) = parse_message("Message-ID: <2@test>\r\nSubject: banana\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &m1).await.unwrap();
    storage.store_article("misc.test", &m2).await.unwrap();
    ClientMock::new()
        .expect("GROUP misc.test", "211 2 1 2 misc.test")
        .expect_multi(
            "XPAT Subject 1-2 *a*",
            vec!["221 Header follows", "1 apple", "2 banana", "."],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn over_message_id() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, msg) =
        parse_message("Message-ID: <1@test>\r\nSubject: A\r\nFrom: a@test\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    ClientMock::new()
        .expect_multi(
            "OVER <1@test>",
            vec![
                "224 Overview information follows",
                "0\tA\ta@test\t\t<1@test>\t\t4\t1",
                ".",
            ],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn over_range() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, m1) =
        parse_message("Message-ID: <1@test>\r\nSubject: A\r\nFrom: a@test\r\n\r\nBody").unwrap();
    let (_, m2) =
        parse_message("Message-ID: <2@test>\r\nSubject: B\r\nFrom: b@test\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &m1).await.unwrap();
    storage.store_article("misc.test", &m2).await.unwrap();
    ClientMock::new()
        .expect("GROUP misc.test", "211 2 1 2 misc.test")
        .expect_multi(
            "OVER 1-2",
            vec![
                "224 Overview information follows",
                "1\tA\ta@test\t\t<1@test>\t\t4\t1",
                "2\tB\tb@test\t\t<2@test>\t\t4\t1",
                ".",
            ],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn head_range() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, m1) = parse_message("Message-ID: <1@test>\r\n\r\nA").unwrap();
    let (_, m2) = parse_message("Message-ID: <2@test>\r\n\r\nB").unwrap();
    storage.store_article("misc.test", &m1).await.unwrap();
    storage.store_article("misc.test", &m2).await.unwrap();
    ClientMock::new()
        .expect("GROUP misc.test", "211 2 1 2 misc.test")
        .expect_multi(
            "HEAD 1-2",
            vec![
                "221 1 <1@test> article headers follow",
                "Message-ID: <1@test>",
                ".",
                "221 2 <2@test> article headers follow",
                "Message-ID: <2@test>",
                ".",
            ],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn body_range() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, m1) = parse_message("Message-ID: <1@test>\r\n\r\nA").unwrap();
    let (_, m2) = parse_message("Message-ID: <2@test>\r\n\r\nB").unwrap();
    storage.store_article("misc.test", &m1).await.unwrap();
    storage.store_article("misc.test", &m2).await.unwrap();
    ClientMock::new()
        .expect("GROUP misc.test", "211 2 1 2 misc.test")
        .expect_multi(
            "BODY 1-2",
            vec![
                "222 1 <1@test> article body follows",
                "A",
                ".",
                "222 2 <2@test> article body follows",
                "B",
                ".",
            ],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn article_range() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, m1) = parse_message("Message-ID: <1@test>\r\n\r\nA").unwrap();
    let (_, m2) = parse_message("Message-ID: <2@test>\r\n\r\nB").unwrap();
    storage.store_article("misc.test", &m1).await.unwrap();
    storage.store_article("misc.test", &m2).await.unwrap();
    ClientMock::new()
        .expect("GROUP misc.test", "211 2 1 2 misc.test")
        .expect_multi(
            "ARTICLE 1-2",
            vec![
                "220 1 <1@test> article follows",
                "Message-ID: <1@test>",
                "",
                "A",
                ".",
                "220 2 <2@test> article follows",
                "Message-ID: <2@test>",
                "",
                "B",
                ".",
            ],
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn ihave_example() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();

    let article = concat!(
        "Path: pathost!demo!somewhere!not-for-mail\r\n",
        "From: \"Demo User\" <nobody@example.com>\r\n",
        "Newsgroups: misc.test\r\n",
        "Subject: I am just a test article\r\n",
        "Date: 6 Oct 1998 04:38:40 -0500\r\n",
        "Organization: An Example Com, San Jose, CA\r\n",
        "Message-ID: <i.am.an.article.you.will.want@example.com>\r\n",
        "\r\n",
        "This is just a test article.\r\n",
        ".\r\n"
    );

    ClientMock::new()
        .expect(
            "IHAVE <i.am.an.article.you.will.want@example.com>",
            "335 Send it; end with <CR-LF>.<CR-LF>",
        )
        .expect(
            article.trim_end_matches("\r\n"),
            "235 Article transferred OK",
        )
        .expect(
            "IHAVE <i.am.an.article.you.will.want@example.com>",
            "435 article not wanted",
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn takethis_example() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let (_, exist) = parse_message(
        "Message-ID: <i.am.an.article.you.have@example.com>\r\nNewsgroups: misc.test\r\n\r\nBody",
    )
    .unwrap();
    storage.store_article("misc.test", &exist).await.unwrap();

    let take_article = concat!(
        "TAKETHIS <i.am.an.article.new@example.com>\r\n",
        "Path: pathost!demo!somewhere!not-for-mail\r\n",
        "From: \"Demo User\" <nobody@example.com>\r\n",
        "Newsgroups: misc.test\r\n",
        "Subject: I am just a test article\r\n",
        "Date: 6 Oct 1998 04:38:40 -0500\r\n",
        "Organization: An Example Com, San Jose, CA\r\n",
        "Message-ID: <i.am.an.article.new@example.com>\r\n",
        "\r\n",
        "This is just a test article.\r\n",
        ".\r\n"
    );

    let take_reject = concat!(
        "TAKETHIS <i.am.an.article.you.have@example.com>\r\n",
        "Path: pathost!demo!somewhere!not-for-mail\r\n",
        "From: \"Demo User\" <nobody@example.com>\r\n",
        "Newsgroups: misc.test\r\n",
        "Subject: I am just a test article\r\n",
        "Date: 6 Oct 1998 04:38:40 -0500\r\n",
        "Organization: An Example Com, San Jose, CA\r\n",
        "Message-ID: <i.am.an.article.you.have@example.com>\r\n",
        "\r\n",
        "This is just a test article.\r\n",
        ".\r\n"
    );
    ClientMock::new()
        .expect(
            take_article.trim_end_matches("\r\n"),
            "239 <i.am.an.article.new@example.com>",
        )
        .expect(
            take_reject.trim_end_matches("\r\n"),
            "439 <i.am.an.article.you.have@example.com>",
        )
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn mode_stream_check_and_takethis() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    ClientMock::new()
        .expect("MODE STREAM", "203 Streaming permitted")
        .expect("CHECK <stream1@test>", "238 <stream1@test>")
        .expect("CHECK <stream2@test>", "238 <stream2@test>")
        .expect(
            concat!(
                "TAKETHIS <stream1@test>\r\n",
                "Newsgroups: misc.test\r\n",
                "From: a@test\r\n",
                "Subject: one\r\n",
                "Message-ID: <stream1@test>\r\n",
                "\r\n",
                "Body one\r\n",
                ".\r\n"
            )
            .trim_end_matches("\r\n"),
            "239 <stream1@test>",
        )
        .expect(
            concat!(
                "TAKETHIS <stream2@test>\r\n",
                "Newsgroups: misc.test\r\n",
                "From: b@test\r\n",
                "Subject: two\r\n",
                "Message-ID: <stream2@test>\r\n",
                "\r\n",
                "Body two\r\n",
                ".\r\n"
            )
            .trim_end_matches("\r\n"),
            "239 <stream2@test>",
        )
        .expect("CHECK <stream1@test>", "438 <stream1@test>")
        .expect("CHECK <stream2@test>", "438 <stream2@test>")
        .run(storage, auth)
        .await;
}
