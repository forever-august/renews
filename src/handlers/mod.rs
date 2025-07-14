//! NNTP command handlers module.
//!
//! This module contains handlers for all NNTP commands, organized by category.

pub mod article;
pub mod auth;
pub mod group;
pub mod info;
pub mod post;
pub mod streaming;
pub mod utils;

use crate::auth::DynAuth;
use crate::config::Config;
use crate::queue::ArticleQueue;
use crate::storage::DynStorage;
use crate::{Command, ConnectionState};
use anyhow::Result;
use std::sync::Arc;
use tokio::io::{AsyncBufRead, AsyncWrite};
use tokio::sync::RwLock;

/// Result type for command handlers.
pub type HandlerResult = Result<()>;

/// Context passed to command handlers.
pub struct HandlerContext<R, W> {
    pub reader: R,
    pub writer: W,
    pub storage: DynStorage,
    pub auth: DynAuth,
    pub config: Arc<RwLock<Config>>,
    pub state: ConnectionState,
    pub queue: ArticleQueue,
}

/// Trait for command handlers.
#[allow(async_fn_in_trait)]
pub trait CommandHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin;
}

/// Dispatch a command to the appropriate handler.
pub async fn dispatch_command<R, W>(ctx: &mut HandlerContext<R, W>, cmd: &Command) -> HandlerResult
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    match cmd.name.to_ascii_uppercase().as_str() {
        // Article retrieval commands
        "ARTICLE" => article::ArticleHandler::handle(ctx, &cmd.args).await,
        "HEAD" => article::HeadHandler::handle(ctx, &cmd.args).await,
        "BODY" => article::BodyHandler::handle(ctx, &cmd.args).await,
        "STAT" => article::StatHandler::handle(ctx, &cmd.args).await,

        // Group and navigation commands
        "GROUP" => group::GroupHandler::handle(ctx, &cmd.args).await,
        "LIST" => group::ListHandler::handle(ctx, &cmd.args).await,
        "LISTGROUP" => group::ListGroupHandler::handle(ctx, &cmd.args).await,
        "NEXT" => group::NextHandler::handle(ctx, &cmd.args).await,
        "LAST" => group::LastHandler::handle(ctx, &cmd.args).await,
        "NEWGROUPS" => group::NewGroupsHandler::handle(ctx, &cmd.args).await,
        "NEWNEWS" => group::NewNewsHandler::handle(ctx, &cmd.args).await,

        // Header and metadata commands
        "HDR" => article::HdrHandler::handle(ctx, &cmd.args).await,
        "XPAT" => article::XPatHandler::handle(ctx, &cmd.args).await,
        "OVER" => article::OverHandler::handle(ctx, &cmd.args).await,
        "XOVER" => article::OverHandler::handle(ctx, &cmd.args).await,

        // Posting and streaming commands
        "POST" => post::PostHandler::handle(ctx, &cmd.args).await,
        "IHAVE" => streaming::IHaveHandler::handle(ctx, &cmd.args).await,
        "CHECK" => streaming::CheckHandler::handle(ctx, &cmd.args).await,
        "TAKETHIS" => streaming::TakeThisHandler::handle(ctx, &cmd.args).await,

        // Authentication and mode commands
        "AUTHINFO" => auth::AuthInfoHandler::handle(ctx, &cmd.args).await,
        "MODE" => auth::ModeHandler::handle(ctx, &cmd.args).await,

        // Information commands
        "CAPABILITIES" => info::CapabilitiesHandler::handle(ctx, &cmd.args).await,
        "DATE" => info::DateHandler::handle(ctx, &cmd.args).await,
        "HELP" => info::HelpHandler::handle(ctx, &cmd.args).await,
        "QUIT" => info::QuitHandler::handle(ctx, &cmd.args).await,

        // Unknown command
        _ => {
            use crate::responses::RESP_500_UNKNOWN_CMD;
            use tokio::io::AsyncWriteExt;
            ctx.writer
                .write_all(RESP_500_UNKNOWN_CMD.as_bytes())
                .await?;
            Ok(())
        }
    }
}
