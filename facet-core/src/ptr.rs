//! Opaque pointers
//!
//! Type-erased pointer helpers for working with reflected values

use core::{marker::PhantomData, ptr::NonNull};

use crate::{Shape, UnsizedError};

const fn get_fat_part<T: ?Sized>(ptr: *const T) -> Option<usize> {
    const USIZE_SIZE: usize = size_of::<usize>();

    let ptr_size = size_of_val(&ptr);
    if ptr_size == USIZE_SIZE {
        None
    } else if ptr_size == 2 * USIZE_SIZE {
        let fat_ptr = unsafe { &*(&raw const ptr).cast::<[usize; 2]>() };
        Some(fat_ptr[1])
    } else {
        unreachable!()
    }
}

/// A type-erased pointer to an uninitialized value
#[derive(Debug, Clone, Copy)]
pub struct PtrUninit<'mem> {
    ptr: *mut u8,
    fat_part: Option<usize>,
    phantom: PhantomData<&'mem mut ()>,
}

impl<'mem> PtrUninit<'mem> {
    /// Copies memory from a source pointer into this location and returns PtrMut
    ///
    /// # Safety
    ///
    /// - The source pointer must be valid for reads of `len` bytes
    /// - This pointer must be valid for writes of `len` bytes and properly aligned
    /// - The regions may not overlap
    pub unsafe fn copy_from<'src>(
        self,
        src: PtrConst<'src>,
        shape: &'static Shape,
    ) -> Result<PtrMut<'mem>, UnsizedError> {
        let layout = shape.layout.sized_layout()?;
        // SAFETY: The caller is responsible for upholding the invariants:
        // - `src` must be valid for reads of `shape.size` bytes
        // - `self` must be valid for writes of `shape.size` bytes and properly aligned
        // - The regions may not overlap
        unsafe {
            core::ptr::copy_nonoverlapping(src.as_byte_ptr(), self.ptr, layout.size());
            Ok(self.assume_init())
        }
    }

    /// Create a new opaque pointer from a mutable pointer
    ///
    /// This is safe because it's generic over T
    pub fn new<T: ?Sized>(ptr: *mut T) -> Self {
        Self {
            ptr: ptr as *mut u8,
            fat_part: get_fat_part(ptr),
            phantom: PhantomData,
        }
    }

    /// Creates a new opaque pointer from a reference to a [`core::mem::MaybeUninit`]
    ///
    /// The pointer will point to the potentially uninitialized contents
    ///
    /// This is safe because it's generic over T
    pub fn from_maybe_uninit<T>(borrow: &'mem mut core::mem::MaybeUninit<T>) -> Self {
        Self {
            ptr: borrow.as_mut_ptr() as *mut u8,
            fat_part: None,
            phantom: PhantomData,
        }
    }

    /// Assumes the pointer is initialized and returns an `Opaque` pointer
    ///
    /// # Safety
    ///
    /// The pointer must actually be pointing to initialized memory of the correct type.
    pub unsafe fn assume_init(self) -> PtrMut<'mem> {
        let ptr = unsafe { NonNull::new_unchecked(self.ptr) };
        PtrMut {
            ptr,
            fat_part: self.fat_part,
            phantom: PhantomData,
        }
    }

    /// Write a value to this location and convert to an initialized pointer
    ///
    /// # Safety
    ///
    /// The pointer must be properly aligned for T and point to allocated memory
    /// that can be safely written to.
    pub unsafe fn put<T>(self, value: T) -> PtrMut<'mem> {
        unsafe {
            core::ptr::write(self.ptr as *mut T, value);
            self.assume_init()
        }
    }

    /// Returns the underlying raw pointer as a byte pointer
    pub fn as_mut_byte_ptr(self) -> *mut u8 {
        self.ptr
    }

    /// Returns the underlying raw pointer as a const byte pointer
    pub fn as_byte_ptr(self) -> *const u8 {
        self.ptr
    }

    /// Returns a pointer with the given offset added
    ///
    /// # Safety
    ///
    /// Offset is within the bounds of the allocated memory
    pub unsafe fn field_uninit_at(self, offset: usize) -> PtrUninit<'mem> {
        if self.fat_part.is_some() {
            panic!("Can't access field of fat pointer");
        }
        PtrUninit::new(unsafe { self.ptr.byte_add(offset) })
    }

    /// Returns a pointer with the given offset added, assuming it's initialized
    ///
    /// # Safety
    ///
    /// The pointer plus offset must be:
    /// - Within bounds of the allocated object
    /// - Properly aligned for the type being pointed to
    /// - Point to initialized data of the correct type
    pub unsafe fn field_init_at(self, offset: usize) -> PtrMut<'mem> {
        if self.fat_part.is_some() {
            panic!("Can't access field of fat pointer");
        }
        PtrMut::new(unsafe { self.ptr.add(offset) })
    }
}

