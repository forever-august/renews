//! Peer synchronization module for NNTP server federation.
//!
//! This module handles the synchronization of articles between NNTP servers
//! using peer relationships. It supports both IHAVE and TAKETHIS transfer modes
//! for efficient article distribution.

use chrono::{DateTime, Utc};
use futures_util::{StreamExt, TryStreamExt};
use rustls_native_certs::load_native_certs;
use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};
use std::error::Error;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_cron_scheduler::{Job, JobScheduler};
use tokio_rustls::{
    TlsConnector,
    rustls::{self, RootCertStore},
};
use uuid;

use crate::storage::DynStorage;
use crate::wildmat::wildmat;
use crate::{
    Message,
    handlers::utils::{extract_message_id, send_body, send_headers, write_simple},
};

/// Result type for peer operations.
type PeerResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Connection credentials for peer authentication.
#[derive(Debug, Clone)]
struct PeerCredentials {
    username: String,
    password: String,
}

/// Parsed peer connection information.
#[derive(Debug, Clone)]
struct PeerConnectionInfo {
    host: String,
    port: u16,
    credentials: Option<PeerCredentials>,
}

/// Parse peer address string into connection components.
///
/// Supports formats like:
/// - `host:port`
/// - `user:pass@host:port`  
/// - `[ipv6]:port`
/// - `user:pass@[ipv6]:port`
fn parse_peer_address(addr: &str, default_port: u16) -> PeerConnectionInfo {
    let (credentials, host_port) = extract_credentials(addr);
    let (host, port) = parse_host_and_port(host_port, default_port);

    PeerConnectionInfo {
        host,
        port,
        credentials,
    }
}

/// Extract username:password credentials from address string.
fn extract_credentials(addr: &str) -> (Option<PeerCredentials>, &str) {
    let Some((creds_part, rest)) = addr.rsplit_once('@') else {
        return (None, addr);
    };

    let Some((username, password)) = creds_part.split_once(':') else {
        return (None, addr);
    };

    let credentials = PeerCredentials {
        username: username.to_string(),
        password: password.to_string(),
    };
    (Some(credentials), rest)
}

/// Parse host and port from address string, handling IPv6 addresses.
fn parse_host_and_port(host_port: &str, default_port: u16) -> (String, u16) {
    // Handle IPv6 addresses wrapped in brackets
    if let Some(ipv6_content) = host_port.strip_prefix('[') {
        return parse_ipv6_address(ipv6_content, default_port);
    }

    // Handle regular host:port format
    parse_regular_address(host_port, default_port)
}

/// Parse IPv6 address format [host]:port
fn parse_ipv6_address(content: &str, default_port: u16) -> (String, u16) {
    let Some(end) = content.find(']') else {
        return (content.to_string(), default_port);
    };

    let host = content[..end].to_string();
    let port_part = &content[end + 1..];

    let port = port_part
        .strip_prefix(':')
        .and_then(|s| s.parse().ok())
        .unwrap_or(default_port);

    (host, port)
}

/// Parse regular host:port format
fn parse_regular_address(host_port: &str, default_port: u16) -> (String, u16) {
    let Some(colon_pos) = host_port.rfind(':') else {
        return (host_port.to_string(), default_port);
    };

    let port_str = &host_port[colon_pos + 1..];
    if !port_str.chars().all(|c| c.is_ascii_digit()) {
        return (host_port.to_string(), default_port);
    }

    let Ok(port) = port_str.parse() else {
        return (host_port.to_string(), default_port);
    };

    (host_port[..colon_pos].to_string(), port)
}

/// Creates a TLS connector for secure peer connections.
fn create_tls_connector() -> PeerResult<TlsConnector> {
    let mut roots = RootCertStore::empty();
    for cert in load_native_certs()? {
        roots.add(&rustls::Certificate(cert.0))?;
    }
    let config = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(TlsConnector::from(Arc::new(config)))
}

/// Manages a connection to a peer NNTP server.
struct PeerConnection {
    reader: BufReader<tokio::io::ReadHalf<tokio_rustls::client::TlsStream<TcpStream>>>,
    writer: tokio::io::WriteHalf<tokio_rustls::client::TlsStream<TcpStream>>,
    line_buffer: String,
}

