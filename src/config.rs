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

/// First entry in the browser picker: means "use the system default",
/// stored as `None` in `Config::browser`.
pub const SYSTEM_DEFAULT_BROWSER: &str = "Systemstandard";

/// One entry in the browser picker, with how to launch it on each supported
/// platform: `macos_app` is the `.app` bundle name for `open -a`, `linux_bins`
/// are candidate executable names tried in `$PATH` order. An empty
/// `linux_bins` means the browser isn't available on Linux/Unix.
pub struct BrowserEntry {
    pub display_name: &'static str,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub macos_app: &'static str,
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub linux_bins: &'static [&'static str],
}

pub const BROWSER_ENTRIES: &[BrowserEntry] = &[
    BrowserEntry { display_name: "Safari", macos_app: "Safari", linux_bins: &[] },
    BrowserEntry {
        display_name: "Google Chrome",
        macos_app: "Google Chrome",
        linux_bins: &["google-chrome-stable", "google-chrome"],
    },
    BrowserEntry { display_name: "Firefox", macos_app: "Firefox", linux_bins: &["firefox"] },
    BrowserEntry {
        display_name: "Microsoft Edge",
        macos_app: "Microsoft Edge",
        linux_bins: &["microsoft-edge-stable", "microsoft-edge"],
    },
    BrowserEntry {
        display_name: "Brave Browser",
        macos_app: "Brave Browser",
        linux_bins: &["brave-browser", "brave"],
    },
    BrowserEntry { display_name: "Vivaldi", macos_app: "Vivaldi", linux_bins: &["vivaldi-stable", "vivaldi"] },
    BrowserEntry { display_name: "Opera", macos_app: "Opera", linux_bins: &["opera"] },
    BrowserEntry { display_name: "Arc", macos_app: "Arc", linux_bins: &[] },
];

/// Best-effort check whether `entry.macos_app` sits as a `.app` bundle in one
/// of the standard Applications directories. Doesn't launch or reveal
/// anything, so it's safe to call just to filter a menu.
#[cfg(target_os = "macos")]
fn is_installed(entry: &BrowserEntry) -> bool {
    let bundle = format!("{}.app", entry.macos_app);
    let mut dirs = vec![PathBuf::from("/Applications"), PathBuf::from("/System/Applications")];
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join("Applications"));
    }
    dirs.iter().any(|dir| dir.join(&bundle).exists())
}

/// Best-effort check whether any of `entry.linux_bins` resolves to an
/// executable file somewhere in `$PATH`.
#[cfg(not(target_os = "macos"))]
fn is_installed(entry: &BrowserEntry) -> bool {
    entry.linux_bins.iter().any(|bin| find_in_path(bin).is_some())
}

#[cfg(not(target_os = "macos"))]
fn find_in_path(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|dir| dir.join(bin)).find(|p| p.is_file())
}

/// Browser choices for the settings dialog: "Systemstandard" plus every
/// entry from `BROWSER_ENTRIES` that is actually installed. `current` (the
/// currently configured browser, if any) is always included even if it can
/// no longer be found, so an existing config is never silently altered.
pub fn available_browsers(current: Option<&str>) -> Vec<String> {
    let mut list = vec![SYSTEM_DEFAULT_BROWSER.to_string()];
    list.extend(
        BROWSER_ENTRIES
            .iter()
            .filter(|entry| is_installed(entry) || current == Some(entry.display_name))
            .map(|entry| entry.display_name.to_string()),
    );
    list
}

/// Opens `url` in the given browser (by `display_name`, as stored in
/// `Config::browser`), or in the system default if `browser` is `None` or
/// unrecognized.
#[cfg(target_os = "macos")]
pub fn open_url(url: &str, browser: Option<&str>) -> std::io::Result<std::process::Child> {
    let mut cmd = std::process::Command::new("open");
    if let Some(entry) = browser.and_then(|name| BROWSER_ENTRIES.iter().find(|e| e.display_name == name)) {
        cmd.arg("-a").arg(entry.macos_app);
    }
    cmd.arg(url).spawn()
}

/// Opens `url` in the given browser (by `display_name`), trying each of its
/// `linux_bins` in `$PATH` in order; falls back to `xdg-open` (the desktop
/// default) if none matched or no browser was configured.
#[cfg(not(target_os = "macos"))]
pub fn open_url(url: &str, browser: Option<&str>) -> std::io::Result<std::process::Child> {
    if let Some(entry) = browser.and_then(|name| BROWSER_ENTRIES.iter().find(|e| e.display_name == name)) {
        for bin in entry.linux_bins {
            if find_in_path(bin).is_some() {
                return std::process::Command::new(bin).arg(url).spawn();
            }
        }
    }
    std::process::Command::new("xdg-open").arg(url).spawn()
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
    let dir = dirs::home_dir().context("kein Home-Verzeichnis gefunden")?.join(".config").join("tldr-glance");
    Ok(dir.join("config.toml"))
}

/// Pre-migration config location (`~/Library/Application Support/tldr-glance`
/// on macOS), kept only to pick up configs written by older versions.
fn legacy_config_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("tldr-glance").join("config.toml"))
}

/// Loads the saved edition selection, or writes and returns the default if
/// no config file exists yet. If a config from the pre-XDG location exists
/// but the new one doesn't, it's moved over first.
pub fn load() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        if let Some(legacy_path) = legacy_config_path() {
            if legacy_path.exists() && legacy_path != path {
                let text = std::fs::read_to_string(&legacy_path)?;
                let config: Config = toml::from_str(&text)?;
                save(&config)?;
                let _ = std::fs::remove_file(&legacy_path);
                return Ok(config);
            }
        }
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
