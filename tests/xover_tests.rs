//! Tests for XOVER command implementation

use renews::auth::sqlite::SqliteAuth;
use renews::handlers::{DynReader, DynWriter, HandlerContext, dispatch_command};
use renews::queue::ArticleQueue;
use renews::session::Session;
use renews::storage::open;
use renews::{Message, parse_command};
use smallvec::smallvec;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::io::{self, AsyncWrite};
use tokio::sync::{Mutex, RwLock};

// Helper to create a test article
fn create_test_article(subject: &str, from: &str, message_id: &str, group: &str) -> Message {
    Message {
        headers: smallvec![
            ("Subject".to_string(), subject.to_string()),
            ("From".to_string(), from.to_string()),
            (
                "Date".to_string(),
                "Mon, 1 Jan 2024 12:00:00 +0000".to_string()
            ),
            ("Message-ID".to_string(), message_id.to_string()),
            ("Newsgroups".to_string(), group.to_string()),
        ],
        body: "This is a test article body.\nWith multiple lines.".to_string(),
    }
}

// Helper to create a mock writer that captures output using shared buffer
struct MockWriter {
    buffer: Arc<Mutex<Vec<u8>>>,
}

impl MockWriter {
    fn new(buffer: Arc<Mutex<Vec<u8>>>) -> Self {
        Self { buffer }
    }
}

