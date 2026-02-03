//! Opaque pointers
//!
//! Type-erased pointer helpers for working with reflected values
//!
//! # Pointer Types
//!
//! - `PtrMut` - Raw mutable pointer, can do everything
//! - `PtrConst` - Wraps PtrMut, exposes only read methods
//! - `PtrUninit` - Wraps PtrMut, for uninitialized memory
//!
//! None of these types have lifetime parameters - safety is the caller's responsibility.
//!
//! # Allocation Helpers
//!
//! - [`alloc_for_layout`] - Allocates memory for a layout, handling ZSTs correctly
//! - [`dealloc_for_layout`] - Deallocates memory for a layout, handling ZSTs correctly
//!
//! These functions avoid undefined behavior when allocating zero-sized types.

mod ptr_layout;
mod tagged;

pub use tagged::{NativeWidePtr, PtrKind, TaggedPtr, ptr_kind};

use crate::Shape;
use core::{fmt, ptr::copy_nonoverlapping};

/// Tried to get the `Layout` of an unsized type
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
pub struct UnsizedError;

impl core::fmt::Display for UnsizedError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Not a Sized type")
    }
}

impl core::error::Error for UnsizedError {}

// ============================================================================
// PtrMut - The base mutable pointer type
// ============================================================================

/// A type-erased mutable pointer (wide pointer) holding a data pointer and metadata.
///
/// This is the base pointer type that can do everything. Uses TaggedPtr for the
/// data pointer, which uses the low bit to distinguish wide vs thin pointers.
///
/// No lifetime parameter - safety is the caller's responsibility.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct PtrMut {
    /// The tagged data pointer (low bit indicates wide vs thin)
    ptr: TaggedPtr,
    /// Metadata for wide pointers (length for slices, vtable for trait objects)
    metadata: *const (),
}

impl fmt::Debug for PtrMut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.ptr.fmt(f)
    }
}

impl PtrMut {
    /// Creates a new mutable pointer from a raw pointer to a (potentially unsized) object.
    #[inline]
    pub fn new<T: ?Sized>(ptr: *mut T) -> Self {
        match ptr_kind::<T>() {
            PtrKind::Thin => Self {
                ptr: TaggedPtr::thin(ptr as *mut u8),
                metadata: core::ptr::null(),
            },
            PtrKind::Wide => {
                let native = NativeWidePtr::from_ptr(ptr);
                Self {
                    ptr: TaggedPtr::wide(native.data_ptr()),
                    metadata: native.metadata(),
                }
            }
            PtrKind::Unknown => {
                panic!("pointer isn't thin, or wide, but a secret third thing")
            }
        }
    }

    /// Creates a new mutable pointer from a raw pointer to a sized type.
    /// This is const because sized types use thin pointers.
    #[inline]
    pub const fn new_sized<T: Sized>(ptr: *mut T) -> Self {
        Self {
            ptr: TaggedPtr::thin(ptr as *mut u8),
            metadata: core::ptr::null(),
        }
    }

    /// Returns true if this is a wide pointer
    #[inline]
    pub fn is_wide(self) -> bool {
        self.ptr.is_wide()
    }

