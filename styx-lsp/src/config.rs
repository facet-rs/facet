//! User configuration for the Styx LSP.
//!
//! Configuration is stored at `~/.config/styx/config.styx` and includes
//! user preferences like allowed LSP extensions.

use std::path::PathBuf;

use facet::Facet;
use tracing::{debug, info, warn};

// Embed the schema for the user config file (generated from StyxUserConfig in build.rs).
// This allows the Styx LSP to provide schema validation for config.styx files.
styx_embed::embed_outdir_file!("config.schema.styx");

/// User configuration for the Styx LSP.
///
/// This configuration is stored at `~/.config/styx/config.styx` and persists
/// user preferences across sessions.
#[derive(Debug, Clone, Default, Facet)]
pub struct StyxUserConfig {
    /// LSP extensions that are allowed to run.
    ///
    /// When a schema specifies an LSP extension (via `meta.lsp.launch`),
    /// the extension command must be in this list to be spawned.
    /// Extensions can be added via the "Allow LSP extension" code action.
    #[facet(default)]
    pub allowed_extensions: Vec<String>,
}

/// Get the path to the user config file.
///
/// Returns `~/.config/styx/config.styx` on Unix, or the equivalent
/// platform-specific config directory on other platforms.
pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("styx").join("config.styx"))
}

/// Load the user configuration from disk.
///
/// Returns `Ok(None)` if the config file doesn't exist yet.
/// Returns `Err` if the file exists but couldn't be parsed.
pub fn load_config() -> Result<Option<StyxUserConfig>, LoadConfigError> {
    let Some(path) = config_path() else {
        debug!("No config directory available");
        return Ok(None);
    };

    if !path.exists() {
        debug!(?path, "Config file does not exist yet");
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path).map_err(|e| LoadConfigError::Io {
        path: path.clone(),
        error: e,
    })?;

    let config: StyxUserConfig =
        facet_styx::from_str(&content).map_err(|e| LoadConfigError::Parse {
            path: path.clone(),
            error: e.to_string(),
        })?;

    info!(?path, extensions = config.allowed_extensions.len(), "Loaded user config");
    Ok(Some(config))
}

/// Save the user configuration to disk.
///
/// Creates the config directory if it doesn't exist.
pub fn save_config(config: &StyxUserConfig) -> Result<(), SaveConfigError> {
    let Some(path) = config_path() else {
        warn!("No config directory available, cannot save config");
        return Err(SaveConfigError::NoConfigDir);
    };

    // Create the directory if it doesn't exist
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| SaveConfigError::Io {
            path: path.clone(),
            error: e,
        })?;
    }

    // Serialize with the schema reference
    let content = format!(
        "meta {{ @schema crate:styx-lsp-config@1 }}\n\n{}",
        facet_styx::to_string(config).map_err(|e| SaveConfigError::Serialize {
            error: e.to_string(),
        })?
    );

    std::fs::write(&path, &content).map_err(|e| SaveConfigError::Io {
        path: path.clone(),
        error: e,
    })?;

    info!(?path, "Saved user config");
    Ok(())
}

/// Error loading the user config.
#[derive(Debug)]
pub enum LoadConfigError {
    Io { path: PathBuf, error: std::io::Error },
    Parse { path: PathBuf, error: String },
}

impl std::fmt::Display for LoadConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadConfigError::Io { path, error } => {
                write!(f, "Failed to read config at {}: {}", path.display(), error)
            }
            LoadConfigError::Parse { path, error } => {
                write!(f, "Failed to parse config at {}: {}", path.display(), error)
            }
        }
    }
}

impl std::error::Error for LoadConfigError {}

/// Error saving the user config.
#[derive(Debug)]
pub enum SaveConfigError {
    NoConfigDir,
    Io { path: PathBuf, error: std::io::Error },
    Serialize { error: String },
}

impl std::fmt::Display for SaveConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveConfigError::NoConfigDir => write!(f, "No config directory available"),
            SaveConfigError::Io { path, error } => {
                write!(f, "Failed to write config at {}: {}", path.display(), error)
            }
            SaveConfigError::Serialize { error } => {
                write!(f, "Failed to serialize config: {}", error)
            }
        }
    }
}

impl std::error::Error for SaveConfigError {}
