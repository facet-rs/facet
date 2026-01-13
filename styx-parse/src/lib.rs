//! Event-based parser for the Styx configuration language.
//!
//! This crate provides a low-level lexer and event-based parser for Styx documents.
//! It's designed to be used by higher-level tools like `styx-tree` (document tree)
//! and `facet-styx` (serde-like deserialization).

// Conditional tracing macros
#[cfg(feature = "tracing")]
macro_rules! trace {
    ($($arg:tt)*) => { ::tracing::trace!($($arg)*) };
}

#[cfg(not(feature = "tracing"))]
macro_rules! trace {
    ($($arg:tt)*) => {};
}

#[allow(unused_imports)]
pub(crate) use trace;

#[cfg(feature = "tracing")]
macro_rules! debug {
    ($($arg:tt)*) => { ::tracing::debug!($($arg)*) };
}

#[cfg(not(feature = "tracing"))]
#[allow(unused_macros)]
macro_rules! debug {
    ($($arg:tt)*) => {};
}

#[allow(unused_imports)]
#[allow(unused_macros)]
pub(crate) use debug;

pub mod callback;
pub mod event;
mod lexer;
pub mod parser;
mod span;
mod token;

pub use callback::ParseCallback;
pub use event::{Event, ParseErrorKind, ScalarKind, Separator};
pub use lexer::Lexer;
pub use parser::Parser;
pub use span::Span;
pub use token::{Token, TokenKind};
