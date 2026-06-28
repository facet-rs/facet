//! Runtime input coordinate types.

use std::fmt;

use facet::Facet;

/// Error raised when constructing runtime input coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeError {
    /// Byte range end was before start.
    ReversedByteRange {
        /// Start byte.
        start: ByteOffset,
        /// End byte.
        end: ByteOffset,
    },
    /// Point range end was before start.
    ReversedPointRange {
        /// Start point.
        start: PointBytes,
        /// End point.
        end: PointBytes,
    },
}

impl fmt::Display for RangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReversedByteRange { start, end } => {
                write!(
                    f,
                    "byte range end {} is before start {}",
                    end.get(),
                    start.get()
                )
            }
            Self::ReversedPointRange { start, end } => {
                write!(
                    f,
                    "point range end {}:{} is before start {}:{}",
                    end.row.get(),
                    end.column.get(),
                    start.row.get(),
                    start.column.get()
                )
            }
        }
    }
}

impl std::error::Error for RangeError {}

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
    start: ByteOffset,
    end: ByteOffset,
}

impl ByteRange {
    /// Construct a half-open byte range.
    pub fn new(start: ByteOffset, end: ByteOffset) -> Result<Self, RangeError> {
        if end < start {
            return Err(RangeError::ReversedByteRange { start, end });
        }
        Ok(Self { start, end })
    }

    /// Start byte.
    pub const fn start(self) -> ByteOffset {
        self.start
    }

    /// End byte.
    pub const fn end(self) -> ByteOffset {
        self.end
    }
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
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq, PartialOrd, Ord)]
pub struct PointBytes {
    row: Row,
    column: Utf8ColumnBytes,
}

impl PointBytes {
    /// Construct a point from row and UTF-8 byte column.
    pub const fn new(row: Row, column: Utf8ColumnBytes) -> Self {
        Self { row, column }
    }

    /// Zero-based row.
    pub const fn row(self) -> Row {
        self.row
    }

    /// Zero-based UTF-8 byte column.
    pub const fn column(self) -> Utf8ColumnBytes {
        self.column
    }
}

/// Half-open point range.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
pub struct PointRange {
    start: PointBytes,
    end: PointBytes,
}

impl PointRange {
    /// Construct a half-open point range.
    pub fn new(start: PointBytes, end: PointBytes) -> Result<Self, RangeError> {
        if end < start {
            return Err(RangeError::ReversedPointRange { start, end });
        }
        Ok(Self { start, end })
    }

    /// Start point.
    pub const fn start(self) -> PointBytes {
        self.start
    }

    /// End point.
    pub const fn end(self) -> PointBytes {
        self.end
    }
}

/// Incremental edit coordinates.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
pub struct InputEdit {
    old_bytes: ByteRange,
    new_end_byte: ByteOffset,
    old_points: PointRange,
    new_end_point: PointBytes,
}

impl InputEdit {
    /// Construct incremental edit coordinates from validated old-input ranges.
    pub const fn new(
        old_bytes: ByteRange,
        new_end_byte: ByteOffset,
        old_points: PointRange,
        new_end_point: PointBytes,
    ) -> Self {
        Self {
            old_bytes,
            new_end_byte,
            old_points,
            new_end_point,
        }
    }

    /// Edited byte range in the old input.
    pub const fn old_bytes(self) -> ByteRange {
        self.old_bytes
    }

    /// New end byte after the edit.
    pub const fn new_end_byte(self) -> ByteOffset {
        self.new_end_byte
    }

    /// Edited point range in the old input.
    pub const fn old_points(self) -> PointRange {
        self.old_points
    }

    /// New end point after the edit.
    pub const fn new_end_point(self) -> PointBytes {
        self.new_end_point
    }
}

/// Range included in a child language parse.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
pub struct IncludedRange {
    bytes: ByteRange,
    points: PointRange,
}

impl IncludedRange {
    /// Construct an included range from validated byte and point ranges.
    pub const fn new(bytes: ByteRange, points: PointRange) -> Self {
        Self { bytes, points }
    }

    /// Included byte range.
    pub const fn bytes(self) -> ByteRange {
        self.bytes
    }

    /// Included point range.
    pub const fn points(self) -> PointRange {
        self.points
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_ranges_reject_reversed_order() {
        let start = ByteOffset::new(10);
        let end = ByteOffset::new(5);

        assert_eq!(
            ByteRange::new(start, end),
            Err(RangeError::ReversedByteRange { start, end })
        );
    }

    #[test]
    fn point_ranges_reject_reversed_order() {
        let start = PointBytes::new(Row::new(2), Utf8ColumnBytes::new(0));
        let end = PointBytes::new(Row::new(1), Utf8ColumnBytes::new(20));

        assert_eq!(
            PointRange::new(start, end),
            Err(RangeError::ReversedPointRange { start, end })
        );
    }
}
