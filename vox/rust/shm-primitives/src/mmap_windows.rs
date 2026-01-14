//! File-backed memory-mapped regions for cross-process shared memory (Windows).
//!
//! This module provides `MmapRegion`, a file-backed memory region that can be
//! shared across processes using Windows file mapping APIs.

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CREATE_ALWAYS, CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_DELETE_ON_CLOSE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, GENERIC_READ, GENERIC_WRITE,
};
use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, FILE_MAP_ALL_ACCESS, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile,
    PAGE_READWRITE, UnmapViewOfFile,
};

use crate::Region;

/// Cleanup behavior for memory-mapped files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileCleanup {
    /// Keep the file after all processes exit (manual cleanup required).
    Manual,
    /// Automatically delete the file when all processes exit.
    /// On Unix: file is unlinked immediately (stays alive while mapped).
    /// On Windows: file is opened with FILE_FLAG_DELETE_ON_CLOSE.
    Auto,
}

/// File-backed memory-mapped region for cross-process shared memory.
///
/// shm[impl shm.file.mmap-windows]
pub struct MmapRegion {
    /// Pointer to the mapped memory
    ptr: *mut u8,
    /// Length of the mapping in bytes
    len: usize,
    /// The underlying file (kept open to maintain the mapping)
    file: File,
    /// Handle to the file mapping object
    mapping_handle: HANDLE,
    /// Path to the file (for cleanup)
    path: PathBuf,
    /// Whether this region owns the file (should delete on drop)
    owns_file: bool,
}

