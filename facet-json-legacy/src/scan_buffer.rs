//! Buffer management for streaming JSON scanning.
//!
//! # Design: No Compaction Needed
//!
//! This buffer uses a grow-only strategy that never requires compacting
//! (shifting data to the left). This is possible because:
//!
//! 1. When the scanner returns a complete token, we materialize it immediately.
//!    The raw bytes are no longer needed.
//!
//! 2. When the scanner returns `Eof` (end of buffer, not real EOF), all tokens
//!    in the buffer have been processed. We can **reset** and read fresh data.
//!
//! 3. When the scanner returns `NeedMore` (mid-token), we **grow** the buffer
//!    and read more data into the new space. Indices remain valid.
//!
//! This avoids all data copying, which is both simpler and more efficient.

use alloc::vec::Vec;

/// Default buffer capacity (8KB)
pub const DEFAULT_CAPACITY: usize = 8 * 1024;

/// A refillable buffer for streaming JSON parsing.
///
/// Uses a grow-only strategy: we either reset (when all data processed)
/// or grow (when mid-token). Never compacts/shifts data.
#[derive(Debug)]
pub struct ScanBuffer {
    /// The underlying buffer
    data: Vec<u8>,
    /// How many bytes are valid (filled with data)
    filled: usize,
    /// Whether EOF has been reached on the underlying reader
    eof: bool,
}

impl ScanBuffer {
    /// Create a new buffer with the default capacity.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    /// Create a new buffer with a specific capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: vec![0u8; capacity],
            filled: 0,
            eof: false,
        }
    }

    /// Create a buffer from an existing slice (for slice-based parsing).
    pub fn from_slice(slice: &[u8]) -> Self {
        let mut data = Vec::with_capacity(slice.len());
        data.extend_from_slice(slice);
        Self {
            filled: data.len(),
            data,
            eof: true, // No more data to read
        }
    }

    /// Get the current buffer contents.
    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.data[..self.filled]
    }

    /// Whether the underlying reader has reached EOF.
    #[inline]
    pub fn is_eof(&self) -> bool {
        self.eof
    }

    /// How many bytes are filled.
    #[inline]
    pub fn filled(&self) -> usize {
        self.filled
    }

    /// Get the buffer's total capacity.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.data.len()
    }

    /// Reset the buffer for fresh data.
    ///
    /// Called when all data has been processed (scanner returned Eof but reader has more).
    /// This is NOT compaction - we're simply starting fresh because everything was consumed.
    pub fn reset(&mut self) {
        self.filled = 0;
        // Note: we don't reset eof here - that's determined by the reader
    }

    /// Grow the buffer to make room for more data.
    ///
    /// Called when mid-token (NeedMore) and buffer is full.
    /// We grow rather than compact because:
    /// - No data copying needed
    /// - Scanner indices remain valid
    /// - Simpler logic
    pub fn grow(&mut self) {
        let new_capacity = self.data.len() * 2;
        self.data.resize(new_capacity, 0);
    }

    /// Refill the buffer from a synchronous reader.
    ///
    /// Reads more data into the unfilled portion of the buffer.
    /// Returns the number of bytes read, or 0 if EOF.
    #[cfg(feature = "std")]
    pub fn refill<R: std::io::Read>(&mut self, reader: &mut R) -> std::io::Result<usize> {
        if self.eof {
            return Ok(0);
        }

        let read_buf = &mut self.data[self.filled..];
        if read_buf.is_empty() {
            // Buffer is full - caller should grow() first if needed
            return Ok(0);
        }

        let n = reader.read(read_buf)?;
        self.filled += n;

        if n == 0 {
            self.eof = true;
        }

        Ok(n)
    }

    /// Refill the buffer from an async reader (tokio).
    #[cfg(feature = "tokio")]
    pub async fn refill_tokio<R>(&mut self, reader: &mut R) -> std::io::Result<usize>
    where
        R: tokio::io::AsyncRead + Unpin,
    {
        use tokio::io::AsyncReadExt;

        if self.eof {
            return Ok(0);
        }

        let read_buf = &mut self.data[self.filled..];
        if read_buf.is_empty() {
            return Ok(0);
        }

        let n = reader.read(read_buf).await?;
        self.filled += n;

        if n == 0 {
            self.eof = true;
        }

        Ok(n)
    }

    /// Refill the buffer from an async reader (futures-io).
    #[cfg(feature = "futures-io")]
    pub async fn refill_futures<R>(&mut self, reader: &mut R) -> std::io::Result<usize>
    where
        R: futures_io::AsyncRead + Unpin,
    {
        use core::pin::Pin;
        use core::task::Context;

        if self.eof {
            return Ok(0);
        }

        let read_buf = &mut self.data[self.filled..];
        if read_buf.is_empty() {
            return Ok(0);
        }

        let n = core::future::poll_fn(|cx: &mut Context<'_>| {
            Pin::new(&mut *reader).poll_read(cx, read_buf)
        })
        .await?;
        self.filled += n;

        if n == 0 {
            self.eof = true;
        }

        Ok(n)
    }
}

impl Default for ScanBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_slice() {
        let buf = ScanBuffer::from_slice(b"hello world");
        assert_eq!(buf.data(), b"hello world");
        assert_eq!(buf.filled(), 11);
        assert!(buf.is_eof());
    }

    #[test]
    fn test_reset() {
        let mut buf = ScanBuffer::from_slice(b"hello");
        assert_eq!(buf.filled(), 5);
        buf.reset();
        assert_eq!(buf.filled(), 0);
        assert_eq!(buf.data(), b"");
    }

    #[test]
    fn test_grow() {
        let mut buf = ScanBuffer::with_capacity(4);
        assert_eq!(buf.capacity(), 4);
        buf.grow();
        assert_eq!(buf.capacity(), 8);
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_refill_from_reader() {
        use std::io::Cursor;

        let mut reader = Cursor::new(b"hello world");
        let mut buf = ScanBuffer::with_capacity(8);

        // First read
        let n = buf.refill(&mut reader).unwrap();
        assert_eq!(n, 8);
        assert_eq!(buf.data(), b"hello wo");

        // Buffer full, refill returns 0
        let n = buf.refill(&mut reader).unwrap();
        assert_eq!(n, 0);

        // Grow and refill
        buf.grow();
        let n = buf.refill(&mut reader).unwrap();
        assert_eq!(n, 3);
        assert_eq!(buf.data(), b"hello world");
        assert!(!buf.is_eof()); // Not EOF yet - we got 3 bytes

        // One more refill to confirm EOF
        let n = buf.refill(&mut reader).unwrap();
        assert_eq!(n, 0);
        assert!(buf.is_eof());
    }
}
