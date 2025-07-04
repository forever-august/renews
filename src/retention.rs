use crate::config::Config;
use crate::storage::Storage;
use crate::Message;
use chrono::{DateTime, Utc};
use std::error::Error;

pub async fn cleanup_expired_articles(
    storage: &dyn Storage,
    cfg: &Config,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let groups = storage.list_groups().await?;
    let now = Utc::now();
    for g in groups {
        let retention = cfg.retention_for_group(&g);
        if let Some(ret) = retention {
            if ret.num_seconds() > 0 {
                let cutoff = now - ret;
                storage.purge_group_before(&g, cutoff).await?;
            }
        }

        // Expire articles with an Expires header in the past
        let ids = storage.list_article_ids(&g).await?;
        for id in ids {
            if let Some(article) = storage.get_article_by_id(&id).await? {
                if let Some(exp) = expires_time(&article) {
                    if exp <= now {
                        storage.delete_article_by_id(&id).await?;
                    }
                }
            }
        }
    }
    storage.purge_orphan_messages().await?;
    Ok(())
}

fn expires_time(msg: &Message) -> Option<DateTime<Utc>> {
    msg.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("Expires"))
        .and_then(|(_, v)| chrono::DateTime::parse_from_rfc2822(v).or_else(|_| chrono::DateTime::parse_from_rfc3339(v)).ok())
        .map(|dt| dt.with_timezone(&Utc))
}
