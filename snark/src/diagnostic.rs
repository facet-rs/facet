//! Import errors and diagnostic-oriented source categories.

use std::{error::Error, fmt, io, path::PathBuf};

use facet::Facet;

#[cfg(feature = "json-import")]
use crate::source::PackageRelativePath;
use crate::source::{InvalidPackagePathReason, SourceId};

/// Diagnostic severity.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
#[repr(u8)]
pub enum Severity {
    /// The operation cannot continue.
    Error,
    /// The operation can continue, but a capability or compatibility fact changed.
    Warning,
}

/// Stable diagnostic code.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
#[repr(u8)]
pub enum DiagnosticCode {
    /// Reading a source file failed.
    ReadFile,
    /// Reading a source directory failed.
    ReadDir,
    /// Validating a package-relative path failed.
    InvalidPackagePath,
    /// Canonicalizing or validating a package root failed.
    InvalidPackageRoot,
    /// A discovered filesystem path was outside the package root.
    PathOutsidePackage,
    /// A Tree-sitter external-token ordinal could not fit in Snark's ordinal type.
    ExternalTokenOrdinalOverflow,
    /// A `tree-sitter.json` manifest did not declare any grammars.
    NoGrammars,
    /// Decoding JSON into a raw compatibility model failed.
    JsonDecode,
}

/// Byte span inside an imported source, when a source id is known.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
pub struct SourceSpan {
    /// Imported source id.
    pub source_id: SourceId,
    /// Byte offset from the start of the source.
    pub start: u32,
    /// Span length in bytes.
    pub len: u32,
}

/// Labeled diagnostic span.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct DiagnosticLabel {
    /// Span being labeled.
    pub span: SourceSpan,
    /// Label message.
    pub message: String,
}

/// Structured diagnostic emitted by import, validation, and later lowering phases.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct Diagnostic {
    /// Diagnostic severity.
    pub severity: Severity,
    /// Stable diagnostic code.
    pub code: DiagnosticCode,
    /// Main diagnostic message.
    pub message: String,
    /// Primary source span, when available.
    pub primary_span: Option<SourceSpan>,
    /// Additional labeled spans.
    pub labels: Vec<DiagnosticLabel>,
    /// Supplemental notes.
    pub notes: Vec<String>,
}

impl Diagnostic {
    fn error(code: DiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code,
            message: message.into(),
            primary_span: None,
            labels: Vec::new(),
            notes: Vec::new(),
        }
    }

    #[cfg(feature = "json-import")]
    fn with_primary_span(mut self, span: SourceSpan) -> Self {
        self.primary_span = Some(span);
        self
    }

    fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}

/// JSON document kind being imported.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
#[repr(u8)]
pub enum JsonDocumentKind {
    /// `src/grammar.json`.
    Grammar,
    /// `tree-sitter.json`.
    TreeSitterConfig,
}

impl fmt::Display for JsonDocumentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Grammar => f.write_str("grammar.json"),
            Self::TreeSitterConfig => f.write_str("tree-sitter.json"),
        }
    }
}

/// Error raised while importing a Tree-sitter package or grammar.
#[derive(Debug)]
pub enum ImportError {
    /// Could not validate the package root.
    PackageRoot {
        /// Root path requested by the caller.
        path: PathBuf,
        /// I/O error raised while validating the root.
        source: io::Error,
    },
    /// Package root did not point to a directory.
    PackageRootNotDirectory {
        /// Root path requested by the caller.
        path: PathBuf,
    },
    /// Could not read a file.
    ReadFile {
        /// Package root used for this import.
        package_root: PathBuf,
        /// File path.
        path: PathBuf,
        /// I/O error.
        source: io::Error,
    },
    /// Could not read a directory.
    ReadDir {
        /// Package root used for this import.
        package_root: PathBuf,
        /// Directory path.
        path: PathBuf,
        /// I/O error.
        source: io::Error,
    },
    /// A package-relative path was invalid.
    InvalidPackagePath {
        /// Invalid path.
        path: PathBuf,
        /// Validation failure.
        reason: InvalidPackagePathReason,
    },
    /// A discovered package file did not live under the package root.
    PathOutsidePackage {
        /// Package root used for this import.
        package_root: PathBuf,
        /// Discovered path.
        path: PathBuf,
    },
    /// Too many external tokens were declared for the ordinal type.
    ExternalTokenOrdinalOverflow {
        /// Source-order index that did not fit.
        index: usize,
    },
    /// `tree-sitter.json` did not declare any grammar entries.
    NoGrammarsInManifest {
        /// Package root used for this import.
        package_root: PathBuf,
    },
    /// Facet JSON deserialization failed.
    #[cfg(feature = "json-import")]
    Json {
        /// Package root used for this import, when known.
        package_root: Option<PathBuf>,
        /// JSON document path, when known.
        path: Option<PathBuf>,
        /// Source id for this JSON document, when known.
        source_id: Option<SourceId>,
        /// Package-relative JSON document path, when known.
        package_path: Option<PackageRelativePath>,
        /// Document kind.
        document: JsonDocumentKind,
        /// Import phase.
        phase: &'static str,
        /// Facet JSON error.
        source: Box<facet_json::DeserializeError>,
    },
}

