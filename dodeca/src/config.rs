//! Configuration file discovery and parsing
//!
//! Searches for `.config/dodeca.kdl` walking up from the current directory.
//! The project root is the parent of `.config/`.

use camino::{Utf8Path, Utf8PathBuf};
use color_eyre::{Result, eyre::eyre};
use facet::Facet;
use facet_kdl as kdl;
use std::env;
use std::fs;

/// Configuration file name
const CONFIG_DIR: &str = ".config";
const CONFIG_FILE: &str = "dodeca.kdl";

/// Dodeca configuration from `.config/dodeca.kdl`
#[derive(Debug, Clone, Facet)]
pub struct DodecaConfig {
    /// Content directory (relative to project root)
    #[facet(kdl::child)]
    pub content: ContentDir,

    /// Output directory (relative to project root)
    #[facet(kdl::child)]
    pub output: OutputDir,

    /// Link checking configuration
    #[facet(kdl::child, default)]
    pub link_check: LinkCheckConfig,
}

/// Link checking configuration
#[derive(Debug, Clone, Default, Facet)]
pub struct LinkCheckConfig {
    /// Domains to skip checking (anti-bot policies, known flaky, etc.)
    #[facet(kdl::children, default)]
    pub skip_domains: Vec<SkipDomain>,
}

/// A domain to skip during external link checking
#[derive(Debug, Clone, Facet)]
pub struct SkipDomain {
    #[facet(kdl::argument)]
    pub domain: String,
}

/// Content directory node
#[derive(Debug, Clone, Facet)]
pub struct ContentDir {
    #[facet(kdl::argument)]
    pub path: String,
}

/// Output directory node
#[derive(Debug, Clone, Facet)]
pub struct OutputDir {
    #[facet(kdl::argument)]
    pub path: String,
}

/// Discovered configuration with resolved paths
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    /// Project root (parent of .config/)
    pub _root: Utf8PathBuf,
    /// Absolute path to content directory
    pub content_dir: Utf8PathBuf,
    /// Absolute path to output directory
    pub output_dir: Utf8PathBuf,
    /// Domains to skip during external link checking
    pub skip_domains: Vec<String>,
}

impl ResolvedConfig {
    /// Discover and load configuration
    pub fn discover() -> Result<Option<Self>> {
        let config_path = find_config_file()?;

        match config_path {
            Some(path) => {
                let resolved = load_config(&path)?;
                Ok(Some(resolved))
            }
            None => Ok(None),
        }
    }
}

/// Search for `.config/dodeca.kdl` walking up from current directory
fn find_config_file() -> Result<Option<Utf8PathBuf>> {
    let cwd = env::current_dir()?;
    let cwd = Utf8PathBuf::try_from(cwd).map_err(|e| {
        eyre!(
            "Current directory is not valid UTF-8: {}",
            e.as_path().display()
        )
    })?;

    let mut current = cwd.as_path();

    loop {
        let config_dir = current.join(CONFIG_DIR);
        let config_file = config_dir.join(CONFIG_FILE);

        if config_file.exists() {
            return Ok(Some(config_file));
        }

        match current.parent() {
            Some(parent) => current = parent,
            None => return Ok(None),
        }
    }
}

/// Load and resolve configuration from a config file path
fn load_config(config_path: &Utf8Path) -> Result<ResolvedConfig> {
    let content = fs::read_to_string(config_path)?;

    let config: DodecaConfig =
        kdl::from_str(&content).map_err(|e| eyre!("Failed to parse {}: {}", config_path, e))?;

    // Project root is the parent of .config/
    let config_dir = config_path
        .parent()
        .ok_or_else(|| eyre!("Config file has no parent directory"))?;
    let root = config_dir
        .parent()
        .ok_or_else(|| eyre!(".config directory has no parent"))?
        .to_owned();

    // Resolve paths relative to project root
    let content_dir = root.join(&config.content.path);
    let output_dir = root.join(&config.output.path);

    // Extract skip domains
    let skip_domains = config
        .link_check
        .skip_domains
        .into_iter()
        .map(|s| s.domain)
        .collect();

    Ok(ResolvedConfig {
        _root: root,
        content_dir,
        output_dir,
        skip_domains,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config() {
        let kdl = r#"
            content "docs/"
            output "public/"
            link_check {
            }
        "#;

        let config: DodecaConfig = kdl::from_str(kdl).unwrap();
        assert_eq!(config.content.path, "docs/");
        assert_eq!(config.output.path, "public/");
    }
}
