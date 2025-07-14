//! NNTP Server implementation
//!
//! This module contains the main server infrastructure for the NNTP server,
//! including TCP and TLS listeners, peer synchronization, and configuration reloading.
//!
//! ## Architecture
//!
//! The server is organized into several key components:
//!
//! - **Server**: Main server struct that orchestrates all components
//! - **ServerComponents**: Shared resources (storage, auth, config)
//! - **ConfigManager**: Handles configuration loading and TLS setup
//! - **PeerManager**: Manages peer synchronization tasks
//!
//! ## Key Features
//!
//! - Concurrent handling of TCP and TLS connections
//! - Hot configuration reloading via SIGHUP
//! - WebSocket bridge support (optional)
//! - Automatic peer synchronization
//! - Article retention cleanup
//!

use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;

use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_rustls::{TlsAcceptor, rustls};
use tracing::{error, info};

use dashmap::DashMap;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::RwLock;
use tokio_cron_scheduler::JobScheduler;

use crate::auth::{self, AuthProvider};
use crate::config::Config;
use crate::peers::{PeerConfig, PeerDb, add_peer_job};
use crate::queue::{ArticleQueue, WorkerPool};
use crate::retention::cleanup_expired_articles;
use crate::storage::{self, Storage};
#[cfg(feature = "websocket")]
use crate::ws;
use rustls_pemfile::{certs, pkcs8_private_keys};

type ServerResult<T> = anyhow::Result<T>;

/// Shared server components
#[derive(Clone)]
struct ServerComponents {
    storage: Arc<dyn Storage>,
    auth: Arc<dyn AuthProvider>,
    config: Arc<RwLock<Config>>,
    queue: ArticleQueue,
}

/// Server handles all lifecycle management
pub struct Server {
    components: ServerComponents,
    config_manager: ConfigManager,
    peer_manager: PeerManager,
    worker_pool: WorkerPool,
}

impl Server {
    /// Create a new server instance
    pub async fn new(cfg: Config) -> ServerResult<Self> {
        let components = Self::initialize_components(&cfg).await?;
        let peer_db = Self::initialize_peer_db(&cfg).await?;
        let config_manager = ConfigManager::new(components.config.clone());
        let peer_manager = PeerManager::new(peer_db).await?;

        // Create worker pool
        let worker_pool = WorkerPool::new(
            components.queue.clone(),
            components.storage.clone(),
            components.auth.clone(),
            components.config.clone(),
            cfg.article_worker_count,
        );

        Ok(Self {
            components,
            config_manager,
            peer_manager,
            worker_pool,
        })
    }

    /// Initialize core server components
    async fn initialize_components(cfg: &Config) -> ServerResult<ServerComponents> {
        let config = Arc::new(RwLock::new(cfg.clone()));

        let storage: Arc<dyn Storage> = storage::open(&cfg.db_path).await?;
        let auth: Arc<dyn AuthProvider> = auth::open(&cfg.auth_db_path).await?;

        // Create article queue with configurable capacity
        let queue = ArticleQueue::new(cfg.article_queue_capacity);

        Ok(ServerComponents {
            storage,
            auth,
            config,
            queue,
        })
    }

    /// Initialize peer database and sync configuration
    async fn initialize_peer_db(cfg: &Config) -> ServerResult<PeerDb> {
        let peer_db = PeerDb::new(&cfg.peer_db_path).await?;
        let names: Vec<String> = cfg.peers.iter().map(|p| p.sitename.clone()).collect();
        peer_db.sync_config(&names).await?;
        Ok(peer_db)
    }

    /// Start all peer synchronization tasks
    async fn start_peer_tasks(&self) -> ServerResult<()> {
        let cfg_guard = self.components.config.read().await;
        self.peer_manager
            .start_peer_tasks(&cfg_guard, self.components.storage.clone())
            .await
    }

