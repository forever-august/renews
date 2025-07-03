use renews::handle_client;
use renews::storage::Storage;
use renews::auth::AuthProvider;
use renews::config::Config;
use std::sync::Arc;
use tokio::io::BufReader;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsConnector, rustls};
use tokio::io::{self, ReadHalf, WriteHalf};
use rcgen::{generate_simple_self_signed, CertifiedKey};

pub async fn setup_server(
    storage: Arc<dyn Storage>,
    auth: Arc<dyn AuthProvider>,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let store_clone = storage.clone();
    let auth_clone = auth.clone();
    let cfg: Config = toml::from_str("port=1199").unwrap();
    let handle = tokio::spawn(async move {
        let (sock, _) = listener.accept().await.unwrap();
        handle_client(sock, store_clone, auth_clone, cfg, false).await.unwrap();
    });
    (addr, handle)
}

pub async fn setup_server_with_cfg(
    storage: Arc<dyn Storage>,
    auth: Arc<dyn AuthProvider>,
    cfg: Config,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let store_clone = storage.clone();
    let auth_clone = auth.clone();
    let handle = tokio::spawn(async move {
        let (sock, _) = listener.accept().await.unwrap();
        handle_client(sock, store_clone, auth_clone, cfg, false).await.unwrap();
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

pub async fn setup_tls_server(
    storage: Arc<dyn Storage>,
    auth: Arc<dyn AuthProvider>,
) -> (
    std::net::SocketAddr,
    rustls::Certificate,
    tokio::task::JoinHandle<()>,
) {
    let CertifiedKey { cert, signing_key } =
        generate_simple_self_signed(["localhost".to_string()]).unwrap();
    let cert_der = cert.der().to_vec();
    let key = signing_key.serialize_der();
    let tls_config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(vec![rustls::Certificate(cert_der.clone())], rustls::PrivateKey(key))
        .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(tls_config));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let store_clone = storage.clone();
    let auth_clone = auth.clone();
    let cfg: Config = toml::from_str("port=1199").unwrap();
    let handle = tokio::spawn(async move {
        let (sock, _) = listener.accept().await.unwrap();
        let stream = acceptor.accept(sock).await.unwrap();
        handle_client(stream, store_clone, auth_clone, cfg, true)
            .await
            .unwrap();
    });
    (addr, rustls::Certificate(cert_der), handle)
}

pub async fn connect_tls(
    addr: std::net::SocketAddr,
    cert: rustls::Certificate,
) -> (
    BufReader<ReadHalf<tokio_rustls::client::TlsStream<TcpStream>>>,
    WriteHalf<tokio_rustls::client::TlsStream<TcpStream>>,
) {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(&cert).unwrap();
    let config = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));
    let stream = TcpStream::connect(addr).await.unwrap();
    let server_name = rustls::ServerName::try_from("localhost").unwrap();
    let tls_stream = connector.connect(server_name, stream).await.unwrap();
    let (r, w) = io::split(tls_stream);
    (BufReader::new(r), w)
}
