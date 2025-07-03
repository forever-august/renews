use renews::parse_message;
use renews::storage::{Storage, sqlite::SqliteStorage};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

mod common;

#[tokio::test]
async fn unknown_command_mail() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"MAIL\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("500"));
}

#[tokio::test]
async fn capabilities_and_unknown_command() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"CAPABILITIES\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("101"));
    let mut has_list = false;
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        let trimmed = line.trim_end();
        if trimmed == "." {
            break;
        }
        if trimmed.starts_with("LIST ") {
            has_list = true;
            assert!(trimmed.contains("ACTIVE"));
            assert!(trimmed.contains("NEWSGROUPS"));
            assert!(trimmed.contains("OVERVIEW.FMT"));
            assert!(trimmed.contains("HEADERS"));
        }
    }
    assert!(has_list);
    line.clear();
    writer.write_all(b"OVER\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("412"));
}

#[tokio::test]
async fn unsupported_mode_variant() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"MODE POSTER\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("501"));
}

#[tokio::test]
async fn article_syntax_error() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer
        .write_all(b"ARTICLE a.message.id@no.angle.brackets\r\n")
        .await
        .unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("501"));
}

#[tokio::test]
async fn head_without_group_returns_412() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"HEAD 1\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("412"));
}

#[tokio::test]
async fn list_unknown_keyword() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"LIST ACTIVE u[ks].*\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("215"));
}

#[tokio::test]
async fn unknown_command_xencrypt() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer
        .write_all(b"XENCRYPT RSA abcd=efg\r\n")
        .await
        .unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("500"));
}

#[tokio::test]
async fn mode_reader_success() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"MODE READER\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("201"));
}

#[tokio::test]
async fn commands_are_case_insensitive() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"mode reader\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("201"));
    line.clear();
    writer.write_all(b"quit\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("205"));
}

#[tokio::test]
async fn group_select_returns_211() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"GROUP misc.test\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("211"));
}

#[tokio::test]
async fn article_success_by_number() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"GROUP misc.test\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"ARTICLE 1\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("220"));
    while line.trim_end() != "." {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
    }
}

#[tokio::test]
async fn article_success_by_id() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"ARTICLE <1@test>\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("220"));
    while line.trim_end() != "." {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
    }
}

#[tokio::test]
async fn article_id_not_found() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"ARTICLE <nope@id>\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("430"));
}

#[tokio::test]
async fn article_number_no_group() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"ARTICLE 1\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("412"));
}

#[tokio::test]
async fn head_success_by_number() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"GROUP misc.test\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"HEAD 1\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("221"));
    while line.trim_end() != "." {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
    }
}

#[tokio::test]
async fn head_success_by_id() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"HEAD <1@test>\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("221"));
    while line.trim_end() != "." {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
    }
}

#[tokio::test]
async fn head_number_not_found() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"GROUP misc.test\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"HEAD 2\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("423"));
}

#[tokio::test]
async fn head_id_not_found() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"HEAD <nope@id>\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("430"));
}

#[tokio::test]
async fn head_no_current_article_selected() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"GROUP misc.test\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"HEAD\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("420"));
}

#[tokio::test]
async fn body_success_by_number() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"GROUP misc.test\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"BODY 1\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("222"));
    while line.trim_end() != "." {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
    }
}

#[tokio::test]
async fn body_success_by_id() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"BODY <1@test>\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("222"));
    while line.trim_end() != "." {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
    }
}

#[tokio::test]
async fn body_number_not_found() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"GROUP misc.test\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"BODY 2\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("423"));
}

#[tokio::test]
async fn body_id_not_found() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"BODY <nope@id>\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("430"));
}

#[tokio::test]
async fn body_number_no_group() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"BODY 1\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("412"));
}

#[tokio::test]
async fn stat_success_by_number() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"GROUP misc.test\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"STAT 1\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("223"));
}

