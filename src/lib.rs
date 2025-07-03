pub mod parse;
pub use parse::{
    Command, Message, Response, parse_command, parse_datetime, parse_message, parse_range,
    parse_response,
};

pub mod config;
pub mod retention;
pub mod storage;
pub mod wildmat;

#[derive(Default)]
pub struct ConnectionState {
    pub current_group: Option<String>,
    pub current_article: Option<u64>,
    pub is_tls: bool,
}

use crate::storage::DynStorage;
use crate::config::Config;
use std::error::Error;
use tokio::io::{
    self, AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader,
};

const RESP_CRLF: &str = "\r\n";
const RESP_DOT_CRLF: &str = ".\r\n";
const RESP_205_CLOSING: &str = "205 closing connection\r\n";
const RESP_411_NO_GROUP: &str = "411 no such group\r\n";
const RESP_501_MISSING_GROUP: &str = "501 missing group\r\n";
const RESP_412_NO_GROUP: &str = "412 no newsgroup selected\r\n";
const RESP_501_INVALID_ID: &str = "501 invalid id\r\n";
const RESP_423_RANGE_EMPTY: &str = "423 no articles in that range\r\n";
const RESP_423_NO_ARTICLE_NUMBER: &str = "423 no such article number in this group\r\n";
const RESP_430_NO_ARTICLE: &str = "430 no such article\r\n";
const RESP_420_NO_CURRENT: &str = "420 no current article selected\r\n";
const RESP_501_INVALID_ARG: &str = "501 invalid argument\r\n";
const RESP_501_INVALID_DATE: &str = "501 invalid date\r\n";
const RESP_335_SEND_IT: &str = "335 Send it; end with <CR-LF>.<CR-LF>\r\n";
const RESP_340_SEND_ARTICLE: &str = "340 send article to be posted. End with <CR-LF>.<CR-LF>\r\n";
const RESP_441_POSTING_FAILED: &str = "441 posting failed\r\n";
const RESP_200_READY: &str = "200 NNTP Service Ready\r\n";
const RESP_201_READY_NO_POST: &str = "201 NNTP Service Ready - no posting allowed\r\n";
const RESP_200_POSTING_ALLOWED: &str = "200 Posting allowed\r\n";
const RESP_201_POSTING_PROHIBITED: &str = "201 Posting prohibited\r\n";
const RESP_421_NO_NEXT: &str = "421 no next article\r\n";
const RESP_422_NO_PREV: &str = "422 no previous article\r\n";
const RESP_435_NOT_WANTED: &str = "435 article not wanted\r\n";
const RESP_437_REJECTED: &str = "437 article rejected\r\n";
const RESP_235_TRANSFER_OK: &str = "235 Article transferred OK\r\n";
const RESP_501_MSGID_REQUIRED: &str = "501 message-id required\r\n";
const RESP_500_UNKNOWN_CMD: &str = "500 command not recognized\r\n";
const RESP_500_SYNTAX: &str = "500 syntax error\r\n";
const RESP_503_DATA_NOT_STORED: &str = "503 Data item not stored\r\n";
const RESP_224_OVERVIEW: &str = "224 Overview information follows\r\n";
const RESP_225_HEADERS: &str = "225 Headers follow\r\n";
const RESP_221_HEADER_FOLLOWS: &str = "221 Header follows\r\n";
const RESP_230_NEW_ARTICLES: &str = "230 list of new articles follows\r\n";
const RESP_231_NEW_GROUPS: &str = "231 list of new newsgroups follows\r\n";
const RESP_211_NUMBERS_FOLLOW: &str = "211 article numbers follow\r\n";
const RESP_215_LIST_FOLLOWS: &str = "215 list of newsgroups follows\r\n";
const RESP_215_INFO_FOLLOWS: &str = "215 information follows\r\n";
const RESP_215_DESCRIPTIONS_FOLLOW: &str = "215 descriptions follow\r\n";
const RESP_215_OVERVIEW_FMT: &str = "215 Order of fields in overview database.\r\n";
const RESP_215_METADATA: &str = "215 metadata items supported:\r\n";
const RESP_101_CAPABILITIES: &str = "101 Capability list follows\r\n";
const RESP_483_SECURE_REQ: &str = "483 Secure connection required\r\n";
const RESP_240_ARTICLE_RECEIVED: &str = "240 article received\r\n";
const RESP_HELP_TEXT: &str = "CAPABILITIES\r\nMODE READER\r\nGROUP\r\nLIST\r\nLISTGROUP\r\nARTICLE\r\nHEAD\r\nBODY\r\nSTAT\r\nHDR\r\nOVER\r\nNEXT\r\nLAST\r\nNEWGROUPS\r\nNEWNEWS\r\nIHAVE\r\nTAKETHIS\r\nPOST\r\nDATE\r\nHELP\r\nQUIT\r\n";
const RESP_CAP_VERSION: &str = "VERSION 2\r\n";
const RESP_CAP_READER: &str = "READER\r\n";
const RESP_CAP_POST: &str = "POST\r\n";
const RESP_CAP_NEWNEWS: &str = "NEWNEWS\r\n";
const RESP_CAP_IHAVE: &str = "IHAVE\r\n";
const RESP_CAP_STREAMING: &str = "STREAMING\r\n";
const RESP_CAP_OVER_MSGID: &str = "OVER MSGID\r\n";
const RESP_CAP_HDR: &str = "HDR\r\n";
const RESP_CAP_LIST: &str =
    "LIST ACTIVE NEWSGROUPS ACTIVE.TIMES DISTRIB.PATS OVERVIEW.FMT HEADERS\r\n";
