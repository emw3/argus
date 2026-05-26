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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for PortsConfig {
    fn default() -> Self {
        Self { enabled: true }
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

    #[serde(default)]
    pub ports: PortsConfig,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let cfg = Config::default();
        assert_eq!(cfg.general.refresh_interval_ms, 2000);
        assert_eq!(cfg.general.default_filter, "all");
        assert!(cfg.providers.systemd);
        assert!(cfg.providers.docker);
        assert!(cfg.providers.pm2);
        assert!(cfg.sessions.ssh);
        assert!(cfg.sessions.tmux);
        assert!(cfg.sessions.claude);
        assert!(cfg.ports.enabled);
    }

    #[test]
    fn empty_toml_uses_defaults() {
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg.general.refresh_interval_ms, 2000);
        assert_eq!(cfg.general.default_filter, "all");
        assert!(cfg.providers.systemd);
        assert!(cfg.ports.enabled);
    }

    #[test]
    fn partial_toml_general_only() {
        let toml = r#"
[general]
refresh_interval_ms = 5000
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.general.refresh_interval_ms, 5000);
        assert_eq!(cfg.general.default_filter, "all"); // default
        assert!(cfg.providers.systemd); // defaults
        assert!(cfg.ports.enabled); // defaults
    }

    #[test]
    fn ports_disabled() {
        let toml = r#"
[ports]
enabled = false
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert!(!cfg.ports.enabled);
        // Other sections default
        assert_eq!(cfg.general.refresh_interval_ms, 2000);
        assert!(cfg.providers.systemd);
    }

    #[test]
    fn ports_section_empty_defaults_enabled() {
        // Bare [ports] section with no keys -> enabled defaults to true
        let toml = r#"
[ports]
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert!(cfg.ports.enabled);
    }

    #[test]
    fn providers_partial() {
        let toml = r#"
[providers]
docker = false
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert!(cfg.providers.systemd); // defaults to true
        assert!(!cfg.providers.docker);
        assert!(cfg.providers.pm2); // defaults to true
    }

    #[test]
    fn sessions_partial() {
        let toml = r#"
[sessions]
claude = false
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert!(cfg.sessions.ssh);
        assert!(cfg.sessions.tmux);
        assert!(!cfg.sessions.claude);
    }

    #[test]
    fn full_toml() {
        let toml = r#"
[general]
refresh_interval_ms = 1000
default_filter = "systemd"

[providers]
systemd = true
docker = false
pm2 = false

[sessions]
ssh = true
tmux = false
claude = true

[ports]
enabled = false
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.general.refresh_interval_ms, 1000);
        assert_eq!(cfg.general.default_filter, "systemd");
        assert!(cfg.providers.systemd);
        assert!(!cfg.providers.docker);
        assert!(!cfg.providers.pm2);
        assert!(cfg.sessions.ssh);
        assert!(!cfg.sessions.tmux);
        assert!(cfg.sessions.claude);
        assert!(!cfg.ports.enabled);
    }

    #[test]
    fn load_missing_file_returns_default() {
        let cfg = Config::load(Some("/tmp/argus_test_nonexistent_config.toml"));
        assert_eq!(cfg.general.refresh_interval_ms, 2000);
        assert!(cfg.ports.enabled);
    }

    #[test]
    fn load_from_temp_file() {
        let path = "/tmp/argus_test_config.toml";
        std::fs::write(
            path,
            r#"
[general]
refresh_interval_ms = 500

[ports]
enabled = false
"#,
        )
        .unwrap();

        let cfg = Config::load(Some(path));
        assert_eq!(cfg.general.refresh_interval_ms, 500);
        assert!(!cfg.ports.enabled);
        assert!(cfg.providers.systemd); // default

        // Cleanup
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn load_invalid_toml_returns_default() {
        let path = "/tmp/argus_test_bad_config.toml";
        std::fs::write(path, "this is not valid toml {{{").unwrap();

        let cfg = Config::load(Some(path));
        assert_eq!(cfg.general.refresh_interval_ms, 2000); // default
        assert!(cfg.ports.enabled); // default

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn config_roundtrip_serialize_deserialize() {
        let cfg = Config::default();
        let toml_str = toml::to_string(&cfg).unwrap();
        let cfg2: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(
            cfg2.general.refresh_interval_ms,
            cfg.general.refresh_interval_ms
        );
        assert_eq!(cfg2.ports.enabled, cfg.ports.enabled);
    }
}
