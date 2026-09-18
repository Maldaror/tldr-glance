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

/// Browser choices offered in the settings dialog. The first entry means "use
/// the system default", stored as `None` in `Config::browser`; the rest are
/// passed to `open -a <name>` and must match the app's macOS display name.
pub const BROWSERS: &[&str] = &[
    "Systemstandard",
    "Safari",
    "Google Chrome",
    "Firefox",
    "Microsoft Edge",
    "Brave Browser",
    "Vivaldi",
    "Opera",
    "Arc",
];

/// Best-effort check whether `<app_name>.app` sits in one of the standard
/// Applications directories. Doesn't launch or reveal anything, so it's safe
/// to call just to filter a menu.
fn is_installed(app_name: &str) -> bool {
    let bundle = format!("{app_name}.app");
    let mut dirs = vec![PathBuf::from("/Applications"), PathBuf::from("/System/Applications")];
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join("Applications"));
    }
    dirs.iter().any(|dir| dir.join(&bundle).exists())
}

/// Browser choices for the settings dialog: "Systemstandard" plus every
/// entry from `BROWSERS` that is actually installed. `current` (the
/// currently configured browser, if any) is always included even if its app
/// can no longer be found, so an existing config is never silently altered.
pub fn available_browsers(current: Option<&str>) -> Vec<String> {
    let mut list = vec![BROWSERS[0].to_string()];
    list.extend(
        BROWSERS[1..].iter().filter(|name| is_installed(name) || current == Some(**name)).map(|name| name.to_string()),
    );
    list
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub editions: Vec<String>,
    #[serde(default)]
    pub browser: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config { editions: vec!["tech".into(), "ai".into(), "dev".into()], browser: None }
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
