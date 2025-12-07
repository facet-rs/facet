//! Parser for facet derive macros
//!
//! This crate provides parsing infrastructure that takes a `TokenStream` (from a struct or enum
//! definition) and returns parsed type representations (`PStruct`, `PEnum`, etc.) from
//! `facet-macro-types`.
//!
//! The parsing is done using `unsynn`, a lightweight proc-macro parsing library.

pub use facet_macro_types::*;
pub use unsynn::*;

mod grammar;
pub use grammar::*;

mod convert;
pub use convert::*;
