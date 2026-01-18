//! Configuration file handling for dibs.
//!
//! Looks for `dibs.toml` in the current directory or any parent directory.

use facet::Facet;
use std::path::{Path, PathBuf};

/// Configuration loaded from `dibs.toml`.
#[derive(Debug, Clone, Facet)]
pub struct Config {
    /// Database crate configuration
    #[facet(default)]
    pub db: DbConfig,
}

/// Database crate configuration.
#[derive(Debug, Clone, Facet, Default)]
pub struct DbConfig {
    /// Name of the crate containing schema definitions (e.g., "my-app-db")
    pub crate_name: Option<String>,

    /// Path to a pre-built binary (for faster iteration)
    /// If not specified, we'll use `cargo run -p <crate_name>`
    pub binary: Option<String>,
}

impl Config {
    /// Load configuration from `dibs.toml`, searching up the directory tree.
    pub fn load() -> Result<(Config, PathBuf), ConfigError> {
        let cwd = std::env::current_dir().map_err(|e| ConfigError::Io(e.to_string()))?;
        Self::load_from(&cwd)
    }

    /// Load configuration starting from a specific directory.
    pub fn load_from(start: &Path) -> Result<(Config, PathBuf), ConfigError> {
        let config_path = Self::find_config_file(start)?;
        let content =
            std::fs::read_to_string(&config_path).map_err(|e| ConfigError::Io(e.to_string()))?;

        let config: Config =
            facet_toml::from_str(&content).map_err(|e| ConfigError::Parse(e.to_string()))?;

        Ok((config, config_path))
    }

    /// Find `dibs.toml` by searching up the directory tree.
    fn find_config_file(start: &Path) -> Result<PathBuf, ConfigError> {
        let mut current = start.to_path_buf();

        loop {
            let config_path = current.join("dibs.toml");
            if config_path.exists() {
                return Ok(config_path);
            }

            if !current.pop() {
                return Err(ConfigError::NotFound);
            }
        }
    }
}

/// Errors that can occur when loading configuration.
#[derive(Debug)]
pub enum ConfigError {
    /// No `dibs.toml` found in any parent directory
    NotFound,
    /// I/O error reading the file
    Io(String),
    /// Parse error in the TOML file
    Parse(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::NotFound => {
                write!(f, "No dibs.toml found in current directory or any parent")
            }
            ConfigError::Io(e) => write!(f, "Failed to read dibs.toml: {}", e),
            ConfigError::Parse(e) => write!(f, "Failed to parse dibs.toml: {}", e),
        }
    }
}

impl std::error::Error for ConfigError {}
