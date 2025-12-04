//! A `no_std` compatible `Write` trait for serialization.
//!
//! This trait is used by facet serializers (like `facet-json`) to write output
//! without depending on `std::io::Write`, enabling `no_std` support.

#[cfg(feature = "alloc")]
use alloc::vec::Vec;

/// A `no_std` compatible write trait used by facet serializers.
///
/// This trait provides a simple interface for writing bytes, designed to work
/// in `no_std` environments while remaining compatible with standard library types.
pub trait Write {
    /// Write all bytes from the buffer to the writer.
    fn write(&mut self, buf: &[u8]);

    /// If the writer supports it, reserve space for `additional` bytes.
    ///
    /// This is an optimization hint and may be ignored by implementations.
    fn reserve(&mut self, additional: usize);
}

#[cfg(feature = "alloc")]
impl Write for Vec<u8> {
    #[inline]
    fn write(&mut self, buf: &[u8]) {
        self.extend_from_slice(buf);
    }

    #[inline]
    fn reserve(&mut self, additional: usize) {
        Vec::reserve(self, additional);
    }
}

#[cfg(feature = "alloc")]
impl Write for &mut Vec<u8> {
    #[inline]
    fn write(&mut self, buf: &[u8]) {
        self.extend_from_slice(buf);
    }

    #[inline]
    fn reserve(&mut self, additional: usize) {
        Vec::reserve(self, additional);
    }
}

#[cfg(feature = "bytes")]
impl Write for bytes::BytesMut {
    #[inline]
    fn write(&mut self, buf: &[u8]) {
        self.extend_from_slice(buf);
    }

    #[inline]
    fn reserve(&mut self, additional: usize) {
        bytes::BytesMut::reserve(self, additional);
    }
}

#[cfg(feature = "bytes")]
impl Write for &mut bytes::BytesMut {
    #[inline]
    fn write(&mut self, buf: &[u8]) {
        self.extend_from_slice(buf);
    }

    #[inline]
    fn reserve(&mut self, additional: usize) {
        bytes::BytesMut::reserve(self, additional);
    }
}
