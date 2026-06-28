//! Non-foundational proof modules.
//!
//! Milestone modules are allowed to be deliberately smaller than the real
//! Tree-sitter compatibility contract. They must not define Snark's runtime
//! semantics, and they are not intended as stable crate API. Production parsing
//! is expected to go through validated lowering into Snark's Weavy dialect.

#[doc(hidden)]
pub mod scannerless;
