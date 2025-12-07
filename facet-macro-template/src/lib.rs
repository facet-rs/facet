//! # Facet Macro Template
//!
//! A token-based templating engine for facet macro code generation.
//!
//! ## Syntax
//!
//! - `#ident` — interpolate a simple variable
//! - `#(expr)` — interpolate a complex expression (e.g., `#(variant.fields[0].ty)`)
//! - `@for ident in collection { ... }` — loop
//! - `@if condition { ... }` — conditional
//! - `@if condition { ... } @else { ... }` — conditional with else
//! - Everything else — literal Rust tokens to emit

mod ast;
mod eval;
mod parse;
mod value;

pub use ast::{ForLoop, IfBlock, Template, TemplateItem};
pub use eval::EvalContext;
pub use value::Value;

// Re-export types from facet-macro-parse for convenience
pub use facet_macro_parse::{
    PAttrs, PEnum, PName, PRepr, PStruct, PStructField, PStructKind, PVariant, PVariantKind,
};
