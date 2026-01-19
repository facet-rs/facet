//! Bundled meta-schema.
//!
//! The meta-schema is the schema that describes STYX schema files themselves.
//! It's bundled as a static string and deserialized at runtime to validate
//! that facet-styx can handle our own schema format.

/// The STYX meta-schema source.
pub const META_SCHEMA_SOURCE: &str = include_str!("../schema/meta.styx");
