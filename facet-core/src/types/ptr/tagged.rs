//! Tagged pointer that uses the high bit (bit 63) to distinguish wide vs thin pointers.
//!
//! On 64-bit systems, user-space addresses only use 48 bits (or 57 with 5-level paging),
//! leaving bit 63 always 0 in user space on Linux, macOS, and Windows.
//! This allows us to use it as a tag without affecting the actual pointer value.
//!
//! The key insight that allows const construction comes from BurntSushi's jiff crate:
//! <https://github.com/BurntSushi/jiff/blob/9d7e099a7a9a653b114de2465c0bc7361700c48b/src/tz/timezone.rs#L2086-L2111>
//!
//! In const contexts, you can't cast pointers to integers (required for bit manipulation).
//! The trick is to make the const-constructible variant have tag 0 (no modification needed).
//! For us: thin pointers (sized types) = tag 0, wide pointers (unsized) = tag 1.
//! Since `Attr::new` only works with sized types, it remains const.

use super::ptr_layout::PTR_FIRST;

/// The kind of a pointer based on its size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtrKind {
    /// A thin pointer (sized types) - one word
    Thin,
    /// A wide pointer (slices, trait objects) - two words
    Wide,
    /// Unknown pointer size (neither one nor two words)
    Unknown,
}

/// Determines the pointer kind for a given type.
///
/// This is a compile-time determination based on the size of `*mut T`.
#[inline]
pub const fn ptr_kind<T: ?Sized>() -> PtrKind {
    if size_of::<*mut T>() == size_of::<*mut u8>() {
        PtrKind::Thin
    } else if size_of::<*mut T>() == 2 * size_of::<*mut u8>() {
        PtrKind::Wide
    } else {
        PtrKind::Unknown
    }
}

/// A pointer that uses the high bit (bit 63) as a tag to indicate wide vs thin.
///
/// - Bit 63 = 0: thin pointer (sized type, no metadata needed)
/// - Bit 63 = 1: wide pointer (unsized type, has metadata)
///
/// This works because user-space addresses on 64-bit Linux/macOS/Windows
/// never use bit 63 (they're limited to 48-57 bits).
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct TaggedPtr(*mut u8);

// On 64-bit systems, use bit 63. On 32-bit, use bit 31.
// User-space addresses never use the high bit on common platforms.
const WIDE_TAG: usize = 1_usize << (usize::BITS - 1);

/// A wide pointer in native platform layout.
///
/// On most platforms this is `[data_ptr, metadata]`, but the order can vary.
/// This type handles the platform-specific layout and can convert to/from
/// our canonical `Ptr` representation.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct NativeWidePtr {
    parts: [*mut u8; 2],
}

impl NativeWidePtr {
    /// Create from a typed wide pointer (e.g., `*mut [T]`, `*mut str`, `*mut dyn Trait`)
    #[inline]
    pub fn from_ptr<T: ?Sized>(ptr: *mut T) -> Self {
        debug_assert!(
            size_of::<*mut T>() == 2 * size_of::<*mut u8>(),
            "from_ptr called with non-wide pointer type"
        );
        // SAFETY: We've verified this is a wide pointer type
        #[allow(clippy::transmute_undefined_repr)]
        let parts: [*mut u8; 2] = unsafe { core::mem::transmute_copy(&ptr) };
        Self { parts }
    }

    /// Get the data pointer
    #[inline]
    pub const fn data_ptr(self) -> *mut u8 {
        if PTR_FIRST {
            self.parts[0]
        } else {
            self.parts[1]
        }
    }

    /// Get the metadata pointer
    #[inline]
    pub const fn metadata(self) -> *const () {
        if PTR_FIRST {
            self.parts[1] as *const ()
        } else {
            self.parts[0] as *const ()
        }
    }

    /// Create from data pointer and metadata
    #[inline]
    pub const fn from_parts(data: *mut u8, metadata: *const ()) -> Self {
        let parts = if PTR_FIRST {
            [data, metadata as *mut u8]
        } else {
            [metadata as *mut u8, data]
        };
        Self { parts }
    }