impl AsyncWrite for MockWriter {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, io::Error>> {
        // Use blocking lock since we're in a poll context
        if let Ok(mut buffer) = self.buffer.try_lock() {
            buffer.extend_from_slice(buf);
            std::task::Poll::Ready(Ok(buf.len()))
        } else {
            std::task::Poll::Pending
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), io::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), io::Error>> {
        std::task::Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn test_xover_command_basic() {
    // Create temporary database
    let db_file = NamedTempFile::new().unwrap();
    let db_path = format!("sqlite://{}", db_file.path().display());
    let storage = open(&db_path).await.unwrap();

    // Add test group
    storage.add_group("test.group", false).await.unwrap();

    // Store test articles
    let article1 = create_test_article(
        "Test Subject 1",
        "user1@example.com",
        "<msg1@example.com>",
        "test.group",
    );
    let article2 = create_test_article(
        "Test Subject 2",
        "user2@example.com",
        "<msg2@example.com>",
        "test.group",
    );

    storage.store_article(&article1).await.unwrap();
    storage.store_article(&article2).await.unwrap();

    // Create test context
    let config = Arc::new(RwLock::new(toml::from_str("addr=\":119\"").unwrap()));
    let auth = SqliteAuth::new(":memory:").await.unwrap();
    let queue = ArticleQueue::new(1000);

    let buffer = Arc::new(Mutex::new(Vec::new()));
    let reader: DynReader = Box::pin(io::empty());
    let writer: DynWriter = Box::pin(MockWriter::new(buffer.clone()));

    let mut ctx = HandlerContext {
        reader,
        writer,
        storage,
        auth: Arc::new(auth),
        config,
        session: {
            let mut s = Session::new(false, false);
            s.select_group("test.group".to_string(), Some(1));
            s
        },
        queue,
    };

    // Test XOVER command with range
    let (_, cmd) = parse_command("XOVER 1-2").unwrap();
    dispatch_command(&mut ctx, &cmd).await.unwrap();

    // Get output from the shared buffer
    let output = String::from_utf8_lossy(&buffer.lock().await).to_string();

    // Verify output contains overview response
    assert!(output.contains("224 Overview information follows"));
    assert!(output.contains("Test Subject 1"));
    assert!(output.contains("Test Subject 2"));
    assert!(output.contains("user1@example.com"));
    assert!(output.contains("user2@example.com"));
    assert!(output.contains("<msg1@example.com>"));
    assert!(output.contains("<msg2@example.com>"));
    assert!(output.ends_with(".\r\n"));
}

#[tokio::test]
async fn test_xover_without_group() {
    // Create temporary database
    let db_file = NamedTempFile::new().unwrap();
    let db_path = format!("sqlite://{}", db_file.path().display());
    let storage = open(&db_path).await.unwrap();

    // Create test context without current group
    let config = Arc::new(RwLock::new(toml::from_str("addr=\":119\"").unwrap()));
    let auth = SqliteAuth::new(":memory:").await.unwrap();
    let queue = ArticleQueue::new(1000);

    let buffer = Arc::new(Mutex::new(Vec::new()));
    let reader: DynReader = Box::pin(io::empty());
    let writer: DynWriter = Box::pin(MockWriter::new(buffer.clone()));

    let mut ctx = HandlerContext {
        reader,
        writer,
        storage,
        auth: Arc::new(auth),
        config,
        session: Session::new(false, false),
        queue,
    };

    // Test XOVER command without current group
    let (_, cmd) = parse_command("XOVER 1-2").unwrap();
    dispatch_command(&mut ctx, &cmd).await.unwrap();

    let output = String::from_utf8_lossy(&buffer.lock().await).to_string();

    // Should get appropriate error response
    assert!(output.contains("412") || output.contains("500") || output.contains("501"));
}

#[tokio::test]
async fn test_xover_single_article() {
    // Create temporary database
    let db_file = NamedTempFile::new().unwrap();
    let db_path = format!("sqlite://{}", db_file.path().display());
    let storage = open(&db_path).await.unwrap();

    // Add test group
    storage.add_group("test.group", false).await.unwrap();

    // Store test article
    let article = create_test_article(
        "Single Test",
        "single@example.com",
        "<single@example.com>",
        "test.group",
    );
    storage.store_article(&article).await.unwrap();

    // Create test context
    let config = Arc::new(RwLock::new(toml::from_str("addr=\":119\"").unwrap()));
    let auth = SqliteAuth::new(":memory:").await.unwrap();
    let queue = ArticleQueue::new(1000);

    let buffer = Arc::new(Mutex::new(Vec::new()));
    let reader: DynReader = Box::pin(io::empty());
    let writer: DynWriter = Box::pin(MockWriter::new(buffer.clone()));

    let mut ctx = HandlerContext {
        reader,
        writer,
        storage,
        auth: Arc::new(auth),
        config,
        session: {
            let mut s = Session::new(false, false);
            s.select_group("test.group".to_string(), Some(1));
            s
        },
        queue,
    };

    // Test XOVER command with single article
    let (_, cmd) = parse_command("XOVER 1").unwrap();
    dispatch_command(&mut ctx, &cmd).await.unwrap();

    let output = String::from_utf8_lossy(&buffer.lock().await).to_string();

    // Verify output
    assert!(output.contains("224 Overview information follows"));
    assert!(output.contains("Single Test"));
    assert!(output.contains("single@example.com"));
    assert!(output.contains("<single@example.com>"));
    assert!(output.ends_with(".\r\n"));
}

#[tokio::test]
async fn test_xover_current_article() {
    // Create temporary database
    let db_file = NamedTempFile::new().unwrap();
    let db_path = format!("sqlite://{}", db_file.path().display());
    let storage = open(&db_path).await.unwrap();

    // Add test group
    storage.add_group("test.group", false).await.unwrap();

    // Store test article
    let article = create_test_article(
        "Current Test",
        "current@example.com",
        "<current@example.com>",
        "test.group",
    );
    storage.store_article(&article).await.unwrap();

    // Create test context
    let config = Arc::new(RwLock::new(toml::from_str("addr=\":119\"").unwrap()));
    let auth = SqliteAuth::new(":memory:").await.unwrap();
    let queue = ArticleQueue::new(1000);

    let buffer = Arc::new(Mutex::new(Vec::new()));
    let reader: DynReader = Box::pin(io::empty());
    let writer: DynWriter = Box::pin(MockWriter::new(buffer.clone()));

    let mut ctx = HandlerContext {
        reader,
        writer,
        storage,
        auth: Arc::new(auth),
        config,
        session: {
            let mut s = Session::new(false, false);
            s.select_group("test.group".to_string(), Some(1));
            s
        },
        queue,
    };

    // Test XOVER command without arguments (current article)
    let (_, cmd) = parse_command("XOVER").unwrap();
    dispatch_command(&mut ctx, &cmd).await.unwrap();

    let output = String::from_utf8_lossy(&buffer.lock().await).to_string();

    // Verify output
    assert!(output.contains("224 Overview information follows"));
    assert!(output.contains("Current Test"));
    assert!(output.contains("current@example.com"));
    assert!(output.contains("<current@example.com>"));
    assert!(output.ends_with(".\r\n"));
}
