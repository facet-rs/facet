//! Futex operations for cross-process signaling.
//!
//! Uses Linux futex syscalls for efficient waiting on shared memory.
//! On non-Linux platforms, provides fallback implementations that yield.

use std::sync::atomic::AtomicU32;
#[cfg(target_os = "linux")]
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Futex wait operation.
///
/// Blocks the calling thread if `*futex == expected`, until woken by `futex_wake`
/// or the timeout expires.
///
/// Returns:
/// - `Ok(true)` if woken by `futex_wake`
/// - `Ok(false)` if the value changed (spurious wakeup or value mismatch)
/// - `Err` on syscall error
#[cfg(target_os = "linux")]
pub fn futex_wait(
    futex: &AtomicU32,
    expected: u32,
    timeout: Option<Duration>,
) -> std::io::Result<bool> {
    use std::ptr;

    let futex_ptr = futex as *const AtomicU32 as *const u32;

    let timespec = timeout.map(|d| libc::timespec {
        tv_sec: d.as_secs() as libc::time_t,
        tv_nsec: d.subsec_nanos() as libc::c_long,
    });

    let timespec_ptr = match &timespec {
        Some(ts) => ts as *const libc::timespec,
        None => ptr::null(),
    };

    // FUTEX_WAIT = 0, FUTEX_PRIVATE_FLAG = 128
    // We don't use PRIVATE since we need cross-process signaling
    let result = unsafe {
        libc::syscall(
            libc::SYS_futex,
            futex_ptr,
            libc::FUTEX_WAIT,
            expected,
            timespec_ptr,
            ptr::null::<u32>(),
            0u32,
        )
    };

    if result == 0 {
        Ok(true)
    } else {
        let err = std::io::Error::last_os_error();
        match err.raw_os_error() {
            Some(libc::EAGAIN) => Ok(false),    // Value changed
            Some(libc::ETIMEDOUT) => Ok(false), // Timeout
            Some(libc::EINTR) => Ok(false),     // Interrupted
            _ => Err(err),
        }
    }
}

/// Futex wake operation.
///
/// Wakes up to `count` threads waiting on the futex.
///
/// Returns the number of threads woken.
#[cfg(target_os = "linux")]
pub fn futex_wake(futex: &AtomicU32, count: u32) -> std::io::Result<u32> {
    let futex_ptr = futex as *const AtomicU32 as *const u32;

    let result = unsafe {
        libc::syscall(
            libc::SYS_futex,
            futex_ptr,
            libc::FUTEX_WAKE,
            count,
            std::ptr::null::<libc::timespec>(),
            std::ptr::null::<u32>(),
            0u32,
        )
    };

    if result >= 0 {
        Ok(result as u32)
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// Signal a futex by incrementing and waking waiters.
///
/// This is the common pattern: increment the futex value, then wake all waiters.
#[cfg(target_os = "linux")]
pub fn futex_signal(futex: &AtomicU32) {
    futex.fetch_add(1, Ordering::Release);
    let _ = futex_wake(futex, u32::MAX);
}

/// Wait on a futex with async integration.
///
/// Uses `spawn_blocking` to avoid blocking the tokio runtime.
#[cfg(target_os = "linux")]
pub async fn futex_wait_async(
    futex: &'static AtomicU32,
    timeout: Option<Duration>,
) -> std::io::Result<bool> {
    let current = futex.load(Ordering::Acquire);
    tokio::task::spawn_blocking(move || futex_wait(futex, current, timeout)).await?
}

/// Wait on a futex asynchronously, safe for SHM pointers.
///
/// Uses `spawn_blocking` to avoid blocking the tokio runtime.
/// The futex pointer is passed as usize to satisfy Send bounds.
///
/// # Safety
///
/// The caller must ensure the futex pointer remains valid for the duration
/// of this call (i.e., the SHM mapping must not be unmapped).
#[cfg(target_os = "linux")]
pub async fn futex_wait_async_ptr(
    futex: &AtomicU32,
    timeout: Option<Duration>,
) -> std::io::Result<bool> {
    let current = futex.load(Ordering::Acquire);
    let ptr_val = futex as *const AtomicU32 as usize;

    tokio::task::spawn_blocking(move || {
        // SAFETY: Caller guarantees the SHM mapping outlives this call.
        // We convert back from usize to reconstruct the pointer.
        let futex = unsafe { &*(ptr_val as *const AtomicU32) };
        futex_wait(futex, current, timeout)
    })
    .await?
}

#[cfg(not(target_os = "linux"))]
pub async fn futex_wait_async_ptr(
    _futex: &AtomicU32,
    _timeout: Option<Duration>,
) -> std::io::Result<bool> {
    tokio::task::yield_now().await;
    Ok(false)
}

// Fallback for non-Linux platforms (just yields)
#[cfg(not(target_os = "linux"))]
pub fn futex_wait(
    _futex: &AtomicU32,
    _expected: u32,
    _timeout: Option<Duration>,
) -> std::io::Result<bool> {
    std::thread::yield_now();
    Ok(false)
}

#[cfg(not(target_os = "linux"))]
pub fn futex_wake(_futex: &AtomicU32, _count: u32) -> std::io::Result<u32> {
    Ok(0)
}

#[cfg(not(target_os = "linux"))]
pub fn futex_signal(_futex: &AtomicU32) {}

#[cfg(not(target_os = "linux"))]
pub async fn futex_wait_async(
    _futex: &'static AtomicU32,
    _timeout: Option<Duration>,
) -> std::io::Result<bool> {
    tokio::task::yield_now().await;
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    use std::sync::atomic::Ordering;
    #[cfg(target_os = "linux")]
    use std::sync::Arc;
    #[cfg(target_os = "linux")]
    use std::thread;

    #[test]
    fn test_futex_wake_without_waiters() {
        let futex = AtomicU32::new(0);
        let woken = futex_wake(&futex, 1).unwrap();
        assert_eq!(woken, 0);
    }

    #[test]
    fn test_futex_wait_value_mismatch() {
        let futex = AtomicU32::new(42);
        // Wait with wrong expected value should return immediately
        let result = futex_wait(&futex, 0, Some(Duration::from_millis(10))).unwrap();
        assert!(!result); // Should return false (value mismatch)
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_futex_signal_and_wait() {
        let futex = Arc::new(AtomicU32::new(0));
        let futex_clone = futex.clone();

        let handle = thread::spawn(move || {
            // Wait a bit then signal
            thread::sleep(Duration::from_millis(50));
            futex_signal(&futex_clone);
        });

        let initial = futex.load(Ordering::Acquire);
        let result = futex_wait(&futex, initial, Some(Duration::from_secs(1))).unwrap();
        // Either woken or value changed
        assert!(result || futex.load(Ordering::Acquire) != initial);

        handle.join().unwrap();
    }
}
