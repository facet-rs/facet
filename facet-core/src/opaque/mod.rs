//! Opaque pointers
//!
//! Type-erased pointer helpers for working with reflected values

use core::{marker::PhantomData, ptr::NonNull};

/// A type-erased pointer to an uninitialized value
#[derive(Clone, Copy)]
pub struct OpaqueUninit<'mem>(*mut u8, PhantomData<&'mem mut ()>);

impl<'mem> OpaqueUninit<'mem> {
    /// Create a new opaque pointer from a mutable pointer
    ///
    /// This is safe because it's generic over T
    pub fn new<T>(ptr: *mut T) -> Self {
        Self(ptr as *mut u8, PhantomData)
    }

    /// Creates a new opaque pointer from a reference to a MaybeUninit<T>
    ///
    /// The pointer will point to the potentially uninitialized contents
    ///
    /// This is safe because it's generic over T
    pub fn from_maybe_uninit<T>(borrow: &'mem mut core::mem::MaybeUninit<T>) -> Self {
        Self(borrow.as_mut_ptr() as *mut u8, PhantomData)
    }

    /// Assumes the pointer is initialized and returns an `Opaque` pointer
    ///
    /// # Safety
    ///
    /// The pointer must actually be pointing to initialized memory of the correct type.
    pub unsafe fn assume_init(self) -> Opaque<'mem> {
        let ptr = unsafe { NonNull::new_unchecked(self.0) };
        Opaque(ptr, PhantomData)
    }

    /// Write a value to this location and convert to an initialized pointer
    ///
    /// # Safety
    ///
    /// The pointer must be properly aligned for T and point to allocated memory
    /// that can be safely written to.
    pub unsafe fn write<T>(self, value: T) -> Opaque<'mem> {
        unsafe {
            core::ptr::write(self.0 as *mut T, value);
            self.assume_init()
        }
    }

    /// Returns the underlying raw pointer as a byte pointer
    pub fn as_mut_bytes(self) -> *mut u8 {
        self.0
    }

    /// Returns the underlying raw pointer as a const byte pointer
    pub fn as_bytes(self) -> *const u8 {
        self.0
    }

    /// Returns a pointer with the given offset added
    ///
    /// # Safety
    ///
    /// Offset is within the bounds of the allocated memory
    pub unsafe fn field_uninit(self, offset: usize) -> OpaqueUninit<'mem> {
        OpaqueUninit(unsafe { self.0.byte_add(offset) }, PhantomData)
    }

    /// Returns a pointer with the given offset added, assuming it's initialized
    ///
    /// # Safety
    ///
    /// The pointer plus offset must be:
    /// - Within bounds of the allocated object
    /// - Properly aligned for the type being pointed to
    /// - Point to initialized data of the correct type
    pub unsafe fn field_init(self, offset: usize) -> Opaque<'mem> {
        Opaque(
            unsafe { NonNull::new_unchecked(self.0.add(offset)) },
            PhantomData,
        )
    }
}

/// A type-erased read-only pointer to an initialized value.
///
/// Cannot be null. May be dangling (for ZSTs)
#[derive(Clone, Copy)]
pub struct OpaqueConst<'mem>(NonNull<u8>, PhantomData<&'mem ()>);

impl<'mem> OpaqueConst<'mem> {
    /// Create a new opaque const pointer from a raw pointer
    ///
    /// # Safety
    ///
    /// The pointer must be non-null, valid, aligned, and point to initialized memory
    /// of the correct type, and be valid for lifetime `'mem`.
    ///
    /// It's encouraged to take the address of something with `&raw const x`, rather than `&x`
    pub fn new<T>(ptr: *const T) -> Self {
        unsafe { Self(NonNull::new_unchecked(ptr as *mut u8), PhantomData) }
    }

    /// Gets the underlying raw pointer as a byte pointer
    pub fn as_byte_ptr(self) -> *const u8 {
        self.0.as_ptr()
    }

    /// Gets the underlying raw pointer as a pointer of type T
    ///
    /// # Safety
    ///
    /// Must be called with the original type T that was used to create this pointer
    pub unsafe fn as_ptr<T>(self) -> *const T {
        self.0.as_ptr() as *const T
    }

    /// Gets the underlying raw pointer as a const pointer of type T
    ///
    /// # Safety
    ///
    /// `T` must be the _actual_ underlying type. You're downcasting with no guardrails.
    pub unsafe fn as_ref<'borrow: 'mem, T>(self) -> &'borrow T {
        unsafe { &*(self.0.as_ptr() as *const T) }
    }

    /// Returns a pointer with the given offset added
    ///
    /// # Safety
    ///
    /// Offset must be within the bounds of the allocated memory,
    /// and the resulting pointer must be properly aligned.
    pub unsafe fn field(self, offset: usize) -> OpaqueConst<'mem> {
        OpaqueConst(
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
    pub unsafe fn read<T>(self) -> T {
        unsafe { core::ptr::read(self.as_ptr()) }
    }
}

/// A type-erased pointer to an initialized value
#[derive(Clone, Copy)]
pub struct Opaque<'mem>(NonNull<u8>, PhantomData<&'mem mut ()>);

impl<'mem> Opaque<'mem> {
    /// Create a new opaque pointer from a raw pointer
    ///
    /// # Safety
    ///
    /// The pointer must be valid, aligned, and point to initialized memory
    /// of the correct type, and be valid for lifetime `'mem`.
    ///
    /// It's encouraged to take the address of something with `&raw mut x`, rather than `&x`
    pub fn new<T>(ptr: *mut T) -> Self {
        Self(
            unsafe { NonNull::new_unchecked(ptr as *mut u8) },
            PhantomData,
        )
    }

    /// Gets the underlying raw pointer
    pub fn as_byte_ptr(self) -> *const u8 {
        self.0.as_ptr()
    }

    /// Gets the underlying raw pointer as mutable
    pub fn as_mut_byte_ptr(self) -> *mut u8 {
        self.0.as_ptr()
    }

    /// Gets the underlying raw pointer as a pointer of type T
    ///
    /// # Safety
    ///
    /// Must be called with the original type T that was used to create this pointer
    pub unsafe fn as_ptr<T>(self) -> *const T {
        self.0.as_ptr() as *const T
    }

    /// Gets the underlying raw pointer as a mutable pointer of type T
    ///
    /// # Safety
    ///
    /// `T` must be the _actual_ underlying type. You're downcasting with no guardrails.
    pub unsafe fn as_mut<'borrow: 'mem, T>(self) -> &'borrow mut T {
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
    pub unsafe fn as_ref<'borrow: 'mem, T>(self) -> &'borrow T {
        unsafe { &*(self.0.as_ptr() as *const T) }
    }

    /// Make a const ptr out of this mut ptr
    pub fn as_const<'borrow: 'mem>(self) -> OpaqueConst<'borrow> {
        OpaqueConst(self.0, PhantomData)
    }

    /// Exposes [`core::ptr::read`]
    ///
    /// # Safety
    ///
    /// `T` must be the actual underlying type of the pointed-to memory.
    /// The memory must be properly initialized and aligned for type `T`.
    pub unsafe fn read<T>(self) -> T {
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
    pub unsafe fn drop_in_place<T>(self) {
        unsafe { core::ptr::drop_in_place(self.as_mut::<T>()) }
    }
}
