//! Lowering contracts for Tree-sitter artifacts.
//!
//! The intended runtime path is:
//!
//! 1. preserve/import Tree-sitter artifacts with provenance;
//! 2. validate them into typed symbol, production, field, scanner, and parse
//!    table facts;
//! 3. lower those facts into a Snark dialect carried by Weavy programs.
//!
//! Raw `grammar.json` DTOs and milestone parsers are not this layer.

#[cfg(feature = "weavy-lowering")]
pub mod weavy;
