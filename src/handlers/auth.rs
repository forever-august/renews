//! Authentication and mode command handlers.

use super::utils::write_simple;
use super::{CommandHandler, HandlerContext, HandlerResult};
use crate::error::AuthError;
use crate::responses::*;
use tracing::Span;

/// Handler for the AUTHINFO command.
pub struct AuthInfoHandler;

impl CommandHandler for AuthInfoHandler {
    async fn handle(ctx: &mut HandlerContext, args: &[String]) -> HandlerResult {
        // Reject authentication on insecure connections unless explicitly allowed
        if !ctx.session.can_authenticate() {
            Span::current().record("outcome", "rejected_insecure");
            write_simple(&mut ctx.writer, RESP_483_SECURE_REQ).await?;
            return Ok(());
        }

        if args.is_empty() {
            write_simple(&mut ctx.writer, RESP_501_NOT_ENOUGH).await?;
            return Ok(());
        }

        match args[0].to_ascii_uppercase().as_str() {
            "USER" => {
                if args.len() < 2 {
                    write_simple(&mut ctx.writer, RESP_501_NOT_ENOUGH).await?;
                    return Ok(());
                }
                ctx.session.set_pending_username(args[1].clone());
                write_simple(&mut ctx.writer, RESP_381_PASSWORD_REQ).await?;
            }
            "PASS" => {
                if args.len() < 2 {
                    write_simple(&mut ctx.writer, RESP_501_NOT_ENOUGH).await?;
                    return Ok(());
                }

                if let Some(username) = ctx.session.pending_username() {
                    let username = username.to_string(); // Clone to avoid borrow issues
                    if ctx.auth.verify_user(&username, &args[1]).await? {
                        ctx.session.confirm_authentication();
                        Span::current().record("outcome", "success");
                        // Log username only at debug level for GDPR compliance
                        tracing::debug!(username = %username, "User authenticated");
                        write_simple(&mut ctx.writer, RESP_281_AUTH_OK).await?;
                    } else {
                        let err = AuthError::InvalidCredentials(username.clone());
                        // Log failure at info level without username, debug level with username
                        tracing::info!("Authentication failed");
                        tracing::debug!(username = %username, error = %err, "Authentication failed details");
                        Span::current().record("outcome", "rejected_invalid");
                        write_simple(&mut ctx.writer, RESP_481_AUTH_REJECTED).await?;
                    }
                } else {
                    Span::current().record("outcome", "rejected_no_user");
                    write_simple(&mut ctx.writer, RESP_481_AUTH_REJECTED).await?;
                }
            }
            _ => {
                write_simple(&mut ctx.writer, RESP_501_SYNTAX).await?;
            }
        }
        Ok(())
    }
}

/// Handler for the MODE command.
pub struct ModeHandler;

impl CommandHandler for ModeHandler {
    async fn handle(ctx: &mut HandlerContext, args: &[String]) -> HandlerResult {
        if args.is_empty() {
            write_simple(&mut ctx.writer, RESP_501_MISSING_MODE).await?;
            return Ok(());
        }

        match args[0].to_ascii_uppercase().as_str() {
            "READER" => {
                if ctx.session.can_post() {
                    write_simple(&mut ctx.writer, RESP_200_POSTING_ALLOWED).await?;
                } else {
                    write_simple(&mut ctx.writer, RESP_201_POSTING_PROHIBITED).await?;
                }
            }
            "STREAM" => {
                write_simple(&mut ctx.writer, RESP_203_STREAMING).await?;
            }
            _ => {
                write_simple(&mut ctx.writer, RESP_501_UNKNOWN_MODE).await?;
            }
        }
        Ok(())
    }
}
