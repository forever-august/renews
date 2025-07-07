#![allow(dead_code)]

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

/// Create an in-memory storage and auth provider pair for tests.
pub async fn setup() -> (Arc<dyn Storage>, Arc<dyn AuthProvider>) {
    use renews::auth::sqlite::SqliteAuth;
    use renews::storage::sqlite::SqliteStorage;

    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let auth = Arc::new(SqliteAuth::new("sqlite::memory:").await.unwrap());
    (storage as _, auth as _)
}

/// Lines returned by the CAPABILITIES command.
pub fn capabilities_lines() -> Vec<String> {
    vec![
        "101 Capability list follows".into(),
        "VERSION 2".into(),
        format!("IMPLEMENTATION Renews {}", env!("CARGO_PKG_VERSION")),
        "READER".into(),
        "NEWNEWS".into(),
        "IHAVE".into(),
        "STREAMING".into(),
        "OVER MSGID".into(),
        "HDR".into(),
        "LIST ACTIVE NEWSGROUPS ACTIVE.TIMES OVERVIEW.FMT HEADERS".into(),
        ".".into(),
    ]
}

/// Split a request string into individual lines.
pub fn request_lines(text: &str) -> Vec<String> {
    text.split("\r\n").map(|l| l.to_string()).collect()
}

/// Build a detached PGP signature for the provided data.
pub fn build_sig(data: &str) -> (String, Vec<String>) {
    use pgp::composed::{Deserializable, SignedSecretKey, StandaloneSignature};
    use pgp::packet::SignatureConfig;
    use pgp::packet::SignatureType;
    use pgp::types::Password;
    use rand::thread_rng;

    const ADMIN_SEC: &str = include_str!("integration/../data/admin.sec.asc");

    let (key, _) = SignedSecretKey::from_string(ADMIN_SEC).unwrap();
    let cfg =
        SignatureConfig::from_key(thread_rng(), &key.primary_key, SignatureType::Binary).unwrap();
    let sig = cfg
        .sign(&key.primary_key, &Password::empty(), data.as_bytes())
        .unwrap();
    let armored = StandaloneSignature::new(sig)
        .to_armored_string(Default::default())
        .unwrap();
    let version = "1".to_string();
    let mut lines = Vec::new();
    for line in armored.lines() {
        if line.starts_with("-----BEGIN") || line.starts_with("Version") || line.is_empty() {
            continue;
        }
        if line.starts_with("-----END") {
            break;
        }
        lines.push(line.to_string());
    }
    (version, lines)
}

/// Generate a self-signed TLS certificate for use in tests.
pub fn generate_self_signed_cert() -> (rustls::Certificate, rustls::PrivateKey, String) {
    let CertifiedKey { cert, signing_key } =
        generate_simple_self_signed(["localhost".to_string()]).unwrap();
    let cert_der = cert.der().to_vec();
    let key_der = signing_key.serialize_der();
    let pem = cert.pem();
    (
        rustls::Certificate(cert_der),
        rustls::PrivateKey(key_der),
        pem,
    )
}

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

pub async fn setup_tls_server_with_cert(
    storage: Arc<dyn Storage>,
    auth: Arc<dyn AuthProvider>,
    cert: rustls::Certificate,
    key: rustls::PrivateKey,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let tls_config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(vec![cert.clone()], key)
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
    (addr, handle)
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
    let (cert, key, pem) = generate_self_signed_cert();
    let (addr, handle) = setup_tls_server_with_cert(storage, auth, cert.clone(), key).await;
    (addr, cert, pem, handle)
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

/// Start a new NNTP server for testing.
pub async fn start_server(
    storage: Arc<dyn Storage>,
    auth: Arc<dyn AuthProvider>,
    cfg: Config,
    tls: bool,
) -> (
    std::net::SocketAddr,
    Option<(rustls::Certificate, String)>,
    tokio::task::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let cfg = Arc::new(RwLock::new(cfg));
    let store_clone = storage.clone();
    let auth_clone = auth.clone();
    if tls {
        let (cert, key, pem) = generate_self_signed_cert();
        let tls_config = rustls::ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(vec![cert.clone()], key)
            .unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(tls_config));
        let handle = tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            let stream = acceptor.accept(sock).await.unwrap();
            handle_client(stream, store_clone, auth_clone, cfg, true)
                .await
                .unwrap();
        });
        (addr, Some((cert, pem)), handle)
    } else {
        let handle = tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            handle_client(sock, store_clone, auth_clone, cfg, false)
                .await
                .unwrap();
        });
        (addr, None, handle)
    }
}

