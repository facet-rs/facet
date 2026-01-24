//! Built-in types for facet reflection
//!
//! This module provides type-erased wrappers that bundle a pointer with its [`Shape`].
//! Types are divided into two categories with different safety invariants:
//!
//! # Pointer-like Types (no lifetime parameter)
//!
//! These types bundle a raw pointer with a shape. They have **no validity guarantees** -
//! the caller is responsible for ensuring the pointer is valid before any dereference.
//! All operations that access the pointed-to data are `unsafe`.
//!
//! - [`OxPtrConst`] - read-only pointer + shape, for vtable dispatch
//! - [`OxPtrMut`] - mutable pointer + shape, for vtable dispatch
//! - [`OxPtrUninit`] - uninitialized pointer + shape, for allocation
//!
//! These are designed for use in vtable function signatures where lifetimes cannot
//! be expressed, and the caller must manually ensure validity.
//!
//! # Reference-like Types (with lifetime parameter)
//!
//! These types **guarantee** that the pointed-to data is valid for their lifetime `'a`.
//! This invariant enables safe implementations of traits like [`PartialEq`], [`Debug`],
//! [`Hash`], etc.
//!
//! - [`OxRef<'a>`] - read-only reference + shape, data valid for `'a`
//! - [`OxMut<'a>`] - mutable reference + shape, exclusive access for `'a`
//! - [`Ox<'a>`] - owned or borrowed value + shape
//!
//! **Safety invariant**: The constructors that take raw pointers ([`OxRef::new`],
//! [`OxMut::new`]) are `unsafe` because the caller must guarantee the pointer is
//! valid for the lifetime `'a`. Safe constructors ([`OxRef::from_ref`],
//! [`OxMut::from_mut`]) capture the lifetime from an actual Rust reference.
//!
//! # Comparison with `Peek`
//!
//! `Peek<'mem, 'facet>` in `facet-reflect` follows the same pattern:
//! - `Peek::new(&'mem T)` - safe, captures lifetime from reference
//! - `Peek::unchecked_new(PtrConst, Shape)` - unsafe, for internal use

use crate::{Facet, PtrConst, PtrMut, PtrUninit, Shape};
use alloc::boxed::Box;
use core::{marker::PhantomData, ptr::NonNull};

/// Wrapper for struct fields whose types we don't want to expose.
/// Prevents direct access while preserving layout.
#[repr(transparent)]
pub struct Opaque<T: ?Sized>(pub T);

impl<T: Default> Default for Opaque<T> {
    fn default() -> Self {
        Opaque(T::default())
    }
}

// ============================================================================
// OxPtrConst - Read-only shaped pointer (no lifetime)
// ============================================================================

/// Read-only shaped pointer for vtable use.
///
/// Bundles a pointer with its shape. No lifetime parameter - safety is
/// the caller's responsibility.
#[derive(Copy, Clone)]
pub struct OxPtrConst {
    /// The pointer to the data.
    pub(crate) ptr: PtrConst,
    /// The shape describing the type.
    pub(crate) shape: &'static Shape,
}

impl OxPtrConst {
    /// Create a new OxPtrConst from a pointer and shape.
    #[inline]
    pub const fn new(ptr: PtrConst, shape: &'static Shape) -> Self {
        Self { ptr, shape }
    }

    /// Get the underlying pointer.
    #[inline]
    pub const fn ptr(&self) -> PtrConst {
        self.ptr
    }

    /// Get the shape.
    #[inline]
    pub const fn shape(&self) -> &'static Shape {
        self.shape
    }

    /// Borrows the underlying object as a reference of type `T`.
    ///
    /// # Safety
    /// - `T` must be the actual underlying type
    /// - The memory must be valid and initialized
    #[inline]
    pub unsafe fn get<'a, T: ?Sized>(&self) -> &'a T {
        unsafe { self.ptr.get::<'a, T>() }
    }
}

// ============================================================================
// OxPtrMut - Mutable shaped pointer (no lifetime)
// ============================================================================

