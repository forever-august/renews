use futures_util::{SinkExt, StreamExt};
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{error, info};

use crate::config::Config;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

fn listen_addr(raw: &str) -> String {
    if raw.parse::<SocketAddr>().is_ok() {
        raw.to_string()
    } else if let Some(port) = raw.strip_prefix(':') {
        format!("0.0.0.0:{port}")
    } else {
        format!("0.0.0.0:{raw}")
    }
}

fn port_from_addr(addr: &str, default_port: u16) -> u16 {
    if let Some(stripped) = addr.strip_prefix('[') {
        if let Some(end) = stripped.find(']') {
            if let Some(p) = stripped[end + 1..].strip_prefix(':') {
                if let Ok(port) = p.parse() {
                    return port;
                }
            }
            return default_port;
        }
    }
    if let Some(idx) = addr.rfind(':') {
        if addr[idx + 1..].chars().all(|c| c.is_ascii_digit()) {
            if let Ok(port) = addr[idx + 1..].parse() {
                return port;
            }
        }
    }
    default_port
}
pub async fn run_ws_bridge(cfg: Arc<RwLock<Config>>) -> Result<(), Box<dyn Error + Send + Sync>> {
    let (ws_addr_raw, nntp_port) = {
        let cfg_guard = cfg.read().await;
        match cfg_guard.ws_addr.as_deref() {
            Some(a) => (a.to_string(), port_from_addr(&cfg_guard.addr, 119)),
            None => return Ok(()),
        }
    };
    let addr = listen_addr(&ws_addr_raw);
    info!("listening WebSocket on {addr}");
    let listener = TcpListener::bind(&addr).await.map_err(|e| {
        format!(
            "Failed to bind to WebSocket address '{}': {}

This error typically occurs when:
- Another process is already using this port (try: lsof -i :{} or netstat -tlnp | grep :{})
- The port number is invalid (must be 1-65535)
- Permission denied for privileged ports (<1024) - try running as root or use a port ≥1024
- The address format is incorrect (should be 'host:port', ':port', or just 'port')

You can change the WebSocket listen address in your configuration file using the 'ws_addr' setting
or disable the WebSocket bridge by removing the 'ws_addr' configuration.",
            addr, e,
            addr.split(':').last().unwrap_or("8080"),
            addr.split(':').last().unwrap_or("8080")
        )
    })?;
    let nntp_addr = format!("127.0.0.1:{nntp_port}");
    loop {
        let (stream, _) = listener.accept().await?;
        let nntp_addr_clone = nntp_addr.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_client(stream, &nntp_addr_clone).await {
                error!("websocket client error: {e}");
            }
        });
    }
}

async fn handle_client(
    stream: TcpStream,
    nntp_addr: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let ws_stream = accept_async(stream).await?;
    let (mut ws_write, mut ws_read) = ws_stream.split();
    let tcp = TcpStream::connect(nntp_addr).await?;
    let (mut nntp_read, mut nntp_write) = io::split(tcp);

    let to_nntp = tokio::spawn(async move {
        while let Some(msg) = ws_read.next().await {
            match msg? {
                Message::Text(t) => {
                    nntp_write.write_all(t.as_bytes()).await?;
                }
                Message::Binary(b) => {
                    nntp_write.write_all(&b).await?;
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        nntp_write.shutdown().await?;
        Ok::<_, Box<dyn Error + Send + Sync>>(())
    });

    let mut buf = [0u8; 1024];
    loop {
        let n = nntp_read.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        ws_write.send(Message::Binary(buf[..n].to_vec())).await?;
    }
    ws_write.close().await?;
    let _ = to_nntp.await?;
    Ok(())
}
