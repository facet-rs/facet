//! Opaque pointers
//!
//! Type-erased pointer helpers for working with reflected values

use core::{fmt, marker::PhantomData, ptr::NonNull};

use crate::{Shape, UnsizedError};

impl<'mem, T: ?Sized> From<TypedPtrUninit<'mem, T>> for PtrUninit<'mem> {
    fn from(ptr: TypedPtrUninit<'mem, T>) -> Self {
        PtrUninit {
            ptr: ptr.0,
            phantom: PhantomData,
        }
    }
}

/// A pointer to an uninitialized value with a lifetime.
#[repr(transparent)]
pub struct TypedPtrUninit<'mem, T: ?Sized>(Ptr, PhantomData<&'mem mut T>);

impl<T: ?Sized> fmt::Debug for TypedPtrUninit<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.ptr.as_ptr().fmt(f)
    }
}

impl<'mem, T: ?Sized> TypedPtrUninit<'mem, T> {
    /// Create a new opaque pointer from a mutable pointer
    #[inline]
    pub const fn new(ptr: NonNull<T>) -> Self {
        Self(Ptr::from_ptr(ptr), PhantomData)
    }

    /// Write a value to this location and convert to an initialized pointer
    ///
    /// # Safety
    ///
    /// The pointer must be properly aligned for T and point to allocated memory
    /// that can be safely written to.
    #[inline]
    pub const unsafe fn put(self, value: T) -> &'mem mut T
    where
        T: Sized,
    {
        unsafe {
            core::ptr::write(self.0.to_ptr(), value);
            self.assume_init()
        }
    }
    /// Assumes the pointer is initialized and returns an `Opaque` pointer
    ///
    /// # Safety
    ///
    /// The pointer must actually be pointing to initialized memory of the correct type.
    #[inline]
    pub const unsafe fn assume_init(self) -> &'mem mut T {
        unsafe { &mut *self.0.to_ptr() }
    }

    /// Returns a pointer with the given offset added
    ///
    /// # Safety
    ///
    /// Offset is within the bounds of the allocated memory and `U` is the correct type for the field.
    #[inline]
    pub const unsafe fn field_uninit_at<U>(&mut self, offset: usize) -> TypedPtrUninit<'mem, U> {
        TypedPtrUninit(
            Ptr {
                ptr: unsafe { self.0.ptr.byte_add(offset) },
                metadata: self.0.metadata,
            },
            PhantomData,
        )
    }
}

/// A pointer to an uninitialized value with a lifetime.
#[repr(transparent)]
pub struct TypedPtrMut<'mem, T: ?Sized>(Ptr, PhantomData<&'mem mut T>);

impl<T: ?Sized> fmt::Debug for TypedPtrMut<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.ptr.as_ptr().fmt(f)
    }
}

impl<'mem, T: ?Sized> TypedPtrMut<'mem, T> {
    /// Creates a new typed pointer from a reference
    #[inline]
    pub const fn new(ptr: &'mem mut T) -> Self {
        Self(
            Ptr::from_ptr(unsafe { NonNull::new_unchecked(ptr) }),
            PhantomData,
        )
    }

    /// Unwraps the typed pointer
    #[inline]
    pub const fn as_ptr(self) -> *mut T {
        unsafe { self.0.to_ptr() }
    }

    /// Unwraps the typed pointer as a reference
    #[inline]
    pub const fn get_mut(self) -> &'mem mut T {
        unsafe { &mut *self.0.to_ptr() }
    }
}

/// A pointer to an uninitialized value with a lifetime.
#[repr(transparent)]
pub struct TypedPtrConst<'mem, T: ?Sized>(Ptr, PhantomData<&'mem T>);

impl<T: ?Sized> fmt::Debug for TypedPtrConst<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.ptr.as_ptr().fmt(f)
    }
}

impl<'mem, T: ?Sized> TypedPtrConst<'mem, T> {
    /// Creates a new typed pointer from a reference
    #[inline]
    pub const fn new(ptr: &'mem T) -> Self {
        Self(
            Ptr::from_ptr(unsafe { NonNull::new_unchecked(ptr as *const T as *mut T) }),
            PhantomData,
        )
    }

