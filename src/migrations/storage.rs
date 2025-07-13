use super::{Migration, Migrator};
use async_trait::async_trait;
use sqlx::{Row, SqlitePool};
use std::error::Error;

/// Version table creation SQL for SQLite storage
const CREATE_VERSION_TABLE_SQLITE: &str = "CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY
)";

/// Version table creation SQL for PostgreSQL storage  
#[cfg(feature = "postgres")]
const CREATE_VERSION_TABLE_POSTGRES: &str = "CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY
)";

/// Migration from version 0 to 1 for SQLite storage.
/// 
/// This represents the current schema as the baseline (version 1).
/// Since the current schema is already created by the existing code,
/// this migration is essentially a no-op but establishes the baseline.
pub struct SqliteStorageMigration001 {
    pool: SqlitePool,
}

impl SqliteStorageMigration001 {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Migration for SqliteStorageMigration001 {
    fn target_version(&self) -> u32 {
        1
    }
    
    fn description(&self) -> &str {
        "Initial schema baseline - messages, group_articles, groups, and overview tables"
    }
    
    async fn apply(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        // The current schema is already created by the existing initialization code.
        // This migration just establishes version 1 as the baseline.
        // All the tables (messages, group_articles, groups, overview) are already
        // created by the existing SqliteStorage::new() method.
        
        tracing::debug!("Applying SQLite storage migration 001: establishing baseline schema");
        
        // Verify that the expected tables exist
        let tables_query = "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('messages', 'group_articles', 'groups', 'overview')";
        let rows = sqlx::query(tables_query).fetch_all(&self.pool).await?;
        
        if rows.len() != 4 {
            return Err("Expected baseline tables not found. Please ensure the database was initialized properly.".into());
        }
        
        tracing::debug!("SQLite storage migration 001 completed successfully");
        Ok(())
    }
}

/// PostgreSQL storage migration from version 0 to 1
#[cfg(feature = "postgres")]
pub struct PostgresStorageMigration001 {
    pool: sqlx::PgPool,
}

#[cfg(feature = "postgres")]
impl PostgresStorageMigration001 {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl Migration for PostgresStorageMigration001 {
    fn target_version(&self) -> u32 {
        1
    }
    
    fn description(&self) -> &str {
        "Initial schema baseline - messages, group_articles, groups, and overview tables"
    }
    
    async fn apply(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        tracing::debug!("Applying PostgreSQL storage migration 001: establishing baseline schema");
        
        // Verify that the expected tables exist
        let tables_query = "SELECT tablename FROM pg_tables WHERE tablename IN ('messages', 'group_articles', 'groups', 'overview')";
        let rows = sqlx::query(tables_query).fetch_all(&self.pool).await?;
        
        if rows.len() != 4 {
            return Err("Expected baseline tables not found. Please ensure the database was initialized properly.".into());
        }
        
        tracing::debug!("PostgreSQL storage migration 001 completed successfully");
        Ok(())
    }
}

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
    async fn get_current_version(&self) -> Result<u32, Box<dyn Error + Send + Sync>> {
        // Ensure version table exists
        sqlx::query(CREATE_VERSION_TABLE_SQLITE)
            .execute(&self.pool)
            .await?;
        
        // Get current version
        let row = sqlx::query("SELECT version FROM schema_version ORDER BY version DESC LIMIT 1")
            .fetch_optional(&self.pool)
            .await?;
        
        match row {
            Some(row) => {
                let version: i64 = row.try_get("version")?;
                Ok(version as u32)
            }
            None => Ok(0), // No version stored = fresh install
        }
    }
    