const RESP_SUBJECT: &str = "Subject:\r\n";
const RESP_FROM: &str = "From:\r\n";
const RESP_DATE: &str = "Date:\r\n";
const RESP_MESSAGE_ID: &str = "Message-ID:\r\n";
const RESP_REFERENCES: &str = "References:\r\n";
const RESP_BYTES: &str = ":bytes\r\n";
const RESP_LINES: &str = ":lines\r\n";
const RESP_COLON: &str = ":\r\n";
const RESP_501_UNKNOWN_KEYWORD: &str = "501 unknown keyword\r\n";
const RESP_501_UNKNOWN_MODE: &str = "501 unknown mode\r\n";
const RESP_501_MISSING_MODE: &str = "501 missing mode\r\n";
const RESP_501_NOT_ENOUGH: &str = "501 not enough arguments\r\n";
const RESP_100_HELP_FOLLOWS: &str = "100 help text follows\r\n";
const DOT: &str = ".";

fn extract_message_id(msg: &Message) -> Option<&str> {
    msg.headers.iter().find_map(|(k, v)| {
        if k.eq_ignore_ascii_case("Message-ID") {
            Some(v.as_str())
        } else {
            None
        }
    })
}

async fn send_body<W: AsyncWrite + Unpin>(
    writer: &mut W,
    body: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    for line in body.split_inclusive('\n') {
        if line.starts_with('.') {
            writer.write_all(DOT.as_bytes()).await?;
        }
        if let Some(stripped) = line.strip_suffix('\n') {
            let stripped = stripped.strip_suffix('\r').unwrap_or(stripped);
            writer.write_all(stripped.as_bytes()).await?;
            writer.write_all(RESP_CRLF.as_bytes()).await?;
        } else {
            let stripped = line.strip_suffix('\r').unwrap_or(line);
            writer.write_all(stripped.as_bytes()).await?;
        }
    }
    if !body.ends_with('\n') {
        writer.write_all(RESP_CRLF.as_bytes()).await?;
    }
    Ok(())
}

async fn send_headers<W: AsyncWrite + Unpin>(
    writer: &mut W,
    article: &Message,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    for (k, v) in article.headers.iter() {
        writer
            .write_all(format!("{}: {}\r\n", k, v).as_bytes())
            .await?;
    }
    Ok(())
}

async fn write_simple<W: AsyncWrite + Unpin>(
    writer: &mut W,
    msg: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.write_all(msg.as_bytes()).await?;
    Ok(())
}

enum ArticleQueryError {
    NoGroup,
    InvalidId,
    RangeEmpty,
    NotFoundByNumber,
    MessageIdNotFound,
    NoCurrentArticle,
}

async fn resolve_articles(
    storage: &DynStorage,
    state: &mut ConnectionState,
    arg: Option<&str>,
) -> Result<Vec<(u64, Message)>, ArticleQueryError> {
    if let Some(arg) = arg {
        if arg.starts_with('<') && arg.ends_with('>') {
            if let Some(article) = storage
                .get_article_by_id(arg)
                .await
                .map_err(|_| ArticleQueryError::MessageIdNotFound)?
            {
                return Ok(vec![(0, article)]);
            } else {
                return Err(ArticleQueryError::MessageIdNotFound);
            }
        }
        let numeric_valid = if let Some((s, e)) = arg.split_once('-') {
            s.parse::<u64>().is_ok() && (e.is_empty() || e.parse::<u64>().is_ok())
        } else {
            arg.parse::<u64>().is_ok()
        };

        let group = match state.current_group.as_deref() {
            Some(g) => g,
            None => {
                return if numeric_valid {
                    Err(ArticleQueryError::NoGroup)
                } else {
                    Err(ArticleQueryError::InvalidId)
                };
            }
        };
        let nums = parse_range(storage, group, arg)
            .await
            .map_err(|_| ArticleQueryError::InvalidId)?;
        if nums.is_empty() {
            return Err(ArticleQueryError::RangeEmpty);
        }
        let mut articles = Vec::new();
        let mut found = false;
        for n in nums {
            if let Some(article) = storage
                .get_article_by_number(group, n)
                .await
                .map_err(|_| ArticleQueryError::NotFoundByNumber)?
            {
                found = true;
                state.current_article = Some(n);
                articles.push((n, article));
            }
        }
        if found {
            Ok(articles)
        } else {
            Err(ArticleQueryError::NotFoundByNumber)
        }
    } else {
        let group = match state.current_group.as_deref() {
            Some(g) => g,
            None => return Err(ArticleQueryError::NoGroup),
        };
        let num = match state.current_article {
            Some(n) => n,
            None => return Err(ArticleQueryError::NoCurrentArticle),
        };
        match storage.get_article_by_number(group, num).await {
            Ok(Some(article)) => Ok(vec![(num, article)]),
            _ => Err(ArticleQueryError::NoCurrentArticle),
        }
    }
}

/// Handle the QUIT command as defined in RFC 3977 Section 5.4.
async fn handle_quit<W: AsyncWrite + Unpin>(
    writer: &mut W,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    writer.write_all(RESP_205_CLOSING.as_bytes()).await?;
    Ok(true)
}

/// Handle the GROUP command as defined in RFC 3977 Section 6.1.1.
async fn handle_group<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    args: &[String],
    state: &mut ConnectionState,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(name) = args.get(0) {
        let groups = storage.list_groups().await?;
        if !groups.iter().any(|g| g == name) {
            writer.write_all(RESP_411_NO_GROUP.as_bytes()).await?;
            return Ok(());
        }
        let nums = storage.list_article_numbers(name).await?;
        let count = nums.len();
        let high = nums.last().copied().unwrap_or(0);
        let low = nums.first().copied().unwrap_or(0);
        state.current_group = Some(name.clone());
        state.current_article = None;
        writer
            .write_all(format!("211 {} {} {} {}\r\n", count, low, high, name).as_bytes())
            .await?;
    } else {
        writer.write_all(RESP_501_MISSING_GROUP.as_bytes()).await?;
    }
    Ok(())
}

