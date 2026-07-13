//! Lowering contracts for validated Snark grammar facts.
//!
//! The intended runtime path is:
//!
//! 1. preserve/import Tree-sitter package inputs with provenance;
//! 2. validate Tree-sitter grammar semantics into Snark's typed symbol,
//!    production, field, scanner, query, recovery, and incremental facts;
//! 3. lower Snark's own facts into a Snark dialect carried by Weavy programs.
//!
//! Raw `grammar.json` DTOs and generated Tree-sitter implementation files are
//! not this layer.
pub mod weavy;
