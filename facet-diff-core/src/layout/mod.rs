//! Layout types and algorithms for diff rendering.
//!
//! This module provides the infrastructure for formatting diffs with proper
//! alignment, coloring, and collapsing of unchanged runs.
//!
//! # Architecture
//!
//! 1. **Format phase**: Walk the Diff, format all scalar values into [`FormatArena`]
//! 2. **Layout phase**: Build [`LayoutNode`] tree, group attrs, calculate alignment
//! 3. **Render phase**: Walk tree, emit to writer with prefixes/colors/padding

mod arena;
mod attrs;
mod backend;
mod build;
mod node;
mod render;

pub use arena::{FormatArena, Span};
pub use attrs::{Attr, AttrStatus, ChangedGroup, FormattedValue, group_changed_attrs};
pub use backend::{AnsiBackend, ColorBackend, PlainBackend, SemanticColor};
pub use build::{BuildOptions, build_layout};
pub use node::{ElementChange, Layout, LayoutNode};
pub use render::{RenderOptions, render, render_to_string};