/// Handle the ARTICLE command as defined in RFC 3977 Section 6.2.1.
async fn handle_article<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    args: &[String],
    state: &mut ConnectionState,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match resolve_articles(storage, state, args.get(0).map(String::as_str)).await {
        Ok(arts) => {
            for (num, article) in arts {
                let id = extract_message_id(&article).unwrap_or("");
                write_simple(writer, &format!("220 {} {} article follows\r\n", num, id)).await?;
                send_headers(writer, &article).await?;
                writer.write_all(RESP_CRLF.as_bytes()).await?;
                send_body(writer, &article.body).await?;
                writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
            }
        }
        Err(ArticleQueryError::NoGroup) => {
            write_simple(writer, "412 no newsgroup selected\r\n").await?;
        }
        Err(ArticleQueryError::InvalidId) => {
            write_simple(writer, "501 invalid id\r\n").await?;
        }
        Err(ArticleQueryError::RangeEmpty) => {
            write_simple(writer, "423 no articles in that range\r\n").await?;
        }
        Err(ArticleQueryError::NotFoundByNumber) => {
            write_simple(writer, "423 no such article number in this group\r\n").await?;
        }
        Err(ArticleQueryError::MessageIdNotFound) => {
            write_simple(writer, "430 no such article\r\n").await?;
        }
        Err(ArticleQueryError::NoCurrentArticle) => {
            write_simple(writer, "420 no current article selected\r\n").await?;
        }
    }
    Ok(())
}

/// Handle the HEAD command as defined in RFC 3977 Section 6.2.2.
async fn handle_head<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    args: &[String],
    state: &mut ConnectionState,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match resolve_articles(storage, state, args.get(0).map(String::as_str)).await {
        Ok(arts) => {
            for (num, article) in arts {
                let id = extract_message_id(&article).unwrap_or("");
                write_simple(
                    writer,
                    &format!("221 {} {} article headers follow\r\n", num, id),
                )
                .await?;
                send_headers(writer, &article).await?;
                writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
            }
        }
        Err(ArticleQueryError::NoGroup) => {
            write_simple(writer, "412 no newsgroup selected\r\n").await?;
        }
        Err(ArticleQueryError::InvalidId) => {
            write_simple(writer, "501 invalid id\r\n").await?;
        }
        Err(ArticleQueryError::RangeEmpty) => {
            write_simple(writer, "423 no articles in that range\r\n").await?;
        }
        Err(ArticleQueryError::NotFoundByNumber) => {
            write_simple(writer, "423 no such article number in this group\r\n").await?;
        }
        Err(ArticleQueryError::MessageIdNotFound) => {
            write_simple(writer, "430 no such article\r\n").await?;
        }
        Err(ArticleQueryError::NoCurrentArticle) => {
            write_simple(writer, "420 no current article selected\r\n").await?;
        }
    }
    Ok(())
}

/// Handle the BODY command as defined in RFC 3977 Section 6.2.3.
async fn handle_body<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    args: &[String],
    state: &mut ConnectionState,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match resolve_articles(storage, state, args.get(0).map(String::as_str)).await {
        Ok(arts) => {
            for (num, article) in arts {
                let id = extract_message_id(&article).unwrap_or("");
                write_simple(
                    writer,
                    &format!("222 {} {} article body follows\r\n", num, id),
                )
                .await?;
                send_body(writer, &article.body).await?;
                writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
            }
        }
        Err(ArticleQueryError::NoGroup) => {
            write_simple(writer, "412 no newsgroup selected\r\n").await?;
        }
        Err(ArticleQueryError::InvalidId) => {
            write_simple(writer, "501 invalid id\r\n").await?;
        }
        Err(ArticleQueryError::RangeEmpty) => {
            write_simple(writer, "423 no articles in that range\r\n").await?;
        }
        Err(ArticleQueryError::NotFoundByNumber) => {
            write_simple(writer, "423 no such article number in this group\r\n").await?;
        }
        Err(ArticleQueryError::MessageIdNotFound) => {
            write_simple(writer, "430 no such article\r\n").await?;
        }
        Err(ArticleQueryError::NoCurrentArticle) => {
            write_simple(writer, "420 no current article selected\r\n").await?;
        }
    }
    Ok(())
}

/// Handle the STAT command as defined in RFC 3977 Section 6.2.4.
async fn handle_stat<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    args: &[String],
    state: &mut ConnectionState,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(arg) = args.get(0) {
        if let Ok(num) = arg.parse::<u64>() {
            if let Some(group) = state.current_group.as_deref() {
                if let Some(article) = storage.get_article_by_number(group, num).await? {
                    state.current_article = Some(num);
                    let id = extract_message_id(&article).unwrap_or("");
                    writer
                        .write_all(format!("223 {} {} article exists\r\n", num, id).as_bytes())
                        .await?;
                } else {
                    writer
                        .write_all(RESP_423_NO_ARTICLE_NUMBER.as_bytes())
                        .await?;
                }
            } else {
                writer.write_all(RESP_412_NO_GROUP.as_bytes()).await?;
            }
        } else if arg.starts_with('<') && arg.ends_with('>') {
            if let Some(article) = storage.get_article_by_id(arg).await? {
                let id = extract_message_id(&article).unwrap_or("");
                writer
                    .write_all(format!("223 0 {} article exists\r\n", id).as_bytes())
                    .await?;
            } else {
                writer.write_all(RESP_430_NO_ARTICLE.as_bytes()).await?;
            }
        } else {
            writer.write_all(RESP_501_INVALID_ID.as_bytes()).await?;
        }
    } else if let Some(num) = state.current_article {
        if let Some(group) = state.current_group.as_deref() {
            if let Some(article) = storage.get_article_by_number(group, num).await? {
                let id = extract_message_id(&article).unwrap_or("");
                writer
                    .write_all(format!("223 {} {} article exists\r\n", num, id).as_bytes())
                    .await?;
            } else {
                writer.write_all(RESP_420_NO_CURRENT.as_bytes()).await?;
            }
        } else {
            writer.write_all(RESP_412_NO_GROUP.as_bytes()).await?;
        }
    } else {
        writer.write_all(RESP_420_NO_CURRENT.as_bytes()).await?;
    }
    Ok(())
}

