// Template engine is a work in progress - many AST fields are for future error reporting
#![allow(dead_code)]

//! Template engine for dodeca
//!
//! A Jinja-like template language with:
//! - Facet as the data model (zero serde)
//! - Rich diagnostics via miette
//! - Parse once, run many times (compiled templates)
//!
//! # Syntax Overview
//!
//! ```text
//! {{ expr }}              - Expression interpolation
//! {% if cond %}...{% endif %}     - Conditionals
//! {% for item in items %}...{% endfor %}  - Loops
//! {{ value | filter }}    - Filters
//! {{ object.field }}      - Field access
//! ```
//!
//! # Example
//!
//! ```ignore
//! use dodeca::template::Template;
//!
//! let source = "Hello, {{ name }}!";
//! let template = Template::parse(source)?;
//!
//! // Render with any Facet type as context
//! let output = template.render(&context)?;
//! ```

mod ast;
mod error;
mod eval;
mod lexer;
mod parser;
mod render;

pub use eval::{Context, Value};
pub use render::{Engine, InMemoryLoader};
