//! Error types for partial value construction.

use std::fmt;

use crate::ops::Path;
use facet_core::Shape;

/// Location where an error occurred.
pub struct ErrorLocation {
    pub shape: &'static Shape,
    pub path: Path,
}

impl fmt::Display for ErrorLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.shape.type_identifier)?;
        if !self.path.is_empty() {
            write!(f, " at path {:?}", self.path.as_slice())?;
        }
        Ok(())
    }
}

impl fmt::Debug for ErrorLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// An error during reflection.
pub struct ReflectError {
    pub location: ErrorLocation,
    pub kind: ReflectErrorKind,
}

impl fmt::Display for ReflectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} for {}", self.kind, self.location)
    }
}

impl fmt::Debug for ReflectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl ReflectError {
    /// Create a new error at the given shape and path.
    pub fn new(shape: &'static Shape, path: Path, kind: ReflectErrorKind) -> Self {
        Self {
            location: ErrorLocation { shape, path },
            kind,
        }
    }

    /// Create a new error at the root (empty path).
    pub fn at_root(shape: &'static Shape, kind: ReflectErrorKind) -> Self {
        Self::new(shape, Path::default(), kind)
    }
}

/// The kind of reflection error.
pub enum ReflectErrorKind {
    /// Shape mismatch during set operation.
    ShapeMismatch {
        expected: &'static Shape,
        actual: &'static Shape,
    },
    /// Tried to build an uninitialized value.
    NotInitialized,
    /// Cannot allocate unsized type.
    Unsized { shape: &'static Shape },
    /// Memory allocation failed.
    AllocFailed { layout: core::alloc::Layout },
    /// Field index out of bounds.
    FieldIndexOutOfBounds { index: u32, field_count: usize },
    /// Type is not a struct (cannot navigate into fields).
    NotAStruct,
    /// Multi-level paths are not yet supported.
    MultiLevelPathNotSupported { depth: usize },
    /// Frame is already initialized.
    AlreadyInitialized,
    /// Expected indexed children but found none.
    NotIndexedChildren,
    /// Arena double-free detected.
    DoubleFree,
    /// Arena slot is empty.
    SlotEmpty,
    /// Partial is poisoned after a previous error.
    Poisoned,
    /// Type does not implement Default.
    NoDefault { shape: &'static Shape },
    /// Cannot use Build with empty path.
    BuildAtEmptyPath,
    /// Cannot End at root frame.
    EndAtRoot,
    /// Cannot End with incomplete children.
    EndWithIncomplete,
    /// Variant index out of bounds.
    VariantIndexOutOfBounds { index: u32, variant_count: usize },
    /// Type is not an enum.
    NotAnEnum,
    /// Enum has unsupported representation (RustNPO).
    UnsupportedEnumRepr,
}

impl fmt::Display for ReflectErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReflectErrorKind::ShapeMismatch { expected, actual } => {
                write!(
                    f,
                    "Shape mismatch: expected {}, got {}",
                    expected.type_identifier, actual.type_identifier
                )
            }
            ReflectErrorKind::NotInitialized => write!(f, "Value not initialized"),
            ReflectErrorKind::Unsized { shape } => {
                write!(f, "Cannot allocate unsized type {}", shape.type_identifier)
            }
            ReflectErrorKind::AllocFailed { layout } => {
                write!(
                    f,
                    "Allocation failed for size={}, align={}",
                    layout.size(),
                    layout.align()
                )
            }
            ReflectErrorKind::FieldIndexOutOfBounds { index, field_count } => {
                write!(
                    f,
                    "Field index {} out of bounds (type has {} fields)",
                    index, field_count
                )
            }
            ReflectErrorKind::NotAStruct => write!(f, "Type is not a struct"),
            ReflectErrorKind::MultiLevelPathNotSupported { depth } => {
                write!(f, "Multi-level paths not supported (depth {})", depth)
            }
            ReflectErrorKind::AlreadyInitialized => write!(f, "Value already initialized"),
            ReflectErrorKind::NotIndexedChildren => write!(f, "Type has no indexed children"),
            ReflectErrorKind::DoubleFree => write!(f, "Double free detected"),
            ReflectErrorKind::SlotEmpty => write!(f, "Arena slot is empty"),
            ReflectErrorKind::Poisoned => write!(f, "Partial is poisoned"),
            ReflectErrorKind::NoDefault { shape } => {
                write!(f, "No default for {}", shape.type_identifier)
            }
            ReflectErrorKind::BuildAtEmptyPath => write!(f, "Cannot build at empty path"),
            ReflectErrorKind::EndAtRoot => write!(f, "Cannot end at root frame"),
            ReflectErrorKind::EndWithIncomplete => {
                write!(f, "Cannot end with incomplete children")
            }
            ReflectErrorKind::VariantIndexOutOfBounds {
                index,
                variant_count,
            } => {
                write!(
                    f,
                    "Variant index {} out of bounds (enum has {} variants)",
                    index, variant_count
                )
            }
            ReflectErrorKind::NotAnEnum => write!(f, "Type is not an enum"),
            ReflectErrorKind::UnsupportedEnumRepr => {
                write!(f, "Enum has unsupported representation")
            }
        }
    }
}

impl fmt::Debug for ReflectErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
