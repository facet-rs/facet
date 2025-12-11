//! SHM-backed allocator using `allocator-api2`.
//!
//! This allocator allows Rust types (e.g., `Vec<u8, ShmAllocator>`) to be allocated
//! directly in shared memory slots. When such data is passed to the encoder, it can
//! detect that the bytes are already in SHM and avoid copying.
//!
//! # Design
//!
//! Each allocation stores a small header at the front of the slot containing:
//! - slot index
//! - generation
//! - length
//!
//! This allows `deallocate` to recover the slot info and free it properly.
//!
//! # Example
//!
//! ```ignore
//! use allocator_api2::vec::Vec;
//! use rapace_transport_shm::{ShmSession, ShmAllocator};
//!
//! let (session, _) = ShmSession::create_pair().unwrap();
//! let alloc = ShmAllocator::new(session);
//!
//! // Allocate a Vec in SHM
//! let mut buf: Vec<u8, _> = Vec::new_in(alloc.clone());
//! buf.extend_from_slice(b"hello, world!");
//!
//! // When passed to the encoder, this will be detected as already in SHM
//! // and referenced zero-copy.
//! ```

use core::alloc::Layout;
use core::ptr::NonNull;

use allocator_api2::alloc::{AllocError, Allocator};

use crate::session::ShmSession;
use std::sync::Arc;

/// Header stored at the front of each SHM allocation.
///
/// This allows `deallocate` to recover the slot info.
#[repr(C)]
struct ShmAllocHeader {
    /// Slot index in the data segment.
    slot: u32,
    /// Generation at time of allocation (for freeing).
    generation: u32,
    /// User-requested allocation size.
    len: u32,
    /// Padding for alignment.
    _pad: u32,
}

const HEADER_SIZE: usize = core::mem::size_of::<ShmAllocHeader>();
const _: () = assert!(HEADER_SIZE == 16);

/// An allocator that allocates from SHM slots.
///
/// Each allocation consumes one slot. The slot size limits the maximum
/// allocation size (minus header overhead).
///
/// This allocator is `Clone` and `Send + Sync`, allowing it to be shared.
#[derive(Clone)]
pub struct ShmAllocator {
    session: Arc<ShmSession>,
}

impl ShmAllocator {
    /// Create a new SHM allocator from a session.
    pub fn new(session: Arc<ShmSession>) -> Self {
        Self { session }
    }

    /// Get the maximum allocation size (slot size minus header).
    pub fn max_allocation_size(&self) -> usize {
        let slot_size = self.session.data_segment().slot_size() as usize;
        slot_size.saturating_sub(HEADER_SIZE)
    }

    /// Get the underlying session.
    pub fn session(&self) -> &Arc<ShmSession> {
        &self.session
    }

    /// Calculate the full layout including header.
    fn full_layout(user_layout: Layout) -> Result<(Layout, usize), AllocError> {
        let header_layout = Layout::new::<ShmAllocHeader>();
        header_layout.extend(user_layout).map_err(|_| AllocError)
    }
}

// SAFETY: ShmAllocator is Send + Sync because ShmSession is Send + Sync,
// and all operations use proper atomic synchronization.
unsafe impl Send for ShmAllocator {}
unsafe impl Sync for ShmAllocator {}

