use crate::wildmat::wildmat;
use chrono::Duration;
use serde::Deserialize;
use serde::de::{self, Deserializer, Visitor};
use std::error::Error;
use std::fmt;
use regex::Regex;

fn default_db_path() -> String {
    "sqlite:///var/renews/news.db".into()
}

fn default_auth_db_path() -> String {
    "sqlite:///var/renews/auth.db".into()
}

fn default_peer_db_path() -> String {
    "sqlite:///var/renews/peers.db".into()
}

fn default_peer_sync_secs() -> u64 {
    3600
}

fn default_peer_sync_schedule() -> String {
    "0 0 * * * *".to_string() // Every hour
}

fn default_idle_timeout_secs() -> u64 {
    600
}

fn default_site_name() -> String {
    std::env::var("HOSTNAME").unwrap_or_else(|_| "localhost".into())
}

fn expand_placeholders(text: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
    let env_re = Regex::new(r"\$ENV\{([^}]+)\}")?;
    let file_re = Regex::new(r"\$FILE\{([^}]+)\}")?;
    let mut out = String::new();
    let mut last = 0;
    for caps in env_re.captures_iter(text) {
        let m = caps.get(0).unwrap();
        out.push_str(&text[last..m.start()]);
        let var = std::env::var(&caps[1])?;
        out.push_str(&var);
        last = m.end();
    }
    out.push_str(&text[last..]);
    let text = out;
    let mut out = String::new();
    let mut last = 0;
    for caps in file_re.captures_iter(&text) {
        let m = caps.get(0).unwrap();
        out.push_str(&text[last..m.start()]);
        let contents = std::fs::read_to_string(&caps[1])?;
        out.push_str(&contents);
        last = m.end();
    }
    out.push_str(&text[last..]);
    Ok(out)
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

    impl Visitor<'_> for SizeVisitor {
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
                Ok(Some(u64::try_from(v).map_err(de::Error::custom)?))
            }
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            parse_size(v)
                .map(Some)
                .ok_or_else(|| de::Error::custom(format!("invalid size: {v}")))
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
    pub addr: String,
    #[serde(default = "default_site_name")]
    pub site_name: String,
    #[serde(default = "default_db_path")]
    pub db_path: String,
    #[serde(default = "default_auth_db_path")]
    pub auth_db_path: String,
    #[serde(default = "default_peer_db_path")]
    pub peer_db_path: String,
    #[serde(default = "default_peer_sync_secs")]
    pub peer_sync_secs: u64,
    #[serde(default = "default_peer_sync_schedule")]
    pub peer_sync_schedule: String,
    #[serde(default = "default_idle_timeout_secs")]
    pub idle_timeout_secs: u64,
    #[serde(default)]
    pub peers: Vec<PeerRule>,
    #[serde(default)]
    pub tls_addr: Option<String>,
    #[serde(default)]
    pub tls_cert: Option<String>,
    #[serde(default)]
    pub tls_key: Option<String>,
    #[serde(default)]
    pub ws_addr: Option<String>,
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

#[derive(Deserialize, Clone)]
pub struct PeerRule {
    pub sitename: String,
    #[serde(default)]
    pub patterns: Vec<String>,
    #[serde(default)]
    pub sync_schedule: Option<String>,
}

impl Config {
    /// Load configuration from a TOML file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn from_file(path: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let text = std::fs::read_to_string(path)?;
        let text = expand_placeholders(&text)?;
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
            .find(|r| r.pattern.as_deref().is_some_and(|p| wildmat(p, group)))
    }

    #[must_use]
    pub fn retention_for_group(&self, group: &str) -> Option<Duration> {
        if let Some(rule) = self.rule_for_group(group) {
            if let Some(days) = rule.retention_days {
                if days > 0 {
                    return Some(Duration::days(days));
                }
                return None;
            }
        }
        self.default_retention_days
            .and_then(|d| if d > 0 { Some(Duration::days(d)) } else { None })
    }

    #[must_use]
    pub fn max_size_for_group(&self, group: &str) -> Option<u64> {
        self.rule_for_group(group)
            .and_then(|r| r.max_article_bytes)
            .or(self.default_max_article_bytes)
    }

    /// Update runtime-adjustable values from a new configuration.
    /// Only retention, group, and TLS settings are changed.
    pub fn update_runtime(&mut self, other: Config) {
        self.default_retention_days = other.default_retention_days;
        self.default_max_article_bytes = other.default_max_article_bytes;
        self.group_settings = other.group_settings;
        self.peer_sync_secs = other.peer_sync_secs;
        self.peer_sync_schedule = other.peer_sync_schedule;
        self.idle_timeout_secs = other.idle_timeout_secs;
        self.peers = other.peers;
        self.tls_cert = other.tls_cert;
        self.tls_key = other.tls_key;
        self.ws_addr = other.ws_addr;
    }
}
