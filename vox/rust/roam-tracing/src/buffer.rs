//! Lossy bounded buffer for tracing records.
//!
//! Provides a thread-safe, bounded buffer that drops oldest entries
//! when full. This ensures tracing never blocks the application.

use peeps_locks::DiagnosticMutex as Mutex;
use std::collections::VecDeque;
use tokio::sync::Notify;

/// A lossy bounded buffer that drops oldest entries when full.
///
/// This ensures the tracing system never blocks the application
/// and has bounded memory usage.
///
/// # Thread Safety
///
/// The buffer is safe to push from any thread (typically the tracing
/// layer running synchronously) and pop from an async task.
pub struct LossyBuffer<T> {
    inner: Mutex<VecDeque<T>>,
    capacity: usize,
    notify: Notify,
}

impl<T> LossyBuffer<T> {
    /// Create a new lossy buffer with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new("LossyBuffer.inner", VecDeque::with_capacity(capacity)),
            capacity,
            notify: Notify::new(),
        }
    }

    /// Push a record, dropping the oldest if at capacity.
    ///
    /// This never blocks - critical for tracing to not affect app behavior.
    /// If the buffer is full, the oldest record is dropped (lossy).
    pub fn push(&self, item: T) {
        let mut queue = self.inner.lock();
        if queue.len() >= self.capacity {
            queue.pop_front(); // Drop oldest (lossy)
        }
        queue.push_back(item);
        drop(queue);
        self.notify.notify_one();
    }

    /// Pop a record, waiting if empty.
    ///
    /// Returns `None` only if the buffer is permanently closed (which
    /// doesn't happen in this implementation - it waits forever).
    #[allow(dead_code)]
    pub async fn pop(&self) -> Option<T> {
        loop {
            {
                let mut queue = self.inner.lock();
                if let Some(item) = queue.pop_front() {
                    return Some(item);
                }
            }
            self.notify.notified().await;
        }
    }

    /// Try to pop without waiting.
    ///
    /// Returns `None` if the buffer is empty.
    pub fn try_pop(&self) -> Option<T> {
        self.inner.lock().pop_front()
    }

    /// Returns the current number of items in the buffer.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    /// Returns true if the buffer is empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.inner.lock().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_pop() {
        let buffer = LossyBuffer::new(10);
        buffer.push(1);
        buffer.push(2);
        buffer.push(3);

        assert_eq!(buffer.try_pop(), Some(1));
        assert_eq!(buffer.try_pop(), Some(2));
        assert_eq!(buffer.try_pop(), Some(3));
        assert_eq!(buffer.try_pop(), None);
    }

    #[test]
    fn test_lossy_on_overflow() {
        let buffer = LossyBuffer::new(3);
        buffer.push(1);
        buffer.push(2);
        buffer.push(3);
        // Buffer is now full
        buffer.push(4); // Should drop 1
        buffer.push(5); // Should drop 2

        assert_eq!(buffer.try_pop(), Some(3));
        assert_eq!(buffer.try_pop(), Some(4));
        assert_eq!(buffer.try_pop(), Some(5));
        assert_eq!(buffer.try_pop(), None);
    }

    #[test]
    fn test_len() {
        let buffer = LossyBuffer::new(10);
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);

        buffer.push(1);
        buffer.push(2);
        assert_eq!(buffer.len(), 2);
        assert!(!buffer.is_empty());

        buffer.try_pop();
        assert_eq!(buffer.len(), 1);
    }
}
