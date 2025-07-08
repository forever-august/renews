//! Posting command handlers.

use super::utils::write_simple;
use super::{CommandHandler, HandlerContext, HandlerResult};
use crate::responses::*;
use crate::{control, ensure_message_id, parse, parse_message};
use std::error::Error;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite};

/// Handler for the POST command.
pub struct PostHandler;

impl CommandHandler for PostHandler {
    async fn handle<R, W>(ctx: &mut HandlerContext<R, W>, _args: &[String]) -> HandlerResult
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        if !ctx.state.is_tls {
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

        // Handle control messages
        if control::handle_control(&message, &ctx.storage, &ctx.auth).await? {
            write_simple(&mut ctx.writer, RESP_240_ARTICLE_RECEIVED).await?;
            return Ok(());
        }

        // Ensure required headers
        ensure_message_id(&mut message);
        parse::ensure_date(&mut message);
        parse::escape_message_id_header(&mut message);

        // Validate article
        let cfg_guard = ctx.config.read().await;
        let size = msg.len() as u64;
        if validate_article(&ctx.storage, &ctx.auth, &cfg_guard, &message, size)
            .await
            .is_err()
        {
            write_simple(&mut ctx.writer, RESP_441_POSTING_FAILED).await?;
            return Ok(());
        }

        // Store article
        ctx.storage.store_article(&message).await?;
        write_simple(&mut ctx.writer, RESP_240_ARTICLE_RECEIVED).await?;
        Ok(())
    }
}

/// Read a message from the reader until dot termination.
async fn read_message<R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let mut msg = String::new();
    let mut line = String::new();

    loop {
        line.clear();
        reader.read_line(&mut line).await?;
        if line == ".\r\n" || line == ".\n" {
            break;
        }
        if line.starts_with("..") {
            msg.push_str(&line[1..]);
        } else {
            msg.push_str(&line);
        }
    }
    Ok(msg)
}

/// Validate an article for posting.
pub async fn validate_article(
    storage: &crate::storage::DynStorage,
    auth: &crate::auth::DynAuth,
    cfg: &crate::config::Config,
    article: &crate::Message,
    size: u64,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Check required headers
    let has_from = article
        .headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("From"));
    let has_subject = article
        .headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("Subject"));
    let newsgroups: Vec<String> = article
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("Newsgroups"))
        .map(|(_, v)| {
            v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if !has_from || !has_subject || newsgroups.is_empty() {
        return Err("missing required headers".into());
    }

    // Check size limit
    if let Some(max_size) = cfg.default_max_article_bytes {
        if size > max_size {
            return Err("article too large".into());
        }
    }

    // Check moderated groups
    let all_groups = storage.list_groups().await?;

    // Get all approved values and signatures
    let approved_values: Vec<String> = article
        .headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("Approved"))
        .map(|(_, v)| v.trim().to_string())
        .collect();

    let sig_headers: Vec<String> = article
        .headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("X-PGP-Sig"))
        .map(|(_, v)| v.clone())
        .collect();

    for group in &newsgroups {
        if !all_groups.contains(group) {
            return Err("group does not exist".into());
        }

        if storage.is_group_moderated(group).await? {
            // Find moderators for this specific group
            let mut group_moderators = Vec::new();
            let mut group_signatures = Vec::new();

            for (i, approved) in approved_values.iter().enumerate() {
                if auth.is_moderator(approved, group).await? {
                    group_moderators.push(approved.clone());
                    if let Some(sig) = sig_headers.get(i) {
                        group_signatures.push(sig.clone());
                    }
                }
            }

            if group_moderators.is_empty() {
                return Err("missing approval for moderated group".into());
            }

            if group_signatures.len() < group_moderators.len() {
                return Err("missing signature for moderator".into());
            }

            // Verify signatures for this group's moderators
            for (i, approved) in group_moderators.iter().enumerate() {
                let sig_header = group_signatures.get(i).ok_or("missing signature")?.clone();
                let mut words = sig_header.split_whitespace();
                let version = words.next().ok_or("bad signature")?;
                let signed = words.next().ok_or("bad signature")?;
                let sig_rest = words.collect::<Vec<_>>().join("\n");

                let mut tmp_headers: Vec<(String, String)> = article
                    .headers
                    .iter()
                    .filter(|(k, _)| !k.eq_ignore_ascii_case("Approved"))
                    .cloned()
                    .collect();
                tmp_headers.push(("Approved".to_string(), approved.clone()));

                let tmp_msg = crate::Message {
                    headers: tmp_headers,
                    body: article.body.clone(),
                };

                control::verify_pgp(&tmp_msg, auth, approved, version, signed, &sig_rest).await?;
            }
        }
    }

    Ok(())
}
