//! Size validation filter
//!
//! Validates that articles are within configured size limits.

use super::ArticleFilter;
use crate::handlers::utils::extract_newsgroups;
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
        let newsgroups = extract_newsgroups(article);

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
