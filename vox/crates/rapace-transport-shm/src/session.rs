//! SHM session management.
//!
//! A session represents a shared memory segment between two peers.

use std::ptr::NonNull;
use std::sync::Arc;

use rapace_core::MsgDescHot;

use crate::layout::{
    calculate_segment_size, DataSegment, DataSegmentHeader, DescRing, DescRingHeader, LayoutError,
    SegmentHeader, SegmentOffsets, SlotMeta, DEFAULT_RING_CAPACITY, DEFAULT_SLOT_COUNT,
    DEFAULT_SLOT_SIZE,
};

/// Configuration for creating an SHM session.
#[derive(Debug, Clone)]
pub struct ShmSessionConfig {
    /// Descriptor ring capacity (must be power of 2).
    pub ring_capacity: u32,
    /// Size of each data slot in bytes.
    pub slot_size: u32,
    /// Number of data slots.
    pub slot_count: u32,
}

impl Default for ShmSessionConfig {
    fn default() -> Self {
        Self {
            ring_capacity: DEFAULT_RING_CAPACITY,
            slot_size: DEFAULT_SLOT_SIZE,
            slot_count: DEFAULT_SLOT_COUNT,
        }
    }
}

/// Which peer role this session endpoint has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerRole {
    /// Peer A (typically the creator/server).
    A,
    /// Peer B (typically the connector/client).
    B,
}

/// A shared memory session between two peers.
///
/// Each session wraps a memory-mapped region containing:
/// - Segment header
/// - Two descriptor rings (A→B and B→A)
/// - Data segment (slab allocator)
pub struct ShmSession {
    /// Our role in this session.
    role: PeerRole,
    /// Pointer to the mapped region.
    base: NonNull<u8>,
    /// Size of the mapped region.
    size: usize,
    /// Calculated offsets.
    offsets: SegmentOffsets,
    /// Configuration used.
    config: ShmSessionConfig,
    /// Local head for our send ring (producer-private).
    local_send_head: std::sync::atomic::AtomicU64,
}

// SAFETY: ShmSession is Send + Sync because:
// - All shared state is synchronized via atomics in the SHM region.
// - local_send_head is only mutated by the owning transport (single producer).
unsafe impl Send for ShmSession {}
unsafe impl Sync for ShmSession {}

impl ShmSession {
    /// Create a connected pair of SHM sessions for testing.
    ///
    /// Uses an anonymous mmap (not backed by a file) for in-process testing.
    /// Both sessions share the same underlying memory region.
    pub fn create_pair() -> Result<(Arc<Self>, Arc<Self>), SessionError> {
        Self::create_pair_with_config(ShmSessionConfig::default())
    }

    /// Create a connected pair with custom configuration.
    pub fn create_pair_with_config(
        config: ShmSessionConfig,
    ) -> Result<(Arc<Self>, Arc<Self>), SessionError> {
        // Validate config.
        if !config.ring_capacity.is_power_of_two() {
            return Err(SessionError::InvalidConfig(
                "ring_capacity must be power of 2",
            ));
        }
        if config.slot_size == 0 {
            return Err(SessionError::InvalidConfig("slot_size must be > 0"));
        }
        if config.slot_count == 0 {
            return Err(SessionError::InvalidConfig("slot_count must be > 0"));
        }

        let size =
            calculate_segment_size(config.ring_capacity, config.slot_size, config.slot_count);
        let offsets = SegmentOffsets::calculate(config.ring_capacity, config.slot_count);

        // Create anonymous mmap.
        let base = unsafe { create_anonymous_mmap(size)? };

        // Initialize the segment.
        unsafe {
            initialize_segment(base.as_ptr(), &config, &offsets)?;
        }

        // Create session A.
        let session_a = Arc::new(Self {
            role: PeerRole::A,
            base,
            size,
            offsets,
            config: config.clone(),
            local_send_head: std::sync::atomic::AtomicU64::new(0),
        });

        // Create session B (shares the same memory).
        let session_b = Arc::new(Self {
            role: PeerRole::B,
            base,
            size,
            offsets,
            config,
            local_send_head: std::sync::atomic::AtomicU64::new(0),
        });

        Ok((session_a, session_b))
    }

    /// Get our peer role.
    #[inline]
    pub fn role(&self) -> PeerRole {
        self.role
    }

    /// Get the segment header.
    #[inline]
    pub fn header(&self) -> &SegmentHeader {
        unsafe { &*(self.base.as_ptr().add(self.offsets.header) as *const SegmentHeader) }
    }

