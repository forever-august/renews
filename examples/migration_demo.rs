use renews::migrations::{Migration, Migrator};
use sqlx::{Row, SqlitePool};
use std::error::Error;
use tempfile::NamedTempFile;

/// Example demonstrating the migration system concept
/// 
/// Note: Since Renews is pre-1.0, no actual migrations exist yet.
/// This demo shows how the migration system would work with example migrations.
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    println!("=== Renews Database Migration System Demo ===\n");

    // Create a temporary database for the demo
    let temp_file = NamedTempFile::new()?;
    let db_path = format!("sqlite://{}", temp_file.path().display());
    let pool = SqlitePool::connect(&db_path).await?;

    println!("1. Created temporary SQLite database at: {}", temp_file.path().display());

    // Create a demo migrator with sample migrations (not the real one)
    let migrator = DemoMigrator::new(pool.clone());

    // Check if this is a fresh database
    if migrator.is_fresh_database().await {
        println!("2. Detected fresh database - initializing with current schema");
        
        // In a real scenario, the application would create the baseline schema here
        sqlx::query("CREATE TABLE IF NOT EXISTS users (username TEXT PRIMARY KEY, password_hash TEXT NOT NULL)").execute(&pool).await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS groups (name TEXT PRIMARY KEY, created_at INTEGER NOT NULL)").execute(&pool).await?;
        
        // Set the initial version (simulating what the backend would do)
        migrator.set_version(2).await?;
        
        println!("   - Created baseline schema at version 2");
    } else {
        // Existing database: apply any pending migrations
        println!("2. Found existing database, checking for migrations");
        migrator.migrate_to_latest().await?;
    }

    // Show final state
    let final_version = migrator.get_current_version().await?;
    println!("3. Final schema version: {final_version}");

    // Show that running again is idempotent
    println!("4. Running migrations again (should be idempotent)...");
    migrator.migrate_to_latest().await?;
    let version_after_second_run = migrator.get_current_version().await?;
    println!("5. Version after second run: {version_after_second_run}");

    // Demonstrate the created schema
    println!("6. Schema verification:");
    let tables = sqlx::query("SELECT name FROM sqlite_master WHERE type='table'")
        .fetch_all(&pool)
        .await?;
    
    for row in tables {
        let table_name: String = row.get("name");
        println!("   ✓ Table: {table_name}");
    }

    println!("\n=== Demo completed successfully! ===");
    println!("\nNote: This demo shows the migration system concept.");
    println!("Since Renews is pre-1.0, no actual migrations exist yet.");
    println!("The real auth and storage backends use stub migrations that do nothing.");

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
                Err("Version table does not exist".into())
            }
        }
    }
    
    async fn set_version(&self, version: u32) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Ensure version table exists first
        sqlx::query("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY)")
            .execute(&self.pool)
            .await?;
            
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