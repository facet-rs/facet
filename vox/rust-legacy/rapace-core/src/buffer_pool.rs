//! Buffer pool for frame payload allocation.
//!
//! This module provides a thread-safe buffer pool using `object-pool` to reduce
//! allocation pressure in high-throughput scenarios. Instead of allocating
//! a fresh `Vec<u8>` for every received frame, buffers are reused from the pool.

// SAFETY: This module contains unsafe code required by the `bytes::BufMut` trait.
//
// The unsafe operations are:
// 1. `unsafe impl BufMut for PooledBuf` - trait requires unsafe impl
// 2. `advance_mut` - calls `Vec::set_len` after caller writes to uninitialized memory
// 3. `chunk_mut` - returns `UninitSlice` pointing to Vec's spare capacity
//
// These are safe because:
// - We only expose uninitialized memory that the Vec has already allocated
// - `advance_mut` is only called after the caller has initialized the bytes
// - The Vec's capacity/length invariants are maintained
#![allow(unsafe_code)]

use bytes::BufMut;
use bytes::buf::UninitSlice;
use object_pool::Pool;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

/// Default buffer size for pooled allocations (64KB).
///
/// This size is chosen to accommodate most RPC frame payloads while avoiding
/// excessive memory waste. Larger frames will fall back to heap allocation.
const DEFAULT_BUFFER_SIZE: usize = 64 * 1024;

/// Default pool capacity (number of buffers to keep in the pool).
const DEFAULT_POOL_CAPACITY: usize = 128;

/// A buffer pool for frame payloads.
///
/// This pool pre-allocates a set of reusable buffers to avoid per-frame
/// allocation overhead. Buffers are automatically returned to the pool when
/// dropped.
#[derive(Clone)]
pub struct BufferPool {
    pool: Arc<Pool<Vec<u8>>>,
    buffer_size: usize,
}

impl std::fmt::Debug for BufferPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BufferPool")
            .field("buffer_size", &self.buffer_size)
            .finish_non_exhaustive()
    }
}

impl BufferPool {
    /// Create a new buffer pool with default settings.
    ///
    /// - Buffer size: 64KB
    /// - Pool capacity: 128 buffers
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_POOL_CAPACITY, DEFAULT_BUFFER_SIZE)
    }

    /// Create a buffer pool with custom capacity and buffer size.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Number of buffers to maintain in the pool
    /// * `buffer_size` - Size of each buffer in bytes
    pub fn with_capacity(capacity: usize, buffer_size: usize) -> Self {
        let pool = Pool::new(capacity, move || Vec::with_capacity(buffer_size));
        Self {
            pool: Arc::new(pool),
            buffer_size,
        }
    }

    /// Get a buffer from the pool.
    ///
    /// Returns a [`PooledBuf`] that will automatically return to the pool
    /// when dropped. The buffer will be empty but pre-allocated to the
    /// pool's buffer size.
    pub fn get(&self) -> PooledBuf {
        let mut reusable = self
            .pool
            .pull_owned(|| Vec::with_capacity(self.buffer_size));

        // SAFETY: Clear the buffer to prevent data leakage from previous uses.
        // The object-pool crate returns buffers in whatever state they were dropped,
        // so we must explicitly clear to ensure the buffer starts empty.
        reusable.clear();

        PooledBuf {
            inner: PooledBufInner::Pooled(reusable),
            pool_buffer_size: self.buffer_size,
        }
    }

    /// Get the configured buffer size for this pool.
    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }
}

impl Default for BufferPool {
    fn default() -> Self {
        Self::new()
    }
}

/// A pooled buffer that automatically returns to the pool when dropped.
///
/// This type wraps an `object-pool` `Reusable` and provides transparent access to the
/// underlying `Vec<u8>` through `Deref` and `DerefMut` traits.
///
/// For oversized data that exceeds the pool's buffer size, this type can hold an
/// unpooled `Vec<u8>` directly, avoiding unnecessary copying and pool pressure.
pub struct PooledBuf {
    inner: PooledBufInner,
    pool_buffer_size: usize,
}

enum PooledBufInner {
    /// Normal case: buffer from the pool
    Pooled(object_pool::ReusableOwned<Vec<u8>>),
    /// Overflow case: unpooled Vec for oversized data
    Unpooled(Vec<u8>),
}

impl PooledBuf {
    /// Create a pooled buffer from raw data.
    ///
    /// The data will be copied into a pooled buffer from the given pool.
    pub fn from_slice(pool: &BufferPool, data: &[u8]) -> Self {
        let mut buf = pool.get();
        debug_assert_eq!(buf.len(), 0, "pool.get() should return empty buffer");
        buf.extend_from_slice(data);
        buf
    }

    /// Create an unpooled buffer from a Vec.
    ///
    /// This is used for oversized data that exceeds the pool's buffer size.
    /// The Vec will not be returned to the pool when dropped.
    pub fn from_vec_unpooled(vec: Vec<u8>, pool_buffer_size: usize) -> Self {
        Self {
            inner: PooledBufInner::Unpooled(vec),
            pool_buffer_size,
        }
    }