    /// Get our send ring (we write, peer reads).
    pub fn send_ring(&self) -> DescRing {
        let (header_offset, descs_offset) = match self.role {
            PeerRole::A => (
                self.offsets.ring_a_to_b_header,
                self.offsets.ring_a_to_b_descs,
            ),
            PeerRole::B => (
                self.offsets.ring_b_to_a_header,
                self.offsets.ring_b_to_a_descs,
            ),
        };

        unsafe {
            DescRing::from_raw(
                self.base.as_ptr().add(header_offset) as *mut DescRingHeader,
                self.base.as_ptr().add(descs_offset) as *mut MsgDescHot,
            )
        }
    }

    /// Get our receive ring (peer writes, we read).
    pub fn recv_ring(&self) -> DescRing {
        let (header_offset, descs_offset) = match self.role {
            PeerRole::A => (
                self.offsets.ring_b_to_a_header,
                self.offsets.ring_b_to_a_descs,
            ),
            PeerRole::B => (
                self.offsets.ring_a_to_b_header,
                self.offsets.ring_a_to_b_descs,
            ),
        };

        unsafe {
            DescRing::from_raw(
                self.base.as_ptr().add(header_offset) as *mut DescRingHeader,
                self.base.as_ptr().add(descs_offset) as *mut MsgDescHot,
            )
        }
    }

    /// Get the data segment.
    pub fn data_segment(&self) -> DataSegment {
        unsafe {
            DataSegment::from_raw(
                self.base.as_ptr().add(self.offsets.data_header) as *mut DataSegmentHeader,
                self.base.as_ptr().add(self.offsets.slot_meta) as *mut SlotMeta,
                self.base.as_ptr().add(self.offsets.slot_data),
            )
        }
    }

    /// Get the local send head (for the producer side).
    #[inline]
    pub fn local_send_head(&self) -> &std::sync::atomic::AtomicU64 {
        &self.local_send_head
    }

    /// Get the base address of the SHM region.
    ///
    /// Used for checking if a pointer is within this SHM segment.
    #[inline]
    pub fn base_addr(&self) -> usize {
        self.base.as_ptr() as usize
    }

    /// Get the size of the SHM region.
    #[inline]
    pub fn size(&self) -> usize {
        self.size
    }

    /// Check if a pointer range is within this SHM segment.
    #[inline]
    pub fn contains_range(&self, ptr: *const u8, len: usize) -> bool {
        let start = ptr as usize;
        let end = start.saturating_add(len);
        let base = self.base_addr();
        let segment_end = base.saturating_add(self.size);
        start >= base && end <= segment_end
    }

    /// Get the slot data region base address.
    #[inline]
    pub fn slot_data_base(&self) -> usize {
        self.base_addr() + self.offsets.slot_data
    }

    /// Find if a pointer is in the slot data region and return (slot_index, offset).
    pub fn find_slot_location(&self, ptr: *const u8, len: usize) -> Option<(u32, u32)> {
        let addr = ptr as usize;
        let slot_base = self.slot_data_base();
        let slot_size = self.config.slot_size as usize;
        let slot_count = self.config.slot_count as usize;
        let slot_end = slot_base + slot_size * slot_count;

        // Check if entirely within slot data region.
        if addr < slot_base || addr >= slot_end {
            return None;
        }

        let relative = addr - slot_base;
        let slot_index = relative / slot_size;
        let offset = relative % slot_size;

        // Check it doesn't cross slot boundary.
        if offset + len > slot_size {
            return None;
        }

        Some((slot_index as u32, offset as u32))
    }

