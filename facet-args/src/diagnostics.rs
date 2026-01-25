//! Shared diagnostic types for rich, late-formatted error reporting.

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;

/// Severity of a diagnostic.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Severity {
    Error,
    Warning,
    Help,
    Info,
}

/// Identifies a source to render labels against.
///
/// This is intentionally lightweight and owned so diagnostics can carry their
/// own sources without external providers.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum SourceId {
    /// A rendered schema/shape definition.
    Schema,
    /// Flattened CLI arguments (or other CLI input).
    Cli,
    /// A config file by path.
    ConfigFile(String),
    /// Catch-all for other sources.
    Other(String),
}

/// A source of text to attach labels to.
#[derive(Debug, Clone)]
pub struct SourceBundle {
    /// Identifier used by labels to select this source.
    pub id: SourceId,
    /// Optional display name (e.g., file path or "schema definition").
    pub name: Option<Cow<'static, str>>,
    /// The full source text.
    pub text: Cow<'static, str>,
}

/// A diagnostic label that references a span within a source.
#[derive(Debug, Clone)]
pub struct LabelSpec {
    /// Which source this label targets.
    pub source: SourceId,
    /// Byte span into the source text.
    pub span: Range<usize>,
    /// Message for this label.
    pub message: Cow<'static, str>,
    /// Whether this is the primary label.
    pub is_primary: bool,
    /// Optional color hint (renderer-specific).
    pub color: Option<ColorHint>,
}

/// Color hint for renderers that support it.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ColorHint {
    Red,
    Yellow,
    Blue,
    Cyan,
    Green,
}

/// Common diagnostic interface for errors and warnings.
///
/// Implementations should retain all context necessary to render later.
/// Sources are carried by the diagnostic itself.
pub trait Diagnostic {
    /// Stable error code, if any.
    fn code(&self) -> &'static str;

    /// Short primary message.
    fn label(&self) -> &'static str;

    /// Optional help text.
    fn help(&self) -> Option<Cow<'static, str>> {
        None
    }

    /// Optional notes to append.
    fn notes(&self) -> Vec<Cow<'static, str>> {
        Vec::new()
    }

    /// Severity of this diagnostic.
    fn severity(&self) -> Severity {
        Severity::Error
    }

    /// Sources used by this diagnostic.
    fn sources(&self) -> Vec<SourceBundle>;

    /// Labels referencing spans in the sources.
    fn labels(&self) -> Vec<LabelSpec>;
}
