use nom::{
    bytes::complete::{is_not, take_till, take_while1},
    character::complete::{char, crlf, digit1, space0, space1},
    combinator::{map_res, opt},
    multi::separated_list1,
    sequence::{preceded, tuple},
};
use nom::IResult;

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
    Ok((input, Command { name: name.to_string(), args: args_vec }))
}

#[derive(Debug, PartialEq, Eq)]
pub struct Response {
    pub code: u16,
    pub text: String,
}

pub fn parse_response(input: &str) -> IResult<&str, Response> {
    let parse_code = map_res(digit1, |d: &str| d.parse::<u16>());
    let (input, (code, text)) = tuple((parse_code, opt(preceded(char(' '), take_till(|c| c == '\r' || c == '\n')))))(input)?;
    let (input, _) = opt(crlf)(input)?;
    Ok((input, Response { code, text: text.unwrap_or("").to_string() }))
}

#[derive(Debug, PartialEq, Eq)]
pub struct Message {
    pub headers: Vec<(String, String)>,
    pub body: String,
}

fn parse_header_line(input: &str) -> IResult<&str, (String, String)> {
    let (input, name) = take_while1(|c: char| c != ':' && c != '\r' && c != '\n')(input)?;
    let (input, _) = char(':')(input)?;
    let (input, _) = space0(input)?;
    let (input, value) = take_till(|c| c == '\r' || c == '\n')(input)?;
    let (input, _) = crlf(input)?;
    Ok((input, (name.to_string(), value.to_string())))
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

pub mod storage;
pub mod wildmat;

use std::error::Error;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use crate::storage::DynStorage;

async fn send_headers<W: AsyncWrite + Unpin>(writer: &mut W, article: &Message) -> Result<(), Box<dyn Error + Send + Sync>> {
    for (k, v) in article.headers.iter() {
        writer
            .write_all(format!("{}: {}\r\n", k, v).as_bytes())
            .await?;
    }
    Ok(())
}

async fn handle_quit<W: AsyncWrite + Unpin>(writer: &mut W) -> Result<bool, Box<dyn Error + Send + Sync>> {
    writer.write_all(b"205 closing connection\r\n").await?;
    Ok(true)
}

async fn handle_group<W: AsyncWrite + Unpin>(
    writer: &mut W,
    args: &[String],
    current_group: &mut String,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(name) = args.get(0) {
        *current_group = name.clone();
        writer
            .write_all(format!("211 0 1 1 {}\r\n", name).as_bytes())
            .await?;
    } else {
        writer.write_all(b"411 missing group\r\n").await?;
    }
    Ok(())
}

async fn handle_article<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    args: &[String],
    current_group: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(arg) = args.get(0) {
        if arg.starts_with('<') && arg.ends_with('>') {
            if let Some(article) = storage.get_article_by_id(arg).await? {
                writer.write_all(b"220 0 article follows\r\n").await?;
                send_headers(writer, &article).await?;
                writer.write_all(b"\r\n").await?;
                writer.write_all(article.body.as_bytes()).await?;
                writer.write_all(b"\r\n.\r\n").await?;
            } else {
                writer.write_all(b"430 no such article\r\n").await?;
            }
        } else if let Ok(num) = arg.parse::<u64>() {
            if let Some(article) = storage.get_article_by_number(current_group, num).await? {
                writer.write_all(b"220 0 article follows\r\n").await?;
                send_headers(writer, &article).await?;
                writer.write_all(b"\r\n").await?;
                writer.write_all(article.body.as_bytes()).await?;
                writer.write_all(b"\r\n.\r\n").await?;
            } else {
                writer
                    .write_all(b"423 no such article number in this group\r\n")
                    .await?;
            }
        } else {
            writer.write_all(b"501 invalid id\r\n").await?;
        }
    } else {
        writer.write_all(b"501 missing id\r\n").await?;
    }
    Ok(())
}