    /// Update our heartbeat timestamp in the SHM header.
    ///
    /// This should be called periodically to signal that we're still alive.
    /// The peer can check our last_seen timestamp to detect if we've crashed.
    pub fn update_heartbeat(&self) {
        use std::sync::atomic::Ordering;
        use std::time::{SystemTime, UNIX_EPOCH};

        let header = self.header();
        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        match self.role {
            PeerRole::A => {
                header.peer_a_last_seen.store(now_nanos, Ordering::Release);
                header.peer_a_epoch.fetch_add(1, Ordering::Relaxed);
            }
            PeerRole::B => {
                header.peer_b_last_seen.store(now_nanos, Ordering::Release);
                header.peer_b_epoch.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Check if the peer is alive based on their last heartbeat.
    ///
    /// Returns `true` if the peer's heartbeat is recent enough.
    /// A peer is considered dead if their last_seen timestamp is older than `timeout_nanos`.
    ///
    /// # Arguments
    ///
    /// * `timeout_nanos` - Maximum age of the peer's heartbeat in nanoseconds.
    ///   Recommended value: 1-5 seconds (1_000_000_000 to 5_000_000_000 nanos).
    pub fn is_peer_alive(&self, timeout_nanos: u64) -> bool {
        use std::sync::atomic::Ordering;
        use std::time::{SystemTime, UNIX_EPOCH};

        let header = self.header();
        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let peer_last_seen = match self.role {
            PeerRole::A => header.peer_b_last_seen.load(Ordering::Acquire),
            PeerRole::B => header.peer_a_last_seen.load(Ordering::Acquire),
        };

        // If peer has never sent a heartbeat (last_seen == 0), they're not alive yet.
        // Once they start sending heartbeats, we check for staleness.
        if peer_last_seen == 0 {
            // Peer hasn't initialized yet - consider them alive for now
            // to avoid false positives during startup.
            return true;
        }

        let age_nanos = now_nanos.saturating_sub(peer_last_seen);
        age_nanos < timeout_nanos
    }

    /// Get the peer's epoch counter.
    ///
    /// This can be used to detect if the peer is making progress.
    pub fn peer_epoch(&self) -> u64 {
        use std::sync::atomic::Ordering;
        let header = self.header();
        match self.role {
            PeerRole::A => header.peer_b_epoch.load(Ordering::Acquire),
            PeerRole::B => header.peer_a_epoch.load(Ordering::Acquire),
        }
    }
}

impl ShmSession {
    /// Create a new file-backed SHM session.
    ///
    /// This creates a new SHM segment backed by a file at the given path.
    /// The file is created and truncated if it exists. The caller takes
    /// the role of Peer A (creator/server).
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the SHM file
    /// * `config` - Session configuration
    ///
    /// # Example
    ///
    /// ```ignore
    /// let session = ShmSession::create_file("/tmp/rapace.shm", ShmSessionConfig::default())?;
    /// // Share the path with the other process...
    /// ```
    pub fn create_file(
        path: impl AsRef<std::path::Path>,
        config: ShmSessionConfig,
    ) -> Result<Arc<Self>, SessionError> {
        // Validate config.
        if !config.ring_capacity.is_power_of_two() {
            return Err(SessionError::InvalidConfig(
                "ring_capacity must be power of 2",
            ));
        }
        if config.slot_size == 0 {
            return Err(SessionError::InvalidConfig("slot_size must be > 0"));
        }
        if config.slot_count == 0 {
            return Err(SessionError::InvalidConfig("slot_count must be > 0"));
        }

        let size =
            calculate_segment_size(config.ring_capacity, config.slot_size, config.slot_count);
        let offsets = SegmentOffsets::calculate(config.ring_capacity, config.slot_count);

        // Create and map the file.
        let base = unsafe { create_file_mmap(path.as_ref(), size, true)? };

        // Initialize the segment.
        unsafe {
            initialize_segment(base.as_ptr(), &config, &offsets)?;
        }

        Ok(Arc::new(Self {
            role: PeerRole::A,
            base,
            size,
            offsets,
            config,
            local_send_head: std::sync::atomic::AtomicU64::new(0),
        }))
    }

    /// Open an existing file-backed SHM session.
    ///
    /// This opens an existing SHM segment created by another process.
    /// The caller takes the role of Peer B (connector/client).
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the SHM file
    /// * `config` - Session configuration (must match the creator's config)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let session = ShmSession::open_file("/tmp/rapace.shm", ShmSessionConfig::default())?;
    /// ```
    pub fn open_file(
        path: impl AsRef<std::path::Path>,
        config: ShmSessionConfig,
    ) -> Result<Arc<Self>, SessionError> {
        // Validate config.
        if !config.ring_capacity.is_power_of_two() {
            return Err(SessionError::InvalidConfig(
                "ring_capacity must be power of 2",
            ));
        }
        if config.slot_size == 0 {
            return Err(SessionError::InvalidConfig("slot_size must be > 0"));
        }
        if config.slot_count == 0 {
            return Err(SessionError::InvalidConfig("slot_count must be > 0"));
        }

        let size =
            calculate_segment_size(config.ring_capacity, config.slot_size, config.slot_count);
        let offsets = SegmentOffsets::calculate(config.ring_capacity, config.slot_count);

        // Open and map the file.
        let base = unsafe { create_file_mmap(path.as_ref(), size, false)? };

        // Validate the segment header.
        let header = unsafe { &*(base.as_ptr().add(offsets.header) as *const SegmentHeader) };
        header.validate()?;

        Ok(Arc::new(Self {
            role: PeerRole::B,
            base,
            size,
            offsets,
            config,
            local_send_head: std::sync::atomic::AtomicU64::new(0),
        }))
    }
}

impl Drop for ShmSession {
    fn drop(&mut self) {
        // Only unmap if we're the last reference.
        // Since we use Arc, the memory will only be unmapped when both sessions are dropped.
        // For create_pair(), both sessions share the same NonNull, so we need reference counting
        // at the mmap level. For now, we'll leak the memory to avoid double-unmap.
        //
        // TODO: Use a proper refcounted mmap wrapper for production.
        // For testing purposes, leaking is acceptable.
    }
}

/// Errors from session operations.
#[derive(Debug)]
pub enum SessionError {
    /// Invalid configuration.
    InvalidConfig(&'static str),
    /// Layout validation failed.
    Layout(LayoutError),
    /// System error (mmap failed, etc.).
    System(std::io::Error),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(msg) => write!(f, "invalid config: {}", msg),
            Self::Layout(e) => write!(f, "layout error: {}", e),
            Self::System(e) => write!(f, "system error: {}", e),
        }
    }
}

impl std::error::Error for SessionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Layout(e) => Some(e),
            Self::System(e) => Some(e),
            _ => None,
        }
    }
}

