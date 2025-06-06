//! Opaque pointers
//!
//! Type-erased pointer helpers for working with reflected values

use core::{marker::PhantomData, mem::transmute, ptr::NonNull};

use crate::{Shape, UnsizedError};

/// A type-erased pointer to an uninitialized value
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct PtrUninit<'mem>(*mut u8, PhantomData<&'mem mut ()>);

impl<'mem> PtrUninit<'mem> {
    /// Copies memory from a source pointer into this location and returns PtrMut
    ///
    /// # Safety
    ///
    /// - The source pointer must be valid for reads of `len` bytes
    /// - This pointer must be valid for writes of `len` bytes and properly aligned
    /// - The regions may not overlap
    #[inline]
    pub unsafe fn copy_from<'src, 'shape>(
        self,
        src: PtrConst<'src>,
        shape: &'shape Shape<'shape>,
    ) -> Result<PtrMut<'mem>, UnsizedError> {
        let layout = shape.layout.sized_layout()?;
        // SAFETY: The caller is responsible for upholding the invariants:
        // - `src` must be valid for reads of `shape.size` bytes
        // - `self` must be valid for writes of `shape.size` bytes and properly aligned
        // - The regions may not overlap
        unsafe {
            core::ptr::copy_nonoverlapping(src.as_byte_ptr(), self.0, layout.size());
            Ok(self.assume_init())
        }
    }

    /// Create a new opaque pointer from a mutable pointer
    ///
    /// This is safe because it's generic over T
    #[inline]
    pub fn new<T>(ptr: *mut T) -> Self {
        Self(ptr as *mut u8, PhantomData)
    }

    /// Creates a new opaque pointer from a reference to a [`core::mem::MaybeUninit`]
    ///
    /// The pointer will point to the potentially uninitialized contents
    ///
    /// This is safe because it's generic over T
    #[inline]
    pub fn from_maybe_uninit<T>(borrow: &'mem mut core::mem::MaybeUninit<T>) -> Self {
        Self(borrow.as_mut_ptr() as *mut u8, PhantomData)
    }

    /// Assumes the pointer is initialized and returns an `Opaque` pointer
    ///
    /// # Safety
    ///
    /// The pointer must actually be pointing to initialized memory of the correct type.
    #[inline]
    pub unsafe fn assume_init(self) -> PtrMut<'mem> {
        let ptr = unsafe { NonNull::new_unchecked(self.0) };
        PtrMut(ptr, PhantomData)
    }

    /// Write a value to this location and convert to an initialized pointer
    ///
    /// # Safety
    ///
    /// The pointer must be properly aligned for T and point to allocated memory
    /// that can be safely written to.
    #[inline]
    pub unsafe fn put<T>(self, value: T) -> PtrMut<'mem> {
        unsafe {
            core::ptr::write(self.0 as *mut T, value);
            self.assume_init()
        }
    }

    /// Returns the underlying raw pointer as a byte pointer
    #[inline]
    pub fn as_mut_byte_ptr(self) -> *mut u8 {
        self.0
    }

    /// Returns the underlying raw pointer as a const byte pointer
    #[inline]
    pub fn as_byte_ptr(self) -> *const u8 {
        self.0
    }

    /// Returns a pointer with the given offset added
    ///
    /// # Safety
    ///
    /// Offset is within the bounds of the allocated memory
    pub unsafe fn field_uninit_at(self, offset: usize) -> PtrUninit<'mem> {
        PtrUninit(unsafe { self.0.byte_add(offset) }, PhantomData)
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
    pub unsafe fn field_init_at(self, offset: usize) -> PtrMut<'mem> {
        PtrMut(
            unsafe { NonNull::new_unchecked(self.0.add(offset)) },
            PhantomData,
        )
    }
}

impl<'mem, T> From<TypedPtrUninit<'mem, T>> for PtrUninit<'mem> {
    fn from(ptr: TypedPtrUninit<'mem, T>) -> Self {
        PtrUninit::new(ptr.0)
    }
}

/// A pointer to an uninitialized value with a lifetime.
#[derive(Debug)]
#[repr(transparent)]
pub struct TypedPtrUninit<'mem, T>(*mut T, PhantomData<&'mem mut ()>);

impl<'mem, T> TypedPtrUninit<'mem, T> {
    /// Create a new opaque pointer from a mutable pointer
    ///
    /// This is safe because it's generic over T
    #[inline]
    pub fn new(ptr: *mut T) -> Self {
        Self(ptr, PhantomData)
    }

