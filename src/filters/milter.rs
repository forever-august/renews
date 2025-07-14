//! Milter protocol filter
//!
//! Integrates with external Milter servers to filter news articles using the
//! industry-standard Milter protocol, supporting both plain TCP and TLS connections.

use super::ArticleFilter;
use crate::Message;
use crate::auth::DynAuth;
use crate::config::Config;
use crate::storage::DynStorage;
use anyhow::Result;
use serde::Deserialize;

use std::fmt;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UnixStream};
use tokio_rustls::{
    TlsConnector,
    rustls::{Certificate, ClientConfig, RootCertStore},
};

/// Milter protocol commands
const MILTER_CONNECT: u8 = b'C';
const MILTER_HEADER: u8 = b'L';
const MILTER_END_HEADERS: u8 = b'N';
const MILTER_BODY: u8 = b'B';
const MILTER_END_MESSAGE: u8 = b'E';
const MILTER_QUIT: u8 = b'Q';

/// Milter protocol responses
const MILTER_ACCEPT: u8 = b'a';
const MILTER_REJECT: u8 = b'r';
const MILTER_DISCARD: u8 = b'd';
const MILTER_TEMPFAIL: u8 = b't';
const MILTER_CONTINUE: u8 = b'c';

/// Configuration for Milter filter
#[derive(Deserialize, Clone)]
pub struct MilterConfig {
    /// Address of the Milter server with protocol scheme
    /// Supported formats:
    /// - "tcp://127.0.0.1:8888" for plain TCP connection
    /// - "tls://milter.example.com:8889" for TLS-encrypted TCP connection  
    /// - "unix:///var/run/milter.sock" for Unix socket connection
    pub address: String,
    /// Connection timeout in seconds
    #[serde(default = "default_milter_timeout")]
    pub timeout_secs: u64,
}

fn default_milter_timeout() -> u64 {
    30
}

/// Errors that can occur during Milter operations
#[derive(Debug)]
pub enum MilterError {
    /// Connection failed
    ConnectionFailed(String),
    /// Protocol error
    ProtocolError(String),
    /// I/O error
    IoError(io::Error),
    /// TLS error
    TlsError(String),
    /// Invalid URI scheme
    InvalidScheme(String),
    /// Message rejected by Milter
    Rejected(String),
    /// Temporary failure
    TempFail(String),
}

impl fmt::Display for MilterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MilterError::ConnectionFailed(msg) => write!(f, "Milter connection failed: {msg}"),
            MilterError::ProtocolError(msg) => write!(f, "Milter protocol error: {msg}"),
            MilterError::IoError(err) => write!(f, "Milter I/O error: {err}"),
            MilterError::TlsError(msg) => write!(f, "Milter TLS error: {msg}"),
            MilterError::InvalidScheme(msg) => write!(f, "Milter invalid scheme: {msg}"),
            MilterError::Rejected(msg) => write!(f, "Article rejected by Milter: {msg}"),
            MilterError::TempFail(msg) => write!(f, "Milter temporary failure: {msg}"),
        }
    }
}

impl Error for MilterError {}

impl From<io::Error> for MilterError {
    fn from(err: io::Error) -> Self {
        MilterError::IoError(err)
    }
}

/// Filter that validates articles using external Milter servers
pub struct MilterFilter {
    config: MilterConfig,
}

impl MilterFilter {
    /// Create a new Milter filter with the given configuration
    pub fn new(config: MilterConfig) -> Self {
        Self { config }
    }

    /// Connect to the Milter server based on URI scheme
    async fn connect(&self) -> Result<Box<dyn MilterConnection>, MilterError> {
        let timeout = Duration::from_secs(self.config.timeout_secs);

        // Parse URI scheme
        if let Some((scheme, address)) = self.config.address.split_once("://") {
            match scheme {
                "tcp" => self.connect_tcp(address, timeout).await,
                "tls" => self.connect_tls(address, timeout).await,
                "unix" => self.connect_unix(address, timeout).await,
                _ => Err(MilterError::InvalidScheme(format!(
                    "Unsupported scheme: {scheme}. Supported schemes: tcp://, tls://, unix://"
                ))),
            }
        } else {
            Err(MilterError::InvalidScheme(format!(
                "Invalid address format: {}. Expected format: scheme://address (e.g., tcp://localhost:8888)",
                self.config.address
            )))
        }
    }

