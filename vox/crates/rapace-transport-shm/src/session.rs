//! SHM session management.
//!
//! A session represents a shared memory segment between two peers.

use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::Arc;

use rapace_core::MsgDescHot;

use crate::layout::{
    DEFAULT_RING_CAPACITY, DEFAULT_SLOT_COUNT, DEFAULT_SLOT_SIZE, DataSegment, DataSegmentHeader,
    DescRing, DescRingHeader, LayoutError, SegmentHeader, SegmentOffsets, SlotMeta,
    calculate_segment_size_checked as layout_calculate_segment_size_checked,
};

const DEFAULT_MAX_SEGMENT_SIZE_BYTES: usize = 512 * 1024 * 1024; // 512MB

#[cfg(test)]
fn munmap_key(base: usize, size: usize) -> u128 {
    ((base as u128) << 64) | (size as u128)
}

#[cfg(test)]
fn munmap_count_map()
-> &'static std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<u128, usize>>> {
    static COUNTS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<u128, usize>>> =
        std::sync::OnceLock::new();
    &COUNTS
}

#[cfg(test)]
fn munmap_count_for(key: u128) -> usize {
    let lock =
        munmap_count_map().get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    lock.lock().unwrap().get(&key).copied().unwrap_or(0)
}

fn max_segment_size_bytes() -> usize {
    std::env::var("RAPACE_SHM_MAX_BYTES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_MAX_SEGMENT_SIZE_BYTES)
}

/// What backs a shared memory mapping (used for logging and error context).
#[derive(Debug, Clone)]
enum ShmMappingKind {
    Anonymous,
    File { path: PathBuf },
}

/// An mmap-backed region that is unmapped on drop.
///
/// This type is expected to be used behind an `Arc` so the region is unmapped
/// exactly once, when the final reference is dropped.
#[derive(Debug)]
struct ShmMapping {
    base_addr: usize,
    size: usize,
    kind: ShmMappingKind,
}

impl ShmMapping {
    #[inline]
    fn base_addr(&self) -> usize {
        self.base_addr
    }

    #[inline]
    fn base_ptr(&self) -> *mut u8 {
        self.base_addr as *mut u8
    }
}

impl Drop for ShmMapping {
    fn drop(&mut self) {
        unsafe {
            if let Err(e) = munmap_region(self.base_ptr(), self.size) {
                match &self.kind {
                    ShmMappingKind::Anonymous => {
                        tracing::error!(error = %e, size = self.size, "munmap failed for anonymous SHM mapping");
                    }
                    ShmMappingKind::File { path } => {
                        tracing::error!(error = %e, size = self.size, path = %path.display(), "munmap failed for file-backed SHM mapping");
                    }
                }
            } else {
                match &self.kind {
                    ShmMappingKind::Anonymous => {
                        tracing::debug!(size = self.size, "unmapped anonymous SHM mapping");
                    }
                    ShmMappingKind::File { path } => {
                        tracing::debug!(size = self.size, path = %path.display(), "unmapped file-backed SHM mapping");
                    }
                }
            }
        }
    }
}

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
    /// Refcounted owner of the underlying mapping.
    mapping: Arc<ShmMapping>,
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
    #[tracing::instrument(
        level = "debug",
        skip(config),
        fields(
            ring_capacity = config.ring_capacity,
            slot_size = config.slot_size,
            slot_count = config.slot_count
        )
    )]
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

        let size = calculate_segment_size_checked_with_max(
            config.ring_capacity,
            config.slot_size,
            config.slot_count,
        )?;
        let offsets = SegmentOffsets::calculate_checked(config.ring_capacity, config.slot_count)
            .map_err(SessionError::InvalidConfig)?;

        // Create anonymous mmap.
        let mapping = unsafe { create_anonymous_mapping(size)? };

        tracing::info!(size, "created SHM session pair mapping");

        // Initialize the segment.
        unsafe {
            initialize_segment(mapping.base_ptr(), &config, &offsets)?;
        }

        // Create session A.
        let session_a = Arc::new(Self {
            role: PeerRole::A,
            mapping: mapping.clone(),
            offsets,
            config: config.clone(),
            local_send_head: std::sync::atomic::AtomicU64::new(0),
        });

        // Create session B (shares the same memory).
        let session_b = Arc::new(Self {
            role: PeerRole::B,
            mapping,
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
        unsafe { &*(self.mapping.base_ptr().add(self.offsets.header) as *const SegmentHeader) }
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
                self.mapping.base_ptr().add(header_offset) as *mut DescRingHeader,
                self.mapping.base_ptr().add(descs_offset) as *mut MsgDescHot,
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
                self.mapping.base_ptr().add(header_offset) as *mut DescRingHeader,
                self.mapping.base_ptr().add(descs_offset) as *mut MsgDescHot,
            )
        }
    }

    /// Get the data segment.
    pub fn data_segment(&self) -> DataSegment {
        unsafe {
            DataSegment::from_raw(
                self.mapping.base_ptr().add(self.offsets.data_header) as *mut DataSegmentHeader,
                self.mapping.base_ptr().add(self.offsets.slot_meta) as *mut SlotMeta,
                self.mapping.base_ptr().add(self.offsets.slot_data),
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
        self.mapping.base_addr()
    }

    /// Get the size of the SHM region.
    #[inline]
    pub fn size(&self) -> usize {
        self.mapping.size
    }

    /// Check if a pointer range is within this SHM segment.
    #[inline]
    pub fn contains_range(&self, ptr: *const u8, len: usize) -> bool {
        let start = ptr as usize;
        let end = start.saturating_add(len);
        let base = self.base_addr();
        let segment_end = base.saturating_add(self.size());
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
    #[tracing::instrument(
        level = "debug",
        skip(path, config),
        fields(
            path = %path.as_ref().display(),
            ring_capacity = config.ring_capacity,
            slot_size = config.slot_size,
            slot_count = config.slot_count
        )
    )]
    pub fn create_file(
        path: impl AsRef<Path>,
        config: ShmSessionConfig,
    ) -> Result<Arc<Self>, SessionError> {
        let path = path.as_ref();
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

        let size = calculate_segment_size_checked_with_max(
            config.ring_capacity,
            config.slot_size,
            config.slot_count,
        )?;
        let offsets = SegmentOffsets::calculate_checked(config.ring_capacity, config.slot_count)
            .map_err(SessionError::InvalidConfig)?;

        // Create and map the file.
        let mapping = unsafe { create_file_mapping(path, size, true)? };

        tracing::info!(size, path = %path.display(), "created file-backed SHM session mapping");

        // Initialize the segment.
        unsafe {
            initialize_segment(mapping.base_ptr(), &config, &offsets)?;
        }

        Ok(Arc::new(Self {
            role: PeerRole::A,
            mapping,
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
    #[tracing::instrument(
        level = "debug",
        skip(path, config),
        fields(
            path = %path.as_ref().display(),
            ring_capacity = config.ring_capacity,
            slot_size = config.slot_size,
            slot_count = config.slot_count
        )
    )]
    pub fn open_file(
        path: impl AsRef<Path>,
        config: ShmSessionConfig,
    ) -> Result<Arc<Self>, SessionError> {
        let path = path.as_ref();
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

        let size = calculate_segment_size_checked_with_max(
            config.ring_capacity,
            config.slot_size,
            config.slot_count,
        )?;
        let offsets = SegmentOffsets::calculate_checked(config.ring_capacity, config.slot_count)
            .map_err(SessionError::InvalidConfig)?;

        // Open and map the file.
        let mapping = unsafe { create_file_mapping(path, size, false)? };

        tracing::info!(size, path = %path.display(), "opened file-backed SHM session mapping");

        // Validate the segment header.
        let header = unsafe { &*(mapping.base_ptr().add(offsets.header) as *const SegmentHeader) };
        header.validate()?;

        Ok(Arc::new(Self {
            role: PeerRole::B,
            mapping,
            offsets,
            config,
            local_send_head: std::sync::atomic::AtomicU64::new(0),
        }))
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
    use libc::{MAP_ANONYMOUS, MAP_FAILED, MAP_SHARED, PROT_READ, PROT_WRITE, mmap};

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

unsafe fn create_anonymous_mapping(size: usize) -> Result<Arc<ShmMapping>, SessionError> {
    tracing::debug!(size, "creating anonymous SHM mapping");
    let base = unsafe { create_anonymous_mmap(size)? };
    Ok(Arc::new(ShmMapping {
        base_addr: base.as_ptr() as usize,
        size,
        kind: ShmMappingKind::Anonymous,
    }))
}

/// Create or open a file-backed mmap region.
///
/// # Safety
///
/// Returns a NonNull pointer to a newly mapped region of `size` bytes.
/// If `create` is true, the file is created/truncated. Otherwise, it must exist.
unsafe fn create_file_mapping(
    path: &Path,
    size: usize,
    create: bool,
) -> Result<Arc<ShmMapping>, SessionError> {
    use libc::{MAP_FAILED, MAP_SHARED, PROT_READ, PROT_WRITE, mmap};
    use std::fs::OpenOptions;
    use std::os::unix::io::AsRawFd;

    let path_buf = path.to_path_buf();
    tracing::debug!(size, create, path = %path_buf.display(), "creating file-backed SHM mapping");

    if path_buf.starts_with("/dev/shm") {
        tracing::warn!(
            path = %path_buf.display(),
            "SHM path is under /dev/shm (tmpfs); memory usage may be accounted as RAM"
        );
    }

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
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let meta = file.metadata()?;
        let actual_len = meta.len();
        let expected_len = size as u64;
        if actual_len < expected_len {
            return Err(SessionError::InvalidConfig(
                "SHM file is smaller than expected for provided config",
            ));
        } else if actual_len > expected_len {
            tracing::warn!(
                path = %path.display(),
                actual = actual_len,
                expected = expected_len,
                "SHM file is larger than expected; extra bytes will be ignored"
            );
        }
        file
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

    let base = NonNull::new(ptr as *mut u8)
        .ok_or_else(|| SessionError::System(std::io::Error::other("mmap returned null")))?;

    Ok(Arc::new(ShmMapping {
        base_addr: base.as_ptr() as usize,
        size,
        kind: ShmMappingKind::File { path: path_buf },
    }))
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

fn calculate_segment_size_checked_with_max(
    ring_capacity: u32,
    slot_size: u32,
    slot_count: u32,
) -> Result<usize, SessionError> {
    let total = layout_calculate_segment_size_checked(ring_capacity, slot_size, slot_count)
        .map_err(SessionError::InvalidConfig)?;

    let max = max_segment_size_bytes();
    if total > max {
        tracing::warn!(
            total_bytes = total,
            max_bytes = max,
            "SHM segment size exceeds configured maximum"
        );
        return Err(SessionError::InvalidConfig(
            "SHM segment size exceeds RAPACE_SHM_MAX_BYTES",
        ));
    }

    Ok(total)
}

/// Unmap an mmap region.
///
/// # Safety
///
/// `ptr` must have been returned by `mmap` (or equivalent) and `size` must match
/// the mapped length.
///
/// In tests, this also records successful `munmap` calls for leak/cleanup assertions.
unsafe fn munmap_region(ptr: *mut u8, size: usize) -> Result<(), std::io::Error> {
    use libc::{c_void, munmap};
    if size == 0 {
        return Ok(());
    }
    let rc = unsafe { munmap(ptr as *mut c_void, size) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    #[cfg(test)]
    {
        let key = munmap_key(ptr as usize, size);
        let lock = munmap_count_map()
            .get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
        let mut guard = lock.lock().unwrap();
        *guard.entry(key).or_insert(0) += 1;
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
        let (slot_idx, generation) = data.alloc().unwrap();

        // Copy data into it.
        let test_data = b"hello, shm!";
        unsafe {
            data.copy_to_slot(slot_idx, test_data).unwrap();
        }

        // Mark in-flight.
        data.mark_in_flight(slot_idx, generation).unwrap();

        // Read it back.
        let read_data = unsafe { data.read_slot(slot_idx, 0, test_data.len() as u32).unwrap() };
        assert_eq!(read_data, test_data);

        // Free it.
        data.free(slot_idx, generation).unwrap();
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

    #[test]
    fn test_create_pair_unmaps_once() {
        let (key, start) = {
            let (a, b) = ShmSession::create_pair().unwrap();
            let key = munmap_key(a.base_addr(), a.size());
            let start = munmap_count_for(key);
            drop(a);
            drop(b);
            (key, start)
        };
        {
            let end = munmap_count_for(key);
            assert_eq!(end, start + 1);
        }
    }

    #[test]
    fn test_file_mapping_unmaps() {
        let path = format!("/tmp/rapace-test-shm-drop-{}.shm", std::process::id());
        let (key, start) = {
            let session =
                ShmSession::create_file(path.as_str(), ShmSessionConfig::default()).unwrap();
            let key = munmap_key(session.base_addr(), session.size());
            let start = munmap_count_for(key);
            drop(session);
            (key, start)
        };
        let end = munmap_count_for(key);
        assert_eq!(end, start + 1);
        let _ = std::fs::remove_file(path);
    }
}