    /// Unwraps the typed pointer
    #[inline]
    pub const fn as_ptr(self) -> *const T {
        unsafe { self.0.to_ptr() }
    }

    /// Unwraps the typed pointer as a reference
    #[inline]
    pub const fn get(self) -> &'mem T {
        unsafe { &*self.0.to_ptr() }
    }
}

impl<'mem, T: ?Sized> From<&'mem T> for TypedPtrConst<'mem, T> {
    #[inline]
    fn from(value: &'mem T) -> Self {
        Self::new(value)
    }
}

impl<'mem, T: ?Sized> From<&'mem mut T> for TypedPtrMut<'mem, T> {
    #[inline]
    fn from(value: &'mem mut T) -> Self {
        Self::new(value)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C)]
///  pointer (fat pointer) structure holding a data pointer and metadata (for unsized types).
struct Ptr {
    ptr: NonNull<u8>,
    metadata: *const (),
}

/// the layout of pointers to DST is not guaranteed so we try to detect it in a const-friendly way
enum PtrLayout {
    /// layout is { ptr, metadata }
    PtrFirst,
    /// layout is { metadata, ptr }
    PtrLast,
}

impl PtrLayout {
    const FOR_SLICE: Self = const {
        unsafe {
            // null slice pointer with non-zero length
            let ptr: *const [()] = core::ptr::slice_from_raw_parts(core::ptr::null::<()>(), 1);
            let ptr: [*const (); 2] = core::mem::transmute(ptr);

            // look for the null part
            if ptr[0].is_null() {
                // make sure the length is non-null
                assert!(!ptr[1].is_null());
                Self::PtrFirst
            } else {
                Self::PtrLast
            }
        }
    };

    const FOR_TRAIT: Self = const {
        unsafe {
            trait Trait {}
            impl Trait for () {}

            // null dyn Trait pointer with non-null vtable (has to point to at least size and alignment)
            let ptr: *const dyn Trait = core::ptr::null::<()>();
            let ptr: [*const (); 2] = core::mem::transmute(ptr);

            // look for the null part
            if ptr[0].is_null() {
                // make sure the vtable is non-null
                assert!(!ptr[1].is_null());
                Self::PtrFirst
            } else {
                Self::PtrLast
            }
        }
    };
}

pub(crate) const PTR_FIRST: bool = {
    match (PtrLayout::FOR_SLICE, PtrLayout::FOR_TRAIT) {
        (PtrLayout::PtrFirst, PtrLayout::PtrFirst) => true,
        (PtrLayout::PtrLast, PtrLayout::PtrLast) => false,
        _ => panic!(),
    }
};

impl Ptr {
    #[inline]
    const fn from_ptr<T: ?Sized>(ptr: NonNull<T>) -> Self {
        let ptr = ptr.as_ptr();
        if const { size_of::<*mut T>() == size_of::<*mut u8>() } {
            Self {
                ptr: unsafe { NonNull::new_unchecked(ptr as *mut u8) },
                metadata: core::ptr::null(),
            }
        } else if const { size_of::<*mut T>() == 2 * size_of::<*mut u8>() } {
            let ptr = unsafe { core::mem::transmute_copy::<*mut T, [*mut u8; 2]>(&ptr) };

            if const { PTR_FIRST } {
                Self {
                    ptr: unsafe { NonNull::new_unchecked(ptr[0]) },
                    metadata: ptr[1] as *const (),
                }
            } else {
                Self {
                    ptr: unsafe { NonNull::new_unchecked(ptr[1]) },
                    metadata: ptr[0] as *const (),
                }
            }
        } else {
            panic!()
        }
    }

    #[inline]
    const unsafe fn to_ptr<T: ?Sized>(self) -> *mut T {
        let ptr = self.ptr.as_ptr();
        if const { size_of::<*mut T>() == size_of::<*mut u8>() } {
            unsafe { core::mem::transmute_copy(&ptr) }
        } else if const { size_of::<*mut T>() == 2 * size_of::<*mut u8>() } {
            let ptr = [ptr, self.metadata as *mut u8];
            if const { PTR_FIRST } {
                unsafe { core::mem::transmute_copy::<[*mut u8; 2], *mut T>(&ptr) }
            } else {
                unsafe { core::mem::transmute_copy::<[*mut u8; 2], *mut T>(&[ptr[1], ptr[0]]) }
            }
        } else {
            panic!()
        }
    }
}

