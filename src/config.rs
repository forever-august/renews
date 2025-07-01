use serde::Deserialize;
use std::error::Error;

fn default_db_path() -> String {
    "/var/spool/renews.db".into()
}

#[derive(Deserialize)]
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
}

impl Config {
    pub fn from_file(path: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let text = std::fs::read_to_string(path)?;
        let cfg: Config = toml::from_str(&text)?;
        Ok(cfg)
    }
}
