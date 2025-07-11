//! Tests for NNTP command handler argument validation failures

use crate::utils::{ClientMock, setup};

#[tokio::test]
async fn test_article_commands_missing_args() {
    let (storage, auth) = setup().await;
    storage.add_group("test.group", false).await.unwrap();

    // Test commands that require article selection but no group is selected
    ClientMock::new()
        .expect("ARTICLE", "412 no newsgroup selected")
        .expect("HEAD", "412 no newsgroup selected")
        .expect("BODY", "412 no newsgroup selected")
        .expect("STAT", "412 no newsgroup selected")
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn test_article_commands_invalid_message_id() {
    let (storage, auth) = setup().await;
    storage.add_group("test.group", false).await.unwrap();

    ClientMock::new()
        .expect("GROUP test.group", "211 0 0 0 test.group")
        .expect("ARTICLE <nonexistent@test>", "430 no such article")
        .expect("HEAD <nonexistent@test>", "430 no such article")
        .expect("BODY <nonexistent@test>", "430 no such article")
        .expect("STAT <nonexistent@test>", "430 no such article")
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn test_article_commands_invalid_article_number() {
    let (storage, auth) = setup().await;
    storage.add_group("test.group", false).await.unwrap();

    ClientMock::new()
        .expect("GROUP test.group", "211 0 0 0 test.group")
        .expect("ARTICLE 999", "423 no such article number in this group")
        .expect("HEAD 999", "423 no such article number in this group")
        .expect("BODY 999", "423 no such article number in this group")
        .expect("STAT 999", "423 no such article number in this group")
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn test_hdr_command_insufficient_args() {
    let (storage, auth) = setup().await;

    ClientMock::new()
        .expect("HDR", "501 not enough arguments")
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn test_xpat_command_insufficient_args() {
    let (storage, auth) = setup().await;

    ClientMock::new()
        // XPAT requires at least 3 arguments: field, range, pattern(s)
        .expect("XPAT", "501 not enough arguments")
        .expect("XPAT Subject", "501 not enough arguments")
        .expect("XPAT Subject 1-100", "501 not enough arguments")
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn test_group_command_missing_args() {
    let (storage, auth) = setup().await;

    ClientMock::new()
        .expect("GROUP", "501 not enough arguments")
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn test_group_command_nonexistent_group() {
    let (storage, auth) = setup().await;

    ClientMock::new()
        // GROUP command in this implementation appears to create the group if it doesn't exist
        .expect("GROUP nonexistent.group", "211 0 0 0 nonexistent.group")
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn test_listgroup_command_no_current_group() {
    let (storage, auth) = setup().await;

    ClientMock::new()
        // LISTGROUP without arguments when no group is selected
        .expect("LISTGROUP", "412 no newsgroup selected")
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn test_listgroup_command_nonexistent_group() {
    let (storage, auth) = setup().await;

    ClientMock::new()
        // LISTGROUP creates empty listing for nonexistent group
        .expect("LISTGROUP nonexistent.group", "211 article numbers follow")
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn test_list_command_unknown_keyword() {
    let (storage, auth) = setup().await;

    ClientMock::new()
        .expect("LIST UNKNOWN", "501 unknown keyword")
        .expect("LIST INVALID.KEYWORD", "501 unknown keyword")
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn test_newgroups_command_invalid_date() {
    let (storage, auth) = setup().await;

    ClientMock::new()
        // Invalid date format
        .expect("NEWGROUPS invalid-date 000000", "501 invalid date")
        .expect("NEWGROUPS 20241301 000000", "501 invalid date") // Invalid month
        .expect("NEWGROUPS 20241201 250000", "501 invalid date") // Invalid hour
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn test_newgroups_command_missing_args() {
    let (storage, auth) = setup().await;

    ClientMock::new()
        .expect("NEWGROUPS", "501 not enough arguments")
        .expect("NEWGROUPS 20241201", "501 not enough arguments")
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn test_newnews_command_invalid_args() {
    let (storage, auth) = setup().await;

    ClientMock::new()
        .expect("NEWNEWS", "501 not enough arguments")
        .expect("NEWNEWS wildmat", "501 not enough arguments")
        .expect("NEWNEWS wildmat 20241201", "501 not enough arguments")
        .expect("NEWNEWS wildmat invalid-date 000000", "501 invalid date")
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn test_authinfo_command_invalid_args() {
    let (storage, auth) = setup().await;

    ClientMock::new()
        .expect("AUTHINFO", "501 not enough arguments")
        .expect("AUTHINFO INVALID", "501 not enough arguments")
        .expect("AUTHINFO USER", "501 not enough arguments")
        .expect("AUTHINFO PASS", "501 not enough arguments")
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn test_authinfo_command_wrong_sequence() {
    let (storage, auth) = setup().await;

    ClientMock::new()
        // Try AUTHINFO PASS without USER first - actual response is 481
        .expect("AUTHINFO PASS password", "481 Authentication rejected")
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn test_mode_command_invalid_args() {
    let (storage, auth) = setup().await;

    ClientMock::new()
        .expect("MODE", "501 missing mode")
        .expect("MODE INVALID", "501 command not understood")
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn test_over_command_invalid_range() {
    let (storage, auth) = setup().await;
    storage.add_group("test.group", false).await.unwrap();

    ClientMock::new()
        .expect("GROUP test.group", "211 0 0 0 test.group")
        .expect("OVER invalid-range", "423 no articles in that range")
        .expect("OVER 999-1000", "423 no articles in that range")
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn test_streaming_commands_without_stream_mode() {
    let (storage, auth) = setup().await;

    ClientMock::new()
        // CHECK command works but returns 238 (article wanted)
        .expect("CHECK <test@example.com>", "238 <test@example.com>")
        // Test TAKETHIS which should also work
        .expect("TAKETHIS <test2@example.com>", "239 <test2@example.com>")
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn test_ihave_command_invalid_message_id() {
    let (storage, auth) = setup().await;

    ClientMock::new()
        .expect("IHAVE", "501 message-id required")
        .expect("IHAVE invalid-message-id", "435 article not wanted")
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}

#[tokio::test]
async fn test_malformed_commands() {
    let (storage, auth) = setup().await;

    ClientMock::new()
        // Commands with invalid syntax that should trigger 500 responses
        .expect("", "500 Syntax error")
        .expect("INVALID@COMMAND", "500 unknown command")
        .expect("123COMMAND", "500 Syntax error")
        .expect("QUIT", "205 closing connection")
        .run(storage, auth)
        .await;
}