    /// Get the pool's buffer size (not the current length).
    pub fn pool_buffer_size(&self) -> usize {
        self.pool_buffer_size
    }

    /// Convert this pooled buffer into a `Bytes` with zero-copy.
    ///
    /// The returned `Bytes` holds a reference to the pooled buffer's data. When the `Bytes`
    /// is dropped, the underlying buffer is automatically returned to the pool.
    ///
    /// This is a zero-copy operation - the buffer is not cloned, only the ownership
    /// is transferred to a ref-counted wrapper.
    pub fn into_bytes(self) -> bytes::Bytes {
        // Wrap the PooledBuf in an Arc and then in PooledBufOwner for AsRef<[u8]>.
        // When all Bytes clones are dropped, the Arc refcount reaches 0, dropping the
        // PooledBuf and returning the buffer to the pool.
        bytes::Bytes::from_owner(PooledBufOwner(Arc::new(self)))
    }

    /// Returns true if this buffer is using pool storage, false if unpooled.
    pub fn is_pooled(&self) -> bool {
        matches!(self.inner, PooledBufInner::Pooled(_))
    }
}

impl Deref for PooledBuf {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        match &self.inner {
            PooledBufInner::Pooled(buf) => buf,
            PooledBufInner::Unpooled(vec) => vec,
        }
    }
}

impl DerefMut for PooledBuf {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match &mut self.inner {
            PooledBufInner::Pooled(buf) => buf,
            PooledBufInner::Unpooled(vec) => vec,
        }
    }
}

impl AsRef<[u8]> for PooledBuf {
    fn as_ref(&self) -> &[u8] {
        match &self.inner {
            PooledBufInner::Pooled(buf) => buf.as_slice(),
            PooledBufInner::Unpooled(vec) => vec.as_slice(),
        }
    }
}

/// `BufMut` implementation enables zero-copy reads via `tokio::io::AsyncReadExt::read_buf`.
///
/// Instead of reading into a temporary buffer and copying:
/// ```ignore
/// let mut buf = vec![0u8; 4096];
/// let n = reader.read(&mut buf).await?;
/// pooled.extend_from_slice(&buf[..n]);  // extra copy!
/// ```
///
/// You can read directly into the pooled buffer:
/// ```ignore
/// let mut pooled = pool.get();
/// reader.read_buf(&mut pooled).await?;  // zero-copy!
/// ```
///
/// # Safety
///
/// This implementation is safe because:
/// - `chunk_mut` returns valid spare capacity from the underlying `Vec`
/// - `advance_mut` only sets length to previously written bytes
/// - The `Vec` ensures memory is properly allocated before we expose it
unsafe impl BufMut for PooledBuf {
    fn remaining_mut(&self) -> usize {
        // Return spare capacity. BufMut users should call reserve() if they need more.
        self.capacity().saturating_sub(self.len())
    }

    unsafe fn advance_mut(&mut self, cnt: usize) {
        let new_len = self.len() + cnt;
        debug_assert!(
            new_len <= self.capacity(),
            "advance_mut: new_len {} exceeds capacity {}",
            new_len,
            self.capacity()
        );
        let vec: &mut Vec<u8> = &mut *self;
        // SAFETY: Caller guarantees that `cnt` bytes have been initialized
        unsafe {
            vec.set_len(new_len);
        }
    }

    fn chunk_mut(&mut self) -> &mut UninitSlice {
        // Read pool_buffer_size before borrowing self mutably
        let pool_buffer_size = self.pool_buffer_size;
        let vec: &mut Vec<u8> = &mut *self;

        // If no spare capacity, reserve more space
        if vec.len() == vec.capacity() {
            vec.reserve(pool_buffer_size);
        }

        let cap = vec.capacity();
        let len = vec.len();

        // SAFETY: We're returning a slice of uninitialized but allocated memory.
        // The Vec guarantees this memory is valid for writes.
        unsafe {
            let ptr = vec.as_mut_ptr().add(len);
            UninitSlice::from_raw_parts_mut(ptr, cap - len)
        }
    }
}

/// Wrapper for Arc<PooledBuf> that implements AsRef<[u8]> for use with Bytes::from_owner.
struct PooledBufOwner(Arc<PooledBuf>);

impl AsRef<[u8]> for PooledBufOwner {
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

impl std::fmt::Debug for PooledBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (len, capacity, is_pooled) = match &self.inner {
            PooledBufInner::Pooled(buf) => (buf.len(), buf.capacity(), true),
            PooledBufInner::Unpooled(vec) => (vec.len(), vec.capacity(), false),
        };
        f.debug_struct("PooledBuf")
            .field("len", &len)
            .field("capacity", &capacity)
            .field("pool_buffer_size", &self.pool_buffer_size)
            .field("is_pooled", &is_pooled)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_pool_basic() {
        let pool = BufferPool::new();
        let mut buf = pool.get();
        assert_eq!(buf.len(), 0, "pool.get() must return empty buffer");
        assert!(buf.capacity() >= DEFAULT_BUFFER_SIZE);

        buf.extend_from_slice(b"hello world");
        assert_eq!(&buf[..], b"hello world");
    }

