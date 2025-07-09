use crate::Message;
use crate::config::Config;
use crate::storage::Storage;
use chrono::{DateTime, Utc};
use std::error::Error;
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
pub async fn cleanup_expired_articles(
    storage: &dyn Storage,
    cfg: &Config,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    info!("Starting retention cleanup");

    let groups = storage.list_groups().await?;
    let now = Utc::now();

    // Apply time-based retention policies for each group
    for group in &groups {
        if let Err(e) = cleanup_group_by_retention(storage, cfg, group, now).await {
            warn!(
                "Failed to apply retention policy for group '{}': {}",
                group, e
            );
        }
    }

    // Remove articles with expired Expires headers
    for group in &groups {
        if let Err(e) = cleanup_group_by_expires_header(storage, group, now).await {
            warn!(
                "Failed to clean up expired articles in group '{}': {}",
                group, e
            );
        }
    }

    // Clean up orphaned messages that are no longer referenced by any group
    debug!("Cleaning up orphaned messages");
    storage.purge_orphan_messages().await?;

    info!("Retention cleanup completed for {} groups", groups.len());
    Ok(())
}

/// Apply time-based retention policy for a single group.
async fn cleanup_group_by_retention(
    storage: &dyn Storage,
    cfg: &Config,
    group: &str,
    now: DateTime<Utc>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
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
                .map_err(|e| format!("Failed to purge old articles from group '{group}': {e}"))?;
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
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let article_ids = storage
        .list_article_ids(group)
        .await
        .map_err(|e| format!("Failed to list article IDs for group '{group}': {e}"))?;

    let mut expired_count = 0;

    for id in article_ids {
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
