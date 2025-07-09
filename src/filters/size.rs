//! Size validation filter
//!
//! Validates that articles are within configured size limits.

use super::ArticleFilter;
use crate::auth::DynAuth;
use crate::config::Config;
use crate::storage::DynStorage;
use crate::Message;
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
        _article: &Message,
        size: u64,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Check size limit
        if let Some(max_size) = cfg.default_max_article_bytes {
            if size > max_size {
                return Err("article too large".into());
            }
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "SizeFilter"
    }
}