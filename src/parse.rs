use nom::IResult;
use nom::{
    bytes::complete::{is_not, take_till, take_while1},
    character::complete::{char, crlf, digit1, space0, space1},
    combinator::{map_res, opt},
    multi::separated_list1,
    sequence::{preceded, tuple},
};

#[derive(Debug, PartialEq, Eq)]
pub struct Command {
    pub name: String,
    pub args: Vec<String>,
}

pub fn parse_command(input: &str) -> IResult<&str, Command> {
    let (input, name) = take_while1(|c: char| c.is_ascii_alphabetic())(input)?;
    let (input, args) = opt(preceded(space1, separated_list1(space1, is_not(" \r\n"))))(input)?;
    let (input, _) = opt(crlf)(input)?;
    let args_vec = args
        .unwrap_or_default()
        .into_iter()
        .map(|s: &str| s.to_string())
        .collect();
    Ok((
        input,
        Command {
            name: name.to_ascii_uppercase(),
            args: args_vec,
        },
    ))
}

#[derive(Debug, PartialEq, Eq)]
pub struct Response {
    pub code: u16,
    pub text: String,
}

pub fn parse_response(input: &str) -> IResult<&str, Response> {
    let parse_code = map_res(digit1, |d: &str| d.parse::<u16>());
    let (input, (code, text)) = tuple((
        parse_code,
        opt(preceded(char(' '), take_till(|c| c == '\r' || c == '\n'))),
    ))(input)?;
    let (input, _) = opt(crlf)(input)?;
    Ok((
        input,
        Response {
            code,
            text: text.unwrap_or("").to_string(),
        },
    ))
}

#[derive(Debug, PartialEq, Eq)]
pub struct Message {
    pub headers: Vec<(String, String)>,
    pub body: String,
}

fn parse_header_line(mut input: &str) -> IResult<&str, (String, String)> {
    let (i, name) = take_while1(|c: char| c != ':' && c != '\r' && c != '\n')(input)?;
    let (i, _) = char(':')(i)?;
    let (i, _) = space0(i)?;
    let (i, value) = take_till(|c| c == '\r' || c == '\n')(i)?;
    let (mut i, _) = crlf(i)?;
    let mut val = value.to_string();

    while i.starts_with(' ') || i.starts_with('\t') {
        let (next, _) = take_while1(|c| c == ' ' || c == '\t')(i)?;
        let (next, cont) = take_till(|c| c == '\r' || c == '\n')(next)?;
        let (next, _) = crlf(next)?;
        val.push(' ');
        val.push_str(cont);
        i = next;
    }

    input = i;
    Ok((input, (name.to_string(), val)))
}

fn parse_headers(mut input: &str) -> IResult<&str, Vec<(String, String)>> {
    let mut headers = Vec::new();
    loop {
        if let Some(rest) = input.strip_prefix("\r\n") {
            input = rest;
            break;
        }
        let (next, header) = parse_header_line(input)?;
        headers.push(header);
        input = next;
    }
    Ok((input, headers))
}

pub fn parse_message(input: &str) -> IResult<&str, Message> {
    let (input, headers) = parse_headers(input)?;
    let body = input.to_string();
    Ok(("", Message { headers, body }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_command_simple() {
        let (_, cmd) = parse_command("ARTICLE\r\n").unwrap();
        assert_eq!(cmd.name, "ARTICLE");
        assert!(cmd.args.is_empty());
    }

    #[test]
    fn test_parse_command_args() {
        let (_, cmd) = parse_command("GROUP comp.lang.rust\r\n").unwrap();
        assert_eq!(cmd.name, "GROUP");
        assert_eq!(cmd.args, vec!["comp.lang.rust"]);
    }

    #[test]
    fn test_parse_response() {
        let (_, resp) = parse_response("211 123 group selected\r\n").unwrap();
        assert_eq!(resp.code, 211);
        assert_eq!(resp.text, "123 group selected");
    }

    #[test]
    fn test_parse_response_no_text() {
        let (_, resp) = parse_response("200\r\n").unwrap();
        assert_eq!(resp.code, 200);
        assert_eq!(resp.text, "");
    }

    #[test]
    fn test_parse_message() {
        let input = "Subject: Test\r\nFrom: user@example.com\r\n\r\nThis is the body.";
        let (_, msg) = parse_message(input).unwrap();
        assert_eq!(msg.headers.len(), 2);
        assert_eq!(msg.headers[0], ("Subject".into(), "Test".into()));
        assert_eq!(msg.headers[1], ("From".into(), "user@example.com".into()));
        assert_eq!(msg.body, "This is the body.");
    }

    #[test]
    fn test_post_command_with_message() {
        let input = "POST\r\nSubject: Example\r\n\r\nBody text";
        let (rest, cmd) = parse_command(input).unwrap();
        assert_eq!(cmd.name, "POST");
        assert!(cmd.args.is_empty());
        let (_, msg) = parse_message(rest).unwrap();
        assert_eq!(msg.headers, vec![("Subject".into(), "Example".into())]);
        assert_eq!(msg.body, "Body text");
    }

    #[test]
    fn test_folded_headers() {
        let input = concat!(
            "Subject: A first",
            "\r\n",
            "\tcontinued",
            "\r\n",
            "From: user@example.com\r\n",
            "\r\n",
            "Body"
        );
        let (_, msg) = parse_message(input).unwrap();
        assert_eq!(msg.headers.len(), 2);
        assert_eq!(msg.headers[0], ("Subject".into(), "A first continued".into()));
        assert_eq!(msg.headers[1], ("From".into(), "user@example.com".into()));
        assert_eq!(msg.body, "Body");
    }
}