/// A type-erased, wide pointer to an uninitialized value.
///
/// This can be useful for working with dynamically sized types, like slices or trait objects,
/// where both a pointer and metadata (such as length or vtable) need to be stored.
///
/// The lifetime `'mem` represents the borrow of the underlying uninitialized memory.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct PtrUninit<'mem> {
    ptr: Ptr,
    phantom: PhantomData<&'mem mut ()>,
}

impl<'mem> PtrUninit<'mem> {
    /// Create a new opaque pointer from a mutable pointer
    #[inline]
    pub const fn new<T: ?Sized>(ptr: NonNull<T>) -> Self {
        Self {
            ptr: Ptr::from_ptr(ptr),
            phantom: PhantomData,
        }
    }

    /// Copies memory from a source pointer into this location and returns PtrMut
    ///
    /// # Safety
    ///
    /// - The source pointer must be valid for reads of `len` bytes
    /// - This pointer must be valid for writes of `len` bytes and properly aligned
    /// - The regions may not overlap
    #[inline]
    pub const unsafe fn copy_from<'src>(
        self,
        src: PtrConst<'src>,
        shape: &'static Shape,
    ) -> Result<PtrMut<'mem>, UnsizedError> {
        let Ok(layout) = shape.layout.sized_layout() else {
            return Err(UnsizedError);
        };
        // SAFETY: The caller is responsible for upholding the invariants:
        // - `src` must be valid for reads of `shape.size` bytes
        // - `self` must be valid for writes of `shape.size` bytes and properly aligned
        // - The regions may not overlap
        unsafe {
            core::ptr::copy_nonoverlapping(
                src.as_byte_ptr(),
                self.as_mut_byte_ptr(),
                layout.size(),
            );
            Ok(self.assume_init())
        }
    }

    /// Creates a new opaque pointer from a reference to a [`core::mem::MaybeUninit`]
    ///
    /// The pointer will point to the potentially uninitialized contents
    #[inline]
    pub const fn from_maybe_uninit<T>(borrow: &'mem mut core::mem::MaybeUninit<T>) -> Self {
        Self {
            ptr: Ptr::from_ptr(unsafe { NonNull::new_unchecked(borrow) }),
            phantom: PhantomData,
        }
    }

    /// Assumes the pointer is initialized and returns an `Opaque` pointer
    ///
    /// # Safety
    ///
    /// The pointer must actually be pointing to initialized memory of the correct type.
    #[inline]
    pub const unsafe fn assume_init(self) -> PtrMut<'mem> {
        PtrMut {
            ptr: self.ptr,
            phantom: PhantomData,
        }
    }

    /// Write a value to this location and convert to an initialized pointer
    ///
    /// # Safety
    ///
    /// The pointer must be properly aligned for T and point to allocated memory
    /// that can be safely written to.
    #[inline]
    pub const unsafe fn put<T>(self, value: T) -> PtrMut<'mem> {
        unsafe {
            core::ptr::write(self.ptr.to_ptr::<T>(), value);
            self.assume_init()
        }
    }

    /// Returns the underlying raw pointer as a byte pointer
    #[inline]
    pub const fn as_mut_byte_ptr(self) -> *mut u8 {
        unsafe { self.ptr.to_ptr() }
    }

    /// Returns the underlying raw pointer as a const byte pointer
    #[inline]
    pub const fn as_byte_ptr(self) -> *const u8 {
        unsafe { self.ptr.to_ptr() }
    }

    /// Returns a pointer with the given offset added
    ///
    /// # Safety
    ///
    /// Offset is within the bounds of the allocated memory
    pub const unsafe fn field_uninit_at(self, offset: usize) -> PtrUninit<'mem> {
        PtrUninit {
            ptr: Ptr {
                ptr: unsafe { self.ptr.ptr.byte_add(offset) },
                metadata: self.ptr.metadata,
            },
            phantom: PhantomData,
        }
    }

    /// Returns a pointer with the given offset added, assuming it's initialized
    ///
    /// # Safety
    ///
    /// The pointer plus offset must be:
    /// - Within bounds of the allocated object
    /// - Properly aligned for the type being pointed to
    /// - Point to initialized data of the correct type
    #[inline]
    pub const unsafe fn field_init_at(self, offset: usize) -> PtrMut<'mem> {
        unsafe { self.field_uninit_at(offset).assume_init() }
    }
}

