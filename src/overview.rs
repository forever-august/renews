//! Overview format handling for NNTP OVER and XOVER commands.
//!
//! This module provides centralized configuration for overview information
//! as specified in RFC2980 and RFC3977.

use crate::Message;
use crate::handlers::utils::{extract_message_id, get_header_value};
use anyhow::Result;

/// Standard overview format fields as defined in RFC2980.
/// This determines the order and content of fields returned by OVER/XOVER commands
/// and the LIST OVERVIEW.FMT command.
pub const OVERVIEW_FORMAT: &[&str] = &[
    "Subject:",
    "From:",
    "Date:",
    "Message-ID:",
    "References:",
    ":bytes",
    ":lines",
];

/// Generate overview line for an article according to the standard format.
/// Returns a tab-separated line with article number and overview fields.
pub async fn generate_overview_line(
    storage: &dyn crate::storage::Storage,
    article_number: u64,
    article: &Message,
) -> Result<String> {
    let subject = get_header_value(article, "Subject").unwrap_or_default();
    let from = get_header_value(article, "From").unwrap_or_default();
    let date = get_header_value(article, "Date").unwrap_or_default();
    let msgid = get_header_value(article, "Message-ID").unwrap_or_default();
    let refs = get_header_value(article, "References").unwrap_or_default();

    let bytes = if let Some(id) = extract_message_id(article) {
        storage
            .get_message_size(&id)
            .await?
            .unwrap_or(article.body.len() as u64)
    } else {
        article.body.len() as u64
    };

    let lines = article.body.lines().count();

    Ok(format!(
        "{article_number}\t{subject}\t{from}\t{date}\t{msgid}\t{refs}\t{bytes}\t{lines}"
    ))
}

/// Get the overview format fields for LIST OVERVIEW.FMT command.
pub fn get_overview_format_lines() -> Vec<String> {
    OVERVIEW_FORMAT
        .iter()
        .map(|&s| format!("{s}\r\n"))
        .collect()
}
