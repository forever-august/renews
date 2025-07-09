//! Utility functions for command handlers.

use crate::storage::DynStorage;
use crate::{ConnectionState, Message};
use std::error::Error;
use std::fmt;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

/// Errors that can occur when querying for articles.
#[derive(Debug, Clone)]
pub enum ArticleQueryError {
    /// No group is currently selected.
    NoGroup,
    /// Invalid message-id format.
    InvalidId,
    /// The specified range is empty.
    RangeEmpty,
    /// Article not found by number.
    NotFoundByNumber,
    /// Article not found by message-id.
    MessageIdNotFound,
    /// No current article is selected.
    NoCurrentArticle,
}

impl fmt::Display for ArticleQueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArticleQueryError::NoGroup => write!(f, "No group selected"),
            ArticleQueryError::InvalidId => write!(f, "Invalid message-id format"),
            ArticleQueryError::RangeEmpty => write!(f, "Range is empty"),
            ArticleQueryError::NotFoundByNumber => write!(f, "Article not found by number"),
            ArticleQueryError::MessageIdNotFound => write!(f, "Article not found by message-id"),
            ArticleQueryError::NoCurrentArticle => write!(f, "No current article selected"),
        }
    }
}

impl Error for ArticleQueryError {}

/// Write a simple response line to the writer.
pub async fn write_simple<W: AsyncWrite + Unpin>(
    writer: &mut W,
    response: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.write_all(response.as_bytes()).await?;
    Ok(())
}

/// Send article headers to the writer.
pub async fn send_headers<W: AsyncWrite + Unpin>(
    writer: &mut W,
    article: &Message,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    for (name, val) in &article.headers {
        writer
            .write_all(format!("{name}: {val}\r\n").as_bytes())
            .await?;
    }
    Ok(())
}

/// Send article body to the writer with proper dot-stuffing.
pub async fn send_body<W: AsyncWrite + Unpin>(
    writer: &mut W,
    body: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    for line in body.lines() {
        if line.starts_with('.') {
            writer.write_all(b".").await?;
        }
        writer.write_all(line.as_bytes()).await?;
        writer.write_all(b"\r\n").await?;
    }
    Ok(())
}

/// Extract Message-ID from an article.
pub fn extract_message_id(article: &Message) -> Option<&str> {
    article
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("Message-ID"))
        .map(|(_, v)| v.as_str())
}

/// Get header value from an article.
pub fn get_header_value(msg: &Message, name: &str) -> Option<String> {
    msg.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.clone())
}

/// Get metadata value for an article.
pub async fn metadata_value(storage: &DynStorage, msg: &Message, name: &str) -> Option<String> {
    match name {
        ":bytes" => {
            if let Some(id) = extract_message_id(msg) {
                storage
                    .get_message_size(id)
                    .await
                    .ok()
                    .flatten()
                    .map(|s| s.to_string())
                    .or_else(|| Some((msg.body.len() as u64).to_string()))
            } else {
                Some((msg.body.len() as u64).to_string())
            }
        }
        ":lines" => Some(msg.body.lines().count().to_string()),
        _ => None,
    }
}

/// Resolve articles based on argument (number, range, or message-id).
pub async fn resolve_articles(
    storage: &DynStorage,
    state: &mut ConnectionState,
    arg: Option<&str>,
) -> Result<Vec<(u64, Message)>, ArticleQueryError> {
    let mut articles = Vec::new();

    if let Some(arg) = arg {
        if arg.starts_with('<') && arg.ends_with('>') {
            // Message-ID
            if let Some(article) = storage
                .get_article_by_id(arg)
                .await
                .map_err(|_| ArticleQueryError::MessageIdNotFound)?
            {
                articles.push((0, article));
            } else {
                return Err(ArticleQueryError::MessageIdNotFound);
            }
        } else if let Some(group) = state.current_group.as_deref() {
            // Article number or range
            let nums = crate::parse_range(storage, group, arg)
                .await
                .map_err(|_| ArticleQueryError::RangeEmpty)?;

            if nums.is_empty() {
                return Err(ArticleQueryError::RangeEmpty);
            }

            for n in nums {
                if let Some(article) = storage
                    .get_article_by_number(group, n)
                    .await
                    .map_err(|_| ArticleQueryError::NotFoundByNumber)?
                {
                    articles.push((n, article));
                    state.current_article = Some(n);
                }
            }

            if articles.is_empty() {
                return Err(ArticleQueryError::NotFoundByNumber);
            }
        } else if arg.parse::<u64>().is_ok() {
            return Err(ArticleQueryError::NoGroup);
        } else {
            return Err(ArticleQueryError::InvalidId);
        }
    } else if let (Some(group), Some(num)) = (state.current_group.as_deref(), state.current_article)
    {
        // Use current article
        if let Some(article) = storage
            .get_article_by_number(group, num)
            .await
            .map_err(|_| ArticleQueryError::NoCurrentArticle)?
        {
            articles.push((num, article));
        } else {
            return Err(ArticleQueryError::NoCurrentArticle);
        }
    } else if state.current_group.is_none() {
        return Err(ArticleQueryError::NoGroup);
    } else {
        return Err(ArticleQueryError::NoCurrentArticle);
    }

    Ok(articles)
}

/// Article operation types.
#[derive(Debug, Clone, Copy)]
pub enum ArticleOperation {
    Full,    // ARTICLE command - send headers + body
    Headers, // HEAD command - send headers only
    Body,    // BODY command - send body only
    Stat,    // STAT command - send status only
}

