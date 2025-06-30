use serde::Deserialize;
use std::error::Error;

#[derive(Deserialize)]
pub struct Config {
    pub port: u16,
    #[serde(default)]
    pub groups: Vec<String>,
}

impl Config {
    pub fn from_file(path: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let text = std::fs::read_to_string(path)?;
        let cfg: Config = toml::from_str(&text)?;
        Ok(cfg)
    }
}
