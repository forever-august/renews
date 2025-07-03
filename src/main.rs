use std::error::Error;
use std::sync::Arc;

use clap::Parser;
use rustls_pemfile::{certs, pkcs8_private_keys};
use std::fs::File;
use std::io::BufReader;
use tokio::net::TcpListener;
use tokio_rustls::{TlsAcceptor, rustls};

use renews::config::Config;
use renews::retention::cleanup_expired_articles;
use renews::storage::Storage;
use renews::storage::sqlite::SqliteStorage;
use renews::auth::{AuthProvider, sqlite::SqliteAuth};

#[derive(Parser)]
struct Args {
    /// Path to the configuration file
    #[arg(long, default_value = "/etc/renews.toml")]
    config: String,
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

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = Args::parse();
    let cfg = Config::from_file(&args.config)?;
    let db_conn = format!("sqlite:{}", cfg.db_path);
    let storage: Arc<dyn Storage> = Arc::new(SqliteStorage::new(&db_conn).await?);
    let auth: Arc<dyn AuthProvider> = Arc::new(SqliteAuth::new(&db_conn).await?);
    for g in &cfg.groups {
        storage.add_group(g).await?;
    }
    let addr = format!("127.0.0.1:{}", cfg.port);
    let listener = TcpListener::bind(&addr).await?;
    let storage_clone = storage.clone();
    let auth_clone = auth.clone();
    let cfg_clone = cfg.clone();
    tokio::spawn(async move {
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            let st = storage_clone.clone();
            let au = auth_clone.clone();
            let cfg = cfg_clone.clone();
            tokio::spawn(async move {
                if let Err(e) = renews::handle_client(socket, st, au, cfg, false).await {
                    eprintln!("client error: {e}");
                }
            });
        }
    });

    if let (Some(tls_port), Some(cert), Some(key)) =
        (cfg.tls_port, cfg.tls_cert.as_ref(), cfg.tls_key.as_ref())
    {
        let tls_addr = format!("127.0.0.1:{}", tls_port);
        let tls_listener = TcpListener::bind(&tls_addr).await?;
        let tls_config = TlsAcceptor::from(Arc::new(load_tls_config(cert, key)?));
        let storage_clone = storage.clone();
        let auth_clone = auth.clone();
        let cfg_clone = cfg.clone();
        tokio::spawn(async move {
            loop {
                let (socket, _) = tls_listener.accept().await.unwrap();
                let acceptor = tls_config.clone();
                let st = storage_clone.clone();
                let au = auth_clone.clone();
                let cfg = cfg_clone.clone();
                tokio::spawn(async move {
                    match acceptor.accept(socket).await {
                        Ok(stream) => {
                            if let Err(e) = renews::handle_client(stream, st, au, cfg, true).await {
                                eprintln!("client error: {e}");
                            }
                        }
                        Err(e) => eprintln!("tls error: {e}"),
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
                eprintln!("retention cleanup error: {e}");
            }
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        }
    });

    tokio::signal::ctrl_c().await?;
    Ok(())
}