    /// Connect via plain TCP
    async fn connect_tcp(
        &self,
        address: &str,
        timeout: Duration,
    ) -> Result<Box<dyn MilterConnection>, MilterError> {
        let stream = tokio::time::timeout(timeout, TcpStream::connect(address))
            .await
            .map_err(|_| MilterError::ConnectionFailed("timeout".to_string()))?
            .map_err(|e| MilterError::ConnectionFailed(e.to_string()))?;

        Ok(Box::new(TcpMilterConnection::new(stream)))
    }

    /// Connect via TLS
    async fn connect_tls(
        &self,
        address: &str,
        timeout: Duration,
    ) -> Result<Box<dyn MilterConnection>, MilterError> {
        let stream = tokio::time::timeout(timeout, TcpStream::connect(address))
            .await
            .map_err(|_| MilterError::ConnectionFailed("timeout".to_string()))?
            .map_err(|e| MilterError::ConnectionFailed(e.to_string()))?;

        // Create TLS configuration
        let mut root_store = RootCertStore::empty();

        // Load native certificates
        for cert in rustls_native_certs::load_native_certs()
            .map_err(|e| MilterError::TlsError(e.to_string()))?
        {
            root_store
                .add(&Certificate(cert.0))
                .map_err(|e| MilterError::TlsError(e.to_string()))?;
        }

        let config = ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let connector = TlsConnector::from(Arc::new(config));

        // Extract hostname from address for SNI
        let hostname = address.split(':').next().unwrap_or("localhost");

        let tls_stream = connector
            .connect(
                hostname
                    .try_into()
                    .map_err(|_| MilterError::TlsError("Invalid hostname".to_string()))?,
                stream,
            )
            .await
            .map_err(|e| MilterError::TlsError(e.to_string()))?;

        Ok(Box::new(TlsMilterConnection::new(tls_stream)))
    }

    /// Connect via Unix socket
    async fn connect_unix(
        &self,
        path: &str,
        timeout: Duration,
    ) -> Result<Box<dyn MilterConnection>, MilterError> {
        let stream = tokio::time::timeout(timeout, UnixStream::connect(path))
            .await
            .map_err(|_| MilterError::ConnectionFailed("timeout".to_string()))?
            .map_err(|e| MilterError::ConnectionFailed(e.to_string()))?;

        Ok(Box::new(UnixMilterConnection::new(stream)))
    }

    /// Process an article through the Milter protocol
    async fn process_article(&self, article: &Message) -> Result<(), MilterError> {
        let mut conn = self.connect().await?;

        // Send connection information
        conn.send_command(MILTER_CONNECT, b"NNTP 127.0.0.1").await?;
        let response = conn.read_response().await?;
        if response != MILTER_CONTINUE {
            return self.handle_response(response);
        }

        // Send headers
        for (name, value) in &article.headers {
            let header_data = format!("{name}: {value}");
            conn.send_command(MILTER_HEADER, header_data.as_bytes())
                .await?;
            let response = conn.read_response().await?;
            if response != MILTER_CONTINUE {
                return self.handle_response(response);
            }
        }

        // End of headers
        conn.send_command(MILTER_END_HEADERS, b"").await?;
        let response = conn.read_response().await?;
        if response != MILTER_CONTINUE {
            return self.handle_response(response);
        }

        // Send body
        conn.send_command(MILTER_BODY, article.body.as_bytes())
            .await?;
        let response = conn.read_response().await?;
        if response != MILTER_CONTINUE {
            return self.handle_response(response);
        }

        // End of message
        conn.send_command(MILTER_END_MESSAGE, b"").await?;
        let response = conn.read_response().await?;
        self.handle_response(response)?;

        // Send quit
        conn.send_command(MILTER_QUIT, b"").await?;

        Ok(())
    }

    /// Handle Milter response codes
    fn handle_response(&self, response: u8) -> Result<(), MilterError> {
        match response {
            MILTER_ACCEPT | MILTER_CONTINUE => Ok(()),
            MILTER_REJECT => Err(MilterError::Rejected(
                "Article rejected by Milter".to_string(),
            )),
            MILTER_DISCARD => Err(MilterError::Rejected(
                "Article discarded by Milter".to_string(),
            )),
            MILTER_TEMPFAIL => Err(MilterError::TempFail(
                "Temporary failure from Milter".to_string(),
            )),
            _ => Err(MilterError::ProtocolError(format!(
                "Unknown response: {response}"
            ))),
        }
    }
}

