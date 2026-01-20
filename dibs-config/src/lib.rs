//! Facet types for the dibs configuration schema.
//!
//! These types define the structure of `dibs.styx` config files and can be:
//! - Deserialized from styx using facet-styx
//! - Used to generate a styx schema via facet-styx's schema generation

use facet::Facet;

/// Configuration loaded from `dibs.styx`.
#[derive(Debug, Clone, Facet)]
pub struct Config {
    /// Database crate configuration.
    #[facet(default)]
    pub db: DbConfig,
}

/// Database crate configuration.
#[derive(Debug, Clone, Facet, Default)]
pub struct DbConfig {
    /// Name of the crate containing schema definitions (e.g., "my-app-db").
    #[facet(rename = "crate")]
    pub crate_name: Option<String>,

    /// Path to a pre-built binary (for faster iteration).
    /// If not specified, we'll use `cargo run -p <crate_name>`.
    pub binary: Option<String>,
}
