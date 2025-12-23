//! Type-safe `Cargo.toml` and `Cargo.lock` parser using [facet](https://github.com/facet-rs/facet).
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use facet_cargo_toml::{CargoToml, CargoLock};
//!
//! // Parse a Cargo.toml
//! let manifest = CargoToml::from_path("Cargo.toml")?;
//! if let Some(pkg) = &manifest.package {
//!     println!("Package: {:?}", pkg.name);
//! }
//!
//! // Parse a Cargo.lock
//! let lockfile = CargoLock::from_path("Cargo.lock")?;
//! println!("Contains {} packages", lockfile.packages.len());
//! # Ok::<_, facet_cargo_toml::Error>(())
//! ```

mod lockfile;
mod manifest;

pub use lockfile::{CRATES_IO_SOURCE, CargoLock, LockPackage};
pub use manifest::*;

use camino::Utf8PathBuf;
use thiserror::Error;

/// Errors that can occur when parsing `Cargo.toml` or `Cargo.lock` files.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// Failed to read a file from disk.
    #[error("failed to read {path}")]
    Io {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Failed to parse TOML content.
    #[error("parse error: {message}")]
    Parse { message: String },
}
