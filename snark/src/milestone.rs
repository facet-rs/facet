//! Non-foundational proof artifacts.
//!
//! Milestone modules are allowed to be deliberately smaller than the real
//! Tree-sitter compatibility contract. They must not define Snark's runtime
//! semantics. Production parsing is expected to go through validated lowering
//! into Snark's Weavy dialect.

pub mod scannerless;
