use super::{AuthProvider, async_trait};
use crate::migrations::Migrator;
use anyhow::Result;
use argon2::password_hash::{SaltString, rand_core::OsRng};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::str::FromStr;

// SQL schemas for SQLite authentication
const USERS_TABLE: &str = "CREATE TABLE IF NOT EXISTS users (
        username TEXT PRIMARY KEY,
        password_hash TEXT NOT NULL,
        key TEXT
    )";

const ADMINS_TABLE: &str = "CREATE TABLE IF NOT EXISTS admins (
        username TEXT PRIMARY KEY REFERENCES users(username)
    )";

const MODERATORS_TABLE: &str = "CREATE TABLE IF NOT EXISTS moderators (
        username TEXT REFERENCES users(username),
        pattern TEXT,
        PRIMARY KEY(username, pattern)
    )";

#[derive(Clone)]
pub struct SqliteAuth {
    pool: SqlitePool,
}

impl SqliteAuth {
    /// Create a new `SQLite` authentication provider.
    ///
    /// # Errors
    ///
    /// Returns an error if the database connection fails or schema creation fails.
    pub async fn new(path: &str) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(path)
            .map_err(|e| {
                anyhow::anyhow!(
                    "Invalid SQLite authentication database URI '{path}': {e}

Please ensure the URI is in the correct format:
- File database: sqlite:///path/to/auth.db
- In-memory database: sqlite::memory:
- Relative path: sqlite://relative/path.db"
                )
            })?
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to connect to SQLite authentication database '{path}': {e}

Possible causes:
- Parent directory does not exist and cannot be created
- Permission denied accessing the database file or directory
- Database file is corrupted or not a valid SQLite database
- Path contains invalid characters for the filesystem
- Disk space is full
- Database is locked by another process"
                )
            })?;

        // Set up migrator to check database state
        let migrator = super::migrations::sqlite::SqliteAuthMigrator::new(pool.clone());

        if migrator.is_fresh_database().await {
            // Fresh database: initialize with current schema
            tracing::info!(
                "Initializing fresh SQLite authentication database at '{}'",
                path
            );

            // Create authentication schema
            sqlx::query(USERS_TABLE).execute(&pool).await.map_err(|e| {
                anyhow::anyhow!(
                    "Failed to create users table in SQLite authentication database '{path}': {e}"
                )
            })?;
            sqlx::query(ADMINS_TABLE).execute(&pool).await.map_err(|e| {
                anyhow::anyhow!("Failed to create admins table in SQLite authentication database '{path}': {e}")
            })?;
            sqlx::query(MODERATORS_TABLE).execute(&pool).await.map_err(|e| {
                anyhow::anyhow!("Failed to create moderators table in SQLite authentication database '{path}': {e}")
            })?;

            // Set current version (since pre-1.0, we use version 1 as the baseline)
            migrator.set_version(1).await.map_err(|e| {
                anyhow::anyhow!(
                    "Failed to set initial schema version for SQLite auth database '{path}': {e}"
                )
            })?;

            tracing::info!("Successfully initialized SQLite authentication database at version 1");
        } else {
            // Existing database: apply any pending migrations
            tracing::info!(
                "Found existing SQLite authentication database, checking for migrations"
            );
            migrator.migrate_to_latest().await.map_err(|e| {
                anyhow::anyhow!("Failed to run auth migrations for SQLite database '{path}': {e}")
            })?;
        }

        Ok(Self { pool })
    }
}

#[async_trait]
impl AuthProvider for SqliteAuth {
    async fn add_user(&self, username: &str, password: &str) -> Result<()> {
        self.add_user_with_key(username, password, None).await
    }

    async fn add_user_with_key(
        &self,
        username: &str,
        password: &str,
        key: Option<&str>,
    ) -> Result<()> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)?
            .to_string();
        sqlx::query("INSERT OR REPLACE INTO users (username, password_hash, key) VALUES (?, ?, ?)")
            .bind(username)
            .bind(hash)
            .bind(key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_password(&self, username: &str, new_password: &str) -> Result<()> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(new_password.as_bytes(), &salt)?
            .to_string();
        sqlx::query("UPDATE users SET password_hash = ? WHERE username = ?")
            .bind(hash)
            .bind(username)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn remove_user(&self, username: &str) -> Result<()> {
        sqlx::query("DELETE FROM users WHERE username = ?")
            .bind(username)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM admins WHERE username = ?")
            .bind(username)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM moderators WHERE username = ?")
            .bind(username)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn verify_user(&self, username: &str, password: &str) -> Result<bool> {
        if let Some(row) = sqlx::query("SELECT password_hash FROM users WHERE username = ?")
            .bind(username)
            .fetch_optional(&self.pool)
            .await?
        {
            let stored: String = row.get(0);
            let parsed = PasswordHash::new(&stored)?;
            Ok(Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok())
        } else {
            Ok(false)
        }
    }

    async fn is_admin(&self, username: &str) -> Result<bool> {
        let row = sqlx::query("SELECT 1 FROM admins WHERE username = ?")
            .bind(username)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.is_some())
    }

    async fn add_admin(&self, username: &str, key: &str) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO admins (username) VALUES (?)")
            .bind(username)
            .execute(&self.pool)
            .await?;
        sqlx::query("UPDATE users SET key = ? WHERE username = ?")
            .bind(key)
            .bind(username)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn add_admin_without_key(&self, username: &str) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO admins (username) VALUES (?)")
            .bind(username)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn remove_admin(&self, username: &str) -> Result<()> {
        sqlx::query("DELETE FROM admins WHERE username = ?")
            .bind(username)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_pgp_key(&self, username: &str, key: &str) -> Result<()> {
        sqlx::query("UPDATE users SET key = ? WHERE username = ?")
            .bind(key)
            .bind(username)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn get_pgp_key(&self, username: &str) -> Result<Option<String>> {
        if let Some(row) = sqlx::query("SELECT key FROM users WHERE username = ?")
            .bind(username)
            .fetch_optional(&self.pool)
            .await?
        {
            let k: Option<String> = row.try_get("key")?;
            Ok(k)
        } else {
            Ok(None)
        }
    }

    async fn add_moderator(&self, username: &str, pattern: &str) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO moderators (username, pattern) VALUES (?, ?)")
            .bind(username)
            .bind(pattern)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn remove_moderator(&self, username: &str, pattern: &str) -> Result<()> {
        sqlx::query("DELETE FROM moderators WHERE username = ? AND pattern = ?")
            .bind(username)
            .bind(pattern)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn is_moderator(&self, username: &str, group: &str) -> Result<bool> {
        let rows = sqlx::query("SELECT pattern FROM moderators WHERE username = ?")
            .bind(username)
            .fetch_all(&self.pool)
            .await?;
        for row in rows {
            let pat: String = row.try_get("pattern")?;
            if crate::wildmat::wildmat(&pat, group) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