    /// Returns the actual data pointer as *mut u8
    #[inline]
    pub fn data_ptr(self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    /// Reconstructs a typed pointer from this representation.
    ///
    /// # Safety
    /// T must match the original type this pointer was created from.
    #[inline]
    pub(crate) unsafe fn to_ptr<T: ?Sized>(self) -> *mut T {
        match ptr_kind::<T>() {
            PtrKind::Thin => {
                #[allow(clippy::transmute_undefined_repr)]
                unsafe {
                    core::mem::transmute_copy(&self.data_ptr())
                }
            }
            PtrKind::Wide => {
                let native = NativeWidePtr::from_parts(self.data_ptr(), self.metadata);
                unsafe { native.to_ptr() }
            }
            PtrKind::Unknown => {
                panic!("pointer isn't thin, or wide, but a secret third thing")
            }
        }
    }

    /// Reconstructs a typed thin pointer.
    ///
    /// # Safety
    /// T must match the original type and must be Sized.
    #[inline]
    unsafe fn to_thin_ptr<T>(self) -> *mut T {
        self.data_ptr() as *mut T
    }

    /// Creates a new pointer with an offset added to the data pointer.
    /// Preserves the wide/thin tag and metadata.
    ///
    /// # Safety
    /// Offset must be within bounds of the allocated memory.
    #[inline]
    pub unsafe fn with_offset(self, offset: usize) -> Self {
        Self {
            ptr: unsafe { self.ptr.with_offset(offset) },
            metadata: self.metadata,
        }
    }

    /// Convert to a `PtrConst`.
    #[inline]
    pub const fn as_const(self) -> PtrConst {
        PtrConst { ptr: self }
    }

    /// Convert to a `PtrUninit`.
    #[inline]
    pub const fn as_uninit(self) -> PtrUninit {
        PtrUninit { ptr: self }
    }

    /// Returns the underlying raw pointer as a const byte pointer.
    ///
    /// # Panics
    /// Panics if this is a wide pointer.
    #[inline]
    pub fn as_byte_ptr(self) -> *const u8 {
        assert!(!self.is_wide(), "as_byte_ptr called on wide pointer");
        self.data_ptr() as *const u8
    }

    /// Returns the underlying raw pointer as a mutable byte pointer.
    ///
    /// # Panics
    /// Panics if this is a wide pointer.
    #[inline]
    pub fn as_mut_byte_ptr(self) -> *mut u8 {
        assert!(!self.is_wide(), "as_mut_byte_ptr called on wide pointer");
        self.data_ptr()
    }

    /// Borrows the underlying object as a reference of type `T`.
    ///
    /// # Safety
    /// - `T` must be the actual underlying type
    /// - The memory must be valid and initialized
    /// - No mutable references may exist
    /// - The returned reference must not outlive the actual data
    #[inline]
    pub unsafe fn get<'a, T: ?Sized>(self) -> &'a T {
        unsafe { &*self.to_ptr::<T>() }
    }

    /// Borrows the underlying object as a mutable reference of type `T`.
    ///
    /// # Safety
    /// - `T` must be the actual underlying type
    /// - The memory must be valid and initialized
    /// - Caller must have exclusive access
    /// - The returned reference must not outlive the actual data
    #[inline]
    pub unsafe fn as_mut<'a, T: ?Sized>(self) -> &'a mut T {
        unsafe { &mut *self.to_ptr::<T>() }
    }

    /// Gets the underlying raw pointer as a const pointer of type T.
    ///
    /// # Safety
    /// Must be called with the original type T.
    #[inline]
    pub unsafe fn as_ptr<T: ?Sized>(self) -> *const T {
        unsafe { self.to_ptr() }
    }

    /// Gets the underlying raw pointer as a mutable pointer of type T.
    ///
    /// # Safety
    /// Must be called with the original type T.
    #[inline]
    pub unsafe fn as_mut_ptr<T: ?Sized>(self) -> *mut T {
        unsafe { self.to_ptr() }
    }

    /// Reads the value from the pointer.
    ///
    /// # Safety
    /// - `T` must be the actual underlying type
    /// - The memory must be properly initialized
    #[inline]
    pub unsafe fn read<T>(self) -> T {
        unsafe { core::ptr::read(self.to_thin_ptr()) }
    }

    /// Drops the value in place and returns a PtrUninit.
    ///
    /// # Safety
    /// - `T` must be the actual underlying type
    /// - The memory must be properly initialized
    #[inline]
    pub unsafe fn drop_in_place<T: ?Sized>(self) -> PtrUninit {
        unsafe { core::ptr::drop_in_place(self.to_ptr::<T>()) }
        self.as_uninit()
    }

    /// Writes a value after dropping the existing one.
    ///
    /// # Safety
    /// - The pointer must be properly aligned
    /// - T must be the actual type
    /// - The memory must already be initialized
    #[inline]
    pub unsafe fn replace<T>(self, value: T) -> Self {
        unsafe { self.drop_in_place::<T>().put(value) }
    }

    /// Returns a pointer with the given offset added.
    ///
    /// # Safety
    /// Offset must be within bounds of the allocated memory.
    #[inline]
    pub unsafe fn field(self, offset: usize) -> PtrMut {
        unsafe { self.with_offset(offset) }
    }
}

// ============================================================================
// PtrConst - Read-only pointer (wraps PtrMut)
// ============================================================================

/// A type-erased read-only pointer.
///
/// Wraps `PtrMut` but only exposes read methods. No lifetime parameter.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct PtrConst {
    pub(crate) ptr: PtrMut,
}

impl fmt::Debug for PtrConst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.ptr.fmt(f)
    }
}

impl PtrConst {
    /// Creates a new const pointer from a raw pointer.
    #[inline]
    pub fn new<T: ?Sized>(ptr: *const T) -> Self {
        Self {
            ptr: PtrMut::new(ptr as *mut T),
        }
    }

    /// Creates a new const pointer from a raw pointer to a sized type.
    /// This is const because sized types use thin pointers.
    #[inline]
    pub const fn new_sized<T: Sized>(ptr: *const T) -> Self {
        Self {
            ptr: PtrMut::new_sized(ptr as *mut T),
        }
    }