impl PeerConnection {
    /// Establish a connection to a peer server.
    async fn connect(connection_info: &PeerConnectionInfo) -> PeerResult<Self> {
        let addr = format!("{}:{}", connection_info.host, connection_info.port);
        let tcp = TcpStream::connect(&addr)
            .await
            .map_err(|e| format!("Failed to connect to {addr}: {e}"))?;

        let connector =
            create_tls_connector().map_err(|e| format!("Failed to create TLS connector: {e}"))?;

        let server_name = rustls::ServerName::try_from(connection_info.host.as_str())
            .map_err(|e| format!("Invalid server name '{}': {}", connection_info.host, e))?;

        let tls_stream = connector
            .connect(server_name, tcp)
            .await
            .map_err(|e| format!("TLS handshake failed for {addr}: {e}"))?;

        let (read_half, write_half) = tokio::io::split(tls_stream);

        let mut connection = Self {
            reader: BufReader::new(read_half),
            writer: write_half,
            line_buffer: String::new(),
        };

        // Read and validate greeting
        let greeting = connection.read_response().await?;
        if !greeting.starts_with("200") && !greeting.starts_with("201") {
            return Err(format!("Unexpected greeting from {}: {}", addr, greeting.trim()).into());
        }

        // Authenticate if credentials are provided
        if let Some(creds) = &connection_info.credentials {
            connection
                .authenticate(creds)
                .await
                .map_err(|e| format!("Authentication failed for {addr}: {e}"))?;
        }

        Ok(connection)
    }

    /// Read a response line from the server.
    async fn read_response(&mut self) -> PeerResult<&str> {
        self.line_buffer.clear();
        self.reader.read_line(&mut self.line_buffer).await?;
        Ok(&self.line_buffer)
    }

    /// Send a command to the server.
    async fn send_command(&mut self, command: &str) -> PeerResult<()> {
        write_simple(&mut self.writer, command).await
    }

    /// Authenticate with the server using provided credentials.
    async fn authenticate(&mut self, creds: &PeerCredentials) -> PeerResult<()> {
        self.send_command(&format!("AUTHINFO USER {}\r\n", creds.username))
            .await?;
        let response = self.read_response().await?;

        if response.starts_with("381") {
            self.send_command(&format!("AUTHINFO PASS {}\r\n", creds.password))
                .await?;
            let response = self.read_response().await?;
            if !response.starts_with("281") {
                return Err("Authentication failed".into());
            }
        } else if !response.starts_with("281") {
            return Err("Authentication failed".into());
        }

        Ok(())
    }

    /// Transfer an article using IHAVE protocol.
    async fn transfer_article(&mut self, article: &Message, msg_id: &str) -> PeerResult<()> {
        // Send IHAVE command
        self.send_command(&format!("IHAVE {msg_id}\r\n")).await?;
        let response = self.read_response().await?;
        if !response.starts_with("335") {
            return Ok(()); // Article not wanted by peer
        }

        // Send article content
        self.send_article_content(article).await?;

        // Read and validate final response
        let response = self.read_response().await?;
        if !response.starts_with("2") {
            return Err(format!("Transfer failed: {}", response.trim()).into());
        }

        Ok(())
    }

    /// Send the complete article content including headers and body.
    async fn send_article_content(&mut self, article: &Message) -> PeerResult<()> {
        send_headers(&mut self.writer, article).await?;
        self.send_command("\r\n").await?;
        send_body(&mut self.writer, &article.body).await?;
        self.send_command(".\r\n").await?;
        Ok(())
    }

    /// Close the connection gracefully.
    async fn close(mut self) -> PeerResult<()> {
        let _ = self.writer.shutdown().await;
        Ok(())
    }
}

#[derive(Clone)]
pub struct PeerDb {
    pool: SqlitePool,
}

impl PeerDb {
    /// Create a new `SQLite` peers database.
    ///
    /// # Errors
    ///
    /// Returns an error if the database connection fails or schema creation fails.
    pub async fn new(path: &str) -> PeerResult<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(path)
            .await?;