    #[test]
    fn test_buffer_reuse() {
        let pool = BufferPool::new();

        // Allocate and drop a buffer
        {
            let mut buf = pool.get();
            buf.clear();
            buf.extend_from_slice(b"test data");
            assert_eq!(buf.len(), 9);
        }

        // Get another buffer - should always be cleared by the pool
        let buf = pool.get();
        assert_eq!(buf.len(), 0, "Buffer should be empty after get()");
        assert!(buf.capacity() >= DEFAULT_BUFFER_SIZE);
    }

    #[test]
    fn test_from_slice() {
        let pool = BufferPool::new();
        let data = b"test payload";
        let buf = PooledBuf::from_slice(&pool, data);

        assert_eq!(&buf[..], data);
        assert!(buf.capacity() >= DEFAULT_BUFFER_SIZE);
    }

    #[test]
    fn test_custom_capacity() {
        let pool = BufferPool::with_capacity(10, 1024);
        assert_eq!(pool.buffer_size(), 1024);

        let buf = pool.get();
        assert_eq!(buf.len(), 0, "pool.get() must return empty buffer");
        assert!(buf.capacity() >= 1024);
    }

    #[test]
    fn test_no_data_leakage() {
        let pool = BufferPool::new();

        // Allocate a buffer and fill it with sensitive data
        {
            let mut buf1 = pool.get();
            buf1.extend_from_slice(b"sensitive secret data");
            assert_eq!(buf1.len(), 21);
            // buf1 is dropped here, returned to pool
        }

        // Get a new buffer - it should be empty, not contain old data
        let buf2 = pool.get();
        assert_eq!(buf2.len(), 0, "New buffer should be empty");

        // Verify extending works correctly on the empty buffer
        let mut buf3 = buf2;
        buf3.extend_from_slice(b"new");
        assert_eq!(&buf3[..], b"new");
        assert_eq!(buf3.len(), 3, "Should only contain new data, not old data");
    }

    #[test]
    fn test_into_bytes_zero_copy() {
        let pool = BufferPool::new();
        let mut buf = pool.get();
        buf.extend_from_slice(b"hello world");

        // Convert to Bytes
        let bytes = buf.into_bytes();
        assert_eq!(&bytes[..], b"hello world");

        // Cloning Bytes is cheap (ref-counted)
        let bytes2 = bytes.clone();
        assert_eq!(&bytes2[..], b"hello world");

        // Both Bytes point to the same underlying data
        assert_eq!(bytes.as_ptr(), bytes2.as_ptr());
    }

    #[test]
    fn test_into_bytes_pool_return() {
        // Create a pool and track its size
        let pool = BufferPool::new();

        // Create a buffer and convert to Bytes
        let bytes = {
            let mut buf = pool.get();
            buf.extend_from_slice(b"test data");
            buf.into_bytes()
        };

        assert_eq!(&bytes[..], b"test data");

        // Clone the Bytes (increments refcount)
        let bytes2 = bytes.clone();

        // Drop the first Bytes (decrements refcount, but buffer not returned yet)
        drop(bytes);
        assert_eq!(&bytes2[..], b"test data");

        // Drop the last Bytes (refcount goes to 0, buffer returns to pool)
        drop(bytes2);

        // Get a new buffer from the pool - should reuse the returned buffer
        let buf2 = pool.get();
        assert_eq!(buf2.len(), 0, "Reused buffer should be empty");
    }

    #[test]
    fn test_buf_mut_basic() {
        use bytes::BufMut;

        let pool = BufferPool::new();
        let mut buf = pool.get();

        // Initially empty with spare capacity
        assert_eq!(buf.len(), 0);
        assert!(buf.remaining_mut() > 0);

        // Use BufMut methods to write data
        buf.put_slice(b"hello ");
        buf.put_slice(b"world");

        assert_eq!(&buf[..], b"hello world");
        assert_eq!(buf.len(), 11);
    }

    #[test]
    fn test_buf_mut_chunk_mut() {
        use bytes::BufMut;

        let pool = BufferPool::new();
        let mut buf = pool.get();

        // Get uninitialized chunk and write to it manually
        let chunk = buf.chunk_mut();
        assert!(chunk.len() >= DEFAULT_BUFFER_SIZE);

        // Write some bytes using write_byte
        chunk.write_byte(0, b'A');
        chunk.write_byte(1, b'B');
        chunk.write_byte(2, b'C');

        // Advance to mark them as initialized
        unsafe {
            buf.advance_mut(3);
        }

        assert_eq!(&buf[..], b"ABC");
    }

    #[test]
    fn test_buf_mut_auto_reserve() {
        use bytes::BufMut;

        // Create a tiny pool to test auto-reserve behavior
        let pool = BufferPool::with_capacity(1, 8);
        let mut buf = pool.get();

        // Fill exactly to capacity
        buf.put_slice(b"12345678");
        assert_eq!(buf.len(), 8);

        // chunk_mut should auto-reserve when at capacity
        let chunk = buf.chunk_mut();
        assert!(chunk.len() > 0, "chunk_mut should reserve more space");

        // Write more data
        buf.put_slice(b"more");
        assert_eq!(&buf[..], b"12345678more");
    }
}