/// A type-erased read-only pointer to an initialized value.
///
/// Cannot be null. May be dangling (for ZSTs)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PtrConst<'mem> {
    ptr: NonNull<u8>,
    fat_part: Option<usize>,
    phantom: PhantomData<&'mem ()>,
}

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
    pub const fn new<T: ?Sized>(ptr: *const T) -> Self {
        Self {
            ptr: unsafe { NonNull::new_unchecked(ptr.cast::<u8>().cast_mut()) },
            fat_part: get_fat_part(ptr),
            phantom: PhantomData,
        }
    }

    /// Gets the underlying raw pointer as a byte pointer
    pub const fn as_byte_ptr(self) -> *const u8 {
        self.ptr.as_ptr()
    }

    pub const fn fat_part(self) -> Option<usize> {
        self.fat_part
    }

    /// Gets the underlying raw pointer as a pointer of type T
    ///
    /// # Safety
    ///
    /// Must be called with the original type T that was used to create this pointer
    pub const unsafe fn as_ptr<T>(self) -> *const T {
        self.as_byte_ptr().cast::<T>()
    }

    /// Gets the underlying raw pointer as a const pointer of type T
    ///
    /// # Safety
    ///
    /// `T` must be the _actual_ underlying type. You're downcasting with no guardrails.
    pub const unsafe fn get<'borrow: 'mem, T>(self) -> &'borrow T {
        // TODO: rename to `get`, or something else? it's technically a borrow...
        unsafe { &*self.as_ptr() }
    }

    /// Returns a pointer with the given offset added
    ///
    /// # Safety
    ///
    /// Offset must be within the bounds of the allocated memory,
    /// and the resulting pointer must be properly aligned.
    pub const unsafe fn field(self, offset: usize) -> PtrConst<'mem> {
        if self.fat_part.is_some() {
            panic!("Can't access field of fat pointer");
        }
        PtrConst::new(unsafe { self.as_byte_ptr().byte_add(offset) })
    }

    /// Exposes [`core::ptr::read`]
    ///
    /// # Safety
    ///
    /// `T` must be the actual underlying type of the pointed-to memory.
    /// The memory must be properly initialized and aligned for type `T`.
    pub const unsafe fn read<T>(self) -> T {
        unsafe { core::ptr::read(self.as_ptr()) }
    }

    pub const unsafe fn as_mut(self) -> PtrMut<'mem> {
        PtrMut {
            ptr: self.ptr,
            fat_part: self.fat_part,
            phantom: PhantomData,
        }
    }
}

/// A type-erased pointer to an initialized value
#[derive(Clone, Copy)]
pub struct PtrMut<'mem> {
    ptr: NonNull<u8>,
    fat_part: Option<usize>,
    phantom: PhantomData<&'mem mut ()>,
}

impl<'mem> PtrMut<'mem> {
    /// Create a new opaque pointer from a raw pointer
    ///
    /// # Safety
    ///
    /// The pointer must be valid, aligned, and point to initialized memory
    /// of the correct type, and be valid for lifetime `'mem`.
    ///
    /// It's encouraged to take the address of something with `&raw mut x`, rather than `&x`
    pub const fn new<T>(ptr: *mut T) -> Self {
        unsafe { PtrConst::new(ptr).as_mut() }
    }

    /// Gets the underlying raw pointer
    pub const fn as_byte_ptr(self) -> *const u8 {
        self.ptr.as_ptr()
    }

    /// Gets the underlying raw pointer as mutable
    pub const fn as_mut_byte_ptr(self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    /// Gets the underlying raw pointer as a pointer of type T
    ///
    /// # Safety
    ///
    /// Must be called with the original type T that was used to create this pointer
    pub const unsafe fn as_ptr<T>(self) -> *const T {
        self.as_byte_ptr().cast::<T>()
    }

    /// Gets the underlying raw pointer as a pointer of type T
    ///
    /// # Safety
    ///
    /// Must be called with the original type T that was used to create this pointer
    pub const unsafe fn as_mut_ptr<T>(self) -> *mut T {
        self.as_mut_byte_ptr().cast::<T>()
    }

    /// Gets the underlying raw pointer as a mutable pointer of type T
    ///
    /// # Safety
    ///
    /// `T` must be the _actual_ underlying type. You're downcasting with no guardrails.
    pub const unsafe fn as_mut<'borrow: 'mem, T>(self) -> &'borrow mut T {
        unsafe { &mut *self.as_mut_ptr() }
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
    pub const unsafe fn get<'borrow: 'mem, T>(self) -> &'borrow T {
        unsafe { &*self.as_ptr() }
    }

    /// Make a const ptr out of this mut ptr
    pub const fn as_const<'borrow: 'mem>(self) -> PtrConst<'borrow> {
        PtrConst {
            ptr: self.ptr,
            fat_part: self.fat_part,
            phantom: PhantomData,
        }
    }

    /// Exposes [`core::ptr::read`]
    ///
    /// # Safety
    ///
    /// `T` must be the actual underlying type of the pointed-to memory.
    /// The memory must be properly initialized and aligned for type `T`.
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
    pub unsafe fn drop_in_place<T>(self) -> PtrUninit<'mem> {
        unsafe { core::ptr::drop_in_place(self.as_mut::<T>()) }
        PtrUninit {
            ptr: self.ptr.as_ptr(),
            fat_part: self.fat_part,
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
    pub unsafe fn replace<T>(self, value: T) -> Self {
        unsafe { self.drop_in_place::<T>().put(value) }
    }
}
