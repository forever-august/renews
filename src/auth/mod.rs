use async_trait::async_trait;
use std::error::Error;
use std::sync::Arc;

#[async_trait]
pub trait AuthProvider: Send + Sync {
    async fn add_user(
        &self,
        username: &str,
        password: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn remove_user(&self, username: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn verify_user(
        &self,
        username: &str,
        password: &str,
    ) -> Result<bool, Box<dyn Error + Send + Sync>>;
    async fn is_admin(&self, username: &str) -> Result<bool, Box<dyn Error + Send + Sync>>;
    async fn add_admin(
        &self,
        username: &str,
        key: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn remove_admin(&self, username: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn update_pgp_key(
        &self,
        username: &str,
        key: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn get_pgp_key(
        &self,
        username: &str,
    ) -> Result<Option<String>, Box<dyn Error + Send + Sync>>;
    async fn add_moderator(
        &self,
        username: &str,
        pattern: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn remove_moderator(
        &self,
        username: &str,
        pattern: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn is_moderator(
        &self,
        username: &str,
        group: &str,
    ) -> Result<bool, Box<dyn Error + Send + Sync>>;
}

pub type DynAuth = Arc<dyn AuthProvider>;

pub mod migrations;
pub mod pgp_discovery;
#[cfg(feature = "postgres")]
pub mod postgres;
pub mod sqlite;

/// Create an authentication backend from a connection URI.
pub async fn open(uri: &str) -> Result<DynAuth, Box<dyn Error + Send + Sync>> {
    if uri.starts_with("sqlite:") {
        sqlite::SqliteAuth::new(uri).await
            .map(|a| Arc::new(a) as DynAuth)
            .map_err(|e| {
                format!(
                    "Failed to connect to SQLite authentication database '{uri}': {e}

Common SQLite connection issues:
- Directory does not exist (SQLite will create the file but not directories)
- Permission denied accessing the database file or directory
- Database file is corrupted
- Path contains invalid characters
- Database is locked by another process

For SQLite URIs:
- Use format: sqlite:///path/to/database.db
- For in-memory database: sqlite::memory:
- Relative paths are relative to the working directory

You can change the authentication database path in your configuration file using the 'auth_db_path' setting."
                ).into()
            })
    } else if uri.starts_with("postgres:") {
        #[cfg(feature = "postgres")]
        {
            postgres::PostgresAuth::new(uri).await
                .map(|a| Arc::new(a) as DynAuth)
                .map_err(|e| {
                    format!(
                        "Failed to connect to PostgreSQL authentication database '{uri}': {e}

Common PostgreSQL connection issues:
- PostgreSQL server is not running
- Incorrect hostname, port, database name, username, or password
- Database does not exist (must be created manually)
- Network connectivity issues
- Authentication method not supported
- Connection limit exceeded
- SSL/TLS configuration issues

For PostgreSQL URIs, use format:
postgres://username:password@host:port/database

You can change the authentication database URI in your configuration file using the 'auth_db_path' setting."
                    ).into()
                })
        }
        #[cfg(not(feature = "postgres"))]
        {
            Err(format!(
                "PostgreSQL backend not enabled: '{uri}'

The renews server was compiled without PostgreSQL support.
To use PostgreSQL:
1. Rebuild with: cargo build --features postgres
2. Or use SQLite instead by changing 'auth_db_path' to a sqlite:// URI in your configuration"
            ).into())
        }
    } else {
        Err(format!(
            "Unknown authentication backend: '{uri}'

Supported database backends:
- SQLite: sqlite:///path/to/database.db
- PostgreSQL: postgres://user:pass@host:port/database (requires --features postgres)

You can change the authentication database URI in your configuration file using the 'auth_db_path' setting."
        ).into())
    }
}
