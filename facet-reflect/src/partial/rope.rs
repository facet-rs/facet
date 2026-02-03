//! Rope data structure for stable list element storage.
//!
//! A rope is a list of fixed-size chunks. Chunks never reallocate - when one fills up,
//! a new chunk is added. This keeps element pointers stable during list building,
//! enabling deferred frame processing for elements inside the rope.
//!
//! On finalization, elements are moved from the rope into the target Vec.

use core::alloc::Layout;
use core::ptr::NonNull;

/// A rope for storing list elements in stable memory.
///
/// Elements are stored in fixed-size chunks. Each chunk is a separately allocated
/// buffer that never moves. When a chunk fills up, a new chunk is allocated.
#[derive(Debug)]
pub(crate) struct ListRope {
    /// The chunks holding elements. Each chunk is a separately allocated buffer.
    chunks: Vec<NonNull<u8>>,
    /// Layout of a single element
    element_layout: Layout,
    /// Number of elements per chunk
    elements_per_chunk: usize,
    /// Total number of elements currently stored
    len: usize,
    /// Number of elements that have been fully initialized
    initialized_count: usize,
}

impl ListRope {
    /// Default number of elements per chunk.
    /// Chosen to balance memory overhead vs allocation frequency.
    const DEFAULT_CHUNK_CAPACITY: usize = 16;

    /// Create a new rope for elements with the given layout.
    ///
    /// # Panics
    ///
    /// Panics if element_layout.size() is 0 (ZST should not use rope).
    pub fn new(element_layout: Layout) -> Self {
        assert!(
            element_layout.size() > 0,
            "ListRope should not be used for zero-sized types"
        );

        Self {
            chunks: Vec::new(),
            element_layout,
            elements_per_chunk: Self::DEFAULT_CHUNK_CAPACITY,
            len: 0,
            initialized_count: 0,
        }
    }

    /// Create a new rope with a specific chunk capacity.
    #[cfg(test)]
    pub fn with_chunk_capacity(element_layout: Layout, elements_per_chunk: usize) -> Self {
        assert!(
            element_layout.size() > 0,
            "ListRope should not be used for zero-sized types"
        );
        assert!(elements_per_chunk > 0, "elements_per_chunk must be > 0");

        Self {
            chunks: Vec::new(),
            element_layout,
            elements_per_chunk,
            len: 0,
            initialized_count: 0,
        }
    }

    /// Returns the number of elements in the rope.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the rope is empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the layout of elements in this rope.
    pub fn element_layout(&self) -> Layout {
        self.element_layout
    }

    /// Allocate space for a new element and return a pointer to it.
    ///
    /// The returned pointer is stable - it will not be invalidated by
    /// subsequent calls to `push_uninit()`.
    pub fn push_uninit(&mut self) -> NonNull<u8> {
        let chunk_idx = self.len / self.elements_per_chunk;
        let idx_in_chunk = self.len % self.elements_per_chunk;

        // Allocate a new chunk if needed
        if chunk_idx >= self.chunks.len() {
            let chunk = self.allocate_chunk();
            self.chunks.push(chunk);
        }

        let chunk_ptr = self.chunks[chunk_idx];
        let element_ptr = unsafe {
            chunk_ptr
                .as_ptr()
                .add(idx_in_chunk * self.element_layout.size())
        };

        self.len += 1;

        // Safety: element_ptr is within the allocated chunk
        unsafe { NonNull::new_unchecked(element_ptr) }
    }

    /// Mark the last pushed element as initialized.
    ///
    /// This should be called after the element has been fully constructed.
    pub fn mark_last_initialized(&mut self) {
        debug_assert!(
            self.initialized_count < self.len,
            "mark_last_initialized called but no uninitialized elements"
        );
        self.initialized_count += 1;
    }

    /// Returns the number of initialized elements.
    pub fn initialized_count(&self) -> usize {
        self.initialized_count
    }

