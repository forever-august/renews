use crate::utils;
use renews::config::Config;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{sleep, timeout};

#[tokio::test]
async fn connection_times_out_after_idle_timeout() {
    let (storage, auth) = utils::setup().await;

    // Set a very short timeout for testing (2 seconds)
    let cfg: Config = toml::from_str(
        r#"
addr = ":119"
idle_timeout_secs = 2
"#,
    )
    .unwrap();

    let (addr, _, _handle) = utils::start_server(storage, auth, cfg, false).await;

    // Connect to the server
    let mut stream = TcpStream::connect(addr).await.unwrap();

    // Read the greeting
    let (reader, mut writer) = stream.split();
    let mut reader = tokio::io::BufReader::new(reader);
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.contains("201")); // Ready greeting

    // Wait longer than the timeout period without sending anything
    sleep(Duration::from_secs(3)).await;

    // Try to send a command - this should fail because the connection was closed
    let result = timeout(Duration::from_secs(1), writer.write_all(b"HELP\r\n")).await;

    match result {
        Ok(Ok(_)) => {
            // If write succeeded, try to read response - should fail due to closed connection
            line.clear();
            let read_result = timeout(Duration::from_secs(1), reader.read_line(&mut line)).await;
            assert!(
                read_result.is_err() || read_result.unwrap().unwrap() == 0,
                "Connection should be closed after timeout"
            );
        }
        Ok(Err(_)) => {
            // Write failed as expected - connection was closed
        }
        Err(_) => {
            // Write timed out - also indicates connection issues
        }
    }
}

#[tokio::test]
async fn connection_stays_alive_with_activity() {
    let (storage, auth) = utils::setup().await;

    // Set a short timeout for testing (3 seconds)
    let cfg: Config = toml::from_str(
        r#"
addr = ":119"
idle_timeout_secs = 3
"#,
    )
    .unwrap();

    let (addr, _, _handle) = utils::start_server(storage, auth, cfg, false).await;

    // Connect to the server
    let mut stream = TcpStream::connect(addr).await.unwrap();

    // Read the greeting
    let (reader, mut writer) = stream.split();
    let mut reader = tokio::io::BufReader::new(reader);
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.contains("201")); // Ready greeting

    // Send a command before timeout
    sleep(Duration::from_secs(1)).await;
    writer.write_all(b"HELP\r\n").await.unwrap();

    // Read help response - it's multi-line, so read until we get the end marker
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        if line.starts_with("100") {
            // Start of help response
            continue;
        }
        if line.trim() == "." {
            // End of help response
            break;
        }
    }

    // Wait again and send another command (QUIT to get a simple response)
    sleep(Duration::from_secs(1)).await;
    writer.write_all(b"QUIT\r\n").await.unwrap();

    // Read response
    line.clear();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.contains("205")); // Closing connection - this confirms connection was alive
}
