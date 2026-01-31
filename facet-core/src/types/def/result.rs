use super::{PtrConst, PtrMut, PtrUninit, Shape};

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
pub type ResultIsOkFn = unsafe fn(result: PtrConst) -> bool;

/// Get the Ok value contained in a result, if present
///
/// # Safety
///
/// The `result` parameter must point to aligned, initialized memory of the correct type.
pub type ResultGetOkFn = unsafe fn(result: PtrConst) -> Option<PtrConst>;

/// Get the Err value contained in a result, if present
///
/// # Safety
///
/// The `result` parameter must point to aligned, initialized memory of the correct type.
pub type ResultGetErrFn = unsafe fn(result: PtrConst) -> Option<PtrConst>;

/// Initialize a result with Ok(value)
///
/// # Safety
///
/// The `result` parameter must point to uninitialized memory of sufficient size.
/// The function must properly initialize the memory.
/// `value` is moved out of (with [`core::ptr::read`]) — it should be deallocated afterwards (e.g.
/// with [`core::mem::forget`]) but NOT dropped.
/// Note: `value` must be PtrMut (not PtrConst) because ownership is transferred and the value
/// may be dropped later, which requires mutable access.
pub type ResultInitOkFn = unsafe fn(result: PtrUninit, value: PtrMut) -> PtrMut;

/// Initialize a result with Err(value)
///
/// # Safety
///
/// The `result` parameter must point to uninitialized memory of sufficient size.
/// The function must properly initialize the memory.
/// `value` is moved out of (with [`core::ptr::read`]) — it should be deallocated afterwards (e.g.
/// with [`core::mem::forget`]) but NOT dropped.
/// Note: `value` must be PtrMut (not PtrConst) because ownership is transferred and the value
/// may be dropped later, which requires mutable access.
pub type ResultInitErrFn = unsafe fn(result: PtrUninit, value: PtrMut) -> PtrMut;

vtable_def! {
    /// Virtual table for `Result<T, E>`
    #[derive(Clone, Copy, Debug)]
    #[repr(C)]
    pub struct ResultVTable + ResultVTableBuilder {
        /// cf. [`ResultIsOkFn`]
        pub is_ok: ResultIsOkFn,
        /// cf. [`ResultGetOkFn`]
        pub get_ok: ResultGetOkFn,
        /// cf. [`ResultGetErrFn`]
        pub get_err: ResultGetErrFn,
        /// cf. [`ResultInitOkFn`]
        pub init_ok: ResultInitOkFn,
        /// cf. [`ResultInitErrFn`]
        pub init_err: ResultInitErrFn,
    }
}