/// Handle the LIST command and its variants as described in
/// RFC 3977 Section 7.6.
async fn handle_list<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    args: &[String],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(keyword) = args.get(0) {
        match keyword.to_ascii_uppercase().as_str() {
            "ACTIVE" => {
                let pattern = args.get(1).map(|s| s.as_str());
                let groups = storage.list_groups().await?;
                writer.write_all(RESP_215_LIST_FOLLOWS.as_bytes()).await?;
                for g in groups {
                    if pattern.map(|p| wildmat::wildmat(p, &g)).unwrap_or(true) {
                        let nums = storage.list_article_numbers(&g).await?;
                        let high = nums.last().copied().unwrap_or(0);
                        let low = nums.first().copied().unwrap_or(0);
                        writer
                            .write_all(format!("{} {} {} y\r\n", g, high, low).as_bytes())
                            .await?;
                    }
                }
                writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
                return Ok(());
            }
            "ACTIVE.TIMES" => {
                let pattern = args.get(1).map(|s| s.as_str());
                let groups = storage.list_groups_with_times().await?;
                writer.write_all(RESP_215_INFO_FOLLOWS.as_bytes()).await?;
                for (g, ts) in groups {
                    if pattern.map(|p| wildmat::wildmat(p, &g)).unwrap_or(true) {
                        writer
                            .write_all(format!("{} {} -\r\n", g, ts).as_bytes())
                            .await?;
                    }
                }
                writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
                return Ok(());
            }
            "DISTRIB.PATS" => {
                writer
                    .write_all(RESP_503_DATA_NOT_STORED.as_bytes())
                    .await?;
                return Ok(());
            }
            "NEWSGROUPS" => {
                let pattern = args.get(1).map(|s| s.as_str());
                let groups = storage.list_groups().await?;
                writer
                    .write_all(RESP_215_DESCRIPTIONS_FOLLOW.as_bytes())
                    .await?;
                for g in groups {
                    if pattern.map(|p| wildmat::wildmat(p, &g)).unwrap_or(true) {
                        writer.write_all(format!("{} \r\n", g).as_bytes()).await?;
                    }
                }
                writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
                return Ok(());
            }
            "OVERVIEW.FMT" => {
                writer.write_all(RESP_215_OVERVIEW_FMT.as_bytes()).await?;
                writer.write_all(RESP_SUBJECT.as_bytes()).await?;
                writer.write_all(RESP_FROM.as_bytes()).await?;
                writer.write_all(RESP_DATE.as_bytes()).await?;
                writer.write_all(RESP_MESSAGE_ID.as_bytes()).await?;
                writer.write_all(RESP_REFERENCES.as_bytes()).await?;
                writer.write_all(RESP_BYTES.as_bytes()).await?;
                writer.write_all(RESP_LINES.as_bytes()).await?;
                writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
                return Ok(());
            }
            "HEADERS" => {
                writer.write_all(RESP_215_METADATA.as_bytes()).await?;
                writer.write_all(RESP_COLON.as_bytes()).await?;
                writer.write_all(RESP_LINES.as_bytes()).await?;
                writer.write_all(RESP_BYTES.as_bytes()).await?;
                writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
                return Ok(());
            }
            _ => {
                writer
                    .write_all(RESP_501_UNKNOWN_KEYWORD.as_bytes())
                    .await?;
                return Ok(());
            }
        }
    }

    // default LIST without keyword behaves like LIST ACTIVE
    let groups = storage.list_groups().await?;
    writer.write_all(RESP_215_LIST_FOLLOWS.as_bytes()).await?;
    for g in groups {
        let nums = storage.list_article_numbers(&g).await?;
        let high = nums.last().copied().unwrap_or(0);
        let low = nums.first().copied().unwrap_or(0);
        writer
            .write_all(format!("{} {} {} y\r\n", g, high, low).as_bytes())
            .await?;
    }
    writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
    Ok(())
}

/// Handle the LISTGROUP command as defined in RFC 3977 Section 6.1.2.
async fn handle_listgroup<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    args: &[String],
    state: &mut ConnectionState,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let group = if let Some(name) = args.get(0) {
        state.current_group = Some(name.clone());
        name.as_str()
    } else {
        match state.current_group.as_deref() {
            Some(g) => g,
            None => {
                writer.write_all(RESP_412_NO_GROUP.as_bytes()).await?;
                return Ok(());
            }
        }
    };
    let nums = storage.list_article_numbers(group).await?;
    writer.write_all(RESP_211_NUMBERS_FOLLOW.as_bytes()).await?;
    for n in nums {
        writer.write_all(format!("{}\r\n", n).as_bytes()).await?;
    }
    writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
    Ok(())
}

/// Handle the NEXT command as defined in RFC 3977 Section 6.1.4.
async fn handle_next<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    state: &mut ConnectionState,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(curr) = state.current_article {
        if let Some(group) = state.current_group.as_deref() {
            let next = curr + 1;
            if let Some(article) = storage.get_article_by_number(group, next).await? {
                state.current_article = Some(next);
                let id = extract_message_id(&article).unwrap_or("");
                writer
                    .write_all(format!("223 {} {} article exists\r\n", next, id).as_bytes())
                    .await?;
            } else {
                writer.write_all(RESP_421_NO_NEXT.as_bytes()).await?;
            }
        } else {
            writer.write_all(RESP_412_NO_GROUP.as_bytes()).await?;
        }
    } else {
        writer.write_all(RESP_420_NO_CURRENT.as_bytes()).await?;
    }
    Ok(())
}

/// Handle the LAST command as defined in RFC 3977 Section 6.1.3.
async fn handle_last<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    state: &mut ConnectionState,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(curr) = state.current_article {
        if let Some(group) = state.current_group.as_deref() {
            if curr > 1 {
                let prev = curr - 1;
                if let Some(article) = storage.get_article_by_number(group, prev).await? {
                    state.current_article = Some(prev);
                    let id = extract_message_id(&article).unwrap_or("");
                    writer
                        .write_all(format!("223 {} {} article exists\r\n", prev, id).as_bytes())
                        .await?;
                } else {
                    writer.write_all(RESP_422_NO_PREV.as_bytes()).await?;
                }
            } else {
                writer.write_all(RESP_422_NO_PREV.as_bytes()).await?;
            }
        } else {
            writer.write_all(RESP_412_NO_GROUP.as_bytes()).await?;
        }
    } else {
        writer.write_all(RESP_420_NO_CURRENT.as_bytes()).await?;
    }
    Ok(())
}