/// A type-erased, read-only wide pointer to an initialized value.
///
/// Like [`PtrConst`], but for unsized types where metadata is needed. Cannot be null
/// (but may be dangling for ZSTs). The lifetime `'mem` represents the borrow of the
/// underlying memory, which must remain valid and initialized.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct PtrConst<'mem> {
    ptr: Ptr,
    phantom: PhantomData<&'mem ()>,
}

impl fmt::Debug for PtrConst<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.ptr.ptr.as_ptr().fmt(f)
    }
}
impl fmt::Debug for PtrUninit<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.ptr.ptr.as_ptr().fmt(f)
    }
}
impl fmt::Debug for PtrMut<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.ptr.ptr.as_ptr().fmt(f)
    }
}

impl<'mem> PtrConst<'mem> {
    /// Creates a new wide const pointer from a raw pointer to a (potentially unsized) object.
    ///
    /// # Arguments
    ///
    /// * `ptr` - Raw pointer to the object. Can be a pointer to a DST (e.g., slice, trait object).
    ///
    /// # Panics
    ///
    /// Panics if a thin pointer is provided where a wide pointer is expected.
    #[inline]
    pub const fn new<T: ?Sized>(ptr: NonNull<T>) -> Self {
        Self {
            ptr: Ptr::from_ptr(ptr),
            phantom: PhantomData,
        }
    }

    /// Returns the underlying data pointer as a pointer to `u8` (the address of the object).
    #[inline]
    pub const fn as_byte_ptr(self) -> *const u8 {
        self.ptr.ptr.as_ptr() as *const u8
    }

    /// Borrows the underlying object as a reference of type `T`.
    ///
    /// # Safety
    ///
    /// - `T` must be the actual underlying (potentially unsized) type of the pointed-to memory.
    /// - The memory must remain valid and not be mutated while this reference exists.
    /// - The pointer must be correctly aligned and point to a valid, initialized value for type `T`.
    #[inline]
    pub const unsafe fn get<T: ?Sized>(self) -> &'mem T {
        unsafe { self.ptr.to_ptr::<T>().as_ref().unwrap() }
    }

    /// Gets the underlying raw pointer as a pointer of type T
    ///
    /// # Safety
    ///
    /// Must be called with the original type T that was used to create this pointer
    #[inline]
    pub const unsafe fn as_ptr<T: ?Sized>(self) -> *const T {
        unsafe { self.ptr.to_ptr() }
    }

    /// Returns a pointer with the given offset added
    ///
    /// # Safety
    ///
    /// Offset must be within the bounds of the allocated memory,
    /// and the resulting pointer must be properly aligned.
    #[inline]
    pub const unsafe fn field(self, offset: usize) -> PtrConst<'mem> {
        PtrConst {
            ptr: Ptr {
                ptr: unsafe { self.ptr.ptr.byte_add(offset) },
                metadata: self.ptr.metadata,
            },
            phantom: PhantomData,
        }
    }

    /// Exposes [`core::ptr::read`]
    ///
    /// # Safety
    ///
    /// `T` must be the actual underlying type of the pointed-to memory.
    /// The memory must be properly initialized and aligned for type `T`.
    #[inline]
    pub const unsafe fn read<T>(self) -> T {
        unsafe { core::ptr::read(self.as_ptr()) }
    }
}

/// A type-erased, mutable wide pointer to an initialized value.
///
/// Like [`PtrMut`], but for unsized types where metadata is needed. Provides mutable access
/// to the underlying object, whose borrow is tracked by lifetime `'mem`.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct PtrMut<'mem> {
    ptr: Ptr,
    phantom: PhantomData<&'mem mut ()>,
}

