//! Article retrieval command handlers.

use super::utils::{
    ArticleOperation, get_header_value, handle_article_operation,
    metadata_value, resolve_articles, write_response_with_values, write_simple,
};
use super::{CommandHandler, HandlerContext, HandlerResult};
use crate::overview::generate_overview_line;
use crate::parse_range;
use crate::responses::*;
use std::error::Error;
use tokio::io::{AsyncBufRead, AsyncWrite, AsyncWriteExt};

/// Macro to create simple article command handlers.
macro_rules! article_handler {
    ($name:ident, $operation:expr) => {
        pub struct $name;

        impl CommandHandler for $name {
            async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, args: &[String]) -> HandlerResult
            where
                R: AsyncBufRead + Unpin,
                W: AsyncWrite + Unpin,
            {
                handle_article_operation(
                    &mut ctx.writer,
                    &ctx.storage,
                    &mut ctx.state,
                    args,
                    $operation,
                )
                .await
            }
        }
    };
}

// Generate handlers for basic article operations
article_handler!(ArticleHandler, ArticleOperation::Full);
article_handler!(HeadHandler, ArticleOperation::Headers);
article_handler!(BodyHandler, ArticleOperation::Body);
article_handler!(StatHandler, ArticleOperation::Stat);

/// Handler for the HDR command.
pub struct HdrHandler;

impl CommandHandler for HdrHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        if args.is_empty() {
            return write_simple(&mut ctx.writer, RESP_501_NOT_ENOUGH).await;
        }

        let field = &args[0];

        // Handle special case for all headers
        if field == ":" {
            return handle_all_headers(ctx, args).await;
        }

        // Collect header values for the specified field
        let values = collect_header_values(
            &ctx.storage,
            &ctx.state,
            field,
            args.get(1).map(|s| s.as_str()),
        )
        .await?;

        // Send response
        write_response_with_values(&mut ctx.writer, RESP_225_HEADERS, &values).await
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
            return write_simple(&mut ctx.writer, RESP_501_NOT_ENOUGH).await;
        }

        let field = &args[0];
        let range_or_msgid = &args[1];
        let patterns: Vec<&str> = args[2..].iter().map(String::as_str).collect();

        let values =
            collect_header_values(&ctx.storage, &ctx.state, field, Some(range_or_msgid)).await?;

        write_simple(&mut ctx.writer, RESP_221_HEADER_FOLLOWS).await?;

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
        match resolve_articles(
            &ctx.storage,
            &mut ctx.state,
            args.first().map(String::as_str),
        )
        .await
        {
            Ok(articles) => {
                ctx.writer.write_all(RESP_224_OVERVIEW.as_bytes()).await?;
                for (num, article) in articles {
                    let overview_line = generate_overview_line(ctx.storage.as_ref(), num, &article).await?;
                    ctx.writer
                        .write_all(format!("{overview_line}\r\n").as_bytes())
                        .await?;
                }
                ctx.writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
            }
            Err(error) => {
                use super::utils::handle_article_error;
                handle_article_error(&mut ctx.writer, error).await?;
            }
        }
        Ok(())
    }
}

/// Handler for the XOVER command (RFC2980).
/// This is functionally equivalent to OVER but follows the RFC2980 specification.
pub struct XOverHandler;

impl CommandHandler for XOverHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        // XOVER command handling with optimized overview storage when possible
        if let Some(current_group) = &ctx.state.current_group {
            match args.first().map(String::as_str) {
                Some(range_str) if !range_str.starts_with('<') => {
                    // Handle range-based requests efficiently using overview storage
                    match parse_range(&ctx.storage, current_group, range_str).await {
                        Ok(numbers) => {
                            if !numbers.is_empty() {
                                let start = *numbers.iter().min().unwrap();
                                let end = *numbers.iter().max().unwrap();
                                
                                match ctx.storage.get_overview_range(current_group, start, end).await {
                                    Ok(overview_lines) => {
                                        ctx.writer.write_all(RESP_224_OVERVIEW.as_bytes()).await?;
                                        for line in overview_lines {
                                            ctx.writer
                                                .write_all(format!("{line}\r\n").as_bytes())
                                                .await?;
                                        }
                                        ctx.writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
                                        return Ok(());
                                    }
                                    Err(_) => {
                                        // Fall back to the standard OVER implementation
                                    }
                                }
                            }
                        }
                        Err(_) => {
                            // Fall back to the standard OVER implementation
                        }
                    }
                }
                _ => {}
            }
        }
        
        // Fall back to standard OVER implementation for non-range requests
        // or when overview storage fails
        OverHandler::handle(ctx, args).await
    }
}

/// Handle the special case of HDR with ":" for all headers.
async fn handle_all_headers<R, W>(ctx: &mut HandlerContext<R, W>, args: &[String]) -> HandlerResult
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    // Use the existing resolve_articles function to handle the complex logic
    let articles = match resolve_articles(
        &ctx.storage,
        &mut ctx.state,
        args.get(1).map(String::as_str),
    )
    .await
    {
        Ok(articles) => articles,
        Err(error) => {
            use super::utils::handle_article_error;
            handle_article_error(&mut ctx.writer, error).await?;
            return Ok(());
        }
    };

    ctx.writer.write_all(RESP_225_HEADERS.as_bytes()).await?;
    for (n, article) in articles {
        for (name, val) in &article.headers {
            let sanitized_val = sanitize_header_value(val);
            ctx.writer
                .write_all(format!("{n} {name}: {sanitized_val}\r\n").as_bytes())
                .await?;
        }
    }
    ctx.writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
    Ok(())
}

/// Sanitize header values by removing tabs and line breaks.
fn sanitize_header_value(val: &str) -> String {
    let mut v = val.replace('\t', " ");
    v.retain(|c| c != '\r' && c != '\n');
    v
}

/// Extract header value for a field (handles both standard headers and metadata).
async fn get_field_value(
    storage: &crate::storage::DynStorage,
    article: &crate::Message,
    field: &str,
) -> Option<String> {
    if field.starts_with(':') {
        metadata_value(storage, article, field).await
    } else {
        get_header_value(article, field)
    }
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
            // Message-ID lookup
            if let Some(article) = storage.get_article_by_id(arg).await? {
                let val = get_field_value(storage, &article, field).await;
                values.push((0, val));
            }
        } else if let Some(group) = state.current_group.as_deref() {
            // Range lookup
            let nums = parse_range(storage, group, arg).await?;
            for n in nums {
                if let Some(article) = storage.get_article_by_number(group, n).await? {
                    let val = get_field_value(storage, &article, field).await;
                    values.push((n, val));
                }
            }
        }
    } else if let (Some(group), Some(num)) = (state.current_group.as_deref(), state.current_article)
    {
        // Current article lookup
        if let Some(article) = storage.get_article_by_number(group, num).await? {
            let val = get_field_value(storage, &article, field).await;
            values.push((num, val));
        }
    }

    Ok(values)
}
