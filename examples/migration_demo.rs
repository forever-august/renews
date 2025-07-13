use renews::migrations::{Migration, Migrator};
use sqlx::{Row, SqlitePool};
use std::error::Error;
use tempfile::NamedTempFile;

/// Example demonstrating the migration system in action
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    println!("=== Renews Database Migration System Demo ===\n");

    // Create a temporary database for the demo
    let temp_file = NamedTempFile::new()?;
    let db_path = format!("sqlite://{}", temp_file.path().display());
    let pool = SqlitePool::connect(&db_path).await?;

    println!("1. Created temporary SQLite database at: {}", temp_file.path().display());

    // Create a simple migrator for demonstration
    let migrator = DemoMigrator::new(pool.clone());

    // Show initial state
    let initial_version = migrator.get_current_version().await?;
    println!("2. Initial schema version: {}", initial_version);

    // Apply migrations
    println!("3. Applying migrations...");
    migrator.migrate_to_latest().await?;

    // Show final state
    let final_version = migrator.get_current_version().await?;
    println!("4. Final schema version: {}", final_version);

    // Show that running again is idempotent
    println!("5. Running migrations again (should be idempotent)...");
    migrator.migrate_to_latest().await?;
    let version_after_second_run = migrator.get_current_version().await?;
    println!("6. Version after second run: {}", version_after_second_run);

    // Demonstrate the created schema
    println!("7. Demonstrating the created schema:");
    let tables = sqlx::query("SELECT name FROM sqlite_master WHERE type='table'")
        .fetch_all(&pool)
        .await?;
    
    for row in tables {
        let table_name: String = row.get("name");
        println!("   - Table: {}", table_name);
    }

    println!("\n=== Demo completed successfully! ===");
    Ok(())
}

/// Demo migrator that creates a simple example schema
struct DemoMigrator {
    pool: SqlitePool,
}

impl DemoMigrator {
    fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl Migrator for DemoMigrator {
    async fn get_current_version(&self) -> Result<u32, Box<dyn Error + Send + Sync>> {
        // Create version table if it doesn't exist
        sqlx::query("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY)")
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
            None => Ok(0),
        }
    }
    
    async fn set_version(&self, version: u32) -> Result<(), Box<dyn Error + Send + Sync>> {
        sqlx::query("DELETE FROM schema_version").execute(&self.pool).await?;
        sqlx::query("INSERT INTO schema_version (version) VALUES (?)")
            .bind(version as i64)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    
    fn get_migrations(&self) -> Vec<Box<dyn Migration>> {
        vec![
            Box::new(DemoMigration001::new(self.pool.clone())),
            Box::new(DemoMigration002::new(self.pool.clone())),
        ]
    }
}

/// First demo migration - creates a users table
struct DemoMigration001 {
    pool: SqlitePool,
}

impl DemoMigration001 {
    fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl Migration for DemoMigration001 {
    fn target_version(&self) -> u32 {
        1
    }
    
    fn description(&self) -> &str {
        "Create users table"
    }
    
    async fn apply(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        println!("   Applying migration 1: {}", self.description());
        sqlx::query("CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY,
            username TEXT UNIQUE NOT NULL,
            email TEXT NOT NULL
        )").execute(&self.pool).await?;
        Ok(())
    }
}

/// Second demo migration - adds an index for performance
struct DemoMigration002 {
    pool: SqlitePool,
}

impl DemoMigration002 {
    fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl Migration for DemoMigration002 {
    fn target_version(&self) -> u32 {
        2
    }
    
    fn description(&self) -> &str {
        "Add index on users.username for performance"
    }
    
    async fn apply(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        println!("   Applying migration 2: {}", self.description());
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_users_username ON users(username)")
            .execute(&self.pool).await?;
        Ok(())
    }
}