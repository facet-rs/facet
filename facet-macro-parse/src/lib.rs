//! # Facet Macro Parse
//!
//! Parsed type representations for the facet macro ecosystem.
//!
//! This crate provides the parsed types (`PStruct`, `PEnum`, `PContainer`, `PAttrs`, etc.)
//! that represent the result of parsing Rust type definitions for code generation.
//!
//! It depends on `facet-macro-types` for the underlying unsynn grammar.

#![allow(uncommon_codepoints)]

// Re-export everything from facet-macro-types for convenience
pub use facet_macro_types::*;

mod parsed;
pub use parsed::*;

mod generic_params;
pub use generic_params::*;

mod unescaping;
pub use unescaping::*;

// ============================================================================
// CONVENIENCE PARSING FUNCTIONS
// ============================================================================

/// Represents a parsed type (either a struct or an enum)
pub enum PType {
    /// A parsed struct
    Struct(PStruct),
    /// A parsed enum
    Enum(PEnum),
}

impl PType {
    /// Get the name identifier of the type
    pub const fn name(&self) -> &Ident {
        match self {
            PType::Struct(s) => &s.container.name,
            PType::Enum(e) => &e.container.name,
        }
    }
}

/// Parse a TokenStream into a `PType` (either struct or enum).
///
/// This is a convenience function that tries to parse the token stream
/// as either a struct or an enum.
pub fn parse_type(tokens: TokenStream) -> std::result::Result<PType, String> {
    let mut iter = tokens.to_token_iter();

    // Try to parse as AdtDecl which can be either Struct or Enum
    match iter.parse::<AdtDecl>() {
        Ok(AdtDecl::Struct(s)) => Ok(PType::Struct(PStruct::parse(&s))),
        Ok(AdtDecl::Enum(e)) => Ok(PType::Enum(PEnum::parse(&e))),
        Err(e) => Err(format!("failed to parse type: {e}")),
    }
}
