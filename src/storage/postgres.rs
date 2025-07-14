use super::{
    ArticleStream, Message, Storage, StringStream, StringTimestampStream, U64Stream,
    common::{Headers, extract_message_id},
};
use crate::migrations::Migrator;
use anyhow::Result;
use async_stream::stream;
use async_trait::async_trait;
use futures_util::StreamExt;
use smallvec::SmallVec;
use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};
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

const OVERVIEW_TABLE: &str = "CREATE TABLE IF NOT EXISTS overview (
        group_name TEXT,
        article_number BIGINT,
        overview_data TEXT,
        PRIMARY KEY(group_name, article_number)
    )";

#[derive(Clone)]
pub struct PostgresStorage {
    pool: PgPool,
}

impl PostgresStorage {
    #[tracing::instrument(skip_all)]
    /// Create a new Postgres storage backend.
    pub async fn new(uri: &str) -> Result<Self> {
        let opts = PgConnectOptions::from_str(uri).map_err(|e| {
            anyhow::anyhow!(
                "Invalid PostgreSQL connection URI '{}': {}

Please ensure the URI is in the correct format:
- Standard connection: postgresql://user:password@host:port/database
- Local connection: postgresql:///database_name
- With SSL: postgresql://user:password@host:port/database?sslmode=require

Required connection components:
- host: PostgreSQL server hostname or IP
- port: PostgreSQL server port (default: 5432)
- database: Target database name
- user: PostgreSQL username
- password: User password (if required)",
                uri, e
            )
        })?;

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to connect to PostgreSQL database '{}': {}

Possible causes:
- PostgreSQL server is not running or unreachable
- Incorrect hostname, port, username, or password
- Database does not exist
- Connection refused due to pg_hba.conf configuration
- SSL/TLS connection required but not configured
- Network firewall blocking the connection
- PostgreSQL server not accepting connections

Please verify:
1. PostgreSQL server is running: systemctl status postgresql
2. Database exists: psql -l
3. User has access privileges: GRANT CONNECT ON DATABASE dbname TO username;
4. Connection settings in pg_hba.conf allow your connection method",
                    uri, e
                )
            })?;

        // Set up migrator to check database state
        let migrator = super::migrations::postgres::PostgresStorageMigrator::new(pool.clone());

        if migrator.is_fresh_database().await {
            // Fresh database: initialize with current schema
            tracing::info!(
                "Initializing fresh PostgreSQL storage database at '{}'",
                uri
            );

            // Create database schema
            sqlx::query(MESSAGES_TABLE)
                .execute(&pool)
                .await
                .map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to create messages table in PostgreSQL database '{}': {}",
                        uri, e
                    )
                })?;
            sqlx::query(GROUP_ARTICLES_TABLE)
                .execute(&pool)
                .await
                .map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to create group_articles table in PostgreSQL database '{}': {}",
                        uri, e
                    )
                })?;
            sqlx::query(GROUPS_TABLE)
                .execute(&pool)
                .await
                .map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to create groups table in PostgreSQL database '{}': {}",
                        uri, e
                    )
                })?;
            sqlx::query(OVERVIEW_TABLE)
                .execute(&pool)
                .await
                .map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to create overview table in PostgreSQL database '{}': {}",
                        uri, e
                    )
                })?;

            // Set current version (since pre-1.0, we use version 1 as the baseline)
            migrator.set_version(1).await.map_err(|e| {
                anyhow::anyhow!(
                    "Failed to set initial schema version for PostgreSQL storage database '{}': {}",
                    uri, e
                )
            })?;

            tracing::info!("Successfully initialized PostgreSQL storage database at version 1");
        } else {
            // Existing database: apply any pending migrations
            tracing::info!("Found existing PostgreSQL storage database, checking for migrations");
            migrator.migrate_to_latest().await.map_err(|e| {
                anyhow::anyhow!(
                    "Failed to run storage migrations for PostgreSQL database '{}': {}",
                    uri, e
                )
            })?;
        }

        Ok(Self { pool })
    }
}

