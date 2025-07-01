use std::error::Error;
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio_rustls::{rustls, TlsAcceptor};
use rustls_pemfile::{certs, pkcs8_private_keys};
use std::fs::File;
use std::io::BufReader;

use renews::storage::sqlite::SqliteStorage;
use renews::storage::Storage;
use renews::config::Config;

fn load_tls_config(cert_path: &str, key_path: &str) -> Result<rustls::ServerConfig, Box<dyn Error + Send + Sync>> {
    let cert_file = &mut BufReader::new(File::open(cert_path)?);
    let key_file = &mut BufReader::new(File::open(key_path)?);
    let certs = certs(cert_file)?.into_iter().map(rustls::Certificate).collect();
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
    let cfg = Config::from_file("config.toml")?;
    let db_conn = format!("sqlite:{}", cfg.db_path);
    let storage = Arc::new(SqliteStorage::new(&db_conn).await?);
    for g in &cfg.groups {
        storage.add_group(g).await?;
    }
    let addr = format!("127.0.0.1:{}", cfg.port);
    let listener = TcpListener::bind(&addr).await?;
    let storage_clone = storage.clone();
    tokio::spawn(async move {
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            let st = storage_clone.clone();
            tokio::spawn(async move {
                if let Err(e) = renews::handle_client(socket, st).await {
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
        tokio::spawn(async move {
            loop {
                let (socket, _) = tls_listener.accept().await.unwrap();
                let acceptor = tls_config.clone();
                let st = storage_clone.clone();
                tokio::spawn(async move {
                    match acceptor.accept(socket).await {
                        Ok(stream) => {
                            if let Err(e) = renews::handle_client(stream, st).await {
                                eprintln!("client error: {e}");
                            }
                        }
                        Err(e) => eprintln!("tls error: {e}"),
                    }
                });
            }
        });
    }

    tokio::signal::ctrl_c().await?;
    Ok(())
}


