//! Server and KataGo configuration.
//!
//! Precedence, lowest to highest: built-in defaults, `config.toml`, environment
//! variables. Environment lookup is injected so tests never touch process state.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

/// Default config file name looked up in the working directory.
pub const DEFAULT_CONFIG_FILE: &str = "config.toml";

/// How log lines are formatted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Human-readable text (default).
    #[default]
    Text,
    /// One JSON object per line, for log aggregators.
    Json,
}

impl std::str::FromStr for LogFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "text" | "pretty" | "plain" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            other => bail!("unknown log format {other:?} (expected \"text\" or \"json\")"),
        }
    }
}

/// HTTP server settings.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    /// Address to bind. `::` binds both IPv6 and IPv4 on most systems.
    pub host: String,
    /// TCP port to listen on.
    pub port: u16,
    /// Upper bound on any single HTTP request, including game analysis.
    pub request_timeout_secs: u64,
    /// Requests being processed at once before the server sheds load with 503.
    pub max_concurrent_requests: usize,
    /// Maximum accepted request body size in bytes.
    pub max_body_bytes: usize,
    /// Allowed CORS origins. `["*"]` allows any origin.
    pub cors_allowed_origins: Vec<String>,
    /// Log line format.
    pub log_format: LogFormat,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "::".to_owned(),
            port: 2718,
            request_timeout_secs: 300,
            max_concurrent_requests: 256,
            max_body_bytes: 1024 * 1024,
            cors_allowed_origins: vec!["*".to_owned()],
            log_format: LogFormat::Text,
        }
    }
}

/// KataGo process settings.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct KatagoConfig {
    /// Path to the `katago` executable.
    pub katago_path: PathBuf,
    /// Path to the neural network (`.bin.gz`).
    pub model_path: PathBuf,
    /// Optional path to a Human SL network; enables `humanSLProfile` overrides.
    pub human_model_path: Option<PathBuf>,
    /// Path to the KataGo analysis engine `.cfg` file.
    pub config_path: PathBuf,
    /// Seconds to wait for each analysed position before giving up.
    pub move_timeout_secs: u64,
    /// `maxVisits` applied when a request does not specify one.
    pub default_max_visits: Option<u32>,
    /// Requests asking for more visits than this are rejected with 400.
    pub max_visits_limit: Option<u32>,
    /// How many times to restart KataGo after it exits before giving up.
    pub max_restart_attempts: u32,
}

impl Default for KatagoConfig {
    fn default() -> Self {
        Self {
            katago_path: PathBuf::from("./katago"),
            model_path: PathBuf::from("./model.bin.gz"),
            human_model_path: None,
            config_path: PathBuf::from("./analysis_config.cfg"),
            move_timeout_secs: 20,
            default_max_visits: Some(10),
            max_visits_limit: None,
            max_restart_attempts: 10,
        }
    }
}

/// Complete configuration.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// HTTP server settings.
    pub server: ServerConfig,
    /// KataGo process settings.
    pub katago: KatagoConfig,
}

impl Config {
    /// Parses TOML text.
    pub fn from_toml(text: &str) -> anyhow::Result<Self> {
        toml::from_str(text).context("invalid config file")
    }

