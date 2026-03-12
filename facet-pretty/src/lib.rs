#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
//!
//! [![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-pretty/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
//! [![crates.io](https://img.shields.io/crates/v/facet-pretty.svg)](https://crates.io/crates/facet-pretty)
//! [![documentation](https://docs.rs/facet-pretty/badge.svg)](https://docs.rs/facet-pretty)
//! [![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-pretty.svg)](./LICENSE)
//! [![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)
//!
//! Provides pretty-printing capabilities for Facet types.
//!
//! Example:
//!
//! ```rust
//! use facet::Facet;
//! use facet_pretty::FacetPretty;
//!
//! #[derive(Debug, Facet)]
//! struct Person {
//!     name: String,
//!     age: u32,
//! }
//!
//! let person = Person {
//!     name: "Alice".to_string(),
//!     age: 30,
//! };
//! println!("Default pretty-printing:");
//! println!("{}", person.pretty());
//! ```
//!
//! Produces the output:
//!
//! ```text
//! Person {
//!   name: "Alice",
//!   age: 30,
//! }
//! ```
#![doc = include_str!("../README-footer.md")]

extern crate alloc;

mod color;
mod display;
mod printer;
mod shape;

pub use color::*;
pub use display::*;
pub use printer::*;
pub use shape::*;