impl ArticleOperation {
    pub fn response_code(&self) -> u16 {
        match self {
            ArticleOperation::Full => 220,
            ArticleOperation::Headers => 221,
            ArticleOperation::Body => 222,
            ArticleOperation::Stat => 223,
        }
    }

    pub fn response_suffix(&self) -> &'static str {
        match self {
            ArticleOperation::Full => "article follows",
            ArticleOperation::Headers => "article headers follow",
            ArticleOperation::Body => "article body follows",
            ArticleOperation::Stat => "article exists",
        }
    }
}

/// Generic handler for article operations (ARTICLE, HEAD, BODY, STAT).
pub async fn handle_article_operation<W: AsyncWrite + Unpin>(
    writer: &mut W,
    storage: &DynStorage,
    state: &mut ConnectionState,
    args: &[String],
    operation: ArticleOperation,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    use crate::responses::*;

    match resolve_articles(storage, state, args.first().map(String::as_str)).await {
        Ok(articles) => {
            for (num, article) in articles {
                let id = extract_message_id(&article).unwrap_or("");
                let response = format!(
                    "{} {} {} {}\r\n",
                    operation.response_code(),
                    num,
                    id,
                    operation.response_suffix()
                );
                write_simple(writer, &response).await?;

                match operation {
                    ArticleOperation::Full => {
                        send_headers(writer, &article).await?;
                        writer.write_all(RESP_CRLF.as_bytes()).await?;
                        send_body(writer, &article.body).await?;
                        writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
                    }
                    ArticleOperation::Headers => {
                        send_headers(writer, &article).await?;
                        writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
                    }
                    ArticleOperation::Body => {
                        send_body(writer, &article.body).await?;
                        writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
                    }
                    ArticleOperation::Stat => {
                        // STAT just sends the status line, no content
                    }
                }
            }
        }
        Err(error) => handle_article_error(writer, error).await?,
    }
    Ok(())
}

/// Handle errors from article queries consistently.
pub async fn handle_article_error<W: AsyncWrite + Unpin>(
    writer: &mut W,
    error: ArticleQueryError,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    use crate::responses::*;

    match error {
        ArticleQueryError::NoGroup => {
            write_simple(writer, RESP_412_NO_GROUP).await?;
        }
        ArticleQueryError::InvalidId => {
            write_simple(writer, RESP_501_INVALID_ID).await?;
        }
        ArticleQueryError::RangeEmpty => {
            write_simple(writer, RESP_423_RANGE_EMPTY).await?;
        }
        ArticleQueryError::NotFoundByNumber => {
            write_simple(writer, RESP_423_NO_ARTICLE_NUM).await?;
        }
        ArticleQueryError::MessageIdNotFound => {
            write_simple(writer, RESP_430_NO_ARTICLE).await?;
        }
        ArticleQueryError::NoCurrentArticle => {
            write_simple(writer, RESP_420_NO_CURRENT).await?;
        }
    }
    Ok(())
}

/// Write a response with header values.
pub async fn write_response_with_values<W: AsyncWrite + Unpin>(
    writer: &mut W,
    response: &str,
    values: &[(u64, Option<String>)],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    writer.write_all(response.as_bytes()).await?;
    for (n, val) in values {
        if let Some(v) = val {
            writer.write_all(format!("{n} {v}\r\n").as_bytes()).await?;
        } else {
            writer.write_all(format!("{n}\r\n").as_bytes()).await?;
        }
    }
    use crate::responses::RESP_DOT_CRLF;
    writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
    Ok(())
}

/// Write multiple response lines to the writer.
pub async fn write_lines<W: AsyncWrite + Unpin>(
    writer: &mut W,
    lines: &[&str],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for line in lines {
        writer.write_all(line.as_bytes()).await?;
    }
    Ok(())
}

/// Read a message from the reader until dot termination.
pub async fn read_message<R: AsyncBufRead + Unpin>(
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

/// Perform basic validation on an article before queuing
///
/// This checks only what can be validated without database access:
/// - Required headers (From, Subject, Newsgroups)
/// - Size limits
///
/// NOTE: This function is maintained for backward compatibility.
/// New code should use the filter-based validation system.
pub async fn basic_validate_article(
    cfg: &crate::config::Config,
    article: &crate::Message,
    size: u64,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Check required headers
    let has_from = article
        .headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("From"));
    let has_subject = article
        .headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("Subject"));
    let newsgroups: Vec<String> = article
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("Newsgroups"))
        .map(|(_, v)| {
            v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if !has_from || !has_subject || newsgroups.is_empty() {
        return Err("missing required headers".into());
    }

    // Check size limit
    if let Some(max_size) = cfg.default_max_article_bytes {
        if size > max_size {
            return Err("article too large".into());
        }
    }

    Ok(())
}

/// Validate an article for posting (comprehensive validation using filter chain).
/// This performs database-dependent validation and should be used by workers.
pub async fn comprehensive_validate_article(
    storage: &crate::storage::DynStorage,
    auth: &crate::auth::DynAuth,
    cfg: &crate::config::Config,
    article: &crate::Message,
    size: u64,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Use the default filter chain for validation
    let filter_chain = crate::filters::FilterChain::default();
    filter_chain.validate(storage, auth, cfg, article, size).await
}

/// Validate an article using a custom filter chain.
/// This allows for customizable validation beyond the default chain.
pub async fn validate_article_with_filters(
    storage: &crate::storage::DynStorage,
    auth: &crate::auth::DynAuth,
    cfg: &crate::config::Config,
    article: &crate::Message,
    size: u64,
    filter_chain: &crate::filters::FilterChain,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    filter_chain.validate(storage, auth, cfg, article, size).await
}
