//! Group and listing command handlers.

use super::utils::{write_lines, write_simple};
use super::{CommandHandler, HandlerContext, HandlerResult};
use crate::responses::*;
use crate::{parse_datetime, wildmat};
use futures_util::{StreamExt, TryStreamExt};
use tokio::io::{AsyncBufRead, AsyncWrite, AsyncWriteExt};

/// Handler for the GROUP command.
pub struct GroupHandler;

impl CommandHandler for GroupHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        if let Some(group_name) = args.first() {
            let stream = ctx.storage.list_article_numbers(group_name);
            let nums = stream.try_collect::<Vec<u64>>().await?;
            let count = nums.len();
            let high = nums.last().copied().unwrap_or(0);
            let low = nums.first().copied().unwrap_or(0);

            ctx.state.current_group = Some(group_name.clone());
            ctx.state.current_article = nums.first().copied();

            write_simple(
                &mut ctx.writer,
                &format!("211 {count} {low} {high} {group_name}\r\n"),
            )
            .await?;
        } else {
            write_simple(&mut ctx.writer, RESP_501_NOT_ENOUGH).await?;
        }
        Ok(())
    }
}

/// Handler for the LIST command.
pub struct ListHandler;

impl CommandHandler for ListHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        if let Some(keyword) = args.first() {
            match keyword.as_str() {
                "ACTIVE" => {
                    handle_list_active(ctx, args.get(1)).await?;
                }
                "NEWSGROUPS" => {
                    handle_list_newsgroups(ctx).await?;
                }
                "ACTIVE.TIMES" => {
                    handle_list_active_times(ctx).await?;
                }
                "OVERVIEW.FMT" => {
                    handle_list_overview_fmt(ctx).await?;
                }
                "HEADERS" => {
                    handle_list_headers(ctx).await?;
                }
                "DISTRIB.PATS" => {
                    write_simple(&mut ctx.writer, RESP_503_NOT_SUPPORTED).await?;
                }
                _ => {
                    write_simple(&mut ctx.writer, RESP_501_UNKNOWN_KEYWORD).await?;
                }
            }
        } else {
            // Default LIST without keyword behaves like LIST ACTIVE
            handle_list_active(ctx, None).await?;
        }
        Ok(())
    }
}

/// Handler for the LISTGROUP command.
pub struct ListGroupHandler;

impl CommandHandler for ListGroupHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let group_name = if let Some(name) = args.first() {
            name.clone()
        } else if let Some(ref current) = ctx.state.current_group {
            current.clone()
        } else {
            write_simple(&mut ctx.writer, RESP_412_NO_GROUP).await?;
            return Ok(());
        };

        write_simple(&mut ctx.writer, RESP_211_LISTGROUP).await?;
        let mut stream = ctx.storage.list_article_numbers(&group_name);
        while let Some(result) = stream.next().await {
            let num = result?;
            ctx.writer
                .write_all(format!("{num}\r\n").as_bytes())
                .await?;
        }
        ctx.writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
        Ok(())
    }
}

/// Handler for the NEXT command.
pub struct NextHandler;

impl CommandHandler for NextHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, _args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        navigate_article(ctx, NavigationDirection::Next).await
    }
}

/// Handler for the LAST command.
pub struct LastHandler;

impl CommandHandler for LastHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, _args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        navigate_article(ctx, NavigationDirection::Previous).await
    }
}

/// Handler for the NEWGROUPS command.
pub struct NewGroupsHandler;

impl CommandHandler for NewGroupsHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        if args.len() < 2 {
            write_simple(&mut ctx.writer, RESP_501_NOT_ENOUGH).await?;
            return Ok(());
        }

        let date = &args[0];
        let time = &args[1];
        let gmt = match args.get(2) {
            Some(arg) => {
                if !arg.eq_ignore_ascii_case("GMT") {
                    write_simple(&mut ctx.writer, RESP_501_INVALID_ARG).await?;
                    return Ok(());
                }
                true
            }
            None => false,
        };
        let Ok(since) = parse_datetime(date, time, gmt) else {
            write_simple(&mut ctx.writer, RESP_501_INVALID_DATE).await?;
            return Ok(());
        };

        write_simple(&mut ctx.writer, RESP_231_NEWGROUPS).await?;
        let mut stream = ctx.storage.list_groups_since(since);
        while let Some(result) = stream.next().await {
            let group = result?;
            ctx.writer
                .write_all(format!("{group}\r\n").as_bytes())
                .await?;
        }
        ctx.writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
        Ok(())
    }
}

/// Handler for the NEWNEWS command.
pub struct NewNewsHandler;

impl CommandHandler for NewNewsHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        if args.len() < 3 {
            write_simple(&mut ctx.writer, RESP_501_NOT_ENOUGH).await?;
            return Ok(());
        }

        let wildmat_pattern = &args[0];
        let date = &args[1];
        let time = &args[2];
        let gmt = match args.get(3) {
            Some(arg) => {
                if !arg.eq_ignore_ascii_case("GMT") {
                    write_simple(&mut ctx.writer, RESP_501_INVALID_ARG).await?;
                    return Ok(());
                }
                true
            }
            None => false,
        };
        let Ok(since) = parse_datetime(date, time, gmt) else {
            write_simple(&mut ctx.writer, RESP_501_INVALID_DATE).await?;
            return Ok(());
        };

        write_simple(&mut ctx.writer, RESP_230_NEWNEWS).await?;
        let mut groups_stream = ctx.storage.list_groups();
        while let Some(result) = groups_stream.next().await {
            let group = result?;
            if wildmat::wildmat(&group, wildmat_pattern) {
                let mut articles_stream = ctx.storage.list_article_ids_since(&group, since);
                while let Some(article_result) = articles_stream.next().await {
                    let article_id = article_result?;
                    ctx.writer
                        .write_all(format!("{article_id}\r\n").as_bytes())
                        .await?;
                }
            }
        }

        ctx.writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
        Ok(())
    }
}