    /// Start TCP listener task
    async fn start_tcp_listener(&self) -> ServerResult<tokio::task::JoinHandle<()>> {
        let addr_config = {
            let cfg_guard = self.components.config.read().await;
            cfg_guard.addr.clone()
        };

        let listener = get_listener(&addr_config).await?;

        // ...existing code...
        let storage = self.components.storage.clone();
        let auth = self.components.auth.clone();
        let config = self.components.config.clone();
        let queue = self.components.queue.clone();

        let handle = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((socket, _)) => {
                        info!("accepted connection");
                        handle_connection(
                            socket,
                            storage.clone(),
                            auth.clone(),
                            config.clone(),
                            false,
                            queue.clone(),
                        )
                        .await;
                    }
                    Err(e) => error!("failed to accept connection: {e}"),
                }
            }
        });

        Ok(handle)
    }

    /// Start TLS listener task if configured
    async fn start_tls_listener(&self) -> ServerResult<Option<tokio::task::JoinHandle<()>>> {
        let cfg_guard = self.components.config.read().await;

        let Some((tls_addr_raw, cert, key)) = (|| {
            Some((
                cfg_guard.tls_addr.as_deref()?,
                cfg_guard.tls_cert.as_ref()?,
                cfg_guard.tls_key.as_ref()?,
            ))
        })() else {
            return Ok(None);
        };

        let tls_listener = get_listener(tls_addr_raw).await?;
        let acceptor = TlsAcceptor::from(Arc::new(load_tls_config(cert, key)?));
        *self.config_manager.tls_acceptor.write().await = Some(acceptor.clone());

        let storage = self.components.storage.clone();
        let auth = self.components.auth.clone();
        let config = self.components.config.clone();
        let queue = self.components.queue.clone();

        let handle = tokio::spawn(async move {
            loop {
                match tls_listener.accept().await {
                    Ok((socket, _)) => {
                        info!("accepted TLS connection");
                        let storage_clone = storage.clone();
                        let auth_clone = auth.clone();
                        let config_clone = config.clone();
                        let acceptor_clone = acceptor.clone();
                        let queue_clone = queue.clone();

                        tokio::spawn(async move {
                            match acceptor_clone.accept(socket).await {
                                Ok(stream) => {
                                    handle_connection(
                                        stream,
                                        storage_clone,
                                        auth_clone,
                                        config_clone,
                                        true,
                                        queue_clone,
                                    )
                                    .await;
                                }
                                Err(e) => error!("tls error: {e}"),
                            }
                        });
                    }
                    Err(e) => error!("failed to accept TLS connection: {e}"),
                }
            }
        });

        Ok(Some(handle))
    }

    /// Start WebSocket bridge task if configured
    #[cfg(feature = "websocket")]
    async fn start_websocket_bridge(&self) -> ServerResult<Option<tokio::task::JoinHandle<()>>> {
        let cfg_guard = self.components.config.read().await;

        if let Some(addr_raw) = cfg_guard.ws_addr.as_deref() {
            info!("websocket bridge on {addr_raw}");
            let config = self.components.config.clone();

            let handle = tokio::spawn(async move {
                if let Err(e) = ws::run_ws_bridge(config).await {
                    error!("websocket error: {e}");
                }
            });

            Ok(Some(handle))
        } else {
            Ok(None)
        }
    }

    /// Start WebSocket bridge task (no-op for non-websocket builds)
    #[cfg(not(feature = "websocket"))]
    async fn start_websocket_bridge(&self) -> ServerResult<Option<tokio::task::JoinHandle<()>>> {
        Ok(None)
    }

    /// Start retention cleanup task
    async fn start_retention_cleanup(&self) -> ServerResult<tokio::task::JoinHandle<()>> {
        let storage = self.components.storage.clone();
        let config = self.components.config.clone();

        let handle = tokio::spawn(async move {
            loop {
                let cfg_guard = config.read().await;
                if let Err(e) = cleanup_expired_articles(&*storage, &cfg_guard).await {
                    error!("retention cleanup error: {e}");
                }
                drop(cfg_guard);
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            }
        });

        Ok(handle)
    }

    /// Start configuration reload handler
    async fn start_config_reload_handler(
        &self,
        cfg_path: String,
    ) -> ServerResult<tokio::task::JoinHandle<()>> {
        let config_manager = self.config_manager.clone();
        let peer_manager = self.peer_manager.clone();
        let storage = self.components.storage.clone();

        let handle = tokio::spawn(async move {
            if let Ok(mut hup) = signal(SignalKind::hangup()) {
                while hup.recv().await.is_some() {
                    if let Err(e) = handle_config_reload_with_managers(
                        &config_manager,
                        &peer_manager,
                        &storage,
                        &cfg_path,
                    )
                    .await
                    {
                        error!("config reload failed: {e}");
                    }
                }
            }
        });

        Ok(handle)
    }

    /// Start all server services
    pub async fn run(self, cfg_path: String) -> ServerResult<()> {
        // Start worker pool first
        let _worker_handles = self.worker_pool.start().await;

        self.start_peer_tasks().await?;

        // Start all listeners and background tasks
        let _tcp_handle = self.start_tcp_listener().await?;
        let _tls_handle = self.start_tls_listener().await?;
        let _ws_handle = self.start_websocket_bridge().await?;
        let _retention_handle = self.start_retention_cleanup().await?;
        let _config_handle = self.start_config_reload_handler(cfg_path).await?;

        // Wait for shutdown signal
        tokio::signal::ctrl_c().await?;
        info!("shutdown signal received");

        Ok(())
    }
}

