use crate::ptr::{PtrConst, PtrMut};

use super::Shape;

/// Fields for n-dimensional array types
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct NdArrayDef {
    /// vtable for interacting with the array
    pub vtable: &'static NdArrayVTable,
    /// shape of the items in the array
    pub t: fn() -> &'static Shape,
}

impl NdArrayDef {
    /// Returns a builder for NdArrayDef
    pub const fn builder() -> NdArrayDefBuilder {
        NdArrayDefBuilder::new()
    }

    /// Returns the shape of the items in the array
    pub fn t(&self) -> &'static Shape {
        (self.t)()
    }
}

/// Builder for NdArrayDef
pub struct NdArrayDefBuilder {
    vtable: Option<&'static NdArrayVTable>,
    t: Option<fn() -> &'static Shape>,
}

impl NdArrayDefBuilder {
    /// Creates a new NdArrayDefBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            vtable: None,
            t: None,
        }
    }

    /// Sets the vtable for the NdArrayDef
    pub const fn vtable(mut self, vtable: &'static NdArrayVTable) -> Self {
        self.vtable = Some(vtable);
        self
    }

    /// Sets the item shape for the NdArrayDef
    pub const fn t(mut self, t: fn() -> &'static Shape) -> Self {
        self.t = Some(t);
        self
    }

    /// Builds the NdArrayDef
    pub const fn build(self) -> NdArrayDef {
        NdArrayDef {
            vtable: self.vtable.unwrap(),
            t: self.t.unwrap(),
        }
    }
}

/// Get the total count of elements in the array.
///
/// # Safety
///
/// The `array` parameter must point to aligned, initialized memory of the correct type.
pub type NdArrayCountFn = unsafe fn(array: PtrConst) -> usize;

/// Get the number of dimensions in the array.
///
/// # Safety
///
/// The `array` parameter must point to aligned, initialized memory of the correct type.
pub type NdArrayNDimFn = unsafe fn(array: PtrConst) -> usize;

/// Get the i-th dimension in the array, or `None` if the dimension index is out of bounds.
///
/// # Safety
///
/// The `array` parameter must point to aligned, initialized memory of the correct type.
pub type NdArrayDimFn = unsafe fn(array: PtrConst, i: usize) -> Option<usize>;

/// Get the i-th stride in the array in bytes, or `None` if the dimension index is out of bounds.
///
/// # Safety
///
/// The `array` parameter must point to aligned, initialized memory of the correct type.
pub type NdArrayByteStrideFn = unsafe fn(array: PtrConst, i: usize) -> Option<isize>;

/// Get pointer to the element at `index` in the array, or `None` if the
/// index is out of bounds.
///
/// The flat index is transformed into separate array indices like this:
/// ```text
///  - i0 = index % d0;
///  - i1 = index / d0 % d1;
///  - …
///  - i{n-1} = index / d0 / d1 / ... / d{n-1} % dn;
///  - remainder = index / d0 / d1 / ... / dn;
/// ```
///
/// if `remainder` is non-zero, the index is out of bounds and `None` is returned.
///
/// # Safety
///
/// The `array` parameter must point to aligned, initialized memory of the correct type.
pub type NdArrayGetFn = unsafe fn(array: PtrConst, index: usize) -> Option<PtrConst>;

/// Get mutable pointer to the element at `index` in the array, or `None` if the
/// index is out of bounds.
///
/// # Safety
///
/// The `array` parameter must point to aligned, initialized memory of the correct type.
pub type NdArrayGetMutFn = unsafe fn(array: PtrMut, index: usize) -> Option<PtrMut>;

/// Get pointer to the data buffer of the array.
///
/// # Safety
///
/// The `array` parameter must point to aligned, initialized memory of the correct type.
pub type NdArrayAsPtrFn = unsafe fn(array: PtrConst) -> PtrConst;

/// Get mutable pointer to the data buffer of the array.
///
/// # Safety
///
/// The `array` parameter must point to aligned, initialized memory of the correct type.
pub type NdArrayAsMutPtrFn = unsafe fn(array: PtrMut) -> PtrMut;

