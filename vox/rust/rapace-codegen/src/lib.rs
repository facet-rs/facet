#![deny(unsafe_code)]

//! Code generation for non-Rust bindings (TypeScript, Swift, Go, Java).
//!
//! Input is a list of `rapace_schema::MethodDetail` values, which are plain Rust structs
//! matching the Rust spec (`docs/content/rust-spec/_index.md`). This is deliberately not an
//! intermediate schema language.

mod render;
pub mod targets;

use rapace_schema::MethodDetail;

pub fn method_id(detail: &MethodDetail) -> u64 {
    rapace_hash::method_id_from_detail(detail)
}
