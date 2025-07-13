//! Moderation validation filter
//!
//! Validates moderated group approval and PGP signatures.

use super::ArticleFilter;
use crate::Message;
use crate::auth::DynAuth;
use crate::config::Config;
use crate::handlers::utils::{extract_newsgroups, get_header_values};
use crate::storage::DynStorage;
use smallvec::SmallVec;
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
        let newsgroups = extract_newsgroups(article);

        // Get all approved values and signatures
        let approved_values = get_header_values(article, "Approved");
        let sig_headers = get_header_values(article, "X-PGP-Sig");

        // Check each newsgroup for moderation requirements
        for group in &newsgroups {
            if storage.is_group_moderated(group).await? {
                // Find moderators for this specific group
                let mut group_moderators = SmallVec::<[String; 2]>::new();
                let mut group_signatures = SmallVec::<[String; 2]>::new();

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

                    let mut tmp_headers: SmallVec<[(String, String); 8]> = article
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

                    crate::control::verify_pgp(
                        &tmp_msg,
                        auth,
                        approved,
                        version,
                        signed,
                        &sig_rest,
                        &_cfg.pgp_key_servers,
                    )
                    .await?;
                }
            }
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "ModerationFilter"
    }
}