    /// Convert to a typed wide pointer
    ///
    /// # Safety
    /// The caller must ensure T matches the actual type this pointer was created from.
    #[inline]
    pub unsafe fn to_ptr<T: ?Sized>(self) -> *mut T {
        debug_assert!(
            size_of::<*mut T>() == 2 * size_of::<*mut u8>(),
            "to_ptr called with non-wide pointer type"
        );
        #[allow(clippy::transmute_undefined_repr)]
        unsafe {
            core::mem::transmute_copy(&self.parts)
        }
    }
}

impl core::fmt::Debug for NativeWidePtr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NativeWidePtr")
            .field("data", &self.data_ptr())
            .field("metadata", &self.metadata())
            .finish()
    }
}

impl TaggedPtr {
    /// Create a tagged pointer for a thin (sized) type.
    #[inline]
    pub const fn thin(ptr: *mut u8) -> Self {
        // No tag bit set
        Self(ptr)
    }

    /// Create a tagged pointer for a wide (unsized) type.
    #[inline]
    pub fn wide(ptr: *mut u8) -> Self {
        // Set the tag bit using map_addr to preserve provenance
        Self(ptr.map_addr(|addr| addr | WIDE_TAG))
    }

    /// Returns true if this is a wide pointer (has metadata).
    #[inline]
    pub fn is_wide(self) -> bool {
        // Use addr() to get the address without casting
        (self.0.addr() & WIDE_TAG) != 0
    }

    /// Returns true if this is a thin pointer (no metadata).
    #[inline]
    pub fn is_thin(self) -> bool {
        !self.is_wide()
    }

    /// Returns the actual data pointer with the tag bit cleared.
    #[inline]
    pub fn as_ptr(self) -> *mut u8 {
        // Use map_addr to preserve provenance when clearing the tag bit
        self.0.map_addr(|addr| addr & !WIDE_TAG)
    }

    /// Returns the raw tagged value (for debugging/testing).
    #[inline]
    pub const fn raw(self) -> *mut u8 {
        self.0
    }

    /// Create a new TaggedPtr with an offset added, preserving the tag.
    ///
    /// # Safety
    /// The offset must be within bounds of the allocation.
    #[inline]
    pub unsafe fn with_offset(self, offset: usize) -> Self {
        let new_ptr = unsafe { self.as_ptr().byte_add(offset) };
        if self.is_wide() {
            Self::wide(new_ptr)
        } else {
            Self::thin(new_ptr)
        }
    }
}

impl core::fmt::Debug for TaggedPtr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TaggedPtr")
            .field("ptr", &self.as_ptr())
            .field("is_wide", &self.is_wide())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thin_pointer_not_tagged() {
        let data: u8 = 42;
        let ptr = &data as *const u8 as *mut u8;
        let tagged = TaggedPtr::thin(ptr);

        assert!(tagged.is_thin());
        assert!(!tagged.is_wide());
        assert_eq!(tagged.as_ptr(), ptr);
    }

    #[test]
    fn wide_pointer_is_tagged() {
        let data: u8 = 42;
        let ptr = &data as *const u8 as *mut u8;
        let tagged = TaggedPtr::wide(ptr);

        assert!(tagged.is_wide());
        assert!(!tagged.is_thin());
        assert_eq!(tagged.as_ptr(), ptr); // as_ptr clears the tag
    }

    #[test]
    fn offset_preserves_tag() {
        let data: [u8; 10] = [0; 10];
        let ptr = data.as_ptr() as *mut u8;

        let thin = TaggedPtr::thin(ptr);
        let thin_offset = unsafe { thin.with_offset(5) };
        assert!(thin_offset.is_thin());
        assert_eq!(thin_offset.as_ptr(), unsafe { ptr.byte_add(5) });

        let wide = TaggedPtr::wide(ptr);
        let wide_offset = unsafe { wide.with_offset(5) };
        assert!(wide_offset.is_wide());
        assert_eq!(wide_offset.as_ptr(), unsafe { ptr.byte_add(5) });
    }
}