/// Mutable shaped pointer for vtable use.
///
/// Bundles a pointer with its shape. No lifetime parameter - safety is
/// the caller's responsibility.
#[derive(Copy, Clone)]
pub struct OxPtrMut {
    /// The pointer to the data.
    pub(crate) ptr: PtrMut,
    /// The shape describing the type.
    pub(crate) shape: &'static Shape,
}

impl OxPtrMut {
    /// Create a new OxPtrMut from a pointer and shape.
    #[inline]
    pub const fn new(ptr: PtrMut, shape: &'static Shape) -> Self {
        Self { ptr, shape }
    }

    /// Get the underlying pointer.
    #[inline]
    pub const fn ptr(&self) -> PtrMut {
        self.ptr
    }

    /// Get the shape.
    #[inline]
    pub const fn shape(&self) -> &'static Shape {
        self.shape
    }

    /// Convert to a read-only OxPtrConst.
    #[inline]
    pub const fn as_const(&self) -> OxPtrConst {
        OxPtrConst {
            ptr: self.ptr.as_const(),
            shape: self.shape,
        }
    }

    /// Convert to an uninitialized OxPtrUninit.
    #[inline]
    pub const fn as_uninit(&self) -> OxPtrUninit {
        OxPtrUninit {
            ptr: self.ptr.as_uninit(),
            shape: self.shape,
        }
    }

    /// Borrows the underlying object as a reference of type `T`.
    ///
    /// # Safety
    /// - `T` must be the actual underlying type
    /// - The memory must be valid and initialized
    #[inline]
    pub unsafe fn get<'a, T: ?Sized>(&self) -> &'a T {
        unsafe { self.ptr.get::<'a, T>() }
    }

    /// Borrows the underlying object as a mutable reference of type `T`.
    ///
    /// # Safety
    /// - `T` must be the actual underlying type
    /// - The memory must be valid and initialized
    /// - Caller must have exclusive access
    #[inline]
    pub unsafe fn as_mut<'a, T: ?Sized>(&self) -> &'a mut T {
        unsafe { self.ptr.as_mut::<'a, T>() }
    }
}

impl From<OxMut<'_>> for OxPtrMut {
    #[inline]
    fn from(ox: OxMut<'_>) -> Self {
        OxPtrMut {
            ptr: ox.ptr,
            shape: ox.shape,
        }
    }
}

// ============================================================================
// OxPtrUninit - Uninitialized shaped pointer (no lifetime)
// ============================================================================

/// Uninitialized shaped pointer for vtable use.
///
/// Bundles a pointer to uninitialized memory with its shape.
/// No lifetime parameter - safety is the caller's responsibility.
#[derive(Copy, Clone)]
pub struct OxPtrUninit {
    /// The pointer to uninitialized data.
    pub(crate) ptr: PtrUninit,
    /// The shape describing the type.
    pub(crate) shape: &'static Shape,
}

impl OxPtrUninit {
    /// Create a new OxPtrUninit from a pointer and shape.
    #[inline]
    pub const fn new(ptr: PtrUninit, shape: &'static Shape) -> Self {
        Self { ptr, shape }
    }

    /// Get the underlying pointer.
    #[inline]
    pub const fn ptr(&self) -> PtrUninit {
        self.ptr
    }

    /// Get the shape.
    #[inline]
    pub const fn shape(&self) -> &'static Shape {
        self.shape
    }

    /// Assumes the pointer is initialized and returns an `OxPtrMut`.
    ///
    /// # Safety
    /// The memory must actually be initialized.
    #[inline]
    pub const unsafe fn assume_init(self) -> OxPtrMut {
        OxPtrMut {
            ptr: unsafe { self.ptr.assume_init() },
            shape: self.shape,
        }
    }

    /// Write a value to this location and return an initialized pointer.
    ///
    /// # Safety
    /// - The pointer must be properly aligned
    /// - T must match the shape
    #[inline]
    pub unsafe fn put<T>(self, value: T) -> OxPtrMut {
        OxPtrMut {
            ptr: unsafe { self.ptr.put(value) },
            shape: self.shape,
        }
    }
}

