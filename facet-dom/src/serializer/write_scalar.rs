//! Scalar value writing for DOM serializers.

extern crate alloc;

use alloc::string::String;
use core::fmt::Write as FmtWrite;
use facet_core::{Def, ScalarType};
use facet_reflect::Peek;

use super::DomSerializer;

/// Extension trait for writing scalar values directly to output.
pub trait WriteScalar: DomSerializer {
    /// Format a scalar value to a string (for attributes).
    ///
    /// Returns `Some(string)` if the value is a scalar, `None` otherwise.
    fn format_scalar(&self, value: Peek<'_, '_>) -> Option<String> {
        // Handle Option<T> by unwrapping if Some
        if let Def::Option(_) = &value.shape().def
            && let Ok(opt) = value.into_option()
        {
            return match opt.value() {
                Some(inner) => self.format_scalar(inner),
                None => None,
            };
        }

        if let Some(scalar_type) = value.scalar_type() {
            let mut buf = ScalarBuffer::new();
            let written = match scalar_type {
                ScalarType::Unit => {
                    buf.push_str("null");
                    true
                }
                ScalarType::Bool => {
                    if let Ok(b) = value.get::<bool>() {
                        buf.push_str(if *b { "true" } else { "false" });
                        true
                    } else {
                        false
                    }
                }
                ScalarType::Char => {
                    if let Ok(c) = value.get::<char>() {
                        buf.push(*c);
                        true
                    } else {
                        false
                    }
                }
                ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
                    if let Some(s) = value.as_str() {
                        return Some(s.to_string());
                    }
                    false
                }
                ScalarType::F32 => {
                    if let Ok(v) = value.get::<f32>() {
                        self.write_float(*v as f64, &mut buf);
                        true
                    } else {
                        false
                    }
                }
                ScalarType::F64 => {
                    if let Ok(v) = value.get::<f64>() {
                        self.write_float(*v, &mut buf);
                        true
                    } else {
                        false
                    }
                }
                ScalarType::U8 => write_int!(buf, value, u8),
                ScalarType::U16 => write_int!(buf, value, u16),
                ScalarType::U32 => write_int!(buf, value, u32),
                ScalarType::U64 => write_int!(buf, value, u64),
                ScalarType::U128 => write_int!(buf, value, u128),
                ScalarType::USize => write_int!(buf, value, usize),
                ScalarType::I8 => write_int!(buf, value, i8),
                ScalarType::I16 => write_int!(buf, value, i16),
                ScalarType::I32 => write_int!(buf, value, i32),
                ScalarType::I64 => write_int!(buf, value, i64),
                ScalarType::I128 => write_int!(buf, value, i128),
                ScalarType::ISize => write_int!(buf, value, isize),
                #[cfg(feature = "net")]
                ScalarType::IpAddr => write_int!(buf, value, core::net::IpAddr),
                #[cfg(feature = "net")]
                ScalarType::Ipv4Addr => write_int!(buf, value, core::net::Ipv4Addr),
                #[cfg(feature = "net")]
                ScalarType::Ipv6Addr => write_int!(buf, value, core::net::Ipv6Addr),
                #[cfg(feature = "net")]
                ScalarType::SocketAddr => write_int!(buf, value, core::net::SocketAddr),
                _ => false,
            };

            if written {
                return Some(buf.as_str().to_string());
            }
        }

        // Try Display for Def::Scalar types (SmolStr, etc.)
        if matches!(value.shape().def, Def::Scalar) && value.shape().vtable.has_display() {
            let mut buf = ScalarBuffer::new();
            let _ = write!(buf, "{}", value);
            return Some(buf.as_str().to_string());
        }

