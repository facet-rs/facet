# Phase 001: Mmap-Backed Regions

## Goal

Replace heap-backed memory (`HeapRegion`) with file-backed mmap for cross-process
shared memory. This is the foundation for all other SHM features.

## Current State

Currently `ShmHost` and `ShmGuest` use `HeapRegion` from `shm-primitives`:

```rust
// host.rs
let backing = HeapRegion::new_zeroed(layout.total_size as usize);
let region = backing.region();

// guest.rs  
pub fn attach(region: Region) -> Result<Self, AttachError>
// ^ Takes a Region, but caller must provide it (no file path API)
```

This works for in-process testing but not for real cross-process IPC.

## Target API

```rust
// Host creates file-backed segment
let host = ShmHost::create("/dev/shm/myapp.shm", config)?;

// Guest attaches to existing segment by path
let guest = ShmGuest::attach_path("/dev/shm/myapp.shm")?;
// or with pre-assigned peer_id:
let guest = ShmGuest::attach_path_with_id("/dev/shm/myapp.shm", peer_id)?;
```

## Spec Rules

| Rule | Description |
|------|-------------|
| `shm.file.path` | Segment file location conventions |
| `shm.file.create` | Creating segment file (open, truncate, mmap, init, magic last) |
| `shm.file.attach` | Attaching to segment (open, mmap, validate, attach) |
| `shm.file.permissions` | File permissions (0600 or 0660) |
| `shm.file.cleanup` | Delete file on graceful shutdown |
| `shm.file.mmap-posix` | POSIX mmap with `MAP_SHARED` |
| `shm.file.mmap-windows` | Windows `CreateFileMapping` (optional) |

## Implementation Plan

### 1. Add `MmapRegion` to shm-primitives

```rust
// shm-primitives/src/mmap.rs

use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::path::Path;

/// File-backed memory-mapped region.
/// 
/// shm[impl shm.file.mmap-posix]
pub struct MmapRegion {
    ptr: *mut u8,
    len: usize,
    file: File,  // Keep file open to maintain mapping
}

impl MmapRegion {
    /// Create a new file-backed region.
    /// 
    /// shm[impl shm.file.create]
    pub fn create(path: &Path, size: usize) -> io::Result<Self> {
        // 1. Open or create file
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        
        // 2. Set permissions
        // shm[impl shm.file.permissions]
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        
        // 3. Truncate to size
        file.set_len(size as u64)?;
        
        // 4. mmap with MAP_SHARED
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        
        if ptr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        
        Ok(Self {
            ptr: ptr as *mut u8,
            len: size,
            file,
        })
    }
    
    /// Attach to an existing file-backed region.
    /// 
    /// shm[impl shm.file.attach]
    pub fn attach(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;
        
        let metadata = file.metadata()?;
        let size = metadata.len() as usize;
        
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        
        if ptr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        
        Ok(Self {
            ptr: ptr as *mut u8,
            len: size,
            file,
        })
    }
    
    pub fn region(&self) -> Region {
        Region::new(self.ptr, self.len)
    }
}

impl Drop for MmapRegion {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr as *mut libc::c_void, self.len);
        }
        // File is closed automatically when dropped
    }
}

// Safety: The mmap region is valid for the lifetime of MmapRegion
unsafe impl Send for MmapRegion {}
unsafe impl Sync for MmapRegion {}
```

### 2. Update ShmHost