    /// Reads and parses a TOML file.
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read config file {}", path.display()))?;
        Self::from_toml(&text).with_context(|| format!("in {}", path.display()))
    }

    /// Loads configuration from an explicit file, or `config.toml` if present,
    /// then applies environment overrides from the real process environment.
    pub fn load(explicit_path: Option<&Path>) -> anyhow::Result<Self> {
        let mut config = match explicit_path {
            Some(path) => Self::from_file(path)?,
            None if Path::new(DEFAULT_CONFIG_FILE).is_file() => {
                Self::from_file(Path::new(DEFAULT_CONFIG_FILE))?
            }
            None => Self::default(),
        };
        config.apply_env_overrides_with(|key| std::env::var(key).ok())?;
        Ok(config)
    }

    /// Applies environment variable overrides using the given lookup function.
    ///
    /// Returns an error if a variable is present but cannot be parsed, rather
    /// than silently ignoring a typo.
    pub fn apply_env_overrides_with(
        &mut self,
        get: impl Fn(&str) -> Option<String>,
    ) -> anyhow::Result<()> {
        fn parse<T: std::str::FromStr>(key: &str, value: &str) -> anyhow::Result<T>
        where
            T::Err: std::fmt::Display,
        {
            value
                .trim()
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid value for {key}={value:?}: {e}"))
        }

        if let Some(v) = get("KATAGO_SERVER_HOST") {
            self.server.host = v;
        }
        if let Some(v) = get("KATAGO_SERVER_PORT") {
            self.server.port = parse("KATAGO_SERVER_PORT", &v)?;
        }
        if let Some(v) = get("KATAGO_SERVER_REQUEST_TIMEOUT_SECS") {
            self.server.request_timeout_secs = parse("KATAGO_SERVER_REQUEST_TIMEOUT_SECS", &v)?;
        }
        if let Some(v) = get("KATAGO_SERVER_MAX_CONCURRENT_REQUESTS") {
            self.server.max_concurrent_requests =
                parse("KATAGO_SERVER_MAX_CONCURRENT_REQUESTS", &v)?;
        }
        if let Some(v) = get("KATAGO_SERVER_MAX_BODY_BYTES") {
            self.server.max_body_bytes = parse("KATAGO_SERVER_MAX_BODY_BYTES", &v)?;
        }
        if let Some(v) = get("KATAGO_SERVER_CORS_ALLOWED_ORIGINS") {
            self.server.cors_allowed_origins = v
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
                .collect();
        }
        if let Some(v) = get("KATAGO_SERVER_LOG_FORMAT") {
            self.server.log_format = parse("KATAGO_SERVER_LOG_FORMAT", &v)?;
        }
        if let Some(v) = get("KATAGO_KATAGO_PATH") {
            self.katago.katago_path = PathBuf::from(v);
        }
        if let Some(v) = get("KATAGO_MODEL_PATH") {
            self.katago.model_path = PathBuf::from(v);
        }
        if let Some(v) = get("KATAGO_HUMAN_MODEL_PATH") {
            self.katago.human_model_path = if v.trim().is_empty() {
                None
            } else {
                Some(PathBuf::from(v))
            };
        }
        if let Some(v) = get("KATAGO_CONFIG_PATH") {
            self.katago.config_path = PathBuf::from(v);
        }
        if let Some(v) = get("KATAGO_MOVE_TIMEOUT_SECS") {
            self.katago.move_timeout_secs = parse("KATAGO_MOVE_TIMEOUT_SECS", &v)?;
        }
        if let Some(v) = get("KATAGO_DEFAULT_MAX_VISITS") {
            self.katago.default_max_visits = if v.trim().is_empty() || v.trim() == "0" {
                None
            } else {
                Some(parse("KATAGO_DEFAULT_MAX_VISITS", &v)?)
            };
        }
        if let Some(v) = get("KATAGO_MAX_VISITS_LIMIT") {
            self.katago.max_visits_limit = if v.trim().is_empty() || v.trim() == "0" {
                None
            } else {
                Some(parse("KATAGO_MAX_VISITS_LIMIT", &v)?)
            };
        }
        if let Some(v) = get("KATAGO_MAX_RESTART_ATTEMPTS") {
            self.katago.max_restart_attempts = parse("KATAGO_MAX_RESTART_ATTEMPTS", &v)?;
        }
        Ok(())
    }

    /// Checks values for internal consistency. Does not touch the filesystem.
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.server.port == 0 {
            bail!("server.port must be between 1 and 65535");
        }
        if self.server.request_timeout_secs == 0 {
            bail!("server.request_timeout_secs must be greater than 0");
        }
        if self.server.max_concurrent_requests == 0 {
            bail!("server.max_concurrent_requests must be greater than 0");
        }
        if self.server.max_body_bytes < 1024 {
            bail!("server.max_body_bytes must be at least 1024");
        }
        if self.server.cors_allowed_origins.is_empty() {
            bail!("server.cors_allowed_origins must not be empty (use [\"*\"] to allow any)");
        }
        if self.katago.move_timeout_secs == 0 {
            bail!("katago.move_timeout_secs must be greater than 0");
        }
        if self.katago.default_max_visits == Some(0) {
            bail!("katago.default_max_visits must be greater than 0 or unset");
        }
        if let (Some(default), Some(limit)) =
            (self.katago.default_max_visits, self.katago.max_visits_limit)
            && default > limit
        {
            bail!(
                "katago.default_max_visits ({default}) exceeds katago.max_visits_limit ({limit})"
            );
        }
        if self.server.request_timeout_secs < self.katago.move_timeout_secs {
            bail!(
                "server.request_timeout_secs ({}) must be at least katago.move_timeout_secs ({})",
                self.server.request_timeout_secs,
                self.katago.move_timeout_secs
            );
        }
        Ok(())
    }

    /// Checks that the KataGo binary, model(s) and config file exist.
    pub fn validate_paths(&self) -> anyhow::Result<()> {
        let k = &self.katago;
        if !k.katago_path.is_file() {
            bail!(
                "KataGo binary not found at {} (set katago.katago_path or KATAGO_KATAGO_PATH)",
                k.katago_path.display()
            );
        }
        if !k.model_path.is_file() {
            bail!(
                "KataGo model not found at {} (set katago.model_path or KATAGO_MODEL_PATH)",
                k.model_path.display()
            );
        }
        if let Some(human) = &k.human_model_path
            && !human.is_file()
        {
            bail!(
                "Human SL model not found at {} (set katago.human_model_path or KATAGO_HUMAN_MODEL_PATH)",
                human.display()
            );
        }
        if !k.config_path.is_file() {
            bail!(
                "KataGo analysis config not found at {} (set katago.config_path or KATAGO_CONFIG_PATH)",
                k.config_path.display()
            );
        }
        Ok(())
    }

    /// Socket address string to bind.
    pub fn bind_address(&self) -> String {
        let host = &self.server.host;
        if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]:{}", self.server.port)
        } else {
            format!("{host}:{}", self.server.port)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |key: &str| map.get(key).cloned()
    }

    #[test]
    fn defaults_are_sane_and_valid() {
        let config = Config::default();
        assert_eq!(config.server.host, "::");
        assert_eq!(config.server.port, 2718);
        assert_eq!(config.katago.katago_path, PathBuf::from("./katago"));
        assert_eq!(config.katago.move_timeout_secs, 20);
        assert_eq!(config.katago.default_max_visits, Some(10));
        config.validate().expect("defaults must validate");
    }

    #[test]
    fn env_overrides_everything() {
        let mut config = Config::default();
        config
            .apply_env_overrides_with(env(&[
                ("KATAGO_SERVER_HOST", "127.0.0.1"),
                ("KATAGO_SERVER_PORT", "3000"),
                ("KATAGO_SERVER_REQUEST_TIMEOUT_SECS", "600"),
                ("KATAGO_SERVER_MAX_CONCURRENT_REQUESTS", "8"),
                ("KATAGO_SERVER_MAX_BODY_BYTES", "4096"),
                (
                    "KATAGO_SERVER_CORS_ALLOWED_ORIGINS",
                    "https://a.example, https://b.example",
                ),
                ("KATAGO_SERVER_LOG_FORMAT", "json"),
                ("KATAGO_KATAGO_PATH", "/usr/bin/katago"),
                ("KATAGO_MODEL_PATH", "/models/best.bin.gz"),
                ("KATAGO_HUMAN_MODEL_PATH", "/models/human.bin.gz"),
                ("KATAGO_CONFIG_PATH", "/config/analysis.cfg"),
                ("KATAGO_MOVE_TIMEOUT_SECS", "30"),
                ("KATAGO_DEFAULT_MAX_VISITS", "50"),
                ("KATAGO_MAX_VISITS_LIMIT", "5000"),
                ("KATAGO_MAX_RESTART_ATTEMPTS", "3"),
            ]))
            .unwrap();
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 3000);
        assert_eq!(config.server.request_timeout_secs, 600);
        assert_eq!(config.server.max_concurrent_requests, 8);
        assert_eq!(config.server.max_body_bytes, 4096);
        assert_eq!(
            config.server.cors_allowed_origins,
            vec!["https://a.example", "https://b.example"]
        );
        assert_eq!(config.server.log_format, LogFormat::Json);
        assert_eq!(config.katago.katago_path, PathBuf::from("/usr/bin/katago"));
        assert_eq!(
            config.katago.model_path,
            PathBuf::from("/models/best.bin.gz")
        );
        assert_eq!(
            config.katago.human_model_path,
            Some(PathBuf::from("/models/human.bin.gz"))
        );
        assert_eq!(
            config.katago.config_path,
            PathBuf::from("/config/analysis.cfg")
        );
        assert_eq!(config.katago.move_timeout_secs, 30);
        assert_eq!(config.katago.default_max_visits, Some(50));
        assert_eq!(config.katago.max_visits_limit, Some(5000));
        assert_eq!(config.katago.max_restart_attempts, 3);
        config.validate().unwrap();
    }

    #[test]
    fn env_typos_are_errors_not_ignored() {
        let mut config = Config::default();
        let err = config
            .apply_env_overrides_with(env(&[("KATAGO_SERVER_PORT", "eighty")]))
            .unwrap_err();
        assert!(err.to_string().contains("KATAGO_SERVER_PORT"));
        let err = config
            .apply_env_overrides_with(env(&[("KATAGO_SERVER_LOG_FORMAT", "yaml")]))
            .unwrap_err();
        assert!(err.to_string().contains("log format"));
    }

    #[test]
    fn empty_or_zero_unsets_optional_limits() {
        let mut config = Config::default();
        config
            .apply_env_overrides_with(env(&[
                ("KATAGO_DEFAULT_MAX_VISITS", "0"),
                ("KATAGO_MAX_VISITS_LIMIT", ""),
                ("KATAGO_HUMAN_MODEL_PATH", ""),
            ]))
            .unwrap();
        assert_eq!(config.katago.default_max_visits, None);
        assert_eq!(config.katago.max_visits_limit, None);
        assert_eq!(config.katago.human_model_path, None);
    }

    #[test]
    fn toml_full_and_partial() {
        let config = Config::from_toml(
            r#"
[server]
host = "localhost"
port = 8080
cors_allowed_origins = ["https://goban.app"]

[katago]
katago_path = "/custom/katago"
model_path = "/custom/model.bin.gz"
config_path = "/custom/config.cfg"
move_timeout_secs = 15
"#,
        )
        .unwrap();
        assert_eq!(config.server.host, "localhost");
        assert_eq!(config.server.port, 8080);
        assert_eq!(
            config.server.cors_allowed_origins,
            vec!["https://goban.app"]
        );
        assert_eq!(config.katago.katago_path, PathBuf::from("/custom/katago"));
        assert_eq!(config.katago.move_timeout_secs, 15);
        assert_eq!(config.server.request_timeout_secs, 300); // default kept

        let partial = Config::from_toml("[katago]\nmodel_path = \"/m.bin.gz\"\n").unwrap();
        assert_eq!(partial.server.port, 2718);
        assert_eq!(partial.katago.model_path, PathBuf::from("/m.bin.gz"));
    }

    #[test]
    fn toml_rejects_unknown_keys() {
        let err = Config::from_toml("[server]\nprot = 1\n").unwrap_err();
        assert!(err.to_string().contains("invalid config file"));
    }

    #[test]
    fn validation_catches_inconsistencies() {
        let mut config = Config::default();
        config.server.request_timeout_secs = 5;
        assert!(
            config
                .validate()
                .unwrap_err()
                .to_string()
                .contains("request_timeout_secs")
        );

        let mut config = Config::default();
        config.katago.default_max_visits = Some(100);
        config.katago.max_visits_limit = Some(50);
        assert!(
            config
                .validate()
                .unwrap_err()
                .to_string()
                .contains("max_visits_limit")
        );

        let mut config = Config::default();
        config.server.cors_allowed_origins.clear();
        assert!(config.validate().is_err());

        let mut config = Config::default();
        config.katago.move_timeout_secs = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_paths_reports_which_file_is_missing() {
        let mut config = Config::default();
        config.katago.katago_path = PathBuf::from("/definitely/not/here/katago");
        let err = config.validate_paths().unwrap_err().to_string();
        assert!(err.contains("KataGo binary not found"));
    }

    #[test]
    fn bind_address_brackets_ipv6() {
        let mut config = Config::default();
        assert_eq!(config.bind_address(), "[::]:2718");
        config.server.host = "0.0.0.0".into();
        assert_eq!(config.bind_address(), "0.0.0.0:2718");
        config.server.host = "localhost".into();
        config.server.port = 1;
        assert_eq!(config.bind_address(), "localhost:1");
    }
}
