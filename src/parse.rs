use crate::storage::DynStorage;
use anyhow;
use chrono::TimeZone;
use futures_util::{TryStreamExt, future};
use nom::IResult;
use nom::{
    bytes::complete::{is_not, take_till, take_while1},
    character::complete::{char, crlf, digit1, space0, space1},
    combinator::{map_res, opt},
    multi::separated_list1,
    sequence::{preceded, tuple},
};
use sha1::{Digest, Sha1};
use smallvec::SmallVec;
#[cfg(test)]
use smallvec::smallvec;
use std::fmt::Write;

#[derive(Debug, PartialEq, Eq)]
pub struct Command {
    pub name: String,
    pub args: Vec<String>,
}

/// Parse a single NNTP command line as described in RFC 3977
/// Section 3.1 "Commands and Responses".
///
/// # Errors
///
/// Returns a parsing error if the input is not a valid NNTP command.
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

/// Parse an NNTP response line as specified in RFC 3977
/// Section 9.4.2 "Initial Response Line Contents".
///
/// # Errors
///
/// Returns a parsing error if the input is not a valid NNTP response.
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

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Message {
    pub headers: SmallVec<[(String, String); 8]>,
    pub body: String,
}

/// Unescape a Message-ID according to RFC 2822 quoted-pair rules.
/// This removes surrounding whitespace and comments, strips quote
/// characters when present and processes backslash escapes.
#[must_use]
pub fn unescape_message_id(id: &str) -> String {
    let mut out = String::new();
    let trimmed = id.trim();
    if !trimmed.starts_with('<') || !trimmed.ends_with('>') {
        return trimmed.to_string();
    }
    out.push('<');
    let mut chars = trimmed[1..trimmed.len() - 1].chars().peekable();
    let mut in_quotes = false;
    let mut in_brackets = false;
    let mut comment_depth = 0u32;
    while let Some(c) = chars.next() {
        if comment_depth > 0 {
            if c == '(' {
                comment_depth += 1;
            } else if c == ')' {
                comment_depth -= 1;
            }
            continue;
        }
        match c {
            '"' if !in_brackets => {
                in_quotes = !in_quotes;
            }
            '[' if !in_quotes => {
                in_brackets = true;
                out.push('[');
            }
            ']' if in_brackets => {
                in_brackets = false;
                out.push(']');
            }
            '\\' if in_quotes || in_brackets => {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            }
            '(' if !in_quotes && !in_brackets => {
                comment_depth = 1;
            }
            c if c.is_whitespace() && !in_quotes && !in_brackets => {}
            _ => out.push(c),
        }
    }
    out.push('>');
    out
}

