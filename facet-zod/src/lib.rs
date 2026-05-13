#![warn(missing_docs)]
//!
//! [![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-zod/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
//! [![crates.io](https://img.shields.io/crates/v/facet-zod.svg)](https://crates.io/crates/facet-zod)
//! [![documentation](https://docs.rs/facet-zod/badge.svg)](https://docs.rs/facet-zod)
//! [![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-zod.svg)](./LICENSE)
//! [![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)
//!
//! Generate [Zod](https://zod.dev) TypeScript schemas from Rust types via Facet
//! reflection.
//!
//! # Example
//!
//! ```
//! use facet::Facet;
//! use facet_zod::generate;
//!
//! #[derive(Facet)]
//! struct User {
//!     name: String,
//!     age: u32,
//!     email: Option<String>,
//! }
//!
//! let schema = generate::<User>();
//! assert!(schema.contains("export const UserSchema = z.object({"));
//! ```
//!
#![doc = include_str!("../readme-footer.md")]

/// Generator configuration: optional-field mapping, integer width handling,
/// export style, and optional file header.
pub mod config;
/// Emit Zod source text from the intermediate [`mapping::ZodType`] tree.
pub mod emit;
/// Top-level [`ZodGenerator`] that walks roots, deduplicates types, and emits schemas.
pub mod generator;
/// Mapping from Facet `Shape`s to the intermediate `ZodType` representation.
pub mod mapping;

pub use config::Config;
pub use generator::ZodGenerator;

use facet_core::Facet;

/// Generate a Zod schema string for `T` using default [`Config`].
pub fn generate<'facet, T: Facet<'facet>>() -> String {
    let mut generator = ZodGenerator::new();
    generator.add::<T>();
    generator.emit()
}

/// Generate a Zod schema string for `T` using the provided [`Config`].
pub fn generate_with_config<'facet, T: Facet<'facet>>(config: Config) -> String {
    let mut generator = ZodGenerator::with_config(config);
    generator.add::<T>();
    generator.emit()
}