        // Create peers table if it doesn't exist
        sqlx::query(
            r"CREATE TABLE IF NOT EXISTS peers (
                sitename TEXT PRIMARY KEY,
                last_sync INTEGER
            )",
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }

    /// List all configured peers.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    ///
    /// # Panics
    ///
    /// Panics if the sitename column cannot be retrieved from the database row.
    pub async fn list_peers(&self) -> PeerResult<Vec<String>> {
        let rows = sqlx::query("SELECT sitename FROM peers")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| r.try_get("sitename").unwrap())
            .collect())
    }

    /// Synchronize peer configuration with the database.
    ///
    /// # Errors
    ///
    /// Returns an error if database operations fail.
    pub async fn sync_config(&self, names: &[String]) -> PeerResult<()> {
        let existing_peers = self.list_peers().await?;

        // Add new peers
        for name in names {
            if !existing_peers.contains(name) {
                sqlx::query("INSERT INTO peers (sitename, last_sync) VALUES (?, 0)")
                    .bind(name)
                    .execute(&self.pool)
                    .await?;
            }
        }

        // Remove peers no longer in configuration
        for existing_peer in existing_peers {
            if !names.contains(&existing_peer) {
                sqlx::query("DELETE FROM peers WHERE sitename = ?")
                    .bind(&existing_peer)
                    .execute(&self.pool)
                    .await?;
            }
        }

        Ok(())
    }

    /// Update the last synchronization time for a peer.
    ///
    /// # Errors
    ///
    /// Returns an error if the database update fails.
    pub async fn update_last_sync(&self, name: &str, when: DateTime<Utc>) -> PeerResult<()> {
        sqlx::query("UPDATE peers SET last_sync = ? WHERE sitename = ?")
            .bind(when.timestamp())
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Get the last synchronization time for a peer.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn get_last_sync(&self, name: &str) -> PeerResult<Option<DateTime<Utc>>> {
        let row = sqlx::query("SELECT last_sync FROM peers WHERE sitename = ?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => {
                let timestamp: i64 = row.try_get("last_sync")?;
                if timestamp == 0 {
                    Ok(None)
                } else {
                    Ok(DateTime::<Utc>::from_timestamp(timestamp, 0))
                }
            }
            None => Ok(None),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PeerConfig {
    pub sitename: String,
    pub patterns: Vec<String>,
    pub sync_schedule: Option<String>,
}

impl From<&crate::config::PeerRule> for PeerConfig {
    fn from(r: &crate::config::PeerRule) -> Self {
        Self {
            sitename: r.sitename.clone(),
            patterns: r.patterns.clone(),
            sync_schedule: r.sync_schedule.clone(),
        }
    }
}

/// Add a peer sync job to the shared scheduler.
///
/// Returns the job UUID on success for later removal.
pub async fn add_peer_job(
    scheduler: &JobScheduler,
    peer: PeerConfig,
    default_schedule: String,
    db: PeerDb,
    storage: DynStorage,
    site_name: String,
) -> PeerResult<uuid::Uuid> {
    let schedule = peer.sync_schedule.as_deref().unwrap_or(&default_schedule);

    tracing::info!(
        "Adding peer sync job for {} with schedule '{}'",
        peer.sitename,
        schedule
    );

    let peer_clone = peer.clone();
    let db_clone = db.clone();
    let storage_clone = storage.clone();
    let site_name_clone = site_name.clone();

    let job = Job::new_async(schedule, move |_uuid, _l| {
        let peer = peer_clone.clone();
        let db = db_clone.clone();
        let storage = storage_clone.clone();
        let site_name = site_name_clone.clone();

        Box::pin(async move {
            let sync_start = std::time::Instant::now();

            match sync_peer_once(&peer, &db, &storage, &site_name).await {
                Ok(()) => {
                    let duration = sync_start.elapsed();
                    tracing::debug!(
                        "Peer sync completed successfully for {} in {:?}",
                        peer.sitename,
                        duration
                    );
                }
                Err(e) => {
                    tracing::error!("Peer sync failed for {}: {}", peer.sitename, e);
                }
            }

            // Update last sync time regardless of success/failure
            if let Err(e) = db.update_last_sync(&peer.sitename, Utc::now()).await {
                tracing::error!(
                    "Failed to update last sync time for {}: {}",
                    peer.sitename,
                    e
                );
            }
        })
    })?;

    let job_uuid = job.guid();
    scheduler.add(job).await?;

    tracing::debug!(
        "Added peer sync job for {} with UUID {}",
        peer.sitename,
        job_uuid
    );
    Ok(job_uuid)
}

async fn send_article_to_peer(host: &str, article: &Message) -> PeerResult<()> {
    let msg_id = extract_message_id(article).ok_or("Article missing Message-ID header")?;

    let connection_info = parse_peer_address(host, 563);
    let mut connection = PeerConnection::connect(&connection_info)
        .await
        .map_err(|e| format!("Failed to connect to peer {host}: {e}"))?;

    let result = connection.transfer_article(article, msg_id).await;

    if let Err(close_err) = connection.close().await {
        tracing::warn!("Failed to close connection to {}: {}", host, close_err);
    }

    result
}

async fn sync_peer_once(
    peer: &PeerConfig,
    db: &PeerDb,
    storage: &DynStorage,
    site_name: &str,
) -> PeerResult<()> {
    let last_sync = db.get_last_sync(&peer.sitename).await?;
    let mut groups = storage.list_groups();
    while let Some(result) = groups.next().await {
        let group = result?;

        if !peer.patterns.iter().any(|pattern| wildmat(pattern, &group)) {
            continue;
        }

        let article_ids_stream = match last_sync {
            Some(timestamp) => storage.list_article_ids_since(&group, timestamp),
            None => storage.list_article_ids(&group),
        };
        let article_ids = article_ids_stream.try_collect::<Vec<String>>().await?;

        process_group_articles(peer, storage, site_name, &group, article_ids).await?;
    }

    Ok(())
}

/// Process and send articles from a specific group to a peer.
async fn process_group_articles(
    peer: &PeerConfig,
    storage: &DynStorage,
    site_name: &str,
    group: &str,
    article_ids: Vec<String>,
) -> PeerResult<()> {
    if article_ids.is_empty() {
        return Ok(());
    }

    tracing::debug!(
        "Processing {} articles from group {} for peer {}",
        article_ids.len(),
        group,
        peer.sitename
    );

    let mut sent_count = 0;
    let mut skipped_count = 0;
    let mut error_count = 0;

    for article_id in &article_ids {
        match process_single_article(peer, storage, site_name, article_id).await {
            Ok(ArticleProcessResult::Sent) => sent_count += 1,
            Ok(ArticleProcessResult::Skipped) => skipped_count += 1,
            Ok(ArticleProcessResult::NotFound) => {
                tracing::debug!("Article {} not found in storage", article_id);
            }
            Err(e) => {
                error_count += 1;
                tracing::warn!(
                    "Failed to process article {} for peer {}: {}",
                    article_id,
                    peer.sitename,
                    e
                );
            }
        }
    }

    tracing::info!(
        "Completed processing for peer {} in group {}: {} sent, {} skipped, {} errors",
        peer.sitename,
        group,
        sent_count,
        skipped_count,
        error_count
    );

    Ok(())
}

/// Result of processing a single article.
#[derive(Debug)]
enum ArticleProcessResult {
    Sent,
    Skipped,
    NotFound,
}

/// Process a single article for peer distribution.
async fn process_single_article(
    peer: &PeerConfig,
    storage: &DynStorage,
    site_name: &str,
    article_id: &str,
) -> PeerResult<ArticleProcessResult> {
    let Some(original_article) = storage.get_article_by_id(article_id).await? else {
        return Ok(ArticleProcessResult::NotFound);
    };

    if should_skip_article(&original_article, &peer.sitename) {
        tracing::debug!(
            "Skipping article {} for peer {} (already in path)",
            article_id,
            peer.sitename
        );
        return Ok(ArticleProcessResult::Skipped);
    }

    let peer_article = create_peer_article(&original_article, site_name)?;
    send_article_to_peer(&peer.sitename, &peer_article).await?;
    tracing::debug!(
        "Successfully sent article {} to {}",
        article_id,
        peer.sitename
    );

    Ok(ArticleProcessResult::Sent)
}

/// Creates a copy of an article with appropriate Path header for peer distribution.
fn create_peer_article(orig: &Message, site_name: &str) -> PeerResult<Message> {
    let mut article = orig.clone();

    // Update or add Path header
    if let Some((_, path_value)) = article
        .headers
        .iter_mut()
        .find(|(k, _)| k.eq_ignore_ascii_case("Path"))
    {
        *path_value = format!("{site_name}!{path_value}");
    } else {
        article.headers.push(("Path".into(), site_name.to_string()));
    }

    Ok(article)
}

/// Checks if an article should be skipped for a specific peer.
fn should_skip_article(article: &Message, peer_sitename: &str) -> bool {
    article
        .headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("Path"))
        .any(|(_, path)| {
            path.split('!')
                .any(|segment| segment.trim() == peer_sitename)
        })
}
