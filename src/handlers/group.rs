//! Group and listing command handlers.

use super::utils::write_simple;
use super::{CommandHandler, HandlerContext, HandlerResult};
use crate::responses::*;
use crate::{parse_datetime, wildmat};
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
            let nums = ctx.storage.list_article_numbers(group_name).await?;
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

        let nums = ctx.storage.list_article_numbers(&group_name).await?;
        write_simple(&mut ctx.writer, "211 article numbers follow\r\n").await?;

        for num in nums {
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
        if let (Some(group), Some(current)) = (
            ctx.state.current_group.as_deref(),
            ctx.state.current_article,
        ) {
            let nums = ctx.storage.list_article_numbers(group).await?;
            if let Some(pos) = nums.iter().position(|&n| n == current) {
                if let Some(&next_num) = nums.get(pos + 1) {
                    if let Some(article) =
                        ctx.storage.get_article_by_number(group, next_num).await?
                    {
                        ctx.state.current_article = Some(next_num);
                        let id = super::utils::extract_message_id(&article).unwrap_or("");
                        write_simple(
                            &mut ctx.writer,
                            &format!("223 {next_num} {id} article exists\r\n"),
                        )
                        .await?;
                    } else {
                        write_simple(&mut ctx.writer, RESP_421_NO_NEXT).await?;
                    }
                } else {
                    write_simple(&mut ctx.writer, RESP_421_NO_NEXT).await?;
                }
            } else {
                write_simple(&mut ctx.writer, RESP_421_NO_NEXT).await?;
            }
        } else {
            write_simple(&mut ctx.writer, RESP_412_NO_GROUP).await?;
        }
        Ok(())
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
        if let (Some(group), Some(current)) = (
            ctx.state.current_group.as_deref(),
            ctx.state.current_article,
        ) {
            let nums = ctx.storage.list_article_numbers(group).await?;
            if let Some(pos) = nums.iter().position(|&n| n == current) {
                if pos > 0 {
                    let prev_num = nums[pos - 1];
                    if let Some(article) =
                        ctx.storage.get_article_by_number(group, prev_num).await?
                    {
                        ctx.state.current_article = Some(prev_num);
                        let id = super::utils::extract_message_id(&article).unwrap_or("");
                        write_simple(
                            &mut ctx.writer,
                            &format!("223 {prev_num} {id} article exists\r\n"),
                        )
                        .await?;
                    } else {
                        write_simple(&mut ctx.writer, RESP_422_NO_PREV).await?;
                    }
                } else {
                    write_simple(&mut ctx.writer, RESP_422_NO_PREV).await?;
                }
            } else {
                write_simple(&mut ctx.writer, RESP_422_NO_PREV).await?;
            }
        } else {
            write_simple(&mut ctx.writer, RESP_412_NO_GROUP).await?;
        }
        Ok(())
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
        let groups = ctx.storage.list_groups_since(since).await?;

        write_simple(&mut ctx.writer, RESP_231_NEWGROUPS).await?;
        for group in groups {
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

        let groups = ctx.storage.list_groups().await?;
        write_simple(&mut ctx.writer, RESP_230_NEWNEWS).await?;

        for group in groups {
            if wildmat::wildmat(&group, wildmat_pattern) {
                let articles = ctx.storage.list_article_ids_since(&group, since).await?;
                for article_id in articles {
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
    let groups = ctx.storage.list_groups().await?;
    write_simple(&mut ctx.writer, RESP_215_LIST_FOLLOWS).await?;

    for group in groups {
        if let Some(pat) = pattern {
            if !wildmat::wildmat(&group, pat) {
                continue;
            }
        }

        let nums = ctx.storage.list_article_numbers(&group).await?;
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
    let groups = ctx.storage.list_groups().await?;
    write_simple(&mut ctx.writer, "215 descriptions follow\r\n").await?;

    for group in groups {
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
    let groups_with_times = ctx.storage.list_groups_with_times().await?;
    write_simple(&mut ctx.writer, "215 information follows\r\n").await?;

    for (group, time) in groups_with_times {
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
    ctx.writer
        .write_all(RESP_215_OVERVIEW_FMT.as_bytes())
        .await?;
    ctx.writer.write_all(RESP_SUBJECT.as_bytes()).await?;
    ctx.writer.write_all(RESP_FROM.as_bytes()).await?;
    ctx.writer.write_all(RESP_DATE.as_bytes()).await?;
    ctx.writer.write_all(RESP_MESSAGE_ID.as_bytes()).await?;
    ctx.writer.write_all(RESP_REFERENCES.as_bytes()).await?;
    ctx.writer.write_all(RESP_BYTES.as_bytes()).await?;
    ctx.writer.write_all(RESP_LINES.as_bytes()).await?;
    ctx.writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
    Ok(())
}

async fn handle_list_headers<R, W>(ctx: &mut HandlerContext<R, W>) -> HandlerResult
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    ctx.writer.write_all(RESP_215_METADATA.as_bytes()).await?;
    ctx.writer.write_all(RESP_COLON.as_bytes()).await?;
    ctx.writer.write_all(RESP_LINES.as_bytes()).await?;
    ctx.writer.write_all(RESP_BYTES.as_bytes()).await?;
    ctx.writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
    Ok(())
}