// ============================================================================
// OxRef<'a> - Safe read-only reference with shape
// ============================================================================

/// Safe read-only reference with shape.
///
/// Unlike `OxPtrConst`, this has a lifetime parameter for borrow checking.
#[derive(Copy, Clone)]
pub struct OxRef<'a> {
    /// The pointer to the data.
    pub(crate) ptr: PtrConst,
    /// The shape describing the type.
    pub(crate) shape: &'static Shape,
    /// Phantom lifetime
    phantom: PhantomData<&'a ()>,
}

impl core::fmt::Debug for OxRef<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        unsafe { self.shape.call_debug(self.ptr, f) }.unwrap_or_else(|| {
            write!(
                f,
                "<{} @ {:p}>",
                self.shape.type_identifier,
                self.ptr.as_byte_ptr()
            )
        })
    }
}

impl core::fmt::Display for OxRef<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        unsafe { self.shape.call_display(self.ptr, f) }
            .unwrap_or_else(|| write!(f, "<{}>", self.shape.type_identifier))
    }
}

impl core::cmp::PartialEq for OxRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        if self.shape != other.shape {
            return false;
        }
        unsafe { self.shape.call_partial_eq(self.ptr, other.ptr) }.unwrap_or(false)
    }
}

impl core::cmp::PartialOrd for OxRef<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        if self.shape != other.shape {
            return None;
        }
        unsafe { self.shape.call_partial_cmp(self.ptr, other.ptr) }.flatten()
    }
}

impl core::hash::Hash for OxRef<'_> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        let mut proxy = crate::HashProxy::new(state);
        if unsafe { self.shape.call_hash(self.ptr, &mut proxy) }.is_none() {
            core::hash::Hash::hash(&self.ptr.as_byte_ptr(), state);
        }
    }
}

impl<'a> OxRef<'a> {
    /// Create an `OxRef` from an actual Rust reference.
    ///
    /// This is the safe constructor - the lifetime `'a` is captured from the reference,
    /// guaranteeing the data is valid for `'a`.
    #[inline]
    pub fn from_ref<T: Facet<'a> + ?Sized>(t: &'a T) -> Self {
        Self {
            ptr: PtrConst::new(NonNull::from(t).as_ptr()),
            shape: T::SHAPE,
            phantom: PhantomData,
        }
    }

    /// Create an `OxRef` from a raw pointer and shape.
    ///
    /// # Safety
    ///
    /// The caller must guarantee that:
    /// - The pointer is valid and points to initialized data matching `shape`
    /// - The data remains valid and immutably borrowed for the lifetime `'a`
    /// - The shape correctly describes the pointed-to type
    ///
    /// Violating these invariants will cause undefined behavior when using
    /// safe methods like [`PartialEq::eq`], [`core::fmt::Debug::fmt`], etc.
    #[inline]
    pub const unsafe fn new(ptr: PtrConst, shape: &'static Shape) -> Self {
        Self {
            ptr,
            shape,
            phantom: PhantomData,
        }
    }

    /// Get the underlying pointer.
    #[inline]
    pub const fn ptr(&self) -> PtrConst {
        self.ptr
    }

    /// Get the shape.
    #[inline]
    pub const fn shape(&self) -> &'static Shape {
        self.shape
    }

    /// Get a typed reference if the shape matches.
    ///
    /// # Safety
    /// The caller must ensure that `T` is the correct type for `expected_shape`.
    #[inline]
    pub unsafe fn get_as<T: 'static>(&self, expected_shape: &'static Shape) -> Option<&'a T> {
        if self.shape == expected_shape {
            Some(unsafe { self.ptr.get::<T>() })
        } else {
            None
        }
    }

    /// Convert to an unlifetimed OxPtrConst.
    #[inline]
    pub const fn as_ptr_const(&self) -> OxPtrConst {
        OxPtrConst {
            ptr: self.ptr,
            shape: self.shape,
        }
    }
}

