//! Typed Cargo.toml and Cargo.lock parser using facet
//!
//! This crate provides complete, type-safe parsing of Cargo manifest and lockfile formats:
//!
//! - **[`CargoManifest`]**: Full Cargo.toml parsing with all fields
//! - **[`Lockfile`]**: Cargo.lock parsing with dependency resolution
//!
//! ## Example
//!
//! ```rust,no_run
//! use facet_cargo_toml::CargoManifest;
//!
//! let manifest = CargoManifest::from_path("Cargo.toml")?;
//! if let Some(package) = &manifest.package {
//!     println!("Package: {:?}", package.name);
//! }
//! # Ok::<_, Box<dyn std::error::Error>>(())
//! ```

pub mod full;
pub mod lockfile;

// Re-export main types
pub use full::CargoManifest;
pub use lockfile::Lockfile;