async fn handle_head<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    args: &[String],
    current_group: &str,
    current_article: &mut Option<u64>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(arg) = args.get(0) {
        if arg.starts_with('<') && arg.ends_with('>') {
            if let Some(article) = storage.get_article_by_id(arg).await? {
                writer.write_all(b"221 0 article headers follow\r\n").await?;
                send_headers(writer, &article).await?;
                writer.write_all(b".\r\n").await?;
            } else {
                writer.write_all(b"430 no such article\r\n").await?;
            }
        } else if let Ok(num) = arg.parse::<u64>() {
            if let Some(article) = storage.get_article_by_number(current_group, num).await? {
                *current_article = Some(num);
                writer.write_all(b"221 0 article headers follow\r\n").await?;
                send_headers(writer, &article).await?;
                writer.write_all(b".\r\n").await?;
            } else {
                writer
                    .write_all(b"423 no such article number in this group\r\n")
                    .await?;
            }
        } else {
            writer.write_all(b"501 invalid id\r\n").await?;
        }
    } else if let Some(num) = *current_article {
        if let Some(article) = storage.get_article_by_number(current_group, num).await? {
            writer.write_all(b"221 0 article headers follow\r\n").await?;
            send_headers(writer, &article).await?;
            writer.write_all(b".\r\n").await?;
        } else {
            writer.write_all(b"420 no current article selected\r\n").await?;
        }
    } else {
        writer.write_all(b"420 no current article selected\r\n").await?;
    }
    Ok(())
}

async fn handle_body<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    args: &[String],
    current_group: &str,
    current_article: &mut Option<u64>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(arg) = args.get(0) {
        if arg.starts_with('<') && arg.ends_with('>') {
            if let Some(article) = storage.get_article_by_id(arg).await? {
                writer.write_all(b"222 0 article body follows\r\n").await?;
                writer.write_all(article.body.as_bytes()).await?;
                writer.write_all(b"\r\n.\r\n").await?;
            } else {
                writer.write_all(b"430 no such article\r\n").await?;
            }
        } else if let Ok(num) = arg.parse::<u64>() {
            if let Some(article) = storage.get_article_by_number(current_group, num).await? {
                *current_article = Some(num);
                writer.write_all(b"222 0 article body follows\r\n").await?;
                writer.write_all(article.body.as_bytes()).await?;
                writer.write_all(b"\r\n.\r\n").await?;
            } else {
                writer
                    .write_all(b"423 no such article number in this group\r\n")
                    .await?;
            }
        } else {
            writer.write_all(b"501 invalid id\r\n").await?;
        }
    } else if let Some(num) = *current_article {
        if let Some(article) = storage.get_article_by_number(current_group, num).await? {
            writer.write_all(b"222 0 article body follows\r\n").await?;
            writer.write_all(article.body.as_bytes()).await?;
            writer.write_all(b"\r\n.\r\n").await?;
        } else {
            writer.write_all(b"420 no current article selected\r\n").await?;
        }
    } else {
        writer.write_all(b"420 no current article selected\r\n").await?;
    }
    Ok(())
}

async fn handle_stat<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    args: &[String],
    current_group: &str,
    current_article: &mut Option<u64>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(arg) = args.get(0) {
        if let Ok(num) = arg.parse::<u64>() {
            if storage.get_article_by_number(current_group, num).await?.is_some() {
                *current_article = Some(num);
                writer.write_all(b"223 0 article exists\r\n").await?;
            } else {
                writer
                    .write_all(b"423 no such article number in this group\r\n")
                    .await?;
            }
        } else if arg.starts_with('<') && arg.ends_with('>') {
            if storage.get_article_by_id(arg).await?.is_some() {
                writer.write_all(b"223 0 article exists\r\n").await?;
            } else {
                writer.write_all(b"430 no such article\r\n").await?;
            }
        } else {
            writer.write_all(b"501 invalid id\r\n").await?;
        }
    } else if let Some(num) = *current_article {
        if storage.get_article_by_number(current_group, num).await?.is_some() {
            writer.write_all(b"223 0 article exists\r\n").await?;
        } else {
            writer.write_all(b"420 no current article selected\r\n").await?;
        }
    } else {
        writer.write_all(b"420 no current article selected\r\n").await?;
    }
    Ok(())
}

