use renews::{parse_command, parse_response, parse_message};

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
        ("XENCRYPT RSA abcd=efg\r\n", "XENCRYPT", vec!["RSA", "abcd=efg"]),
        (
            "IHAVE <i.am.an.article.you.will.want@example.com>\r\n",
            "IHAVE",
            vec!["<i.am.an.article.you.will.want@example.com>"],
        ),
        ("GROUP secret.group\r\n", "GROUP", vec!["secret.group"]),
        ("XSECRET fred flintstone\r\n", "XSECRET", vec!["fred", "flintstone"]),
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
        ("200 news.example.com server ready (posting allowed)\r\n", 200),
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
        (
            "290 binary.news.example.org virtual host selected\r\n",
            290,
        ),
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
    assert_eq!(msg.headers[0], ("From".into(), "\"Demo User\" <nobody@example.net>".into()));
    assert_eq!(msg.headers[1], ("Newsgroups".into(), "misc.test".into()));
    assert_eq!(msg.headers[2], ("Subject".into(), "I am just a test article".into()));
    assert_eq!(msg.headers[3], ("Organization".into(), "An Example Net".into()));
    assert_eq!(msg.body, "This is just a test article.");
}