// Helper functions for LIST subcommands

async fn handle_list_active<R, W>(
    ctx: &mut HandlerContext<R, W>,
    pattern: Option<&String>,
) -> HandlerResult
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    write_simple(&mut ctx.writer, RESP_215_LIST_FOLLOWS).await?;
    let mut groups_stream = ctx.storage.list_groups();
    while let Some(result) = groups_stream.next().await {
        let group = result?;
        if let Some(pat) = pattern {
            if !wildmat::wildmat(&group, pat) {
                continue;
            }
        }

        let nums_stream = ctx.storage.list_article_numbers(&group);
        let nums = nums_stream.try_collect::<Vec<u64>>().await?;
        let high = nums.last().copied().unwrap_or(0);
        let low = nums.first().copied().unwrap_or(0);
        ctx.writer
            .write_all(format!("{group} {high} {low} y\r\n").as_bytes())
            .await?;
    }

    ctx.writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
    Ok(())
}

async fn handle_list_newsgroups<R, W>(ctx: &mut HandlerContext<R, W>) -> HandlerResult
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    write_simple(&mut ctx.writer, RESP_215_DESCRIPTIONS).await?;
    let mut groups_stream = ctx.storage.list_groups();
    while let Some(result) = groups_stream.next().await {
        let group = result?;
        ctx.writer
            .write_all(format!("{group} \r\n").as_bytes())
            .await?;
    }
    ctx.writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
    Ok(())
}

async fn handle_list_active_times<R, W>(ctx: &mut HandlerContext<R, W>) -> HandlerResult
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    write_simple(&mut ctx.writer, RESP_215_INFO_FOLLOWS).await?;
    let mut stream = ctx.storage.list_groups_with_times();
    while let Some(result) = stream.next().await {
        let (group, time) = result?;
        ctx.writer
            .write_all(format!("{group} {time} -\r\n").as_bytes())
            .await?;
    }

    ctx.writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
    Ok(())
}

async fn handle_list_overview_fmt<R, W>(ctx: &mut HandlerContext<R, W>) -> HandlerResult
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    write_lines(
        &mut ctx.writer,
        &[
            RESP_215_OVERVIEW_FMT,
            RESP_SUBJECT,
            RESP_FROM,
            RESP_DATE,
            RESP_MESSAGE_ID,
            RESP_REFERENCES,
            RESP_BYTES,
            RESP_LINES,
            RESP_DOT_CRLF,
        ],
    )
    .await
}

async fn handle_list_headers<R, W>(ctx: &mut HandlerContext<R, W>) -> HandlerResult
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    write_lines(
        &mut ctx.writer,
        &[
            RESP_215_METADATA,
            RESP_COLON,
            RESP_LINES,
            RESP_BYTES,
            RESP_DOT_CRLF,
        ],
    )
    .await
}

/// Navigate to the next or previous article in the current group.
async fn navigate_article<R, W>(
    ctx: &mut HandlerContext<R, W>,
    direction: NavigationDirection,
) -> HandlerResult
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let (group, current) = match (&ctx.state.current_group, ctx.state.current_article) {
        (Some(group), Some(current)) => (group.as_str(), current),
        _ => {
            write_simple(&mut ctx.writer, RESP_412_NO_GROUP).await?;
            return Ok(());
        }
    };

    let stream = ctx.storage.list_article_numbers(group);
    let nums = stream.try_collect::<Vec<u64>>().await?;
    let pos = match nums.iter().position(|&n| n == current) {
        Some(pos) => pos,
        None => {
            let response = match direction {
                NavigationDirection::Next => RESP_421_NO_NEXT,
                NavigationDirection::Previous => RESP_422_NO_PREV,
            };
            write_simple(&mut ctx.writer, response).await?;
            return Ok(());
        }
    };

    let new_pos = match direction {
        NavigationDirection::Next => pos + 1,
        NavigationDirection::Previous => pos.saturating_sub(1),
    };

    if let Some(&new_num) = nums.get(new_pos) {
        if let Some(article) = ctx.storage.get_article_by_number(group, new_num).await? {
            ctx.state.current_article = Some(new_num);
            let id = super::utils::extract_message_id(&article).unwrap_or("");
            write_simple(
                &mut ctx.writer,
                &format!("223 {new_num} {id} article exists\r\n"),
            )
            .await?;
        } else {
            let response = match direction {
                NavigationDirection::Next => RESP_421_NO_NEXT,
                NavigationDirection::Previous => RESP_422_NO_PREV,
            };
            write_simple(&mut ctx.writer, response).await?;
        }
    } else {
        let response = match direction {
            NavigationDirection::Next => RESP_421_NO_NEXT,
            NavigationDirection::Previous => RESP_422_NO_PREV,
        };
        write_simple(&mut ctx.writer, response).await?;
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum NavigationDirection {
    Next,
    Previous,
}
