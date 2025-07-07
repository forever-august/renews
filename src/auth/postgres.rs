use super::{AuthProvider, Error, async_trait};
use argon2::password_hash::{SaltString, rand_core::OsRng};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::error::Error;

#[derive(Clone)]
pub struct PostgresAuth {
    pool: PgPool,
}

impl PostgresAuth {
    /// Create a new Postgres authentication provider.
    ///
    /// # Errors
    ///
    /// Returns an error if the database connection fails or schema creation fails.
    pub async fn new(uri: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let pool = PgPoolOptions::new().max_connections(5).connect(uri).await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS users (\n                    username TEXT PRIMARY KEY,\n                    password_hash TEXT NOT NULL,\n                    key TEXT\n                )",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS admins (\n                    username TEXT PRIMARY KEY REFERENCES users(username)\n                )",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS moderators (\n                    username TEXT REFERENCES users(username),\n                    pattern TEXT,\n                PRIMARY KEY(username, pattern)\n                )",
        )
        .execute(&pool)
        .await?;
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
        sqlx::query("INSERT INTO users (username, password_hash) VALUES ($1, $2) ON CONFLICT(username) DO UPDATE SET password_hash = EXCLUDED.password_hash")
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
        sqlx::query("INSERT INTO admins (username) VALUES ($1) ON CONFLICT DO NOTHING")
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
        sqlx::query("INSERT INTO moderators (username, pattern) VALUES ($1, $2) ON CONFLICT (username, pattern) DO NOTHING")
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
