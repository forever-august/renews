use crate::Message;
use crate::config::Config;
use crate::storage::Storage;
use anyhow::Result;
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use tracing::{debug, info, warn};

/// Clean up expired articles based on retention policies.
///
/// This function performs two types of cleanup:
/// 1. Time-based retention: Removes articles older than the configured retention period for each group
/// 2. Expires header cleanup: Removes articles with an `Expires` header that has passed
///
/// # Errors
///
/// Returns an error if there are issues accessing the storage or configuration.
pub async fn cleanup_expired_articles(storage: &dyn Storage, cfg: &Config) -> Result<()> {
    info!("Starting retention cleanup");
    let now = Utc::now();
    let mut groups = storage.list_groups();
    while let Some(result) = groups.next().await {
        let group = result?;
        // Apply time-based retention policies for  group
        if let Err(e) = cleanup_group_by_retention(storage, cfg, group.as_str(), now).await {
            warn!(
                "Failed to apply retention policy for group '{}': {}",
                group, e
            );
        }
        // Remove articles with expired Expires headers
        if let Err(e) = cleanup_group_by_expires_header(storage, group.as_str(), now).await {
            warn!(
                "Failed to clean up expired articles in group '{}': {}",
                group, e
            );
        }
        info!("Finished cleanup for group {}", group);
    }

    // Clean up orphaned messages that are no longer referenced by any group
    info!("Cleaning up orphaned messages");
    storage.purge_orphan_messages().await?;

    info!("Finished cleaning up expired articles");
    Ok(())
}

/// Apply time-based retention policy for a single group.
async fn cleanup_group_by_retention(
    storage: &dyn Storage,
    cfg: &Config,
    group: &str,
    now: DateTime<Utc>,
) -> Result<()> {
    let retention = cfg.retention_for_group(group);

    if let Some(retention_duration) = retention {
        if retention_duration.num_seconds() > 0 {
            let cutoff = now - retention_duration;
            debug!(
                "Applying retention policy for group '{}': removing articles older than {}",
                group, cutoff
            );
            storage
                .purge_group_before(group, cutoff)
                .await
                .map_err(|e| {
                    anyhow::anyhow!("Failed to purge old articles from group '{group}': {e}")
                })?;
        } else {
            debug!(
                "Group '{}' has zero retention period, skipping cleanup",
                group
            );
        }
    } else {
        debug!("No retention policy configured for group '{}'", group);
    }

    Ok(())
}

/// Remove articles with expired Expires headers from a single group.
async fn cleanup_group_by_expires_header(
    storage: &dyn Storage,
    group: &str,
    now: DateTime<Utc>,
) -> Result<()> {
    let mut stream = storage.list_article_ids(group);

    let mut expired_count = 0;
    while let Some(result) = stream.next().await {
        let id = result?;
        match storage.get_article_by_id(&id).await {
            Ok(Some(article)) => {
                if let Some(expires_time) = parse_expires_header(&article) {
                    if expires_time <= now {
                        if let Err(e) = storage.delete_article_by_id(&id).await {
                            warn!("Failed to delete expired article '{}': {}", id, e);
                        } else {
                            expired_count += 1;
                        }
                    }
                }
            }
            Ok(None) => {
                // Article doesn't exist anymore, skip
                continue;
            }
            Err(e) => {
                warn!(
                    "Failed to retrieve article '{}' for expiration check: {}",
                    id, e
                );
            }
        }
    }

    if expired_count > 0 {
        debug!(
            "Removed {} articles with expired Expires headers from group '{}'",
            expired_count, group
        );
    }

    Ok(())
}

/// Parse the Expires header from a message and return the expiration time.
///
/// This function looks for an `Expires` header in the message and attempts to parse it
/// using both RFC 2822 and RFC 3339 formats.
///
/// # Arguments
/// * `msg` - The message to parse the Expires header from
///
/// # Returns
/// * `Some(DateTime<Utc>)` if a valid Expires header is found and parsed successfully
/// * `None` if no Expires header is found or it cannot be parsed
fn parse_expires_header(msg: &Message) -> Option<DateTime<Utc>> {
    msg.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("Expires"))
        .and_then(|(_, v)| {
            chrono::DateTime::parse_from_rfc2822(v)
                .or_else(|_| chrono::DateTime::parse_from_rfc3339(v))
                .ok()
        })
        .map(|dt| dt.with_timezone(&Utc))
}