async fn handle_list<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    args: &[String],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(keyword) = args.get(0) {
        if keyword.eq_ignore_ascii_case("NEWSGROUPS") {
            let groups = storage.list_groups().await?;
            writer.write_all(b"215 descriptions follow\r\n").await?;
            for g in groups {
                writer
                    .write_all(format!("{} \r\n", g).as_bytes())
                    .await?;
            }
            writer.write_all(b".\r\n").await?;
            return Ok(());
        } else {
            writer.write_all(b"501 unknown keyword\r\n").await?;
            return Ok(());
        }
    }

    let groups = storage.list_groups().await?;
    writer.write_all(b"215 list of newsgroups follows\r\n").await?;
    for g in groups {
        writer.write_all(format!("{} 0 0 y\r\n", g).as_bytes()).await?;
    }
    writer.write_all(b".\r\n").await?;
    Ok(())
}

async fn handle_listgroup<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    args: &[String],
    current_group: &mut String,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let group = if let Some(name) = args.get(0) {
        *current_group = name.clone();
        name.as_str()
    } else {
        current_group.as_str()
    };
    let nums = storage.list_article_numbers(group).await?;
    writer.write_all(b"211 article numbers follow\r\n").await?;
    for n in nums {
        writer.write_all(format!("{}\r\n", n).as_bytes()).await?;
    }
    writer.write_all(b".\r\n").await?;
    Ok(())
}

async fn handle_next<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    current_group: &str,
    current_article: &mut Option<u64>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(curr) = *current_article {
        let next = curr + 1;
        if storage
            .get_article_by_number(current_group, next)
            .await?
            .is_some()
        {
            *current_article = Some(next);
            writer.write_all(b"223 0 article exists\r\n").await?;
        } else {
            writer.write_all(b"421 no next article\r\n").await?;
        }
    } else {
        writer.write_all(b"420 no current article selected\r\n").await?;
    }
    Ok(())
}

async fn handle_last<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    current_group: &str,
    current_article: &mut Option<u64>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(curr) = *current_article {
        if curr > 1 {
            let prev = curr - 1;
            if storage
                .get_article_by_number(current_group, prev)
                .await?
                .is_some()
            {
                *current_article = Some(prev);
                writer.write_all(b"223 0 article exists\r\n").await?;
            } else {
                writer.write_all(b"422 no previous article\r\n").await?;
            }
        } else {
            writer.write_all(b"422 no previous article\r\n").await?;
        }
    } else {
        writer.write_all(b"420 no current article selected\r\n").await?;
    }
    Ok(())
}

async fn handle_newgroups<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    args: &[String],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if args.len() < 2 {
        writer.write_all(b"501 not enough arguments\r\n").await?;
        return Ok(());
    }

    let date = &args[0];
    let time = &args[1];
    if !(date.len() == 6 || date.len() == 8) || !date.chars().all(|c| c.is_ascii_digit()) {
        writer.write_all(b"501 invalid date\r\n").await?;
        return Ok(());
    }
    if time.len() != 6 || !time.chars().all(|c| c.is_ascii_digit()) {
        writer.write_all(b"501 invalid time\r\n").await?;
        return Ok(());
    }
    if let Some(arg) = args.get(2) {
        if !arg.eq_ignore_ascii_case("GMT") {
            writer.write_all(b"501 invalid argument\r\n").await?;
            return Ok(());
        }
    }

    let fmt = if date.len() == 6 { "%y%m%d" } else { "%Y%m%d" };
    let naive_date = match chrono::NaiveDate::parse_from_str(date, fmt) {
        Ok(d) => d,
        Err(_) => {
            writer.write_all(b"501 invalid date\r\n").await?;
            return Ok(());
        }
    };
    let naive_time = match chrono::NaiveTime::parse_from_str(time, "%H%M%S") {
        Ok(t) => t,
        Err(_) => {
            writer.write_all(b"501 invalid time\r\n").await?;
            return Ok(());
        }
    };
    let since = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
        naive_date.and_time(naive_time),
        chrono::Utc,
    );

    let groups = storage.list_groups_since(since).await?;
    writer
        .write_all(b"231 list of new newsgroups follows\r\n")
        .await?;
    for g in groups {
        writer.write_all(format!("{}\r\n", g).as_bytes()).await?;
    }
    writer.write_all(b".\r\n").await?;
    Ok(())
}

