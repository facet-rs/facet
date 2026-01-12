//! Event-based parser for the Styx configuration language.
//!
//! This crate provides a low-level lexer and event-based parser for Styx documents.
//! It's designed to be used by higher-level tools like `styx-tree` (document tree)
//! and `facet-styx` (serde-like deserialization).

mod lexer;
mod span;
mod token;

pub use lexer::Lexer;
pub use span::Span;
pub use token::{Token, TokenKind};
