use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn default_refresh_interval_ms() -> u64 {
    2000
}

fn default_filter() -> String {
    "all".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_refresh_interval_ms")]
    pub refresh_interval_ms: u64,

    #[serde(default = "default_filter")]
    pub default_filter: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            refresh_interval_ms: default_refresh_interval_ms(),
            default_filter: default_filter(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidersConfig {
    #[serde(default = "default_true")]
    pub systemd: bool,

    #[serde(default = "default_true")]
    pub docker: bool,

    #[serde(default = "default_true")]
    pub pm2: bool,
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            systemd: true,
            docker: true,
            pm2: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionsConfig {
    #[serde(default = "default_true")]
    pub ssh: bool,

    #[serde(default = "default_true")]
    pub tmux: bool,

    #[serde(default = "default_true")]
    pub claude: bool,
}

impl Default for SessionsConfig {
    fn default() -> Self {
        Self {
            ssh: true,
            tmux: true,
            claude: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,

    #[serde(default)]
    pub providers: ProvidersConfig,

    #[serde(default)]
    pub sessions: SessionsConfig,
}

impl Config {
    /// Load config from the given path, or from the default location.
    /// Falls back to default config on any error (missing file, parse error).
    pub fn load(custom_path: Option<&str>) -> Self {
        let path = if let Some(p) = custom_path {
            PathBuf::from(p)
        } else {
            default_config_path()
        };

        std::fs::read_to_string(&path)
            .ok()
            .and_then(|contents| toml::from_str(&contents).ok())
            .unwrap_or_default()
    }
}

fn default_config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    PathBuf::from(home)
        .join(".config")
        .join("argus")
        .join("config.toml")
}
