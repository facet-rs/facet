//! Low-level byte primitives shared by the identity hash and the self-describing
//! codec: a write [`Sink`], width-tagged writers, and a validating [`Reader`].
//!
//! The same [`Sink`] feeds either a `blake3::Hasher` (identity hashing, no
//! allocation) or a `Vec<u8>` (producing wire bytes, or capturing canonical
//! bytes for inspection). The [`Reader`] is the decode counterpart and is where
//! the hostile-input checks (`r[validate.*]`) live: bounds, length-vs-remaining,
//! UTF-8, and Unicode-scalar validation.

use core::fmt;

// ============================================================================
// Write side
// ============================================================================

/// A destination for encoded bytes.
pub trait Sink {
    fn put(&mut self, bytes: &[u8]);
}

impl Sink for blake3::Hasher {
    fn put(&mut self, bytes: &[u8]) {
        self.update(bytes);
    }
}

impl Sink for Vec<u8> {
    fn put(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }
}

pub fn write_u8<S: Sink>(out: &mut S, n: u8) {
    out.put(&[n]);
}

pub fn write_u16<S: Sink>(out: &mut S, n: u16) {
    out.put(&n.to_le_bytes());
}

pub fn write_u32<S: Sink>(out: &mut S, n: u32) {
    out.put(&n.to_le_bytes());
}

pub fn write_u64<S: Sink>(out: &mut S, n: u64) {
    out.put(&n.to_le_bytes());
}

pub fn write_u128<S: Sink>(out: &mut S, n: u128) {
    out.put(&n.to_le_bytes());
}

pub fn write_i8<S: Sink>(out: &mut S, n: i8) {
    out.put(&n.to_le_bytes());
}

pub fn write_i16<S: Sink>(out: &mut S, n: i16) {
    out.put(&n.to_le_bytes());
}

pub fn write_i32<S: Sink>(out: &mut S, n: i32) {
    out.put(&n.to_le_bytes());
}

pub fn write_i64<S: Sink>(out: &mut S, n: i64) {
    out.put(&n.to_le_bytes());
}

pub fn write_i128<S: Sink>(out: &mut S, n: i128) {
    out.put(&n.to_le_bytes());
}

pub fn write_f32<S: Sink>(out: &mut S, n: f32) {
    out.put(&n.to_le_bytes());
}

pub fn write_f64<S: Sink>(out: &mut S, n: f64) {
    out.put(&n.to_le_bytes());
}

pub fn write_bool<S: Sink>(out: &mut S, b: bool) {
    write_u8(out, u8::from(b));
}

/// A length-prefixed string: a `u32` LE byte length then the UTF-8 bytes.
pub fn write_str<S: Sink>(out: &mut S, s: &str) {
    write_u32(out, s.len() as u32);
    out.put(s.as_bytes());
}

/// A length-prefixed byte run: a `u32` LE length then the raw bytes.
pub fn write_bytes<S: Sink>(out: &mut S, b: &[u8]) {
    write_u32(out, b.len() as u32);
    out.put(b);
}

// ============================================================================
// Decode errors
// ============================================================================

/// Why decoding failed. A crafted message must never crash the decoder; every
/// malformed input becomes one of these (`r[validate.*]`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// Fewer bytes remain than the next read needs.
    UnexpectedEof { needed: usize, remaining: usize },
    /// A self-describing tag byte outside the defined table (`r[validate.tags]`).
    UnknownTag(u8),
    /// A `bool` byte other than 0 or 1.
    InvalidBool(u8),
    /// A `string` whose bytes are not valid UTF-8 (`r[validate.text]`).
    InvalidUtf8,
    /// A `char` value that is not a Unicode scalar (`r[validate.text]`).
    InvalidChar(u32),
    /// A length or count larger than the remaining buffer could hold
    /// (`r[validate.lengths]`).
    LengthTooLarge { count: u64, remaining: usize },
    /// Nesting deeper than the decoder's bound (`r[validate.depth]`).
    DepthExceeded,
    /// A duplicate key in a `map` (`r[validate.uniqueness]`).
    DuplicateKey,
    /// A duplicate element in a `set` (`r[validate.uniqueness]`).
    DuplicateElement,
    /// A typed decode found a tag it did not expect for the type being read.
    UnexpectedTag { expected: &'static str, got: u8 },
    /// A typed decode found an enum variant name it does not recognize.
    UnknownVariant(String),
    /// A typed decode found a structurally wrong value (e.g. an unexpected
    /// struct field count).
    Malformed(&'static str),
    /// Bytes remained after a value that should have consumed the whole input.
    TrailingBytes(usize),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::UnexpectedEof { needed, remaining } => {
                write!(f, "unexpected end of input: need {needed}, have {remaining}")
            }
            DecodeError::UnknownTag(t) => write!(f, "unknown tag {t:#04x}"),
            DecodeError::InvalidBool(b) => write!(f, "invalid bool byte {b:#04x}"),
            DecodeError::InvalidUtf8 => write!(f, "invalid UTF-8 in string"),
            DecodeError::InvalidChar(c) => write!(f, "invalid Unicode scalar {c:#x}"),
            DecodeError::LengthTooLarge { count, remaining } => {
                write!(f, "length {count} exceeds {remaining} bytes remaining")
            }
            DecodeError::DepthExceeded => write!(f, "maximum nesting depth exceeded"),
            DecodeError::DuplicateKey => write!(f, "duplicate map key"),
            DecodeError::DuplicateElement => write!(f, "duplicate set element"),
            DecodeError::UnexpectedTag { expected, got } => {
                write!(f, "expected {expected}, got tag {got:#04x}")
            }
            DecodeError::UnknownVariant(name) => write!(f, "unknown variant {name:?}"),
            DecodeError::Malformed(what) => write!(f, "malformed value: {what}"),
            DecodeError::TrailingBytes(n) => write!(f, "{n} trailing bytes after value"),
        }
    }
}

