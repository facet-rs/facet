# Phase 006: Variable-Size Slot Pools (Optional in spec, likely required for dodeca)

## Goal

Implement variable-size slot pools with multiple size classes for efficient
handling of diverse payload sizes. This is optional for MVP but improves
memory efficiency for real workloads with mixed payload sizes.

## Current State

The current implementation uses fixed-size slots:
- All slots are `slot_size` bytes
- Small payloads waste space
- Large payloads require fragmentation or rejection

Dodeca has diverse payloads: small RPC args, medium responses, large images/fonts.

## Target API

```rust
// Configure size classes
let config = SegmentConfig {
    size_classes: vec![
        SizeClass { slot_size: 1024, count: 1024 },    // 1 KB × 1024 = 1 MB
        SizeClass { slot_size: 16384, count: 256 },   // 16 KB × 256 = 4 MB
        SizeClass { slot_size: 262144, count: 32 },   // 256 KB × 32 = 8 MB
        SizeClass { slot_size: 4194304, count: 8 },   // 4 MB × 8 = 32 MB
    ],
    ..Default::default()
};

// Allocation finds smallest fitting class
let slot = pool.alloc(payload_size)?;  // Returns slot from appropriate class

// Or explicitly request a class
let slot = pool.alloc_from_class(2)?;  // Class index 2 (256 KB)
```

## Spec Rules

| Rule | Description |
|------|-------------|
| `shm.varslot.classes` | Size class configuration |
| `shm.varslot.selection` | Smallest-fit slot selection |
| `shm.varslot.shared` | Shared pool across all guests |
| `shm.varslot.ownership` | Per-slot ownership tracking |
| `shm.varslot.extents` | Extent-based growth (optional) |
| `shm.varslot.extent-layout` | Extent memory layout |
| `shm.varslot.freelist` | Treiber stack free list |
| `shm.varslot.allocation` | Lock-free allocation |
| `shm.varslot.freeing` | Lock-free freeing |

## Implementation Plan

### 1. Size Class Configuration

```rust
// layout.rs

/// A slot size class.
#[derive(Debug, Clone, Copy)]
pub struct SizeClass {
    /// Size of each slot in this class (bytes)
    pub slot_size: u32,
    /// Number of slots in this class
    pub count: u32,
}

impl SegmentConfig {
    /// Default size classes for typical workloads.
    pub fn default_size_classes() -> Vec<SizeClass> {
        vec![
            SizeClass { slot_size: 1024, count: 1024 },      // 1 KB
            SizeClass { slot_size: 16 * 1024, count: 256 },  // 16 KB
            SizeClass { slot_size: 256 * 1024, count: 32 },  // 256 KB
            SizeClass { slot_size: 4 * 1024 * 1024, count: 8 }, // 4 MB
        ]
    }
}
```

### 2. Slot Metadata

```rust
// slot_pool.rs

/// Per-slot metadata for ownership and free list.
///
/// shm[impl shm.varslot.ownership]
#[repr(C)]
pub struct SlotMeta {
    /// ABA counter, incremented on allocation
    pub generation: AtomicU32,
    /// Slot state: 0 = Free, 1 = Allocated, 2 = InFlight
    pub state: AtomicU32,
    /// Peer ID that allocated (0 = host, 1-255 = guest)
    pub owner_peer: AtomicU32,
    /// Free list link (next free slot index, or u32::MAX for end)
    pub next_free: AtomicU32,
}

#[repr(u32)]
pub enum SlotState {
    Free = 0,
    Allocated = 1,
    InFlight = 2,  // Sent but not yet freed by receiver
}
```

### 3. Size Class Header

```rust
// slot_pool.rs

/// Header for a single size class.
///
/// shm[impl shm.varslot.freelist]
#[repr(C)]
pub struct SizeClassHeader {
    /// Size of each slot in this class
    pub slot_size: u32,
    /// Number of slots in this class
    pub slot_count: u32,
    /// Free list head: packed (index, generation)
    /// Index in upper 32 bits, generation in lower 32 bits
    pub free_head: AtomicU64,
    /// Reserved for alignment
    pub _reserved: [u8; 48],
}
// Total: 64 bytes

impl SizeClassHeader {
    const END_OF_LIST: u64 = u64::MAX;
    
    fn pack(index: u32, gen: u32) -> u64 {
        ((index as u64) << 32) | (gen as u64)
    }
    
    fn unpack(packed: u64) -> (u32, u32) {
        let index = (packed >> 32) as u32;
        let gen = packed as u32;
        (index, gen)
    }
}
```

### 4. Variable Slot Pool

