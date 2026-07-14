//! Facet types for the dibs configuration schema.
//!
//! These types define the structure of `dibs.styx` config files and can be:
//! - Deserialized from styx using facet-styx
//! - Used to generate a styx schema via facet-styx's schema generation

use facet::Facet;

/// Configuration loaded from `dibs.styx`.
#[derive(Debug, Clone, Default, Facet)]
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

    /// Address of the application's explicitly enabled Dibs tooling endpoint.
    ///
    /// The Dibs CLI never discovers or launches an application binary. The
    /// application owns the schema inventory and chooses when to expose this
    /// endpoint, normally through a development-only process mode.
    pub endpoint: Option<String>,
}
