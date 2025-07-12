//! Size validation filter
//!
//! Validates that articles are within configured size limits.

use super::ArticleFilter;
use crate::Message;
use crate::auth::DynAuth;
use crate::config::Config;
use crate::storage::DynStorage;
use std::error::Error;

/// Filter that validates article size limits
pub struct SizeFilter;

#[async_trait::async_trait]
impl ArticleFilter for SizeFilter {
    async fn validate(
        &self,
        _storage: &DynStorage,
        _auth: &DynAuth,
        cfg: &Config,
        article: &Message,
        size: u64,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Extract newsgroups from the article
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

        // Check size limit for each newsgroup
        for group in &newsgroups {
            if let Some(max_size) = cfg.max_size_for_group(group) {
                if size > max_size {
                    return Err(format!("article too large for group {group}").into());
                }
            }
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "SizeFilter"
    }
}
