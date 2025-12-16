//! SHM-backed allocator using `allocator-api2`.
//!
//! This allocator allows Rust types (e.g., `allocator_api2::vec::Vec<u8, ShmAllocator>`) to be
//! allocated directly in shared memory slots.
//!
//! Dropping an allocation attempts to free the slot if it was never sent (still in `Allocated`
//! state). If the allocation was sent, the transport transitions the slot to `InFlight` and the
//! receiver will free it.

use core::alloc::Layout;
use core::ptr::NonNull;

use allocator_api2::alloc::{AllocError, Allocator};

use super::session::ShmSession;
use std::sync::Arc;

/// Header stored at the front of each SHM allocation.
#[repr(C)]
struct ShmAllocHeader {
    slot: u32,
    generation: u32,
    len: u32,
    _pad: u32,
}

const HEADER_SIZE: usize = core::mem::size_of::<ShmAllocHeader>();
const _: () = assert!(HEADER_SIZE == 16);

#[derive(Clone)]
pub struct ShmAllocator {
    session: Arc<ShmSession>,
}

impl ShmAllocator {
    pub fn new(session: Arc<ShmSession>) -> Self {
        Self { session }
    }

    pub fn max_allocation_size(&self) -> usize {
        let slot_size = self.session.data_segment().slot_size() as usize;
        slot_size.saturating_sub(HEADER_SIZE)
    }

    pub fn session(&self) -> &Arc<ShmSession> {
        &self.session
    }

    fn full_layout(user_layout: Layout) -> Result<(Layout, usize), AllocError> {
        let header_layout = Layout::new::<ShmAllocHeader>();
        header_layout.extend(user_layout).map_err(|_| AllocError)
    }
}

unsafe impl Send for ShmAllocator {}
unsafe impl Sync for ShmAllocator {}

unsafe impl Allocator for ShmAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let (full_layout, user_offset) = Self::full_layout(layout)?;

        let data_segment = self.session.data_segment();
        let slot_size = data_segment.slot_size() as usize;
        if full_layout.size() > slot_size {
            return Err(AllocError);
        }

        let (slot, generation) = data_segment.alloc().map_err(|_| AllocError)?;

        // SAFETY: slot returned by alloc() is in-range.
        let base = unsafe { data_segment.data_ptr_public(slot) };

        let header = ShmAllocHeader {
            slot,
            generation,
            len: layout.size() as u32,
            _pad: 0,
        };
        unsafe {
            (base as *mut ShmAllocHeader).write(header);
        }

        let user_ptr = unsafe { base.add(user_offset) };
        let slice_ptr =
            NonNull::slice_from_raw_parts(NonNull::new(user_ptr).ok_or(AllocError)?, layout.size());
        Ok(slice_ptr)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        let (_, user_offset) = match Self::full_layout(layout) {
            Ok(v) => v,
            Err(_) => return,
        };

        let user_addr = ptr.as_ptr() as usize;
        let header_addr = user_addr - user_offset;
        let header_ptr = header_addr as *const ShmAllocHeader;

        let header = unsafe { header_ptr.read() };

        let data_segment = self.session.data_segment();
        let _ = data_segment.free_allocated(header.slot, header.generation);
    }
}