#[tokio::test]
async fn stat_success_by_id() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"STAT <1@test>\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("223"));
}

#[tokio::test]
async fn stat_number_not_found() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"GROUP misc.test\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"STAT 2\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("423"));
}

#[tokio::test]
async fn stat_id_not_found() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"STAT <nope@id>\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("430"));
}

#[tokio::test]
async fn stat_number_no_group() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"STAT 1\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("412"));
}

#[tokio::test]
async fn listgroup_returns_numbers() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"LISTGROUP misc.test\r\n").await.unwrap();
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
    assert_eq!(nums, vec!["1"]);
}

#[tokio::test]
async fn listgroup_without_group_selected() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"LISTGROUP\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("412"));
}

#[tokio::test]
async fn list_newsgroups_returns_groups() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    storage.add_group("alt.test").await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"LIST NEWSGROUPS\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("215"));
    let mut groups = Vec::new();
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        let trimmed = line.trim_end();
        if trimmed == "." {
            break;
        }
        let name = trimmed.split_whitespace().next().unwrap_or("");
        groups.push(name.to_string());
    }
    assert!(groups.contains(&"misc.test".to_string()));
    assert!(groups.contains(&"alt.test".to_string()));
}

#[tokio::test]
async fn list_all_keywords() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();

    writer.write_all(b"LIST ACTIVE\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("215"));
    let mut found = false;
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        let trimmed = line.trim_end();
        if trimmed == "." { break; }
        if trimmed.starts_with("misc.test") { found = true; }
    }
    assert!(found);
    line.clear();

    writer.write_all(b"LIST ACTIVE.TIMES\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("215"));
    let mut found = false;
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        let trimmed = line.trim_end();
        if trimmed == "." { break; }
        if trimmed.starts_with("misc.test") { found = true; }
    }
    assert!(found);
    line.clear();

    writer.write_all(b"LIST DISTRIB.PATS\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("503"));
    line.clear();

    writer.write_all(b"LIST OVERVIEW.FMT\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("215"));
    let mut has_subject = false;
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        let trimmed = line.trim_end();
        if trimmed == "." { break; }
        if trimmed == "Subject:" { has_subject = true; }
    }
    assert!(has_subject);
    line.clear();

    writer.write_all(b"LIST HEADERS\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("215"));
    let mut has_colon = false;
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        let trimmed = line.trim_end();
        if trimmed == "." { break; }
        if trimmed == ":" { has_colon = true; }
    }
    assert!(has_colon);
}

#[tokio::test]
async fn newnews_lists_recent_articles() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer
        .write_all(b"NEWNEWS misc.test 19700101 000000\r\n")
        .await
        .unwrap();
    reader.read_line(&mut line).await.unwrap();
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
    assert_eq!(ids, vec!["<1@test>".to_string()]);
}

#[tokio::test]
async fn newnews_no_matches_returns_empty() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    use chrono::{Duration, Utc};
    let future = Utc::now() + Duration::seconds(1);
    let date = future.format("%Y%m%d").to_string();
    let time = future.format("%H%M%S").to_string();
    writer
        .write_all(format!("NEWNEWS misc.test {} {}\r\n", date, time).as_bytes())
        .await
        .unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("230"));
    let mut none = true;
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        let trimmed = line.trim_end();
        if trimmed == "." {
            break;
        }
        none = false;
    }
    assert!(none);
}

#[tokio::test]
async fn hdr_subject_by_message_id() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\nSubject: Hello\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"HDR Subject <1@test>\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("225"));
    line.clear();
    reader.read_line(&mut line).await.unwrap();
    assert_eq!(line.trim_end(), "0 Hello");
    line.clear();
    reader.read_line(&mut line).await.unwrap();
    assert_eq!(line.trim_end(), ".");
}

