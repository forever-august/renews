//! Article validation filters
//!
//! This module provides a composable filter system for article validation.
//! Each filter implements the `ArticleFilter` trait and can be combined
//! into a validation chain that must all pass for an article to be accepted.

use crate::Message;
use crate::auth::DynAuth;
use crate::config::Config;
use crate::storage::DynStorage;
use std::error::Error;

pub mod factory;
pub mod groups;
pub mod header;
pub mod milter;
pub mod moderation;
pub mod size;

/// Trait for article validation filters
#[async_trait::async_trait]
pub trait ArticleFilter: Send + Sync {
    /// Validate an article according to this filter's rules
    ///
    /// Returns Ok(()) if the article passes validation, Err if it fails.
    async fn validate(
        &self,
        storage: &DynStorage,
        auth: &DynAuth,
        cfg: &Config,
        article: &Message,
        size: u64,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;

    /// Get a descriptive name for this filter (for logging/debugging)
    fn name(&self) -> &'static str;
}

/// A chain of filters that all must pass for validation to succeed
pub struct FilterChain {
    filters: Vec<Box<dyn ArticleFilter>>,
}

impl FilterChain {
    /// Create a new empty filter chain
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
        }
    }

    /// Add a filter to the chain
    pub fn add_filter(mut self, filter: Box<dyn ArticleFilter>) -> Self {
        self.filters.push(filter);
        self
    }

    /// Run all filters in the chain, returning on first failure
    pub async fn validate(
        &self,
        storage: &DynStorage,
        auth: &DynAuth,
        cfg: &Config,
        article: &Message,
        size: u64,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        for filter in &self.filters {
            filter.validate(storage, auth, cfg, article, size).await?;
        }
        Ok(())
    }

    /// Get a list of filter names in the chain
    pub fn filter_names(&self) -> Vec<&'static str> {
        self.filters.iter().map(|f| f.name()).collect()
    }
}

impl Default for FilterChain {
    /// Create the default filter chain with all standard validation filters
    fn default() -> Self {
        Self::new()
            .add_filter(Box::new(header::HeaderFilter))
            .add_filter(Box::new(size::SizeFilter))
            .add_filter(Box::new(groups::GroupExistenceFilter))
            .add_filter(Box::new(moderation::ModerationFilter))
    }
}
