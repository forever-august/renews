//! Article retrieval command handlers.

use super::utils::{
    extract_message_id, get_header_value, handle_article_error, metadata_value, resolve_articles,
    send_body, send_headers, write_simple,
};
use super::{CommandHandler, HandlerContext, HandlerResult};
use crate::parse_range;
use crate::responses::*;
use std::error::Error;
use tokio::io::{AsyncBufRead, AsyncWrite, AsyncWriteExt};

/// Handler for the ARTICLE command.
pub struct ArticleHandler;

impl CommandHandler for ArticleHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        match resolve_articles(
            &ctx.storage,
            &mut ctx.state,
            args.first().map(String::as_str),
        )
        .await
        {
            Ok(articles) => {
                for (num, article) in articles {
                    let id = extract_message_id(&article).unwrap_or("");
                    write_simple(
                        &mut ctx.writer,
                        &format!("220 {num} {id} article follows\r\n"),
                    )
                    .await?;
                    send_headers(&mut ctx.writer, &article).await?;
                    ctx.writer.write_all(RESP_CRLF.as_bytes()).await?;
                    send_body(&mut ctx.writer, &article.body).await?;
                    ctx.writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
                }
            }
            Err(error) => handle_article_error(&mut ctx.writer, error).await?,
        }
        Ok(())
    }
}

/// Handler for the HEAD command.
pub struct HeadHandler;

impl CommandHandler for HeadHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        match resolve_articles(
            &ctx.storage,
            &mut ctx.state,
            args.first().map(String::as_str),
        )
        .await
        {
            Ok(articles) => {
                for (num, article) in articles {
                    let id = extract_message_id(&article).unwrap_or("");
                    write_simple(
                        &mut ctx.writer,
                        &format!("221 {num} {id} article headers follow\r\n"),
                    )
                    .await?;
                    send_headers(&mut ctx.writer, &article).await?;
                    ctx.writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
                }
            }
            Err(error) => handle_article_error(&mut ctx.writer, error).await?,
        }
        Ok(())
    }
}

/// Handler for the BODY command.
pub struct BodyHandler;

impl CommandHandler for BodyHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        match resolve_articles(
            &ctx.storage,
            &mut ctx.state,
            args.first().map(String::as_str),
        )
        .await
        {
            Ok(articles) => {
                for (num, article) in articles {
                    let id = extract_message_id(&article).unwrap_or("");
                    write_simple(
                        &mut ctx.writer,
                        &format!("222 {num} {id} article body follows\r\n"),
                    )
                    .await?;
                    send_body(&mut ctx.writer, &article.body).await?;
                    ctx.writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
                }
            }
            Err(error) => handle_article_error(&mut ctx.writer, error).await?,
        }
        Ok(())
    }
}

/// Handler for the STAT command.
pub struct StatHandler;

impl CommandHandler for StatHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        match resolve_articles(
            &ctx.storage,
            &mut ctx.state,
            args.first().map(String::as_str),
        )
        .await
        {
            Ok(articles) => {
                for (num, article) in articles {
                    let id = extract_message_id(&article).unwrap_or("");
                    write_simple(
                        &mut ctx.writer,
                        &format!("223 {num} {id} article exists\r\n"),
                    )
                    .await?;
                }
            }
            Err(error) => handle_article_error(&mut ctx.writer, error).await?,
        }
        Ok(())
    }
}

/// Handler for the HDR command.
pub struct HdrHandler;

impl CommandHandler for HdrHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        if args.is_empty() {
            write_simple(&mut ctx.writer, RESP_501_NOT_ENOUGH).await?;
            return Ok(());
        }

        let field = &args[0];

        // Handle special case for all headers
        if field == ":" {
            return handle_all_headers(ctx, args).await;
        }

        let values = collect_header_values(
            &ctx.storage,
            &ctx.state,
            field,
            args.get(1).map(|s| s.as_str()),
        )
        .await?;

        ctx.writer.write_all(RESP_225_HEADERS.as_bytes()).await?;
        for (n, val) in values {
            if let Some(v) = val {
                ctx.writer
                    .write_all(format!("{n} {v}\r\n").as_bytes())
                    .await?;
            } else {
                ctx.writer.write_all(format!("{n}\r\n").as_bytes()).await?;
            }
        }
        ctx.writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
        Ok(())
    }
}

/// Handler for the XPAT command.
pub struct XPatHandler;

impl CommandHandler for XPatHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        if args.len() < 3 {
            write_simple(&mut ctx.writer, RESP_501_NOT_ENOUGH).await?;
            return Ok(());
        }

        let field = &args[0];
        let range_or_msgid = &args[1];
        let patterns: Vec<&str> = args[2..].iter().map(String::as_str).collect();

        let values =
            collect_header_values(&ctx.storage, &ctx.state, field, Some(range_or_msgid)).await?;

        write_simple(&mut ctx.writer, "221 Header follows\r\n").await?;
        for (n, val) in values {
            if let Some(v) = val {
                if patterns.iter().any(|pat| crate::wildmat::wildmat(pat, &v)) {
                    ctx.writer
                        .write_all(format!("{n} {v}\r\n").as_bytes())
                        .await?;
                }
            }
        }
        ctx.writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
        Ok(())
    }
}

/// Handler for the OVER command.
pub struct OverHandler;