    /// Create a wide pointer from raw data pointer and metadata.
    #[inline]
    pub fn new_wide(data: *const u8, metadata: *const ()) -> Self {
        Self {
            ptr: PtrMut {
                ptr: TaggedPtr::wide(data as *mut u8),
                metadata,
            },
        }
    }

    /// Returns true if this is a wide pointer.
    #[inline]
    pub fn is_wide(self) -> bool {
        self.ptr.is_wide()
    }

    /// Returns the raw data pointer.
    #[inline]
    pub fn raw_ptr(self) -> *const u8 {
        self.ptr.data_ptr() as *const u8
    }

    /// Returns the underlying data pointer as a byte pointer.
    ///
    /// # Panics
    /// Panics if this is a wide pointer.
    #[inline]
    pub fn as_byte_ptr(self) -> *const u8 {
        self.ptr.as_byte_ptr()
    }

    /// Borrows the underlying object as a reference of type `T`.
    ///
    /// # Safety
    /// - `T` must be the actual underlying type
    /// - The memory must be valid and initialized
    /// - The returned reference must not outlive the actual data
    #[inline]
    pub unsafe fn get<'a, T: ?Sized>(self) -> &'a T {
        unsafe { self.ptr.get::<T>() }
    }

    /// Gets the underlying raw pointer as a const pointer of type T.
    ///
    /// # Safety
    /// Must be called with the original type T.
    #[inline]
    pub unsafe fn as_ptr<T: ?Sized>(self) -> *const T {
        unsafe { self.ptr.as_ptr() }
    }

    /// Reads the value from the pointer.
    ///
    /// # Safety
    /// - `T` must be the actual underlying type
    /// - The memory must be properly initialized
    #[inline]
    pub unsafe fn read<T>(self) -> T {
        unsafe { self.ptr.read() }
    }

    /// Returns a pointer with the given offset added.
    ///
    /// # Safety
    /// Offset must be within bounds of the allocated memory.
    #[inline]
    pub unsafe fn field(self, offset: usize) -> PtrConst {
        PtrConst {
            ptr: unsafe { self.ptr.with_offset(offset) },
        }
    }

    /// Convert to a mutable pointer.
    ///
    /// # Safety
    /// Caller must ensure they have exclusive access.
    #[inline]
    pub const unsafe fn into_mut(self) -> PtrMut {
        self.ptr
    }
}

// ============================================================================
// PtrUninit - Pointer to uninitialized memory (wraps PtrMut)
// ============================================================================

/// A type-erased pointer to uninitialized memory.
///
/// Wraps `PtrMut` but represents uninitialized memory. No lifetime parameter.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct PtrUninit {
    pub(crate) ptr: PtrMut,
}

impl fmt::Debug for PtrUninit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.ptr.fmt(f)
    }
}

impl PtrUninit {
    /// Create a new uninit pointer from a mutable pointer.
    #[inline]
    pub fn new<T: ?Sized>(ptr: *mut T) -> Self {
        Self {
            ptr: PtrMut::new(ptr),
        }
    }

    /// Create a new uninit pointer from a pointer to a sized type.
    /// This is const because sized types use thin pointers.
    #[inline]
    pub const fn new_sized<T: Sized>(ptr: *mut T) -> Self {
        Self {
            ptr: PtrMut::new_sized(ptr),
        }
    }

    /// Creates from a reference to a `MaybeUninit`.
    #[inline]
    pub fn from_maybe_uninit<T>(borrow: &mut core::mem::MaybeUninit<T>) -> Self {
        Self {
            ptr: PtrMut::new(borrow as *mut core::mem::MaybeUninit<T> as *mut T),
        }
    }

    /// Returns the underlying raw pointer as a mutable byte pointer.
    ///
    /// # Panics
    /// Panics if this is a wide pointer.
    #[inline]
    pub fn as_mut_byte_ptr(self) -> *mut u8 {
        self.ptr.as_mut_byte_ptr()
    }

    /// Returns the underlying raw pointer as a const byte pointer.
    ///
    /// # Panics
    /// Panics if this is a wide pointer.
    #[inline]
    pub fn as_byte_ptr(self) -> *const u8 {
        self.ptr.as_byte_ptr()
    }

    /// Assumes the pointer is initialized and returns a `PtrMut`.
    ///
    /// # Safety
    /// The memory must actually be initialized.
    #[inline]
    pub const unsafe fn assume_init(self) -> PtrMut {
        self.ptr
    }

