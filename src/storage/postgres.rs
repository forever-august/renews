use super::{
    Message, Storage,
    common::{Headers, extract_message_id},
};
use async_trait::async_trait;
use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::error::Error;
use std::str::FromStr;

// SQL schemas for PostgreSQL storage
const MESSAGES_TABLE: &str = "CREATE TABLE IF NOT EXISTS messages (
        message_id TEXT PRIMARY KEY,
        headers TEXT,
        body TEXT,
        size BIGINT NOT NULL
    )";

const GROUP_ARTICLES_TABLE: &str = "CREATE TABLE IF NOT EXISTS group_articles (
        group_name TEXT,
        number BIGINT,
        message_id TEXT,
        inserted_at BIGINT NOT NULL,
        PRIMARY KEY(group_name, number),
        FOREIGN KEY(message_id) REFERENCES messages(message_id)
    )";

const GROUPS_TABLE: &str = "CREATE TABLE IF NOT EXISTS groups (
        name TEXT PRIMARY KEY,
        created_at BIGINT NOT NULL,
        moderated BOOLEAN NOT NULL DEFAULT FALSE
    )";

#[derive(Clone)]
pub struct PostgresStorage {
    pool: PgPool,
}

impl PostgresStorage {
    #[tracing::instrument(skip_all)]
    /// Create a new Postgres storage backend.
    pub async fn new(uri: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let opts = PgConnectOptions::from_str(uri)?;
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await?;

        // Create database schema
        sqlx::query(MESSAGES_TABLE).execute(&pool).await?;
        sqlx::query(GROUP_ARTICLES_TABLE).execute(&pool).await?;
        sqlx::query(GROUPS_TABLE).execute(&pool).await?;

        Ok(Self { pool })
    }
}

#[async_trait]
impl Storage for PostgresStorage {
    #[tracing::instrument(skip_all)]
    async fn store_article(&self, article: &Message) -> Result<(), Box<dyn Error + Send + Sync>> {
        let msg_id = extract_message_id(article).ok_or("missing Message-ID")?;
        let headers = serde_json::to_string(&Headers(article.headers.clone()))?;

        // Store the message once
        sqlx::query(
            "INSERT INTO messages (message_id, headers, body, size) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
        )
        .bind(&msg_id)
        .bind(&headers)
        .bind(&article.body)
        .bind(i64::try_from(article.body.len()).unwrap_or(i64::MAX))
        .execute(&self.pool)
        .await?;

        // Extract newsgroups from headers
        let newsgroups: Vec<String> = article
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Newsgroups"))
            .map(|(_, v)| {
                v.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        // Associate with each group
        let now = chrono::Utc::now().timestamp();
        for group in newsgroups {
            let next: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(number),0)+1 FROM group_articles WHERE group_name = $1",
            )
            .bind(&group)
            .fetch_one(&self.pool)
            .await?;

            sqlx::query(
                "INSERT INTO group_articles (group_name, number, message_id, inserted_at) VALUES ($1, $2, $3, $4)",
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
            "SELECT m.headers, m.body FROM messages m JOIN group_articles g ON m.message_id = g.message_id WHERE g.group_name = $1 AND g.number = $2",
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
        if let Some(row) = sqlx::query("SELECT headers, body FROM messages WHERE message_id = $1")
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
            "INSERT INTO groups (name, created_at, moderated) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(group)
        .bind(now)
        .bind(moderated)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn remove_group(&self, group: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        sqlx::query("DELETE FROM group_articles WHERE group_name = $1")
            .bind(group)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM groups WHERE name = $1")
            .bind(group)
            .execute(&self.pool)
            .await?;
        sqlx::query(
            "DELETE FROM messages WHERE message_id NOT IN (SELECT DISTINCT message_id FROM group_articles)",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn is_group_moderated(&self, group: &str) -> Result<bool, Box<dyn Error + Send + Sync>> {
        let row = sqlx::query("SELECT moderated FROM groups WHERE name = $1")
            .bind(group)
            .fetch_optional(&self.pool)
            .await?;
        if let Some(r) = row {
            let m: bool = r.try_get("moderated")?;
            Ok(m)
        } else {
            Ok(false)
        }
    }

    #[tracing::instrument(skip_all)]
    async fn list_groups(&self) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
        let rows = sqlx::query("SELECT name FROM groups ORDER BY name")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| row.try_get::<String, _>("name").unwrap())
            .collect())
    }

    #[tracing::instrument(skip_all)]
    async fn list_groups_since(
        &self,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
        let rows = sqlx::query("SELECT name FROM groups WHERE created_at > $1 ORDER BY name")
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
        let rows =
            sqlx::query("SELECT number FROM group_articles WHERE group_name = $1 ORDER BY number")
                .bind(group)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows
            .into_iter()
            .map(|r| u64::try_from(r.try_get::<i64, _>("number").unwrap()).unwrap_or(0))
            .collect())
    }

    #[tracing::instrument(skip_all)]
    async fn list_article_ids(
        &self,
        group: &str,
    ) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
        let rows = sqlx::query(
            "SELECT message_id FROM group_articles WHERE group_name = $1 ORDER BY number",
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
            "SELECT message_id FROM group_articles WHERE group_name = $1 AND inserted_at > $2 ORDER BY number",
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
        sqlx::query("DELETE FROM group_articles WHERE group_name = $1 AND inserted_at < $2")
            .bind(group)
            .bind(before.timestamp())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn purge_orphan_messages(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        sqlx::query(
            "DELETE FROM messages WHERE message_id NOT IN (SELECT DISTINCT message_id FROM group_articles)",
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
        if let Some(row) = sqlx::query("SELECT size FROM messages WHERE message_id = $1")
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
        sqlx::query("DELETE FROM group_articles WHERE message_id = $1")
            .bind(message_id)
            .execute(&self.pool)
            .await?;
        sqlx::query(
            "DELETE FROM messages WHERE message_id = $1 AND NOT EXISTS (SELECT 1 FROM group_articles WHERE message_id = $1)",
        )
        .bind(message_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
