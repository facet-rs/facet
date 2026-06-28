// Template engine is a work in progress - many AST fields are for future error reporting
#![allow(dead_code)]

//! gingembre - A Jinja-like template engine
//!
//! A template language featuring:
//! - Rich diagnostics
//! - Parse once, run many times (compiled templates)
//! - Template inheritance and includes
//! - Macro system with imports
//!
//! # Syntax Overview
//!
//! ```text
//! {{ expr }}              - Expression interpolation
//! {% if cond %}...{% endif %}     - Conditionals
//! {% for item in items %}...{% endfor %}  - Loops
//! {{ value | filter }}    - Filters
//! {{ object.field }}      - Field access
//! {% extends "base.html" %}       - Template inheritance
//! {% block name %}...{% endblock %} - Block definitions
//! {% include "partial.html" %}    - Template includes
//! {% macro name(args) %}...{% endmacro %} - Macro definitions
//! ```
//!
//! # Example
//!
//! ```ignore
//! use gingembre::{Engine, Context, Value, InMemoryLoader};
//!
//! let mut loader = InMemoryLoader::new();
//! loader.add("hello.html", "Hello, {{ name }}!");
//!
//! let engine = Engine::new(loader);
//! let mut ctx = Context::new();
//! ctx.set("name", Value::String("World".into()));
//!
//! let output = engine.render("hello.html", &ctx)?;
//! assert_eq!(output, "Hello, World!");
//! ```

pub mod ast;
mod cst_lower;
pub use cst_lower::{parse_template, parse_template_recovered};
mod error;
mod eval;
mod lazy;
mod render;
pub mod semantic;

pub use error::{
    NamedSource, PrettyError, RenderError, SourceLocation, SourceSpan, TemplateError,
    format_template_error_pretty,
};
pub use eval::{
    BUILTIN_FILTERS, BUILTIN_TESTS, BuiltinItemInfo, Context, GlobalFn, Value, ValueExt,
    builtin_filter, builtin_filter_names, builtin_test, builtin_test_names,
};
pub use lazy::{DataPath, DataResolver, LazyValue};
pub use render::{Engine, InMemoryLoader, TemplateLoader};

// Re-export facet_value types for convenience
pub use facet_value::{VArray, VObject, VSafeString, VString};

/// Evaluate a standalone expression string against a context.
/// Useful for REPL-style evaluation.
pub async fn eval_expression(expr: &str, ctx: &Context) -> Result<Value, TemplateError> {
    use error::TemplateSource;

    let source = TemplateSource::new("<repl>", expr);
    let ast = cst_lower::parse_expression(expr)?;
    let evaluator = eval::Evaluator::new(ctx, &source);
    evaluator.eval_concrete(&ast).await
}