    /// Get a pointer to the element at the given index.
    ///
    /// # Safety
    ///
    /// The caller must ensure `index < self.len()`.
    #[allow(dead_code)]
    pub unsafe fn get_ptr(&self, index: usize) -> NonNull<u8> {
        debug_assert!(index < self.len, "index out of bounds");

        let chunk_idx = index / self.elements_per_chunk;
        let idx_in_chunk = index % self.elements_per_chunk;

        let chunk_ptr = self.chunks[chunk_idx];
        unsafe {
            let element_ptr = chunk_ptr
                .as_ptr()
                .add(idx_in_chunk * self.element_layout.size());
            NonNull::new_unchecked(element_ptr)
        }
    }

    /// Iterate over all initialized elements, yielding pointers to each.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `initialized_count` elements have actually
    /// been initialized.
    pub unsafe fn iter_initialized(&self) -> impl Iterator<Item = NonNull<u8>> + '_ {
        (0..self.initialized_count).map(|i| {
            let chunk_idx = i / self.elements_per_chunk;
            let idx_in_chunk = i % self.elements_per_chunk;
            let chunk_ptr = self.chunks[chunk_idx];
            unsafe {
                let element_ptr = chunk_ptr
                    .as_ptr()
                    .add(idx_in_chunk * self.element_layout.size());
                NonNull::new_unchecked(element_ptr)
            }
        })
    }

    /// Allocate a new chunk.
    fn allocate_chunk(&self) -> NonNull<u8> {
        let chunk_size = self.element_layout.size() * self.elements_per_chunk;
        let chunk_layout =
            Layout::from_size_align(chunk_size, self.element_layout.align()).unwrap();

        // Safety: chunk_layout has non-zero size (element_layout.size() > 0)
        let ptr = unsafe { alloc::alloc::alloc(chunk_layout) };
        NonNull::new(ptr).expect("allocation failed")
    }

    /// Deallocate a chunk.
    fn deallocate_chunk(&self, chunk: NonNull<u8>) {
        let chunk_size = self.element_layout.size() * self.elements_per_chunk;
        let chunk_layout =
            Layout::from_size_align(chunk_size, self.element_layout.align()).unwrap();
        unsafe {
            alloc::alloc::dealloc(chunk.as_ptr(), chunk_layout);
        }
    }

    /// Drop all initialized elements and deallocate all chunks.
    ///
    /// # Safety
    ///
    /// The caller must provide a valid drop function for the element type.
    /// The drop function will be called for each initialized element.
    pub unsafe fn drop_all(&mut self, drop_fn: Option<unsafe fn(*mut u8)>) {
        // Drop initialized elements
        if let Some(drop_fn) = drop_fn {
            for i in 0..self.initialized_count {
                let chunk_idx = i / self.elements_per_chunk;
                let idx_in_chunk = i % self.elements_per_chunk;
                let chunk_ptr = self.chunks[chunk_idx];
                unsafe {
                    let element_ptr = chunk_ptr
                        .as_ptr()
                        .add(idx_in_chunk * self.element_layout.size());
                    drop_fn(element_ptr);
                }
            }
        }

        // Deallocate all chunks
        let chunks: Vec<_> = self.chunks.drain(..).collect();
        for chunk in chunks {
            self.deallocate_chunk(chunk);
        }

        self.len = 0;
        self.initialized_count = 0;
    }

    /// Move all initialized elements out, calling the provided function for each.
    /// After this call, the rope is empty and all chunks are deallocated.
    ///
    /// # Safety
    ///
    /// The caller must ensure that:
    /// - All `initialized_count` elements are actually initialized
    /// - The move function properly takes ownership of each element
    pub unsafe fn drain_into<F>(&mut self, mut move_fn: F)
    where
        F: FnMut(NonNull<u8>),
    {
        for i in 0..self.initialized_count {
            let chunk_idx = i / self.elements_per_chunk;
            let idx_in_chunk = i % self.elements_per_chunk;
            let chunk_ptr = self.chunks[chunk_idx];
            unsafe {
                let element_ptr = chunk_ptr
                    .as_ptr()
                    .add(idx_in_chunk * self.element_layout.size());
                move_fn(NonNull::new_unchecked(element_ptr));
            }
        }

        // Deallocate all chunks (elements have been moved out)
        let chunks: Vec<_> = self.chunks.drain(..).collect();
        for chunk in chunks {
            self.deallocate_chunk(chunk);
        }

        self.len = 0;
        self.initialized_count = 0;
    }
}