/// Configuration management for the server
#[derive(Clone)]
struct ConfigManager {
    config: Arc<RwLock<Config>>,
    tls_acceptor: Arc<RwLock<Option<TlsAcceptor>>>,
}

impl ConfigManager {
    fn new(config: Arc<RwLock<Config>>) -> Self {
        Self {
            config,
            tls_acceptor: Arc::new(RwLock::new(None)),
        }
    }

    async fn reload(&self, cfg_path: &str) -> ServerResult<()> {
        let new_cfg = Config::from_file(cfg_path)?;

        // Update TLS configuration if present
        if let (Some(cert), Some(key)) = (new_cfg.tls_cert.as_ref(), new_cfg.tls_key.as_ref()) {
            match load_tls_config(cert, key) {
                Ok(conf) => {
                    *self.tls_acceptor.write().await = Some(TlsAcceptor::from(Arc::new(conf)));
                }
                Err(e) => error!("failed to load tls config: {e}"),
            }
        }

        // Update runtime configuration
        self.config.write().await.update_runtime(new_cfg);
        info!("configuration reloaded");

        Ok(())
    }
}

/// Peer management for the server
#[derive(Clone)]
struct PeerManager {
    peer_db: PeerDb,
    scheduler: Arc<JobScheduler>,
    peer_jobs: Arc<DashMap<String, uuid::Uuid>>,
}

impl PeerManager {
    async fn new(peer_db: PeerDb) -> ServerResult<Self> {
        let scheduler = JobScheduler::new().await?;
        scheduler.start().await?;

        Ok(Self {
            peer_db,
            scheduler: Arc::new(scheduler),
            peer_jobs: Arc::new(DashMap::new()),
        })
    }

    async fn start_peer_tasks(
        &self,
        config: &Config,
        storage: Arc<dyn Storage>,
    ) -> ServerResult<()> {
        let default_schedule = config.peer_sync_schedule.clone();

        for peer in &config.peers {
            let pc = PeerConfig::from(peer);
            let name = pc.sitename.clone();

            match add_peer_job(
                &self.scheduler,
                pc,
                default_schedule.clone(),
                self.peer_db.clone(),
                storage.clone(),
                config.site_name.clone(),
            )
            .await
            {
                Ok(job_uuid) => {
                    self.peer_jobs.insert(name, job_uuid);
                }
                Err(e) => {
                    error!("Failed to add peer job for {}: {}", name, e);
                }
            }
        }

        Ok(())
    }

    async fn update_tasks(&self, new_cfg: &Config, storage: &Arc<dyn Storage>) -> ServerResult<()> {
        let names: Vec<String> = new_cfg.peers.iter().map(|p| p.sitename.clone()).collect();
        self.peer_db.sync_config(&names).await?;

        let default_schedule = new_cfg.peer_sync_schedule.clone();

        // Start new peer tasks
        for peer in &new_cfg.peers {
            if !self.peer_jobs.contains_key(&peer.sitename) {
                let pc = PeerConfig::from(peer);
                let name = pc.sitename.clone();

                match add_peer_job(
                    &self.scheduler,
                    pc,
                    default_schedule.clone(),
                    self.peer_db.clone(),
                    storage.clone(),
                    new_cfg.site_name.clone(),
                )
                .await
                {
                    Ok(job_uuid) => {
                        self.peer_jobs.insert(name, job_uuid);
                    }
                    Err(e) => {
                        error!("Failed to add peer job for {}: {}", name, e);
                    }
                }
            }
        }

        // Remove obsolete peer tasks
        let to_remove: Vec<String> = self
            .peer_jobs
            .iter()
            .filter(|entry| !new_cfg.peers.iter().any(|p| &p.sitename == entry.key()))
            .map(|entry| entry.key().clone())
            .collect();

        for name in to_remove {
            if let Some((_, job_uuid)) = self.peer_jobs.remove(&name) {
                if let Err(e) = self.scheduler.remove(&job_uuid).await {
                    error!("Failed to remove peer job for {}: {}", name, e);
                }
            }
        }

        Ok(())
    }
}

