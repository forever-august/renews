use crate::migrations::{Migration, Migrator};
use anyhow::Result;
use async_trait::async_trait;
use sqlx::Row;

/// Version table creation SQL for PostgreSQL auth
const CREATE_VERSION_TABLE_POSTGRES: &str = "CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY
)";

/// Migration to version 2: Add user_limits and user_usage tables
#[cfg(feature = "postgres")]
struct MigrationV2 {
    pool: sqlx::PgPool,
}

#[cfg(feature = "postgres")]
impl MigrationV2 {
    fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl Migration for MigrationV2 {
    fn target_version(&self) -> u32 {
        2
    }

    fn description(&self) -> &str {
        "Add user_limits and user_usage tables for per-user limits"
    }

    async fn apply(&self) -> Result<()> {
        // Create user_limits table for per-user limit overrides
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS user_limits (
                username TEXT PRIMARY KEY REFERENCES users(username) ON DELETE CASCADE,
                can_post BOOLEAN,
                max_connections INTEGER,
                bandwidth_limit_bytes BIGINT,
                bandwidth_period_secs BIGINT,
                created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
            )",
        )
        .execute(&self.pool)
        .await?;

        // Create user_usage table for usage tracking
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS user_usage (
                username TEXT PRIMARY KEY REFERENCES users(username) ON DELETE CASCADE,
                bytes_uploaded BIGINT NOT NULL DEFAULT 0,
                bytes_downloaded BIGINT NOT NULL DEFAULT 0,
                window_start_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
            )",
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

/// PostgreSQL auth migrator
#[cfg(feature = "postgres")]
pub struct PostgresAuthMigrator {
    pool: sqlx::PgPool,
}

#[cfg(feature = "postgres")]
impl PostgresAuthMigrator {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl Migrator for PostgresAuthMigrator {
    async fn get_current_version(&self) -> Result<u32> {
        // Try to read from the version table - if table doesn't exist, this will fail
        let row = sqlx::query("SELECT version FROM schema_version ORDER BY version DESC LIMIT 1")
            .fetch_optional(&self.pool)
            .await;

        match row {
            Ok(Some(row)) => {
                let version: i32 = row.try_get("version")?;
                Ok(version as u32)
            }
            Ok(None) => {
                // Version table exists but no version stored, this means fresh database
                // that was just initialized
                Ok(0)
            }
            Err(_) => {
                // Table doesn't exist, definitely a fresh database
                Err(anyhow::anyhow!("Version table does not exist"))
            }
        }
    }

    async fn set_version(&self, version: u32) -> Result<()> {
        // Ensure version table exists first
        sqlx::query(CREATE_VERSION_TABLE_POSTGRES)
            .execute(&self.pool)
            .await?;

        // Delete old version and insert new one (simple approach)
        sqlx::query("DELETE FROM schema_version")
            .execute(&self.pool)
            .await?;

        sqlx::query("INSERT INTO schema_version (version) VALUES ($1)")
            .bind(version as i32)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    fn get_migrations(&self) -> Vec<Box<dyn Migration>> {
        vec![Box::new(MigrationV2::new(self.pool.clone()))]
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "postgres")]
    use super::*;

    #[cfg(feature = "postgres")]
    #[tokio::test]
    async fn test_postgres_auth_migrator_fresh_database() {
        // This test would require a PostgreSQL instance, so we'll skip it in CI
        // but include it for local testing with a real PostgreSQL setup
        if std::env::var("POSTGRES_TEST_URL").is_err() {
            return;
        }

        let db_url = std::env::var("POSTGRES_TEST_URL").unwrap();
        let pool = sqlx::PgPool::connect(&db_url).await.unwrap();
        let migrator = PostgresAuthMigrator::new(pool.clone());

        // Fresh database should fail to read version initially
        assert!(migrator.is_fresh_database().await);

        // But once we set a version, it should work
        migrator.set_version(1).await.unwrap();
        assert!(!migrator.is_fresh_database().await);

        let version = migrator.get_current_version().await.unwrap();
        assert_eq!(version, 1);
    }

    #[cfg(feature = "postgres")]
    #[tokio::test]
    async fn test_postgres_auth_migrator_migrations() {
        if std::env::var("POSTGRES_TEST_URL").is_err() {
            return;
        }

        let db_url = std::env::var("POSTGRES_TEST_URL").unwrap();
        let pool = sqlx::PgPool::connect(&db_url).await.unwrap();
        let migrator = PostgresAuthMigrator::new(pool.clone());

        // Should have migrations available
        let migrations = migrator.get_migrations();
        assert!(!migrations.is_empty());

        // V2 migration should be present
        assert_eq!(migrations[0].target_version(), 2);

        // Set up base schema (version 1 - simulates existing database)
        // First create the users table that user_limits references
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS users (
            username TEXT PRIMARY KEY,
            password_hash TEXT NOT NULL,
            key TEXT
        )",
        )
        .execute(&pool)
        .await
        .unwrap();

        migrator.set_version(1).await.unwrap();

        // Run migration
        migrator.migrate_to_latest().await.unwrap();

        let version = migrator.get_current_version().await.unwrap();
        assert_eq!(version, 2);
    }
}