impl CommandHandler for OverHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let articles = if let Some(arg) = args.first() {
            if arg.starts_with('<') && arg.ends_with('>') {
                if let Some(article) = ctx.storage.get_article_by_id(arg).await? {
                    vec![(0, article)]
                } else {
                    write_simple(&mut ctx.writer, RESP_430_NO_ARTICLE).await?;
                    return Ok(());
                }
            } else if let Some(group) = ctx.state.current_group.as_deref() {
                let nums = parse_range(&ctx.storage, group, arg).await?;
                if nums.is_empty() {
                    write_simple(&mut ctx.writer, RESP_423_RANGE_EMPTY).await?;
                    return Ok(());
                }

                let mut articles = Vec::new();
                for n in nums {
                    if let Some(article) = ctx.storage.get_article_by_number(group, n).await? {
                        articles.push((n, article));
                    }
                }
                articles
            } else {
                write_simple(&mut ctx.writer, RESP_412_NO_GROUP).await?;
                return Ok(());
            }
        } else if let (Some(group), Some(num)) = (
            ctx.state.current_group.as_deref(),
            ctx.state.current_article,
        ) {
            if let Some(article) = ctx.storage.get_article_by_number(group, num).await? {
                vec![(num, article)]
            } else {
                write_simple(&mut ctx.writer, RESP_420_NO_CURRENT).await?;
                return Ok(());
            }
        } else if ctx.state.current_group.is_none() {
            write_simple(&mut ctx.writer, RESP_412_NO_GROUP).await?;
            return Ok(());
        } else {
            write_simple(&mut ctx.writer, RESP_420_NO_CURRENT).await?;
            return Ok(());
        };

        ctx.writer.write_all(RESP_224_OVERVIEW.as_bytes()).await?;
        for (num, article) in articles {
            let subject = get_header_value(&article, "Subject").unwrap_or_default();
            let from = get_header_value(&article, "From").unwrap_or_default();
            let date = get_header_value(&article, "Date").unwrap_or_default();
            let msgid = get_header_value(&article, "Message-ID").unwrap_or_default();
            let refs = get_header_value(&article, "References").unwrap_or_default();
            let bytes = if let Some(id) = extract_message_id(&article) {
                ctx.storage
                    .get_message_size(id)
                    .await?
                    .unwrap_or(article.body.len() as u64)
            } else {
                article.body.len() as u64
            };
            let lines = article.body.lines().count();
            ctx.writer
                .write_all(
                    format!(
                        "{num}\t{subject}\t{from}\t{date}\t{msgid}\t{refs}\t{bytes}\t{lines}\r\n"
                    )
                    .as_bytes(),
                )
                .await?;
        }
        ctx.writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
        Ok(())
    }
}

/// Handle the special case of HDR with ":" for all headers.
async fn handle_all_headers<R, W>(ctx: &mut HandlerContext<R, W>, args: &[String]) -> HandlerResult
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut articles = Vec::new();
    if let Some(arg) = args.get(1) {
        if arg.starts_with('<') && arg.ends_with('>') {
            if let Some(article) = ctx.storage.get_article_by_id(arg).await? {
                articles.push((0, article));
            } else {
                write_simple(&mut ctx.writer, RESP_430_NO_ARTICLE).await?;
                return Ok(());
            }
        } else if let Some(group) = ctx.state.current_group.as_deref() {
            let nums = parse_range(&ctx.storage, group, arg).await?;
            if nums.is_empty() {
                write_simple(&mut ctx.writer, RESP_423_RANGE_EMPTY).await?;
                return Ok(());
            }
            for n in nums {
                if let Some(article) = ctx.storage.get_article_by_number(group, n).await? {
                    articles.push((n, article));
                }
            }
        } else {
            write_simple(&mut ctx.writer, RESP_412_NO_GROUP).await?;
            return Ok(());
        }
    } else if let (Some(group), Some(num)) = (
        ctx.state.current_group.as_deref(),
        ctx.state.current_article,
    ) {
        if let Some(article) = ctx.storage.get_article_by_number(group, num).await? {
            articles.push((num, article));
        } else {
            write_simple(&mut ctx.writer, RESP_420_NO_CURRENT).await?;
            return Ok(());
        }
    } else if ctx.state.current_group.is_none() {
        write_simple(&mut ctx.writer, RESP_412_NO_GROUP).await?;
        return Ok(());
    } else {
        write_simple(&mut ctx.writer, RESP_420_NO_CURRENT).await?;
        return Ok(());
    }

    ctx.writer.write_all(RESP_225_HEADERS.as_bytes()).await?;
    for (n, article) in articles {
        for (name, val) in &article.headers {
            let mut v = val.replace('\t', " ");
            v.retain(|c| c != '\r' && c != '\n');
            ctx.writer
                .write_all(format!("{n} {name}: {v}\r\n").as_bytes())
                .await?;
        }
    }
    ctx.writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
    Ok(())
}

/// Collect header values for HDR/XPAT commands.
async fn collect_header_values(
    storage: &crate::storage::DynStorage,
    state: &crate::ConnectionState,
    field: &str,
    range_or_msgid: Option<&str>,
) -> Result<Vec<(u64, Option<String>)>, Box<dyn Error + Send + Sync>> {
    let mut values = Vec::new();

    if let Some(arg) = range_or_msgid {
        if arg.starts_with('<') && arg.ends_with('>') {
            if let Some(article) = storage.get_article_by_id(arg).await? {
                let val = if field.starts_with(':') {
                    metadata_value(storage, &article, field).await
                } else {
                    get_header_value(&article, field)
                };
                values.push((0, val));
            }
        } else if let Some(group) = state.current_group.as_deref() {
            let nums = parse_range(storage, group, arg).await?;
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
        }
    }

    Ok(values)
}
