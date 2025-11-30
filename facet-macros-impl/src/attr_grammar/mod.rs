//! Attribute grammar system for declarative extension attribute parsing.
//!
//! This module provides the infrastructure for extension crates to define
//! type-safe, self-documenting attribute grammars using `define_attr_grammar!`.
//!
//! # Architecture
//!
//! The system consists of several components:
//!
//! - **Grammar Compiler** (`make_parse_attr`): Parses the grammar DSL and generates
//!   type definitions and dispatcher macros.
//!
//! - **Unified Dispatcher** (`dispatch_attr`): Routes parsed attribute names to
//!   the appropriate variant handlers (unit, newtype, or struct).
//!
//! - **Struct Field Builder** (`build_struct_fields`): Parses struct field
//!   assignments with type validation and helpful error messages.
//!
//! - **Error Handling** (`attr_error`, `field_error`, `spanned_error`): Generates
//!   compile-time errors with typo suggestions and span preservation.

mod attr_error;
mod build_struct_fields;
mod dispatch_attr;
mod field_error;
mod make_parse_attr;
mod spanned_error;

pub use attr_error::attr_error;
pub use build_struct_fields::build_struct_fields;
pub use dispatch_attr::dispatch_attr;
pub use field_error::field_error;
pub use make_parse_attr::make_parse_attr;
pub use spanned_error::spanned_error;
