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
//! let xml = facet_xml_legacy::diff_to_string(&diff)?;
//! // Output:
//! // <rect
//! // - fill="red"
//! // + fill="blue"
//! //   x="10" y="10" width="50" height="50"
//! // />
//! ```

pub use facet_diff_core::{ChangeKind, DiffSymbols, DiffTheme};

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
    pub indent: String,

    /// Maximum line width before wrapping attribute groups.
    pub max_line_width: usize,

    /// Number of unchanged siblings to show as context around changes.
    pub context: usize,

    /// Collapse runs of unchanged siblings longer than this.
    pub collapse_threshold: usize,
}

impl Default for DiffSerializeOptions {
    fn default() -> Self {
        Self {
            symbols: DiffSymbols::default(),
            theme: DiffTheme::default(),
            colors: true,
            indent: "  ".to_string(),
            max_line_width: 80,
            context: 2,
            collapse_threshold: 3,
        }
    }
}

// TODO: Implement diff_to_string and diff_to_writer
//
// The implementation needs to:
// 1. Take a Diff<'mem, 'facet> reference
// 2. Walk the diff structure
// 3. Emit XML with appropriate -/+/←/→ prefixes
// 4. Color only VALUES, not keys
// 5. Align multiple changed attributes
// 6. Collapse unchanged runs