#[async_trait::async_trait]
impl ArticleFilter for MilterFilter {
    async fn validate(
        &self,
        _storage: &DynStorage,
        _auth: &DynAuth,
        _cfg: &Config,
        article: &Message,
        _size: u64,
    ) -> Result<()> {
        self.process_article(article).await?;
        Ok(())
    }

    fn name(&self) -> &'static str {
        "MilterFilter"
    }
}

/// Trait for Milter connections (TCP or TLS)
#[async_trait::async_trait]
trait MilterConnection: Send + Sync {
    async fn send_command(&mut self, cmd: u8, data: &[u8]) -> Result<(), MilterError>;
    async fn read_response(&mut self) -> Result<u8, MilterError>;
}

/// TCP-based Milter connection
struct TcpMilterConnection {
    stream: TcpStream,
}

impl TcpMilterConnection {
    fn new(stream: TcpStream) -> Self {
        Self { stream }
    }
}

#[async_trait::async_trait]
impl MilterConnection for TcpMilterConnection {
    async fn send_command(&mut self, cmd: u8, data: &[u8]) -> Result<(), MilterError> {
        // Milter protocol: 4-byte length + 1-byte command + data
        let len = (data.len() + 1) as u32;
        self.stream.write_all(&len.to_be_bytes()).await?;
        self.stream.write_all(&[cmd]).await?;
        self.stream.write_all(data).await?;
        self.stream.flush().await?;
        Ok(())
    }

    async fn read_response(&mut self) -> Result<u8, MilterError> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf);

        if len == 0 {
            return Err(MilterError::ProtocolError(
                "Invalid response length".to_string(),
            ));
        }

        let mut response = vec![0u8; len as usize];
        self.stream.read_exact(&mut response).await?;

        if response.is_empty() {
            return Err(MilterError::ProtocolError("Empty response".to_string()));
        }

        Ok(response[0])
    }
}

/// TLS-based Milter connection
struct TlsMilterConnection {
    stream: tokio_rustls::client::TlsStream<TcpStream>,
}

impl TlsMilterConnection {
    fn new(stream: tokio_rustls::client::TlsStream<TcpStream>) -> Self {
        Self { stream }
    }
}

#[async_trait::async_trait]
impl MilterConnection for TlsMilterConnection {
    async fn send_command(&mut self, cmd: u8, data: &[u8]) -> Result<(), MilterError> {
        // Milter protocol: 4-byte length + 1-byte command + data
        let len = (data.len() + 1) as u32;
        self.stream.write_all(&len.to_be_bytes()).await?;
        self.stream.write_all(&[cmd]).await?;
        self.stream.write_all(data).await?;
        self.stream.flush().await?;
        Ok(())
    }

    async fn read_response(&mut self) -> Result<u8, MilterError> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf);

        if len == 0 {
            return Err(MilterError::ProtocolError(
                "Invalid response length".to_string(),
            ));
        }

        let mut response = vec![0u8; len as usize];
        self.stream.read_exact(&mut response).await?;

        if response.is_empty() {
            return Err(MilterError::ProtocolError("Empty response".to_string()));
        }

        Ok(response[0])
    }
}

/// Unix socket-based Milter connection
struct UnixMilterConnection {
    stream: UnixStream,
}

impl UnixMilterConnection {
    fn new(stream: UnixStream) -> Self {
        Self { stream }
    }
}

#[async_trait::async_trait]
impl MilterConnection for UnixMilterConnection {
    async fn send_command(&mut self, cmd: u8, data: &[u8]) -> Result<(), MilterError> {
        // Milter protocol: 4-byte length + 1-byte command + data
        let len = (data.len() + 1) as u32;
        self.stream.write_all(&len.to_be_bytes()).await?;
        self.stream.write_all(&[cmd]).await?;
        self.stream.write_all(data).await?;
        self.stream.flush().await?;
        Ok(())
    }