```rust
// host.rs

pub struct ShmHost {
    /// Backing memory
    backing: ShmBacking,
    /// Path to segment file (for cleanup)
    path: Option<PathBuf>,
    // ... rest unchanged
}

enum ShmBacking {
    Heap(HeapRegion),
    Mmap(MmapRegion),
}

impl ShmHost {
    /// Create a file-backed SHM segment.
    /// 
    /// shm[impl shm.file.create]
    pub fn create<P: AsRef<Path>>(path: P, config: SegmentConfig) -> io::Result<Self> {
        let layout = config.layout()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        
        let backing = MmapRegion::create(path.as_ref(), layout.total_size as usize)?;
        let region = backing.region();
        
        // Initialize (same as before, but write magic LAST)
        unsafe {
            Self::init_peer_table(&region, &layout);
            Self::init_slot_pools(&region, &layout);
            Self::init_guest_areas(&region, &layout);
            // Magic last signals segment is ready
            Self::init_header(&region, &layout);
        }
        
        Ok(Self {
            backing: ShmBacking::Mmap(backing),
            path: Some(path.as_ref().to_path_buf()),
            region,
            layout,
            // ...
        })
    }
    
    /// Create a heap-backed SHM segment (for testing).
    pub fn create_heap(config: SegmentConfig) -> io::Result<Self> {
        // Existing implementation
    }
}

impl Drop for ShmHost {
    fn drop(&mut self) {
        // shm[impl shm.file.cleanup]
        if let Some(ref path) = self.path {
            let _ = std::fs::remove_file(path);
        }
    }
}
```

### 3. Update ShmGuest

```rust
// guest.rs

pub struct ShmGuest {
    /// Backing memory (None if attached to external region)
    backing: Option<ShmBacking>,
    // ... rest unchanged
}

impl ShmGuest {
    /// Attach to a file-backed SHM segment by path.
    /// 
    /// shm[impl shm.file.attach]
    pub fn attach_path<P: AsRef<Path>>(path: P) -> Result<Self, AttachError> {
        let backing = MmapRegion::attach(path.as_ref())
            .map_err(AttachError::Io)?;
        let region = backing.region();
        
        let mut guest = Self::attach(region)?;
        guest.backing = Some(ShmBacking::Mmap(backing));
        Ok(guest)
    }
    
    /// Attach with a pre-assigned peer ID (for spawned guests).
    pub fn attach_path_with_id<P: AsRef<Path>>(
        path: P,
        peer_id: PeerId,
    ) -> Result<Self, AttachError> {
        let backing = MmapRegion::attach(path.as_ref())
            .map_err(AttachError::Io)?;
        let region = backing.region();
        
        // Validate and claim the reserved slot instead of finding empty one
        // ...
    }
    
    /// Attach to an existing region (for testing or external mapping).
    pub fn attach(region: Region) -> Result<Self, AttachError> {
        // Existing implementation (unchanged)
    }
}
```

### 4. Add Integration Test

```rust
// tests/mmap.rs

#[test]
fn test_file_backed_segment() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.shm");
    
    // Host creates segment
    let config = SegmentConfig::default();
    let mut host = ShmHost::create(&path, config).unwrap();
    
    // Guest attaches
    let mut guest = ShmGuest::attach_path(&path).unwrap();
    
    // Send message host -> guest
    let frame = Frame::new_inline(MsgDesc { ... }, b"hello");
    host.send(guest.peer_id(), frame.clone()).unwrap();
    
    // Guest receives
    let received = guest.recv().unwrap();
    assert_eq!(received.payload(), frame.payload());
}

#[test]
fn test_segment_cleanup_on_drop() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.shm");
    
    {
        let _host = ShmHost::create(&path, SegmentConfig::default()).unwrap();
        assert!(path.exists());
    }
    
    // File should be deleted after host is dropped
    assert!(!path.exists());
}
```

## Tasks

- [ ] Add `MmapRegion` to `shm-primitives`
- [ ] Add `ShmBacking` enum to handle both heap and mmap
- [ ] Update `ShmHost::create()` to take a path
- [ ] Add `ShmHost::create_heap()` for testing
- [ ] Update `ShmGuest::attach_path()`
- [ ] Add file cleanup in `ShmHost::drop()`
- [ ] Add tracey annotations for spec rules
- [ ] Write integration tests for cross-process IPC
- [ ] Test on Linux (primary target)
- [ ] Test on macOS

## Testing Strategy

1. **Unit tests**: Use `HeapRegion` (fast, no filesystem)
2. **Integration tests**: Use `MmapRegion` with tempfile
3. **Cross-process test**: Fork and communicate via SHM

## Dependencies

- `libc` crate for mmap syscalls
- `tempfile` crate for tests

## Notes

- Windows support (`shm.file.mmap-windows`) can be added later if needed
- The existing `HeapRegion` path remains useful for unit tests
- Magic is written last to signal segment readiness (atomic visibility)
