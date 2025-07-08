use async_trait::async_trait;
use std::error::Error;
use std::sync::Arc;

#[async_trait]
pub trait AuthProvider: Send + Sync {
    async fn add_user(&self, username: &str, password: &str)
        -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn remove_user(&self, username: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn verify_user(&self, username: &str, password: &str)
        -> Result<bool, Box<dyn Error + Send + Sync>>;
    async fn is_admin(&self, username: &str) -> Result<bool, Box<dyn Error + Send + Sync>>;
    async fn add_admin(&self, username: &str, key: &str)
        -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn remove_admin(&self, username: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn update_pgp_key(&self, username: &str, key: &str)
        -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn get_pgp_key(&self, username: &str)
        -> Result<Option<String>, Box<dyn Error + Send + Sync>>;
    async fn add_moderator(&self, username: &str, pattern: &str)
        -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn remove_moderator(&self, username: &str, pattern: &str)
        -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn is_moderator(&self, username: &str, group: &str)
        -> Result<bool, Box<dyn Error + Send + Sync>>;
}

pub type DynAuth = Arc<dyn AuthProvider>;

mod common;
pub mod sqlite;
#[cfg(feature = "postgres")]
pub mod postgres;

/// Create an authentication backend from a connection URI.
pub async fn open(uri: &str) -> Result<DynAuth, Box<dyn Error + Send + Sync>> {
    if uri.starts_with("sqlite:") {
        Ok(Arc::new(sqlite::SqliteAuth::new(uri).await?))
    } else if uri.starts_with("postgres:") {
        #[cfg(feature = "postgres")]
        {
            Ok(Arc::new(postgres::PostgresAuth::new(uri).await?))
        }
        #[cfg(not(feature = "postgres"))]
        {
            Err("postgres backend not enabled".into())
        }
    } else {
        Err("unknown auth backend".into())
    }
}
