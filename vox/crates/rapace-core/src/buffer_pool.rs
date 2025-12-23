//! Buffer pool for frame payload allocation.
//!
//! This module provides a thread-safe buffer pool using `object-pool` to reduce
//! allocation pressure in high-throughput scenarios. Instead of allocating
//! a fresh `Vec<u8>` for every received frame, buffers are reused from the pool.

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
            inner: reusable,
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
pub struct PooledBuf {
    inner: object_pool::ReusableOwned<Vec<u8>>,
    pool_buffer_size: usize,
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

    /// Get the pool's buffer size (not the current length).
    pub fn pool_buffer_size(&self) -> usize {
        self.pool_buffer_size
    }
}

impl Deref for PooledBuf {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for PooledBuf {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl AsRef<[u8]> for PooledBuf {
    fn as_ref(&self) -> &[u8] {
        self.inner.as_slice()
    }
}

impl std::fmt::Debug for PooledBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PooledBuf")
            .field("len", &self.inner.len())
            .field("capacity", &self.inner.capacity())
            .field("pool_buffer_size", &self.pool_buffer_size)
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
}
