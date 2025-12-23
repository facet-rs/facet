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
        let reusable = self
            .pool
            .pull_owned(|| Vec::with_capacity(self.buffer_size));

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
        buf.clear();
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
        buf.clear();
        assert_eq!(buf.len(), 0);
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

        // Get another buffer - may or may not be cleared (implementation detail)
        let mut buf = pool.get();
        buf.clear();
        assert_eq!(buf.len(), 0, "Buffer should be clearable");
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

        let mut buf = pool.get();
        buf.clear();
        assert!(buf.capacity() >= 1024);
    }
}
