pub mod parse;
pub use parse::{
    Command, Message, Response, ensure_date, ensure_message_id, parse_command, parse_datetime,
    parse_message, parse_range, parse_response,
};

pub mod auth;
pub mod config;
pub mod control;
pub mod error;
pub mod filters;
pub mod handlers;
mod migrations;
pub mod overview;
pub mod peers;
pub mod prelude;
pub mod queue;
pub mod responses;
pub mod retention;
pub mod server;
pub mod session;
pub mod storage;
pub mod wildmat;
#[cfg(feature = "websocket")]
pub mod ws;

use crate::auth::DynAuth;
use crate::config::Config;
use crate::handlers::{HandlerContext, dispatch_command};
use crate::queue::ArticleQueue;
use crate::session::Session;
use crate::storage::DynStorage;
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{self, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::RwLock;
use tracing::debug;

/// Per-connection cached configuration values.
/// These are read once at connection start and not updated mid-connection.
struct ConnectionConfig {
    #[allow(dead_code)]
    site_name: String,
    idle_timeout: Duration,
}

/// Handle a client connection.
///
/// # Errors
///
/// Returns an error if there's a problem handling the client connection,
/// such as network I/O errors or protocol violations.
#[tracing::instrument(skip(socket, storage, auth, cfg, queue))]
pub async fn handle_client<S>(
    socket: S,
    storage: DynStorage,
    auth: DynAuth,
    cfg: Arc<RwLock<Config>>,
    is_tls: bool,
    queue: ArticleQueue,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    use crate::responses::*;

    let (read_half, write_half) = io::split(socket);
    let reader = BufReader::new(read_half);

    // Cache configuration values at connection start so they don't change mid-connection
    let (connection_config, allow_auth_insecure, allow_anonymous_posting) = {
        let cfg_guard = cfg.read().await;
        (
            ConnectionConfig {
                site_name: cfg_guard.site_name.clone(),
                idle_timeout: Duration::from_secs(cfg_guard.idle_timeout_secs),
            },
            cfg_guard.allow_auth_insecure_connections,
            cfg_guard.allow_anonymous_posting,
        )
    };

    let mut ctx = HandlerContext {
        reader: Box::pin(reader),
        writer: Box::pin(write_half),
        storage,
        auth,
        config: cfg,
        session: Session::new(is_tls, allow_auth_insecure, allow_anonymous_posting),
        queue,
    };

    // Send greeting - reflects current posting ability
    if ctx.session.can_post() {
        ctx.writer.write_all(RESP_200_READY.as_bytes()).await?;
    } else {
        ctx.writer
            .write_all(RESP_201_READY_NO_POST.as_bytes())
            .await?;
    }

    let mut line = String::new();
    loop {
        line.clear();

        // Apply timeout to the read operation using cached idle_timeout
        let read_result = tokio::time::timeout(
            connection_config.idle_timeout,
            ctx.reader.read_line(&mut line),
        )
        .await;

        let n = match read_result {
            Ok(Ok(n)) => n,
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => {
                // Timeout occurred
                debug!(
                    "Connection timed out after {} seconds",
                    connection_config.idle_timeout.as_secs()
                );
                break;
            }
        };

        if n == 0 {
            break;
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        let Ok((_, cmd)) = parse_command(trimmed) else {
            ctx.writer.write_all(RESP_500_SYNTAX.as_bytes()).await?;
            continue;
        };

        debug!("command" = %cmd.name);

        // Handle QUIT specially since it needs to break the loop
        if cmd.name.as_str() == "QUIT" {
            ctx.writer.write_all(RESP_205_CLOSING.as_bytes()).await?;
            break;
        }

        if let Err(e) = dispatch_command(&mut ctx, &cmd).await {
            // Log the error but continue processing other commands
            debug!("Command {} failed: {}", cmd.name, e);
        }
    }

    Ok(())
}
