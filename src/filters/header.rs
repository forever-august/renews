//! Header validation filter
//!
//! Validates that articles have required headers (From, Subject, Newsgroups).

use super::ArticleFilter;
use crate::Message;
use crate::auth::DynAuth;
use crate::config::Config;
use crate::storage::DynStorage;
use smallvec::SmallVec;
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
        let has_from = article
            .headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("From"));
        let has_subject = article
            .headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("Subject"));
        let newsgroups: SmallVec<[String; 4]> = article
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Newsgroups"))
            .map(|(_, v)| {
                v.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(std::string::ToString::to_string)
                    .collect::<SmallVec<[String; 4]>>()
            })
            .unwrap_or_default();

        if !has_from || !has_subject || newsgroups.is_empty() {
            return Err("missing required headers".into());
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "HeaderFilter"
    }
}
