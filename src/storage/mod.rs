use crate::Message;
use async_trait::async_trait;
use futures_core::Stream;
use std::error::Error;
use std::pin::Pin;
use std::sync::Arc;

// Type aliases for complex stream types to improve readability
type StringStreamResult<'a> = Pin<Box<dyn Stream<Item = Result<String, Box<dyn Error + Send + Sync>>> + Send + 'a>>;
type GroupTimeStreamResult<'a> = Pin<Box<dyn Stream<Item = Result<(String, i64), Box<dyn Error + Send + Sync>>> + Send + 'a>>;
type NumberStreamResult<'a> = Pin<Box<dyn Stream<Item = Result<u64, Box<dyn Error + Send + Sync>>> + Send + 'a>>;

#[async_trait]
pub trait Storage: Send + Sync {
    /// Store `article` and associate it with all groups specified in the Newsgroups header
    async fn store_article(&self, article: &Message) -> Result<(), Box<dyn Error + Send + Sync>>;

    /// Retrieve an article by group name and article number
    async fn get_article_by_number(
        &self,
        group: &str,
        number: u64,
    ) -> Result<Option<Message>, Box<dyn Error + Send + Sync>>;

    /// Retrieve an article by its Message-ID header
    async fn get_article_by_id(
        &self,
        message_id: &str,
    ) -> Result<Option<Message>, Box<dyn Error + Send + Sync>>;

    /// Add a newsgroup to the server's list. When `moderated` is true the group
    /// requires an `Approved` header on posted articles.
    async fn add_group(
        &self,
        group: &str,
        moderated: bool,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;

    /// Remove a newsgroup from the server's list
    async fn remove_group(&self, group: &str) -> Result<(), Box<dyn Error + Send + Sync>>;

    /// Retrieve all newsgroups carried by the server
    fn list_groups(
        &self,
    ) -> StringStreamResult<'_>;

    /// Retrieve newsgroups created after the specified time
    fn list_groups_since(
        &self,
        since: chrono::DateTime<chrono::Utc>,
    ) -> StringStreamResult<'_>;

    /// Retrieve all newsgroups with their creation timestamps
    fn list_groups_with_times(
        &self,
    ) -> GroupTimeStreamResult<'_>;

    /// List all article numbers for a group
    fn list_article_numbers(
        &self,
        group: &str,
    ) -> NumberStreamResult<'_>;

    /// List all message-ids for a group
    fn list_article_ids(
        &self,
        group: &str,
    ) -> StringStreamResult<'_>;

    /// List message-ids for a group added after the specified time
    fn list_article_ids_since(
        &self,
        group: &str,
        since: chrono::DateTime<chrono::Utc>,
    ) -> StringStreamResult<'_>;

    /// Remove articles in `group` that were inserted before `before`
    async fn purge_group_before(
        &self,
        group: &str,
        before: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;

    /// Delete any messages no longer referenced by any group
    async fn purge_orphan_messages(&self) -> Result<(), Box<dyn Error + Send + Sync>>;

    /// Retrieve the stored size in bytes of a message by its Message-ID
    async fn get_message_size(
        &self,
        message_id: &str,
    ) -> Result<Option<u64>, Box<dyn Error + Send + Sync>>;

    /// Delete an article by Message-ID from all groups
    async fn delete_article_by_id(
        &self,
        message_id: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;

    /// Check if a group is moderated.
    async fn is_group_moderated(&self, group: &str) -> Result<bool, Box<dyn Error + Send + Sync>>;
}

pub type DynStorage = Arc<dyn Storage>;

pub mod common;
#[cfg(feature = "postgres")]
pub mod postgres;
pub mod sqlite;

/// Create a storage backend from a connection URI.
pub async fn open(uri: &str) -> Result<DynStorage, Box<dyn Error + Send + Sync>> {
    if uri.starts_with("sqlite:") {
        Ok(Arc::new(sqlite::SqliteStorage::new(uri).await?))
    } else if uri.starts_with("postgres:") {
        #[cfg(feature = "postgres")]
        {
            Ok(Arc::new(postgres::PostgresStorage::new(uri).await?))
        }
        #[cfg(not(feature = "postgres"))]
        {
            Err("postgres backend not enabled".into())
        }
    } else {
        Err("unknown storage backend".into())
    }
}
