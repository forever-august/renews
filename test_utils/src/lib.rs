use rcgen::{CertifiedKey, generate_simple_self_signed};
use renews::auth::AuthProvider;
use renews::config::Config;
use renews::handle_client;
use renews::storage::Storage;
use std::sync::Arc;
use tokio::io::BufReader;
use tokio::io::{self, ReadHalf, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio_rustls::{TlsAcceptor, TlsConnector, rustls};
use tokio_test::io::Builder as IoBuilder;

pub async fn setup_server(
    storage: Arc<dyn Storage>,
    auth: Arc<dyn AuthProvider>,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let store_clone = storage.clone();
    let auth_clone = auth.clone();
    let cfg: Arc<RwLock<Config>> = Arc::new(RwLock::new(toml::from_str("port=119").unwrap()));
    let handle = tokio::spawn(async move {
        let (sock, _) = listener.accept().await.unwrap();
        handle_client(sock, store_clone, auth_clone, cfg, false)
            .await
            .unwrap();
    });
    (addr, handle)
}

pub async fn setup_server_with_cfg(
    storage: Arc<dyn Storage>,
    auth: Arc<dyn AuthProvider>,
    cfg: Arc<RwLock<Config>>,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let store_clone = storage.clone();
    let auth_clone = auth.clone();
    let handle = tokio::spawn(async move {
        let (sock, _) = listener.accept().await.unwrap();
        handle_client(sock, store_clone, auth_clone, cfg, false)
            .await
            .unwrap();
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
    String,
    tokio::task::JoinHandle<()>,
) {
    let CertifiedKey { cert, signing_key } =
        generate_simple_self_signed(["localhost".to_string()]).unwrap();
    let cert_der = cert.der().to_vec();
    let cert_pem = cert.pem();
    let key = signing_key.serialize_der();
    let tls_config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(
            vec![rustls::Certificate(cert_der.clone())],
            rustls::PrivateKey(key),
        )
        .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(tls_config));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let store_clone = storage.clone();
    let auth_clone = auth.clone();
    let cfg: Arc<RwLock<Config>> = Arc::new(RwLock::new(toml::from_str("port=119").unwrap()));
    let handle = tokio::spawn(async move {
        let (sock, _) = listener.accept().await.unwrap();
        let stream = acceptor.accept(sock).await.unwrap();
        handle_client(stream, store_clone, auth_clone, cfg, true)
            .await
            .unwrap();
    });
    (addr, rustls::Certificate(cert_der), cert_pem, handle)
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

/// Builder to mock a client connection using `tokio_test::io`.
pub struct ClientMock {
    steps: Vec<(String, Vec<String>)>,
}

impl ClientMock {
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    /// Expect a command with a single-line response.
    pub fn expect(mut self, cmd: &str, resp: &str) -> Self {
        self.steps
            .push((cmd.to_string(), vec![resp.to_string()]));
        self
    }

    /// Expect a command with a multi-line response.
    pub fn expect_multi<S: Into<String>>(mut self, cmd: &str, resp: Vec<S>) -> Self {
        self.steps.push((cmd.to_string(), resp.into_iter().map(Into::into).collect()));
        self
    }

    pub async fn run(self, storage: Arc<dyn Storage>, auth: Arc<dyn AuthProvider>) {
        use renews::config::Config;
        use tokio::sync::RwLock;
        let mut builder = IoBuilder::new();
        builder.write(b"201 NNTP Service Ready - no posting allowed\r\n");
        for (cmd, resps) in self.steps {
            builder.read(format!("{}\r\n", cmd).as_bytes());
            for line in resps {
                builder.write(format!("{}\r\n", line).as_bytes());
            }
        }
        builder.read(b"");
        let mock = builder.build();
        let cfg: Arc<RwLock<Config>> = Arc::new(RwLock::new(toml::from_str("port=119").unwrap()));
        renews::handle_client(mock, storage, auth, cfg, false)
            .await
            .unwrap();
    }
}
