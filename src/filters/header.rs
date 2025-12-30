//! Header validation filter
//!
//! Validates that articles have required headers (From, Subject, Newsgroups).

use super::{ArticleFilter, FilterContext};
use crate::handlers::utils::{extract_newsgroups, has_header};
use anyhow::Result;

/// Filter that validates required article headers
pub struct HeaderFilter;

#[async_trait::async_trait]
impl ArticleFilter for HeaderFilter {
    async fn validate(&self, ctx: &FilterContext<'_>) -> Result<()> {
        // Check required headers
        let has_from = has_header(ctx.article, "From");
        let has_subject = has_header(ctx.article, "Subject");
        let newsgroups = extract_newsgroups(ctx.article);

        if !has_from || !has_subject || newsgroups.is_empty() {
            return Err(anyhow::anyhow!("missing required headers"));
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "HeaderFilter"
    }
}
