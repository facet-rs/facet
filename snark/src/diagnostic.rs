//! Import errors and diagnostic-oriented source categories.

use std::{error::Error, fmt, io, path::PathBuf};

use crate::source::InvalidPackagePathReason;

/// JSON document kind being imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonDocumentKind {
    /// `src/grammar.json`.
    Grammar,
    /// `tree-sitter.json`.
    TreeSitterConfig,
    /// `src/node-types.json`.
    NodeTypes,
}

impl fmt::Display for JsonDocumentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Grammar => f.write_str("grammar.json"),
            Self::TreeSitterConfig => f.write_str("tree-sitter.json"),
            Self::NodeTypes => f.write_str("node-types.json"),
        }
    }
}

/// Error raised while importing a Tree-sitter package or grammar.
#[derive(Debug)]
pub enum ImportError {
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
    /// Facet JSON deserialization failed.
    #[cfg(feature = "json-import")]
    Json {
        /// Package root used for this import, when known.
        package_root: Option<PathBuf>,
        /// JSON document path, when known.
        path: Option<PathBuf>,
        /// Document kind.
        document: JsonDocumentKind,
        /// Import phase.
        phase: &'static str,
        /// Facet JSON error.
        source: facet_json::DeserializeError,
    },
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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
            Self::ReadFile { source, .. } | Self::ReadDir { source, .. } => Some(source),
            #[cfg(feature = "json-import")]
            Self::Json { source, .. } => Some(source),
            Self::InvalidPackagePath { .. } => None,
        }
    }
}
