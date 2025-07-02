use crate::wildmat::wildmat;
use chrono::Duration;
use serde::Deserialize;
use std::error::Error;

fn default_db_path() -> String {
    "/var/spool/renews.db".into()
}

#[derive(Deserialize, Clone)]
pub struct Config {
    pub port: u16,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default = "default_db_path")]
    pub db_path: String,
    #[serde(default)]
    pub tls_port: Option<u16>,
    #[serde(default)]
    pub tls_cert: Option<String>,
    #[serde(default)]
    pub tls_key: Option<String>,
    #[serde(default)]
    pub default_retention_days: Option<i64>,
    #[serde(default)]
    pub retention: Vec<RetentionRule>,
}

#[derive(Deserialize, Clone)]
pub struct RetentionRule {
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub pattern: Option<String>,
    pub days: i64,
}

impl Config {
    pub fn from_file(path: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let text = std::fs::read_to_string(path)?;
        let cfg: Config = toml::from_str(&text)?;
        Ok(cfg)
    }

    pub fn retention_for_group(&self, group: &str) -> Option<Duration> {
        if let Some(rule) = self
            .retention
            .iter()
            .find(|r| r.group.as_deref() == Some(group))
        {
            return Some(Duration::days(rule.days));
        }
        if let Some(rule) = self
            .retention
            .iter()
            .filter(|r| r.group.is_none())
            .find(|r| {
                r.pattern
                    .as_deref()
                    .map(|p| wildmat(p, group))
                    .unwrap_or(false)
            })
        {
            return Some(Duration::days(rule.days));
        }
        self.default_retention_days.map(Duration::days)
    }
}