impl Drop for ListRope {
    fn drop(&mut self) {
        // If there are still chunks, we need to deallocate them.
        // Note: we can't drop initialized elements here because we don't have the drop_fn.
        // The caller is responsible for calling drop_all() before dropping the rope
        // if there are initialized elements that need dropping.
        //
        // This is safe because:
        // 1. If elements were moved out via drain_into(), chunks are already empty
        // 2. If drop_all() was called, chunks are already deallocated
        // 3. If we get here with chunks, it's a leak (but not UB)
        if !self.chunks.is_empty() {
            // Log a warning in debug builds
            #[cfg(debug_assertions)]
            if self.initialized_count > 0 {
                eprintln!(
                    "ListRope dropped with {} initialized elements - potential memory leak",
                    self.initialized_count
                );
            }

            // Deallocate chunks without dropping elements
            let chunks: Vec<_> = self.chunks.drain(..).collect();
            for chunk in chunks {
                self.deallocate_chunk(chunk);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rope_basic() {
        let layout = Layout::new::<u64>();
        let mut rope = ListRope::new(layout);

        assert_eq!(rope.len(), 0);
        assert!(rope.is_empty());

        // Push some elements
        let ptr1 = rope.push_uninit();
        unsafe {
            ptr1.cast::<u64>().as_ptr().write(42);
        }
        rope.mark_last_initialized();

        let ptr2 = rope.push_uninit();
        unsafe {
            ptr2.cast::<u64>().as_ptr().write(99);
        }
        rope.mark_last_initialized();

        assert_eq!(rope.len(), 2);
        assert_eq!(rope.initialized_count(), 2);

        // Verify pointers are stable (ptr1 should still be valid)
        unsafe {
            assert_eq!(ptr1.cast::<u64>().as_ptr().read(), 42);
            assert_eq!(ptr2.cast::<u64>().as_ptr().read(), 99);
        }

        // Drain and verify
        let mut values = Vec::new();
        unsafe {
            rope.drain_into(|ptr| {
                values.push(ptr.cast::<u64>().as_ptr().read());
            });
        }

        assert_eq!(values, vec![42, 99]);
        assert_eq!(rope.len(), 0);
    }

    #[test]
    fn test_rope_multiple_chunks() {
        let layout = Layout::new::<u32>();
        // Use small chunk size to test chunking behavior
        let mut rope = ListRope::with_chunk_capacity(layout, 4);

        // Push more elements than fit in one chunk
        let mut ptrs = Vec::new();
        for i in 0..10u32 {
            let ptr = rope.push_uninit();
            unsafe {
                ptr.cast::<u32>().as_ptr().write(i);
            }
            rope.mark_last_initialized();
            ptrs.push(ptr);
        }

        assert_eq!(rope.len(), 10);
        // Should have 3 chunks (4 + 4 + 2)
        assert_eq!(rope.chunks.len(), 3);

        // Verify all pointers are still valid (stability test)
        for (i, ptr) in ptrs.iter().enumerate() {
            unsafe {
                assert_eq!(ptr.cast::<u32>().as_ptr().read(), i as u32);
            }
        }

        // Drain and verify order
        let mut values = Vec::new();
        unsafe {
            rope.drain_into(|ptr| {
                values.push(ptr.cast::<u32>().as_ptr().read());
            });
        }

        assert_eq!(values, (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn test_rope_pointer_stability_across_chunks() {
        let layout = Layout::new::<u64>();
        let mut rope = ListRope::with_chunk_capacity(layout, 2);

        // Get pointer to first element
        let ptr1 = rope.push_uninit();
        unsafe {
            ptr1.cast::<u64>().as_ptr().write(111);
        }
        rope.mark_last_initialized();

        // Fill first chunk
        let ptr2 = rope.push_uninit();
        unsafe {
            ptr2.cast::<u64>().as_ptr().write(222);
        }
        rope.mark_last_initialized();

        // This should allocate a new chunk
        let ptr3 = rope.push_uninit();
        unsafe {
            ptr3.cast::<u64>().as_ptr().write(333);
        }
        rope.mark_last_initialized();

        // ptr1 and ptr2 should still be valid after new chunk allocation
        unsafe {
            assert_eq!(ptr1.cast::<u64>().as_ptr().read(), 111);
            assert_eq!(ptr2.cast::<u64>().as_ptr().read(), 222);
            assert_eq!(ptr3.cast::<u64>().as_ptr().read(), 333);
        }

        // Clean up
        unsafe {
            rope.drop_all(None); // u64 doesn't need drop
        }
    }

    #[test]
    fn test_rope_with_drop() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

        #[repr(C)]
        struct DropCounter(u64);

        impl Drop for DropCounter {
            fn drop(&mut self) {
                DROP_COUNT.fetch_add(1, Ordering::SeqCst);
            }
        }

        DROP_COUNT.store(0, Ordering::SeqCst);

        let layout = Layout::new::<DropCounter>();
        let mut rope = ListRope::with_chunk_capacity(layout, 4);

        // Push 5 elements
        for i in 0..5u64 {
            let ptr = rope.push_uninit();
            unsafe {
                ptr.cast::<DropCounter>().as_ptr().write(DropCounter(i));
            }
            rope.mark_last_initialized();
        }

        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 0);

        // Drop all
        unsafe {
            rope.drop_all(Some(|ptr| {
                core::ptr::drop_in_place(ptr as *mut DropCounter);
            }));
        }

        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 5);
    }

    #[test]
    fn test_rope_iter_initialized() {
        let layout = Layout::new::<i32>();
        let mut rope = ListRope::with_chunk_capacity(layout, 3);

        for i in 0..7i32 {
            let ptr = rope.push_uninit();
            unsafe {
                ptr.cast::<i32>().as_ptr().write(i * 10);
            }
            rope.mark_last_initialized();
        }

        let values: Vec<i32> = unsafe {
            rope.iter_initialized()
                .map(|ptr| ptr.cast::<i32>().as_ptr().read())
                .collect()
        };

        assert_eq!(values, vec![0, 10, 20, 30, 40, 50, 60]);

        // Clean up
        unsafe {
            rope.drop_all(None);
        }
    }

    #[test]
    fn test_rope_partial_initialization() {
        let layout = Layout::new::<u64>();
        let mut rope = ListRope::new(layout);

        // Push 3 elements but only initialize 2
        let ptr1 = rope.push_uninit();
        unsafe {
            ptr1.cast::<u64>().as_ptr().write(1);
        }
        rope.mark_last_initialized();

        let ptr2 = rope.push_uninit();
        unsafe {
            ptr2.cast::<u64>().as_ptr().write(2);
        }
        rope.mark_last_initialized();

        let _ptr3 = rope.push_uninit();
        // Don't initialize ptr3

        assert_eq!(rope.len(), 3);
        assert_eq!(rope.initialized_count(), 2);

        // iter_initialized should only yield 2 elements
        let values: Vec<u64> = unsafe {
            rope.iter_initialized()
                .map(|ptr| ptr.cast::<u64>().as_ptr().read())
                .collect()
        };
        assert_eq!(values, vec![1, 2]);

        // Clean up (only drops initialized elements)
        unsafe {
            rope.drop_all(None);
        }
    }
}