/// Load TLS configuration from certificate and key files
///
/// # Arguments
/// * `cert_path` - Path to the certificate file in PEM format
/// * `key_path` - Path to the private key file in PKCS#8 format
///
/// # Errors
/// Returns an error if the files cannot be read or contain invalid data
fn load_tls_config(cert_path: &str, key_path: &str) -> ServerResult<rustls::ServerConfig> {
    let cert_file = &mut BufReader::new(File::open(cert_path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            anyhow::anyhow!(
                "TLS certificate file not found: '{cert_path}'

Please ensure the certificate file exists at the specified path.
For Let's Encrypt certificates, this is typically '/etc/letsencrypt/live/domain/fullchain.pem'."
            )
        }
        std::io::ErrorKind::PermissionDenied => {
            anyhow::anyhow!(
                "Permission denied reading TLS certificate file: '{cert_path}'

Please ensure the file is readable by the current user.
You may need to run as root or adjust file permissions."
            )
        }
        _ => anyhow::anyhow!("Failed to open TLS certificate file '{cert_path}': {e}"),
    })?);
    let key_file = &mut BufReader::new(File::open(key_path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            anyhow::anyhow!(
                "TLS private key file not found: '{key_path}'

Please ensure the private key file exists at the specified path.
For Let's Encrypt certificates, this is typically '/etc/letsencrypt/live/domain/privkey.pem'."
            )
        }
        std::io::ErrorKind::PermissionDenied => {
            anyhow::anyhow!(
                "Permission denied reading TLS private key file: '{key_path}'

Please ensure the file is readable by the current user.
You may need to run as root or adjust file permissions.
Note: Private key files should be protected (mode 600)."
            )
        }
        _ => anyhow::anyhow!("Failed to open TLS private key file '{key_path}': {e}"),
    })?);

    let certs = certs(cert_file)
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse TLS certificate file '{cert_path}': {e}

Please ensure the certificate file is in valid PEM format.
The file should contain one or more certificates starting with '-----BEGIN CERTIFICATE-----'."
            )
        })?
        .into_iter()
        .map(rustls::Certificate)
        .collect();

    let mut keys = pkcs8_private_keys(key_file).map_err(|e| {
        anyhow::anyhow!(
            "Failed to parse TLS private key file '{key_path}': {e}

Please ensure the private key file is in valid PKCS#8 PEM format.
The file should contain a private key starting with '-----BEGIN PRIVATE KEY-----'.
If your key is in a different format, you may need to convert it:
- For RSA keys: openssl rsa -in old_key.pem -out new_key.pem
- For EC keys: openssl ec -in old_key.pem -out new_key.pem"
        )
    })?;

    if keys.is_empty() {
        return Err(anyhow::anyhow!(
            "No valid private key found in TLS key file '{key_path}'

Please ensure the file contains a valid PKCS#8 private key.
The file should have content starting with '-----BEGIN PRIVATE KEY-----'."
        ));
    }

    let key = rustls::PrivateKey(keys.remove(0));
    let config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to create TLS configuration: {e}

This error typically occurs when:
- The certificate and private key don't match
- The certificate chain is incomplete
- The certificate has expired
- The certificate format is invalid

Please verify that your certificate and key files are correct and match each other."
            )
        })?;

    Ok(config)
}

/// Convert raw address string to a proper listen address
///
/// # Arguments
/// * `raw` - Raw address string (can be just port, :port, or full address)
///
/// # Returns
/// A properly formatted address string suitable for binding
fn listen_addr(raw: &str) -> String {
    if raw.parse::<SocketAddr>().is_ok() {
        raw.to_string()
    } else if let Some(port) = raw.strip_prefix(':') {
        format!("0.0.0.0:{port}")
    } else {
        format!("0.0.0.0:{raw}")
    }
}