fn get_header_value(msg: &Message, name: &str) -> Option<String> {
    for (k, v) in &msg.headers {
        if k.eq_ignore_ascii_case(name) {
            let mut val = v.replace('\t', " ");
            val.retain(|c| c != '\r' && c != '\n');
            return Some(val);
        }
    }
    None
}

async fn metadata_value(storage: &DynStorage, msg: &Message, name: &str) -> Option<String> {
    match name.to_ascii_lowercase().as_str() {
        ":lines" => Some(msg.body.lines().count().to_string()),
        ":bytes" => {
            if let Some(id) = extract_message_id(msg) {
                match storage.get_message_size(id).await.ok()? {
                    Some(sz) => Some(sz.to_string()),
                    None => Some(msg.body.as_bytes().len().to_string()),
                }
            } else {
                Some(msg.body.as_bytes().len().to_string())
            }
        }
        _ => None,
    }
}

/// Handle the HDR command as defined in RFC 3977 Section 8.5.
async fn handle_hdr<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    args: &[String],
    state: &ConnectionState,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if args.is_empty() {
        writer.write_all(RESP_501_NOT_ENOUGH.as_bytes()).await?;
        return Ok(());
    }
    let field = &args[0];
    if field == ":" {
        let mut articles = Vec::new();
        if let Some(arg) = args.get(1) {
            if arg.starts_with('<') && arg.ends_with('>') {
                if let Some(article) = storage.get_article_by_id(arg).await? {
                    articles.push((0, article));
                } else {
                    writer.write_all(RESP_430_NO_ARTICLE.as_bytes()).await?;
                    return Ok(());
                }
            } else if let Some(group) = state.current_group.as_deref() {
                let nums = parse_range(storage, group, arg).await?;
                if nums.is_empty() {
                    writer.write_all(RESP_423_RANGE_EMPTY.as_bytes()).await?;
                    return Ok(());
                }
                for n in nums {
                    if let Some(article) = storage.get_article_by_number(group, n).await? {
                        articles.push((n, article));
                    }
                }
            } else {
                writer.write_all(RESP_412_NO_GROUP.as_bytes()).await?;
                return Ok(());
            }
        } else if let (Some(group), Some(num)) = (state.current_group.as_deref(), state.current_article) {
            if let Some(article) = storage.get_article_by_number(group, num).await? {
                articles.push((num, article));
            } else {
                writer.write_all(RESP_420_NO_CURRENT.as_bytes()).await?;
                return Ok(());
            }
        } else if state.current_group.is_none() {
            writer.write_all(RESP_412_NO_GROUP.as_bytes()).await?;
            return Ok(());
        } else {
            writer.write_all(RESP_420_NO_CURRENT.as_bytes()).await?;
            return Ok(());
        }

        writer.write_all(RESP_225_HEADERS.as_bytes()).await?;
        for (n, article) in articles {
            for (name, val) in article.headers.iter() {
                let mut v = val.replace('\t', " ");
                v.retain(|c| c != '\r' && c != '\n');
                writer
                    .write_all(format!("{} {}: {}\r\n", n, name, v).as_bytes())
                    .await?;
            }
        }
        writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
        return Ok(());
    }
    let mut values = Vec::new();
    if let Some(arg) = args.get(1) {
        if arg.starts_with('<') && arg.ends_with('>') {
            if let Some(article) = storage.get_article_by_id(arg).await? {
                let val = if field.starts_with(':') {
                    metadata_value(storage, &article, field).await
                } else {
                    get_header_value(&article, field)
                };
                values.push((0, val));
            } else {
                writer.write_all(RESP_430_NO_ARTICLE.as_bytes()).await?;
                return Ok(());
            }
        } else if let Some(group) = state.current_group.as_deref() {
            let nums = parse_range(storage, group, arg).await?;
            if nums.is_empty() {
                writer.write_all(RESP_423_RANGE_EMPTY.as_bytes()).await?;
                return Ok(());
            }
            for n in nums {
                if let Some(article) = storage.get_article_by_number(group, n).await? {
                    let val = if field.starts_with(':') {
                        metadata_value(storage, &article, field).await
                    } else {
                        get_header_value(&article, field)
                    };
                    values.push((n, val));
                }
            }
        } else {
            writer.write_all(RESP_412_NO_GROUP.as_bytes()).await?;
            return Ok(());
        }
    } else if let (Some(group), Some(num)) = (state.current_group.as_deref(), state.current_article)
    {
        if let Some(article) = storage.get_article_by_number(group, num).await? {
            let val = if field.starts_with(':') {
                metadata_value(storage, &article, field).await
            } else {
                get_header_value(&article, field)
            };
            values.push((num, val));
        } else {
            writer.write_all(RESP_420_NO_CURRENT.as_bytes()).await?;
            return Ok(());
        }
    } else if state.current_group.is_none() {
        writer.write_all(RESP_412_NO_GROUP.as_bytes()).await?;
        return Ok(());
    } else {
        writer.write_all(RESP_420_NO_CURRENT.as_bytes()).await?;
        return Ok(());
    }

    writer.write_all(RESP_225_HEADERS.as_bytes()).await?;
    for (n, val) in values {
        if let Some(v) = val {
            writer
                .write_all(format!("{} {}\r\n", n, v).as_bytes())
                .await?;
        } else {
            writer.write_all(format!("{}\r\n", n).as_bytes()).await?;
        }
    }
    writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
    Ok(())
}

