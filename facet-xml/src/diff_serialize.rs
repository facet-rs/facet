//! Diff-aware XML serialization.
//!
//! This module provides XML rendering of diffs with `-`/`+` prefixes,
//! value-only coloring, and proper alignment.
//!
//! # Example
//!
//! ```ignore
//! use facet_diff::FacetDiff;
//!
//! let old = Rect { fill: "red".into(), x: 10, .. };
//! let new = Rect { fill: "blue".into(), x: 10, .. };
//! let diff = old.diff(&new);
//!
//! // Render as diff-aware XML
//! let xml = facet_xml::diff_to_string(&old, &new, &diff)?;
//! // Output:
//! // <rect
//! // ← fill="red"
//! // → fill="blue"
//! //   x="10" y="10" width="50" height="50"
//! // />
//! ```

use std::fmt::Write;

use facet_diff_core::{
    AnsiBackend, BuildOptions, Diff, PlainBackend, RenderOptions, XmlFlavor, build_layout, render,
    render_to_string,
};
pub use facet_diff_core::{DiffSymbols, DiffTheme};
use facet_reflect::Peek;

/// Options for diff-aware XML serialization.
#[derive(Debug, Clone)]
pub struct DiffSerializeOptions {
    /// Symbols to use for diff markers.
    pub symbols: DiffSymbols,

    /// Color theme for diff rendering.
    pub theme: DiffTheme,

    /// Whether to emit ANSI color codes.
    pub colors: bool,

    /// Indentation string (default: 2 spaces).
    pub indent: &'static str,

    /// Maximum line width for attribute grouping.
    pub max_line_width: usize,

    /// Maximum number of unchanged fields to show inline.
    pub max_unchanged_fields: usize,

    /// Collapse runs of unchanged siblings longer than this.
    pub collapse_threshold: usize,
}

impl Default for DiffSerializeOptions {
    fn default() -> Self {
        Self {
            symbols: DiffSymbols::default(),
            theme: DiffTheme::default(),
            colors: true,
            indent: "  ",
            max_line_width: 80,
            max_unchanged_fields: 5,
            collapse_threshold: 3,
        }
    }
}

impl DiffSerializeOptions {
    /// Create new default options (with ANSI colors).
    pub fn new() -> Self {
        Self::default()
    }

    /// Disable ANSI color output.
    pub fn no_colors(mut self) -> Self {
        self.colors = false;
        self
    }

    /// Set custom indentation string.
    pub fn indent(mut self, indent: &'static str) -> Self {
        self.indent = indent;
        self
    }

    /// Set maximum line width for attribute grouping.
    pub fn max_line_width(mut self, width: usize) -> Self {
        self.max_line_width = width;
        self
    }

    /// Set maximum number of unchanged fields to show inline.
    pub fn max_unchanged_fields(mut self, count: usize) -> Self {
        self.max_unchanged_fields = count;
        self
    }

    /// Set collapse threshold for unchanged runs.
    pub fn collapse_threshold(mut self, threshold: usize) -> Self {
        self.collapse_threshold = threshold;
        self
    }
}

/// Render a diff as XML to a String with ANSI colors.
///
/// # Arguments
///
/// * `from` - The original value
/// * `to` - The new value  
/// * `diff` - The diff between `from` and `to`
///
/// # Example
///
/// ```ignore
/// use facet_diff::FacetDiff;
///
/// let old = Point { x: 10, y: 20 };
/// let new = Point { x: 15, y: 20 };
/// let diff = old.diff(&new);
///
/// let xml = facet_xml::diff_to_string(&old, &new, &diff);
/// println!("{}", xml);
/// ```
pub fn diff_to_string<'mem, 'facet>(
    from: &'mem impl facet_core::Facet<'facet>,
    to: &'mem impl facet_core::Facet<'facet>,
    diff: &Diff<'mem, 'facet>,
) -> String {
    diff_to_string_with_options(from, to, diff, &DiffSerializeOptions::default())
}

/// Render a diff as XML to a String with custom options.
///
/// # Arguments
///
/// * `from` - The original value
/// * `to` - The new value
/// * `diff` - The diff between `from` and `to`
/// * `options` - Serialization options
pub fn diff_to_string_with_options<'mem, 'facet>(
    from: &'mem impl facet_core::Facet<'facet>,
    to: &'mem impl facet_core::Facet<'facet>,
    diff: &Diff<'mem, 'facet>,
    options: &DiffSerializeOptions,
) -> String {
    let from_peek = Peek::new(from);
    let to_peek = Peek::new(to);

    let build_opts = BuildOptions {
        max_line_width: options.max_line_width,
        max_unchanged_fields: options.max_unchanged_fields,
        collapse_threshold: options.collapse_threshold,
        float_precision: None,
    };

    let flavor = XmlFlavor;
    let layout = build_layout(diff, from_peek, to_peek, &build_opts, &flavor);

    if options.colors {
        let render_opts = RenderOptions {
            indent: options.indent,
            symbols: options.symbols.clone(),
            backend: AnsiBackend::new(options.theme.clone()),
        };
        render_to_string(&layout, &render_opts, &flavor)
    } else {
        let render_opts = RenderOptions {
            indent: options.indent,
            symbols: options.symbols.clone(),
            backend: PlainBackend,
        };
        render_to_string(&layout, &render_opts, &flavor)
    }
}

/// Render a diff as XML to a writer.
///
/// # Arguments
///
/// * `from` - The original value
/// * `to` - The new value
/// * `diff` - The diff between `from` and `to`
/// * `writer` - The output writer
pub fn diff_to_writer<'mem, 'facet, W: Write>(
    from: &'mem impl facet_core::Facet<'facet>,
    to: &'mem impl facet_core::Facet<'facet>,
    diff: &Diff<'mem, 'facet>,
    writer: &mut W,
) -> std::fmt::Result {
    diff_to_writer_with_options(from, to, diff, writer, &DiffSerializeOptions::default())
}

/// Render a diff as XML to a writer with custom options.
///
/// # Arguments
///
/// * `from` - The original value
/// * `to` - The new value
/// * `diff` - The diff between `from` and `to`
/// * `writer` - The output writer
/// * `options` - Serialization options
pub fn diff_to_writer_with_options<'mem, 'facet, W: Write>(
    from: &'mem impl facet_core::Facet<'facet>,
    to: &'mem impl facet_core::Facet<'facet>,
    diff: &Diff<'mem, 'facet>,
    writer: &mut W,
    options: &DiffSerializeOptions,
) -> std::fmt::Result {
    let from_peek = Peek::new(from);
    let to_peek = Peek::new(to);

    let build_opts = BuildOptions {
        max_line_width: options.max_line_width,
        max_unchanged_fields: options.max_unchanged_fields,
        collapse_threshold: options.collapse_threshold,
        float_precision: None,
    };

    let flavor = XmlFlavor;
    let layout = build_layout(diff, from_peek, to_peek, &build_opts, &flavor);

    if options.colors {
        let render_opts = RenderOptions {
            indent: options.indent,
            symbols: options.symbols.clone(),
            backend: AnsiBackend::new(options.theme.clone()),
        };
        render(&layout, writer, &render_opts, &flavor)
    } else {
        let render_opts = RenderOptions {
            indent: options.indent,
            symbols: options.symbols.clone(),
            backend: PlainBackend,
        };
        render(&layout, writer, &render_opts, &flavor)
    }
}