/// Escape a Message-ID by quoting components when necessary and
/// escaping special characters. The input should already be unescaped.
#[must_use]
pub fn escape_message_id(id: &str) -> String {
    let trimmed = id.trim();
    if !trimmed.starts_with('<') || !trimmed.ends_with('>') {
        return trimmed.to_string();
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    let Some((left, right)) = inner.split_once('@') else {
        return trimmed.to_string();
    };
    let mut esc_left = String::new();
    let needs_quote = left.chars().any(|c| {
        !(c.is_ascii_alphanumeric()
            || matches!(
                c,
                '!' | '#'
                    | '$'
                    | '%'
                    | '&'
                    | '\''
                    | '*'
                    | '+'
                    | '-'
                    | '/'
                    | '='
                    | '?'
                    | '^'
                    | '_'
                    | '`'
                    | '{'
                    | '|'
                    | '}'
                    | '~'
                    | '.'
            ))
    });
    if needs_quote {
        esc_left.push('"');
    }
    for ch in left.chars() {
        if ch == '"' || ch == '\\' {
            esc_left.push('\\');
        }
        esc_left.push(ch);
    }
    if needs_quote {
        esc_left.push('"');
    }

    let mut esc_right = String::new();
    if right.starts_with('[') && right.ends_with(']') {
        esc_right.push('[');
        let inner = &right[1..right.len() - 1];
        for ch in inner.chars() {
            if ch == '\\' || ch == ']' || ch == '[' {
                esc_right.push('\\');
            }
            esc_right.push(ch);
        }
        esc_right.push(']');
    } else {
        esc_right.push_str(right);
    }
    format!("<{esc_left}@{esc_right}>")
}

/// Replace the Message-ID header of `msg` with an escaped version.
pub fn escape_message_id_header(msg: &mut Message) {
    for (name, val) in &mut msg.headers {
        if name.eq_ignore_ascii_case("Message-ID") {
            *val = escape_message_id(val);
        }
    }
}

/// Parse a single article header line including folded continuation
/// lines as defined in RFC 3977 Section 3.6 "Articles".
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

/// Parse the header block of an article until the blank line
/// separating headers from the body, as specified in RFC 3977
/// Section 3.6.
fn parse_headers(mut input: &str) -> IResult<&str, SmallVec<[(String, String); 8]>> {
    let mut headers = SmallVec::new();
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

/// Parse an entire article consisting of headers and body
/// following the rules in RFC 3977 Section 3.6.
///
/// # Errors
///
/// Returns a parsing error if the input is not a valid message format.
pub fn parse_message(input: &str) -> IResult<&str, Message> {
    let (input, mut headers) = parse_headers(input)?;
    for (name, val) in &mut headers {
        if name.eq_ignore_ascii_case("Message-ID") {
            *val = unescape_message_id(val);
        }
    }
    let body = input.to_string();
    Ok(("", Message { headers, body }))
}

/// Ensure a Message-ID header is present. When missing, one is
/// generated by hashing the article body using SHA-1 and the provided domain.
pub fn ensure_message_id(msg: &mut Message, domain: &str) {
    if msg
        .headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("Message-ID"))
    {
        return;
    }
    let hash = Sha1::digest(msg.body.as_bytes());
    let mut hex = String::new();
    for b in hash {
        let _ = write!(hex, "{b:02x}");
    }
    msg.headers
        .push(("Message-ID".into(), format!("<{hex}@{domain}>")));
}

/// Ensure a Date header is present. When missing, one is set to the current
/// time in RFC 2822 format.
pub fn ensure_date(msg: &mut Message) {
    if msg
        .headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("Date"))
    {
        return;
    }
    let now = chrono::Utc::now();
    msg.headers.push(("Date".into(), now.to_rfc2822()));
}

/// Parse the date and time arguments used by NEWGROUPS and NEWNEWS
/// commands as described in RFC 3977 Sections 7.3.1 and 7.4.1.
///
/// # Errors
///
/// Returns an error if the date or time format is invalid.
pub fn parse_datetime(
    date: &str,
    time: &str,
    gmt: bool,
) -> Result<chrono::DateTime<chrono::Utc>, &'static str> {
    if !(date.len() == 6 || date.len() == 8) || !date.chars().all(|c| c.is_ascii_digit()) {
        return Err("invalid date");
    }
    if time.len() != 6 || !time.chars().all(|c| c.is_ascii_digit()) {
        return Err("invalid time");
    }
    let fmt = if date.len() == 6 { "%y%m%d" } else { "%Y%m%d" };
    let naive_date = chrono::NaiveDate::parse_from_str(date, fmt).map_err(|_| "invalid date")?;
    let naive_time =
        chrono::NaiveTime::parse_from_str(time, "%H%M%S").map_err(|_| "invalid time")?;
    let naive = naive_date.and_time(naive_time);
    Ok(if gmt {
        chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(naive, chrono::Utc)
    } else {
        chrono::Local
            .from_local_datetime(&naive)
            .single()
            .ok_or("invalid local time")?
            .with_timezone(&chrono::Utc)
    })
}

