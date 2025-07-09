//! Moderation validation filter
//!
//! Validates moderated group approval and PGP signatures.

use super::ArticleFilter;
use crate::auth::DynAuth;
use crate::config::Config;
use crate::storage::DynStorage;
use crate::Message;
use std::error::Error;

/// Filter that validates moderated group requirements
pub struct ModerationFilter;

#[async_trait::async_trait]
impl ArticleFilter for ModerationFilter {
    async fn validate(
        &self,
        storage: &DynStorage,
        auth: &DynAuth,
        _cfg: &Config,
        article: &Message,
        _size: u64,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Get newsgroups from the article
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

        // Check each newsgroup for moderation requirements
        for group in &newsgroups {
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

                    crate::control::verify_pgp(&tmp_msg, auth, approved, version, signed, &sig_rest).await?;
                }
            }
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "ModerationFilter"
    }
}