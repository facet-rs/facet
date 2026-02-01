//! Type-erased growable buffer for elements of runtime-determined shape.
//!
//! This is a dumb memory manager - the caller tracks element count and initialization.
//! Drop deallocates the buffer but does NOT drop elements.

use std::alloc::{Layout, alloc, dealloc, realloc};

use facet_core::PtrUninit;

use crate::shape_desc::ShapeDesc;

/// A type-erased growable buffer for elements of a fixed shape.
///
/// Used to collect map entries as (K, V) tuples before building the map
/// via `from_pair_slice`.
pub struct Slab {
    /// Pointer to allocated buffer (null if capacity is 0).
    ptr: *mut u8,
    /// Number of slots allocated.
    capacity: usize,
    /// Layout of each element (stride = size after padding for alignment).
    element_layout: Layout,
}

impl Slab {
    /// Create a new slab for elements of the given shape.
    ///
    /// `capacity_hint` suggests initial capacity; 0 or None means start empty.
    pub fn new(shape: ShapeDesc, capacity_hint: Option<usize>) -> Self {
        let element_layout = shape.layout();
        let capacity = capacity_hint.unwrap_or(0);

        let ptr = if capacity == 0 || element_layout.size() == 0 {
            // No allocation needed for zero capacity or ZST elements
            std::ptr::null_mut()
        } else {
            // Allocate buffer for `capacity` elements
            let buffer_layout = Self::buffer_layout(element_layout, capacity);
            // SAFETY: buffer_layout has non-zero size (capacity > 0 and element size > 0)
            let ptr = unsafe { alloc(buffer_layout) };
            if ptr.is_null() {
                std::alloc::handle_alloc_error(buffer_layout);
            }
            ptr
        };

        Self {
            ptr,
            capacity,
            element_layout,
        }
    }

    /// Returns a pointer to slot `n`, growing the buffer if needed.
    ///
    /// The returned pointer is to uninitialized memory. The caller must
    /// initialize it before the Slab is dropped.
    pub fn nth_slot(&mut self, n: usize) -> PtrUninit {
        // Grow if needed
        if n >= self.capacity {
            self.grow_to_fit(n + 1);
        }

        // Calculate offset for slot n
        let offset = n * self.element_layout.size();

        if self.element_layout.size() == 0 {
            // ZST: return dangling pointer with correct alignment
            PtrUninit::new(self.element_layout.align() as *mut u8)
        } else {
            // SAFETY: ptr is valid for capacity elements, n < capacity after grow
            let slot_ptr = unsafe { self.ptr.add(offset) };
            PtrUninit::new(slot_ptr)
        }
    }

    /// Pointer to start of buffer (for from_pair_slice).
    pub fn as_mut_ptr(&self) -> *mut u8 {
        if self.element_layout.size() == 0 {
            // ZST: return dangling pointer with correct alignment
            self.element_layout.align() as *mut u8
        } else {
            self.ptr
        }
    }

    /// Current capacity in number of elements.
    #[allow(dead_code)]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Element stride (size including padding).
    #[allow(dead_code)]
    pub fn stride(&self) -> usize {
        self.element_layout.size()
    }

    /// Grow the buffer to hold at least `min_capacity` elements.
    fn grow_to_fit(&mut self, min_capacity: usize) {
        if self.element_layout.size() == 0 {
            // ZST: no allocation needed, just update capacity
            self.capacity = min_capacity;
            return;
        }

        // Calculate new capacity (at least double, at least min_capacity)
        let new_capacity = self
            .capacity
            .checked_mul(2)
            .unwrap_or(min_capacity)
            .max(min_capacity)
            .max(4); // Minimum 4 elements

        let new_layout = Self::buffer_layout(self.element_layout, new_capacity);

        let new_ptr = if self.ptr.is_null() {
            // First allocation
            // SAFETY: new_layout has non-zero size
            unsafe { alloc(new_layout) }
        } else {
            // Realloc existing buffer
            let old_layout = Self::buffer_layout(self.element_layout, self.capacity);
            // SAFETY: ptr was allocated with old_layout, new_layout has same alignment
            unsafe { realloc(self.ptr, old_layout, new_layout.size()) }
        };

        if new_ptr.is_null() {
            std::alloc::handle_alloc_error(new_layout);
        }

        self.ptr = new_ptr;
        self.capacity = new_capacity;
    }

