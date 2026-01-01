use crate::migrations::{Migration, Migrator};
use anyhow::Result;
use async_trait::async_trait;
use sqlx::Row;

/// Version table creation SQL for PostgreSQL storage  
const CREATE_VERSION_TABLE_POSTGRES: &str = "CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY
)";

/// Migration to add description column to groups table
#[cfg(feature = "postgres")]
struct AddGroupDescriptionMigration {
    pool: sqlx::PgPool,
}

#[cfg(feature = "postgres")]
#[async_trait]
impl Migration for AddGroupDescriptionMigration {
    fn target_version(&self) -> u32 {
        2
    }

    fn description(&self) -> &str {
        "Add description column to groups table"
    }

    async fn apply(&self) -> Result<()> {
        sqlx::query("ALTER TABLE groups ADD COLUMN description TEXT NOT NULL DEFAULT ''")
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

/// PostgreSQL storage migrator
#[cfg(feature = "postgres")]
pub struct PostgresStorageMigrator {
    pool: sqlx::PgPool,
}

#[cfg(feature = "postgres")]
impl PostgresStorageMigrator {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl Migrator for PostgresStorageMigrator {
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
        vec![Box::new(AddGroupDescriptionMigration {
            pool: self.pool.clone(),
        })]
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "postgres")]
    use super::*;

    #[cfg(feature = "postgres")]
    #[tokio::test]
    async fn test_postgres_storage_migrator_fresh_database() {
        // This test would require a PostgreSQL instance, so we'll skip it in CI
        // but include it for local testing with a real PostgreSQL setup
        if std::env::var("POSTGRES_TEST_URL").is_err() {
            return;
        }

        let db_url = std::env::var("POSTGRES_TEST_URL").unwrap();
        let pool = sqlx::PgPool::connect(&db_url).await.unwrap();
        let migrator = PostgresStorageMigrator::new(pool.clone());

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
    async fn test_postgres_storage_migrator_no_migrations() {
        if std::env::var("POSTGRES_TEST_URL").is_err() {
            return;
        }

        let db_url = std::env::var("POSTGRES_TEST_URL").unwrap();
        let pool = sqlx::PgPool::connect(&db_url).await.unwrap();
        let migrator = PostgresStorageMigrator::new(pool.clone());

        // Verify we have the expected migrations
        let migrations = migrator.get_migrations();
        assert_eq!(migrations.len(), 1);
        assert_eq!(migrations[0].target_version(), 2);
        assert_eq!(
            migrations[0].description(),
            "Add description column to groups table"
        );
    }
}
