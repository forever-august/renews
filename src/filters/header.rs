//! Header validation filter
//!
//! Validates that articles have required headers (From, Subject, Newsgroups).

use super::ArticleFilter;
use crate::Message;
use crate::auth::DynAuth;
use crate::config::Config;
use crate::handlers::utils::{extract_newsgroups, has_header};
use crate::storage::DynStorage;
use std::error::Error;

/// Filter that validates required article headers
pub struct HeaderFilter;

#[async_trait::async_trait]
impl ArticleFilter for HeaderFilter {
    async fn validate(
        &self,
        _storage: &DynStorage,
        _auth: &DynAuth,
        _cfg: &Config,
        article: &Message,
        _size: u64,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Check required headers
        let has_from = has_header(article, "From");
        let has_subject = has_header(article, "Subject");
        let newsgroups = extract_newsgroups(article);

        if !has_from || !has_subject || newsgroups.is_empty() {
            return Err("missing required headers".into());
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "HeaderFilter"
    }
}
