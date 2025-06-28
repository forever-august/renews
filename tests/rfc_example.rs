use renews::{parse_command, parse_response, parse_message};

#[test]
fn parse_example_commands() {
    // Example commands from RFC 3977 section illustrating a simple session
    let (_, cmd) = parse_command("MODE READER\r\n").unwrap();
    assert_eq!(cmd.name, "MODE");
    assert_eq!(cmd.args, vec!["READER"]);

    let (_, cmd) = parse_command("GROUP misc.test\r\n").unwrap();
    assert_eq!(cmd.name, "GROUP");
    assert_eq!(cmd.args, vec!["misc.test"]);

    let (_, cmd) = parse_command("ARTICLE 1\r\n").unwrap();
    assert_eq!(cmd.name, "ARTICLE");
    assert_eq!(cmd.args, vec!["1"]);

    let (_, cmd) = parse_command("QUIT\r\n").unwrap();
    assert_eq!(cmd.name, "QUIT");
    assert!(cmd.args.is_empty());
}

#[test]
fn parse_example_responses() {
    let (_, resp) = parse_response(
        "200 news.example.com server ready (posting allowed)\r\n",
    )
    .unwrap();
    assert_eq!(resp.code, 200);

    let (_, resp) = parse_response("211 1 1 1 misc.test\r\n").unwrap();
    assert_eq!(resp.code, 211);

    let (_, resp) = parse_response(
        "340 send article to be posted. End with <CR-LF>.<CR-LF>\r\n",
    )
    .unwrap();
    assert_eq!(resp.code, 340);

    let (_, resp) = parse_response("205 closing connection\r\n").unwrap();
    assert_eq!(resp.code, 205);
}

#[test]
fn parse_example_article() {
    let text = "From: test@example.com\r\nSubject: Test article\r\n\r\nThis is a test.";
    let (_, msg) = parse_message(text).unwrap();
    assert_eq!(msg.headers.len(), 2);
    assert_eq!(msg.headers[0], ("From".into(), "test@example.com".into()));
    assert_eq!(msg.headers[1], ("Subject".into(), "Test article".into()));
    assert_eq!(msg.body, "This is a test.");
}
