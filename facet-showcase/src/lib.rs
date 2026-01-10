//! Unified showcase infrastructure for facet format crates.
//!
//! This crate provides consistent rendering for showcases across all facet
//! format crates (JSON, YAML, TOML, etc.), supporting both terminal output
//! and HTML generation for the website.
//!
//! # Example
//!
//! ```ignore
//! use facet_showcase::{ShowcaseRunner, Language};
//!
//! let mut runner = ShowcaseRunner::new("JSON Error Showcase");
//!
//! runner.scenario("Unknown Field")
//!     .description("JSON contains a field that doesn't exist in the target struct.")
//!     .input(Language::Json, r#"{"username": "alice", "emial": "test@example.com"}"#)
//!     .target_type::<User>()
//!     .error(&err)
//!     .finish();
//!
//! runner.finish();
//! ```

mod highlighter;
mod output;
mod runner;

pub use highlighter::{Highlighter, Language, ansi_to_html, html_escape};
pub use output::OutputMode;
pub use runner::{Provenance, Scenario, ShowcaseRunner};
