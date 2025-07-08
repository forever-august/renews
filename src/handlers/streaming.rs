//! Streaming command handlers (IHAVE, CHECK, TAKETHIS).

use super::utils::{read_message, write_simple};
use super::{CommandHandler, HandlerContext, HandlerResult};
use crate::responses::*;
use crate::{control, ensure_message_id, parse, parse_message};
use tokio::io::{AsyncBufRead, AsyncWrite};

/// Handler for the IHAVE command.
pub struct IHaveHandler;

impl CommandHandler for IHaveHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        if let Some(id) = args.first() {
            if ctx.storage.get_article_by_id(id).await?.is_some() {
                write_simple(&mut ctx.writer, RESP_435_NOT_WANTED).await?;
                return Ok(());
            }

            write_simple(&mut ctx.writer, RESP_335_SEND_IT).await?;
            let msg = read_message(&mut ctx.reader).await?;
            let Ok((_, mut article)) = parse_message(&msg) else {
                write_simple(&mut ctx.writer, RESP_437_REJECTED).await?;
                return Ok(());
            };

            if control::handle_control(&article, &ctx.storage, &ctx.auth).await? {
                write_simple(&mut ctx.writer, RESP_235_TRANSFER_OK).await?;
                return Ok(());
            }

            ensure_message_id(&mut article);
            parse::ensure_date(&mut article);
            parse::escape_message_id_header(&mut article);

            let cfg_guard = ctx.config.read().await;
            let size = msg.len() as u64;
            if super::post::validate_article(&ctx.storage, &ctx.auth, &cfg_guard, &article, size)
                .await
                .is_err()
            {
                write_simple(&mut ctx.writer, RESP_437_REJECTED).await?;
                return Ok(());
            }

            ctx.storage.store_article(&article).await?;
            write_simple(&mut ctx.writer, RESP_235_TRANSFER_OK).await?;
        } else {
            write_simple(&mut ctx.writer, RESP_501_MSGID_REQUIRED).await?;
        }
        Ok(())
    }
}

/// Handler for the CHECK command.
pub struct CheckHandler;

impl CommandHandler for CheckHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        if let Some(id) = args.first() {
            if ctx.storage.get_article_by_id(id).await?.is_some() {
                write_simple(&mut ctx.writer, &format!("438 {id}\r\n")).await?;
            } else {
                write_simple(&mut ctx.writer, &format!("238 {id}\r\n")).await?;
            }
        } else {
            write_simple(&mut ctx.writer, RESP_501_MSGID_REQUIRED).await?;
        }
        Ok(())
    }
}

/// Handler for the TAKETHIS command.
pub struct TakeThisHandler;

impl CommandHandler for TakeThisHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        if let Some(id) = args.first() {
            let msg = read_message(&mut ctx.reader).await?;
            let Ok((_, mut article)) = parse_message(&msg) else {
                write_simple(&mut ctx.writer, &format!("439 {id}\r\n")).await?;
                return Ok(());
            };

            if ctx.storage.get_article_by_id(id).await?.is_some() {
                write_simple(&mut ctx.writer, &format!("439 {id}\r\n")).await?;
                return Ok(());
            }

            if control::handle_control(&article, &ctx.storage, &ctx.auth).await? {
                write_simple(&mut ctx.writer, &format!("239 {id}\r\n")).await?;
                return Ok(());
            }

            ensure_message_id(&mut article);
            parse::ensure_date(&mut article);
            parse::escape_message_id_header(&mut article);

            let cfg_guard = ctx.config.read().await;
            let size = msg.len() as u64;
            if super::post::validate_article(&ctx.storage, &ctx.auth, &cfg_guard, &article, size)
                .await
                .is_err()
            {
                write_simple(&mut ctx.writer, &format!("439 {id}\r\n")).await?;
                return Ok(());
            }

            ctx.storage.store_article(&article).await?;
            write_simple(&mut ctx.writer, &format!("239 {id}\r\n")).await?;
        } else {
            write_simple(&mut ctx.writer, RESP_501_MSGID_REQUIRED).await?;
        }
        Ok(())
    }
}
