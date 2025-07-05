use crate::Message;
use async_trait::async_trait;
use std::error::Error;
use std::sync::Arc;

#[async_trait]
pub trait Storage: Send + Sync {
    /// Store `article` in `group` returning the assigned article number
    async fn store_article(
        &self,
        group: &str,
        article: &Message,
    ) -> Result<u64, Box<dyn Error + Send + Sync>>;

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
    async fn list_groups(&self) -> Result<Vec<String>, Box<dyn Error + Send + Sync>>;

    /// Retrieve newsgroups created after the specified time
    async fn list_groups_since(
        &self,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<String>, Box<dyn Error + Send + Sync>>;

    /// Retrieve all newsgroups with their creation timestamps
    async fn list_groups_with_times(
        &self,
    ) -> Result<Vec<(String, i64)>, Box<dyn Error + Send + Sync>>;

    /// List all article numbers for a group
    async fn list_article_numbers(
        &self,
        group: &str,
    ) -> Result<Vec<u64>, Box<dyn Error + Send + Sync>>;

    /// List all message-ids for a group
    async fn list_article_ids(
        &self,
        group: &str,
    ) -> Result<Vec<String>, Box<dyn Error + Send + Sync>>;

    /// List message-ids for a group added after the specified time
    async fn list_article_ids_since(
        &self,
        group: &str,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<String>, Box<dyn Error + Send + Sync>>;

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

pub mod sqlite {
    use super::*;
    use serde::{Deserialize, Serialize};
    use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};

    #[derive(Clone)]
    pub struct SqliteStorage {
        pool: SqlitePool,
    }

    #[derive(Serialize, Deserialize)]
    struct Headers(Vec<(String, String)>);

    impl SqliteStorage {
        #[tracing::instrument(skip_all)]
        pub async fn new(path: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
            let pool = SqlitePoolOptions::new()
                .max_connections(5)
                .connect(path)
                .await?;
            // table storing unique messages keyed by Message-ID
            sqlx::query(
                "CREATE TABLE IF NOT EXISTS messages (
                    message_id TEXT PRIMARY KEY,
                    headers TEXT,
                    body TEXT,
                    size INTEGER NOT NULL
                )",
            )
            .execute(&pool)
            .await?;
            // table mapping groups and numbers to message IDs
            sqlx::query(
                "CREATE TABLE IF NOT EXISTS group_articles (
                    group_name TEXT,
                    number INTEGER,
                    message_id TEXT,
                    inserted_at INTEGER NOT NULL,
                    PRIMARY KEY(group_name, number),
                    FOREIGN KEY(message_id) REFERENCES messages(message_id)
                )",
            )
            .execute(&pool)
            .await?;
            // table of available newsgroups with creation time
            sqlx::query(
                "CREATE TABLE IF NOT EXISTS groups (
                    name TEXT PRIMARY KEY,
                    created_at INTEGER NOT NULL,
                    moderated INTEGER NOT NULL DEFAULT 0
                )",
            )
            .execute(&pool)
            .await?;
            Ok(Self { pool })
        }