#[async_trait]
impl Storage for PostgresStorage {
    #[tracing::instrument(skip_all)]
    async fn store_article(&self, article: &Message) -> Result<()> {
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

        // Associate with each group and create overview data
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

            // Generate and store overview data
            let overview_data = {
                use crate::overview::generate_overview_line;
                generate_overview_line(self, next as u64, article).await?
            };

            sqlx::query(
                "INSERT INTO overview (group_name, article_number, overview_data) VALUES ($1, $2, $3) ON CONFLICT (group_name, article_number) DO UPDATE SET overview_data = EXCLUDED.overview_data",
            )
            .bind(&group)
            .bind(next)
            .bind(&overview_data)
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
    ) -> Result<Option<Message>> {
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
            Ok(Some(crate::storage::common::reconstruct_message_from_row(&headers_str, &body)?))
        } else {
            Ok(None)
        }
    }

    #[tracing::instrument(skip_all)]
    async fn get_article_by_id(
        &self,
        message_id: &str,
    ) -> Result<Option<Message>> {
        if let Some(row) = sqlx::query("SELECT headers, body FROM messages WHERE message_id = $1")
            .bind(message_id)
            .fetch_optional(&self.pool)
            .await?
        {
            let headers_str: String = row.try_get("headers")?;
            let body: String = row.try_get("body")?;
            Ok(Some(crate::storage::common::reconstruct_message_from_row(
                &headers_str,
                &body,
            )?))
        } else {
            Ok(None)
        }
    }

    #[tracing::instrument(skip_all)]
    fn get_articles_by_ids<'a>(&'a self, message_ids: &'a [String]) -> ArticleStream<'a> {
        let pool = self.pool.clone();

        Box::pin(stream! {
            if message_ids.is_empty() {
                return;
            }

            // Build a parameterized query with the right number of placeholders
            let placeholders = (1..=message_ids.len()).map(|i| format!("${i}")).collect::<Vec<_>>().join(", ");
            let query = format!("SELECT message_id, headers, body FROM messages WHERE message_id IN ({placeholders})");

            let mut query_builder = sqlx::query(&query);
            for message_id in message_ids {
                query_builder = query_builder.bind(message_id);
            }

            let mut rows = query_builder.fetch(&pool);

            while let Some(row) = rows.next().await {
                match row {
                    Ok(r) => {
                        match (
                            r.try_get::<String, _>("message_id"),
                            r.try_get::<String, _>("headers"),
                            r.try_get::<String, _>("body")
                        ) {
                            (Ok(message_id), Ok(headers_str), Ok(body)) => {
                                match crate::storage::common::reconstruct_message_from_row(&headers_str, &body) {
                                    Ok(message) => yield Ok((message_id, message)),
                                    Err(e) => yield Err(e),
                                }
                            },
                            (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => {
                                yield Err(anyhow::Error::from(e))
                            }
                        }
                    },
                    Err(e) => yield Err(anyhow::Error::from(e)),
                }
            }
        })
    }

    #[tracing::instrument(skip_all)]
    async fn add_group(
        &self,
        group: &str,
        moderated: bool,
    ) -> Result<()> {
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
    async fn set_group_moderated(
        &self,
        group: &str,
        moderated: bool,
    ) -> Result<()> {
        sqlx::query("UPDATE groups SET moderated = $1 WHERE name = $2")
            .bind(moderated)
            .bind(group)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn remove_group(&self, group: &str) -> Result<()> {
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
    async fn remove_groups_by_pattern(
        &self,
        pattern: &str,
    ) -> Result<()> {
        // Get all group names that match the pattern
        let rows = sqlx::query("SELECT name FROM groups")
            .fetch_all(&self.pool)
            .await?;

        let mut matching_groups = Vec::new();
        for row in rows {
            let name: String = row.try_get("name")?;
            if crate::wildmat::wildmat(pattern, &name) {
                matching_groups.push(name);
            }
        }

        // Remove each matching group
        for group in matching_groups {
            self.remove_group(&group).await?;
        }

        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn is_group_moderated(&self, group: &str) -> Result<bool> {
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
    async fn group_exists(&self, group: &str) -> Result<bool> {
        let row = sqlx::query("SELECT 1 FROM groups WHERE name = $1 LIMIT 1")
            .bind(group)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.is_some())
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
                        Err(e) => yield Err(anyhow::Error::from(e)),
                    },
                    Err(e) => yield Err(anyhow::Error::from(e)),
                }
            }
        })
    }

    #[tracing::instrument(skip_all)]
    fn list_groups_since(&self, since: chrono::DateTime<chrono::Utc>) -> StringStream<'_> {
        let pool = self.pool.clone();
        let timestamp = since.timestamp();
        Box::pin(stream! {
            let mut rows = sqlx::query("SELECT name FROM groups WHERE created_at > $1 ORDER BY name")
                .bind(timestamp)
                .fetch(&pool);

            while let Some(row) = rows.next().await {
                match row {
                    Ok(r) => match r.try_get::<String, _>("name") {
                        Ok(name) => yield Ok(name),
                        Err(e) => yield Err(anyhow::Error::from(e)),
                    },
                    Err(e) => yield Err(anyhow::Error::from(e)),
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
                            (Err(e), _) => yield Err(anyhow::Error::from(e)),
                            (_, Err(e)) => yield Err(anyhow::Error::from(e)),
                        }
                    },
                    Err(e) => yield Err(anyhow::Error::from(e)),
                }
            }
        })
    }

    #[tracing::instrument(skip_all)]
    fn list_article_numbers(&self, group: &str) -> U64Stream<'_> {
        let pool = self.pool.clone();
        let group = group.to_string();
        Box::pin(stream! {
            let mut rows = sqlx::query("SELECT number FROM group_articles WHERE group_name = $1 ORDER BY number")
                .bind(&group)
                .fetch(&pool);

            while let Some(row) = rows.next().await {
                match row {
                    Ok(r) => match r.try_get::<i64, _>("number") {
                        Ok(number) => yield Ok(u64::try_from(number).unwrap_or(0)),
                        Err(e) => yield Err(anyhow::Error::from(e)),
                    },
                    Err(e) => yield Err(anyhow::Error::from(e)),
                }
            }
        })
    }

    #[tracing::instrument(skip_all)]
    fn list_article_ids(&self, group: &str) -> StringStream<'_> {
        let pool = self.pool.clone();
        let group = group.to_string();
        Box::pin(stream! {
            let mut rows = sqlx::query("SELECT message_id FROM group_articles WHERE group_name = $1 ORDER BY number")
                .bind(&group)
                .fetch(&pool);

            while let Some(row) = rows.next().await {
                match row {
                    Ok(r) => match r.try_get::<String, _>("message_id") {
                        Ok(message_id) => yield Ok(message_id),
                        Err(e) => yield Err(anyhow::Error::from(e)),
                    },
                    Err(e) => yield Err(anyhow::Error::from(e)),
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
            let mut rows = sqlx::query("SELECT message_id FROM group_articles WHERE group_name = $1 AND inserted_at > $2 ORDER BY number")
                .bind(&group)
                .bind(timestamp)
                .fetch(&pool);

            while let Some(row) = rows.next().await {
                match row {
                    Ok(r) => match r.try_get::<String, _>("message_id") {
                        Ok(message_id) => yield Ok(message_id),
                        Err(e) => yield Err(anyhow::Error::from(e)),
                    },
                    Err(e) => yield Err(anyhow::Error::from(e)),
                }
            }
        })
    }

    #[tracing::instrument(skip_all)]
    async fn purge_group_before(
        &self,
        group: &str,
        before: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        sqlx::query("DELETE FROM group_articles WHERE group_name = $1 AND inserted_at < $2")
            .bind(group)
            .bind(before.timestamp())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn purge_orphan_messages(&self) -> Result<()> {
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
    ) -> Result<Option<u64>> {
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
    ) -> Result<()> {
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

    #[tracing::instrument(skip_all)]
    async fn get_overview_range(
        &self,
        group: &str,
        start: u64,
        end: u64,
    ) -> Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT overview_data FROM overview WHERE group_name = $1 AND article_number >= $2 AND article_number <= $3 ORDER BY article_number",
        )
        .bind(group)
        .bind(i64::try_from(start).unwrap_or(0))
        .bind(i64::try_from(end).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await?;

        let mut overview_lines = Vec::new();
        for row in rows {
            let overview_data: String = row.try_get("overview_data")?;
            overview_lines.push(overview_data);
        }

        Ok(overview_lines)
    }
}
