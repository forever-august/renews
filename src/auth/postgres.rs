use super::{AuthProvider, Error, async_trait};
use crate::migrations::Migrator;
use argon2::password_hash::{SaltString, rand_core::OsRng};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::str::FromStr;

// SQL schemas for PostgreSQL authentication
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
pub struct PostgresAuth {
    pool: PgPool,
}

impl PostgresAuth {
    /// Create a new Postgres authentication provider.
    pub async fn new(uri: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let opts = PgConnectOptions::from_str(uri).map_err(|e| {
            format!(
                "Invalid PostgreSQL authentication database URI '{}': {}

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
                format!(
                    "Failed to connect to PostgreSQL authentication database '{}': {}

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
        let migrator = super::migrations::postgres::PostgresAuthMigrator::new(pool.clone());

        if migrator.is_fresh_database().await {
            // Fresh database: initialize with current schema
            tracing::info!(
                "Initializing fresh PostgreSQL authentication database at '{}'",
                uri
            );

            // Create authentication schema
            sqlx::query(USERS_TABLE).execute(&pool).await.map_err(|e| {
                format!(
                    "Failed to create users table in PostgreSQL authentication database '{}': {}",
                    uri, e
                )
            })?;
            sqlx::query(ADMINS_TABLE).execute(&pool).await.map_err(|e| {
                format!("Failed to create admins table in PostgreSQL authentication database '{}': {}", uri, e)
            })?;
            sqlx::query(MODERATORS_TABLE).execute(&pool).await.map_err(|e| {
                format!("Failed to create moderators table in PostgreSQL authentication database '{}': {}", uri, e)
            })?;

            // Set current version (since pre-1.0, we use version 1 as the baseline)
            migrator.set_version(1).await.map_err(|e| {
                format!(
                    "Failed to set initial schema version for PostgreSQL auth database '{}': {}",
                    uri, e
                )
            })?;

            tracing::info!(
                "Successfully initialized PostgreSQL authentication database at version 1"
            );
        } else {
            // Existing database: apply any pending migrations
            tracing::info!(
                "Found existing PostgreSQL authentication database, checking for migrations"
            );
            migrator.migrate_to_latest().await.map_err(|e| {
                format!(
                    "Failed to run auth migrations for PostgreSQL database '{}': {}",
                    uri, e
                )
            })?;
        }

        Ok(Self { pool })
    }
}

#[async_trait]
impl AuthProvider for PostgresAuth {
    async fn add_user(
        &self,
        username: &str,
        password: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)?
            .to_string();
        sqlx::query(
            "INSERT INTO users (username, password_hash) VALUES ($1, $2)\
            ON CONFLICT (username) DO UPDATE SET password_hash = EXCLUDED.password_hash",
        )
        .bind(username)
        .bind(hash)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn remove_user(&self, username: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        sqlx::query("DELETE FROM users WHERE username = $1")
            .bind(username)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM admins WHERE username = $1")
            .bind(username)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM moderators WHERE username = $1")
            .bind(username)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn verify_user(
        &self,
        username: &str,
        password: &str,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        if let Some(row) = sqlx::query("SELECT password_hash FROM users WHERE username = $1")
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

    async fn is_admin(&self, username: &str) -> Result<bool, Box<dyn Error + Send + Sync>> {
        let row = sqlx::query("SELECT 1 FROM admins WHERE username = $1")
            .bind(username)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.is_some())
    }

    async fn add_admin(
        &self,
        username: &str,
        key: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        sqlx::query("INSERT INTO admins (username) VALUES ($1) ON CONFLICT (username) DO NOTHING")
            .bind(username)
            .execute(&self.pool)
            .await?;
        sqlx::query("UPDATE users SET key = $1 WHERE username = $2")
            .bind(key)
            .bind(username)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn remove_admin(&self, username: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        sqlx::query("DELETE FROM admins WHERE username = $1")
            .bind(username)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_pgp_key(
        &self,
        username: &str,
        key: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        sqlx::query("UPDATE users SET key = $1 WHERE username = $2")
            .bind(key)
            .bind(username)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn get_pgp_key(
        &self,
        username: &str,
    ) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
        if let Some(row) = sqlx::query("SELECT key FROM users WHERE username = $1")
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

    async fn add_moderator(
        &self,
        username: &str,
        pattern: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        sqlx::query(
            "INSERT INTO moderators (username, pattern) VALUES ($1, $2)\
            ON CONFLICT (username, pattern) DO UPDATE SET pattern = EXCLUDED.pattern",
        )
        .bind(username)
        .bind(pattern)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn remove_moderator(
        &self,
        username: &str,
        pattern: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        sqlx::query("DELETE FROM moderators WHERE username = $1 AND pattern = $2")
            .bind(username)
            .bind(pattern)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn is_moderator(
        &self,
        username: &str,
        group: &str,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        let rows = sqlx::query("SELECT pattern FROM moderators WHERE username = $1")
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