        fn message_id(article: &Message) -> Option<String> {
            article.headers.iter().find_map(|(k, v)| {
                if k.eq_ignore_ascii_case("Message-ID") {
                    Some(v.clone())
                } else {
                    None
                }
            })
        }
    }

    #[async_trait]
    impl Storage for SqliteStorage {
        #[tracing::instrument(skip_all)]
        async fn store_article(
            &self,
            group: &str,
            article: &Message,
        ) -> Result<u64, Box<dyn Error + Send + Sync>> {
            let msg_id = Self::message_id(article).ok_or("missing Message-ID")?;
            let headers = serde_json::to_string(&Headers(article.headers.clone()))?;
            sqlx::query(
                "INSERT OR IGNORE INTO messages (message_id, headers, body, size) VALUES (?, ?, ?, ?)",
            )
            .bind(&msg_id)
            .bind(&headers)
            .bind(&article.body)
            .bind(article.body.len() as i64)
            .execute(&self.pool)
            .await?;
            let next: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(number),0)+1 FROM group_articles WHERE group_name = ?",
            )
            .bind(group)
            .fetch_one(&self.pool)
            .await?;
            let now = chrono::Utc::now().timestamp();
            sqlx::query(
                "INSERT INTO group_articles (group_name, number, message_id, inserted_at) VALUES (?, ?, ?, ?)",
            )
            .bind(group)
            .bind(next)
            .bind(&msg_id)
            .bind(now)
            .execute(&self.pool)
            .await?;
            Ok(next as u64)
        }

        #[tracing::instrument(skip_all)]
        async fn get_article_by_number(
            &self,
            group: &str,
            number: u64,
        ) -> Result<Option<Message>, Box<dyn Error + Send + Sync>> {
            if let Some(row) = sqlx::query(
                "SELECT m.headers, m.body FROM messages m \
                 JOIN group_articles g ON m.message_id = g.message_id \
                 WHERE g.group_name = ? AND g.number = ?",
            )
            .bind(group)
            .bind(number as i64)
            .fetch_optional(&self.pool)
            .await?
            {
                let headers_str: String = row.try_get("headers")?;
                let body: String = row.try_get("body")?;
                let Headers(headers) = serde_json::from_str(&headers_str)?;
                Ok(Some(Message { headers, body }))
            } else {
                Ok(None)
            }
        }

        #[tracing::instrument(skip_all)]
        async fn get_article_by_id(
            &self,
            message_id: &str,
        ) -> Result<Option<Message>, Box<dyn Error + Send + Sync>> {
            if let Some(row) =
                sqlx::query("SELECT headers, body FROM messages WHERE message_id = ?")
                    .bind(message_id)
                    .fetch_optional(&self.pool)
                    .await?
            {
                let headers_str: String = row.try_get("headers")?;
                let body: String = row.try_get("body")?;
                let Headers(headers) = serde_json::from_str(&headers_str)?;
                Ok(Some(Message { headers, body }))
            } else {
                Ok(None)
            }
        }

        #[tracing::instrument(skip_all)]
        async fn add_group(
            &self,
            group: &str,
            moderated: bool,
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            let now = chrono::Utc::now().timestamp();
            sqlx::query(
                "INSERT OR IGNORE INTO groups (name, created_at, moderated) VALUES (?, ?, ?)",
            )
            .bind(group)
            .bind(now)
            .bind(if moderated { 1 } else { 0 })
            .execute(&self.pool)
            .await?;
            Ok(())
        }

        #[tracing::instrument(skip_all)]
        async fn remove_group(&self, group: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
            sqlx::query("DELETE FROM group_articles WHERE group_name = ?")
                .bind(group)
                .execute(&self.pool)
                .await?;
            sqlx::query("DELETE FROM groups WHERE name = ?")
                .bind(group)
                .execute(&self.pool)
                .await?;
            sqlx::query(
                "DELETE FROM messages WHERE message_id NOT IN (SELECT DISTINCT message_id FROM group_articles)"
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        }

        #[tracing::instrument(skip_all)]
        async fn is_group_moderated(
            &self,
            group: &str,
        ) -> Result<bool, Box<dyn Error + Send + Sync>> {
            let row = sqlx::query("SELECT moderated FROM groups WHERE name = ?")
                .bind(group)
                .fetch_optional(&self.pool)
                .await?;
            if let Some(r) = row {
                let m: i64 = r.try_get("moderated")?;
                Ok(m != 0)
            } else {
                Ok(false)
            }
        }

        #[tracing::instrument(skip_all)]
        async fn list_groups(&self) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
            let rows = sqlx::query("SELECT name FROM groups ORDER BY name")
                .fetch_all(&self.pool)
                .await?;
            let groups = rows
                .into_iter()
                .map(|row| row.try_get::<String, _>("name").unwrap())
                .collect();
            Ok(groups)
        }

        #[tracing::instrument(skip_all)]
        async fn list_groups_since(
            &self,
            since: chrono::DateTime<chrono::Utc>,
        ) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
            let rows = sqlx::query("SELECT name FROM groups WHERE created_at > ? ORDER BY name")
                .bind(since.timestamp())
                .fetch_all(&self.pool)
                .await?;
            Ok(rows
                .into_iter()
                .map(|r| r.try_get::<String, _>("name").unwrap())
                .collect())
        }

        #[tracing::instrument(skip_all)]
        async fn list_groups_with_times(
            &self,
        ) -> Result<Vec<(String, i64)>, Box<dyn Error + Send + Sync>> {
            let rows = sqlx::query("SELECT name, created_at FROM groups ORDER BY name")
                .fetch_all(&self.pool)
                .await?;
            Ok(rows
                .into_iter()
                .map(|r| {
                    let name = r.try_get::<String, _>("name").unwrap();
                    let ts = r.try_get::<i64, _>("created_at").unwrap();
                    (name, ts)
                })
                .collect())
        }

        #[tracing::instrument(skip_all)]
        async fn list_article_numbers(
            &self,
            group: &str,
        ) -> Result<Vec<u64>, Box<dyn Error + Send + Sync>> {
            let rows = sqlx::query(
                "SELECT number FROM group_articles WHERE group_name = ? ORDER BY number",
            )
            .bind(group)
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| r.try_get::<i64, _>("number").unwrap() as u64)
                .collect())
        }

        #[tracing::instrument(skip_all)]
        async fn list_article_ids(
            &self,
            group: &str,
        ) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
            let rows = sqlx::query(
                "SELECT message_id FROM group_articles WHERE group_name = ? ORDER BY number",
            )
            .bind(group)
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| r.try_get::<String, _>("message_id").unwrap())
                .collect())
        }

        #[tracing::instrument(skip_all)]
        async fn list_article_ids_since(
            &self,
            group: &str,
            since: chrono::DateTime<chrono::Utc>,
        ) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
            let rows = sqlx::query(
                "SELECT message_id FROM group_articles WHERE group_name = ? AND inserted_at > ? ORDER BY number",
            )
            .bind(group)
            .bind(since.timestamp())
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| r.try_get::<String, _>("message_id").unwrap())
                .collect())
        }

        #[tracing::instrument(skip_all)]
        async fn purge_group_before(
            &self,
            group: &str,
            before: chrono::DateTime<chrono::Utc>,
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            sqlx::query("DELETE FROM group_articles WHERE group_name = ? AND inserted_at < ?")
                .bind(group)
                .bind(before.timestamp())
                .execute(&self.pool)
                .await?;
            Ok(())
        }

        #[tracing::instrument(skip_all)]
        async fn purge_orphan_messages(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
            sqlx::query(
                "DELETE FROM messages WHERE message_id NOT IN (SELECT DISTINCT message_id FROM group_articles)"
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        }

        #[tracing::instrument(skip_all)]
        async fn get_message_size(
            &self,
            message_id: &str,
        ) -> Result<Option<u64>, Box<dyn Error + Send + Sync>> {
            if let Some(row) = sqlx::query("SELECT size FROM messages WHERE message_id = ?")
                .bind(message_id)
                .fetch_optional(&self.pool)
                .await?
            {
                let size: i64 = row.try_get("size")?;
                Ok(Some(size as u64))
            } else {
                Ok(None)
            }
        }

        async fn delete_article_by_id(
            &self,
            message_id: &str,
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            sqlx::query("DELETE FROM group_articles WHERE message_id = ?")
                .bind(message_id)
                .execute(&self.pool)
                .await?;
            sqlx::query(
                "DELETE FROM messages WHERE message_id = ? AND NOT EXISTS (SELECT 1 FROM group_articles WHERE message_id = ?)",
            )
            .bind(message_id)
            .bind(message_id)
            .execute(&self.pool)
            .await?;
            Ok(())
        }
    }
}