impl<'mem> PtrMut<'mem> {
    /// Creates a new mutable wide pointer from a raw pointer to a (potentially unsized) object.
    ///
    /// # Arguments
    ///
    /// * `ptr` - Raw mutable pointer to the object. Can be a pointer to a DST (e.g., slice, trait object).
    ///
    /// # Panics
    ///
    /// Panics if a thin pointer is provided where a wide pointer is expected.
    #[inline]
    pub const fn new<T: ?Sized>(ptr: NonNull<T>) -> Self {
        Self {
            ptr: Ptr::from_ptr(ptr),
            phantom: PhantomData,
        }
    }

    /// Gets the underlying raw pointer
    #[inline]
    pub const fn as_byte_ptr(self) -> *const u8 {
        self.ptr.ptr.as_ptr()
    }

    /// Gets the underlying raw pointer as mutable
    #[inline]
    pub const fn as_mut_byte_ptr(self) -> *mut u8 {
        self.ptr.ptr.as_ptr()
    }

    /// Assumes the pointer is initialized and returns an `Opaque` pointer
    ///
    /// # Safety
    ///
    /// The pointer must actually be pointing to initialized memory of the correct type.
    #[inline]
    pub const fn as_uninit(self) -> PtrUninit<'mem> {
        PtrUninit {
            ptr: self.ptr,
            phantom: PhantomData,
        }
    }

    /// Gets the underlying raw pointer as a pointer of type T
    ///
    /// # Safety
    ///
    /// Must be called with the original type T that was used to create this pointer
    #[inline]
    pub const unsafe fn as_ptr<T: ?Sized>(self) -> *const T {
        unsafe { self.ptr.to_ptr() as *const T }
    }

    /// Gets the underlying raw pointer as a mutable pointer of type T
    ///
    /// # Safety
    ///
    /// `T` must be the _actual_ underlying type. You're downcasting with no guardrails.
    #[inline]
    pub const unsafe fn as_mut<'borrow: 'mem, T: ?Sized>(self) -> &'borrow mut T {
        unsafe { &mut *self.ptr.to_ptr::<T>() }
    }

    /// Gets the underlying raw pointer as a const pointer of type T
    ///
    /// # Safety
    ///
    /// `T` must be the _actual_ underlying type. You're downcasting with no guardrails.
    /// You must respect AXM (aliasing xor mutability). Holding onto the borrow while
    /// calling as_mut is UB.
    ///
    /// Basically this is UB land. Careful.
    #[inline]
    pub const unsafe fn get<'borrow: 'mem, T>(self) -> &'borrow T {
        unsafe { &*(self.ptr.to_ptr::<T>() as *const T) }
    }

    /// Make a const ptr out of this mut ptr
    #[inline]
    pub const fn as_const<'borrow: 'mem>(self) -> PtrConst<'borrow> {
        PtrConst {
            ptr: self.ptr,
            phantom: PhantomData,
        }
    }

    /// Exposes [`core::ptr::read`]
    ///
    /// # Safety
    ///
    /// `T` must be the actual underlying type of the pointed-to memory.
    /// The memory must be properly initialized and aligned for type `T`.
    #[inline]
    pub const unsafe fn read<T>(self) -> T {
        unsafe { core::ptr::read(self.as_mut()) }
    }

    /// Exposes [`core::ptr::drop_in_place`]
    ///
    /// # Safety
    ///
    /// `T` must be the actual underlying type of the pointed-to memory.
    /// The memory must be properly initialized and aligned for type `T`.
    /// After calling this function, the memory should not be accessed again
    /// until it is properly reinitialized.
    #[inline]
    pub unsafe fn drop_in_place<T: ?Sized>(self) -> PtrUninit<'mem> {
        unsafe { core::ptr::drop_in_place(self.as_ptr::<T>() as *mut T) }
        PtrUninit {
            ptr: self.ptr,
            phantom: PhantomData,
        }
    }

    /// Write a value to this location after dropping the existing value
    ///
    /// # Safety
    ///
    /// - The pointer must be properly aligned for T and point to allocated memory
    ///   that can be safely written to.
    /// - T must be the actual type of the object being pointed to
    /// - The memory must already be initialized to a valid T value
    #[inline]
    pub unsafe fn replace<T>(self, value: T) -> Self {
        unsafe { self.drop_in_place::<T>().put(value) }
    }
}