async fn handle_newnews<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    args: &[String],
    _current_group: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if args.len() < 3 {
        writer.write_all(b"501 not enough arguments\r\n").await?;
        return Ok(());
    }

    let wildmat = &args[0];
    let date = &args[1];
    let time = &args[2];
    if !(date.len() == 6 || date.len() == 8) || !date.chars().all(|c| c.is_ascii_digit()) {
        writer.write_all(b"501 invalid date\r\n").await?;
        return Ok(());
    }
    if time.len() != 6 || !time.chars().all(|c| c.is_ascii_digit()) {
        writer.write_all(b"501 invalid time\r\n").await?;
        return Ok(());
    }
    if let Some(arg) = args.get(3) {
        if !arg.eq_ignore_ascii_case("GMT") {
            writer.write_all(b"501 invalid argument\r\n").await?;
            return Ok(());
        }
    }

    let fmt = if date.len() == 6 { "%y%m%d" } else { "%Y%m%d" };
    let naive_date = match chrono::NaiveDate::parse_from_str(date, fmt) {
        Ok(d) => d,
        Err(_) => {
            writer.write_all(b"501 invalid date\r\n").await?;
            return Ok(());
        }
    };
    let naive_time = match chrono::NaiveTime::parse_from_str(time, "%H%M%S") {
        Ok(t) => t,
        Err(_) => {
            writer.write_all(b"501 invalid time\r\n").await?;
            return Ok(());
        }
    };
    let since = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
        naive_date.and_time(naive_time),
        chrono::Utc,
    );

    let groups = storage.list_groups().await?;
    let mut ids = Vec::new();
    for g in groups {
        if wildmat::wildmat(wildmat, &g) {
            ids.extend(storage.list_article_ids_since(&g, since).await?);
        }
    }

    writer
        .write_all(b"230 list of new articles follows\r\n")
        .await?;
    for id in ids {
        writer.write_all(format!("{}\r\n", id).as_bytes()).await?;
    }
    writer.write_all(b".\r\n").await?;
    Ok(())
}

async fn handle_capabilities<W: AsyncWrite + Unpin>(
    writer: &mut W,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.write_all(b"101 Capability list follows\r\n").await?;
    writer.write_all(b"VERSION 2\r\n").await?;
    writer.write_all(b"READER\r\n").await?;
    writer.write_all(b"POST\r\n").await?;
    writer.write_all(b"NEWNEWS\r\n").await?;
    writer.write_all(b".\r\n").await?;
    Ok(())
}

async fn handle_date<W: AsyncWrite + Unpin>(
    writer: &mut W,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    use chrono::Utc;
    let now = Utc::now().format("%Y%m%d%H%M%S").to_string();
    writer
        .write_all(format!("111 {}\r\n", now).as_bytes())
        .await?;
    Ok(())
}

async fn handle_help<W: AsyncWrite + Unpin>(
    writer: &mut W,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.write_all(b"100 help text follows\r\n").await?;
    writer
        .write_all(
            b"CAPABILITIES\r\nMODE READER\r\nGROUP\r\nLIST\r\nLISTGROUP\r\nARTICLE\r\nHEAD\r\nBODY\r\nSTAT\r\nNEXT\r\nLAST\r\nNEWGROUPS\r\nNEWNEWS\r\nPOST\r\nDATE\r\nHELP\r\nQUIT\r\n",
        )
        .await?;
    writer.write_all(b".\r\n").await?;
    Ok(())
}