        None
    }

    /// Write a scalar value to the serializer's output.
    ///
    /// Returns `Ok(true)` if the value was written, `Ok(false)` if not a scalar.
    /// Override to customize formatting (e.g., custom float precision).
    fn write_scalar(&mut self, value: Peek<'_, '_>) -> Result<bool, Self::Error> {
        // Handle Option<T> by unwrapping if Some
        if let Def::Option(_) = &value.shape().def
            && let Ok(opt) = value.into_option()
        {
            return match opt.value() {
                Some(inner) => self.write_scalar(inner),
                None => Ok(false),
            };
        }

        if let Some(scalar_type) = value.scalar_type() {
            let mut buf = ScalarBuffer::new();
            let written = match scalar_type {
                ScalarType::Unit => {
                    buf.push_str("null");
                    true
                }
                ScalarType::Bool => {
                    if let Ok(b) = value.get::<bool>() {
                        buf.push_str(if *b { "true" } else { "false" });
                        true
                    } else {
                        false
                    }
                }
                ScalarType::Char => {
                    if let Ok(c) = value.get::<char>() {
                        buf.push(*c);
                        true
                    } else {
                        false
                    }
                }
                ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
                    if let Some(s) = value.as_str() {
                        self.text(s)?;
                        return Ok(true);
                    }
                    false
                }
                ScalarType::F32 => {
                    if let Ok(v) = value.get::<f32>() {
                        self.write_float(*v as f64, &mut buf);
                        true
                    } else {
                        false
                    }
                }
                ScalarType::F64 => {
                    if let Ok(v) = value.get::<f64>() {
                        self.write_float(*v, &mut buf);
                        true
                    } else {
                        false
                    }
                }
                ScalarType::U8 => write_int!(buf, value, u8),
                ScalarType::U16 => write_int!(buf, value, u16),
                ScalarType::U32 => write_int!(buf, value, u32),
                ScalarType::U64 => write_int!(buf, value, u64),
                ScalarType::U128 => write_int!(buf, value, u128),
                ScalarType::USize => write_int!(buf, value, usize),
                ScalarType::I8 => write_int!(buf, value, i8),
                ScalarType::I16 => write_int!(buf, value, i16),
                ScalarType::I32 => write_int!(buf, value, i32),
                ScalarType::I64 => write_int!(buf, value, i64),
                ScalarType::I128 => write_int!(buf, value, i128),
                ScalarType::ISize => write_int!(buf, value, isize),
                #[cfg(feature = "net")]
                ScalarType::IpAddr => write_display!(buf, value, core::net::IpAddr),
                #[cfg(feature = "net")]
                ScalarType::Ipv4Addr => write_display!(buf, value, core::net::Ipv4Addr),
                #[cfg(feature = "net")]
                ScalarType::Ipv6Addr => write_display!(buf, value, core::net::Ipv6Addr),
                #[cfg(feature = "net")]
                ScalarType::SocketAddr => write_display!(buf, value, core::net::SocketAddr),
                _ => false,
            };

            if written {
                self.text(buf.as_str())?;
                return Ok(true);
            }
        }

        // Try Display for Def::Scalar types (SmolStr, etc.)
        if matches!(value.shape().def, Def::Scalar) && value.shape().vtable.has_display() {
            let mut buf = ScalarBuffer::new();
            let _ = write!(buf, "{}", value);
            self.text(buf.as_str())?;
            return Ok(true);
        }

        Ok(false)
    }

    /// Write a float value. Override to customize float formatting.
    fn write_float(&self, value: f64, buf: &mut ScalarBuffer) {
        let _ = write!(buf, "{}", value);
    }
}

// Blanket implementation for all DomSerializers
impl<T: DomSerializer> WriteScalar for T {}

macro_rules! write_int {
    ($buf:expr, $value:expr, $ty:ty) => {{
        if let Ok(v) = $value.get::<$ty>() {
            let _ = write!($buf, "{}", v);
            true
        } else {
            false
        }
    }};
}
use write_int;

#[cfg(feature = "net")]
macro_rules! write_display {
    ($buf:expr, $value:expr, $ty:ty) => {{
        if let Ok(v) = $value.get::<$ty>() {
            let _ = write!($buf, "{}", v);
            true
        } else {
            false
        }
    }};
}
#[cfg(feature = "net")]
use write_display;

/// Buffer for formatting scalar values without heap allocation for small values.
/// Uses a small inline buffer, falling back to heap for larger values.
pub struct ScalarBuffer {
    // Inline buffer for small values (covers most integers, small floats)
    inline: [u8; 32],
    len: usize,
    // Overflow to heap if needed
    overflow: Option<String>,
}

impl ScalarBuffer {
    /// Create a new empty buffer.
    pub fn new() -> Self {
        Self {
            inline: [0u8; 32],
            len: 0,
            overflow: None,
        }
    }

    /// Get the buffer contents as a string slice.
    pub fn as_str(&self) -> &str {
        if let Some(ref s) = self.overflow {
            s.as_str()
        } else {
            // Safety: we only write valid UTF-8 via fmt::Write
            unsafe { core::str::from_utf8_unchecked(&self.inline[..self.len]) }
        }
    }

    fn push_str(&mut self, s: &str) {
        if let Some(ref mut overflow) = self.overflow {
            overflow.push_str(s);
        } else if self.len + s.len() <= self.inline.len() {
            self.inline[self.len..self.len + s.len()].copy_from_slice(s.as_bytes());
            self.len += s.len();
        } else {
            // Overflow to heap
            let mut heap = String::with_capacity(self.len + s.len() + 32);
            heap.push_str(unsafe { core::str::from_utf8_unchecked(&self.inline[..self.len]) });
            heap.push_str(s);
            self.overflow = Some(heap);
        }
    }

    fn push(&mut self, c: char) {
        let mut buf = [0u8; 4];
        let s = c.encode_utf8(&mut buf);
        self.push_str(s);
    }
}

impl Default for ScalarBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl FmtWrite for ScalarBuffer {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.push_str(s);
        Ok(())
    }
}