/// Parse the article number range format used by several commands
/// such as LISTGROUP as defined in RFC 3977 Section 6.1.2.
///
/// # Errors
///
/// Returns an error if the range format is invalid or if there's a storage error.
pub async fn parse_range(
    storage: &DynStorage,
    group: &str,
    spec: &str,
) -> anyhow::Result<Vec<u64>> {
    if let Some((start_s, end_s)) = spec.split_once('-') {
        let start: u64 = start_s.parse().map_err(|_| anyhow::anyhow!("invalid range"))?;
        if end_s.is_empty() {
            let stream = storage.list_article_numbers(group);
            let nums = stream
                .try_filter(|n| future::ready(*n >= start))
                .try_collect::<Vec<u64>>()
                .await?;
            Ok(nums)
        } else {
            let end: u64 = end_s.parse().map_err(|_| anyhow::anyhow!("invalid range"))?;
            if end < start {
                return Ok(Vec::new());
            }
            Ok((start..=end).collect())
        }
    } else {
        Ok(vec![spec.parse()?])
    }
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
        let expected_headers: SmallVec<[(String, String); 8]> =
            smallvec![("Subject".to_string(), "Example".to_string())];
        assert_eq!(msg.headers, expected_headers);
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
        assert_eq!(
            msg.headers[0],
            ("Subject".into(), "A first continued".into())
        );
        assert_eq!(msg.headers[1], ("From".into(), "user@example.com".into()));
        assert_eq!(msg.body, "Body");
    }

    #[test]
    fn test_ensure_message_id_adds_header() {
        let (_, mut msg) = parse_message("Newsgroups: misc\r\n\r\nBody").unwrap();
        ensure_message_id(&mut msg, "example.com");
        assert!(
            msg.headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("Message-ID"))
        );
    }

    #[test]
    fn test_ensure_message_id_preserves_existing() {
        let (_, mut msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
        ensure_message_id(&mut msg, "example.com");
        let ids: Vec<_> = msg
            .headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("Message-ID"))
            .map(|(_, v)| v.clone())
            .collect();
        assert_eq!(ids, vec!["<1@test>".to_string()]);
    }

    #[test]
    fn test_ensure_message_id_format() {
        let (_, mut msg) = parse_message("Newsgroups: misc\r\n\r\nTestBody").unwrap();
        ensure_message_id(&mut msg, "example.com");
        let msg_id = msg
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Message-ID"))
            .map(|(_, v)| v.clone())
            .unwrap();

        // Should be in format <hash@domain>
        assert!(msg_id.starts_with("<"));
        assert!(msg_id.ends_with(">"));
        assert!(msg_id.contains("@example.com"));

        // Extract the hash part and verify it's a valid SHA1 hex string
        let inner = &msg_id[1..msg_id.len() - 1]; // Remove < >
        let (hash, domain) = inner.split_once('@').unwrap();
        assert_eq!(domain, "example.com");
        assert_eq!(hash.len(), 40); // SHA1 hex is 40 characters
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_ensure_date_adds_header() {
        let (_, mut msg) = parse_message("Newsgroups: misc\r\n\r\nBody").unwrap();
        ensure_date(&mut msg);
        assert!(
            msg.headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("Date"))
        );
    }

    #[test]
    fn test_ensure_date_preserves_existing() {
        let (_, mut msg) = parse_message("Date: 6 Oct 1998 04:38:40 -0500\r\n\r\nBody").unwrap();
        ensure_date(&mut msg);
        let dates: Vec<_> = msg
            .headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("Date"))
            .map(|(_, v)| v.clone())
            .collect();
        assert_eq!(dates, vec!["6 Oct 1998 04:38:40 -0500".to_string()]);
    }

    #[test]
    fn test_escape_unescape_message_id() {
        let text = "Message-ID: <\"id\\\"left\"@example.com>\r\n\r\nB";
        let (_, mut msg) = parse_message(text).unwrap();
        let id_unescaped = msg
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Message-ID"))
            .map(|(_, v)| v.clone())
            .unwrap();
        assert_eq!(id_unescaped, "<id\"left@example.com>");
        escape_message_id_header(&mut msg);
        let id_escaped = msg
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Message-ID"))
            .map(|(_, v)| v.clone())
            .unwrap();
        assert_eq!(id_escaped, "<\"id\\\"left\"@example.com>");
    }
}
