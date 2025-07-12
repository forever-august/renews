use super::{
    Message, Storage, StringStream, StringTimestampStream, U64Stream,
    common::{Headers, extract_message_id},
};
use async_stream::stream;
use async_trait::async_trait;
use futures_util::StreamExt;
use smallvec::SmallVec;
use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::{error::Error, str::FromStr};

// SQL schemas for SQLite storage
const MESSAGES_TABLE: &str = "CREATE TABLE IF NOT EXISTS messages (
        message_id TEXT PRIMARY KEY,
        headers TEXT,
        body TEXT,
        size INTEGER NOT NULL
    )";

const GROUP_ARTICLES_TABLE: &str = "CREATE TABLE IF NOT EXISTS group_articles (
        group_name TEXT,
        number INTEGER,
        message_id TEXT,
        inserted_at INTEGER NOT NULL,
        PRIMARY KEY(group_name, number),
        FOREIGN KEY(message_id) REFERENCES messages(message_id)
    )";

const GROUPS_TABLE: &str = "CREATE TABLE IF NOT EXISTS groups (
        name TEXT PRIMARY KEY,
        created_at INTEGER NOT NULL,
        moderated INTEGER NOT NULL DEFAULT 0
    )";

#[derive(Clone)]
pub struct SqliteStorage {
    pool: SqlitePool,
}

impl SqliteStorage {
    #[tracing::instrument(skip_all)]
    /// Create a new SQLite storage backend.
    ///
    /// # Errors
    ///
    /// Returns an error if the database connection fails or schema creation fails.
    pub async fn new(path: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let options = SqliteConnectOptions::from_str(path)?
            .create_if_missing(true);
        
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        // Create database schema
        sqlx::query(MESSAGES_TABLE).execute(&pool).await?;
        sqlx::query(GROUP_ARTICLES_TABLE).execute(&pool).await?;
        sqlx::query(GROUPS_TABLE).execute(&pool).await?;

        Ok(Self { pool })
    }
}

#[async_trait]
impl Storage for SqliteStorage {
    #[tracing::instrument(skip_all)]
    async fn store_article(&self, article: &Message) -> Result<(), Box<dyn Error + Send + Sync>> {
        let msg_id = extract_message_id(article).ok_or("missing Message-ID")?;
        let headers = serde_json::to_string(&Headers(article.headers.clone()))?;

        // Store the message once
        sqlx::query(
            "INSERT OR IGNORE INTO messages (message_id, headers, body, size) VALUES (?, ?, ?, ?)",
        )
        .bind(&msg_id)
        .bind(&headers)
        .bind(&article.body)
        .bind(i64::try_from(article.body.len()).unwrap_or(i64::MAX))
        .execute(&self.pool)
        .await?;

        // Extract newsgroups from headers
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

        // Associate with each group
        let now = chrono::Utc::now().timestamp();
        for group in newsgroups {
            let next: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(number),0)+1 FROM group_articles WHERE group_name = ?",
            )
            .bind(&group)
            .fetch_one(&self.pool)
            .await?;

            sqlx::query(
                "INSERT INTO group_articles (group_name, number, message_id, inserted_at) VALUES (?, ?, ?, ?)",
            )
            .bind(&group)
            .bind(next)
            .bind(&msg_id)
            .bind(now)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
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
        .bind(i64::try_from(number).unwrap_or(-1))
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
        if let Some(row) = sqlx::query("SELECT headers, body FROM messages WHERE message_id = ?")
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
        sqlx::query("INSERT OR IGNORE INTO groups (name, created_at, moderated) VALUES (?, ?, ?)")
            .bind(group)
            .bind(now)
            .bind(i32::from(moderated))
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
    async fn is_group_moderated(&self, group: &str) -> Result<bool, Box<dyn Error + Send + Sync>> {
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
    fn list_groups(&self) -> StringStream<'_> {
        let pool = self.pool.clone();
        Box::pin(stream! {
            let mut rows = sqlx::query("SELECT name FROM groups ORDER BY name")
                .fetch(&pool);

            while let Some(row) = rows.next().await {
                match row {
                    Ok(r) => match r.try_get::<String, _>("name") {
                        Ok(name) => yield Ok(name),
                        Err(e) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                    },
                    Err(e) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                }
            }
        })
    }