impl std::error::Error for DecodeError {}

// ============================================================================
// Read side
// ============================================================================

/// A cursor over an in-memory message that validates as it reads.
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    #[must_use]
    pub fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    /// Bytes not yet consumed.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    /// Number of bytes consumed so far.
    #[must_use]
    pub fn position(&self) -> usize {
        self.pos
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        if self.remaining() < n {
            return Err(DecodeError::UnexpectedEof {
                needed: n,
                remaining: self.remaining(),
            });
        }
        let slice = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    /// Read exactly `n` raw bytes (bounds-checked), borrowed from the buffer.
    pub fn read_slice(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        self.take(n)
    }

    pub fn read_u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }

    pub fn read_u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    pub fn read_u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub fn read_u64(&mut self) -> Result<u64, DecodeError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    pub fn read_u128(&mut self) -> Result<u128, DecodeError> {
        Ok(u128::from_le_bytes(self.take(16)?.try_into().unwrap()))
    }

    pub fn read_i8(&mut self) -> Result<i8, DecodeError> {
        Ok(i8::from_le_bytes(self.take(1)?.try_into().unwrap()))
    }

    pub fn read_i16(&mut self) -> Result<i16, DecodeError> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    pub fn read_i32(&mut self) -> Result<i32, DecodeError> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub fn read_i64(&mut self) -> Result<i64, DecodeError> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    pub fn read_i128(&mut self) -> Result<i128, DecodeError> {
        Ok(i128::from_le_bytes(self.take(16)?.try_into().unwrap()))
    }

    pub fn read_f32(&mut self) -> Result<f32, DecodeError> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub fn read_f64(&mut self) -> Result<f64, DecodeError> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    pub fn read_bool(&mut self) -> Result<bool, DecodeError> {
        match self.read_u8()? {
            0 => Ok(false),
            1 => Ok(true),
            b => Err(DecodeError::InvalidBool(b)),
        }
    }

    /// A `char`: 4 LE bytes validated as a Unicode scalar value
    /// (`r[validate.text]`).
    pub fn read_char(&mut self) -> Result<char, DecodeError> {
        let n = self.read_u32()?;
        char::from_u32(n).ok_or(DecodeError::InvalidChar(n))
    }

    /// A length-prefixed UTF-8 string, borrowed from the buffer
    /// (`r[validate.text]`).
    pub fn read_str(&mut self) -> Result<&'a str, DecodeError> {
        let len = self.read_len(1)?;
        let bytes = self.take(len)?;
        core::str::from_utf8(bytes).map_err(|_| DecodeError::InvalidUtf8)
    }

    /// A length-prefixed byte run, borrowed from the buffer.
    pub fn read_bytes(&mut self) -> Result<&'a [u8], DecodeError> {
        let len = self.read_len(1)?;
        self.take(len)
    }

    /// Read a `u32` count/length and check it cannot drive a read or
    /// pre-allocation larger than the buffer allows: with each element costing at
    /// least `min_elem_size` bytes, the count may not exceed
    /// `remaining / min_elem_size` (`r[validate.lengths]`).
    pub fn read_len(&mut self, min_elem_size: usize) -> Result<usize, DecodeError> {
        let count = self.read_u32()? as usize;
        let max = self.remaining() / min_elem_size.max(1);
        if count > max {
            return Err(DecodeError::LengthTooLarge {
                count: count as u64,
                remaining: self.remaining(),
            });
        }
        Ok(count)
    }
}