    /// Write a value to this location and convert to an initialized pointer
    ///
    /// # Safety
    ///
    /// The pointer must be properly aligned for T and point to allocated memory
    /// that can be safely written to.
    #[inline]
    pub unsafe fn put(self, value: T) -> &'mem mut T {
        unsafe {
            core::ptr::write(self.0, value);
            self.assume_init()
        }
    }
    /// Assumes the pointer is initialized and returns an `Opaque` pointer
    ///
    /// # Safety
    ///
    /// The pointer must actually be pointing to initialized memory of the correct type.
    #[inline]
    pub unsafe fn assume_init(self) -> &'mem mut T {
        unsafe { &mut *self.0 }
    }

    /// Returns a pointer with the given offset added
    ///
    /// # Safety
    ///
    /// Offset is within the bounds of the allocated memory and `U` is the correct type for the field.
    #[inline]
    pub unsafe fn field_uninit_at<U>(&mut self, offset: usize) -> TypedPtrUninit<'mem, U> {
        TypedPtrUninit(unsafe { self.0.byte_add(offset).cast() }, PhantomData)
    }
}

/// A type-erased read-only pointer to an initialized value.
///
/// Cannot be null. May be dangling (for ZSTs)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct PtrConst<'mem>(NonNull<u8>, PhantomData<&'mem ()>);

unsafe impl Send for PtrConst<'_> {}
unsafe impl Sync for PtrConst<'_> {}

impl<'mem> PtrConst<'mem> {
    /// Create a new opaque const pointer from a raw pointer
    ///
    /// # Safety
    ///
    /// The pointer must be non-null, valid, aligned, and point to initialized memory
    /// of the correct type, and be valid for lifetime `'mem`.
    ///
    /// It's encouraged to take the address of something with `&raw const x`, rather than `&x`
    #[inline]
    pub const fn new<T>(ptr: *const T) -> Self {
        unsafe { Self(NonNull::new_unchecked(ptr as *mut u8), PhantomData) }
    }

    /// Gets the underlying raw pointer as a byte pointer
    #[inline]
    pub const fn as_byte_ptr(self) -> *const u8 {
        self.0.as_ptr()
    }

    /// Gets the underlying raw pointer as a pointer of type T
    ///
    /// # Safety
    ///
    /// Must be called with the original type T that was used to create this pointer
    #[inline]
    pub const unsafe fn as_ptr<T>(self) -> *const T {
        if core::mem::size_of::<*const T>() == core::mem::size_of::<*const u8>() {
            unsafe { core::mem::transmute_copy(&(self.0.as_ptr())) }
        } else {
            panic!("cannot!");
        }
    }

    /// Gets the underlying raw pointer as a const pointer of type T
    ///
    /// # Safety
    ///
    /// `T` must be the _actual_ underlying type. You're downcasting with no guardrails.
    #[inline]
    pub const unsafe fn get<'borrow: 'mem, T>(self) -> &'borrow T {
        // TODO: rename to `get`, or something else? it's technically a borrow...
        unsafe { &*(self.0.as_ptr() as *const T) }
    }

    /// Returns a pointer with the given offset added
    ///
    /// # Safety
    ///
    /// Offset must be within the bounds of the allocated memory,
    /// and the resulting pointer must be properly aligned.
    #[inline]
    pub const unsafe fn field(self, offset: usize) -> PtrConst<'mem> {
        PtrConst(
            unsafe { NonNull::new_unchecked(self.0.as_ptr().byte_add(offset)) },
            PhantomData,
        )
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

/// A type-erased pointer to an initialized value
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct PtrMut<'mem>(NonNull<u8>, PhantomData<&'mem mut ()>);

impl<'mem> PtrMut<'mem> {
    /// Create a new opaque pointer from a raw pointer
    ///
    /// # Safety
    ///
    /// The pointer must be valid, aligned, and point to initialized memory
    /// of the correct type, and be valid for lifetime `'mem`.
    ///
    /// It's encouraged to take the address of something with `&raw mut x`, rather than `&x`
    #[inline]
    pub const fn new<T>(ptr: *mut T) -> Self {
        Self(
            unsafe { NonNull::new_unchecked(ptr as *mut u8) },
            PhantomData,
        )
    }

    /// Gets the underlying raw pointer
    #[inline]
    pub const fn as_byte_ptr(self) -> *const u8 {
        self.0.as_ptr()
    }

    /// Gets the underlying raw pointer as mutable
    #[inline]
    pub const fn as_mut_byte_ptr(self) -> *mut u8 {
        self.0.as_ptr()
    }

