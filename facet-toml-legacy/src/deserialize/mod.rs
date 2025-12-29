//! Parse TOML strings into Rust values.

#[cfg(not(feature = "alloc"))]
compile_error!("feature `alloc` is required");

mod error;
mod streaming;

pub use error::{TomlDeError, TomlDeErrorKind};
pub use streaming::from_str;
