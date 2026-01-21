//! Configuration file handling for dibs.
//!
//! Looks for `.config/dibs.styx` in the current directory or any parent directory.

pub use dibs_config::Config;

use std::path::{Path, PathBuf};

/// Load configuration from `.config/dibs.styx`, searching up the directory tree.
pub fn load() -> Result<(Config, PathBuf), ConfigError> {
    let cwd = std::env::current_dir().map_err(|e| ConfigError::Io(e.to_string()))?;
    load_from(&cwd)
}

/// Load configuration starting from a specific directory.
pub fn load_from(start: &Path) -> Result<(Config, PathBuf), ConfigError> {
    let config_path = find_config_file(start)?;
    let content =
        std::fs::read_to_string(&config_path).map_err(|e| ConfigError::Io(e.to_string()))?;

    let config: Config =
        facet_styx::from_str(&content).map_err(|e| ConfigError::Parse(e.to_string()))?;

    Ok((config, config_path))
}

/// Find `.config/dibs.styx` by searching up the directory tree.
fn find_config_file(start: &Path) -> Result<PathBuf, ConfigError> {
    let mut current = start.to_path_buf();

    loop {
        let config_path = current.join(".config/dibs.styx");
        if config_path.exists() {
            return Ok(config_path);
        }

        if !current.pop() {
            return Err(ConfigError::NotFound);
        }
    }
}

/// Errors that can occur when loading configuration.
#[derive(Debug)]
pub enum ConfigError {
    /// No `.config/dibs.styx` found in any parent directory
    NotFound,
    /// I/O error reading the file
    Io(String),
    /// Parse error in the Styx file
    Parse(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::NotFound => {
                write!(
                    f,
                    "No .config/dibs.styx found in current directory or any parent"
                )
            }
            ConfigError::Io(e) => write!(f, "Failed to read .config/dibs.styx: {}", e),
            ConfigError::Parse(e) => write!(f, "Failed to parse .config/dibs.styx: {}", e),
        }
    }
}

impl std::error::Error for ConfigError {}
