use futures_util::{SinkExt, StreamExt};
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{error, info};

use crate::config::Config;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::RwLock;

pub async fn run_ws_bridge(cfg: Arc<RwLock<Config>>) -> Result<(), Box<dyn Error + Send + Sync>> {
    let (ws_port, nntp_port) = {
        let cfg_guard = cfg.read().await;
        match cfg_guard.ws_port {
            Some(p) => (p, cfg_guard.port),
            None => return Ok(()),
        }
    };
    let addr = format!("127.0.0.1:{ws_port}");
    info!("listening WebSocket on {addr}");
    let listener = TcpListener::bind(&addr).await?;
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
