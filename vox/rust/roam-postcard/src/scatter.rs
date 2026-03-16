//! ScatterPlan: zero-copy serialization that separates structural bytes
//! from borrowed payload references.

use facet_reflect::Peek;

use crate::error::SerializeError;
use crate::serialize::Writer;

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

pub(crate) struct ScatterBuilder {
    staging: Vec<u8>,
    segments: Vec<Segment<'static>>,
    total_size: usize,
}

impl ScatterBuilder {
    fn new() -> Self {
        Self {
            staging: Vec::new(),
            segments: Vec::new(),
            total_size: 0,
        }
    }

    fn finish(self) -> ScatterPlan<'static> {
        ScatterPlan {
            staging: self.staging,
            segments: self.segments,
            total_size: self.total_size,
        }
    }
}

impl Writer for ScatterBuilder {
    fn write_byte(&mut self, byte: u8) {
        self.write_bytes(&[byte]);
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let offset = self.staging.len();
        self.staging.extend_from_slice(bytes);
        let len = bytes.len();

        // Merge with previous staged segment if contiguous
        if let Some(Segment::Staged {
            offset: prev_offset,
            len: prev_len,
        }) = self.segments.last_mut()
        {
            if *prev_offset + *prev_len == offset {
                *prev_len += len;
                self.total_size += len;
                return;
            }
        }

        self.segments.push(Segment::Staged { offset, len });
        self.total_size += len;
    }
}

/// Build a scatter plan from a Peek value.
pub fn peek_to_scatter_plan<'input, 'facet>(
    peek: Peek<'input, 'facet>,
) -> Result<ScatterPlan<'input>, SerializeError> {
    let mut builder = ScatterBuilder::new();
    crate::serialize::serialize_peek(peek, &mut builder)?;
    // SAFETY: ScatterBuilder only stores Staged segments (no Reference segments),
    // so the 'static lifetime on segments is fine — we can widen to 'input.
    let plan = builder.finish();
    // Transmute the lifetime from 'static to 'input. This is safe because
    // all segments are Staged (no borrowed references).
    #[allow(unsafe_code)]
    let plan = unsafe { std::mem::transmute::<ScatterPlan<'static>, ScatterPlan<'input>>(plan) };
    Ok(plan)
}
