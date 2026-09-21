//! Pointer that remembers whether it is wide (has metadata) or thin.
//!
//! This flag is stored *beside* the pointer, not inside it. It used to live in
//! bit 63 of the address, on the assumption that user-space addresses never set
//! the high bit. That assumption does not hold on Top-Byte-Ignore platforms:
//! Android on arm64 tags every `malloc` result in bits 56-63 for apps targeting
//! API 30+, so ordinary thin heap pointers came back with bit 63 set and were
//! misread as wide. Masking the bit back off was equally wrong there - it
//! changes the tag, and `free` wants the pointer it handed out.
//! See <https://github.com/facet-rs/facet/issues/2659>.
//!
//! 32-bit is not safe either: a 3G/1G split Linux or a large-address-aware
//! Windows process hands out addresses with bit 31 set.
//!
//! Keeping the flag out of band costs one extra word in `PtrMut`, and buys back
//! const construction for wide pointers too - nothing has to touch the address.

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

/// A data pointer plus a flag saying whether it came from an unsized type.
///
/// The address is stored verbatim - every bit of it, tag byte included - so it
/// is safe to hand straight back to `free`, and it round-trips unchanged on
/// platforms that put meaning in the high bits.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct TaggedPtr {
    ptr: *mut u8,
    wide: bool,
}

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
        Self { ptr, wide: false }
    }

    /// Create a tagged pointer for a wide (unsized) type.
    #[inline]
    pub const fn wide(ptr: *mut u8) -> Self {
        Self { ptr, wide: true }
    }

    /// Returns true if this is a wide pointer (has metadata).
    #[inline]
    pub const fn is_wide(self) -> bool {
        self.wide
    }

    /// Returns true if this is a thin pointer (no metadata).
    #[inline]
    pub const fn is_thin(self) -> bool {
        !self.wide
    }

    /// Returns the data pointer, exactly as it was given to us.
    #[inline]
    pub const fn as_ptr(self) -> *mut u8 {
        self.ptr
    }

    /// Returns the raw pointer value (for debugging/testing).
    #[inline]
    pub const fn raw(self) -> *mut u8 {
        self.ptr
    }

    /// Create a new TaggedPtr with an offset added, preserving the tag.
    ///
    /// # Safety
    /// The offset must be within bounds of the allocation.
    #[inline]
    pub unsafe fn with_offset(self, offset: usize) -> Self {
        Self {
            ptr: unsafe { self.ptr.byte_add(offset) },
            wide: self.wide,
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

    /// The tag Android's allocator actually puts in the top byte of every
    /// `malloc` result on arm64. 0xb4 has bit 7 set, so bit 63 of the address is
    /// set on a perfectly ordinary thin pointer.
    fn with_top_byte_tag(addr: usize) -> usize {
        addr | (0xb4_usize << (usize::BITS - 8))
    }

    /// Reading the wide flag out of the address made tagged heap pointers look
    /// wide, which blew up `as_mut_byte_ptr` on valid input.
    /// <https://github.com/facet-rs/facet/issues/2659>
    #[test]
    fn top_byte_tagged_pointer_is_still_thin() {
        let data: u8 = 42;
        let ptr = (&data as *const u8 as *mut u8).map_addr(with_top_byte_tag);

        let tagged = TaggedPtr::thin(ptr);
        assert!(tagged.is_thin());
        assert!(!tagged.is_wide());
    }

    /// And the flag must not eat bits of the address on the way back out: a
    /// masked-off tag is a different pointer than the one `malloc` handed us,
    /// which `free` is entitled to reject.
    #[test]
    fn top_byte_tag_survives_round_trip() {
        let data: u8 = 42;
        let ptr = (&data as *const u8 as *mut u8).map_addr(with_top_byte_tag);

        assert_eq!(TaggedPtr::thin(ptr).as_ptr().addr(), ptr.addr());
        assert_eq!(TaggedPtr::wide(ptr).as_ptr().addr(), ptr.addr());
        assert!(TaggedPtr::wide(ptr).is_wide());
    }

    /// Same address, different kind, so they must not compare equal - the flag
    /// no longer being part of the address must not collapse them.
    #[test]
    fn thin_and_wide_differ() {
        let data: u8 = 42;
        let ptr = &data as *const u8 as *mut u8;

        assert_ne!(TaggedPtr::thin(ptr), TaggedPtr::wide(ptr));
    }

    /// The panic from the issue, at the level it was reported: a thin pointer
    /// with a tagged address going through `PtrUninit`.
    #[test]
    fn tagged_address_reaches_as_mut_byte_ptr() {
        let data: u8 = 42;
        let ptr = (&data as *const u8 as *mut u8).map_addr(with_top_byte_tag);

        let uninit = crate::PtrUninit::new_sized(ptr);
        assert_eq!(uninit.as_mut_byte_ptr().addr(), ptr.addr());
    }

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
        assert_eq!(tagged.as_ptr(), ptr); // the address is untouched
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