impl From<LayoutError> for SessionError {
    fn from(e: LayoutError) -> Self {
        Self::Layout(e)
    }
}

impl From<std::io::Error> for SessionError {
    fn from(e: std::io::Error) -> Self {
        Self::System(e)
    }
}

/// Create an anonymous mmap region.
///
/// # Safety
///
/// Returns a NonNull pointer to a newly mapped region of `size` bytes.
/// The region is initialized to zero.
unsafe fn create_anonymous_mmap(size: usize) -> Result<NonNull<u8>, SessionError> {
    use libc::{mmap, MAP_ANONYMOUS, MAP_FAILED, MAP_SHARED, PROT_READ, PROT_WRITE};

    let ptr = unsafe {
        mmap(
            std::ptr::null_mut(),
            size,
            PROT_READ | PROT_WRITE,
            MAP_SHARED | MAP_ANONYMOUS,
            -1, // No file descriptor for anonymous mapping.
            0,
        )
    };

    if ptr == MAP_FAILED {
        return Err(SessionError::System(std::io::Error::last_os_error()));
    }

    NonNull::new(ptr as *mut u8)
        .ok_or_else(|| SessionError::System(std::io::Error::other("mmap returned null")))
}

/// Create or open a file-backed mmap region.
///
/// # Safety
///
/// Returns a NonNull pointer to a newly mapped region of `size` bytes.
/// If `create` is true, the file is created/truncated. Otherwise, it must exist.
unsafe fn create_file_mmap(
    path: &std::path::Path,
    size: usize,
    create: bool,
) -> Result<NonNull<u8>, SessionError> {
    use libc::{mmap, MAP_FAILED, MAP_SHARED, PROT_READ, PROT_WRITE};
    use std::fs::OpenOptions;
    use std::os::unix::io::AsRawFd;

    // Open/create the file.
    let file = if create {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;

        // Set the file size.
        file.set_len(size as u64)?;
        file
    } else {
        OpenOptions::new().read(true).write(true).open(path)?
    };

    let fd = file.as_raw_fd();

    let ptr = unsafe {
        mmap(
            std::ptr::null_mut(),
            size,
            PROT_READ | PROT_WRITE,
            MAP_SHARED,
            fd,
            0,
        )
    };

    // Keep file open for the lifetime of the mapping.
    // In practice, the kernel keeps it alive, but we let it close.
    std::mem::drop(file);

    if ptr == MAP_FAILED {
        return Err(SessionError::System(std::io::Error::last_os_error()));
    }

    NonNull::new(ptr as *mut u8)
        .ok_or_else(|| SessionError::System(std::io::Error::other("mmap returned null")))
}

