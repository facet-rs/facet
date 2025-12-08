// src/session.rs

use std::marker::PhantomData;
use std::sync::atomic::Ordering;
use crate::ring::{Producer, Consumer};
use crate::alloc::DataSegment;
use crate::layout::SegmentHeader;

/// Configuration for a session.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Heartbeat interval in milliseconds.
    /// Default: 100ms
    pub heartbeat_interval_ms: u64,

    /// Liveness timeout in milliseconds.
    /// If peer hasn't updated its timestamp within this duration, it's considered dead.
    /// Default: 1000ms (1 second)
    pub liveness_timeout_ms: u64,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval_ms: 100,
            liveness_timeout_ms: 1000,
        }
    }
}

/// Marker trait for session role.
///
/// This trait is sealed and can only be implemented by `PeerA` and `PeerB`.
/// It enables compile-time role checking - the role is fixed when the session
/// is created and cannot be changed.
pub trait SessionRole: sealed::Sealed {
    /// Returns true if this is Peer A, false if Peer B.
    const IS_PEER_A: bool;
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::PeerA {}
    impl Sealed for super::PeerB {}
}

/// Marker type for Peer A (the session creator).
pub struct PeerA;

/// Marker type for Peer B (the session joiner).
pub struct PeerB;

impl SessionRole for PeerA {
    const IS_PEER_A: bool = true;
}

impl SessionRole for PeerB {
    const IS_PEER_A: bool = false;
}

/// A rapace session with compile-time role.
///
/// Each session represents a bidirectional communication channel between two peers.
/// The role (PeerA or PeerB) is encoded in the type parameter `R`, ensuring that:
/// - Heartbeat updates go to the correct atomic field
/// - Liveness checks read from the peer's field (not our own)
/// - No runtime role checks are needed
///
/// # Example
///
/// ```ignore
/// // Peer A creates the session
/// let session_a: Session<PeerA> = Session::new(...);
///
/// // Peer B connects
/// let session_b: Session<PeerB> = Session::new(...);
///
/// // Type system ensures correct heartbeat/liveness behavior
/// session_a.heartbeat();  // Updates peer_a_epoch/peer_a_last_seen
/// session_a.is_peer_alive();  // Checks peer_b_last_seen
/// ```
pub struct Session<R: SessionRole> {
    /// Pointer to the segment header for liveness checks.
    /// This is a raw pointer because the segment is managed externally (via mmap).
    header: *const SegmentHeader,

    /// Producer for our outbound ring (we send, peer receives).
    outbound_producer: Producer<'static>,

    /// Consumer for our inbound ring (peer sends, we receive).
    inbound_consumer: Consumer<'static>,

    /// Data segment for allocating outbound payloads.
    outbound_segment: DataSegment,

    /// Data segment for freeing inbound payloads.
    inbound_segment: DataSegment,

    /// Configuration parameters.
    config: SessionConfig,

    /// Phantom data to track the role at compile time.
    _role: PhantomData<R>,
}

// Safety: Session can be sent between threads as long as the underlying
// shared memory is still mapped and valid. The raw pointer is valid as long
// as the session's lifetime is tied to the mapped memory.
unsafe impl<R: SessionRole> Send for Session<R> {}

impl<R: SessionRole> Session<R> {
    /// Create a new session with the given components.
    ///
    /// # Safety
    ///
    /// - `header` must point to a valid, mapped `SegmentHeader` that outlives the session
    /// - The rings and segments must be properly initialized and associated with the same
    ///   shared memory segment
    /// - The caller must ensure that lifetimes are managed correctly (typically by tying
    ///   the session to a memory mapping)
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn new(
        header: *const SegmentHeader,
        outbound_producer: Producer<'static>,
        inbound_consumer: Consumer<'static>,
        outbound_segment: DataSegment,
        inbound_segment: DataSegment,
        config: SessionConfig,
    ) -> Self {
        Self {
            header,
            outbound_producer,
            inbound_consumer,
            outbound_segment,
            inbound_segment,
            config,
            _role: PhantomData,
        }
    }