/// Handle the XPAT command as defined in RFC 2980 Section 2.9.
async fn handle_xpat<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    args: &[String],
    state: &ConnectionState,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if args.len() < 3 {
        writer.write_all(RESP_501_NOT_ENOUGH.as_bytes()).await?;
        return Ok(());
    }
    let header = &args[0];
    let spec = &args[1];
    let pattern = args[2..].join(" ");
    let mut values = Vec::new();
    if spec.starts_with('<') && spec.ends_with('>') {
        if let Some(article) = storage.get_article_by_id(spec).await? {
            if let Some(val) = get_header_value(&article, header) {
                if wildmat::wildmat(&pattern, &val) {
                    values.push((0, val));
                }
            }
        } else {
            writer.write_all(RESP_430_NO_ARTICLE.as_bytes()).await?;
            return Ok(());
        }
    } else if let Some(group) = state.current_group.as_deref() {
        let nums = parse_range(storage, group, spec).await?;
        for n in nums {
            if let Some(article) = storage.get_article_by_number(group, n).await? {
                if let Some(val) = get_header_value(&article, header) {
                    if wildmat::wildmat(&pattern, &val) {
                        values.push((n, val));
                    }
                }
            }
        }
    } else {
        writer.write_all(RESP_412_NO_GROUP.as_bytes()).await?;
        return Ok(());
    }

    writer.write_all(RESP_221_HEADER_FOLLOWS.as_bytes()).await?;
    for (n, val) in values {
        writer
            .write_all(format!("{} {}\r\n", n, val).as_bytes())
            .await?;
    }
    writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
    Ok(())
}

/// Handle the OVER command as defined in RFC 3977 Section 8.3.
async fn handle_over<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    args: &[String],
    state: &ConnectionState,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut articles: Vec<(u64, Message)> = Vec::new();
    if let Some(arg) = args.get(0) {
        if arg.starts_with('<') && arg.ends_with('>') {
            if let Some(article) = storage.get_article_by_id(arg).await? {
                articles.push((0, article));
            } else {
                writer.write_all(RESP_430_NO_ARTICLE.as_bytes()).await?;
                return Ok(());
            }
        } else if let Some(group) = state.current_group.as_deref() {
            let nums = parse_range(storage, group, arg).await?;
            if nums.is_empty() {
                writer.write_all(RESP_423_RANGE_EMPTY.as_bytes()).await?;
                return Ok(());
            }
            for n in nums {
                if let Some(article) = storage.get_article_by_number(group, n).await? {
                    articles.push((n, article));
                }
            }
        } else {
            writer.write_all(RESP_412_NO_GROUP.as_bytes()).await?;
            return Ok(());
        }
    } else if let (Some(group), Some(num)) = (state.current_group.as_deref(), state.current_article)
    {
        if let Some(article) = storage.get_article_by_number(group, num).await? {
            articles.push((num, article));
        } else {
            writer.write_all(RESP_420_NO_CURRENT.as_bytes()).await?;
            return Ok(());
        }
    } else if state.current_group.is_none() {
        writer.write_all(RESP_412_NO_GROUP.as_bytes()).await?;
        return Ok(());
    } else {
        writer.write_all(RESP_420_NO_CURRENT.as_bytes()).await?;
        return Ok(());
    }

    writer.write_all(RESP_224_OVERVIEW.as_bytes()).await?;
    for (num, article) in articles {
        let subject = get_header_value(&article, "Subject").unwrap_or_default();
        let from = get_header_value(&article, "From").unwrap_or_default();
        let date = get_header_value(&article, "Date").unwrap_or_default();
        let msgid = get_header_value(&article, "Message-ID").unwrap_or_default();
        let refs = get_header_value(&article, "References").unwrap_or_default();
        let bytes = if let Some(id) = extract_message_id(&article) {
            storage
                .get_message_size(id)
                .await?
                .unwrap_or(article.body.as_bytes().len() as u64)
        } else {
            article.body.as_bytes().len() as u64
        };
        let lines = article.body.lines().count();
        writer
            .write_all(
                format!(
                    "{}|{}|{}|{}|{}|{}|{}|{}\r\n",
                    num, subject, from, date, msgid, refs, bytes, lines
                )
                .as_bytes(),
            )
            .await?;
    }
    writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
    Ok(())
}

/// Handle the NEWGROUPS command as defined in RFC 3977 Section 7.3.
async fn handle_newgroups<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    args: &[String],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if args.len() < 2 {
        writer.write_all(RESP_501_NOT_ENOUGH.as_bytes()).await?;
        return Ok(());
    }

    let date = &args[0];
    let time = &args[1];
    let gmt = match args.get(2) {
        Some(arg) => {
            if !arg.eq_ignore_ascii_case("GMT") {
                writer.write_all(RESP_501_INVALID_ARG.as_bytes()).await?;
                return Ok(());
            }
            true
        }
        None => false,
    };
    let since = match parse_datetime(date, time, gmt) {
        Ok(s) => s,
        Err(_) => {
            writer.write_all(RESP_501_INVALID_DATE.as_bytes()).await?;
            return Ok(());
        }
    };

    let groups = storage.list_groups_since(since).await?;
    writer.write_all(RESP_231_NEW_GROUPS.as_bytes()).await?;
    for g in groups {
        writer.write_all(format!("{}\r\n", g).as_bytes()).await?;
    }
    writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
    Ok(())
}

/// Handle the NEWNEWS command as defined in RFC 3977 Section 7.4.
async fn handle_newnews<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    args: &[String],
    _current_group: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if args.len() < 3 {
        writer.write_all(RESP_501_NOT_ENOUGH.as_bytes()).await?;
        return Ok(());
    }

    let wildmat = &args[0];
    let date = &args[1];
    let time = &args[2];
    let gmt = match args.get(3) {
        Some(arg) => {
            if !arg.eq_ignore_ascii_case("GMT") {
                writer.write_all(RESP_501_INVALID_ARG.as_bytes()).await?;
                return Ok(());
            }
            true
        }
        None => false,
    };
    let since = match parse_datetime(date, time, gmt) {
        Ok(s) => s,
        Err(_) => {
            writer.write_all(RESP_501_INVALID_DATE.as_bytes()).await?;
            return Ok(());
        }
    };

    let groups = storage.list_groups().await?;
    let mut ids = Vec::new();
    for g in groups {
        if wildmat::wildmat(wildmat, &g) {
            ids.extend(storage.list_article_ids_since(&g, since).await?);
        }
    }

    writer.write_all(RESP_230_NEW_ARTICLES.as_bytes()).await?;
    for id in ids {
        writer.write_all(format!("{}\r\n", id).as_bytes()).await?;
    }
    writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
    Ok(())
}

