//! Posting command handlers.

use super::utils::{comprehensive_validate_article, read_message, write_simple};
use super::{CommandHandler, HandlerContext, HandlerResult};
use crate::prelude::*;
use crate::queue::QueuedArticle;
use crate::responses::*;
use crate::{control, ensure_message_id, parse, parse_message};
use tokio::io::{AsyncBufRead, AsyncWrite};

/// Handler for the POST command.
pub struct PostHandler;

impl CommandHandler for PostHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, _args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        if !ctx.state.is_tls && !ctx.state.allow_posting_insecure {
            write_simple(&mut ctx.writer, RESP_483_SECURE_REQ).await?;
            return Ok(());
        }

        if !ctx.state.authenticated {
            write_simple(&mut ctx.writer, RESP_480_AUTH_REQUIRED).await?;
            return Ok(());
        }

        write_simple(&mut ctx.writer, RESP_340_SEND_ARTICLE).await?;

        let msg = read_message(&mut ctx.reader).await?;
        let Ok((_, mut message)) = parse_message(&msg) else {
            write_simple(&mut ctx.writer, RESP_441_POSTING_FAILED).await?;
            return Ok(());
        };

        // Check if this is a control message first
        let is_control = control::is_control_message(&message);

        // Ensure required headers
        let cfg_guard = ctx.config.read().await;
        ensure_message_id(&mut message, &cfg_guard.site_name);
        parse::ensure_date(&mut message);
        parse::escape_message_id_header(&mut message);

        // Comprehensive validation before queuing for POST (to maintain expected behavior)
        let size = msg.len() as u64;
        if comprehensive_validate_article(&ctx.storage, &ctx.auth, &cfg_guard, &message, size)
            .await
            .is_err()
        {
            write_simple(&mut ctx.writer, RESP_441_POSTING_FAILED).await?;
            return Ok(());
        }
        drop(cfg_guard);

        // Submit to queue for background processing
        let queued_article = QueuedArticle {
            message,
            size,
            is_control,
            already_validated: true, // POST uses comprehensive validation and queues for storage only
        };

        if ctx.queue.submit(queued_article).await.is_err() {
            write_simple(&mut ctx.writer, RESP_441_POSTING_FAILED).await?;
            return Ok(());
        }

        write_simple(&mut ctx.writer, RESP_240_ARTICLE_RECEIVED).await?;
        Ok(())
    }
}

/// Validate an article for posting (legacy function, now uses comprehensive validation).
pub async fn validate_article(
    storage: &crate::storage::DynStorage,
    auth: &crate::auth::DynAuth,
    cfg: &crate::config::Config,
    article: &crate::Message,
    size: u64,
) -> Result<()> {
    comprehensive_validate_article(storage, auth, cfg, article, size).await
}