impl MmapRegion {
    /// Create a new file-backed region.
    ///
    /// This creates the file, truncates it to the given size, and maps it
    /// into memory. The file is created with default permissions.
    ///
    /// shm[impl shm.file.create]
    pub fn create(path: &Path, size: usize, cleanup: FileCleanup) -> io::Result<Self> {
        if size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "size must be > 0",
            ));
        }

        // 1. Open or create file with read/write, truncate
        let file = if cleanup == FileCleanup::Auto {
            // Use raw Windows API to set FILE_FLAG_DELETE_ON_CLOSE
            let path_wide: Vec<u16> = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            let handle = unsafe {
                CreateFileW(
                    path_wide.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    std::ptr::null(),
                    CREATE_ALWAYS,
                    FILE_ATTRIBUTE_NORMAL | FILE_FLAG_DELETE_ON_CLOSE,
                    0,
                )
            };

            if handle == INVALID_HANDLE_VALUE {
                let err = io::Error::last_os_error();
                let msg = std::format!("Failed to create SHM file at {}: {}", path.display(), err);
                return Err(io::Error::new(err.kind(), msg));
            }

            // SAFETY: We just created this handle
            unsafe { File::from_raw_handle(handle as _) }
        } else {
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)
                .map_err(|e| {
                    let msg = std::format!("Failed to create SHM file at {}: {}", path.display(), e);
                    io::Error::new(e.kind(), msg)
                })?
        };

        // 2. Truncate to desired size
        file.set_len(size as u64)?;

        // 3. Create file mapping
        let file_handle = file.as_raw_handle() as HANDLE;
        let mapping_handle = unsafe {
            CreateFileMappingW(
                file_handle,
                std::ptr::null(),    // default security
                PAGE_READWRITE,      // read/write access
                (size >> 32) as u32, // high-order DWORD of size
                size as u32,         // low-order DWORD of size
                std::ptr::null(),    // no name (file-backed)
            )
        };

        if mapping_handle.is_null() {
            return Err(io::Error::last_os_error());
        }

        // 4. Map view of file
        let ptr = unsafe {
            MapViewOfFile(
                mapping_handle,
                FILE_MAP_ALL_ACCESS,
                0,    // offset high
                0,    // offset low
                size, // bytes to map (0 = entire file)
            )
        };

        if ptr.Value.is_null() {
            unsafe { CloseHandle(mapping_handle) };
            return Err(io::Error::last_os_error());
        }

        Ok(Self {
            ptr: ptr.Value as *mut u8,
            len: size,
            file,
            mapping_handle,
            path: path.to_path_buf(),
            owns_file: cleanup == FileCleanup::Manual,
        })
    }

    /// Attach to an existing file-backed region.
    ///
    /// This opens the file and maps it into memory.
    /// The file size determines the mapping size.
    ///
    /// shm[impl shm.file.attach]
    pub fn attach(path: &Path) -> io::Result<Self> {
        // Open existing file for read/write
        let file = OpenOptions::new().read(true).write(true).open(path)
            .map_err(|e| {
                let msg = std::format!("Failed to open SHM file at {}: {}", path.display(), e);
                io::Error::new(e.kind(), msg)
            })?;

        // Get file size
        let metadata = file.metadata()?;
        let size = metadata.len() as usize;

        if size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "segment file is empty",
            ));
        }

        // Create file mapping
        let file_handle = file.as_raw_handle() as HANDLE;
        let mapping_handle = unsafe {
            CreateFileMappingW(
                file_handle,
                std::ptr::null(),
                PAGE_READWRITE,
                (size >> 32) as u32,
                size as u32,
                std::ptr::null(),
            )
        };

        if mapping_handle.is_null() {
            return Err(io::Error::last_os_error());
        }

        // Map view of file
        let ptr = unsafe { MapViewOfFile(mapping_handle, FILE_MAP_ALL_ACCESS, 0, 0, size) };

        if ptr.Value.is_null() {
            unsafe { CloseHandle(mapping_handle) };
            return Err(io::Error::last_os_error());
        }

        Ok(Self {
            ptr: ptr.Value as *mut u8,
            len: size,
            file,
            mapping_handle,
            path: path.to_path_buf(),
            owns_file: false, // Attached regions don't own the file
        })
    }

    /// Get a `Region` view of this mmap.
    #[inline]
    pub fn region(&self) -> Region {
        // SAFETY: The mmap is valid for the lifetime of MmapRegion
        unsafe { Region::from_raw(self.ptr, self.len) }
    }

    /// Get the size of the region in bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the region is empty (zero bytes).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get the path to the backing file.
    #[inline]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Take ownership of the file for cleanup purposes.
    ///
    /// After calling this, the file will be deleted when this region is dropped.
    pub fn take_ownership(&mut self) {
        self.owns_file = true;
    }

    /// Release ownership of the file.
    ///
    /// After calling this, the file will NOT be deleted when this region is dropped.
    pub fn release_ownership(&mut self) {
        self.owns_file = false;
    }

    /// Resize the region by growing the backing file and remapping.
    ///
    /// This is typically a host-only operation. The base pointer may change,
    /// so callers must update any cached `Region` references after calling this.
    ///
    /// # Errors
    ///
    /// Returns an error if the new size is smaller than current size (shrinking
    /// is not supported), or if the underlying file/mmap operations fail.
    ///
    /// shm[impl shm.varslot.extents]
    pub fn resize(&mut self, new_size: usize) -> io::Result<()> {
        if new_size < self.len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "shrinking is not supported",
            ));
        }
        if new_size == self.len {
            return Ok(()); // No change needed
        }

        // 1. Unmap old view
        let unmap_result = unsafe {
            UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                Value: self.ptr as *mut _,
            })
        };
        if unmap_result == 0 {
            return Err(io::Error::last_os_error());
        }

        // 2. Close old mapping handle
        unsafe { CloseHandle(self.mapping_handle) };

        // 3. Grow the backing file
        self.file.set_len(new_size as u64)?;

        // 4. Create new mapping
        let file_handle = self.file.as_raw_handle() as HANDLE;
        let mapping_handle = unsafe {
            CreateFileMappingW(
                file_handle,
                std::ptr::null(),
                PAGE_READWRITE,
                (new_size >> 32) as u32,
                new_size as u32,
                std::ptr::null(),
            )
        };

        if mapping_handle.is_null() {
            return Err(io::Error::last_os_error());
        }

        // 5. Map new view
        let ptr = unsafe { MapViewOfFile(mapping_handle, FILE_MAP_ALL_ACCESS, 0, 0, new_size) };

        if ptr.Value.is_null() {
            unsafe { CloseHandle(mapping_handle) };
            return Err(io::Error::last_os_error());
        }

        self.ptr = ptr.Value as *mut u8;
        self.len = new_size;
        self.mapping_handle = mapping_handle;
        Ok(())
    }

    /// Check if the backing file has grown and remap if needed.
    ///
    /// This is useful for guests to detect when the host has grown the segment.
    /// Returns `true` if the region was remapped, `false` if no change.
    ///
    /// # Errors
    ///
    /// Returns an error if file metadata cannot be read or remapping fails.
    pub fn check_and_remap(&mut self) -> io::Result<bool> {
        let file_size = self.file.metadata()?.len() as usize;
        if file_size > self.len {
            self.resize(file_size)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl Drop for MmapRegion {
    fn drop(&mut self) {
        // Unmap the memory
        unsafe {
            UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                Value: self.ptr as *mut _,
            });
            CloseHandle(self.mapping_handle);
        }

        // Delete the file if we own it
        // shm[impl shm.file.cleanup]
        if self.owns_file {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

// SAFETY: The mmap region is valid for the lifetime of MmapRegion and can be
// safely accessed from multiple threads (the underlying memory is shared).
unsafe impl Send for MmapRegion {}
unsafe impl Sync for MmapRegion {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_attach() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.shm");

        // Create region
        let region1 = MmapRegion::create(&path, 4096, FileCleanup::Manual).unwrap();
        assert_eq!(region1.len(), 4096);
        assert!(path.exists());

        // Write some data
        let data = region1.region();
        unsafe {
            std::ptr::write(data.as_ptr(), 0x42);
            std::ptr::write(data.as_ptr().add(1), 0x43);
        }

        // Attach from another "process" (same process, different mapping)
        let region2 = MmapRegion::attach(&path).unwrap();
        assert_eq!(region2.len(), 4096);

        // Verify data is visible
        let data2 = region2.region();
        unsafe {
            assert_eq!(std::ptr::read(data2.as_ptr()), 0x42);
            assert_eq!(std::ptr::read(data2.as_ptr().add(1)), 0x43);
        }
    }

    #[test]
    fn test_cleanup_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cleanup.shm");

        {
            let _region = MmapRegion::create(&path, 1024, FileCleanup::Manual).unwrap();
            assert!(path.exists());
        }

        // File should be deleted after owner drops
        assert!(!path.exists());
    }

    #[test]
    fn test_attached_does_not_cleanup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("attached.shm");

        let owner = MmapRegion::create(&path, 1024, FileCleanup::Manual).unwrap();

        {
            let _attached = MmapRegion::attach(&path).unwrap();
            assert!(path.exists());
        }

        // File should still exist after attached drops
        assert!(path.exists());

        // File should be deleted after owner drops
        drop(owner);
        assert!(!path.exists());
    }

    #[test]
    fn test_shared_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shared.shm");

        let region1 = MmapRegion::create(&path, 4096, FileCleanup::Manual).unwrap();
        let region2 = MmapRegion::attach(&path).unwrap();

        // Write from region2
        let data2 = region2.region();
        unsafe {
            std::ptr::write(data2.as_ptr().add(100), 0xAB);
        }

        // Read from region1
        let data1 = region1.region();
        unsafe {
            assert_eq!(std::ptr::read(data1.as_ptr().add(100)), 0xAB);
        }
    }

    #[test]
    fn test_zero_size_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("zero.shm");

        let result = MmapRegion::create(&path, 0, FileCleanup::Manual);
        assert!(result.is_err());
    }

    #[test]
    fn test_resize_grows_region() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("resize.shm");

        let mut region = MmapRegion::create(&path, 4096, FileCleanup::Manual).unwrap();
        assert_eq!(region.len(), 4096);

        // Write data at the start
        unsafe {
            std::ptr::write(region.region().as_ptr(), 0xAB);
        }

        // Resize to 8192
        region.resize(8192).unwrap();
        assert_eq!(region.len(), 8192);

        // Original data should still be accessible
        unsafe {
            assert_eq!(std::ptr::read(region.region().as_ptr()), 0xAB);
        }

        // Can write to new area
        unsafe {
            std::ptr::write(region.region().as_ptr().add(5000), 0xCD);
            assert_eq!(std::ptr::read(region.region().as_ptr().add(5000)), 0xCD);
        }
    }

    #[test]
    fn test_resize_shrink_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shrink.shm");

        let mut region = MmapRegion::create(&path, 8192, FileCleanup::Manual).unwrap();
        let result = region.resize(4096);
        assert!(result.is_err());
    }

    #[test]
    fn test_check_and_remap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("remap.shm");

        // Create owner region
        let mut owner = MmapRegion::create(&path, 4096, FileCleanup::Manual).unwrap();

        // Attach guest
        let mut guest = MmapRegion::attach(&path).unwrap();
        assert_eq!(guest.len(), 4096);

        // Owner grows the file
        owner.resize(8192).unwrap();

        // Guest detects and remaps
        let remapped = guest.check_and_remap().unwrap();
        assert!(remapped);
        assert_eq!(guest.len(), 8192);

        // Second check should return false (no change)
        let remapped2 = guest.check_and_remap().unwrap();
        assert!(!remapped2);
    }

    #[test]
    fn test_resize_preserves_shared_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shared_resize.shm");

        let mut owner = MmapRegion::create(&path, 4096, FileCleanup::Manual).unwrap();
        let mut guest = MmapRegion::attach(&path).unwrap();

        // Write from owner
        unsafe {
            std::ptr::write(owner.region().as_ptr().add(100), 0x42);
        }

        // Verify guest sees it
        unsafe {
            assert_eq!(std::ptr::read(guest.region().as_ptr().add(100)), 0x42);
        }

        // Owner resizes
        owner.resize(8192).unwrap();

        // Guest remaps
        guest.check_and_remap().unwrap();

        // Data should still be visible
        unsafe {
            assert_eq!(std::ptr::read(guest.region().as_ptr().add(100)), 0x42);
        }

        // Owner writes to new area
        unsafe {
            std::ptr::write(owner.region().as_ptr().add(5000), 0x99);
        }

        // Guest should see it
        unsafe {
            assert_eq!(std::ptr::read(guest.region().as_ptr().add(5000)), 0x99);
        }
    }
}