/// Handle the CAPABILITIES command as defined in RFC 3977 Section 5.2.
async fn handle_capabilities<W: AsyncWrite + Unpin>(
    writer: &mut W,
    state: &ConnectionState,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.write_all(RESP_101_CAPABILITIES.as_bytes()).await?;
    writer.write_all(RESP_CAP_VERSION.as_bytes()).await?;
    writer.write_all(RESP_CAP_READER.as_bytes()).await?;
    if state.is_tls {
        writer.write_all(RESP_CAP_POST.as_bytes()).await?;
    }
    writer.write_all(RESP_CAP_NEWNEWS.as_bytes()).await?;
    writer.write_all(RESP_CAP_IHAVE.as_bytes()).await?;
    writer.write_all(RESP_CAP_STREAMING.as_bytes()).await?;
    writer.write_all(RESP_CAP_OVER_MSGID.as_bytes()).await?;
    writer.write_all(RESP_CAP_HDR.as_bytes()).await?;
    writer.write_all(RESP_CAP_LIST.as_bytes()).await?;
    writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
    Ok(())
}

/// Handle the DATE command as defined in RFC 3977 Section 7.1.
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

/// Handle the HELP command as defined in RFC 3977 Section 7.2.
async fn handle_help<W: AsyncWrite + Unpin>(
    writer: &mut W,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.write_all(RESP_100_HELP_FOLLOWS.as_bytes()).await?;
    writer.write_all(RESP_HELP_TEXT.as_bytes()).await?;
    writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
    Ok(())
}

/// Handle the MODE command as defined in RFC 3977 Section 5.3.
async fn handle_mode<W: AsyncWrite + Unpin>(
    writer: &mut W,
    args: &[String],
    state: &ConnectionState,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(arg) = args.get(0) {
        if arg.eq_ignore_ascii_case("READER") {
            if state.is_tls {
                writer
                    .write_all(RESP_200_POSTING_ALLOWED.as_bytes())
                    .await?;
            } else {
                writer
                    .write_all(RESP_201_POSTING_PROHIBITED.as_bytes())
                    .await?;
            }
        } else {
            writer.write_all(RESP_501_UNKNOWN_MODE.as_bytes()).await?;
        }
    } else {
        writer.write_all(RESP_501_MISSING_MODE.as_bytes()).await?;
    }
    Ok(())
}

/// Handle the POST command as defined in RFC 3977 Section 6.3.1.
async fn handle_post<R, W>(
    reader: &mut R,
    writer: &mut W,
    storage: &DynStorage,
    cfg: &Config,
) -> Result<(), Box<dyn Error + Send + Sync>>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    writer.write_all(RESP_340_SEND_ARTICLE.as_bytes()).await?;
    let mut msg = String::new();
    let mut line = String::new();
    loop {
        line.clear();
        reader.read_line(&mut line).await?;
        if line == ".\r\n" || line == ".\n" {
            break;
        }
        if line.starts_with("..") {
            msg.push_str(&line[1..]);
        } else {
            msg.push_str(&line);
        }
    }
    let (_, message) = match parse_message(&msg) {
        Ok(m) => m,
        Err(_) => {
            writer.write_all(RESP_441_POSTING_FAILED.as_bytes()).await?;
            return Ok(());
        }
    };
    let newsgroups = match message
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("Newsgroups"))
    {
        Some((_, v)) => v
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>(),
        None => Vec::new(),
    };
    if newsgroups.is_empty() {
        writer.write_all(RESP_441_POSTING_FAILED.as_bytes()).await?;
        return Ok(());
    }
    let existing = storage.list_groups().await?;
    if !newsgroups.iter().all(|g| existing.iter().any(|e| e == g)) {
        writer.write_all(RESP_441_POSTING_FAILED.as_bytes()).await?;
        return Ok(());
    }
    let size = msg.as_bytes().len() as u64;
    for g in &newsgroups {
        if let Some(max) = cfg.max_size_for_group(g) {
            if size > max {
                writer.write_all(RESP_441_POSTING_FAILED.as_bytes()).await?;
                return Ok(());
            }
        }
    }
    for g in newsgroups {
        let _ = storage.store_article(g, &message).await?;
    }
    writer
        .write_all(RESP_240_ARTICLE_RECEIVED.as_bytes())
        .await?;
    Ok(())
}

async fn read_message<R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let mut msg = String::new();
    let mut line = String::new();
    loop {
        line.clear();
        reader.read_line(&mut line).await?;
        if line == ".\r\n" || line == ".\n" {
            break;
        }
        if line.starts_with("..") {
            msg.push_str(&line[1..]);
        } else {
            msg.push_str(&line);
        }
    }
    Ok(msg)
}