impl<'a> From<OxRef<'a>> for OxPtrConst {
    #[inline]
    fn from(ox: OxRef<'a>) -> Self {
        ox.as_ptr_const()
    }
}

// ============================================================================
// OxMut<'a> - Safe mutable reference with shape
// ============================================================================

/// Safe mutable reference with shape.
///
/// Unlike `OxPtrMut`, this has a lifetime parameter for borrow checking.
#[derive(Copy, Clone)]
pub struct OxMut<'a> {
    /// The pointer to the data.
    pub(crate) ptr: PtrMut,
    /// The shape describing the type.
    pub(crate) shape: &'static Shape,
    /// Phantom lifetime
    phantom: PhantomData<&'a mut ()>,
}

impl<'a> OxMut<'a> {
    /// Create an `OxMut` from an actual Rust mutable reference.
    ///
    /// This is the safe constructor - the lifetime `'a` is captured from the reference,
    /// guaranteeing exclusive access for `'a`.
    #[inline]
    pub fn from_mut<T: Facet<'a> + ?Sized>(t: &'a mut T) -> Self {
        Self {
            ptr: PtrMut::new(NonNull::from(t).as_ptr()),
            shape: T::SHAPE,
            phantom: PhantomData,
        }
    }

    /// Create an `OxMut` from a raw pointer and shape.
    ///
    /// # Safety
    ///
    /// The caller must guarantee that:
    /// - The pointer is valid and points to initialized data matching `shape`
    /// - The caller has exclusive (mutable) access to the data for lifetime `'a`
    /// - The shape correctly describes the pointed-to type
    ///
    /// Violating these invariants will cause undefined behavior when using
    /// safe methods like [`PartialEq::eq`], [`core::fmt::Debug::fmt`], etc.
    #[inline]
    pub const unsafe fn new(ptr: PtrMut, shape: &'static Shape) -> Self {
        Self {
            ptr,
            shape,
            phantom: PhantomData,
        }
    }

    /// Get the underlying pointer.
    #[inline]
    pub const fn ptr(&self) -> PtrMut {
        self.ptr
    }

    /// Get the shape.
    #[inline]
    pub const fn shape(&self) -> &'static Shape {
        self.shape
    }

    /// Convert to an immutable OxRef.
    #[inline]
    pub const fn as_ref(&self) -> OxRef<'a> {
        // SAFETY: OxMut's invariant guarantees the data is valid for 'a,
        // so creating an OxRef with the same lifetime is safe.
        unsafe { OxRef::new(self.ptr.as_const(), self.shape) }
    }

    /// Convert to an unlifetimed OxPtrMut.
    #[inline]
    pub const fn as_ptr_mut(&self) -> OxPtrMut {
        OxPtrMut {
            ptr: self.ptr,
            shape: self.shape,
        }
    }

    /// Get a typed mutable reference if the shape matches.
    ///
    /// # Safety
    /// The caller must ensure that `T` is the correct type for `expected_shape`.
    #[inline]
    pub unsafe fn get_as_mut<T: 'static>(
        &mut self,
        expected_shape: &'static Shape,
    ) -> Option<&'a mut T> {
        if self.shape == expected_shape {
            Some(unsafe { self.ptr.as_mut::<T>() })
        } else {
            None
        }
    }
}

impl core::fmt::Debug for OxMut<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl core::fmt::Display for OxMut<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl core::cmp::PartialEq for OxMut<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref().eq(&other.as_ref())
    }
}

impl core::cmp::PartialOrd for OxMut<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.as_ref().partial_cmp(&other.as_ref())
    }
}

impl core::hash::Hash for OxMut<'_> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state)
    }
}

// ============================================================================
// Ox<'a> - Owned or borrowed value with shape
// ============================================================================