impl ImportError {
    /// Convert this import error into Snark's structured diagnostic contract.
    pub fn diagnostic(&self) -> Diagnostic {
        match self {
            Self::PackageRoot { path, source } => Diagnostic::error(
                DiagnosticCode::InvalidPackageRoot,
                format!("could not validate package root {}", path.display()),
            )
            .with_note(source.to_string()),
            Self::PackageRootNotDirectory { path } => Diagnostic::error(
                DiagnosticCode::InvalidPackageRoot,
                format!("package root {} is not a directory", path.display()),
            ),
            Self::ReadFile {
                package_root,
                path,
                source,
            } => Diagnostic::error(
                DiagnosticCode::ReadFile,
                format!("could not read {}", path.display()),
            )
            .with_note(format!("package root: {}", package_root.display()))
            .with_note(source.to_string()),
            Self::ReadDir {
                package_root,
                path,
                source,
            } => Diagnostic::error(
                DiagnosticCode::ReadDir,
                format!("could not read directory {}", path.display()),
            )
            .with_note(format!("package root: {}", package_root.display()))
            .with_note(source.to_string()),
            Self::InvalidPackagePath { path, reason } => Diagnostic::error(
                DiagnosticCode::InvalidPackagePath,
                format!("invalid package-relative path {}", path.display()),
            )
            .with_note(reason.to_string()),
            Self::PathOutsidePackage { package_root, path } => Diagnostic::error(
                DiagnosticCode::PathOutsidePackage,
                format!(
                    "package path {} is outside the package root",
                    path.display()
                ),
            )
            .with_note(format!("package root: {}", package_root.display())),
            Self::ExternalTokenOrdinalOverflow { index } => Diagnostic::error(
                DiagnosticCode::ExternalTokenOrdinalOverflow,
                format!("external token index {index} does not fit in u32"),
            ),
            Self::NoGrammarsInManifest { package_root } => Diagnostic::error(
                DiagnosticCode::NoGrammars,
                "tree-sitter.json did not declare any grammars",
            )
            .with_note(format!("package root: {}", package_root.display())),
            #[cfg(feature = "json-import")]
            Self::Json {
                source_id,
                package_path,
                document,
                phase,
                source,
                ..
            } => {
                let mut diagnostic = Diagnostic::error(
                    DiagnosticCode::JsonDecode,
                    format!("could not deserialize {document} during {phase}"),
                );
                if let (Some(source_id), Some(span)) = (*source_id, source.span) {
                    diagnostic = diagnostic.with_primary_span(SourceSpan {
                        source_id,
                        start: span.offset,
                        len: span.len,
                    });
                }
                if let Some(package_path) = package_path {
                    diagnostic = diagnostic.with_note(format!("package path: {package_path}"));
                }
                if let Some(path) = &source.path {
                    diagnostic = diagnostic.with_note(format!("facet path: {path}"));
                }
                diagnostic.with_note(source.kind.to_string())
            }
        }
    }
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PackageRoot { path, source } => {
                write!(
                    f,
                    "could not validate package root {}: {}",
                    path.display(),
                    source
                )
            }
            Self::PackageRootNotDirectory { path } => {
                write!(f, "package root {} is not a directory", path.display())
            }
            Self::ReadFile {
                package_root,
                path,
                source,
            } => {
                write!(
                    f,
                    "could not read {} under package {}: {}",
                    path.display(),
                    package_root.display(),
                    source
                )
            }
            Self::ReadDir {
                package_root,
                path,
                source,
            } => {
                write!(
                    f,
                    "could not read directory {} under package {}: {}",
                    path.display(),
                    package_root.display(),
                    source
                )
            }
            Self::InvalidPackagePath { path, reason } => {
                write!(
                    f,
                    "invalid package-relative path {}: {}",
                    path.display(),
                    reason
                )
            }
            Self::PathOutsidePackage { package_root, path } => {
                write!(
                    f,
                    "path {} is not under package root {}",
                    path.display(),
                    package_root.display()
                )
            }
            Self::ExternalTokenOrdinalOverflow { index } => {
                write!(f, "external token index {index} does not fit in u32")
            }
            Self::NoGrammarsInManifest { package_root } => write!(
                f,
                "tree-sitter.json under package {} did not declare any grammars",
                package_root.display()
            ),
            #[cfg(feature = "json-import")]
            Self::Json {
                path,
                document,
                phase,
                source,
                ..
            } => match path {
                Some(path) => write!(
                    f,
                    "could not deserialize {document} at {} during {phase}: {source}",
                    path.display()
                ),
                None => write!(
                    f,
                    "could not deserialize {document} during {phase}: {source}"
                ),
            },
        }
    }
}

impl Error for ImportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PackageRoot { source, .. }
            | Self::ReadFile { source, .. }
            | Self::ReadDir { source, .. } => Some(source),
            #[cfg(feature = "json-import")]
            Self::Json { source, .. } => Some(source.as_ref()),
            Self::InvalidPackagePath { .. }
            | Self::PathOutsidePackage { .. }
            | Self::PackageRootNotDirectory { .. }
            | Self::ExternalTokenOrdinalOverflow { .. }
            | Self::NoGrammarsInManifest { .. } => None,
        }
    }
}
