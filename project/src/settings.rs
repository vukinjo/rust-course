//! The service's settings, in layers, like Spring Boot's profiles:
//!
//! 1. `config/base.yaml`: defaults for every environment;
//! 2. `config/{APP_ENV}.yaml`: the active environment, default `local`;
//! 3. environment variables `APP_<SECTION>__<KEY>`, which override both,
//!    for example `APP_DATABASE__URL`.
//!
//! The merged result is deserialized into `Settings`, so a missing or
//! mistyped value stops the service at start-up, with the key's name.
use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use config::{ConfigError, Environment, File};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Settings {
    /// The active environment's name; not read from the files
    #[serde(skip)]
    pub profile: String,
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
    pub url: String,
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
    /// Reads `config/` in the current directory and the process environment.
    pub fn load() -> Result<Settings, ConfigError> {
        let profile = env::var("APP_ENV").unwrap_or_else(|_| "local".to_string());
        Settings::from_sources(Path::new("config"), &profile, None)
    }

    /// The layers, from a directory of files and a profile name. `vars` stands
    /// in for the environment variables in tests (None reads the real ones),
    /// because changing the process environment is `unsafe` in edition 2024.
    pub fn from_sources(
        dir: &Path,
        profile: &str,
        vars: Option<HashMap<String, String>>,
    ) -> Result<Settings, ConfigError> {
        let file = |name: &str| File::with_name(&dir.join(name).to_string_lossy());
        let mut settings: Settings = config::Config::builder()
            .add_source(file("base"))
            // Required: a misspelled APP_ENV is an error, not silently "base only"
            .add_source(file(profile))
            .add_source(
                Environment::with_prefix("APP")
                    .prefix_separator("_") // APP_ then the key
                    .separator("__") // __ between levels: DATABASE__URL is database.url
                    .source(vars),
            )
            .build()?
            .try_deserialize()?;
        settings.profile = profile.to_string();
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(profile: &str, vars: &[(&str, &str)]) -> Result<Settings, ConfigError> {
        let vars = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Settings::from_sources(Path::new("config"), profile, Some(vars))
    }

    #[test]
    fn local_needs_no_environment_variables() {
        let settings = load("local", &[]).unwrap();
        assert_eq!(settings.server.addr.to_string(), "127.0.0.1:3000");
        assert!(settings.database.url.ends_with("@localhost:5432/shoes"));
        assert_eq!(settings.database.max_connections, 10);
        assert_eq!(settings.reviews.timeout(), Duration::from_secs(5));
    }

    #[test]
    fn production_overrides_base_and_variables_override_both() {
        let vars = [
            ("APP_DATABASE__URL", "postgres://u:p@db:5432/shoes"),
            ("APP_DATABASE__MAX_CONNECTIONS", "30"),
        ];
        let settings = load("production", &vars).unwrap();
        assert_eq!(settings.profile, "production");
        assert_eq!(settings.server.addr.to_string(), "0.0.0.0:3000"); // from production.yaml
        assert_eq!(settings.database.max_connections, 30); // from the variable
        assert_eq!(settings.reviews.url, "https://jsonplaceholder.typicode.com"); // from base.yaml
    }

    #[test]
    fn production_without_a_database_url_is_an_error() {
        let error = load("production", &[]).unwrap_err();
        assert_eq!(
            error.to_string(),
            r#"missing configuration field "database.url""#
        );
    }

    #[test]
    fn errors_name_the_key_or_the_file() {
        let error = load("local", &[("APP_DATABASE__MAX_CONNECTIONS", "many")]).unwrap_err();
        assert!(
            error.to_string().contains("database.max_connections"),
            "{error}"
        );
        let error = load("staging", &[]).unwrap_err();
        assert_eq!(
            error.to_string(),
            r#"configuration file "config/staging" not found"#
        );
    }
}
