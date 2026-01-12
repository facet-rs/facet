//! Blocking wait utilities for ring buffers and slot pools.
//!
//! Uses futex (Linux) or polling fallback (other platforms) for efficient
//! cross-process waiting on shared memory.
//!
//! shm[impl shm.wakeup.fallback]

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use shm_primitives::{futex_wait, futex_wake};

/// Error from wait operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitError {
    /// Timeout expired before condition was met
    Timeout,
}

impl std::fmt::Display for WaitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WaitError::Timeout => write!(f, "wait timeout"),
        }
    }
}

impl std::error::Error for WaitError {}

/// Wait until the ring has space for a new descriptor.
///
/// This is used by producers (senders) to block until there's room in the ring.
///
/// shm[impl shm.wakeup.producer-wait]
pub fn wait_for_ring_space(
    head: &AtomicU32,
    tail: &AtomicU32,
    ring_size: u32,
    timeout: Option<Duration>,
) -> Result<(), WaitError> {
    let deadline = timeout.map(|t| std::time::Instant::now() + t);

    loop {
        let h = head.load(Ordering::Acquire);
        let t = tail.load(Ordering::Acquire);

        // Ring has space when (head - tail) < (ring_size - 1)
        // We leave one slot empty to distinguish full from empty
        if h.wrapping_sub(t) < ring_size - 1 {
            return Ok(()); // Has space
        }

        // Calculate remaining timeout
        let remaining = if let Some(deadline) = deadline {
            let now = std::time::Instant::now();
            if now >= deadline {
                return Err(WaitError::Timeout);
            }
            Some(deadline - now)
        } else {
            None
        };

        // Wait for tail to change (consumer increments tail)
        let _ = futex_wait(tail, t, remaining);

        // Check timeout after waking
        if let Some(deadline) = deadline
            && std::time::Instant::now() >= deadline
        {
            return Err(WaitError::Timeout);
        }
    }
}

/// Wait until the ring has data to consume.
///
/// This is used by consumers (receivers) to block until there's a message.
///
/// shm[impl shm.wakeup.consumer-wait]
pub fn wait_for_ring_data(
    head: &AtomicU32,
    tail: &AtomicU32,
    timeout: Option<Duration>,
) -> Result<(), WaitError> {
    let deadline = timeout.map(|t| std::time::Instant::now() + t);

    loop {
        let h = head.load(Ordering::Acquire);
        let t = tail.load(Ordering::Acquire);

        // Ring has data when head != tail
        if h != t {
            return Ok(()); // Has data
        }

        // Calculate remaining timeout
        let remaining = if let Some(deadline) = deadline {
            let now = std::time::Instant::now();
            if now >= deadline {
                return Err(WaitError::Timeout);
            }
            Some(deadline - now)
        } else {
            None
        };

        // Wait for head to change (producer increments head)
        let _ = futex_wait(head, h, remaining);

        // Check timeout after waking
        if let Some(deadline) = deadline
            && std::time::Instant::now() >= deadline
        {
            return Err(WaitError::Timeout);
        }
    }
}

/// Wake consumers waiting for ring data.
///
/// Call this after publishing a new message (incrementing head).
pub fn wake_ring_consumers(head: &AtomicU32) {
    let _ = futex_wake(head, u32::MAX);
}

/// Wake producers waiting for ring space.
///
/// Call this after consuming a message (incrementing tail).
pub fn wake_ring_producers(tail: &AtomicU32) {
    let _ = futex_wake(tail, u32::MAX);
}

