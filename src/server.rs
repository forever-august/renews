use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio_rustls::{TlsAcceptor, rustls};
use tracing::{error, info};
use std::net::SocketAddr;

use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::RwLock;

use crate::auth::{AuthProvider, sqlite::SqliteAuth};
use crate::config::Config;
use crate::peers::{PeerConfig, PeerDb, peer_task};
use crate::retention::cleanup_expired_articles;
use crate::storage::{self, Storage};
#[cfg(feature = "websocket")]
use crate::ws;
use rustls_pemfile::{certs, pkcs8_private_keys};

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

fn listen_addr(raw: &str) -> String {
    if raw.parse::<SocketAddr>().is_ok() {
        raw.to_string()
    } else if let Some(port) = raw.strip_prefix(':') {
        format!("0.0.0.0:{port}")
    } else {
        format!("0.0.0.0:{raw}")
    }
}


#[allow(clippy::too_many_lines)]
pub async fn run(
    cfg_initial: Config,
    cfg_path: String,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let cfg = Arc::new(RwLock::new(cfg_initial));
    let tls_acceptor: Arc<RwLock<Option<TlsAcceptor>>> = Arc::new(RwLock::new(None));
    let storage: Arc<dyn Storage> = {
        let cfg_guard = cfg.read().await;
        storage::open(&cfg_guard.db_path).await?
    };
    let auth_path = {
        let cfg_guard = cfg.read().await;
        cfg_guard.auth_db_path.clone()
    };
    let auth: Arc<dyn AuthProvider> = Arc::new(SqliteAuth::new(&auth_path).await?);
    let peer_db = {
        let cfg_guard = cfg.read().await;
        PeerDb::new(&cfg_guard.peer_db_path).await?
    };
    {
        let cfg_guard = cfg.read().await;
        let names: Vec<String> = cfg_guard.peers.iter().map(|p| p.sitename.clone()).collect();
        peer_db.sync_config(&names).await?;
    }
    let peer_tasks: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>> =
        Arc::new(RwLock::new(HashMap::new()));
    {
        let cfg_guard = cfg.read().await;
        let default_interval = cfg_guard.peer_sync_secs;
        for peer in &cfg_guard.peers {
            let pc = PeerConfig::from(peer);
            let name = pc.sitename.clone();
            let db_clone = peer_db.clone();
            let storage_clone = storage.clone();
            let site = cfg_guard.site_name.clone();
            let handle = tokio::spawn(peer_task(
                pc.clone(),
                default_interval,
                db_clone,
                storage_clone,
                site,
            ));
            peer_tasks.write().await.insert(name, handle);
        }
    }
    let addr = {
        let cfg_guard = cfg.read().await;
        listen_addr(&cfg_guard.addr)
    };
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
                if let Err(e) = crate::handle_client(socket, st, au, cfg, false).await {
                    error!("client error: {e}");
                }
            });
        }
    });

    {
        let cfg_guard = cfg.read().await;
        if let (Some(tls_addr_raw), Some(cert), Some(key)) = (
            cfg_guard.tls_addr.as_deref(),
            cfg_guard.tls_cert.as_ref(),
            cfg_guard.tls_key.as_ref(),
        ) {
            let tls_addr = listen_addr(tls_addr_raw);
            info!("listening TLS on {tls_addr}");
            let tls_listener = TcpListener::bind(&tls_addr).await?;
            let acceptor = TlsAcceptor::from(Arc::new(load_tls_config(cert, key)?));
            *tls_acceptor.write().await = Some(acceptor.clone());
            let storage_clone = storage.clone();
            let auth_clone = auth.clone();
            let cfg_clone = cfg.clone();
            let acceptor_handle = tls_acceptor.clone();
            tokio::spawn(async move {
                loop {
                    let (socket, _) = tls_listener.accept().await.unwrap();
                    info!("accepted TLS connection");
                    let st = storage_clone.clone();
                    let au = auth_clone.clone();
                    let cfg = cfg_clone.clone();
                    let acceptor_opt = { acceptor_handle.read().await.clone() };
                    tokio::spawn(async move {
                        if let Some(acc) = acceptor_opt {
                            match acc.accept(socket).await {
                                Ok(stream) => {
                                    if let Err(e) =
                                        crate::handle_client(stream, st, au, cfg, true).await
                                    {
                                        error!("client error: {e}");
                                    }
                                }
                                Err(e) => error!("tls error: {e}"),
                            }
                        }
                    });
                }
            });
        }

        #[cfg(feature = "websocket")]
        {
            let cfg_ws = cfg.clone();
            if let Some(addr_raw) = cfg.read().await.ws_addr.as_deref() {
                info!("websocket bridge on {addr_raw}");
                tokio::spawn(async move {
                    if let Err(e) = ws::run_ws_bridge(cfg_ws).await {
                        error!("websocket error: {e}");
                    }
                });
            }
        }

        let storage_clone = storage.clone();
        let cfg_clone = cfg.clone();
        tokio::spawn(async move {
            loop {
                let cfg_guard = cfg_clone.read().await;
                if let Err(e) = cleanup_expired_articles(&*storage_clone, &cfg_guard).await {
                    error!("retention cleanup error: {e}");
                }
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            }
        });

        let cfg_reload = cfg.clone();
        let tls_reload = tls_acceptor.clone();
        let cfg_path_reload = cfg_path.clone();
        let peer_db_reload = peer_db.clone();
        let peer_tasks_reload = peer_tasks.clone();
        let storage_reload = storage.clone();
        tokio::spawn(async move {
            if let Ok(mut hup) = signal(SignalKind::hangup()) {
                while hup.recv().await.is_some() {
                    match Config::from_file(&cfg_path_reload) {
                        Ok(new_cfg) => {
                            if let (Some(cert), Some(key)) =
                                (new_cfg.tls_cert.as_ref(), new_cfg.tls_key.as_ref())
                            {
                                match load_tls_config(cert, key) {
                                    Ok(conf) => {
                                        *tls_reload.write().await =
                                            Some(TlsAcceptor::from(Arc::new(conf)));
                                    }
                                    Err(e) => error!("failed to load tls config: {e}"),
                                }
                            }
                            {
                                let names: Vec<String> =
                                    new_cfg.peers.iter().map(|p| p.sitename.clone()).collect();
                                if let Err(e) = peer_db_reload.sync_config(&names).await {
                                    error!("peer db sync error: {e}");
                                }
                                let mut tasks = peer_tasks_reload.write().await;
                                let default_interval = new_cfg.peer_sync_secs;
                                for peer in &new_cfg.peers {
                                    if !tasks.contains_key(&peer.sitename) {
                                        let dbc = peer_db_reload.clone();
                                        let pc = PeerConfig::from(peer);
                                        let name = pc.sitename.clone();
                                        let storage_clone = storage_reload.clone();
                                        let site = new_cfg.site_name.clone();
                                        let handle = tokio::spawn(peer_task(
                                            pc.clone(),
                                            default_interval,
                                            dbc,
                                            storage_clone,
                                            site,
                                        ));
                                        tasks.insert(name, handle);
                                    }
                                }
                                let to_remove: Vec<String> = tasks
                                    .keys()
                                    .filter(|k| !new_cfg.peers.iter().any(|p| &p.sitename == *k))
                                    .cloned()
                                    .collect();
                                for name in to_remove {
                                    if let Some(h) = tasks.remove(&name) {
                                        h.abort();
                                    }
                                }
                            }
                            cfg_reload.write().await.update_runtime(new_cfg);
                            info!("configuration reloaded");
                        }
                        Err(e) => {
                            error!("failed to reload config: {e}");
                        }
                    }
                }
            }
        });

        tokio::signal::ctrl_c().await?;
        info!("shutdown signal received");
        Ok(())
    }
}
