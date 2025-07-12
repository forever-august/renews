use crate::wildmat::wildmat;
use chrono::Duration;
use regex::Regex;
use serde::Deserialize;
use serde::de::{self, Deserializer, Visitor};
use std::error::Error;
use std::fmt;

fn default_db_path() -> String {
    "sqlite:///var/lib/renews/news.db".into()
}

fn default_auth_db_path() -> String {
    "sqlite:///var/lib/renews/auth.db".into()
}

fn default_peer_db_path() -> String {
    "sqlite:///var/lib/renews/peers.db".into()
}

fn default_peer_sync_schedule() -> String {
    "0 0 * * * *".to_string() // Every hour
}

fn default_idle_timeout_secs() -> u64 {
    600
}

fn default_article_queue_capacity() -> usize {
    1000
}

fn default_article_worker_count() -> usize {
    4
}

fn default_site_name() -> String {
    std::env::var("HOSTNAME").unwrap_or_else(|_| "localhost".into())
}

pub fn default_pgp_key_servers() -> Vec<String> {
    vec![
        "hkps://keys.openpgp.org/pks/lookup?op=get&search=<email>".to_string(),
        "hkps://pgp.mit.edu/pks/lookup?op=get&search=<email>".to_string(),
        "hkps://keyserver.ubuntu.com/pks/lookup?op=get&search=<email>".to_string(),
    ]
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

    #[serde(default = "default_peer_sync_schedule")]
    pub peer_sync_schedule: String,
    #[serde(default = "default_idle_timeout_secs")]
    pub idle_timeout_secs: u64,
    #[serde(default, alias = "peer")]
    pub peers: Vec<PeerRule>,
    #[serde(default)]
    pub tls_addr: Option<String>,
    #[serde(default)]
    pub tls_cert: Option<String>,
    #[serde(default)]
    pub tls_key: Option<String>,
    #[serde(default)]
    pub ws_addr: Option<String>,
    #[serde(default = "default_article_queue_capacity")]
    pub article_queue_capacity: usize,
    #[serde(default = "default_article_worker_count")]
    pub article_worker_count: usize,
    #[serde(default, alias = "group")]
    pub group_settings: Vec<GroupRule>,
    #[serde(default, alias = "filter")]
    pub filters: Vec<FilterConfig>,

    #[serde(default = "default_pgp_key_servers")]
    pub pgp_key_servers: Vec<String>,
    
    #[serde(default)]
    pub allow_posting_insecure_connections: bool,
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

#[derive(Deserialize, Clone)]
pub struct FilterConfig {
    pub name: String,
    #[serde(flatten)]
    pub parameters: serde_json::Map<String, serde_json::Value>,
}



impl Config {
    /// Load configuration from a TOML file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn from_file(path: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let text = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) => {
                return match e.kind() {
                    std::io::ErrorKind::NotFound => {
                        Err(format!(
                            "Configuration file not found: '{}'

Please ensure the configuration file exists at the specified path.
You can:
- Create a configuration file at '{}'
- Use --config <path> to specify a different location
- Set the RENEWS_CONFIG environment variable
- See the example configuration at 'examples/config.toml'",
                            path, path
                        ).into())
                    }
                    std::io::ErrorKind::PermissionDenied => {
                        Err(format!(
                            "Permission denied reading configuration file: '{}'

Please ensure the file is readable by the current user.
You may need to check file permissions or run with appropriate privileges.",
                            path
                        ).into())
                    }
                    _ => {
                        Err(format!(
                            "Failed to read configuration file '{}': {}

Please ensure the file exists and is readable.",
                            path, e
                        ).into())
                    }
                }
            }
        };
        
        let text = expand_placeholders(&text).map_err(|e| {
            format!(
                "Failed to process configuration placeholders in '{}': {}

Please check that all $ENV{{...}} and $FILE{{...}} placeholders are valid.",
                path, e
            )
        })?;
        
        let mut cfg: Config = toml::from_str(&text).map_err(|e| {
            format!(
                "Failed to parse configuration file '{}': {}

Please check the TOML syntax. Common issues:
- Missing quotes around string values
- Incorrect section headers
- Malformed array or table syntax

See 'examples/config.toml' for a valid configuration example.",
                path, e
            )
        })?;

        // Enforce minimum values for queue configuration
        cfg.article_queue_capacity = cfg.article_queue_capacity.max(1);
        cfg.article_worker_count = cfg.article_worker_count.max(1);

        Ok(cfg)
    }

    #[must_use]
    pub fn retention_for_group(&self, group: &str) -> Option<Duration> {
        // First check for exact group matches
        if let Some(rule) = self
            .group_settings
            .iter()
            .find(|r| r.group.as_deref() == Some(group))
        {
            if let Some(days) = rule.retention_days {
                if days > 0 {
                    return Some(Duration::days(days));
                }
                return None;
            }
        }
        
        // Then check for pattern matches, looking for the most specific pattern that has retention_days
        let mut matches: Vec<_> = self
            .group_settings
            .iter()
            .filter(|r| r.group.is_none())
            .filter(|r| r.pattern.as_deref().is_some_and(|p| wildmat(p, group)))
            .filter(|r| r.retention_days.is_some())
            .collect();
            
        if matches.is_empty() {
            return None;
        }
        
        // Sort by pattern specificity (fewer wildcards = more specific)
        matches.sort_by_key(|r| {
            let pattern = r.pattern.as_ref().unwrap();
            // Count wildcards - fewer wildcards means more specific
            let wildcard_count = pattern.chars().filter(|c| *c == '*' || *c == '?').count();
            // Also consider pattern length - longer patterns with same wildcard count are more specific
            (wildcard_count, -(pattern.len() as i32))
        });
        
        if let Some(rule) = matches.first() {
            if let Some(days) = rule.retention_days {
                if days > 0 {
                    return Some(Duration::days(days));
                }
            }
        }
        
        None
    }

    #[must_use]
    pub fn max_size_for_group(&self, group: &str) -> Option<u64> {
        // First check for exact group matches
        if let Some(rule) = self
            .group_settings
            .iter()
            .find(|r| r.group.as_deref() == Some(group))
        {
            if rule.max_article_bytes.is_some() {
                return rule.max_article_bytes;
            }
        }
        
        // Then check for pattern matches, looking for the most specific pattern that has max_article_bytes
        let mut matches: Vec<_> = self
            .group_settings
            .iter()
            .filter(|r| r.group.is_none())
            .filter(|r| r.pattern.as_deref().is_some_and(|p| wildmat(p, group)))
            .filter(|r| r.max_article_bytes.is_some())
            .collect();
            
        if matches.is_empty() {
            return None;
        }
        
        // Sort by pattern specificity (fewer wildcards = more specific)
        matches.sort_by_key(|r| {
            let pattern = r.pattern.as_ref().unwrap();
            // Count wildcards - fewer wildcards means more specific
            let wildcard_count = pattern.chars().filter(|c| *c == '*' || *c == '?').count();
            // Also consider pattern length - longer patterns with same wildcard count are more specific
            (wildcard_count, -(pattern.len() as i32))
        });
        
        matches.first().and_then(|r| r.max_article_bytes)
    }

    /// Update runtime-adjustable values from a new configuration.
    /// Only retention, group, filter pipeline, and TLS settings are changed.
    pub fn update_runtime(&mut self, other: Config) {
        self.group_settings = other.group_settings;
        self.filters = other.filters;

        self.peer_sync_schedule = other.peer_sync_schedule;
        self.idle_timeout_secs = other.idle_timeout_secs;
        self.peers = other.peers;
        self.tls_cert = other.tls_cert;
        self.tls_key = other.tls_key;
        self.ws_addr = other.ws_addr;
        self.pgp_key_servers = other.pgp_key_servers;
        self.allow_posting_insecure_connections = other.allow_posting_insecure_connections;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_pgp_key_servers() {
        let servers = default_pgp_key_servers();
        assert_eq!(servers.len(), 3);
        assert!(servers.iter().any(|s| s.contains("keys.openpgp.org")));
        assert!(servers.iter().any(|s| s.contains("pgp.mit.edu")));
        assert!(servers.iter().any(|s| s.contains("keyserver.ubuntu.com")));
    }

    #[test]
    fn test_config_with_default_pgp_servers() {
        let config_str = r#"
            addr = ":119"
            site_name = "test.com"
        "#;
        let config: Config = toml::from_str(config_str).unwrap();
        assert_eq!(config.pgp_key_servers.len(), 3);
        assert!(
            config
                .pgp_key_servers
                .iter()
                .any(|s| s.contains("keys.openpgp.org"))
        );
    }

    #[test]
    fn test_config_with_custom_pgp_servers() {
        let config_str = r#"
            addr = ":119"
            site_name = "test.com"
            pgp_key_servers = [
                "hkps://custom1.example.com/pks/lookup?op=get&search=<email>",
                "hkps://custom2.example.com/pks/lookup?op=get&search=<email>"
            ]
        "#;
        let config: Config = toml::from_str(config_str).unwrap();
        assert_eq!(config.pgp_key_servers.len(), 2);
        assert!(
            config
                .pgp_key_servers
                .iter()
                .any(|s| s.contains("custom1.example.com"))
        );
        assert!(
            config
                .pgp_key_servers
                .iter()
                .any(|s| s.contains("custom2.example.com"))
        );
    }

    #[test]
    fn test_config_update_runtime_includes_pgp_servers() {
        let mut config1: Config = toml::from_str(
            r#"
            addr = ":119"
            site_name = "test.com"
        "#,
        )
        .unwrap();

        let config2: Config = toml::from_str(
            r#"
            addr = ":119"
            site_name = "test.com"
            pgp_key_servers = ["hkps://updated.example.com/pks/lookup?op=get&search=<email>"]
        "#,
        )
        .unwrap();

        config1.update_runtime(config2);
        assert_eq!(config1.pgp_key_servers.len(), 1);
        assert!(config1.pgp_key_servers[0].contains("updated.example.com"));
    }

    #[test]
    fn test_config_empty_pgp_servers() {
        let config_str = r#"
            addr = ":119"
            site_name = "test.com"
            pgp_key_servers = []
        "#;
        let config: Config = toml::from_str(config_str).unwrap();
        assert_eq!(config.pgp_key_servers.len(), 0);
    }
}
