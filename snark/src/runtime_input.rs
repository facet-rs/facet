//! Runtime input coordinate types.

use facet::Facet;

/// UTF-8 byte offset.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteOffset(u32);

impl ByteOffset {
    /// Create a byte offset.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Return the numeric byte offset.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Half-open byte range.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
pub struct ByteRange {
    /// Start byte.
    pub start: ByteOffset,
    /// End byte.
    pub end: ByteOffset,
}

/// Zero-based row.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Row(u32);

impl Row {
    /// Create a row.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Return the numeric row.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Zero-based column measured in UTF-8 bytes.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Utf8ColumnBytes(u32);

impl Utf8ColumnBytes {
    /// Create a byte column.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Return the numeric byte column.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Row/column coordinate using UTF-8 byte columns.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
pub struct PointBytes {
    /// Zero-based row.
    pub row: Row,
    /// Zero-based UTF-8 byte column.
    pub column: Utf8ColumnBytes,
}

/// Half-open point range.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
pub struct PointRange {
    /// Start point.
    pub start: PointBytes,
    /// End point.
    pub end: PointBytes,
}

/// Incremental edit coordinates.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
pub struct InputEdit {
    /// Edited byte range in the old input.
    pub old_bytes: ByteRange,
    /// New end byte after the edit.
    pub new_end_byte: ByteOffset,
    /// Edited point range in the old input.
    pub old_points: PointRange,
    /// New end point after the edit.
    pub new_end_point: PointBytes,
}

/// Range included in a child language parse.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
pub struct IncludedRange {
    /// Included byte range.
    pub bytes: ByteRange,
    /// Included point range.
    pub points: PointRange,
}
