//! Output mode detection and configuration.

use std::env;

/// Output mode for showcase rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputMode {
    /// Terminal output with ANSI colors
    #[default]
    Terminal,
    /// Markdown output with embedded HTML for Zola
    ///
    /// Headings are emitted as Markdown so Zola can build a table of contents.
    /// Content blocks (code, errors, etc.) are emitted as HTML.
    Markdown,
}

impl OutputMode {
    /// Detect output mode from environment variable `FACET_SHOWCASE_OUTPUT`.
    ///
    /// Values:
    /// - `markdown` or `MARKDOWN` → OutputMode::Markdown
    /// - anything else (or unset) → OutputMode::Terminal
    pub fn from_env() -> Self {
        match env::var("FACET_SHOWCASE_OUTPUT").as_deref() {
            Ok("markdown") | Ok("MARKDOWN") => OutputMode::Markdown,
            _ => OutputMode::Terminal,
        }
    }

    /// Check if this is terminal output mode.
    pub fn is_terminal(self) -> bool {
        self == OutputMode::Terminal
    }

    /// Check if this is Markdown output mode.
    pub fn is_markdown(self) -> bool {
        self == OutputMode::Markdown
    }
}