pub async fn run_client(
    client: ClientMock,
    storage: Arc<dyn Storage>,
    auth: Arc<dyn AuthProvider>,
) {
    run_client_with_cfg(client, toml::from_str("port=119").unwrap(), storage, auth, false).await;
}

pub async fn run_client_tls(
    client: ClientMock,
    storage: Arc<dyn Storage>,
    auth: Arc<dyn AuthProvider>,
) {
    run_client_with_cfg(client, toml::from_str("port=119").unwrap(), storage, auth, true).await;
}

pub async fn run_client_with_cfg(
    client: ClientMock,
    cfg: Config,
    storage: Arc<dyn Storage>,
    auth: Arc<dyn AuthProvider>,
    tls: bool,
) {
    let (addr, cert, handle) = start_server(storage, auth, cfg, tls).await;
    if let Some((c, _)) = cert {
        client.run_tls_at(addr, c).await;
    } else {
        client.run_tcp_at(addr).await;
    }
    handle.await.unwrap();
}

pub async fn run_client_with_cfg_tls(
    client: ClientMock,
    cfg: Config,
    storage: Arc<dyn Storage>,
    auth: Arc<dyn AuthProvider>,
) {
    run_client_with_cfg(client, cfg, storage, auth, true).await;
}

impl ClientMock {
    pub async fn run(
        self,
        storage: Arc<dyn Storage>,
        auth: Arc<dyn AuthProvider>,
    ) {
        run_client(self, storage, auth).await;
    }

    pub async fn run_tls(
        self,
        storage: Arc<dyn Storage>,
        auth: Arc<dyn AuthProvider>,
    ) {
        run_client_tls(self, storage, auth).await;
    }

    pub async fn run_with_cfg(
        self,
        cfg: Config,
        storage: Arc<dyn Storage>,
        auth: Arc<dyn AuthProvider>,
    ) {
        run_client_with_cfg(self, cfg, storage, auth, false).await;
    }

    pub async fn run_with_cfg_tls(
        self,
        cfg: Config,
        storage: Arc<dyn Storage>,
        auth: Arc<dyn AuthProvider>,
    ) {
        run_client_with_cfg_tls(self, cfg, storage, auth).await;
    }
}

/// Builder to mock a client connection using `tokio_test::io`.
pub struct ClientMock {
    steps: Vec<(Vec<String>, Vec<String>)>,
}

impl Default for ClientMock {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientMock {
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    /// Expect a command with a single-line response.
    pub fn expect(mut self, cmd: &str, resp: &str) -> Self {
        self.steps
            .push((vec![cmd.to_string()], vec![resp.to_string()]));
        self
    }

    /// Expect a command with a multi-line response.
    pub fn expect_multi<S: Into<String>>(mut self, cmd: &str, resp: Vec<S>) -> Self {
        self.steps.push((
            vec![cmd.to_string()],
            resp.into_iter().map(Into::into).collect(),
        ));
        self
    }

    /// Expect a multi-line request with optional multi-line response.
    pub fn expect_request_multi<R, S>(mut self, cmds: Vec<R>, resp: Vec<S>) -> Self
    where
        R: Into<String>,
        S: Into<String>,
    {
        self.steps.push((
            cmds.into_iter().map(Into::into).collect(),
            resp.into_iter().map(Into::into).collect(),
        ));
        self
    }

    pub async fn drive<R, W>(self, mut reader: R, mut writer: W)
    where
        R: tokio::io::AsyncBufRead + Unpin,
        W: tokio::io::AsyncWrite + Unpin,
    {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        for (cmds, resps) in self.steps {
            for cmd in cmds {
                writer
                    .write_all(format!("{cmd}\r\n").as_bytes())
                    .await
                    .unwrap();
            }
            for resp in resps {
                line.clear();
                reader.read_line(&mut line).await.unwrap();
                assert_eq!(line.trim_end_matches(['\r', '\n']), resp);
            }
        }
        let _ = writer.shutdown().await;
    }

    pub async fn run_tcp_at(self, addr: std::net::SocketAddr) {
        let (reader, writer) = connect(addr).await;
        self.drive(reader, writer).await;
    }

    pub async fn run_tls_at(self, addr: std::net::SocketAddr, cert: rustls::Certificate) {
        let (reader, writer) = connect_tls(addr, cert).await;
        self.drive(reader, writer).await;
    }
}