#[tokio::test]
async fn hdr_subject_range() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, m1) = parse_message("Message-ID: <1@test>\r\nSubject: A\r\n\r\nBody").unwrap();
    let (_, m2) = parse_message("Message-ID: <2@test>\r\nSubject: B\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &m1).await.unwrap();
    storage.store_article("misc.test", &m2).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"GROUP misc.test\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"HDR Subject 1-2\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("225"));
    let mut vals = Vec::new();
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        let trimmed = line.trim_end();
        if trimmed == "." {
            break;
        }
        vals.push(trimmed.to_string());
    }
    assert_eq!(vals, vec!["1 A", "2 B"]);
}

#[tokio::test]
async fn xpat_subject_message_id() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, msg) = parse_message("Message-ID: <1@test>\r\nSubject: Hello\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"XPAT Subject <1@test> *ell*\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("221"));
    line.clear();
    reader.read_line(&mut line).await.unwrap();
    assert_eq!(line.trim_end(), "0 Hello");
    line.clear();
    reader.read_line(&mut line).await.unwrap();
    assert_eq!(line.trim_end(), ".");
}

#[tokio::test]
async fn xpat_subject_range() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, m1) = parse_message("Message-ID: <1@test>\r\nSubject: apple\r\n\r\nBody").unwrap();
    let (_, m2) = parse_message("Message-ID: <2@test>\r\nSubject: banana\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &m1).await.unwrap();
    storage.store_article("misc.test", &m2).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"GROUP misc.test\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"XPAT Subject 1-2 *a*\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("221"));
    let mut vals = Vec::new();
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        let trimmed = line.trim_end();
        if trimmed == "." {
            break;
        }
        vals.push(trimmed.to_string());
    }
    assert_eq!(vals, vec!["1 apple", "2 banana"]);
}

#[tokio::test]
async fn over_message_id() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, msg) =
        parse_message("Message-ID: <1@test>\r\nSubject: A\r\nFrom: a@test\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"OVER <1@test>\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("224"));
    line.clear();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("0|"));
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        if line.trim_end() == "." {
            break;
        }
    }
}

#[tokio::test]
async fn over_range() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, m1) =
        parse_message("Message-ID: <1@test>\r\nSubject: A\r\nFrom: a@test\r\n\r\nBody").unwrap();
    let (_, m2) =
        parse_message("Message-ID: <2@test>\r\nSubject: B\r\nFrom: b@test\r\n\r\nBody").unwrap();
    storage.store_article("misc.test", &m1).await.unwrap();
    storage.store_article("misc.test", &m2).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"GROUP misc.test\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"OVER 1-2\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("224"));
    let mut count = 0;
    loop {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        let trimmed = line.trim_end();
        if trimmed == "." {
            break;
        }
        count += 1;
    }
    assert_eq!(count, 2);
}

#[tokio::test]
async fn head_range() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, m1) = parse_message("Message-ID: <1@test>\r\n\r\nA").unwrap();
    let (_, m2) = parse_message("Message-ID: <2@test>\r\n\r\nB").unwrap();
    storage.store_article("misc.test", &m1).await.unwrap();
    storage.store_article("misc.test", &m2).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"GROUP misc.test\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"HEAD 1-2\r\n").await.unwrap();
    for _ in 0..2 {
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
    }
}

#[tokio::test]
async fn body_range() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, m1) = parse_message("Message-ID: <1@test>\r\n\r\nA").unwrap();
    let (_, m2) = parse_message("Message-ID: <2@test>\r\n\r\nB").unwrap();
    storage.store_article("misc.test", &m1).await.unwrap();
    storage.store_article("misc.test", &m2).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"GROUP misc.test\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"BODY 1-2\r\n").await.unwrap();
    for _ in 0..2 {
        reader.read_line(&mut line).await.unwrap();
        assert!(line.starts_with("222"));
        loop {
            line.clear();
            reader.read_line(&mut line).await.unwrap();
            if line.trim_end() == "." {
                break;
            }
        }
        line.clear();
    }
}

