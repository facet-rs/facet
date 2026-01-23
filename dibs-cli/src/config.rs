//! Configuration file handling for dibs.
//!
//! Looks for `.config/dibs.styx` in the current directory or any parent directory.

pub use dibs_config::Config;

use std::path::{Path, PathBuf};
use std::process::Command;

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

/// Find the migrations directory for the configured crate.
///
/// Uses `cargo metadata` to find the crate path, then returns `{crate_path}/src/migrations`.
/// Falls back to `./src/migrations` if no crate is configured or if the crate can't be found.
pub fn find_migrations_dir(config: &Config, project_root: &Path) -> PathBuf {
    if let Some(crate_name) = &config.db.crate_name
        && let Some(crate_path) = find_crate_path(crate_name, project_root)
    {
        return crate_path.join("src/migrations");
    }
    // Fallback to current directory
    PathBuf::from("src/migrations")
}

/// Cargo metadata output (subset we care about)
#[derive(Debug, facet::Facet)]
struct CargoMetadata {
    packages: Vec<Package>,
}

#[derive(Debug, facet::Facet)]
struct Package {
    name: String,
    manifest_path: String,
}

/// Find the path to a crate in the workspace using cargo metadata.
fn find_crate_path(crate_name: &str, project_root: &Path) -> Option<PathBuf> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(project_root)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let metadata: CargoMetadata = facet_json::from_str(&stdout).ok()?;

    for package in metadata.packages {
        if package.name == crate_name {
            let manifest_path = PathBuf::from(&package.manifest_path);
            return manifest_path.parent().map(|p| p.to_path_buf());
        }
    }

    None
}

/// Find the path to a crate for file watching.
/// Uses the current directory as the project root.
pub fn find_crate_path_for_watch(crate_name: &str) -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    find_crate_path(crate_name, &cwd)
}
