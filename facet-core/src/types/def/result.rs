use super::Shape;
use crate::ptr::{PtrConst, PtrMut, PtrUninit};

/// Describes a Result — including a vtable to query and alter its state,
/// and the inner shapes (the `T` and `E` in `Result<T, E>`).
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct ResultDef {
    /// vtable for interacting with the result
    pub vtable: &'static ResultVTable,

    /// shape of the Ok type
    pub t: &'static Shape,

    /// shape of the Err type
    pub e: &'static Shape,
}

impl ResultDef {
    /// Construct a `ResultDef` from its vtable and ok/err shapes.
    pub const fn new(vtable: &'static ResultVTable, t: &'static Shape, e: &'static Shape) -> Self {
        Self { vtable, t, e }
    }

    /// Returns the Ok type shape of the result
    pub const fn t(&self) -> &'static Shape {
        self.t
    }

    /// Returns the Err type shape of the result
    pub const fn e(&self) -> &'static Shape {
        self.e
    }
}

/// Check if a result is Ok
///
/// # Safety
///
/// The `result` parameter must point to aligned, initialized memory of the correct type.
pub type ResultIsOkFn = for<'result> unsafe fn(result: PtrConst<'result>) -> bool;

/// Get the Ok value contained in a result, if present
///
/// # Safety
///
/// The `result` parameter must point to aligned, initialized memory of the correct type.
pub type ResultGetOkFn =
    for<'result> unsafe fn(result: PtrConst<'result>) -> Option<PtrConst<'result>>;

/// Get the Err value contained in a result, if present
///
/// # Safety
///
/// The `result` parameter must point to aligned, initialized memory of the correct type.
pub type ResultGetErrFn =
    for<'result> unsafe fn(result: PtrConst<'result>) -> Option<PtrConst<'result>>;

/// Initialize a result with Ok(value)
///
/// # Safety
///
/// The `result` parameter must point to uninitialized memory of sufficient size.
/// The function must properly initialize the memory.
/// `value` is moved out of (with [`core::ptr::read`]) — it should be deallocated afterwards (e.g.
/// with [`core::mem::forget`]) but NOT dropped.
pub type ResultInitOkFn =
    for<'result> unsafe fn(result: PtrUninit<'result>, value: PtrConst<'_>) -> PtrMut<'result>;

/// Initialize a result with Err(value)
///
/// # Safety
///
/// The `result` parameter must point to uninitialized memory of sufficient size.
/// The function must properly initialize the memory.
/// `value` is moved out of (with [`core::ptr::read`]) — it should be deallocated afterwards (e.g.
/// with [`core::mem::forget`]) but NOT dropped.
pub type ResultInitErrFn =
    for<'result> unsafe fn(result: PtrUninit<'result>, value: PtrConst<'_>) -> PtrMut<'result>;

/// Virtual table for `Result<T, E>`
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct ResultVTable {
    /// cf. [`ResultIsOkFn`]
    pub is_ok_fn: ResultIsOkFn,

    /// cf. [`ResultGetOkFn`]
    pub get_ok_fn: ResultGetOkFn,

    /// cf. [`ResultGetErrFn`]
    pub get_err_fn: ResultGetErrFn,

    /// cf. [`ResultInitOkFn`]
    pub init_ok_fn: ResultInitOkFn,

    /// cf. [`ResultInitErrFn`]
    pub init_err_fn: ResultInitErrFn,
}

impl ResultVTable {
    /// Const ctor for result vtable; all hooks required.
    pub const fn new(
        is_ok_fn: ResultIsOkFn,
        get_ok_fn: ResultGetOkFn,
        get_err_fn: ResultGetErrFn,
        init_ok_fn: ResultInitOkFn,
        init_err_fn: ResultInitErrFn,
    ) -> Self {
        Self {
            is_ok_fn,
            get_ok_fn,
            get_err_fn,
            init_ok_fn,
            init_err_fn,
        }
    }
}
