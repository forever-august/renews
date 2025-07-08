use super::Message;
use serde::{Deserialize, Serialize};

/// Serializable wrapper for message headers.
#[derive(Serialize, Deserialize)]
pub struct Headers(pub Vec<(String, String)>);

/// Extract the Message-ID header from an article.
/// 
/// Returns the Message-ID value if found, None otherwise.
pub fn extract_message_id(article: &Message) -> Option<String> {
    article.headers.iter().find_map(|(k, v)| {
        if k.eq_ignore_ascii_case("Message-ID") {
            Some(v.clone())
        } else {
            None
        }
    })
}

/// SQL schemas and queries shared between storage implementations.
pub mod sql {
    /// Schema for the messages table (stores unique messages keyed by Message-ID).
    pub const MESSAGES_TABLE_SQLITE: &str = 
        "CREATE TABLE IF NOT EXISTS messages (
            message_id TEXT PRIMARY KEY,
            headers TEXT,
            body TEXT,
            size INTEGER NOT NULL
        )";

    pub const MESSAGES_TABLE_POSTGRES: &str = 
        "CREATE TABLE IF NOT EXISTS messages (
            message_id TEXT PRIMARY KEY,
            headers TEXT,
            body TEXT,
            size BIGINT NOT NULL
        )";

    /// Schema for the group_articles table (maps groups and numbers to message IDs).
    pub const GROUP_ARTICLES_TABLE_SQLITE: &str = 
        "CREATE TABLE IF NOT EXISTS group_articles (
            group_name TEXT,
            number INTEGER,
            message_id TEXT,
            inserted_at INTEGER NOT NULL,
            PRIMARY KEY(group_name, number),
            FOREIGN KEY(message_id) REFERENCES messages(message_id)
        )";

    pub const GROUP_ARTICLES_TABLE_POSTGRES: &str = 
        "CREATE TABLE IF NOT EXISTS group_articles (
            group_name TEXT,
            number BIGINT,
            message_id TEXT,
            inserted_at BIGINT NOT NULL,
            PRIMARY KEY(group_name, number),
            FOREIGN KEY(message_id) REFERENCES messages(message_id)
        )";

    /// Schema for the groups table (available newsgroups with creation time).
    pub const GROUPS_TABLE_SQLITE: &str = 
        "CREATE TABLE IF NOT EXISTS groups (
            name TEXT PRIMARY KEY,
            created_at INTEGER NOT NULL,
            moderated INTEGER NOT NULL DEFAULT 0
        )";

    pub const GROUPS_TABLE_POSTGRES: &str = 
        "CREATE TABLE IF NOT EXISTS groups (
            name TEXT PRIMARY KEY,
            created_at BIGINT NOT NULL,
            moderated BOOLEAN NOT NULL DEFAULT FALSE
        )";
}