/// Wait until credit is available.
///
/// shm[impl shm.wakeup.credit-wait]
pub fn wait_for_credit(
    granted_total: &AtomicU32,
    sent_total: u32,
    needed: u32,
    timeout: Option<Duration>,
) -> Result<(), WaitError> {
    let deadline = timeout.map(|t| std::time::Instant::now() + t);

    loop {
        let granted = granted_total.load(Ordering::Acquire);
        let remaining_credit = granted.wrapping_sub(sent_total);

        if remaining_credit >= needed {
            return Ok(());
        }

        // Calculate remaining timeout
        let remaining = if let Some(deadline) = deadline {
            let now = std::time::Instant::now();
            if now >= deadline {
                return Err(WaitError::Timeout);
            }
            Some(deadline - now)
        } else {
            None
        };

        // Wait for granted_total to change
        let _ = futex_wait(granted_total, granted, remaining);

        // Check timeout after waking
        if let Some(deadline) = deadline
            && std::time::Instant::now() >= deadline
        {
            return Err(WaitError::Timeout);
        }
    }
}

/// Wake senders waiting for credit.
///
/// Call this after granting credit.
pub fn wake_credit_waiters(granted_total: &AtomicU32) {
    let _ = futex_wake(granted_total, u32::MAX);
}

/// Wait until a slot becomes available in the pool.
///
/// This waits on a designated futex word that gets signaled when slots are freed.
/// The `futex_word` should be the first word of the bitmap or a dedicated counter.
///
/// shm[impl shm.wakeup.slot-wait]
pub fn wait_for_slot<F>(
    futex_word: &AtomicU32,
    try_alloc: F,
    timeout: Option<Duration>,
) -> Result<(), WaitError>
where
    F: Fn() -> bool,
{
    let deadline = timeout.map(|t| std::time::Instant::now() + t);

    loop {
        // Try to allocate first
        if try_alloc() {
            return Ok(());
        }

        let current = futex_word.load(Ordering::Acquire);

        // Calculate remaining timeout
        let remaining = if let Some(deadline) = deadline {
            let now = std::time::Instant::now();
            if now >= deadline {
                return Err(WaitError::Timeout);
            }
            Some(deadline - now)
        } else {
            None
        };

        // Wait for the futex word to change
        let _ = futex_wait(futex_word, current, remaining);

        // Check timeout after waking
        if let Some(deadline) = deadline
            && std::time::Instant::now() >= deadline
        {
            return Err(WaitError::Timeout);
        }
    }
}

/// Wake waiters blocked on slot availability.
///
/// Call this after freeing a slot.
pub fn wake_slot_waiters(futex_word: &AtomicU32) {
    let _ = futex_wake(futex_word, 1); // Wake one waiter
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_ring_space_available() {
        let head = AtomicU32::new(0);
        let tail = AtomicU32::new(0);

        // Ring is empty, should have space immediately
        let result = wait_for_ring_space(&head, &tail, 8, Some(Duration::from_millis(10)));
        assert!(result.is_ok());
    }

    #[test]
    fn test_ring_data_timeout() {
        let head = AtomicU32::new(0);
        let tail = AtomicU32::new(0);

        // Ring is empty, should timeout
        let result = wait_for_ring_data(&head, &tail, Some(Duration::from_millis(10)));
        assert_eq!(result, Err(WaitError::Timeout));
    }

    #[test]
    fn test_ring_producer_consumer() {
        let head = Arc::new(AtomicU32::new(0));
        let tail = Arc::new(AtomicU32::new(0));

        let head2 = head.clone();
        let tail2 = tail.clone();

        // Consumer thread waits for data
        let consumer = thread::spawn(move || {
            wait_for_ring_data(&head2, &tail2, Some(Duration::from_secs(5))).unwrap();
            head2.load(Ordering::Acquire)
        });

        // Give consumer time to start waiting
        thread::sleep(Duration::from_millis(50));

        // Producer publishes
        head.store(1, Ordering::Release);
        wake_ring_consumers(&head);

        let result = consumer.join().unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn test_credit_available() {
        let granted = AtomicU32::new(1000);

        // Have plenty of credit
        let result = wait_for_credit(&granted, 0, 100, Some(Duration::from_millis(10)));
        assert!(result.is_ok());
    }

    #[test]
    fn test_credit_timeout() {
        let granted = AtomicU32::new(0);

        // No credit, should timeout
        let result = wait_for_credit(&granted, 0, 100, Some(Duration::from_millis(10)));
        assert_eq!(result, Err(WaitError::Timeout));
    }
}
