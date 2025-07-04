use crate::wildmat::wildmat;
use chrono::Duration;
use serde::Deserialize;
use serde::de::{self, Deserializer, Visitor};
use std::error::Error;
use std::fmt;

fn default_db_path() -> String {
    "/var/spool/renews.db".into()
}

fn default_site_name() -> String {
    std::env::var("HOSTNAME").unwrap_or_else(|_| "localhost".into())
}

fn parse_size(input: &str) -> Option<u64> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (digits, factor) = match trimmed.chars().last()? {
        'K' | 'k' => (&trimmed[..trimmed.len() - 1], 1024u64),
        'M' | 'm' => (&trimmed[..trimmed.len() - 1], 1024u64 * 1024),
        'G' | 'g' => (&trimmed[..trimmed.len() - 1], 1024u64 * 1024 * 1024),
        '0'..='9' => (trimmed, 1u64),
        _ => return None,
    };
    digits
        .trim()
        .parse::<u64>()
        .ok()
        .and_then(|n| n.checked_mul(factor))
}

fn deserialize_size<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    struct SizeVisitor;

    impl<'de> Visitor<'de> for SizeVisitor {
        type Value = Option<u64>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("an integer or string with optional K, M, G suffix")
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E> {
            Ok(Some(v))
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if v < 0 {
                Err(de::Error::custom("size must be positive"))
            } else {
                Ok(Some(v as u64))
            }
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            parse_size(v)
                .map(Some)
                .ok_or_else(|| de::Error::custom(format!("invalid size: {}", v)))
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
    }

    deserializer.deserialize_any(SizeVisitor)
}

#[derive(Deserialize, Clone)]
pub struct Config {
    pub port: u16,
    #[serde(default = "default_site_name")]
    pub site_name: String,
    #[serde(default = "default_db_path")]
    pub db_path: String,
    #[serde(default)]
    pub auth_db_path: Option<String>,
    #[serde(default)]
    pub tls_port: Option<u16>,
    #[serde(default)]
    pub tls_cert: Option<String>,
    #[serde(default)]
    pub tls_key: Option<String>,
    #[serde(default)]
    pub default_retention_days: Option<i64>,
    #[serde(default, deserialize_with = "deserialize_size")]
    pub default_max_article_bytes: Option<u64>,
    #[serde(default)]
    pub group_settings: Vec<GroupRule>,
}

#[derive(Deserialize, Clone)]
pub struct GroupRule {
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub retention_days: Option<i64>,
    #[serde(default, deserialize_with = "deserialize_size")]
    pub max_article_bytes: Option<u64>,
}

impl Config {
    pub fn from_file(path: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let text = std::fs::read_to_string(path)?;
        let cfg: Config = toml::from_str(&text)?;
        Ok(cfg)
    }

    fn rule_for_group(&self, group: &str) -> Option<&GroupRule> {
        if let Some(rule) = self
            .group_settings
            .iter()
            .find(|r| r.group.as_deref() == Some(group))
        {
            return Some(rule);
        }
        self.group_settings
            .iter()
            .filter(|r| r.group.is_none())
            .find(|r| {
                r.pattern
                    .as_deref()
                    .map(|p| wildmat(p, group))
                    .unwrap_or(false)
            })
    }

    pub fn retention_for_group(&self, group: &str) -> Option<Duration> {
        if let Some(rule) = self.rule_for_group(group) {
            if let Some(days) = rule.retention_days {
                return Some(Duration::days(days));
            }
        }
        self.default_retention_days.map(Duration::days)
    }

    pub fn max_size_for_group(&self, group: &str) -> Option<u64> {
        self.rule_for_group(group)
            .and_then(|r| r.max_article_bytes)
            .or(self.default_max_article_bytes)
    }
}