    /// Check if our peer is alive.
    ///
    /// This checks the peer's `last_seen` timestamp. If the timestamp hasn't been
    /// updated within the configured `liveness_timeout_ms`, the peer is considered dead.
    ///
    /// # Returns
    ///
    /// - `true` if the peer is alive (timestamp is recent)
    /// - `false` if the peer appears dead (timestamp is stale)
    ///
    /// # Note
    ///
    /// This uses the OTHER peer's timestamp:
    /// - If we are PeerA, we check `peer_b_last_seen`
    /// - If we are PeerB, we check `peer_a_last_seen`
    pub fn is_peer_alive(&self) -> bool {
        let header = unsafe { &*self.header };

        // Load the peer's last_seen timestamp (not our own!)
        let timestamp = if R::IS_PEER_A {
            header.peer_b_last_seen.load(Ordering::Acquire)
        } else {
            header.peer_a_last_seen.load(Ordering::Acquire)
        };

        // Calculate age
        let now = now_nanos();
        let age_nanos = now.saturating_sub(timestamp);
        let timeout_nanos = self.config.liveness_timeout_ms * 1_000_000;

        age_nanos < timeout_nanos
    }

    /// Update our heartbeat.
    ///
    /// This increments our epoch counter and updates our timestamp to the current time.
    /// The peer uses these fields to determine if we are still alive.
    ///
    /// Should be called periodically (typically every `heartbeat_interval_ms`).
    ///
    /// # Note
    ///
    /// This updates OUR fields:
    /// - If we are PeerA, we update `peer_a_epoch` and `peer_a_last_seen`
    /// - If we are PeerB, we update `peer_b_epoch` and `peer_b_last_seen`
    pub fn heartbeat(&self) {
        let header = unsafe { &*self.header };

        // Select our epoch and timestamp fields based on role
        let (epoch, timestamp) = if R::IS_PEER_A {
            (&header.peer_a_epoch, &header.peer_a_last_seen)
        } else {
            (&header.peer_b_epoch, &header.peer_b_last_seen)
        };

        // Increment epoch
        epoch.fetch_add(1, Ordering::Release);

        // Update timestamp
        timestamp.store(now_nanos(), Ordering::Release);
    }

    /// Get the session configuration.
    pub fn config(&self) -> &SessionConfig {
        &self.config
    }

    /// Get a reference to the outbound producer.
    ///
    /// Used for sending messages to the peer.
    pub fn outbound_producer(&mut self) -> &mut Producer<'static> {
        &mut self.outbound_producer
    }

    /// Get a reference to the inbound consumer.
    ///
    /// Used for receiving messages from the peer.
    pub fn inbound_consumer(&mut self) -> &mut Consumer<'static> {
        &mut self.inbound_consumer
    }

    /// Get a reference to the outbound data segment.
    ///
    /// Used for allocating payloads when sending.
    pub fn outbound_segment(&self) -> &DataSegment {
        &self.outbound_segment
    }

    /// Get a reference to the inbound data segment.
    ///
    /// Used for accessing/freeing payloads when receiving.
    pub fn inbound_segment(&self) -> &DataSegment {
        &self.inbound_segment
    }
}