```rust
// slot_pool.rs

/// Variable-size slot pool with multiple size classes.
///
/// shm[impl shm.varslot.shared]
pub struct VarSlotPool {
    region: Region,
    /// Offset to first size class header
    base_offset: u64,
    /// Size class configurations
    classes: Vec<SizeClass>,
    /// Offsets to each class's slot area
    class_offsets: Vec<u64>,
}

impl VarSlotPool {
    /// Allocate a slot that can hold `size` bytes.
    ///
    /// shm[impl shm.varslot.selection]
    pub fn alloc(&self, size: u32, owner: u8) -> Option<VarSlotHandle> {
        // Find smallest class that fits
        for (class_idx, class) in self.classes.iter().enumerate() {
            if class.slot_size >= size {
                if let Some(handle) = self.alloc_from_class(class_idx, owner) {
                    return Some(handle);
                }
                // Class exhausted, try next larger
            }
        }
        None  // All classes exhausted
    }
    
    /// Allocate from a specific size class.
    ///
    /// shm[impl shm.varslot.allocation]
    pub fn alloc_from_class(&self, class_idx: usize, owner: u8) -> Option<VarSlotHandle> {
        let header = self.class_header(class_idx);
        
        loop {
            let head = header.free_head.load(Ordering::Acquire);
            if head == SizeClassHeader::END_OF_LIST {
                return None;  // Class exhausted
            }
            
            let (index, gen) = SizeClassHeader::unpack(head);
            let meta = self.slot_meta(class_idx, index);
            
            // Read next pointer before CAS
            let next = meta.next_free.load(Ordering::Acquire);
            let next_packed = if next == u32::MAX {
                SizeClassHeader::END_OF_LIST
            } else {
                SizeClassHeader::pack(next, gen.wrapping_add(1))
            };
            
            // Try to pop from free list
            match header.free_head.compare_exchange_weak(
                head,
                next_packed,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    // Success! Initialize slot metadata
                    meta.generation.fetch_add(1, Ordering::AcqRel);
                    meta.state.store(SlotState::Allocated as u32, Ordering::Release);
                    meta.owner_peer.store(owner as u32, Ordering::Release);
                    
                    return Some(VarSlotHandle {
                        class_idx: class_idx as u8,
                        slot_idx: index,
                        generation: meta.generation.load(Ordering::Acquire),
                    });
                }
                Err(_) => continue,  // Retry
            }
        }
    }
    
    /// Free a slot back to its pool.
    ///
    /// shm[impl shm.varslot.freeing]
    pub fn free(&self, handle: VarSlotHandle) -> Result<(), FreeError> {
        let meta = self.slot_meta(handle.class_idx as usize, handle.slot_idx);
        
        // Verify generation (detect double-free)
        let current_gen = meta.generation.load(Ordering::Acquire);
        if current_gen != handle.generation {
            return Err(FreeError::GenerationMismatch);
        }
        
        // Mark as free
        meta.state.store(SlotState::Free as u32, Ordering::Release);
        
        // Push to free list
        let header = self.class_header(handle.class_idx as usize);
        
        loop {
            let head = header.free_head.load(Ordering::Acquire);
            let (head_idx, head_gen) = if head == SizeClassHeader::END_OF_LIST {
                (u32::MAX, 0u32)
            } else {
                SizeClassHeader::unpack(head)
            };
            
            // Set our next pointer
            meta.next_free.store(head_idx, Ordering::Release);
            
            // Try to become new head
            let new_head = SizeClassHeader::pack(handle.slot_idx, head_gen.wrapping_add(1));
            
            match header.free_head.compare_exchange_weak(
                head,
                new_head,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(()),
                Err(_) => continue,  // Retry
            }
        }
    }
    
    /// Recover all slots owned by a crashed peer.
    pub fn recover_peer(&self, peer_id: u8) {
        for class_idx in 0..self.classes.len() {
            let class = &self.classes[class_idx];
            for slot_idx in 0..class.count {
                let meta = self.slot_meta(class_idx, slot_idx);
                
                let owner = meta.owner_peer.load(Ordering::Acquire);
                let state = meta.state.load(Ordering::Acquire);
                
                if owner == peer_id as u32 && state != SlotState::Free as u32 {
                    // Force free this slot
                    let handle = VarSlotHandle {
                        class_idx: class_idx as u8,
                        slot_idx,
                        generation: meta.generation.load(Ordering::Acquire),
                    };
                    let _ = self.free(handle);
                }
            }
        }
    }
}

/// Handle to an allocated variable-size slot.
#[derive(Debug, Clone, Copy)]
pub struct VarSlotHandle {
    pub class_idx: u8,
    pub slot_idx: u32,
    pub generation: u32,
}

#[derive(Debug)]
pub enum FreeError {
    GenerationMismatch,
}
```

### 5. Layout Calculations