/// Try to get a systemd socket by name or bind directly to an address
///
/// # Arguments
/// * `addr_config` - Address configuration (can be socket name, systemd:// URL, or regular address)
///
/// # Returns
/// A TcpListener bound to the specified address or systemd socket
async fn get_listener(addr_config: &str) -> ServerResult<TcpListener> {
    // First check for systemd:// URLs
    if addr_config.starts_with("systemd://") {
        match addr_config.parse::<systemd_socket::SocketAddr>() {
            Ok(socket_addr) => {
                match socket_addr.bind() {
                    Ok(std_listener) => {
                        // Convert std::net::TcpListener to tokio::net::TcpListener
                        match std_listener.set_nonblocking(true) {
                            Ok(()) => match TcpListener::from_std(std_listener) {
                                Ok(listener) => {
                                    info!("using systemd socket: {addr_config}");
                                    Ok(listener)
                                }
                                Err(e) => {
                                    Err(anyhow::anyhow!("failed to convert socket to tokio: {e}"))
                                }
                            },
                            Err(e) => {
                                Err(anyhow::anyhow!("failed to set socket to non-blocking: {e}"))
                            }
                        }
                    }
                    Err(e) => Err(anyhow::anyhow!(
                        "Failed to bind to address '{addr_config}': {e}

This error typically occurs when:
- Another process is already using this port (try: lsof -i :<port> or netstat -tlnp | grep :<port>)
- The port number is invalid (must be 1-65535)
- Permission denied for privileged ports (<1024) - try running as root or use a port ≥1024
- The address format is incorrect (should be 'host:port', ':port', or just 'port')
- For systemd socket activation, the socket is not available

You can use 'systemd://socket_name' format for systemd socket activation."
                    )),
                }
            }
            Err(e) => Err(anyhow::anyhow!(
                "Invalid systemd socket address '{addr_config}': {e}"
            )),
        }
    } else {
        // For regular addresses, use our own parsing logic
        let addr = listen_addr(addr_config);
        info!("listening on {addr}");
        match TcpListener::bind(&addr).await {
            Ok(listener) => Ok(listener),
            Err(e) => Err(anyhow::anyhow!(
                "Failed to bind to address '{}': {}

This error typically occurs when:
- Another process is already using this port (try: lsof -i :{} or netstat -tlnp | grep :{})
- The port number is invalid (must be 1-65535)
- Permission denied for privileged ports (<1024) - try running as root or use a port ≥1024
- The address format is incorrect (should be 'host:port', ':port', or just 'port')

You can use 'systemd://socket_name' format for systemd socket activation.",
                addr_config,
                e,
                addr.split(':').next_back().unwrap_or("119"),
                addr.split(':').next_back().unwrap_or("119")
            )),
        }
    }
}

/// Handle an incoming client connection
async fn handle_connection<S>(
    socket: S,
    storage: Arc<dyn Storage>,
    auth: Arc<dyn AuthProvider>,
    config: Arc<RwLock<Config>>,
    is_tls: bool,
    queue: ArticleQueue,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        if let Err(e) = crate::handle_client(socket, storage, auth, config, is_tls, queue).await {
            error!("client error: {e}");
        }
    });
}

/// Main server entry point
///
/// This function initializes the server and starts all necessary components:
/// - TCP and TLS listeners for NNTP connections
/// - WebSocket bridge (if enabled)
/// - Peer synchronization tasks
/// - Retention cleanup task
/// - Configuration reload handler
///
/// # Arguments
/// * `cfg_initial` - Initial server configuration
/// * `cfg_path` - Path to configuration file for reloading
///
/// # Errors
/// Returns an error if server initialization or startup fails
pub async fn run(cfg_initial: Config, cfg_path: String) -> ServerResult<()> {
    let server = Server::new(cfg_initial).await?;
    server.run(cfg_path).await
}

/// Handle a single configuration reload using managers
///
/// # Arguments
/// * `config_manager` - Configuration manager
/// * `peer_manager` - Peer manager
/// * `storage` - Storage backend
/// * `cfg_path` - Path to configuration file
///
/// # Errors
/// Returns an error if configuration reload fails
async fn handle_config_reload_with_managers(
    config_manager: &ConfigManager,
    peer_manager: &PeerManager,
    storage: &Arc<dyn Storage>,
    cfg_path: &str,
) -> ServerResult<()> {
    let new_cfg = Config::from_file(cfg_path)?;

    // Update configuration using manager
    config_manager.reload(cfg_path).await?;

    // Update peer configuration using manager
    peer_manager.update_tasks(&new_cfg, storage).await?;

    Ok(())
}
