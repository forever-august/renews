use crate::Message;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// Serializable wrapper for message headers.
#[derive(Serialize, Deserialize)]
pub struct Headers(pub SmallVec<[(String, String); 8]>);

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

/// Parse newsgroups from a message, returning a SmallVec for efficiency
pub fn parse_newsgroups_from_message(article: &Message) -> SmallVec<[String; 4]> {
    article
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("Newsgroups"))
        .map(|(_, v)| {
            v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(std::string::ToString::to_string)
                .collect::<SmallVec<[String; 4]>>()
        })
        .unwrap_or_default()
}

/// Common logic for reconstructing a Message from database row data
pub fn reconstruct_message_from_row(headers_str: &str, body: &str) -> anyhow::Result<Message> {
    let Headers(headers) = serde_json::from_str(headers_str)?;
    Ok(Message {
        headers,
        body: body.to_string(),
    })
}