/// Virtual table for a n-dimensional array type (like `Matrix<T>`, `Tensor<T>`, etc.)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct NdArrayVTable {
    /// cf. [`NdArrayCountFn`]
    pub count: NdArrayCountFn,

    /// cf. [`NdArrayNDimFn`]
    pub n_dim: NdArrayNDimFn,

    /// cf. [`NdArrayDimFn`]
    pub dim: NdArrayDimFn,

    /// cf. [`NdArrayGetFn`]
    pub get: NdArrayGetFn,

    /// cf. [`NdArrayGetMutFn`]
    /// Only available for mutable arrays
    pub get_mut: Option<NdArrayGetMutFn>,

    /// cf. [`NdArrayStrideFn`]
    /// Only available for types that can be accessed as a strided array
    pub byte_stride: Option<NdArrayByteStrideFn>,

    /// cf. [`NdArrayAsPtrFn`]
    /// Only available for types that can be accessed as a strided array
    pub as_ptr: Option<NdArrayAsPtrFn>,

    /// cf. [`NdArrayAsMutPtrFn`]
    /// Only available for types that can be accessed as a strided array
    pub as_mut_ptr: Option<NdArrayAsMutPtrFn>,
}

impl NdArrayVTable {
    /// Returns a builder for NdArrayVTable
    pub const fn builder() -> NdArrayVTableBuilder {
        NdArrayVTableBuilder::new()
    }
}

/// Builds a [`NdArrayVTable`]
pub struct NdArrayVTableBuilder {
    count: Option<NdArrayCountFn>,
    n_dim: Option<NdArrayNDimFn>,
    dim: Option<NdArrayDimFn>,
    get: Option<NdArrayGetFn>,
    get_mut: Option<NdArrayGetMutFn>,
    byte_stride: Option<NdArrayByteStrideFn>,
    as_ptr: Option<NdArrayAsPtrFn>,
    as_mut_ptr: Option<NdArrayAsMutPtrFn>,
}

impl NdArrayVTableBuilder {
    /// Creates a new [`NdArrayVTableBuilder`] with all fields set to `None`.
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            count: None,
            n_dim: None,
            dim: None,
            get: None,
            get_mut: None,
            byte_stride: None,
            as_ptr: None,
            as_mut_ptr: None,
        }
    }

    /// Sets the `n_dim` field
    pub const fn n_dim(mut self, f: NdArrayNDimFn) -> Self {
        self.n_dim = Some(f);
        self
    }

    /// Sets the `dim` field
    pub const fn dim(mut self, f: NdArrayDimFn) -> Self {
        self.dim = Some(f);
        self
    }

    /// Sets the `get` field
    pub const fn get(mut self, f: NdArrayGetFn) -> Self {
        self.get = Some(f);
        self
    }

    /// Sets the `get_mut` field
    pub const fn get_mut(mut self, f: NdArrayGetMutFn) -> Self {
        self.get_mut = Some(f);
        self
    }

    /// Sets the `byte_stride` field
    pub const fn byte_stride(mut self, f: NdArrayByteStrideFn) -> Self {
        self.byte_stride = Some(f);
        self
    }

    /// Sets the `as_ptr` field
    pub const fn as_ptr(mut self, f: NdArrayAsPtrFn) -> Self {
        self.as_ptr = Some(f);
        self
    }

    /// Sets the `as_mut_ptr` field
    pub const fn as_mut_ptr(mut self, f: NdArrayAsMutPtrFn) -> Self {
        self.as_mut_ptr = Some(f);
        self
    }

    /// Builds the [`NdArrayVTable`] from the current state of the builder.
    ///
    /// # Panics
    ///
    /// Panic if any of the required fields (len, get, as_ptr, iter_vtable) are `None`.
    pub const fn build(self) -> NdArrayVTable {
        assert!(self.as_ptr.is_some());
        NdArrayVTable {
            count: self.count.unwrap(),
            n_dim: self.n_dim.unwrap(),
            dim: self.dim.unwrap(),
            get: self.get.unwrap(),
            get_mut: self.get_mut,
            byte_stride: self.byte_stride,
            as_ptr: self.as_ptr,
            as_mut_ptr: self.as_mut_ptr,
        }
    }
}