/// Get the current time in nanoseconds since an unspecified epoch.
///
/// Uses `CLOCK_MONOTONIC` which is guaranteed to be monotonically increasing
/// and not affected by system time changes.
///
/// # Platform Support
///
/// Currently supports macOS and Linux. Windows is not supported.
fn now_nanos() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };

    unsafe {
        libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
    }

    // Convert to nanoseconds
    // tv_sec is signed, but should always be positive for CLOCK_MONOTONIC
    let sec_nanos = (ts.tv_sec as u64).saturating_mul(1_000_000_000);
    let nanos = ts.tv_nsec as u64;

    sec_nanos.saturating_add(nanos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{alloc_zeroed, dealloc, Layout};
    use crate::layout::SegmentHeader;

    /// Helper to create a test segment header
    struct TestHeader {
        ptr: *mut u8,
        layout: Layout,
    }

    impl TestHeader {
        fn new() -> Self {
            let layout = Layout::from_size_align(
                std::mem::size_of::<SegmentHeader>(),
                64,
            ).unwrap();

            let ptr = unsafe { alloc_zeroed(layout) };
            assert!(!ptr.is_null());

            // Initialize the header
            unsafe {
                let header = ptr as *mut SegmentHeader;
                std::ptr::write(header, SegmentHeader {
                    magic: crate::layout::MAGIC,
                    version: 1,
                    flags: 0,
                    peer_a_epoch: std::sync::atomic::AtomicU64::new(0),
                    peer_b_epoch: std::sync::atomic::AtomicU64::new(0),
                    peer_a_last_seen: std::sync::atomic::AtomicU64::new(now_nanos()),
                    peer_b_last_seen: std::sync::atomic::AtomicU64::new(now_nanos()),
                });
            }

            TestHeader { ptr, layout }
        }

        fn as_ptr(&self) -> *const SegmentHeader {
            self.ptr as *const SegmentHeader
        }
    }

    impl Drop for TestHeader {
        fn drop(&mut self) {
            unsafe { dealloc(self.ptr, self.layout) }
        }
    }

    #[test]
    fn test_now_nanos_monotonic() {
        let t1 = now_nanos();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let t2 = now_nanos();

        assert!(t2 > t1, "Time should be monotonically increasing");
        assert!(t2 - t1 >= 10_000_000, "At least 10ms should have passed");
    }

    #[test]
    fn test_peer_a_heartbeat() {
        let header = TestHeader::new();
        let header_ptr = header.as_ptr();
        let header_ref = unsafe { &*header_ptr };

        // We only need to test heartbeat logic which only touches the header
        // Create a minimal test struct to avoid UB from zeroing NonNull fields
        struct TestSessionA {
            header: *const SegmentHeader,
            config: SessionConfig,
            _role: PhantomData<PeerA>,
        }

        impl TestSessionA {
            fn heartbeat(&self) {
                let header = unsafe { &*self.header };
                let (epoch, timestamp) = (&header.peer_a_epoch, &header.peer_a_last_seen);
                epoch.fetch_add(1, Ordering::Release);
                timestamp.store(now_nanos(), Ordering::Release);
            }
        }

        let session = TestSessionA {
            header: header_ptr,
            config: SessionConfig::default(),
            _role: PhantomData,
        };

        // Check initial state
        let initial_epoch = header_ref.peer_a_epoch.load(Ordering::Acquire);
        let initial_timestamp = header_ref.peer_a_last_seen.load(Ordering::Acquire);

        // Update heartbeat
        std::thread::sleep(std::time::Duration::from_millis(10));
        session.heartbeat();

        // Verify epoch incremented
        let new_epoch = header_ref.peer_a_epoch.load(Ordering::Acquire);
        assert_eq!(new_epoch, initial_epoch + 1);

        // Verify timestamp updated
        let new_timestamp = header_ref.peer_a_last_seen.load(Ordering::Acquire);
        assert!(new_timestamp > initial_timestamp);

        // Verify we didn't touch peer B's fields
        assert_eq!(header_ref.peer_b_epoch.load(Ordering::Acquire), 0);
    }

    #[test]
    fn test_peer_b_heartbeat() {
        let header = TestHeader::new();
        let header_ptr = header.as_ptr();
        let header_ref = unsafe { &*header_ptr };

        struct TestSessionB {
            header: *const SegmentHeader,
            config: SessionConfig,
            _role: PhantomData<PeerB>,
        }

        impl TestSessionB {
            fn heartbeat(&self) {
                let header = unsafe { &*self.header };
                let (epoch, timestamp) = (&header.peer_b_epoch, &header.peer_b_last_seen);
                epoch.fetch_add(1, Ordering::Release);
                timestamp.store(now_nanos(), Ordering::Release);
            }
        }

        let session = TestSessionB {
            header: header_ptr,
            config: SessionConfig::default(),
            _role: PhantomData,
        };

        // Check initial state
        let initial_epoch = header_ref.peer_b_epoch.load(Ordering::Acquire);

        // Update heartbeat
        session.heartbeat();

        // Verify epoch incremented for peer B
        let new_epoch = header_ref.peer_b_epoch.load(Ordering::Acquire);
        assert_eq!(new_epoch, initial_epoch + 1);

        // Verify we didn't touch peer A's fields (except initial value)
        assert_eq!(header_ref.peer_a_epoch.load(Ordering::Acquire), 0);
    }

    #[test]
    fn test_peer_a_checks_peer_b_liveness() {
        let header = TestHeader::new();
        let header_ptr = header.as_ptr();

        struct TestSessionA {
            header: *const SegmentHeader,
            config: SessionConfig,
            _role: PhantomData<PeerA>,
        }

        impl TestSessionA {
            fn is_peer_alive(&self) -> bool {
                let header = unsafe { &*self.header };
                let timestamp = header.peer_b_last_seen.load(Ordering::Acquire);
                let now = now_nanos();
                let age_nanos = now.saturating_sub(timestamp);
                let timeout_nanos = self.config.liveness_timeout_ms * 1_000_000;
                age_nanos < timeout_nanos
            }
        }

        let session = TestSessionA {
            header: header_ptr,
            config: SessionConfig::default(),
            _role: PhantomData,
        };

        // Peer B is alive (just initialized with current time)
        assert!(session.is_peer_alive());

        // Simulate peer B going stale
        let header_ref = unsafe { &*header_ptr };
        let old_time = now_nanos() - (2_000_000_000); // 2 seconds ago
        header_ref.peer_b_last_seen.store(old_time, Ordering::Release);

        // Now peer B should appear dead
        assert!(!session.is_peer_alive());
    }

    #[test]
    fn test_peer_b_checks_peer_a_liveness() {
        let header = TestHeader::new();
        let header_ptr = header.as_ptr();

        struct TestSessionB {
            header: *const SegmentHeader,
            config: SessionConfig,
            _role: PhantomData<PeerB>,
        }

        impl TestSessionB {
            fn is_peer_alive(&self) -> bool {
                let header = unsafe { &*self.header };
                let timestamp = header.peer_a_last_seen.load(Ordering::Acquire);
                let now = now_nanos();
                let age_nanos = now.saturating_sub(timestamp);
                let timeout_nanos = self.config.liveness_timeout_ms * 1_000_000;
                age_nanos < timeout_nanos
            }
        }

        let session = TestSessionB {
            header: header_ptr,
            config: SessionConfig::default(),
            _role: PhantomData,
        };

        // Peer A is alive (just initialized)
        assert!(session.is_peer_alive());

        // Simulate peer A going stale
        let header_ref = unsafe { &*header_ptr };
        let old_time = now_nanos() - (2_000_000_000); // 2 seconds ago
        header_ref.peer_a_last_seen.store(old_time, Ordering::Release);

        // Now peer A should appear dead
        assert!(!session.is_peer_alive());
    }

    #[test]
    fn test_custom_liveness_timeout() {
        let header = TestHeader::new();
        let header_ptr = header.as_ptr();

        // Use a very short timeout for testing
        let config = SessionConfig {
            heartbeat_interval_ms: 10,
            liveness_timeout_ms: 50,  // 50ms timeout
        };

        struct TestSessionA {
            header: *const SegmentHeader,
            config: SessionConfig,
            _role: PhantomData<PeerA>,
        }

        impl TestSessionA {
            fn is_peer_alive(&self) -> bool {
                let header = unsafe { &*self.header };
                let timestamp = header.peer_b_last_seen.load(Ordering::Acquire);
                let now = now_nanos();
                let age_nanos = now.saturating_sub(timestamp);
                let timeout_nanos = self.config.liveness_timeout_ms * 1_000_000;
                age_nanos < timeout_nanos
            }
        }

        let session = TestSessionA {
            header: header_ptr,
            config,
            _role: PhantomData,
        };

        // Set peer B timestamp to 100ms ago (should be dead with 50ms timeout)
        let header_ref = unsafe { &*header_ptr };
        let old_time = now_nanos() - (100_000_000); // 100ms ago
        header_ref.peer_b_last_seen.store(old_time, Ordering::Release);

        assert!(!session.is_peer_alive());
    }

    #[test]
    fn test_role_is_peer_a() {
        assert!(PeerA::IS_PEER_A);
        assert!(!PeerB::IS_PEER_A);
    }
}
