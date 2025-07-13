use super::{Migration, Migrator};
use async_trait::async_trait;
use sqlx::{Row, SqlitePool};
use std::error::Error;

/// Version table creation SQL for SQLite auth
const CREATE_VERSION_TABLE_SQLITE: &str = "CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY
)";

/// Version table creation SQL for PostgreSQL auth
#[cfg(feature = "postgres")]
const CREATE_VERSION_TABLE_POSTGRES: &str = "CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY
)";

/// Migration from version 0 to 1 for SQLite auth.
/// 
/// This represents the current schema as the baseline (version 1).
/// Since the current schema is already created by the existing code,
/// this migration is essentially a no-op but establishes the baseline.
pub struct SqliteAuthMigration001 {
    pool: SqlitePool,
}

impl SqliteAuthMigration001 {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Migration for SqliteAuthMigration001 {
    fn target_version(&self) -> u32 {
        1
    }
    
    fn description(&self) -> &str {
        "Initial schema baseline - users, admins, and moderators tables"
    }
    
    async fn apply(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        // The current schema is already created by the existing initialization code.
        // This migration just establishes version 1 as the baseline.
        // All the tables (users, admins, moderators) are already
        // created by the existing SqliteAuth::new() method.
        
        tracing::debug!("Applying SQLite auth migration 001: establishing baseline schema");
        
        // Verify that the expected tables exist
        let tables_query = "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('users', 'admins', 'moderators')";
        let rows = sqlx::query(tables_query).fetch_all(&self.pool).await?;
        
        if rows.len() != 3 {
            return Err("Expected baseline tables not found. Please ensure the database was initialized properly.".into());
        }
        
        tracing::debug!("SQLite auth migration 001 completed successfully");
        Ok(())
    }
}

/// PostgreSQL auth migration from version 0 to 1
#[cfg(feature = "postgres")]
pub struct PostgresAuthMigration001 {
    pool: sqlx::PgPool,
}

#[cfg(feature = "postgres")]
impl PostgresAuthMigration001 {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl Migration for PostgresAuthMigration001 {
    fn target_version(&self) -> u32 {
        1
    }
    
    fn description(&self) -> &str {
        "Initial schema baseline - users, admins, and moderators tables"
    }
    
    async fn apply(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        tracing::debug!("Applying PostgreSQL auth migration 001: establishing baseline schema");
        
        // Verify that the expected tables exist
        let tables_query = "SELECT tablename FROM pg_tables WHERE tablename IN ('users', 'admins', 'moderators')";
        let rows = sqlx::query(tables_query).fetch_all(&self.pool).await?;
        
        if rows.len() != 3 {
            return Err("Expected baseline tables not found. Please ensure the database was initialized properly.".into());
        }
        
        tracing::debug!("PostgreSQL auth migration 001 completed successfully");
        Ok(())
    }
}

/// SQLite auth migrator
pub struct SqliteAuthMigrator {
    pool: SqlitePool,
}

impl SqliteAuthMigrator {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Migrator for SqliteAuthMigrator {
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
            Box::new(SqliteAuthMigration001::new(self.pool.clone())),
        ]
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
            Box::new(PostgresAuthMigration001::new(self.pool.clone())),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_sqlite_auth_migrator_fresh_install() {
        let temp_file = NamedTempFile::new().unwrap();
        let db_path = format!("sqlite://{}", temp_file.path().display());
        
        let pool = sqlx::SqlitePool::connect(&db_path).await.unwrap();
        
        // Create the baseline tables first (simulating what the existing code does)
        sqlx::query("CREATE TABLE IF NOT EXISTS users (
            username TEXT PRIMARY KEY,
            password_hash TEXT NOT NULL,
            key TEXT
        )").execute(&pool).await.unwrap();
        
        sqlx::query("CREATE TABLE IF NOT EXISTS admins (
            username TEXT PRIMARY KEY REFERENCES users(username)
        )").execute(&pool).await.unwrap();
        
        sqlx::query("CREATE TABLE IF NOT EXISTS moderators (
            username TEXT REFERENCES users(username),
            pattern TEXT,
            PRIMARY KEY(username, pattern)
        )").execute(&pool).await.unwrap();
        
        let migrator = SqliteAuthMigrator::new(pool.clone());
        
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
    async fn test_sqlite_auth_migrator_version_tracking() {
        let temp_file = NamedTempFile::new().unwrap();
        let db_path = format!("sqlite://{}", temp_file.path().display());
        
        let pool = sqlx::SqlitePool::connect(&db_path).await.unwrap();
        let migrator = SqliteAuthMigrator::new(pool.clone());
        
        // Initialize the version table
        let _version = migrator.get_current_version().await.unwrap();
        
        // Set a specific version
        migrator.set_version(3).await.unwrap();
        
        // Verify it was set
        let version = migrator.get_current_version().await.unwrap();
        assert_eq!(version, 3);
        
        // Set a different version
        migrator.set_version(7).await.unwrap();
        let version = migrator.get_current_version().await.unwrap();
        assert_eq!(version, 7);
    }

    #[tokio::test]
    async fn test_sqlite_auth_migration_001() {
        let temp_file = NamedTempFile::new().unwrap();
        let db_path = format!("sqlite://{}", temp_file.path().display());
        
        let pool = sqlx::SqlitePool::connect(&db_path).await.unwrap();
        
        // Create the baseline tables that the existing code would create
        sqlx::query("CREATE TABLE IF NOT EXISTS users (
            username TEXT PRIMARY KEY,
            password_hash TEXT NOT NULL,
            key TEXT
        )").execute(&pool).await.unwrap();
        
        sqlx::query("CREATE TABLE IF NOT EXISTS admins (
            username TEXT PRIMARY KEY REFERENCES users(username)
        )").execute(&pool).await.unwrap();
        
        sqlx::query("CREATE TABLE IF NOT EXISTS moderators (
            username TEXT REFERENCES users(username),
            pattern TEXT,
            PRIMARY KEY(username, pattern)
        )").execute(&pool).await.unwrap();
        
        // Now test the migration
        let migration = SqliteAuthMigration001::new(pool.clone());
        
        assert_eq!(migration.target_version(), 1);
        assert_eq!(migration.description(), "Initial schema baseline - users, admins, and moderators tables");
        
        // Apply the migration
        migration.apply().await.unwrap();
        
        // Should be idempotent
        migration.apply().await.unwrap();
    }
}