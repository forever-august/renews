//! Example of how to add custom filters to the filter system
//!
//! This example demonstrates creating a custom filter and using it
//! in a customized filter chain.

use renews::Message;
use renews::auth::DynAuth;
use renews::config::Config;
use renews::filters::{ArticleFilter, FilterChain};
use renews::storage::DynStorage;
use std::error::Error;

/// Example custom filter that blocks articles with certain words in the subject
pub struct ProfanityFilter {
    blocked_words: Vec<String>,
}

impl ProfanityFilter {
    pub fn new(blocked_words: Vec<String>) -> Self {
        Self { blocked_words }
    }
}

#[async_trait::async_trait]
impl ArticleFilter for ProfanityFilter {
    async fn validate(
        &self,
        _storage: &DynStorage,
        _auth: &DynAuth,
        _cfg: &Config,
        article: &Message,
        _size: u64,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Find the subject header
        if let Some((_name, subject)) = article
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Subject"))
        {
            let subject_lower = subject.to_lowercase();
            for blocked_word in &self.blocked_words {
                if subject_lower.contains(&blocked_word.to_lowercase()) {
                    return Err(format!("Subject contains blocked word: {blocked_word}").into());
                }
            }
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProfanityFilter"
    }
}

/// Example custom filter that requires a specific header to be present
pub struct RequiredHeaderFilter {
    required_header: String,
}

impl RequiredHeaderFilter {
    pub fn new(required_header: String) -> Self {
        Self { required_header }
    }
}

#[async_trait::async_trait]
impl ArticleFilter for RequiredHeaderFilter {
    async fn validate(
        &self,
        _storage: &DynStorage,
        _auth: &DynAuth,
        _cfg: &Config,
        article: &Message,
        _size: u64,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let has_header = article
            .headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case(&self.required_header));

        if !has_header {
            return Err(format!("Missing required header: {}", self.required_header).into());
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "RequiredHeaderFilter"
    }
}

/// Example of creating a custom filter chain with additional filters
pub fn create_custom_filter_chain() -> FilterChain {
    FilterChain::new()
        // Start with built-in filters
        .add_filter(Box::new(renews::filters::header::HeaderFilter))
        .add_filter(Box::new(renews::filters::size::SizeFilter))
        // Add custom filters
        .add_filter(Box::new(ProfanityFilter::new(vec![
            "spam".to_string(),
            "scam".to_string(),
        ])))
        .add_filter(Box::new(RequiredHeaderFilter::new(
            "X-Site-Policy".to_string(),
        )))
        // Continue with remaining built-in filters
        .add_filter(Box::new(renews::filters::groups::GroupExistenceFilter))
        .add_filter(Box::new(renews::filters::moderation::ModerationFilter))
}

/// Example of using the custom filter chain
pub async fn example_usage() -> Result<(), Box<dyn Error + Send + Sync>> {
    // Create example dependencies (these would be your actual storage/auth/config in real usage)
    let storage = create_example_storage().await;
    let auth = create_example_auth().await;
    let config = create_example_config();

    // Create a custom filter chain
    let filter_chain = create_custom_filter_chain();

    // Example article that should fail profanity filter
    let bad_article = Message {
        headers: vec![
            ("From".to_string(), "user@example.com".to_string()),
            ("Subject".to_string(), "This is spam content".to_string()),
            ("Newsgroups".to_string(), "alt.test".to_string()),
        ],
        body: "Article body".to_string(),
    };

    // This should fail validation
    match filter_chain
        .validate(&storage, &auth, &config, &bad_article, 100)
        .await
    {
        Ok(()) => println!("Article passed validation (unexpected)"),
        Err(e) => println!("Article failed validation: {e}"),
    }

    // Example article that should pass
    let good_article = Message {
        headers: vec![
            ("From".to_string(), "user@example.com".to_string()),
            ("Subject".to_string(), "This is a good article".to_string()),
            ("Newsgroups".to_string(), "alt.test".to_string()),
            ("X-Site-Policy".to_string(), "accepted".to_string()),
        ],
        body: "Article body".to_string(),
    };

    // This should pass validation (assuming the group exists)
    match filter_chain
        .validate(&storage, &auth, &config, &good_article, 100)
        .await
    {
        Ok(()) => println!("Article passed validation"),
        Err(e) => println!("Article failed validation: {e}"),
    }

    Ok(())
}

// Helper functions for example (you'd use your actual implementations)
async fn create_example_storage() -> DynStorage {
    use renews::storage::sqlite::SqliteStorage;
    let storage = SqliteStorage::new(":memory:").await.unwrap();
    std::sync::Arc::new(storage)
}

async fn create_example_auth() -> DynAuth {
    use renews::auth::sqlite::SqliteAuth;
    let auth = SqliteAuth::new(":memory:").await.unwrap();
    std::sync::Arc::new(auth)
}

fn create_example_config() -> Config {
    let toml = r#"
addr = ":119"
db_path = "sqlite:///:memory:"
auth_db_path = "sqlite:///:memory:"
peer_db_path = "sqlite:///:memory:"
idle_timeout_secs = 600
article_queue_capacity = 100
article_worker_count = 1
site_name = "test.local"
"#;
    toml::from_str(toml).unwrap()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    println!("Running custom filters example...");
    example_usage().await?;
    println!("Example completed successfully!");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_profanity_filter() {
        let filter = ProfanityFilter::new(vec!["spam".to_string()]);
        let storage = create_example_storage().await;
        let auth = create_example_auth().await;
        let config = create_example_config();

        let article = Message {
            headers: vec![("Subject".to_string(), "This is spam".to_string())],
            body: "Body".to_string(),
        };

        let result = filter
            .validate(&storage, &auth, &config, &article, 100)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("spam"));
    }

    #[tokio::test]
    async fn test_required_header_filter() {
        let filter = RequiredHeaderFilter::new("X-Custom".to_string());
        let storage = create_example_storage().await;
        let auth = create_example_auth().await;
        let config = create_example_config();

        let article_without_header = Message {
            headers: vec![("Subject".to_string(), "Test".to_string())],
            body: "Body".to_string(),
        };

        let result = filter
            .validate(&storage, &auth, &config, &article_without_header, 100)
            .await;
        assert!(result.is_err());

        let article_with_header = Message {
            headers: vec![
                ("Subject".to_string(), "Test".to_string()),
                ("X-Custom".to_string(), "value".to_string()),
            ],
            body: "Body".to_string(),
        };

        let result = filter
            .validate(&storage, &auth, &config, &article_with_header, 100)
            .await;
        assert!(result.is_ok());
    }
}
