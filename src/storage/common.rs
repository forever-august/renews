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