async fn handle_mode<W: AsyncWrite + Unpin>(
    writer: &mut W,
    args: &[String],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(arg) = args.get(0) {
        if arg.eq_ignore_ascii_case("READER") {
            writer.write_all(b"200 Reader mode acknowledged\r\n").await?;
        } else {
            writer.write_all(b"501 unknown mode\r\n").await?;
        }
    } else {
        writer.write_all(b"501 missing mode\r\n").await?;
    }
    Ok(())
}

async fn handle_post<R, W>(
    reader: &mut R,
    writer: &mut W,
    storage: &DynStorage,
    current_group: &str,
) -> Result<(), Box<dyn Error + Send + Sync>>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    writer
        .write_all(b"340 send article to be posted. End with <CR-LF>.<CR-LF>\r\n")
        .await?;
    let mut msg = String::new();
    let mut line = String::new();
    loop {
        line.clear();
        reader.read_line(&mut line).await?;
        if line.trim_end() == "." {
            break;
        }
        msg.push_str(&line);
    }
    let (_, message) = parse_message(&msg).map_err(|_| "invalid message")?;
    let _ = storage.store_article(current_group, &message).await?;
    writer.write_all(b"240 article received\r\n").await?;
    Ok(())
}

pub async fn handle_client(
    mut socket: TcpStream,
    storage: DynStorage,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let (read_half, mut write_half) = socket.split();
    let mut reader = BufReader::new(read_half);
    write_half.write_all(b"200 NNTP Service Ready\r\n").await?;
    let mut line = String::new();
    let mut current_group = String::from("misc");
    let mut current_article: Option<u64> = None;
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        let (_, cmd) = match parse_command(trimmed) {
            Ok(c) => c,
            Err(_) => {
                write_half.write_all(b"500 syntax error\r\n").await?;
                continue;
            }
        };
        match cmd.name.as_str() {
            "QUIT" => {
                if handle_quit(&mut write_half).await? {
                    break;
                }
            }
            "GROUP" => {
                handle_group(&mut write_half, &cmd.args, &mut current_group).await?;
            }
            "ARTICLE" => {
                handle_article(&mut write_half, &storage, &cmd.args, &current_group).await?;
            }
            "HEAD" => {
                handle_head(&mut write_half, &storage, &cmd.args, &current_group, &mut current_article).await?;
            }
            "BODY" => {
                handle_body(&mut write_half, &storage, &cmd.args, &current_group, &mut current_article).await?;
            }
            "STAT" => {
                handle_stat(&mut write_half, &storage, &cmd.args, &current_group, &mut current_article).await?;
            }
            "LIST" => {
                handle_list(&mut write_half, &storage, &cmd.args).await?;
            }
            "LISTGROUP" => {
                handle_listgroup(&mut write_half, &storage, &cmd.args, &mut current_group).await?;
            }
            "NEXT" => {
                handle_next(&mut write_half, &storage, &current_group, &mut current_article).await?;
            }
            "LAST" => {
                handle_last(&mut write_half, &storage, &current_group, &mut current_article).await?;
            }
            "NEWGROUPS" => {
                handle_newgroups(&mut write_half, &storage, &cmd.args).await?;
            }
            "NEWNEWS" => {
                handle_newnews(&mut write_half, &storage, &cmd.args, &current_group).await?;
            }
            "CAPABILITIES" => {
                handle_capabilities(&mut write_half).await?;
            }
            "DATE" => {
                handle_date(&mut write_half).await?;
            }
            "HELP" => {
                handle_help(&mut write_half).await?;
            }
            "MODE" => {
                handle_mode(&mut write_half, &cmd.args).await?;
            }
            "POST" => {
                handle_post(&mut reader, &mut write_half, &storage, &current_group).await?;
            }
            _ => {
                write_half.write_all(b"500 command not recognized\r\n").await?;
            }
        }
    }
    Ok(())
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
}
