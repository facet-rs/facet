// src/shm.rs

use std::ffi::CString;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::ptr::NonNull;

/// A shared memory segment.
pub struct SharedMemory {
    ptr: NonNull<u8>,
    len: usize,
    fd: OwnedFd,
}

// Safety: SharedMemory is a memory-mapped region that can be shared
unsafe impl Send for SharedMemory {}
unsafe impl Sync for SharedMemory {}

impl SharedMemory {
    /// Create a new anonymous shared memory segment.
    ///
    /// On Linux, uses memfd_create for anonymous shared memory.
    /// On macOS, uses shm_open with a unique name, then shm_unlink.
    pub fn create(name: &str, size: usize) -> io::Result<Self> {
        let fd = Self::create_fd(name)?;

        // Set the size
        let result = unsafe { libc::ftruncate(fd.as_raw_fd(), size as libc::off_t) };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }

        // Map it
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd.as_raw_fd(),
                0,
            )
        };

        if ptr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }

        Ok(SharedMemory {
            ptr: NonNull::new(ptr as *mut u8).unwrap(),
            len: size,
            fd,
        })
    }

    /// Open an existing shared memory segment from a file descriptor.
    pub fn from_fd(fd: OwnedFd, size: usize) -> io::Result<Self> {
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd.as_raw_fd(),
                0,
            )
        };

        if ptr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }

        Ok(SharedMemory {
            ptr: NonNull::new(ptr as *mut u8).unwrap(),
            len: size,
            fd,
        })
    }

    #[cfg(target_os = "linux")]
    fn create_fd(name: &str) -> io::Result<OwnedFd> {
        let c_name = CString::new(name).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "invalid name")
        })?;

        let fd = unsafe {
            libc::memfd_create(c_name.as_ptr(), libc::MFD_CLOEXEC)
        };

        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    #[cfg(target_os = "macos")]
    fn create_fd(name: &str) -> io::Result<OwnedFd> {
        // macOS doesn't have memfd_create, use shm_open + shm_unlink
        let unique_name = format!("/rapace-{}-{}", std::process::id(), name);
        let c_name = CString::new(unique_name.clone()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "invalid name")
        })?;

        let fd = unsafe {
            libc::shm_open(
                c_name.as_ptr(),
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
                0o600,
            )
        };

        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        // Immediately unlink so it's anonymous
        unsafe {
            libc::shm_unlink(c_name.as_ptr());
        }

        // Set close-on-exec
        unsafe {
            libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
        }

        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    fn create_fd(name: &str) -> io::Result<OwnedFd> {
        // Generic fallback using shm_open
        let unique_name = format!("/rapace-{}-{}", std::process::id(), name);
        let c_name = CString::new(unique_name).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "invalid name")
        })?;

        let fd = unsafe {
            libc::shm_open(
                c_name.as_ptr(),
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
                0o600,
            )
        };

        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        // Immediately unlink
        unsafe {
            libc::shm_unlink(c_name.as_ptr());
        }

        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    /// Get a pointer to the start of the shared memory.
    pub fn as_ptr(&self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    /// Get the length of the shared memory region.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Check if the shared memory region is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get the file descriptor (for passing to another process).
    pub fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }

    /// Get a slice view of the shared memory.
    ///
    /// # Safety
    /// Caller must ensure no concurrent writes to the region being read.
    pub unsafe fn as_slice(&self) -> &[u8] {
        std::slice::from_raw_parts(self.ptr.as_ptr(), self.len)
    }

    /// Get a mutable slice view of the shared memory.
    ///
    /// # Safety
    /// Caller must ensure exclusive access to the region being written.
    pub unsafe fn as_mut_slice(&mut self) -> &mut [u8] {
        std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len)
    }

    /// Get a NonNull pointer for use with layout types.
    pub fn as_non_null(&self) -> NonNull<u8> {
        self.ptr
    }

    /// Try to duplicate the file descriptor.
    pub fn try_clone_fd(&self) -> io::Result<OwnedFd> {
        let new_fd = unsafe { libc::dup(self.fd.as_raw_fd()) };
        if new_fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { OwnedFd::from_raw_fd(new_fd) })
    }
}

impl Drop for SharedMemory {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr.as_ptr() as *mut libc::c_void, self.len);
        }
    }
}

/// Calculate the required size for a rapace segment.
///
/// Layout:
/// - SegmentHeader (64 bytes)
/// - A→B DescRing header (192 bytes) + descriptors
/// - B→A DescRing header (192 bytes) + descriptors
/// - Slot metadata array
/// - Slot data array
pub fn calculate_segment_size(
    ring_capacity: u32,
    slot_count: u32,
    slot_size: u32,
) -> usize {
    use crate::layout::{SegmentHeader, DescRingHeader, MsgDescHot, SlotMeta};

    let header_size = std::mem::size_of::<SegmentHeader>();
    let ring_header_size = std::mem::size_of::<DescRingHeader>();
    let desc_size = std::mem::size_of::<MsgDescHot>();
    let slot_meta_size = std::mem::size_of::<SlotMeta>();

    // Two rings (A→B and B→A)
    let ring_size = ring_header_size + (desc_size * ring_capacity as usize);
    let total_rings = ring_size * 2;

    // Slot metadata and data
    let meta_size = slot_meta_size * slot_count as usize;
    let data_size = slot_size as usize * slot_count as usize;

    // Align everything to 64 bytes
    let align = |x: usize| (x + 63) & !63;

    align(header_size) + align(total_rings) + align(meta_size) + align(data_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_write() {
        let mut shm = SharedMemory::create("test1", 4096).unwrap();

        unsafe {
            let slice = shm.as_mut_slice();
            slice[0] = 42;
            slice[1] = 43;
        }

        unsafe {
            let slice = shm.as_slice();
            assert_eq!(slice[0], 42);
            assert_eq!(slice[1], 43);
        }
    }

    #[test]
    fn size_and_pointer() {
        let shm = SharedMemory::create("test2", 8192).unwrap();

        assert_eq!(shm.len(), 8192);
        assert!(!shm.as_ptr().is_null());
    }

    #[test]
    fn duplicate_fd() {
        let shm = SharedMemory::create("test3", 4096).unwrap();
        let fd2 = shm.try_clone_fd().unwrap();

        assert!(fd2.as_raw_fd() >= 0);
        assert_ne!(fd2.as_raw_fd(), shm.as_raw_fd());
    }

    #[test]
    fn from_fd_maps_same_memory() {
        let shm1 = SharedMemory::create("test4", 4096).unwrap();

        // Write something
        unsafe {
            (*shm1.as_ptr()) = 123;
        }

        // Map same fd
        let fd2 = shm1.try_clone_fd().unwrap();
        let shm2 = SharedMemory::from_fd(fd2, 4096).unwrap();

        // Should see the same data
        unsafe {
            assert_eq!(*shm2.as_ptr(), 123);
        }
    }

    #[test]
    fn calculate_size() {
        let size = calculate_segment_size(64, 16, 4096);

        // Should be reasonable
        assert!(size > 0);
        assert!(size < 1024 * 1024); // Less than 1MB for these params

        // Should be 64-byte aligned
        assert_eq!(size % 64, 0);
    }
}
