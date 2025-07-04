use std::error::Error;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use rustls_pemfile::{certs, pkcs8_private_keys};
use std::fs::File;
use std::io::BufReader;
use tokio::net::TcpListener;
use tokio_rustls::{TlsAcceptor, rustls};
use tracing::{error, info};

use renews::auth::{AuthProvider, sqlite::SqliteAuth};
use renews::config::Config;
use renews::retention::cleanup_expired_articles;
use renews::storage::Storage;
use renews::storage::sqlite::SqliteStorage;

#[derive(Parser)]
struct Args {
    /// Path to the configuration file
    #[arg(long, env = "RENEWS_CONFIG", default_value = "/etc/renews.toml")]
    config: String,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Administrative actions
    #[command(subcommand)]
    Admin(AdminCommand),
}

#[derive(Subcommand)]
enum AdminCommand {
    /// Add a newsgroup
    AddGroup { group: String },
    /// Remove a newsgroup
    RemoveGroup { group: String },
    /// Add a user
    AddUser { username: String, password: String },
    /// Remove a user
    RemoveUser { username: String },
    /// Grant admin privileges to a user
    AddAdmin { username: String },
    /// Revoke admin privileges from a user
    RemoveAdmin { username: String },
}

fn load_tls_config(
    cert_path: &str,
    key_path: &str,
) -> Result<rustls::ServerConfig, Box<dyn Error + Send + Sync>> {
    let cert_file = &mut BufReader::new(File::open(cert_path)?);
    let key_file = &mut BufReader::new(File::open(key_path)?);
    let certs = certs(cert_file)?
        .into_iter()
        .map(rustls::Certificate)
        .collect();
    let mut keys = pkcs8_private_keys(key_file)?;
    if keys.is_empty() {
        return Err("no private key found".into());
    }
    let key = rustls::PrivateKey(keys.remove(0));
    let config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;
    Ok(config)
}

async fn run_admin(cmd: AdminCommand, cfg: &Config) -> Result<(), Box<dyn Error + Send + Sync>> {
    let db_conn = format!("sqlite:{}", cfg.db_path);
    let storage = SqliteStorage::new(&db_conn).await?;
    let auth_path = cfg.auth_db_path.as_deref().unwrap_or(&cfg.db_path);
    let auth_conn = format!("sqlite:{}", auth_path);
    let auth = SqliteAuth::new(&auth_conn).await?;
    match cmd {
        AdminCommand::AddGroup { group } => storage.add_group(&group).await?,
        AdminCommand::RemoveGroup { group } => storage.remove_group(&group).await?,
        AdminCommand::AddUser { username, password } => auth.add_user(&username, &password).await?,
        AdminCommand::RemoveUser { username } => auth.remove_user(&username).await?,
        AdminCommand::AddAdmin { username } => auth.add_admin(&username).await?,
        AdminCommand::RemoveAdmin { username } => auth.remove_admin(&username).await?,
    }
    Ok(())
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    let cfg = Config::from_file(&args.config)?;

    if let Some(Command::Admin(cmd)) = args.command {
        run_admin(cmd, &cfg).await?;
        return Ok(());
    }
    let db_conn = format!("sqlite:{}", cfg.db_path);
    let storage: Arc<dyn Storage> = Arc::new(SqliteStorage::new(&db_conn).await?);
    let auth_path = cfg.auth_db_path.as_deref().unwrap_or(&cfg.db_path);
    let auth_conn = format!("sqlite:{}", auth_path);
    let auth: Arc<dyn AuthProvider> = Arc::new(SqliteAuth::new(&auth_conn).await?);
    let addr = format!("127.0.0.1:{}", cfg.port);
    info!("listening on {addr}");
    let listener = TcpListener::bind(&addr).await?;
    let storage_clone = storage.clone();
    let auth_clone = auth.clone();
    let cfg_clone = cfg.clone();
    tokio::spawn(async move {
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            info!("accepted connection");
            let st = storage_clone.clone();
            let au = auth_clone.clone();
            let cfg = cfg_clone.clone();
            tokio::spawn(async move {
                if let Err(e) = renews::handle_client(socket, st, au, cfg, false).await {
                    error!("client error: {e}");
                }
            });
        }
    });

    if let (Some(tls_port), Some(cert), Some(key)) =
        (cfg.tls_port, cfg.tls_cert.as_ref(), cfg.tls_key.as_ref())
    {
        let tls_addr = format!("127.0.0.1:{}", tls_port);
        info!("listening TLS on {tls_addr}");
        let tls_listener = TcpListener::bind(&tls_addr).await?;
        let tls_config = TlsAcceptor::from(Arc::new(load_tls_config(cert, key)?));
        let storage_clone = storage.clone();
        let auth_clone = auth.clone();
        let cfg_clone = cfg.clone();
        tokio::spawn(async move {
            loop {
                let (socket, _) = tls_listener.accept().await.unwrap();
                info!("accepted TLS connection");
                let acceptor = tls_config.clone();
                let st = storage_clone.clone();
                let au = auth_clone.clone();
                let cfg = cfg_clone.clone();
                tokio::spawn(async move {
                    match acceptor.accept(socket).await {
                        Ok(stream) => {
                            if let Err(e) = renews::handle_client(stream, st, au, cfg, true).await {
                                error!("client error: {e}");
                            }
                        }
                        Err(e) => error!("tls error: {e}"),
                    }
                });
            }
        });
    }

    let storage_clone = storage.clone();
    let cfg_clone = cfg.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = cleanup_expired_articles(&*storage_clone, &cfg_clone).await {
                error!("retention cleanup error: {e}");
            }
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        }
    });

    tokio::signal::ctrl_c().await?;
    info!("shutdown signal received");
    Ok(())
}
