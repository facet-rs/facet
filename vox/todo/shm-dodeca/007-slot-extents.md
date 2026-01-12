# Phase 007: Extent-Based Slot Pool Growth

## Goal

Implement extent-based dynamic growth for variable-size slot pools. This allows
size classes to grow beyond their initial allocation when demand exceeds supply.

## Current State

Phase 006 implemented variable-size slot pools with fixed capacity per class.
When a size class is exhausted, allocation falls back to the next larger class.
This works but wastes memory when many small allocations consume large slots.

## Spec Rules

| Rule | Description |
|------|-------------|
| `shm.varslot.extents` | Extent-based growth mechanism |
| `shm.varslot.extent-layout` | Extent memory layout |

## Design Considerations

### Segment Resizing

Extent-based growth requires expanding the shared memory segment:

1. **File-backed segments**: Can use `ftruncate()` to grow the file, then
   `mremap()` (Linux) or remap (other platforms) to extend the mapping.

2. **Cross-process coordination**: All attached guests must be notified to
   remap their views. Options:
   - Doorbell signal + poll for new segment size
   - Store `current_size` in header, guests check periodically
   - Use separate extent files that guests mmap independently

3. **Lock-free growth**: The extent table must be updatable without blocking
   ongoing allocations in existing extents.

### Extent Table Design

```rust
/// Maximum number of extents per size class.
pub const MAX_EXTENTS_PER_CLASS: usize = 16;

/// Header for a size class with extent support.
#[repr(C, align(64))]
pub struct ExtentSizeClassHeader {
    /// Size of each slot in this class.
    pub slot_size: u32,
    /// Slots per extent.
    pub slots_per_extent: u32,
    /// Number of extents currently allocated (atomic for lock-free read).
    pub extent_count: AtomicU32,
    /// Reserved for alignment.
    pub _reserved: u32,
    /// Offsets to each extent (relative to segment start).
    /// Only entries 0..extent_count are valid.
    pub extent_offsets: [AtomicU64; MAX_EXTENTS_PER_CLASS],
}

/// Per-extent header.
#[repr(C, align(64))]
pub struct ExtentHeader {
    /// Size class this extent belongs to.
    pub class_idx: u32,
    /// Extent index within the class.
    pub extent_idx: u32,
    /// Number of slots in this extent.
    pub slot_count: u32,
    /// Free list head for this extent.
    pub free_head: AtomicU64,
    /// Reserved.
    pub _reserved: [u8; 40],
}
```

### Allocation Strategy

When allocating from a size class with extents:

1. Try each extent in order (0 to extent_count-1)
2. If all extents exhausted, request growth (host-only operation)
3. Growth atomically increments `extent_count` after initializing new extent
4. Guests observe new extent on next allocation attempt

### Growth Protocol

Only the host can grow the segment:

```rust
impl VarSlotPool {
    /// Request growth of a size class (host only).
    ///
    /// Returns the new extent index, or None if max extents reached.
    pub fn grow_class(&mut self, class_idx: usize) -> Option<u32> {
        let header = self.class_header_mut(class_idx);
        let current = header.extent_count.load(Ordering::Acquire);
        
        if current as usize >= MAX_EXTENTS_PER_CLASS {
            return None; // Max extents reached
        }
        
        // 1. Grow segment file
        // 2. Remap or extend mapping
        // 3. Initialize new extent at end of segment
        // 4. Store offset in extent_offsets[current]
        // 5. Atomically increment extent_count
        
        // ...
        
        Some(current)
    }
}
```

## Implementation Plan

### 1. Extend Segment Header

Add fields to track dynamic segment size:

```rust
pub struct SegmentHeader {
    // ... existing fields ...
    
    /// Current segment size (may grow beyond initial total_size).
    pub current_size: AtomicU64,
    /// Flags indicating segment capabilities.
    pub flags: u32,
}

pub mod segment_flags {
    /// Segment supports extent-based growth.
    pub const EXTENTS_ENABLED: u32 = 1 << 0;
}
```

### 2. Extent-Aware VarSlotPool

```rust
pub struct ExtentVarSlotPool {
    region: Region,
    base_offset: u64,
    classes: Vec<SizeClass>,
    /// Cached extent count per class (refreshed on allocation failure).
    cached_extent_counts: Vec<u32>,
}

impl ExtentVarSlotPool {
    pub fn alloc(&self, size: u32, owner: u8) -> Option<VarSlotHandle> {
        for (class_idx, class) in self.classes.iter().enumerate() {
            if class.slot_size >= size {
                // Try each extent
                let extent_count = self.extent_count(class_idx);
                for extent_idx in 0..extent_count {
                    if let Some(handle) = self.alloc_from_extent(class_idx, extent_idx, owner) {
                        return Some(handle);
                    }
                }
                // All extents exhausted, try next class
            }
        }
        None
    }
}
```

### 3. Cross-Process Remap Notification

```rust
/// Check if segment has grown and remap if needed.
pub fn check_and_remap(&mut self) -> bool {
    let header = self.header();
    let current = header.current_size.load(Ordering::Acquire);
    
    if current > self.mapped_size {
        // Platform-specific remap
        #[cfg(target_os = "linux")]
        {
            // Use mremap(MREMAP_MAYMOVE) to extend mapping
        }
        #[cfg(not(target_os = "linux"))]
        {
            // Unmap and remap with new size
        }
        true
    } else {
        false
    }
}
```

## Tasks

- [ ] Add `current_size` and `flags` to SegmentHeader
- [ ] Implement `ExtentSizeClassHeader` and `ExtentHeader`
- [ ] Implement extent-aware allocation in `VarSlotPool`
- [ ] Implement `grow_class()` for host
- [ ] Implement cross-process remap notification
- [ ] Add platform-specific remap implementations
- [ ] Update tracey annotations
- [ ] Write tests for extent growth

## Dependencies

- Phase 006 (variable-size slot pools)

## Notes

- This is a significant complexity increase; evaluate if dodeca actually needs
  dynamic growth or if generous initial sizing is sufficient
- Consider a simpler "extent files" approach where each extent is a separate
  mmap, avoiding segment resize entirely
- mremap with MREMAP_MAYMOVE may change the base address, requiring all
  pointers to be recalculated as offsets
