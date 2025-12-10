//! Core types and helpers for diff rendering.
//!
//! This crate provides shared infrastructure for rendering diffs across
//! different serialization formats (XML, JSON, TOML, etc.).
//!
//! # Symbols
//!
//! ```text
//! -  deleted (red)
//! +  inserted (green)
//! ←  moved from here (blue)
//! →  moved to here (blue)
//! ```
//!
//! # Value-only coloring
//!
//! Keys/field names stay neutral, only the changed VALUES are colored:
//!
//! ```text
//! - fill="red"    ← "red" is red, "fill=" is white
//! + fill="blue"   ← "blue" is green, "fill=" is white
//! ```

mod display;
pub mod layout;
mod path;
mod sequences;
mod symbols;
mod theme;
mod types;

pub use layout::{
    AnsiBackend, Attr, AttrStatus, BuildOptions, ChangedGroup, ColorBackend, DiffFlavor,
    ElementChange, FieldPresentation, FormatArena, FormattedValue, JsonFlavor, Layout, LayoutNode,
    PlainBackend, RenderOptions, RustFlavor, SemanticColor, Span, XmlFlavor, build_layout,
    group_changed_attrs, render, render_to_string,
};
pub use path::*;
pub use sequences::*;
pub use symbols::*;
pub use theme::*;
pub use types::*;
