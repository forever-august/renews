//! Group existence validation filter
//!
//! Validates that all newsgroups in an article exist in the server.

use super::ArticleFilter;
use crate::Message;
use crate::auth::DynAuth;
use crate::config::Config;
use crate::handlers::utils::extract_newsgroups;
use crate::storage::DynStorage;
use futures_util::TryStreamExt;
use std::error::Error;

/// Filter that validates newsgroups exist in the server
pub struct GroupExistenceFilter;

#[async_trait::async_trait]
impl ArticleFilter for GroupExistenceFilter {
    async fn validate(
        &self,
        storage: &DynStorage,
        _auth: &DynAuth,
        _cfg: &Config,
        article: &Message,
        _size: u64,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Get newsgroups from the article
        let newsgroups = extract_newsgroups(article);

        // Check that all groups exist
        let stream = storage.list_groups();
        let all_groups = stream.try_collect::<Vec<String>>().await?;
        for group in &newsgroups {
            if !all_groups.contains(group) {
                return Err("group does not exist".into());
            }
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "GroupExistenceFilter"
    }
}