    async fn set_version(&self, version: u32) -> Result<(), Box<dyn Error + Send + Sync>> {
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
        vec![
            Box::new(SqliteStorageMigration001::new(self.pool.clone())),
        ]
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
    async fn get_current_version(&self) -> Result<u32, Box<dyn Error + Send + Sync>> {
        // Ensure version table exists
        sqlx::query(CREATE_VERSION_TABLE_POSTGRES)
            .execute(&self.pool)
            .await?;
        
        // Get current version
        let row = sqlx::query("SELECT version FROM schema_version ORDER BY version DESC LIMIT 1")
            .fetch_optional(&self.pool)
            .await?;
        
        match row {
            Some(row) => {
                let version: i32 = row.try_get("version")?;
                Ok(version as u32)
            }
            None => Ok(0), // No version stored = fresh install
        }
    }
    
    async fn set_version(&self, version: u32) -> Result<(), Box<dyn Error + Send + Sync>> {
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
        vec![
            Box::new(PostgresStorageMigration001::new(self.pool.clone())),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_sqlite_storage_migrator_fresh_install() {
        let temp_file = NamedTempFile::new().unwrap();
        let db_path = format!("sqlite://{}", temp_file.path().display());
        
        let pool = sqlx::SqlitePool::connect(&db_path).await.unwrap();
        
        // Create the baseline tables first (simulating what the existing code does)
        sqlx::query("CREATE TABLE IF NOT EXISTS messages (
            message_id TEXT PRIMARY KEY,
            headers TEXT,
            body TEXT,
            size INTEGER NOT NULL
        )").execute(&pool).await.unwrap();
        
        sqlx::query("CREATE TABLE IF NOT EXISTS group_articles (
            group_name TEXT,
            number INTEGER,
            message_id TEXT,
            inserted_at INTEGER NOT NULL,
            PRIMARY KEY(group_name, number),
            FOREIGN KEY(message_id) REFERENCES messages(message_id)
        )").execute(&pool).await.unwrap();
        
        sqlx::query("CREATE TABLE IF NOT EXISTS groups (
            name TEXT PRIMARY KEY,
            created_at INTEGER NOT NULL,
            moderated INTEGER NOT NULL DEFAULT 0
        )").execute(&pool).await.unwrap();
        
        sqlx::query("CREATE TABLE IF NOT EXISTS overview (
            group_name TEXT,
            article_number INTEGER,
            overview_data TEXT,
            PRIMARY KEY(group_name, article_number)
        )").execute(&pool).await.unwrap();
        
        let migrator = SqliteStorageMigrator::new(pool.clone());
        
        // Fresh install should return version 0
        let version = migrator.get_current_version().await.unwrap();
        assert_eq!(version, 0);
        
        // Apply migrations
        migrator.migrate_to_latest().await.unwrap();
        
        // Should now be at version 1
        let version = migrator.get_current_version().await.unwrap();
        assert_eq!(version, 1);
        
        // Running again should be idempotent
        migrator.migrate_to_latest().await.unwrap();
        let version = migrator.get_current_version().await.unwrap();
        assert_eq!(version, 1);
    }

    #[tokio::test]
    async fn test_sqlite_storage_migrator_version_tracking() {
        let temp_file = NamedTempFile::new().unwrap();
        let db_path = format!("sqlite://{}", temp_file.path().display());
        
        let pool = sqlx::SqlitePool::connect(&db_path).await.unwrap();
        let migrator = SqliteStorageMigrator::new(pool.clone());
        
        // Initialize the version table
        let _version = migrator.get_current_version().await.unwrap();
        
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
    async fn test_sqlite_storage_migration_001() {
        let temp_file = NamedTempFile::new().unwrap();
        let db_path = format!("sqlite://{}", temp_file.path().display());
        
        let pool = sqlx::SqlitePool::connect(&db_path).await.unwrap();
        
        // Create the baseline tables that the existing code would create
        sqlx::query("CREATE TABLE IF NOT EXISTS messages (
            message_id TEXT PRIMARY KEY,
            headers TEXT,
            body TEXT,
            size INTEGER NOT NULL
        )").execute(&pool).await.unwrap();
        
        sqlx::query("CREATE TABLE IF NOT EXISTS group_articles (
            group_name TEXT,
            number INTEGER,
            message_id TEXT,
            inserted_at INTEGER NOT NULL,
            PRIMARY KEY(group_name, number),
            FOREIGN KEY(message_id) REFERENCES messages(message_id)
        )").execute(&pool).await.unwrap();
        
        sqlx::query("CREATE TABLE IF NOT EXISTS groups (
            name TEXT PRIMARY KEY,
            created_at INTEGER NOT NULL,
            moderated INTEGER NOT NULL DEFAULT 0
        )").execute(&pool).await.unwrap();
        
        sqlx::query("CREATE TABLE IF NOT EXISTS overview (
            group_name TEXT,
            article_number INTEGER,
            overview_data TEXT,
            PRIMARY KEY(group_name, article_number)
        )").execute(&pool).await.unwrap();
        
        // Now test the migration
        let migration = SqliteStorageMigration001::new(pool.clone());
        
        assert_eq!(migration.target_version(), 1);
        assert_eq!(migration.description(), "Initial schema baseline - messages, group_articles, groups, and overview tables");
        
        // Apply the migration
        migration.apply().await.unwrap();
        
        // Should be idempotent
        migration.apply().await.unwrap();
    }

    #[cfg(feature = "postgres")]
    #[tokio::test]
    async fn test_postgres_storage_migrator_fresh_install() {
        // This test would require a PostgreSQL instance, so we'll skip it in CI
        // but include it for local testing with a real PostgreSQL setup
        if std::env::var("POSTGRES_TEST_URL").is_err() {
            return;
        }
        
        let db_url = std::env::var("POSTGRES_TEST_URL").unwrap();
        let pool = sqlx::PgPool::connect(&db_url).await.unwrap();
        let migrator = PostgresStorageMigrator::new(pool.clone());
        
        // Fresh install should return version 0
        let version = migrator.get_current_version().await.unwrap();
        assert_eq!(version, 0);
        
        // Apply migrations
        migrator.migrate_to_latest().await.unwrap();
        
        // Should now be at version 1
        let version = migrator.get_current_version().await.unwrap();
        assert_eq!(version, 1);
    }
}