/// Type-erased value with ownership tracking — like `Cow` but for any shape.
pub enum Ox<'a> {
    /// We own this value and must drop it.
    Owned(OxMut<'a>),
    /// We're borrowing this value; someone else drops it.
    Borrowed(OxRef<'a>),
}

impl Ox<'static> {
    /// Take ownership of a boxed value.
    pub fn from_boxed<T: crate::Facet<'static>>(boxed: Box<T>) -> Self {
        let ptr = Box::into_raw(boxed);
        // SAFETY: We just created this pointer from Box::into_raw, so it's valid
        // and we have exclusive ownership (hence 'static lifetime is fine).
        Ox::Owned(unsafe { OxMut::new(PtrMut::new(ptr), T::SHAPE) })
    }

    /// Take ownership of a value by boxing it.
    #[inline]
    pub fn new<T: crate::Facet<'static>>(value: T) -> Self {
        Self::from_boxed(Box::new(value))
    }
}

impl<'a> Ox<'a> {
    /// Get an immutable view of the value.
    #[inline]
    pub const fn as_ref(&self) -> OxRef<'_> {
        match self {
            Ox::Owned(inner) => inner.as_ref(),
            Ox::Borrowed(inner) => *inner,
        }
    }

    /// Get a mutable view of the value (only if owned).
    #[inline]
    pub const fn as_mut(&mut self) -> Option<OxMut<'_>> {
        match self {
            Ox::Owned(inner) => Some(OxMut {
                ptr: inner.ptr,
                shape: inner.shape,
                phantom: PhantomData,
            }),
            Ox::Borrowed(_) => None,
        }
    }

    /// For read-only vtable operations.
    #[inline]
    pub const fn ptr_const(&self) -> PtrConst {
        match self {
            Ox::Owned(inner) => inner.ptr.as_const(),
            Ox::Borrowed(inner) => inner.ptr,
        }
    }

    /// For mutating vtable operations (only if owned).
    #[inline]
    pub const fn ptr_mut(&mut self) -> Option<PtrMut> {
        match self {
            Ox::Owned(inner) => Some(inner.ptr),
            Ox::Borrowed(_) => None,
        }
    }

    /// Get the shape.
    #[inline]
    pub const fn shape(&self) -> &'static Shape {
        match self {
            Ox::Owned(inner) => inner.shape,
            Ox::Borrowed(inner) => inner.shape,
        }
    }

    /// Get a typed reference if the shape matches.
    ///
    /// # Safety
    /// The caller must ensure that `T` is the correct type for `expected_shape`.
    #[inline]
    pub unsafe fn get_as<T: 'static>(&self, expected_shape: &'static Shape) -> Option<&T> {
        if self.shape() == expected_shape {
            match self {
                Ox::Owned(inner) => Some(unsafe { inner.ptr.get::<T>() }),
                Ox::Borrowed(inner) => Some(unsafe { inner.ptr.get::<T>() }),
            }
        } else {
            None
        }
    }
}

impl Drop for Ox<'_> {
    fn drop(&mut self) {
        if let Ox::Owned(inner) = self {
            let shape = inner.shape;
            let ptr = inner.ptr.as_mut_byte_ptr();

            // Call drop_in_place via type_ops if available
            if let Some(type_ops) = shape.type_ops {
                match type_ops {
                    crate::TypeOps::Direct(ops) => {
                        unsafe { (ops.drop_in_place)(inner.ptr.as_mut_byte_ptr() as *mut ()) };
                    }
                    crate::TypeOps::Indirect(ops) => {
                        unsafe { (ops.drop_in_place)(OxPtrMut::new(inner.ptr, shape)) };
                    }
                }
            }

            // Deallocate the memory
            if let Ok(layout) = shape.layout.sized_layout()
                && layout.size() > 0
            {
                unsafe {
                    alloc::alloc::dealloc(ptr, layout);
                }
            }
        }
    }
}

impl core::fmt::Debug for Ox<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl core::fmt::Display for Ox<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl core::cmp::PartialEq for Ox<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref().eq(&other.as_ref())
    }
}

impl core::cmp::PartialOrd for Ox<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.as_ref().partial_cmp(&other.as_ref())
    }
}

impl core::hash::Hash for Ox<'_> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state)
    }
}