    /// Write a value to this location and return an initialized pointer.
    ///
    /// # Safety
    /// The pointer must be properly aligned and point to allocated memory.
    #[inline]
    pub unsafe fn put<T>(self, value: T) -> PtrMut {
        unsafe {
            core::ptr::write(self.ptr.to_thin_ptr::<T>(), value);
            self.assume_init()
        }
    }

    /// Copies memory from a source pointer into this location.
    ///
    /// # Safety
    /// - The source pointer must be valid for reads
    /// - This pointer must be valid for writes and properly aligned
    /// - The regions may not overlap
    #[inline]
    pub unsafe fn copy_from(
        self,
        src: PtrConst,
        shape: &'static Shape,
    ) -> Result<PtrMut, UnsizedError> {
        let Ok(layout) = shape.layout.sized_layout() else {
            return Err(UnsizedError);
        };
        unsafe {
            copy_nonoverlapping(src.as_byte_ptr(), self.as_mut_byte_ptr(), layout.size());
            Ok(self.assume_init())
        }
    }

    /// Returns a pointer with the given offset added (still uninit).
    ///
    /// # Safety
    /// Offset must be within bounds of the allocated memory.
    #[inline]
    pub unsafe fn field_uninit(self, offset: usize) -> PtrUninit {
        PtrUninit {
            ptr: unsafe { self.ptr.with_offset(offset) },
        }
    }

    /// Returns a pointer with the given offset added, assuming it's initialized.
    ///
    /// # Safety
    /// - Offset must be within bounds
    /// - The memory at that offset must be initialized
    #[inline]
    pub unsafe fn field_init(self, offset: usize) -> PtrMut {
        unsafe { self.field_uninit(offset).assume_init() }
    }
}

// ============================================================================
// Conversions
// ============================================================================

impl From<PtrMut> for PtrConst {
    #[inline]
    fn from(p: PtrMut) -> Self {
        p.as_const()
    }
}

impl From<PtrMut> for PtrUninit {
    #[inline]
    fn from(p: PtrMut) -> Self {
        p.as_uninit()
    }
}

// ============================================================================
// Allocation Helpers
// ============================================================================

/// Allocates memory for a layout, correctly handling zero-sized types.
///
/// For ZSTs (zero-sized types), returns a dangling but properly aligned pointer
/// without actually allocating. This avoids undefined behavior since
/// `alloc::alloc::alloc` with a zero-sized layout is UB.
///
/// # Returns
///
/// A `PtrUninit` pointing to:
/// - Newly allocated memory for non-zero-sized layouts
/// - A dangling, aligned pointer for zero-sized layouts
///
/// # Panics
///
/// Panics if allocation fails (calls `handle_alloc_error`).
///
/// # Example
///
/// ```ignore
/// use core::alloc::Layout;
/// use facet_core::{alloc_for_layout, dealloc_for_layout};
///
/// let layout = Layout::new::<u32>();
/// let ptr = alloc_for_layout(layout);
/// // ... use ptr ...
/// unsafe { dealloc_for_layout(ptr.assume_init(), layout); }
/// ```
#[cfg(feature = "alloc")]
pub fn alloc_for_layout(layout: core::alloc::Layout) -> PtrUninit {
    if layout.size() == 0 {
        // ZST: return aligned dangling pointer, never actually allocate
        // This is the same pattern used in std for Box<ZST>, Vec<ZST>, etc.
        PtrUninit::new(core::ptr::null_mut::<u8>().wrapping_byte_add(layout.align()))
    } else {
        // SAFETY: layout.size() > 0, so this is valid
        let ptr = unsafe { alloc::alloc::alloc(layout) };
        if ptr.is_null() {
            alloc::alloc::handle_alloc_error(layout);
        }
        PtrUninit::new(ptr)
    }
}

/// Deallocates memory for a layout, correctly handling zero-sized types.
///
/// For ZSTs (zero-sized types), this is a no-op since no memory was actually
/// allocated. This avoids undefined behavior since `alloc::alloc::dealloc`
/// with a zero-sized layout is UB.
///
/// # Safety
///
/// - For non-ZST layouts, `ptr` must have been allocated by [`alloc_for_layout`]
///   (or `alloc::alloc::alloc`) with the same layout.
/// - `ptr` must not have been deallocated already.
/// - For ZST layouts, `ptr` is ignored (should be the dangling pointer from
///   `alloc_for_layout`, but this isn't checked).
#[cfg(feature = "alloc")]
pub unsafe fn dealloc_for_layout(ptr: PtrMut, layout: core::alloc::Layout) {
    if layout.size() == 0 {
        // ZST: nothing to deallocate
        return;
    }
    // SAFETY: caller guarantees ptr was allocated with this layout
    unsafe { alloc::alloc::dealloc(ptr.as_mut_byte_ptr(), layout) }
}