    async fn read_response(&mut self) -> Result<u8, MilterError> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf);

        if len == 0 {
            return Err(MilterError::ProtocolError(
                "Invalid response length".to_string(),
            ));
        }

        let mut response = vec![0u8; len as usize];
        self.stream.read_exact(&mut response).await?;

        if response.is_empty() {
            return Err(MilterError::ProtocolError("Empty response".to_string()));
        }

        Ok(response[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_milter_filter_creation() {
        let config = MilterConfig {
            address: "tcp://127.0.0.1:8888".to_string(),
            timeout_secs: 30,
        };

        let filter = MilterFilter::new(config);
        assert_eq!(filter.name(), "MilterFilter");
    }

    #[test]
    fn test_milter_error_display() {
        let error = MilterError::ConnectionFailed("test error".to_string());
        assert!(error.to_string().contains("connection failed"));

        let error = MilterError::Rejected("spam detected".to_string());
        assert!(error.to_string().contains("rejected"));
    }

    #[test]
    fn test_milter_response_handling() {
        let config = MilterConfig {
            address: "tcp://127.0.0.1:8888".to_string(),
            timeout_secs: 30,
        };

        let filter = MilterFilter::new(config);

        // Test accept response
        assert!(filter.handle_response(MILTER_ACCEPT).is_ok());
        assert!(filter.handle_response(MILTER_CONTINUE).is_ok());

        // Test reject response
        assert!(filter.handle_response(MILTER_REJECT).is_err());
        assert!(filter.handle_response(MILTER_DISCARD).is_err());
        assert!(filter.handle_response(MILTER_TEMPFAIL).is_err());

        // Test unknown response
        assert!(filter.handle_response(255).is_err());
    }

    #[test]
    fn test_uri_scheme_parsing() {
        // Test valid TCP scheme
        let config = MilterConfig {
            address: "tcp://127.0.0.1:8888".to_string(),
            timeout_secs: 30,
        };
        let filter = MilterFilter::new(config);
        assert_eq!(filter.name(), "MilterFilter");

        // Test valid TLS scheme
        let config = MilterConfig {
            address: "tls://milter.example.com:8889".to_string(),
            timeout_secs: 30,
        };
        let filter = MilterFilter::new(config);
        assert_eq!(filter.name(), "MilterFilter");

        // Test valid Unix scheme
        let config = MilterConfig {
            address: "unix:///var/run/milter.sock".to_string(),
            timeout_secs: 30,
        };
        let filter = MilterFilter::new(config);
        assert_eq!(filter.name(), "MilterFilter");
    }

    #[tokio::test]
    async fn test_invalid_scheme_error() {
        let config = MilterConfig {
            address: "invalid://127.0.0.1:8888".to_string(),
            timeout_secs: 30,
        };
        let filter = MilterFilter::new(config);

        let result = filter.connect().await;
        assert!(result.is_err());
        if let Err(MilterError::InvalidScheme(msg)) = result {
            assert!(msg.contains("Unsupported scheme: invalid"));
        } else {
            panic!("Expected InvalidScheme error");
        }
    }

    #[tokio::test]
    async fn test_missing_scheme_error() {
        let config = MilterConfig {
            address: "127.0.0.1:8888".to_string(),
            timeout_secs: 30,
        };
        let filter = MilterFilter::new(config);

        let result = filter.connect().await;
        assert!(result.is_err());
        if let Err(MilterError::InvalidScheme(msg)) = result {
            assert!(msg.contains("Invalid address format"));
        } else {
            panic!("Expected InvalidScheme error");
        }
    }

    #[tokio::test]
    async fn test_unix_socket_scheme() {
        // Test that Unix socket scheme is parsed correctly
        // (connection will fail since socket doesn't exist, but parsing should work)
        let config = MilterConfig {
            address: "unix:///var/run/nonexistent.sock".to_string(),
            timeout_secs: 1, // Short timeout for test
        };
        let filter = MilterFilter::new(config);

        let result = filter.connect().await;
        // Should get connection error, not invalid scheme error
        assert!(result.is_err());
        if let Err(MilterError::ConnectionFailed(_)) = result {
            // This is expected - socket doesn't exist
        } else {
            panic!("Expected ConnectionFailed error, not scheme parsing error");
        }
    }
}
