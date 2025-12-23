//! TOML serialization for facet using the new format architecture.
//!
//! This is the successor to `facet-toml`, using the unified `facet-format` traits.
//!
//! # Deserialization
//!
//! ```
//! use facet::Facet;
//! use facet_format_toml::from_str;
//!
//! #[derive(Facet, Debug)]
//! struct Config {
//!     name: String,
//!     port: u16,
//! }
//!
//! let toml = r#"
//! name = "my-app"
//! port = 8080
//! "#;
//!
//! let config: Config = from_str(toml).unwrap();
//! assert_eq!(config.name, "my-app");
//! assert_eq!(config.port, 8080);
//! ```

extern crate alloc;

mod error;
mod parser;
mod serializer;

pub use error::{TomlError, TomlErrorKind};
pub use parser::{TomlParser, TomlProbe, from_str};
pub use serializer::{SerializeOptions, TomlSerializeError, TomlSerializer, to_string, to_vec};
