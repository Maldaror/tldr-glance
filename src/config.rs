use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Every TLDR newsletter edition (slug, human-readable name), from
/// tldr.tech/newsletters.
pub const ALL_EDITIONS: &[(&str, &str)] = &[
    ("tech", "Startups, Tech & Programming"),
    ("dev", "Dev"),
    ("ai", "AI"),
    ("infosec", "Information Security"),
    ("product", "Product Management"),
    ("devops", "DevOps"),
    ("founders", "Founders"),
    ("design", "Design"),
    ("marketing", "Marketing"),
    ("crypto", "Crypto"),
    ("fintech", "Fintech"),
    ("it", "IT"),
    ("data", "Data"),
    ("hardware", "Hardware"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub editions: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config { editions: vec!["tech".into(), "ai".into(), "dev".into()] }
    }
}

fn config_path() -> Result<PathBuf> {
    let dir = dirs::config_dir().context("kein Config-Verzeichnis gefunden")?.join("tldr-glance");
    Ok(dir.join("config.toml"))
}

/// Loads the saved edition selection, or writes and returns the default if
/// no config file exists yet.
pub fn load() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        let config = Config::default();
        save(&config)?;
        return Ok(config);
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(toml::from_str(&text)?)
}

pub fn save(config: &Config) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, toml::to_string_pretty(config)?)?;
    Ok(())
}