```rust
// layout.rs

impl SegmentLayout {
    /// Calculate layout for variable-size pools.
    pub fn with_var_slots(config: &SegmentConfig) -> Result<Self, LayoutError> {
        let mut offset = HEADER_SIZE as u64;
        
        // Peer table
        let peer_table_offset = offset;
        offset += config.max_guests as u64 * PEER_ENTRY_SIZE as u64;
        
        // Per-guest rings (unchanged)
        // ...
        
        // Shared variable-size slot pool
        let var_slot_offset = Self::align_up(offset, 64);
        
        let mut class_offsets = Vec::new();
        let mut pool_offset = var_slot_offset;
        
        // Size class headers
        pool_offset += config.size_classes.len() as u64 * 64;
        
        for class in &config.size_classes {
            class_offsets.push(pool_offset);
            
            // SlotMeta array
            let meta_size = class.count as u64 * std::mem::size_of::<SlotMeta>() as u64;
            pool_offset += Self::align_up(meta_size, 64);
            
            // Slot data array
            let data_size = class.count as u64 * class.slot_size as u64;
            pool_offset += Self::align_up(data_size, 64);
        }
        
        // ...
    }
}
```

### 6. MsgDesc Changes

For variable slots, `MsgDesc.payload_slot` needs to encode both class and index:

```rust
// msg.rs

impl MsgDesc {
    /// Encode variable slot handle into payload fields.
    pub fn set_var_slot(&mut self, handle: VarSlotHandle) {
        // Pack class_idx in upper 8 bits, slot_idx in lower 24 bits
        self.payload_slot = ((handle.class_idx as u32) << 24) | (handle.slot_idx & 0x00FFFFFF);
        self.payload_generation = handle.generation;
    }
    
    /// Decode variable slot handle from payload fields.
    pub fn get_var_slot(&self) -> Option<VarSlotHandle> {
        if self.payload_slot == INLINE_PAYLOAD_SLOT {
            return None;
        }
        
        Some(VarSlotHandle {
            class_idx: (self.payload_slot >> 24) as u8,
            slot_idx: self.payload_slot & 0x00FFFFFF,
            generation: self.payload_generation,
        })
    }
}
```

## Tasks

- [ ] Add `SizeClass` configuration
- [ ] Add `SlotMeta` and `SizeClassHeader` types
- [ ] Implement `VarSlotPool` with lock-free alloc/free
- [ ] Update layout calculations
- [ ] Update `MsgDesc` for variable slot encoding
- [ ] Add peer recovery for crashed owner cleanup
- [ ] Integrate with host/guest send/recv
- [ ] Add tracey annotations
- [ ] Write tests

## Testing Strategy

```rust
#[test]
fn test_var_slot_allocation() {
    let pool = create_test_var_pool();
    
    // Small payload uses small class
    let small = pool.alloc(100, 0).unwrap();
    assert_eq!(small.class_idx, 0);  // 1 KB class
    
    // Medium payload uses medium class
    let medium = pool.alloc(10_000, 0).unwrap();
    assert_eq!(medium.class_idx, 1);  // 16 KB class
    
    // Large payload uses large class
    let large = pool.alloc(200_000, 0).unwrap();
    assert_eq!(large.class_idx, 2);  // 256 KB class
}

#[test]
fn test_var_slot_exhaustion_fallback() {
    let pool = create_test_var_pool();
    
    // Exhaust small class
    let mut handles = Vec::new();
    while let Some(h) = pool.alloc_from_class(0, 0) {
        handles.push(h);
    }
    
    // Next small alloc should use medium class
    let fallback = pool.alloc(100, 0).unwrap();
    assert_eq!(fallback.class_idx, 1);  // Fell back to 16 KB
}

#[test]
fn test_concurrent_alloc_free() {
    use std::sync::Arc;
    use std::thread;
    
    let pool = Arc::new(create_test_var_pool());
    let mut handles = Vec::new();
    
    for _ in 0..8 {
        let pool = pool.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..1000 {
                if let Some(h) = pool.alloc(100, 0) {
                    pool.free(h).unwrap();
                }
            }
        }));
    }
    
    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn test_peer_recovery() {
    let pool = create_test_var_pool();
    
    // Allocate slots for peer 1
    let h1 = pool.alloc(100, 1).unwrap();
    let h2 = pool.alloc(100, 1).unwrap();
    
    // Allocate for peer 2
    let h3 = pool.alloc(100, 2).unwrap();
    
    // Simulate peer 1 crash - recover its slots
    pool.recover_peer(1);
    
    // Peer 1's slots should be free, peer 2's should not
    // (test by re-allocating)
    let new1 = pool.alloc(100, 0).unwrap();
    let new2 = pool.alloc(100, 0).unwrap();
    
    // Should get the same slots back (or at least be able to allocate)
    assert!(new1.slot_idx == h1.slot_idx || new1.slot_idx == h2.slot_idx ||
            new2.slot_idx == h1.slot_idx || new2.slot_idx == h2.slot_idx);
}
```

## Dependencies

- Phases 001-003 for basic infrastructure
- No external crate dependencies

## Notes

- This phase is **optional for MVP** - fixed slots work, just less efficiently
- The Treiber stack is ABA-safe due to generation counters
- Extent-based growth (`shm.varslot.extents`) is deferred - requires segment resize
- Consider using `crossbeam-utils` for better CAS loops if performance matters
- Dodeca can start with fixed slots and migrate to variable later
