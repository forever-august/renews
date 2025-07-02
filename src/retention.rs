use crate::config::Config;
use crate::storage::Storage;
use chrono::Utc;
use std::error::Error;

pub async fn cleanup_expired_articles(
    storage: &dyn Storage,
    cfg: &Config,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let groups = storage.list_groups().await?;
    for g in groups {
        if let Some(ret) = cfg.retention_for_group(&g) {
            let cutoff = Utc::now() - ret;
            storage.purge_group_before(&g, cutoff).await?;
        }
    }
    storage.purge_orphan_messages().await?;
    Ok(())
}
