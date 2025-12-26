use core::mem::{align_of, size_of};
use core::ptr::NonNull;

/// A contiguous region of memory addressed by offset.
///
/// # Safety
///
/// The caller must ensure:
/// - `base` is valid for `len` bytes and properly aligned for all contained types
/// - the memory remains valid for the lifetime of this Region
#[derive(Clone, Copy)]
pub struct Region {
    base: NonNull<u8>,
    len: usize,
}

impl Region {
    /// Create a region from a raw pointer and length.
    ///
    /// # Safety
    ///
    /// - `base` must be valid for `len` bytes
    /// - `base` must be aligned for all contained types
    /// - the memory must remain valid for the lifetime of this Region
    pub unsafe fn from_raw(base: *mut u8, len: usize) -> Self {
        let base = NonNull::new(base).expect("region base must be non-null");
        Self { base, len }
    }

    /// Returns the base pointer of the region.
    #[inline]
    pub fn as_ptr(&self) -> *mut u8 {
        self.base.as_ptr()
    }

    /// Returns the size of the region in bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the region has zero length.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns a pointer to offset `off` within the region.
    #[inline]
    pub fn offset(&self, off: usize) -> *mut u8 {
        assert!(
            off < self.len,
            "offset {off} out of bounds (len={})",
            self.len
        );
        unsafe { self.as_ptr().add(off) }
    }

    /// Returns a reference to a `T` at the given byte offset.
    ///
    /// # Safety
    ///
    /// The offset must be aligned for `T` and within bounds.
    #[inline]
    pub unsafe fn get<T>(&self, off: usize) -> &T {
        debug_assert!(off + size_of::<T>() <= self.len);
        debug_assert!(off.is_multiple_of(align_of::<T>()));
        unsafe { &*(self.offset(off) as *const T) }
    }

    /// Returns a mutable reference to a `T` at the given byte offset.
    ///
    /// # Safety
    ///
    /// The offset must be aligned for `T` and within bounds.
    #[inline]
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn get_mut<T>(&self, off: usize) -> &mut T {
        debug_assert!(off + size_of::<T>() <= self.len);
        debug_assert!(off.is_multiple_of(align_of::<T>()));
        unsafe { &mut *(self.offset(off) as *mut T) }
    }
}

unsafe impl Send for Region {}
unsafe impl Sync for Region {}

#[cfg(any(test, feature = "alloc"))]
mod heap {
    use super::Region;
    use alloc::alloc::{Layout, alloc_zeroed, dealloc};
    use core::ptr::NonNull;

    /// Heap-backed region for tests or heap-based usage.
    pub struct HeapRegion {
        base: NonNull<u8>,
        len: usize,
        layout: Layout,
    }

    impl HeapRegion {
        /// Allocate a zeroed heap region aligned to 64 bytes.
        pub fn new_zeroed(size: usize) -> Self {
            let layout =
                Layout::from_size_align(size.max(1), 64).expect("invalid heap region layout");
            let ptr = unsafe { alloc_zeroed(layout) };
            let base = NonNull::new(ptr).expect("heap region allocation failed");
            Self {
                base,
                len: size,
                layout,
            }
        }

        /// Returns a Region view of this allocation.
        #[inline]
        pub fn region(&self) -> Region {
            unsafe { Region::from_raw(self.base.as_ptr(), self.len) }
        }

        /// Returns the allocation size.
        #[inline]
        pub fn len(&self) -> usize {
            self.len
        }

        /// Returns true if the allocation is zero-length.
        #[inline]
        pub fn is_empty(&self) -> bool {
            self.len == 0
        }
    }

    impl Drop for HeapRegion {
        fn drop(&mut self) {
            unsafe { dealloc(self.base.as_ptr(), self.layout) };
        }
    }

    unsafe impl Send for HeapRegion {}
    unsafe impl Sync for HeapRegion {}
}

#[cfg(any(test, feature = "alloc"))]
pub use heap::HeapRegion;