unsafe impl Allocator for ShmAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let (full_layout, user_offset) = Self::full_layout(layout)?;

        let data_segment = self.session.data_segment();
        let slot_size = data_segment.slot_size() as usize;

        // Check if allocation fits in a slot.
        if full_layout.size() > slot_size {
            return Err(AllocError);
        }

        // Allocate a slot.
        let (slot, generation) = data_segment.alloc().map_err(|_| AllocError)?;

        // Get the base pointer for this slot.
        // SAFETY: slot < slot_count, guaranteed by alloc().
        let base = unsafe { data_segment.data_ptr_public(slot) };

        // Write the header at base.
        let header = ShmAllocHeader {
            slot,
            generation,
            len: layout.size() as u32,
            _pad: 0,
        };
        // SAFETY: base is valid and aligned for ShmAllocHeader (16-byte struct).
        unsafe {
            (base as *mut ShmAllocHeader).write(header);
        }

        // User pointer starts after header.
        // SAFETY: user_offset is within the slot (checked above).
        let user_ptr = unsafe { base.add(user_offset) };

        // Create the slice pointer.
        let slice_ptr =
            NonNull::slice_from_raw_parts(NonNull::new(user_ptr).ok_or(AllocError)?, layout.size());

        Ok(slice_ptr)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        let (_, user_offset) = match Self::full_layout(layout) {
            Ok(v) => v,
            Err(_) => return, // Invalid layout, can't deallocate.
        };

        // Recover the header from before the user pointer.
        let user_addr = ptr.as_ptr() as usize;
        let header_addr = user_addr - user_offset;
        let header_ptr = header_addr as *const ShmAllocHeader;

        // SAFETY: header_ptr points to the header we wrote in allocate().
        let header = unsafe { header_ptr.read() };

        // Free the slot.
        // Note: We need to mark it as InFlight first if it's still Allocated.
        // The slot state depends on whether the data was sent or not.
        //
        // For now, we'll try to transition Allocated -> Free directly.
        // This is safe for local allocations that were never sent.
        let data_segment = self.session.data_segment();
        let _ = data_segment.free_allocated(header.slot, header.generation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use allocator_api2::boxed::Box as AllocBox;
    use allocator_api2::vec::Vec as AllocVec;

    #[test]
    fn test_allocator_basic() {
        let (session, _) = ShmSession::create_pair().unwrap();
        let alloc = ShmAllocator::new(session);

        // Allocate a Box.
        let boxed: AllocBox<u64, _> = AllocBox::new_in(42, alloc.clone());
        assert_eq!(*boxed, 42);

        // Should be in SHM.
        let ptr = &*boxed as *const u64 as *const u8;
        assert!(alloc.session.contains_range(ptr, 8));
    }

    #[test]
    fn test_allocator_vec() {
        let (session, _) = ShmSession::create_pair().unwrap();
        let alloc = ShmAllocator::new(session);

        // Allocate a Vec.
        let mut vec: AllocVec<u8, _> = AllocVec::new_in(alloc.clone());
        vec.extend_from_slice(b"hello, shm!");

        // Data should be in SHM.
        let ptr = vec.as_ptr();
        let len = vec.len();
        assert!(alloc.session.contains_range(ptr, len));

        // Should be findable by find_slot_location.
        let (slot, offset) = alloc.session.find_slot_location(ptr, len).unwrap();
        // Slot should be valid (0..slot_count).
        assert!(slot < alloc.session.data_segment().slot_count());
        // Offset should account for header.
        assert!(offset >= HEADER_SIZE as u32);
    }

    #[test]
    fn test_allocator_multiple() {
        let (session, _) = ShmSession::create_pair().unwrap();
        let alloc = ShmAllocator::new(session);

        // Allocate multiple items.
        let b1: AllocBox<[u8; 100], _> = AllocBox::new_in([1u8; 100], alloc.clone());
        let b2: AllocBox<[u8; 100], _> = AllocBox::new_in([2u8; 100], alloc.clone());
        let b3: AllocBox<[u8; 100], _> = AllocBox::new_in([3u8; 100], alloc.clone());

        // Each should be in a different slot.
        let p1 = b1.as_ptr();
        let p2 = b2.as_ptr();
        let p3 = b3.as_ptr();

        let (s1, _) = alloc.session.find_slot_location(p1, 100).unwrap();
        let (s2, _) = alloc.session.find_slot_location(p2, 100).unwrap();
        let (s3, _) = alloc.session.find_slot_location(p3, 100).unwrap();

        assert_ne!(s1, s2);
        assert_ne!(s2, s3);
        assert_ne!(s1, s3);
    }

    #[test]
    fn test_allocator_too_large() {
        let (session, _) = ShmSession::create_pair().unwrap();
        let alloc = ShmAllocator::new(session);

        // Try to allocate more than slot_size.
        let max = alloc.max_allocation_size();
        let layout = Layout::from_size_align(max + 1, 1).unwrap();

        // Should fail.
        assert!(alloc.allocate(layout).is_err());
    }

    #[test]
    fn test_allocator_dealloc() {
        let (session, _) = ShmSession::create_pair().unwrap();
        let alloc = ShmAllocator::new(session);
        let slot_count = alloc.session.data_segment().slot_count();

        // Allocate all slots.
        let mut boxes: Vec<AllocBox<[u8; 100], _>> = Vec::new();
        for _ in 0..slot_count {
            boxes.push(AllocBox::new_in([0u8; 100], alloc.clone()));
        }

        // Should be out of slots now.
        let layout = Layout::from_size_align(100, 1).unwrap();
        assert!(alloc.allocate(layout).is_err());

        // Drop half of them.
        boxes.truncate(slot_count as usize / 2);

        // Should be able to allocate again.
        let _new_box: AllocBox<[u8; 100], _> = AllocBox::new_in([0u8; 100], alloc.clone());
    }
}