/// Initialize the SHM segment.
///
/// # Safety
///
/// `base` must point to a valid, zeroed memory region of appropriate size.
unsafe fn initialize_segment(
    base: *mut u8,
    config: &ShmSessionConfig,
    offsets: &SegmentOffsets,
) -> Result<(), SessionError> {
    // Initialize segment header.
    let header = unsafe { &mut *(base.add(offsets.header) as *mut SegmentHeader) };
    header.init();

    // Initialize A→B ring.
    let ring_a_to_b =
        unsafe { &mut *(base.add(offsets.ring_a_to_b_header) as *mut DescRingHeader) };
    ring_a_to_b.init(config.ring_capacity);

    // Initialize B→A ring.
    let ring_b_to_a =
        unsafe { &mut *(base.add(offsets.ring_b_to_a_header) as *mut DescRingHeader) };
    ring_b_to_a.init(config.ring_capacity);

    // Initialize data segment header.
    let data_header = unsafe { &mut *(base.add(offsets.data_header) as *mut DataSegmentHeader) };
    data_header.init(config.slot_size, config.slot_count);

    // Initialize slot metadata.
    for i in 0..config.slot_count {
        let meta = unsafe { &mut *(base.add(offsets.slot_meta) as *mut SlotMeta).add(i as usize) };
        meta.init();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_pair() {
        let (a, b) = ShmSession::create_pair().unwrap();
        assert_eq!(a.role(), PeerRole::A);
        assert_eq!(b.role(), PeerRole::B);
    }

    #[test]
    fn test_header_valid() {
        let (a, _b) = ShmSession::create_pair().unwrap();
        let header = a.header();
        assert!(header.validate().is_ok());
    }

    #[test]
    fn test_rings_created() {
        let (a, b) = ShmSession::create_pair().unwrap();

        // A's send ring should be B's recv ring (A→B).
        let a_send = a.send_ring();
        let b_recv = b.recv_ring();

        assert_eq!(a_send.capacity(), b_recv.capacity());
        assert!(a_send.is_empty());
        assert!(b_recv.is_empty());
    }

    #[test]
    fn test_data_segment_created() {
        let (a, _b) = ShmSession::create_pair().unwrap();
        let data = a.data_segment();

        assert_eq!(data.slot_size(), DEFAULT_SLOT_SIZE);
        assert_eq!(data.slot_count(), DEFAULT_SLOT_COUNT);
    }

    #[test]
    fn test_ring_enqueue_dequeue() {
        let (a, b) = ShmSession::create_pair().unwrap();

        let send_ring = a.send_ring();
        let recv_ring = b.recv_ring();

        // Create a test descriptor.
        let mut desc = MsgDescHot::new();
        desc.msg_id = 42;
        desc.channel_id = 1;
        desc.method_id = 100;

        // Enqueue on A.
        let mut local_head = a
            .local_send_head()
            .load(std::sync::atomic::Ordering::Relaxed);
        send_ring.enqueue(&mut local_head, &desc).unwrap();
        a.local_send_head()
            .store(local_head, std::sync::atomic::Ordering::Release);

        // Dequeue on B.
        let received = recv_ring.dequeue().unwrap();
        assert_eq!(received.msg_id, 42);
        assert_eq!(received.channel_id, 1);
        assert_eq!(received.method_id, 100);
    }

    #[test]
    fn test_slot_alloc_free() {
        let (a, _b) = ShmSession::create_pair().unwrap();
        let data = a.data_segment();

        // Allocate a slot.
        let (slot_idx, gen) = data.alloc().unwrap();

        // Copy data into it.
        let test_data = b"hello, shm!";
        unsafe {
            data.copy_to_slot(slot_idx, test_data).unwrap();
        }

        // Mark in-flight.
        data.mark_in_flight(slot_idx, gen).unwrap();

        // Read it back.
        let read_data = unsafe { data.read_slot(slot_idx, 0, test_data.len() as u32).unwrap() };
        assert_eq!(read_data, test_data);

        // Free it.
        data.free(slot_idx, gen).unwrap();
    }

    #[test]
    fn test_find_slot_location() {
        let (a, _b) = ShmSession::create_pair().unwrap();

        // Allocate a slot to get its address.
        let data = a.data_segment();
        let (slot_idx, _gen) = data.alloc().unwrap();

        // Get the pointer to slot data.
        let slot_base = a.slot_data_base();
        let slot_size = data.slot_size() as usize;
        let slot_ptr = (slot_base + slot_idx as usize * slot_size) as *const u8;

        // Should find the slot.
        let (found_idx, found_offset) = a.find_slot_location(slot_ptr, 100).unwrap();
        assert_eq!(found_idx, slot_idx);
        assert_eq!(found_offset, 0);

        // Check with offset.
        let offset_ptr = unsafe { slot_ptr.add(50) };
        let (found_idx, found_offset) = a.find_slot_location(offset_ptr, 50).unwrap();
        assert_eq!(found_idx, slot_idx);
        assert_eq!(found_offset, 50);

        // Outside SHM should return None.
        let outside: *const u8 = 0x12345678 as *const u8;
        assert!(a.find_slot_location(outside, 100).is_none());
    }
}
