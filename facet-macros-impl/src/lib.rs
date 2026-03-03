#![warn(missing_docs)]
#![allow(uncommon_codepoints)]
//! # facet-macros-impl
//!
//! [![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-macros-impl/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
//! [![crates.io](https://img.shields.io/crates/v/facet-macros-impl.svg)](https://crates.io/crates/facet-macros-impl)
//! [![documentation](https://docs.rs/facet-macros-impl/badge.svg)](https://docs.rs/facet-macros-impl)
//! [![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-macros-impl.svg)](./LICENSE)
//! [![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)
//!
//! Implementation of facet derive macros, combining parsing and code generation.
//!
//! This crate provides the internal implementation for `#[derive(Facet)]` and related procedural macros. It's used by `facet-macros` (the proc-macro crate) and should not be used directly.
//!
//! ## Sponsors
//!
//! Thanks to all individual sponsors:
//!
//! <p> <a href="https://github.com/sponsors/fasterthanlime">
//! <picture>
//! <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/github-dark.svg">
//! <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/github-light.svg" height="40" alt="GitHub Sponsors">
//! </picture>
//! </a> <a href="https://patreon.com/fasterthanlime">
//!     <picture>
//!     <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/patreon-dark.svg">
//!     <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/patreon-light.svg" height="40" alt="Patreon">
//!     </picture>
//! </a> </p>
//!
//! ...along with corporate sponsors:
//!
//! <p> <a href="https://aws.amazon.com">
//! <picture>
//! <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/aws-dark.svg">
//! <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/aws-light.svg" height="40" alt="AWS">
//! </picture>
//! </a> <a href="https://zed.dev">
//! <picture>
//! <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/zed-dark.svg">
//! <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/zed-light.svg" height="40" alt="Zed">
//! </picture>
//! </a> <a href="https://depot.dev?utm_source=facet">
//! <picture>
//! <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/depot-dark.svg">
//! <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/depot-light.svg" height="40" alt="Depot">
//! </picture>
//! </a> </p>
//!
//! ...without whom this work could not exist.
//!
//! ## Special thanks
//!
//! The facet logo was drawn by [Misiasart](https://misiasart.com/).
//!
//! ## License
//!
//! Licensed under either of:
//!
//! - Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
//! - MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
//!
//! at your option.

// ============================================================================
// RE-EXPORTS FROM FACET-MACRO-TYPES (grammar) AND FACET-MACRO-PARSE (parsed types)
// ============================================================================

// Re-export everything from facet-macro-types (includes unsynn, grammar, RenameRule)
pub use facet_macro_types::*;

// Re-export everything from facet-macro-parse (includes parsed types like PStruct, PEnum, etc.)
pub use facet_macro_parse::*;

// ============================================================================
// FUNCTION PARSING (optional feature)
// ============================================================================

/// Parse function signature shape
#[cfg(feature = "function")]
pub mod function;

// ============================================================================
// CODE EMISSION
// ============================================================================

mod process_enum;
mod process_struct;

// ============================================================================
// DOC STRIPPING DETECTION
// ============================================================================

/// Returns true if doc strings should be stripped from generated shapes.
///
/// Controlled by `--cfg facet_no_doc`. Set via rustflags in .cargo/config.toml,
/// Cargo.toml profile, or RUSTFLAGS env var. The cfg is evaluated when the
/// proc-macro is compiled.
#[cfg(all(feature = "doc", facet_no_doc))]
pub const fn is_no_doc() -> bool {
    true
}

/// Returns true if doc strings should be stripped from generated shapes.
#[cfg(all(feature = "doc", not(facet_no_doc)))]
pub const fn is_no_doc() -> bool {
    false
}

mod derive;
pub use derive::*;

mod plugin;
pub use plugin::*;

mod extension;
pub use extension::*;

mod on_error;
pub use on_error::*;

/// Attribute grammar infrastructure for extension crates
pub mod attr_grammar;

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn test_struct_with_field_doc_comments() {
        let input = quote! {
            #[derive(Facet)]
            pub struct User {
                #[doc = " The user's unique identifier"]
                pub id: u64,
            }
        };

        let mut it = input.to_token_iter();
        let parsed = it.parse::<Struct>().expect("Failed to parse struct");

        // Check that we parsed the struct correctly
        assert_eq!(parsed.name.to_string(), "User");

        // Extract fields from the struct
        if let StructKind::Struct { fields, .. } = &parsed.kind {
            assert_eq!(fields.content.len(), 1);

            // Check first field (id)
            let id_field = &fields.content[0].value;
            assert_eq!(id_field.name.to_string(), "id");

            // Extract doc comments from id field
            let mut doc_found = false;
            for attr in &id_field.attributes {
                match &attr.body.content {
                    AttributeInner::Doc(doc_inner) => {
                        // This should work with LiteralString
                        assert_eq!(doc_inner.value, " The user's unique identifier");
                        doc_found = true;
                    }
                    _ => {
                        // Skip non-doc attributes
                    }
                }
            }
            assert!(doc_found, "Should have found a doc comment");
        } else {
            panic!("Expected a regular struct with named fields");
        }
    }
}
