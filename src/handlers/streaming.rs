//! Streaming command handlers (IHAVE, CHECK, TAKETHIS).

use super::utils::{comprehensive_validate_article, read_message, write_simple};
use super::{CommandHandler, HandlerContext, HandlerResult};
use crate::prelude::*;
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
            if ctx
                .storage
                .get_article_by_id(id)
                .await
                .to_anyhow()?
                .is_some()
            {
                write_simple(&mut ctx.writer, RESP_435_NOT_WANTED).await?;
                return Ok(());
            }

            write_simple(&mut ctx.writer, RESP_335_SEND_IT).await?;
            let msg = read_message(&mut ctx.reader).await?;
            let Ok((_, mut article)) = parse_message(&msg) else {
                write_simple(&mut ctx.writer, RESP_437_REJECTED).await?;
                return Ok(());
            };

            // Check if this is a control message first
            let is_control = control::is_control_message(&article);

            let cfg_guard = ctx.config.read().await;
            ensure_message_id(&mut article, &cfg_guard.site_name);
            parse::ensure_date(&mut article);
            parse::escape_message_id_header(&mut article);

            // Handle control messages immediately without comprehensive validation
            if is_control {
                if control::handle_control(&article, &ctx.storage, &ctx.auth, &cfg_guard).await? {
                    write_simple(&mut ctx.writer, RESP_235_TRANSFER_OK).await?;
                    return Ok(());
                } else {
                    write_simple(&mut ctx.writer, RESP_437_REJECTED).await?;
                    return Ok(());
                }
            }

            // Comprehensive validation before queuing for IHAVE (non-control messages)
            let size = msg.len() as u64;
            if comprehensive_validate_article(&ctx.storage, &ctx.auth, &cfg_guard, &article, size)
                .await
                .is_err()
            {
                write_simple(&mut ctx.writer, RESP_437_REJECTED).await?;
                return Ok(());
            }
            drop(cfg_guard);

            // Submit to queue for background storage and immediate storage for protocol compliance
            let queued_article = crate::queue::QueuedArticle {
                message: article.clone(),
                size,
                is_control: false, // Control messages are handled above, so this is always false
                already_validated: true, // IHAVE does comprehensive validation before queuing
            };

            // Store immediately for protocol compliance (second IHAVE should know article exists)
            if ctx.storage.store_article(&article).await.is_err() {
                write_simple(&mut ctx.writer, RESP_437_REJECTED).await?;
                return Ok(());
            }

            // Also queue for background processing consistency
            let _ = ctx.queue.submit(queued_article).await; // Don't fail if queue is full since we already stored
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

            // Check if this is a control message first
            let is_control = control::is_control_message(&article);

            let cfg_guard = ctx.config.read().await;
            ensure_message_id(&mut article, &cfg_guard.site_name);
            parse::ensure_date(&mut article);
            parse::escape_message_id_header(&mut article);

            // Handle control messages immediately without comprehensive validation
            if is_control {
                if control::handle_control(&article, &ctx.storage, &ctx.auth, &cfg_guard).await? {
                    write_simple(&mut ctx.writer, &format!("239 {id}\r\n")).await?;
                    return Ok(());
                } else {
                    write_simple(&mut ctx.writer, &format!("439 {id}\r\n")).await?;
                    return Ok(());
                }
            }

            // Comprehensive validation before queuing for TAKETHIS (non-control messages)
            let size = msg.len() as u64;
            if comprehensive_validate_article(&ctx.storage, &ctx.auth, &cfg_guard, &article, size)
                .await
                .is_err()
            {
                write_simple(&mut ctx.writer, &format!("439 {id}\r\n")).await?;
                return Ok(());
            }
            drop(cfg_guard);

            // Submit to queue for background storage and immediate storage for protocol compliance
            let queued_article = crate::queue::QueuedArticle {
                message: article.clone(),
                size,
                is_control: false, // Control messages are handled above, so this is always false
                already_validated: true, // TAKETHIS does comprehensive validation before queuing
            };

            // Store immediately for protocol compliance (duplicate TAKETHIS should be detected)
            if ctx.storage.store_article(&article).await.is_err() {
                write_simple(&mut ctx.writer, &format!("439 {id}\r\n")).await?;
                return Ok(());
            }

            // Also queue for background processing consistency
            let _ = ctx.queue.submit(queued_article).await; // Don't fail if queue is full since we already stored
            write_simple(&mut ctx.writer, &format!("239 {id}\r\n")).await?;
        } else {
            write_simple(&mut ctx.writer, RESP_501_MSGID_REQUIRED).await?;
        }
        Ok(())
    }
}
