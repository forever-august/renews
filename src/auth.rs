use async_trait::async_trait;
use std::error::Error;
use std::sync::Arc;

#[async_trait]
pub trait AuthProvider: Send + Sync {
    async fn add_user(&self, username: &str, password: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn remove_user(&self, username: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn verify_user(&self, username: &str, password: &str) -> Result<bool, Box<dyn Error + Send + Sync>>;
    async fn is_admin(&self, username: &str) -> Result<bool, Box<dyn Error + Send + Sync>>;
    async fn add_admin(&self, username: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn remove_admin(&self, username: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
}

pub type DynAuth = Arc<dyn AuthProvider>;

pub mod sqlite {
    use super::*;
    use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
    use argon2::password_hash::{SaltString, rand_core::OsRng};
    use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};

    #[derive(Clone)]
    pub struct SqliteAuth {
        pool: SqlitePool,
    }

    impl SqliteAuth {
        pub async fn new(path: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
            let pool = SqlitePoolOptions::new()
                .max_connections(5)
                .connect(path)
                .await?;
            sqlx::query(
                "CREATE TABLE IF NOT EXISTS users (\n                    username TEXT PRIMARY KEY,\n                    password_hash TEXT NOT NULL\n                )",
            )
            .execute(&pool)
            .await?;
            sqlx::query(
                "CREATE TABLE IF NOT EXISTS admins (\n                    username TEXT PRIMARY KEY REFERENCES users(username)\n                )",
            )
            .execute(&pool)
            .await?;
            Ok(Self { pool })
        }
    }

    #[async_trait]
    impl AuthProvider for SqliteAuth {
        async fn add_user(&self, username: &str, password: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
            let salt = SaltString::generate(&mut OsRng);
            let hash = Argon2::default()
                .hash_password(password.as_bytes(), &salt)?
                .to_string();
            sqlx::query("INSERT OR REPLACE INTO users (username, password_hash) VALUES (?, ?)")
                .bind(username)
                .bind(hash)
                .execute(&self.pool)
                .await?;
            Ok(())
        }

        async fn remove_user(&self, username: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
            sqlx::query("DELETE FROM users WHERE username = ?")
                .bind(username)
                .execute(&self.pool)
                .await?;
            sqlx::query("DELETE FROM admins WHERE username = ?")
                .bind(username)
                .execute(&self.pool)
                .await?;
            Ok(())
        }

        async fn verify_user(&self, username: &str, password: &str) -> Result<bool, Box<dyn Error + Send + Sync>> {
            if let Some(row) = sqlx::query("SELECT password_hash FROM users WHERE username = ?")
                .bind(username)
                .fetch_optional(&self.pool)
                .await? {
                let stored: String = row.get(0);
                let parsed = PasswordHash::new(&stored)?;
                Ok(Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
            } else {
                Ok(false)
            }
        }

        async fn is_admin(&self, username: &str) -> Result<bool, Box<dyn Error + Send + Sync>> {
            let row = sqlx::query("SELECT 1 FROM admins WHERE username = ?")
                .bind(username)
                .fetch_optional(&self.pool)
                .await?;
            Ok(row.is_some())
        }

        async fn add_admin(&self, username: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
            sqlx::query("INSERT OR IGNORE INTO admins (username) VALUES (?)")
                .bind(username)
                .execute(&self.pool)
                .await?;
            Ok(())
        }

        async fn remove_admin(&self, username: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
            sqlx::query("DELETE FROM admins WHERE username = ?")
                .bind(username)
                .execute(&self.pool)
                .await?;
            Ok(())
        }
    }
}

