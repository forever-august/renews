use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};
use std::error::Error;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::{
    rustls::{self, RootCertStore},
    TlsConnector,
};
use rustls_native_certs::load_native_certs;

use crate::storage::DynStorage;
use crate::wildmat::wildmat;
use crate::{Message, extract_message_id, send_body, send_headers, write_simple};

fn parse_host_port(addr: &str, default_port: u16) -> (String, u16) {
    if let Some(stripped) = addr.strip_prefix('[') {
        if let Some(end) = stripped.find(']') {
            let host = stripped[..end].to_string();
            if let Some(p) = stripped[end + 1..].strip_prefix(':') {
                if let Ok(port) = p.parse() {
                    return (host, port);
                }
            }
            return (host, default_port);
        }
    }
    if let Some(idx) = addr.rfind(':') {
        if addr[idx + 1..].chars().all(|c| c.is_ascii_digit()) {
            if let Ok(port) = addr[idx + 1..].parse() {
                return (addr[..idx].to_string(), port);
            }
        }
    }
    (addr.to_string(), default_port)
}

fn tls_connector() -> Result<TlsConnector, Box<dyn Error + Send + Sync>> {
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

#[derive(Clone)]
pub struct PeerDb {
    pool: SqlitePool,
}

impl PeerDb {
    pub async fn new(path: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(path)
            .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS peers (\n                sitename TEXT PRIMARY KEY,\n                last_sync INTEGER\n            )",
        )
        .execute(&pool)
        .await?;
        Ok(Self { pool })
    }

    pub async fn list_peers(&self) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
        let rows = sqlx::query("SELECT sitename FROM peers")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| r.try_get("sitename").unwrap())
            .collect())
    }

    pub async fn sync_config(&self, names: &[String]) -> Result<(), Box<dyn Error + Send + Sync>> {
        let existing = self.list_peers().await?;
        for n in names {
            if !existing.iter().any(|e| e == n) {
                sqlx::query("INSERT INTO peers (sitename, last_sync) VALUES (?, 0)")
                    .bind(n)
                    .execute(&self.pool)
                    .await?;
            }
        }
        for e in existing {
            if !names.iter().any(|n| n == &e) {
                sqlx::query("DELETE FROM peers WHERE sitename = ?")
                    .bind(e)
                    .execute(&self.pool)
                    .await?;
            }
        }
        Ok(())
    }

    pub async fn update_last_sync(
        &self,
        name: &str,
        when: DateTime<Utc>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        sqlx::query("UPDATE peers SET last_sync = ? WHERE sitename = ?")
            .bind(when.timestamp())
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_last_sync(
        &self,
        name: &str,
    ) -> Result<Option<DateTime<Utc>>, Box<dyn Error + Send + Sync>> {
        if let Some(row) = sqlx::query("SELECT last_sync FROM peers WHERE sitename = ?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?
        {
            let ts: i64 = row.try_get("last_sync")?;
            if ts == 0 {
                return Ok(None);
            }
            Ok(DateTime::<Utc>::from_timestamp(ts, 0))
        } else {
            Ok(None)
        }
    }
}

#[derive(Clone, Debug)]
pub struct PeerConfig {
    pub sitename: String,
    pub patterns: Vec<String>,
    pub sync_interval_secs: Option<u64>,
}

impl From<&crate::config::PeerRule> for PeerConfig {
    fn from(r: &crate::config::PeerRule) -> Self {
        Self {
            sitename: r.sitename.clone(),
            patterns: r.patterns.clone(),
            sync_interval_secs: r.sync_interval_secs,
        }
    }
}

pub async fn peer_task(
    peer: PeerConfig,
    default_interval: u64,
    db: PeerDb,
    storage: DynStorage,
    site_name: String,
) {
    let interval = peer.sync_interval_secs.unwrap_or(default_interval);
    let delay = tokio::time::Duration::from_secs(interval.max(1));
    let use_takethis = peer.sync_interval_secs == Some(0);
    loop {
        if let Err(e) = sync_peer_once(&peer, &db, &storage, &site_name, use_takethis).await {
            tracing::error!("peer sync error: {e}");
        }
        let _ = db.update_last_sync(&peer.sitename, Utc::now()).await;
        tokio::time::sleep(delay).await;
    }
}

async fn send_article(
    host: &str,
    article: &Message,
    use_takethis: bool,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let msg_id = extract_message_id(article).ok_or("missing Message-ID")?;
    let (host_part, port) = parse_host_port(host, 563);
    let addr = format!("{}:{}", host_part, port);
    let tcp = TcpStream::connect(addr).await?;
    let connector = tls_connector()?;
    let server_name = rustls::ServerName::try_from(host_part.as_str())?;
    let tls_stream = connector.connect(server_name, tcp).await?;
    let (read_half, mut write_half) = tokio::io::split(tls_stream);
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    reader.read_line(&mut line).await?; // greeting
    if use_takethis {
        write_simple(&mut write_half, "MODE STREAM\r\n").await?;
        line.clear();
        reader.read_line(&mut line).await?; // ignore
        write_simple(&mut write_half, &format!("TAKETHIS {}\r\n", msg_id)).await?;
    } else {
        write_simple(&mut write_half, &format!("IHAVE {}\r\n", msg_id)).await?;
        line.clear();
        reader.read_line(&mut line).await?;
        if !line.starts_with("335") {
            return Ok(());
        }
    }
    send_headers(&mut write_half, article).await?;
    write_simple(&mut write_half, "\r\n").await?;
    send_body(&mut write_half, &article.body).await?;
    write_simple(&mut write_half, ".\r\n").await?;
    line.clear();
    reader.read_line(&mut line).await?;
    let _ = write_half.shutdown().await;
    Ok(())
}

async fn sync_peer_once(
    peer: &PeerConfig,
    db: &PeerDb,
    storage: &DynStorage,
    site_name: &str,
    use_takethis: bool,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let last = db.get_last_sync(&peer.sitename).await?;
    let groups = storage.list_groups().await?;
    for group in groups {
        if !peer.patterns.iter().any(|p| wildmat(p, &group)) {
            continue;
        }
        let ids = if let Some(ts) = last {
            storage.list_article_ids_since(&group, ts).await?
        } else {
            storage.list_article_ids(&group).await?
        };
        for id in ids {
            if let Some(orig) = storage.get_article_by_id(&id).await? {
                let mut article = Message {
                    headers: orig.headers.clone(),
                    body: orig.body.clone(),
                };
                let mut has_path = false;
                let mut skip = false;
                for (k, v) in article.headers.iter_mut() {
                    if k.eq_ignore_ascii_case("Path") {
                        if v.split('!').any(|s| s.trim() == peer.sitename) {
                            skip = true;
                            break;
                        }
                        *v = format!("{}!{}", site_name, v);
                        has_path = true;
                    }
                }
                if skip {
                    continue;
                }
                if !has_path {
                    article.headers.push(("Path".into(), site_name.to_string()));
                }
                let _ = send_article(&peer.sitename, &article, use_takethis).await;
            }
        }
    }
    Ok(())
}
