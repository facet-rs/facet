#![warn(missing_docs)]
#![allow(uncommon_codepoints)]
#![doc = include_str!("../README.md")]

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