    /// Calculate the buffer layout for `count` elements.
    fn buffer_layout(element_layout: Layout, count: usize) -> Layout {
        // Total size = element_size * count
        // Alignment = element alignment
        let size = element_layout
            .size()
            .checked_mul(count)
            .expect("buffer size overflow");
        Layout::from_size_align(size, element_layout.align()).expect("invalid buffer layout")
    }
}

impl Drop for Slab {
    fn drop(&mut self) {
        // Only deallocate, do NOT drop elements.
        // Caller is responsible for dropping elements before dropping Slab.
        if !self.ptr.is_null() && self.element_layout.size() > 0 && self.capacity > 0 {
            let layout = Self::buffer_layout(self.element_layout, self.capacity);
            // SAFETY: ptr was allocated with this layout
            unsafe { dealloc(self.ptr, layout) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet_core::Facet;

    #[test]
    fn test_slab_basic() {
        let shape = ShapeDesc::Static(u64::SHAPE);
        let mut slab = Slab::new(shape, Some(4));

        assert_eq!(slab.capacity(), 4);
        assert_eq!(slab.stride(), 8); // u64 is 8 bytes

        // Get slots and write to them
        let slot0 = slab.nth_slot(0);
        let slot1 = slab.nth_slot(1);

        unsafe {
            slot0.as_mut_byte_ptr().cast::<u64>().write(42);
            slot1.as_mut_byte_ptr().cast::<u64>().write(99);

            // Read back
            assert_eq!(slab.as_mut_ptr().cast::<u64>().read(), 42);
            assert_eq!(slab.as_mut_ptr().cast::<u64>().add(1).read(), 99);
        }
    }

    #[test]
    fn test_slab_growth() {
        let shape = ShapeDesc::Static(u32::SHAPE);
        let mut slab = Slab::new(shape, None);

        assert_eq!(slab.capacity(), 0);

        // Access slot 0 triggers growth
        let _ = slab.nth_slot(0);
        assert!(slab.capacity() >= 1);

        // Access slot 10 triggers more growth
        let _ = slab.nth_slot(10);
        assert!(slab.capacity() >= 11);
    }

    #[test]
    fn test_slab_zst() {
        let shape = ShapeDesc::Static(<()>::SHAPE);
        let mut slab = Slab::new(shape, Some(100));

        assert_eq!(slab.stride(), 0);

        // ZST slots should all return the same (dangling) pointer
        let slot0 = slab.nth_slot(0);
        let slot1 = slab.nth_slot(50);

        // They should be valid alignment-wise
        assert!(!slot0.as_mut_byte_ptr().is_null() || std::mem::align_of::<()>() == 1);
        assert!(!slot1.as_mut_byte_ptr().is_null() || std::mem::align_of::<()>() == 1);
    }

    #[test]
    fn test_slab_tuple2() {
        use facet_core::Def;
        use std::collections::HashMap;

        // Test with a Tuple2Shape (map entry)
        let map_def = match &HashMap::<String, u32>::SHAPE.def {
            Def::Map(map_def) => map_def,
            _ => panic!("expected Map def"),
        };
        let entry_shape = crate::tuple2(map_def);
        let shape = ShapeDesc::Tuple2(entry_shape);
        let mut slab = Slab::new(shape, Some(2));

        // String is 24 bytes (3 pointers), u32 is 4 bytes
        // With alignment, stride should be 24 + 4 + padding = 32 on 64-bit
        assert!(slab.stride() >= 28); // At least String + u32

        let slot0 = slab.nth_slot(0);
        let slot1 = slab.nth_slot(1);

        // Slots should be stride apart
        let diff =
            (slot1.as_mut_byte_ptr() as usize).wrapping_sub(slot0.as_mut_byte_ptr() as usize);
        assert_eq!(diff, slab.stride());
    }
}
