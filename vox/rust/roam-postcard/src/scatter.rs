//! ScatterPlan: zero-copy serialization that separates structural bytes
//! from borrowed payload references.

use facet_reflect::Peek;

use crate::error::SerializeError;
use crate::serialize::{PostcardWriter, Writer};

/// A segment of the serialized output.
#[derive(Debug)]
pub enum Segment<'a> {
    /// Structural bytes stored in the staging buffer.
    Staged { offset: usize, len: usize },
    /// Bytes borrowed directly from the source value's memory (zero-copy).
    Reference { bytes: &'a [u8] },
}

/// A plan for writing serialized output with minimal copying.
///
/// Structural metadata (varints, discriminants) lives in `staging`.
/// Payload data (strings, byte arrays) is referenced from the source value.
pub struct ScatterPlan<'a> {
    staging: Vec<u8>,
    segments: Vec<Segment<'a>>,
    total_size: usize,
}

impl<'a> ScatterPlan<'a> {
    pub fn total_size(&self) -> usize {
        self.total_size
    }

    pub fn staging(&self) -> &[u8] {
        &self.staging
    }

    pub fn segments(&self) -> &[Segment<'a>] {
        &self.segments
    }

    /// Write the full serialized output into `dest`.
    /// `dest` must be at least `total_size()` bytes.
    pub fn write_into(&self, dest: &mut [u8]) {
        let mut cursor = 0;
        for segment in &self.segments {
            match segment {
                Segment::Staged { offset, len } => {
                    dest[cursor..cursor + len]
                        .copy_from_slice(&self.staging[*offset..*offset + len]);
                    cursor += len;
                }
                Segment::Reference { bytes } => {
                    dest[cursor..cursor + bytes.len()].copy_from_slice(bytes);
                    cursor += bytes.len();
                }
            }
        }
        debug_assert_eq!(cursor, self.total_size);
    }
}

pub(crate) struct ScatterBuilder<'a> {
    staging: Vec<u8>,
    segments: Vec<Segment<'a>>,
    total_size: usize,
}

impl<'a> ScatterBuilder<'a> {
    fn new() -> Self {
        Self {
            staging: Vec::new(),
            segments: Vec::new(),
            total_size: 0,
        }
    }

    fn finish(self) -> ScatterPlan<'a> {
        ScatterPlan {
            staging: self.staging,
            segments: self.segments,
            total_size: self.total_size,
        }
    }

    fn push_staged_segment(&mut self, offset: usize, len: usize) {
        if len == 0 {
            return;
        }
        // Merge with previous staged segment if contiguous
        if let Some(Segment::Staged {
            offset: prev_offset,
            len: prev_len,
        }) = self.segments.last_mut()
        {
            if *prev_offset + *prev_len == offset {
                *prev_len += len;
                return;
            }
        }
        self.segments.push(Segment::Staged { offset, len });
    }
}

impl Writer for ScatterBuilder<'_> {
    fn write_byte(&mut self, byte: u8) {
        let offset = self.staging.len();
        self.staging.push(byte);
        self.total_size += 1;
        self.push_staged_segment(offset, 1);
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let offset = self.staging.len();
        self.staging.extend_from_slice(bytes);
        self.total_size += bytes.len();
        self.push_staged_segment(offset, bytes.len());
    }
}

impl<'a> PostcardWriter<'a> for ScatterBuilder<'a> {
    fn write_referenced_bytes(&mut self, bytes: &'a [u8]) {
        if bytes.is_empty() {
            return;
        }
        self.segments.push(Segment::Reference { bytes });
        self.total_size += bytes.len();
    }
}

/// Build a scatter plan from a Peek value.
pub fn peek_to_scatter_plan<'input, 'facet>(
    peek: Peek<'input, 'facet>,
) -> Result<ScatterPlan<'input>, SerializeError> {
    let mut builder = ScatterBuilder::new();
    crate::serialize::serialize_peek(peek, &mut builder)?;
    Ok(builder.finish())
}