#[tokio::test]
async fn article_range() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, m1) = parse_message("Message-ID: <1@test>\r\n\r\nA").unwrap();
    let (_, m2) = parse_message("Message-ID: <2@test>\r\n\r\nB").unwrap();
    storage.store_article("misc.test", &m1).await.unwrap();
    storage.store_article("misc.test", &m2).await.unwrap();
    let (addr, _h) = common::setup_server(storage).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"GROUP misc.test\r\n").await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    line.clear();
    writer.write_all(b"ARTICLE 1-2\r\n").await.unwrap();
    for _ in 0..2 {
        reader.read_line(&mut line).await.unwrap();
        assert!(line.starts_with("220"));
        loop {
            line.clear();
            reader.read_line(&mut line).await.unwrap();
            if line.trim_end() == "." {
                break;
            }
        }
        line.clear();
    }
}

#[tokio::test]
async fn ihave_example() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();

    let (addr, _h) = common::setup_server(storage.clone()).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();

    writer
        .write_all(b"IHAVE <i.am.an.article.you.will.want@example.com>\r\n")
        .await
        .unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("335"));
    line.clear();
    let article = concat!(
        "Path: pathost!demo!somewhere!not-for-mail\r\n",
        "From: \"Demo User\" <nobody@example.com>\r\n",
        "Newsgroups: misc.test\r\n",
        "Subject: I am just a test article\r\n",
        "Date: 6 Oct 1998 04:38:40 -0500\r\n",
        "Organization: An Example Com, San Jose, CA\r\n",
        "Message-ID: <i.am.an.article.you.will.want@example.com>\r\n",
        "\r\n",
        "This is just a test article.\r\n",
        ".\r\n"
    );
    writer.write_all(article.as_bytes()).await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("235"));
    line.clear();

    writer
        .write_all(b"IHAVE <i.am.an.article.you.will.want@example.com>\r\n")
        .await
        .unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("435"));
}

#[tokio::test]
async fn takethis_example() {
    let storage = Arc::new(SqliteStorage::new("sqlite::memory:").await.unwrap());
    storage.add_group("misc.test").await.unwrap();
    let (_, exist) = parse_message(
        "Message-ID: <i.am.an.article.you.have@example.com>\r\nNewsgroups: misc.test\r\n\r\nBody",
    )
    .unwrap();
    storage.store_article("misc.test", &exist).await.unwrap();

    let (addr, _h) = common::setup_server(storage.clone()).await;
    let (mut reader, mut writer) = common::connect(addr).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    line.clear();

    let take_article = concat!(
        "TAKETHIS <i.am.an.article.new@example.com>\r\n",
        "Path: pathost!demo!somewhere!not-for-mail\r\n",
        "From: \"Demo User\" <nobody@example.com>\r\n",
        "Newsgroups: misc.test\r\n",
        "Subject: I am just a test article\r\n",
        "Date: 6 Oct 1998 04:38:40 -0500\r\n",
        "Organization: An Example Com, San Jose, CA\r\n",
        "Message-ID: <i.am.an.article.new@example.com>\r\n",
        "\r\n",
        "This is just a test article.\r\n",
        ".\r\n"
    );
    writer.write_all(take_article.as_bytes()).await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("239"));
    line.clear();

    let take_reject = concat!(
        "TAKETHIS <i.am.an.article.you.have@example.com>\r\n",
        "Path: pathost!demo!somewhere!not-for-mail\r\n",
        "From: \"Demo User\" <nobody@example.com>\r\n",
        "Newsgroups: misc.test\r\n",
        "Subject: I am just a test article\r\n",
        "Date: 6 Oct 1998 04:38:40 -0500\r\n",
        "Organization: An Example Com, San Jose, CA\r\n",
        "Message-ID: <i.am.an.article.you.have@example.com>\r\n",
        "\r\n",
        "This is just a test article.\r\n",
        ".\r\n"
    );
    writer.write_all(take_reject.as_bytes()).await.unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.starts_with("439"));
}
