//! Information command handlers (DATE, HELP, CAPABILITIES, QUIT).

use super::utils::write_simple;
use super::{CommandHandler, HandlerContext, HandlerResult};
use crate::responses::*;
use tokio::io::{AsyncBufRead, AsyncWrite, AsyncWriteExt};

/// Handler for the DATE command.
pub struct DateHandler;

impl CommandHandler for DateHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, _args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        use chrono::Utc;
        let now = Utc::now().format("%Y%m%d%H%M%S").to_string();
        write_simple(&mut ctx.writer, &format!("111 {now}\r\n")).await?;
        Ok(())
    }
}

/// Handler for the HELP command.
pub struct HelpHandler;

impl CommandHandler for HelpHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, _args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        ctx.writer
            .write_all(RESP_100_HELP_FOLLOWS.as_bytes())
            .await?;
        ctx.writer.write_all(RESP_HELP_TEXT.as_bytes()).await?;
        ctx.writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
        Ok(())
    }
}

/// Handler for the CAPABILITIES command.
pub struct CapabilitiesHandler;

impl CommandHandler for CapabilitiesHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, _args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        ctx.writer
            .write_all(RESP_101_CAPABILITIES.as_bytes())
            .await?;
        ctx.writer.write_all(RESP_CAP_VERSION.as_bytes()).await?;
        ctx.writer
            .write_all(RESP_CAP_IMPLEMENTATION.as_bytes())
            .await?;
        ctx.writer.write_all(RESP_CAP_READER.as_bytes()).await?;

        if ctx.state.is_tls {
            ctx.writer.write_all(RESP_CAP_POST.as_bytes()).await?;
        }

        ctx.writer.write_all(RESP_CAP_NEWNEWS.as_bytes()).await?;
        ctx.writer.write_all(RESP_CAP_IHAVE.as_bytes()).await?;
        ctx.writer.write_all(RESP_CAP_STREAMING.as_bytes()).await?;
        ctx.writer.write_all(RESP_CAP_OVER.as_bytes()).await?;
        ctx.writer.write_all(RESP_CAP_HDR.as_bytes()).await?;
        ctx.writer.write_all(RESP_CAP_LIST.as_bytes()).await?;
        ctx.writer.write_all(RESP_DOT_CRLF.as_bytes()).await?;
        Ok(())
    }
}

/// Handler for the QUIT command.
pub struct QuitHandler;

impl CommandHandler for QuitHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, _args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        write_simple(&mut ctx.writer, RESP_205_CLOSING).await?;
        // Return an error to signal the connection should close
        Err(anyhow::anyhow!("Connection closed by QUIT command"))
    }
}
