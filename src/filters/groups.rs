//! Group existence validation filter
//!
//! Validates that all newsgroups in an article exist in the server.

use super::{ArticleFilter, FilterContext};
use crate::handlers::utils::extract_newsgroups;
use anyhow::Result;
use futures_util::TryStreamExt;

/// Filter that validates newsgroups exist in the server
pub struct GroupExistenceFilter;

#[async_trait::async_trait]
impl ArticleFilter for GroupExistenceFilter {
    async fn validate(&self, ctx: &FilterContext<'_>) -> Result<()> {
        // Get newsgroups from the article
        let newsgroups = extract_newsgroups(ctx.article);

        // Check that all groups exist
        let stream = ctx.storage.list_groups();
        let all_groups = stream.try_collect::<Vec<String>>().await?;
        for group in &newsgroups {
            if !all_groups.contains(group) {
                return Err(anyhow::anyhow!("group does not exist"));
            }
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "GroupExistenceFilter"
    }
}
