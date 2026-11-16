//! The service's settings, read from environment variables in every
//! environment: `APP_ENVIRONMENT` and one variable `APP_<SECTION>__<KEY>` per
//! value, for example `APP_DATABASE__URL`. The binary reads no file and has no
//! defaults, so no configuration ships with it or with the container image.
//! On your machine, `cargo run` and `cargo test` get the local values from
//! `.cargo/config.toml`, which Cargo turns into variables.
//!
//! The values are deserialized into `Settings`, so a missing or mistyped value
//! stops the service at start-up, with the key's name.
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

use config::ConfigError;
use secrecy::SecretString;
use serde::Deserialize;

/// Where the service runs, from `APP_ENVIRONMENT`. It shows up in the logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")] // deserialized as a string, then checked by try_from
pub enum Environment {
    /// `cargo run` on your machine
    Local,
    Development,
    Staging,
    Production,
    Release,
}

impl Environment {
    pub fn as_str(self) -> &'static str {
        match self {
            Environment::Local => "local",
            Environment::Development => "development",
            Environment::Staging => "staging",
            Environment::Production => "production",
            Environment::Release => "release",
        }
    }
}

/// A misspelled name is an error, not an environment of its own.
impl TryFrom<String> for Environment {
    type Error = String;

    fn try_from(name: String) -> Result<Environment, String> {
        match name.as_str() {
            "local" => Ok(Environment::Local),
            "development" => Ok(Environment::Development),
            "staging" => Ok(Environment::Staging),
            "production" => Ok(Environment::Production),
            "release" => Ok(Environment::Release),
            _ => Err(format!(
                "must be local, development, staging, production or release, got {name:?}"
            )),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub environment: Environment,
    pub server: ServerSettings,
    pub database: DatabaseSettings,
    pub reviews: ReviewsSettings,
}

#[derive(Debug, Deserialize)]
pub struct ServerSettings {
    pub addr: SocketAddr,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseSettings {
    /// Contains the password. `SecretString` prints as `[REDACTED]` in debug
    /// output, and reading it takes an explicit `expose_secret()`.
    pub url: SecretString,
    pub max_connections: u32,
}

#[derive(Debug, Deserialize)]
pub struct ReviewsSettings {
    pub url: String,
    pub timeout_ms: u64,
}

impl ReviewsSettings {
    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }
}

impl Settings {
    /// Reads the process environment.
    pub fn load() -> Result<Settings, ConfigError> {
        Settings::from_vars(None)
    }

    /// The settings from the variables. `vars` stands in for the process
    /// environment in tests (None reads the real one), because changing the
    /// process environment is `unsafe` in edition 2024.
    pub fn from_vars(vars: Option<HashMap<String, String>>) -> Result<Settings, ConfigError> {
        config::Config::builder()
            .add_source(
                config::Environment::with_prefix("APP")
                    .prefix_separator("_") // APP_ then the key
                    .separator("__") // __ between levels: DATABASE__URL is database.url
                    .source(vars),
            )
            .build()?
            .try_deserialize()
    }
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret;

    use super::*;

    fn load(vars: &[(&str, &str)]) -> Result<Settings, ConfigError> {
        let vars = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Settings::from_vars(Some(vars))
    }

    /// Everything an environment has to set
    const PRODUCTION: [(&str, &str); 6] = [
        ("APP_ENVIRONMENT", "production"),
        ("APP_SERVER__ADDR", "0.0.0.0:3000"),
        ("APP_DATABASE__URL", "postgres://u:secret@db:5432/shoes"),
        ("APP_DATABASE__MAX_CONNECTIONS", "20"),
        ("APP_REVIEWS__URL", "https://reviews.example.com"),
        ("APP_REVIEWS__TIMEOUT_MS", "2000"),
    ];

    /// Replaces or adds one variable
    fn with<'a>(key: &'a str, value: &'a str) -> Vec<(&'a str, &'a str)> {
        let mut vars: Vec<_> = PRODUCTION.into_iter().filter(|(k, _)| *k != key).collect();
        vars.push((key, value));
        vars
    }

    #[test]
    fn every_value_comes_from_a_variable() {
        let settings = load(&PRODUCTION).unwrap();
        assert_eq!(settings.environment, Environment::Production);
        assert_eq!(settings.server.addr.to_string(), "0.0.0.0:3000");
        assert_eq!(settings.database.max_connections, 20);
        assert_eq!(settings.reviews.url, "https://reviews.example.com");
        assert_eq!(settings.reviews.timeout(), Duration::from_secs(2));
        assert_eq!(
            settings.database.url.expose_secret(),
            "postgres://u:secret@db:5432/shoes"
        );
        // Debug output, as in a log line, doesn't show the password
        assert!(!format!("{settings:?}").contains("secret@"));
    }

    #[test]
    fn cargo_config_sets_every_local_value() {
        // The real process environment: under `cargo test` it holds the
        // values from .cargo/config.toml, unless the shell overrides them
        let settings = Settings::load().unwrap();
        assert_eq!(settings.environment, Environment::Local);
    }

    #[test]
    fn a_missing_variable_is_an_error() {
        let vars: Vec<_> = PRODUCTION
            .into_iter()
            .filter(|(key, _)| *key != "APP_REVIEWS__URL")
            .collect();
        let error = load(&vars).unwrap_err();
        assert_eq!(
            error.to_string(),
            r#"missing configuration field "reviews.url""#
        );
    }

    #[test]
    fn errors_name_the_key() {
        let error = load(&with("APP_DATABASE__MAX_CONNECTIONS", "many")).unwrap_err();
        assert!(
            error.to_string().contains("database.max_connections"),
            "{error}"
        );
        let error = load(&with("APP_ENVIRONMENT", "prodution")).unwrap_err();
        assert_eq!(
            error.to_string(),
            r#"must be local, development, staging, production or release, got "prodution" for key `environment`"#
        );
    }
}
