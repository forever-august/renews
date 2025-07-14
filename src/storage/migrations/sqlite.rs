use anyhow::Result;
use crate::migrations::{Migration, Migrator};
use async_trait::async_trait;
use sqlx::{Row, SqlitePool};

/// Version table creation SQL for SQLite storage
const CREATE_VERSION_TABLE_SQLITE: &str = "CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY
)";

/// SQLite storage migrator
pub struct SqliteStorageMigrator {
    pool: SqlitePool,
}

impl SqliteStorageMigrator {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Migrator for SqliteStorageMigrator {
    async fn get_current_version(&self) -> Result<u32> {
        // Try to read from the version table - if table doesn't exist, this will fail
        let row = sqlx::query("SELECT version FROM schema_version ORDER BY version DESC LIMIT 1")
            .fetch_optional(&self.pool)
            .await;

        match row {
            Ok(Some(row)) => {
                let version: i64 = row.try_get("version")?;
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
        sqlx::query(CREATE_VERSION_TABLE_SQLITE)
            .execute(&self.pool)
            .await?;

        // Delete old version and insert new one (simple approach)
        sqlx::query("DELETE FROM schema_version")
            .execute(&self.pool)
            .await?;

        sqlx::query("INSERT INTO schema_version (version) VALUES (?)")
            .bind(version as i64)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    fn get_migrations(&self) -> Vec<Box<dyn Migration>> {
        // Since this is pre-1.0, no migrations exist yet
        // This will serve as a template for future migrations
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_sqlite_storage_migrator_fresh_database() {
        let temp_file = NamedTempFile::new().unwrap();
        let db_path = format!("sqlite://{}", temp_file.path().display());

        let pool = sqlx::SqlitePool::connect(&db_path).await.unwrap();
        let migrator = SqliteStorageMigrator::new(pool.clone());

        // Fresh database should fail to read version initially
        assert!(migrator.is_fresh_database().await);

        // But once we set a version, it should work
        migrator.set_version(1).await.unwrap();
        assert!(!migrator.is_fresh_database().await);

        let version = migrator.get_current_version().await.unwrap();
        assert_eq!(version, 1);
    }

    #[tokio::test]
    async fn test_sqlite_storage_migrator_version_tracking() {
        let temp_file = NamedTempFile::new().unwrap();
        let db_path = format!("sqlite://{}", temp_file.path().display());

        let pool = sqlx::SqlitePool::connect(&db_path).await.unwrap();
        let migrator = SqliteStorageMigrator::new(pool.clone());

        // Set a specific version
        migrator.set_version(5).await.unwrap();

        // Verify it was set
        let version = migrator.get_current_version().await.unwrap();
        assert_eq!(version, 5);

        // Set a different version
        migrator.set_version(10).await.unwrap();
        let version = migrator.get_current_version().await.unwrap();
        assert_eq!(version, 10);
    }

    #[tokio::test]
    async fn test_sqlite_storage_migrator_no_migrations() {
        let temp_file = NamedTempFile::new().unwrap();
        let db_path = format!("sqlite://{}", temp_file.path().display());

        let pool = sqlx::SqlitePool::connect(&db_path).await.unwrap();
        let migrator = SqliteStorageMigrator::new(pool.clone());

        // Pre-1.0, so no migrations should exist
        let migrations = migrator.get_migrations();
        assert!(migrations.is_empty());

        // Migration should be a no-op
        migrator.set_version(1).await.unwrap(); // Simulate fresh initialization
        migrator.migrate_to_latest().await.unwrap();

        let version = migrator.get_current_version().await.unwrap();
        assert_eq!(version, 1); // Should remain at 1
    }
}