    #[tracing::instrument(skip_all)]
    fn list_groups_since(&self, since: chrono::DateTime<chrono::Utc>) -> StringStream<'_> {
        let pool = self.pool.clone();
        let timestamp = since.timestamp();
        Box::pin(stream! {
            let mut rows = sqlx::query("SELECT name FROM groups WHERE created_at > ? ORDER BY name")
                .bind(timestamp)
                .fetch(&pool);

            while let Some(row) = rows.next().await {
                match row {
                    Ok(r) => match r.try_get::<String, _>("name") {
                        Ok(name) => yield Ok(name),
                        Err(e) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                    },
                    Err(e) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                }
            }
        })
    }

    #[tracing::instrument(skip_all)]
    fn list_groups_with_times(&self) -> StringTimestampStream<'_> {
        let pool = self.pool.clone();
        Box::pin(stream! {
            let mut rows = sqlx::query("SELECT name, created_at FROM groups ORDER BY name")
                .fetch(&pool);

            while let Some(row) = rows.next().await {
                match row {
                    Ok(r) => {
                        match (r.try_get::<String, _>("name"), r.try_get::<i64, _>("created_at")) {
                            (Ok(name), Ok(ts)) => yield Ok((name, ts)),
                            (Err(e), _) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                            (_, Err(e)) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                        }
                    },
                    Err(e) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                }
            }
        })
    }

    #[tracing::instrument(skip_all)]
    fn list_article_numbers(&self, group: &str) -> U64Stream<'_> {
        let pool = self.pool.clone();
        let group = group.to_string();
        Box::pin(stream! {
            let mut rows = sqlx::query("SELECT number FROM group_articles WHERE group_name = ? ORDER BY number")
                .bind(&group)
                .fetch(&pool);

            while let Some(row) = rows.next().await {
                match row {
                    Ok(r) => match r.try_get::<i64, _>("number") {
                        Ok(number) => yield Ok(u64::try_from(number).unwrap_or(0)),
                        Err(e) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                    },
                    Err(e) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                }
            }
        })
    }

    #[tracing::instrument(skip_all)]
    fn list_article_ids(&self, group: &str) -> StringStream<'_> {
        let pool = self.pool.clone();
        let group = group.to_string();
        Box::pin(stream! {
            let mut rows = sqlx::query("SELECT message_id FROM group_articles WHERE group_name = ? ORDER BY number")
                .bind(&group)
                .fetch(&pool);

            while let Some(row) = rows.next().await {
                match row {
                    Ok(r) => match r.try_get::<String, _>("message_id") {
                        Ok(message_id) => yield Ok(message_id),
                        Err(e) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                    },
                    Err(e) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                }
            }
        })
    }

    #[tracing::instrument(skip_all)]
    fn list_article_ids_since(
        &self,
        group: &str,
        since: chrono::DateTime<chrono::Utc>,
    ) -> StringStream<'_> {
        let pool = self.pool.clone();
        let group = group.to_string();
        let timestamp = since.timestamp();
        Box::pin(stream! {
            let mut rows = sqlx::query("SELECT message_id FROM group_articles WHERE group_name = ? AND inserted_at > ? ORDER BY number")
                .bind(&group)
                .bind(timestamp)
                .fetch(&pool);

            while let Some(row) = rows.next().await {
                match row {
                    Ok(r) => match r.try_get::<String, _>("message_id") {
                        Ok(message_id) => yield Ok(message_id),
                        Err(e) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                    },
                    Err(e) => yield Err(Box::new(e) as Box<dyn Error + Send + Sync>),
                }
            }
        })
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
            Ok(Some(u64::try_from(size).unwrap_or(0)))
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
