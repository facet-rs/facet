//! Lossless Concrete Syntax Tree for the Styx configuration language.
//!
//! This crate provides a CST (Concrete Syntax Tree) representation of Styx documents
//! using the [rowan](https://docs.rs/rowan) library. Unlike an AST, the CST preserves
//! all source information including whitespace, comments, and exact token positions,
//! making it ideal for tooling like formatters, refactoring tools, and language servers.
//!
//! # Features
//!
//! - **Lossless representation**: Source text can be exactly reconstructed from the CST
//! - **Cheap cloning**: Syntax nodes use reference counting internally
//! - **Parent pointers**: Navigate up and down the tree
//! - **Typed AST layer**: Ergonomic wrappers over raw CST nodes
//! - **Semantic validation**: Check for issues like duplicate keys and mixed separators
//!
//! # Example
//!
//! ```
//! use styx_cst::{parse, ast::{AstNode, Document}};
//!
//! let source = r#"
//! host localhost
//! port 8080
//! "#;
//!
//! let parsed = parse(source);
//! assert!(parsed.is_ok());
//!
//! let doc = Document::cast(parsed.syntax()).unwrap();
//! for entry in doc.entries() {
//!     if let Some(key) = entry.key_text() {
//!         println!("Found key: {}", key);
//!     }
//! }
//!
//! // Roundtrip: source can be exactly reconstructed
//! assert_eq!(parsed.syntax().to_string(), source);
//! ```
//!
//! # Validation
//!
//! ```
//! use styx_cst::{parse, validation::validate};
//!
//! let source = "{ a 1, a 2 }"; // Duplicate key
//! let parsed = parse(source);
//! let diagnostics = validate(&parsed.syntax());
//!
//! assert!(!diagnostics.is_empty());
//! ```

pub mod ast;
pub mod parser;
pub mod syntax_kind;
pub mod validation;

pub use parser::{Parse, ParseError, parse};
pub use syntax_kind::{StyxLanguage, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};
pub use validation::{Diagnostic, Severity, validate};

// Re-export rowan types for convenience
pub use rowan::{TextRange, TextSize};
