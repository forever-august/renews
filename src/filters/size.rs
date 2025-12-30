//! Size validation filter
//!
//! Validates that articles are within configured size limits.

use super::{ArticleFilter, FilterContext};
use crate::handlers::utils::extract_newsgroups;
use anyhow::Result;

/// Filter that validates article size limits
pub struct SizeFilter;

#[async_trait::async_trait]
impl ArticleFilter for SizeFilter {
    async fn validate(&self, ctx: &FilterContext<'_>) -> Result<()> {
        // Extract newsgroups from the article
        let newsgroups = extract_newsgroups(ctx.article);

        // Check size limit for each newsgroup
        for group in &newsgroups {
            if let Some(max_size) = ctx.cfg.max_size_for_group(group)
                && ctx.size > max_size
            {
                return Err(anyhow::anyhow!("article too large for group {group}"));
            }
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "SizeFilter"
    }
}