/// Handle the IHAVE command as defined in RFC 3977 Section 6.3.2.
async fn handle_ihave<R, W>(
    reader: &mut R,
    writer: &mut W,
    storage: &DynStorage,
    cfg: &Config,
    args: &[String],
) -> Result<(), Box<dyn Error + Send + Sync>>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    if let Some(id) = args.get(0) {
        if storage.get_article_by_id(id).await?.is_some() {
            writer.write_all(RESP_435_NOT_WANTED.as_bytes()).await?;
            return Ok(());
        }
        writer.write_all(RESP_335_SEND_IT.as_bytes()).await?;
        let msg = read_message(reader).await?;
        let (_, article) = match parse_message(&msg) {
            Ok(m) => m,
            Err(_) => {
                writer.write_all(RESP_437_REJECTED.as_bytes()).await?;
                return Ok(());
            }
        };
        let newsgroups = article
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Newsgroups"))
            .map(|(_, v)| {
                v.split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if newsgroups.is_empty() {
            writer.write_all(RESP_437_REJECTED.as_bytes()).await?;
            return Ok(());
        }
        let existing = storage.list_groups().await?;
        if !newsgroups.iter().all(|g| existing.iter().any(|e| e == g)) {
            writer.write_all(RESP_437_REJECTED.as_bytes()).await?;
            return Ok(());
        }
        let size = msg.as_bytes().len() as u64;
        for g in &newsgroups {
            if let Some(max) = cfg.max_size_for_group(g) {
                if size > max {
                    writer.write_all(RESP_437_REJECTED.as_bytes()).await?;
                    return Ok(());
                }
            }
        }
        for g in newsgroups {
            let _ = storage.store_article(g, &article).await?;
        }
        writer.write_all(RESP_235_TRANSFER_OK.as_bytes()).await?;
    } else {
        writer.write_all(RESP_501_MSGID_REQUIRED.as_bytes()).await?;
    }
    Ok(())
}

/// Handle the TAKETHIS command (streaming extension, not in RFC 3977).
async fn handle_takethis<R, W>(
    reader: &mut R,
    writer: &mut W,
    storage: &DynStorage,
    cfg: &Config,
    args: &[String],
) -> Result<(), Box<dyn Error + Send + Sync>>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    if let Some(id) = args.get(0) {
        if storage.get_article_by_id(id).await?.is_some() {
            writer
                .write_all(format!("439 {}\r\n", id).as_bytes())
                .await?;
            return Ok(());
        }
        let msg = read_message(reader).await?;
        let (_, article) = match parse_message(&msg) {
            Ok(m) => m,
            Err(_) => {
                writer
                    .write_all(format!("439 {}\r\n", id).as_bytes())
                    .await?;
                return Ok(());
            }
        };
        let newsgroups = article
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Newsgroups"))
            .map(|(_, v)| {
                v.split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if newsgroups.is_empty() {
            writer
                .write_all(format!("439 {}\r\n", id).as_bytes())
                .await?;
            return Ok(());
        }
        let existing = storage.list_groups().await?;
        if !newsgroups.iter().all(|g| existing.iter().any(|e| e == g)) {
            writer
                .write_all(format!("439 {}\r\n", id).as_bytes())
                .await?;
            return Ok(());
        }
        let size = msg.as_bytes().len() as u64;
        for g in &newsgroups {
            if let Some(max) = cfg.max_size_for_group(g) {
                if size > max {
                    writer
                        .write_all(format!("439 {}\r\n", id).as_bytes())
                        .await?;
                    return Ok(());
                }
            }
        }
        for g in newsgroups {
            let _ = storage.store_article(g, &article).await?;
        }
        writer
            .write_all(format!("239 {}\r\n", id).as_bytes())
            .await?;
    } else {
        writer.write_all(RESP_501_MSGID_REQUIRED.as_bytes()).await?;
    }
    Ok(())
}

pub async fn handle_client<S>(
    socket: S,
    storage: DynStorage,
    cfg: Config,
    is_tls: bool,
) -> Result<(), Box<dyn Error + Send + Sync>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (read_half, mut write_half) = io::split(socket);
    let mut reader = BufReader::new(read_half);
    if is_tls {
        write_half.write_all(RESP_200_READY.as_bytes()).await?;
    } else {
        write_half
            .write_all(RESP_201_READY_NO_POST.as_bytes())
            .await?;
    }
    let mut line = String::new();
    let mut state = ConnectionState::default();
    state.is_tls = is_tls;
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
                write_half.write_all(RESP_500_SYNTAX.as_bytes()).await?;
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
                handle_group(&mut write_half, &storage, &cmd.args, &mut state).await?;
            }
            "ARTICLE" => {
                handle_article(&mut write_half, &storage, &cmd.args, &mut state).await?;
            }
            "HEAD" => {
                handle_head(&mut write_half, &storage, &cmd.args, &mut state).await?;
            }
            "BODY" => {
                handle_body(&mut write_half, &storage, &cmd.args, &mut state).await?;
            }
            "HDR" => {
                handle_hdr(&mut write_half, &storage, &cmd.args, &state).await?;
            }
            "XPAT" => {
                handle_xpat(&mut write_half, &storage, &cmd.args, &state).await?;
            }
            "OVER" => {
                handle_over(&mut write_half, &storage, &cmd.args, &state).await?;
            }
            "STAT" => {
                handle_stat(&mut write_half, &storage, &cmd.args, &mut state).await?;
            }
            "LIST" => {
                handle_list(&mut write_half, &storage, &cmd.args).await?;
            }
            "LISTGROUP" => {
                handle_listgroup(&mut write_half, &storage, &cmd.args, &mut state).await?;
            }
            "NEXT" => {
                handle_next(&mut write_half, &storage, &mut state).await?;
            }
            "LAST" => {
                handle_last(&mut write_half, &storage, &mut state).await?;
            }
            "NEWGROUPS" => {
                handle_newgroups(&mut write_half, &storage, &cmd.args).await?;
            }
            "NEWNEWS" => {
                handle_newnews(
                    &mut write_half,
                    &storage,
                    &cmd.args,
                    state.current_group.as_deref().unwrap_or(""),
                )
                .await?;
            }
            "IHAVE" => {
                handle_ihave(&mut reader, &mut write_half, &storage, &cfg, &cmd.args).await?;
            }
            "TAKETHIS" => {
                handle_takethis(&mut reader, &mut write_half, &storage, &cfg, &cmd.args).await?;
            }
            "CAPABILITIES" => {
                handle_capabilities(&mut write_half, &state).await?;
            }
            "DATE" => {
                handle_date(&mut write_half).await?;
            }
            "HELP" => {
                handle_help(&mut write_half).await?;
            }
            "MODE" => {
                handle_mode(&mut write_half, &cmd.args, &state).await?;
            }
            "POST" => {
                if state.is_tls {
                    handle_post(&mut reader, &mut write_half, &storage, &cfg).await?;
                } else {
                    write_half.write_all(RESP_483_SECURE_REQ.as_bytes()).await?;
                }
            }
            _ => {
                write_half
                    .write_all(RESP_500_UNKNOWN_CMD.as_bytes())
                    .await?;
            }
        }
    }
    Ok(())
}
