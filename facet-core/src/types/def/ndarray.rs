use super::{PtrConst, PtrMut};

use super::Shape;

/// Fields for n-dimensional array types
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct NdArrayDef {
    /// vtable for interacting with the array
    pub vtable: &'static NdArrayVTable,
    /// shape of the items in the array
    pub t: &'static Shape,
}

impl NdArrayDef {
    /// Construct a `NdArrayDef` from its vtable and element shape.
    pub const fn new(vtable: &'static NdArrayVTable, t: &'static Shape) -> Self {
        Self { vtable, t }
    }

    /// Returns the shape of the items in the array
    pub const fn t(&self) -> &'static Shape {
        self.t
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

    /// cf. [`NdArrayByteStrideFn`]
    /// Only available for types that can be accessed as a strided array
    pub byte_stride: Option<NdArrayByteStrideFn>,

    /// cf. [`NdArrayAsPtrFn`]
    /// Only available for types that can be accessed as a strided array
    pub as_ptr: Option<NdArrayAsPtrFn>,

    /// cf. [`NdArrayAsMutPtrFn`]
    /// Only available for types that can be accessed as a strided array
    pub as_mut_ptr: Option<NdArrayAsMutPtrFn>,
}
