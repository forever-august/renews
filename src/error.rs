//! Domain error types for the NNTP server
//!
//! Errors are structured internally for logging/debugging but provide
//! generic responses to clients to avoid leaking sensitive information.

use thiserror::Error;

/// Top-level server error type
#[derive(Error, Debug)]
pub enum NntpError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Validation error: {0}")]
    Validation(#[from] ValidationError),

    #[error("Authentication error: {0}")]
    Auth(#[from] AuthError),

    #[error("Limit error: {0}")]
    Limit(#[from] LimitError),

    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Protocol error: {0}")]
    Protocol(String),
}

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Article not found: {0}")]
    ArticleNotFound(String),

    #[error("Group not found: {0}")]
    GroupNotFound(String),

    #[error("Database error: {0}")]
    Database(#[source] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Missing required header: {0}")]
    MissingHeader(&'static str),

    #[error("Article size {actual} exceeds limit {limit}")]
    SizeExceeded { limit: u64, actual: u64 },

    #[error("Group does not exist: {0}")]
    GroupNotFound(String),

    #[error("Moderated group requires Approved header")]
    ModerationRequired,

    #[error("Invalid header format: {0}")]
    InvalidHeader(String),

    #[error("Filter rejected: {0}")]
    FilterRejected(String),
}

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Authentication required")]
    Required,

    #[error("Invalid credentials for user: {0}")]
    InvalidCredentials(String),

    #[error("User not found: {0}")]
    UserNotFound(String),

    #[error("Account disabled: {0}")]
    AccountDisabled(String),
}

#[derive(Error, Debug)]
pub enum LimitError {
    #[error("Posting disabled for user")]
    PostingDisabled,

    #[error("Bandwidth limit exceeded")]
    BandwidthExceeded,

    #[error("Connection limit exceeded")]
    ConnectionLimitExceeded,
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Invalid configuration: {0}")]
    Invalid(String),

    #[error("Missing required field: {0}")]
    MissingField(&'static str),

    #[error("File not found: {0}")]
    FileNotFound(String),
}

impl NntpError {
    /// Get the NNTP response code for this error
    pub fn response_code(&self) -> u16 {
        match self {
            NntpError::Storage(StorageError::ArticleNotFound(_)) => 430,
            NntpError::Storage(StorageError::GroupNotFound(_)) => 411,
            NntpError::Storage(_) => 403,

            NntpError::Validation(_) => 441,

            NntpError::Auth(AuthError::Required) => 480,
            NntpError::Auth(_) => 481,

            NntpError::Limit(LimitError::PostingDisabled) => 440,
            NntpError::Limit(LimitError::BandwidthExceeded) => 403,
            NntpError::Limit(LimitError::ConnectionLimitExceeded) => 481,

            NntpError::Config(_) => 403,
            NntpError::Io(_) => 403,
            NntpError::Protocol(_) => 500,
        }
    }

    /// Get a client-safe response message (generic, no internal details)
    pub fn client_message(&self) -> &'static str {
        match self {
            NntpError::Storage(StorageError::ArticleNotFound(_)) => "No such article",
            NntpError::Storage(StorageError::GroupNotFound(_)) => "No such group",
            NntpError::Storage(_) => "Service temporarily unavailable",

            NntpError::Validation(_) => "Posting failed",

            NntpError::Auth(AuthError::Required) => "authentication required",
            NntpError::Auth(_) => "Authentication failed",

            NntpError::Limit(LimitError::PostingDisabled) => "posting not allowed",
            NntpError::Limit(LimitError::BandwidthExceeded) => "bandwidth limit exceeded",
            NntpError::Limit(LimitError::ConnectionLimitExceeded) => "connection limit exceeded",

            NntpError::Config(_) => "Service temporarily unavailable",
            NntpError::Io(_) => "Service temporarily unavailable",
            NntpError::Protocol(_) => "Command not recognized",
        }
    }

    /// Format as NNTP response line (code + generic message)
    pub fn to_response(&self) -> String {
        format!("{} {}\r\n", self.response_code(), self.client_message())
    }
}
