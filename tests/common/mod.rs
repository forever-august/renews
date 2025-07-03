use renews::handle_client;
use renews::storage::Storage;
use renews::config::Config;
use std::sync::Arc;
use tokio::io::BufReader;
use tokio::net::{TcpListener, TcpStream};

pub async fn setup_server(
    storage: Arc<dyn Storage>,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let store_clone = storage.clone();
    let cfg: Config = toml::from_str("port=1199").unwrap();
    let handle = tokio::spawn(async move {
        let (sock, _) = listener.accept().await.unwrap();
        handle_client(sock, store_clone, cfg, false).await.unwrap();
    });
    (addr, handle)
}

pub async fn setup_server_with_cfg(
    storage: Arc<dyn Storage>,
    cfg: Config,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let store_clone = storage.clone();
    let handle = tokio::spawn(async move {
        let (sock, _) = listener.accept().await.unwrap();
        handle_client(sock, store_clone, cfg, false).await.unwrap();
    });
    (addr, handle)
}

pub async fn connect(
    addr: std::net::SocketAddr,
) -> (
    BufReader<tokio::net::tcp::OwnedReadHalf>,
    tokio::net::tcp::OwnedWriteHalf,
) {
    let stream = TcpStream::connect(addr).await.unwrap();
    let (r, w) = stream.into_split();
    (BufReader::new(r), w)
}
