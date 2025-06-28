use renews::{handle_client, parse_message};
use renews::storage::{sqlite::SqliteStorage, Storage};
use chrono::{Duration, Utc};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

#[tokio::test]
async fn head_and_list_commands() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc").await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\nSubject: T\r\n\r\nBody").unwrap();
    storage.store_article("misc", &msg).await.unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let store_clone = storage.clone();
    tokio::spawn(async move {
        let (sock, _) = listener.accept().await.unwrap();
        handle_client(sock, store_clone).await.unwrap();
    });

    let stream = TcpStream::connect(addr).await.unwrap();
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("200"));
    line.clear();

    write_half.write_all(b"MODE READER\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("200"));
    line.clear();

    write_half.write_all(b"GROUP misc\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("211"));
    line.clear();

    write_half.write_all(b"HEAD 1\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("221"));
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        if line.trim_end() == "." {
            break;
        }
    }
    line.clear();

    write_half.write_all(b"LIST\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("215"));
    let mut found = false;
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        let trimmed = line.trim_end();
        if trimmed == "." {
            break;
        }
        if trimmed.starts_with("misc") {
            found = true;
        }
    }
    assert!(found);

    line.clear();
    write_half.write_all(b"QUIT\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("205"));
}

#[tokio::test]
async fn listgroup_and_navigation_commands() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc").await.unwrap();
    let (_, m1) = parse_message("Message-ID: <1@test>\r\n\r\nA").unwrap();
    let (_, m2) = parse_message("Message-ID: <2@test>\r\n\r\nB").unwrap();
    storage.store_article("misc", &m1).await.unwrap();
    storage.store_article("misc", &m2).await.unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let store_clone = storage.clone();
    tokio::spawn(async move {
        let (sock, _) = listener.accept().await.unwrap();
        handle_client(sock, store_clone).await.unwrap();
    });

    let stream = TcpStream::connect(addr).await.unwrap();
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    reader.read_line(&mut line).await.unwrap();
    line.clear();

    write_half.write_all(b"MODE READER\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();

    write_half.write_all(b"GROUP misc\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();

    write_half.write_all(b"LISTGROUP\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("211"));
    let mut nums = Vec::new();
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        let trimmed = line.trim_end();
        if trimmed == "." {
            break;
        }
        nums.push(trimmed.to_string());
    }
    assert_eq!(nums, vec!["1", "2"]);

    line.clear();
    write_half.write_all(b"HEAD 1\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        if line.trim_end() == "." {
            break;
        }
    }
    line.clear();

    write_half.write_all(b"NEXT\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("223"));
    line.clear();

    write_half.write_all(b"LAST\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("223"));
    line.clear();

    // requesting all groups since the epoch should return the "misc" group
    write_half.write_all(b"NEWGROUPS 19700101 000000\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("231"));
    let mut groups_list = Vec::new();
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        let trimmed = line.trim_end();
        if trimmed == "." { break; }
        groups_list.push(trimmed.to_string());
    }
    assert!(groups_list.contains(&"misc".to_string()));
    line.clear();

    // with a future time we should see no groups listed
    let future = Utc::now() + Duration::seconds(1);
    let date = future.format("%Y%m%d").to_string();
    let time = future.format("%H%M%S").to_string();
    write_half
        .write_all(format!("NEWGROUPS {} {}\r\n", date, time).as_bytes())
        .await
        .unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("231"));
    let mut none = true;
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        let trimmed = line.trim_end();
        if trimmed == "." { break; }
        none = false;
    }
    assert!(none);

    // clear buffer before issuing NEWNEWS
    line.clear();

    write_half.write_all(b"NEWNEWS misc 19700101 000000\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    eprintln!("NEWNEWS response: {}", line.trim_end());
    assert!(line.starts_with("230"));
    let mut ids = Vec::new();
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        let trimmed = line.trim_end();
        if trimmed == "." {
            break;
        }
        ids.push(trimmed.to_string());
    }
    assert!(ids.contains(&"<1@test>".to_string()));

    write_half.write_all(b"QUIT\r\n").await.unwrap();
    loop {
        line.clear();
        if reader.read_line(&mut line).await.unwrap() == 0 {
            break;
        }
        if line.starts_with("205") {
            break;
        }
    }
    assert!(line.starts_with("205"));
}

#[tokio::test]
async fn capabilities_and_misc_commands() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc").await.unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let store_clone = storage.clone();
    tokio::spawn(async move {
        let (sock, _) = listener.accept().await.unwrap();
        handle_client(sock, store_clone).await.unwrap();
    });

    let stream = TcpStream::connect(addr).await.unwrap();
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    reader.read_line(&mut line).await.unwrap();
    line.clear();

    write_half.write_all(b"CAPABILITIES\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("101"));
    let mut has_version = false;
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        let trimmed = line.trim_end();
        if trimmed == "." { break; }
        if trimmed.starts_with("VERSION") { has_version = true; }
    }
    assert!(has_version);
    line.clear();

    write_half.write_all(b"DATE\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("111"));
    assert_eq!(line.trim_end().len(), 18); // "111 " + 14 digits
    line.clear();

    write_half.write_all(b"HELP\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("100"));
    let mut has_cap = false;
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        let trimmed = line.trim_end();
        if trimmed == "." { break; }
        if trimmed == "CAPABILITIES" { has_cap = true; }
    }
    assert!(has_cap);

    line.clear();
    write_half.write_all(b"LIST NEWSGROUPS\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("215"));
    let mut seen = false;
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        let trimmed = line.trim_end();
        if trimmed == "." { break; }
        if trimmed.starts_with("misc") { seen = true; }
    }
    assert!(seen);

    line.clear();
    write_half.write_all(b"QUIT\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("205"));
}
