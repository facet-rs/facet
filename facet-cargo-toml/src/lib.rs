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
use facet::Facet;

/// Errors that can occur when parsing `Cargo.toml` or `Cargo.lock` files.
#[derive(Debug, Facet)]
#[facet(derive(Error))]
#[repr(u8)]
#[non_exhaustive]
pub enum Error {
    /// failed to read {path}: {source}
    Io { path: Utf8PathBuf, source: IoError },

    /// parse error: {message}
    Parse { message: String },
}

/// Wrapper for `std::io::Error` that implements `Facet`.
#[derive(Debug, Facet)]
#[repr(transparent)]
pub struct IoError(#[facet(opaque)] std::io::Error);

impl From<std::io::Error> for IoError {
    fn from(e: std::io::Error) -> Self {
        IoError(e)
    }
}

impl std::fmt::Display for IoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
