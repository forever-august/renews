use crate::Message;
use async_trait::async_trait;
use futures_core::Stream;
use std::error::Error;
use std::pin::Pin;
use std::sync::Arc;

// Type aliases for complex stream return types
type StringStream<'a> = Pin<Box<dyn Stream<Item = Result<String, Box<dyn Error + Send + Sync>>> + Send + 'a>>;
type U64Stream<'a> = Pin<Box<dyn Stream<Item = Result<u64, Box<dyn Error + Send + Sync>>> + Send + 'a>>;
type StringTimestampStream<'a> = Pin<Box<dyn Stream<Item = Result<(String, i64), Box<dyn Error + Send + Sync>>> + Send + 'a>>;

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
    fn list_groups(&self) -> StringStream<'_>;

    /// Retrieve newsgroups created after the specified time
    fn list_groups_since(
        &self,
        since: chrono::DateTime<chrono::Utc>,
    ) -> StringStream<'_>;

    /// Retrieve all newsgroups with their creation timestamps
    fn list_groups_with_times(&self) -> StringTimestampStream<'_>;

    /// List all article numbers for a group
    fn list_article_numbers(&self, group: &str) -> U64Stream<'_>;

    /// List all message-ids for a group
    fn list_article_ids(&self, group: &str) -> StringStream<'_>;

    /// List message-ids for a group added after the specified time
    fn list_article_ids_since(
        &self,
        group: &str,
        since: chrono::DateTime<chrono::Utc>,
    ) -> StringStream<'_>;

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

    /// Check if a group exists.
    async fn group_exists(&self, group: &str) -> Result<bool, Box<dyn Error + Send + Sync>>;
}

pub type DynStorage = Arc<dyn Storage>;

pub mod common;
#[cfg(feature = "postgres")]
pub mod postgres;
pub mod sqlite;

/// Create a storage backend from a connection URI.
pub async fn open(uri: &str) -> Result<DynStorage, Box<dyn Error + Send + Sync>> {
    if uri.starts_with("sqlite:") {
        sqlite::SqliteStorage::new(uri).await
            .map(|s| Arc::new(s) as DynStorage)
            .map_err(|e| {
                format!(
                    "Failed to connect to SQLite database '{}': {}

Common SQLite connection issues:
- Directory does not exist (SQLite will create the file but not directories)
- Permission denied accessing the database file or directory
- Database file is corrupted
- Path contains invalid characters
- Database is locked by another process

For SQLite URIs:
- Use format: sqlite:///path/to/database.db
- For in-memory database: sqlite::memory:
- Relative paths are relative to the working directory

You can change the database path in your configuration file using the 'db_path' setting.",
                    uri, e
                ).into()
            })
    } else if uri.starts_with("postgres:") {
        #[cfg(feature = "postgres")]
        {
            postgres::PostgresStorage::new(uri).await
                .map(|s| Arc::new(s) as DynStorage)
                .map_err(|e| {
                    format!(
                        "Failed to connect to PostgreSQL database '{}': {}

Common PostgreSQL connection issues:
- PostgreSQL server is not running
- Incorrect hostname, port, database name, username, or password
- Database does not exist (must be created manually)
- Network connectivity issues
- Authentication method not supported
- Connection limit exceeded
- SSL/TLS configuration issues

For PostgreSQL URIs, use format:
postgres://username:password@host:port/database

You can change the database URI in your configuration file using the 'db_path' setting.",
                        uri, e
                    ).into()
                })
        }
        #[cfg(not(feature = "postgres"))]
        {
            Err(format!(
                "PostgreSQL backend not enabled: '{}'

The renews server was compiled without PostgreSQL support.
To use PostgreSQL:
1. Rebuild with: cargo build --features postgres
2. Or use SQLite instead by changing 'db_path' to a sqlite:// URI in your configuration",
                uri
            ).into())
        }
    } else {
        Err(format!(
            "Unknown storage backend: '{}'

Supported database backends:
- SQLite: sqlite:///path/to/database.db
- PostgreSQL: postgres://user:pass@host:port/database (requires --features postgres)

You can change the database URI in your configuration file using the 'db_path' setting.",
            uri
        ).into())
    }
}
