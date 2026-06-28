//! Source identities and package-relative paths.

#[cfg(feature = "tree-sitter-import")]
use std::io;
use std::{
    fmt,
    path::{Component, Path, PathBuf},
};

use facet::Facet;

use crate::diagnostic::ImportError;

/// Deterministic source id assigned during package import.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceId(u32);

impl SourceId {
    /// Return the zero-based numeric id.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Allocates source ids in deterministic importer order.
#[derive(Debug, Default)]
#[cfg(feature = "tree-sitter-import")]
pub(crate) struct SourceIdAllocator {
    next: u32,
}

#[cfg(feature = "tree-sitter-import")]
impl SourceIdAllocator {
    pub(crate) const fn new() -> Self {
        Self { next: 0 }
    }

    pub(crate) fn allocate(&mut self) -> SourceId {
        let id = SourceId(self.next);
        self.next += 1;
        id
    }
}

/// Package root used for diagnostics and package-relative path resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageRoot(PathBuf);

impl PackageRoot {
    /// Build a package root from a lexical filesystem path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    /// Canonicalize an existing package root.
    #[cfg(feature = "tree-sitter-import")]
    pub(crate) fn from_existing_dir(path: impl AsRef<Path>) -> Result<Self, ImportError> {
        let path = path.as_ref();
        let canonical = std::fs::canonicalize(path).map_err(|source| ImportError::PackageRoot {
            path: path.to_owned(),
            source,
        })?;
        if !canonical.is_dir() {
            return Err(ImportError::PackageRootNotDirectory { path: canonical });
        }
        Ok(Self(canonical))
    }

    /// Filesystem path for this package root.
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Resolve a package-relative path under this root.
    pub fn join(&self, path: &PackageRelativePath) -> PathBuf {
        self.0.join(path.as_str())
    }
}

impl AsRef<Path> for PackageRoot {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

/// Slash-normalized path relative to a Tree-sitter package root.
#[derive(Debug, Clone, Facet, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageRelativePath(String);

impl PackageRelativePath {
    /// Validate and normalize a package-relative path.
    pub fn new(path: impl AsRef<Path>) -> Result<Self, ImportError> {
        let path = path.as_ref();
        let mut parts = Vec::new();
        for component in path.components() {
            match component {
                Component::Normal(part) => {
                    let Some(part) = part.to_str() else {
                        return Err(ImportError::InvalidPackagePath {
                            path: path.to_owned(),
                            reason: InvalidPackagePathReason::NonUtf8,
                        });
                    };
                    parts.push(part.to_owned());
                }
                Component::CurDir => {}
                Component::ParentDir => {
                    return Err(ImportError::InvalidPackagePath {
                        path: path.to_owned(),
                        reason: InvalidPackagePathReason::ParentComponent,
                    });
                }
                Component::Prefix(_) | Component::RootDir => {
                    return Err(ImportError::InvalidPackagePath {
                        path: path.to_owned(),
                        reason: InvalidPackagePathReason::Absolute,
                    });
                }
            }
        }

        if parts.is_empty() {
            return Err(ImportError::InvalidPackagePath {
                path: path.to_owned(),
                reason: InvalidPackagePathReason::Empty,
            });
        }

        Ok(Self(parts.join("/")))
    }

    /// Return the slash-normalized path string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PackageRelativePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Reason a package-relative path failed validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidPackagePathReason {
    /// Path was empty after normalization.
    Empty,
    /// Path was absolute.
    Absolute,
    /// Path contained `..`.
    ParentComponent,
    /// Path contained non-UTF-8 bytes.
    NonUtf8,
}

impl fmt::Display for InvalidPackagePathReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("empty path"),
            Self::Absolute => f.write_str("absolute path"),
            Self::ParentComponent => f.write_str("parent component"),
            Self::NonUtf8 => f.write_str("non-UTF-8 path"),
        }
    }
}

/// Imported package source with stable id and package-relative path.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct SourceFile<T> {
    /// Stable source id assigned during import.
    pub id: SourceId,
    /// Package-relative source path.
    pub path: PackageRelativePath,
    /// Source payload.
    pub body: T,
}

impl<T> SourceFile<T> {
    /// Transform this source file's body while preserving source identity.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> SourceFile<U> {
        SourceFile {
            id: self.id,
            path: self.path,
            body: f(self.body),
        }
    }
}

#[cfg(feature = "tree-sitter-import")]
pub(crate) fn read_source_string(
    root: &PackageRoot,
    relative: &PackageRelativePath,
    ids: &mut SourceIdAllocator,
) -> Result<SourceFile<String>, ImportError> {
    let absolute = contained_file_path(root, relative)?;
    let body = std::fs::read_to_string(&absolute).map_err(|source| ImportError::ReadFile {
        package_root: root.as_path().to_owned(),
        path: absolute,
        source,
    })?;
    Ok(SourceFile {
        id: ids.allocate(),
        path: relative.clone(),
        body,
    })
}

#[cfg(feature = "tree-sitter-import")]
pub(crate) fn read_optional_source_string(
    root: &PackageRoot,
    relative: &PackageRelativePath,
    ids: &mut SourceIdAllocator,
) -> Result<Option<SourceFile<String>>, ImportError> {
    let absolute = match contained_optional_file_path(root, relative)? {
        Some(absolute) => absolute,
        None => return Ok(None),
    };
    match std::fs::read_to_string(&absolute) {
        Ok(body) => Ok(Some(SourceFile {
            id: ids.allocate(),
            path: relative.clone(),
            body,
        })),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(ImportError::ReadFile {
            package_root: root.as_path().to_owned(),
            path: absolute,
            source,
        }),
    }
}

#[cfg(feature = "tree-sitter-import")]
pub(crate) fn contained_file_path(
    root: &PackageRoot,
    relative: &PackageRelativePath,
) -> Result<PathBuf, ImportError> {
    let lexical = root.join(relative);
    let canonical = std::fs::canonicalize(&lexical).map_err(|source| ImportError::ReadFile {
        package_root: root.as_path().to_owned(),
        path: lexical,
        source,
    })?;
    ensure_under_root(root, canonical)
}

#[cfg(feature = "tree-sitter-import")]
fn contained_optional_file_path(
    root: &PackageRoot,
    relative: &PackageRelativePath,
) -> Result<Option<PathBuf>, ImportError> {
    let lexical = root.join(relative);
    let canonical = match std::fs::canonicalize(&lexical) {
        Ok(canonical) => canonical,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(ImportError::ReadFile {
                package_root: root.as_path().to_owned(),
                path: lexical,
                source,
            });
        }
    };
    ensure_under_root(root, canonical).map(Some)
}

#[cfg(feature = "tree-sitter-import")]
fn ensure_under_root(root: &PackageRoot, path: PathBuf) -> Result<PathBuf, ImportError> {
    if path.starts_with(root.as_path()) {
        Ok(path)
    } else {
        Err(ImportError::PathOutsidePackage {
            package_root: root.as_path().to_owned(),
            path,
        })
    }
}