    /// Gets the underlying raw pointer as a pointer of type T
    ///
    /// # Safety
    ///
    /// Must be called with the original type T that was used to create this pointer
    #[inline]
    pub const unsafe fn as_ptr<T>(self) -> *const T {
        self.0.as_ptr() as *const T
    }

    /// Gets the underlying raw pointer as a mutable pointer of type T
    ///
    /// # Safety
    ///
    /// `T` must be the _actual_ underlying type. You're downcasting with no guardrails.
    #[inline]
    pub const unsafe fn as_mut<'borrow: 'mem, T>(self) -> &'borrow mut T {
        unsafe { &mut *(self.0.as_ptr() as *mut T) }
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
        unsafe { &*(self.0.as_ptr() as *const T) }
    }

    /// Make a const ptr out of this mut ptr
    #[inline]
    pub const fn as_const<'borrow: 'mem>(self) -> PtrConst<'borrow> {
        PtrConst(self.0, PhantomData)
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
    pub unsafe fn drop_in_place<T>(self) -> PtrUninit<'mem> {
        unsafe { core::ptr::drop_in_place(self.as_mut::<T>()) }
        PtrUninit(self.0.as_ptr(), PhantomData)
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

#[derive(Clone, Copy)]
#[repr(C)]
/// Wide pointer (fat pointer) structure holding a data pointer and metadata (for unsized types).
struct PtrWide {
    ptr: NonNull<u8>,
    metadata: usize,
}

impl PtrWide {
    const fn from_ptr<T: ?Sized>(ptr: *mut T) -> Self {
        if size_of_val(&ptr) != size_of::<Self>() {
            panic!("Tried to construct a wide pointer from a thin pointer");
        }
        let ptr_ref = &ptr;
        let self_ref = unsafe { transmute::<&*mut T, &Self>(ptr_ref) };
        *self_ref
    }

    unsafe fn to_ptr<T: ?Sized>(self) -> *mut T {
        if size_of::<*mut T>() != size_of::<Self>() {
            panic!("Tried to get a wide pointer as a thin pointer");
        }
        let self_ref = &self;
        let ptr_ref = unsafe { transmute::<&Self, &*mut T>(self_ref) };
        *ptr_ref
    }
}

/// A type-erased, wide pointer to an uninitialized value.
///
/// This can be useful for working with dynamically sized types, like slices or trait objects,
/// where both a pointer and metadata (such as length or vtable) need to be stored.
///
/// The lifetime `'mem` represents the borrow of the underlying uninitialized memory.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct PtrUninitWide<'mem> {
    ptr: PtrWide,
    phantom: PhantomData<&'mem mut ()>,
}

/// A type-erased, read-only wide pointer to an initialized value.
///
/// Like [`PtrConst`], but for unsized types where metadata is needed. Cannot be null
/// (but may be dangling for ZSTs). The lifetime `'mem` represents the borrow of the
/// underlying memory, which must remain valid and initialized.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct PtrConstWide<'mem> {
    ptr: PtrWide,
    phantom: PhantomData<&'mem ()>,
}

impl<'mem> PtrConstWide<'mem> {
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
    pub const fn new<T: ?Sized>(ptr: *const T) -> Self {
        Self {
            ptr: PtrWide::from_ptr(ptr.cast_mut()),
            phantom: PhantomData,
        }
    }

    /// Returns the underlying data pointer as a pointer to `u8` (the address of the object).
    #[inline]
    pub fn as_byte_ptr(self) -> *const u8 {
        self.ptr.ptr.as_ptr()
    }

    /// Borrows the underlying object as a reference of type `T`.
    ///
    /// # Safety
    ///
    /// - `T` must be the actual underlying (potentially unsized) type of the pointed-to memory.
    /// - The memory must remain valid and not be mutated while this reference exists.
    /// - The pointer must be correctly aligned and point to a valid, initialized value for type `T`.
    #[inline]
    pub unsafe fn get<T: ?Sized>(self) -> &'mem T {
        unsafe { self.ptr.to_ptr::<T>().as_ref().unwrap() }
    }
}

/// A type-erased, mutable wide pointer to an initialized value.
///
/// Like [`PtrMut`], but for unsized types where metadata is needed. Provides mutable access
/// to the underlying object, whose borrow is tracked by lifetime `'mem`.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct PtrMutWide<'mem> {
    ptr: PtrWide,
    phantom: PhantomData<&'mem mut ()>,
}

impl<'mem> PtrMutWide<'mem> {
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
    pub const fn new<T: ?Sized>(ptr: *mut T) -> Self {
        Self {
            ptr: PtrWide::from_ptr(ptr),
            phantom: PhantomData,
        }
    }
}
