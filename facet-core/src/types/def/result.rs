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
    /// Returns a builder for ResultDef
    pub const fn builder() -> ResultDefBuilder {
        ResultDefBuilder::new()
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

/// Builder for ResultDef
pub struct ResultDefBuilder {
    vtable: Option<&'static ResultVTable>,
    t: Option<&'static Shape>,
    e: Option<&'static Shape>,
}

impl ResultDefBuilder {
    /// Creates a new ResultDefBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            vtable: None,
            t: None,
            e: None,
        }
    }

    /// Sets the vtable for the ResultDef
    pub const fn vtable(mut self, vtable: &'static ResultVTable) -> Self {
        self.vtable = Some(vtable);
        self
    }

    /// Sets the Ok type shape for the ResultDef
    pub const fn t(mut self, t: &'static Shape) -> Self {
        self.t = Some(t);
        self
    }

    /// Sets the Err type shape for the ResultDef
    pub const fn e(mut self, e: &'static Shape) -> Self {
        self.e = Some(e);
        self
    }

    /// Builds the ResultDef
    pub const fn build(self) -> ResultDef {
        ResultDef {
            vtable: self.vtable.unwrap(),
            t: self.t.unwrap(),
            e: self.e.unwrap(),
        }
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
    /// Returns a builder for ResultVTable
    pub const fn builder() -> ResultVTableBuilder {
        ResultVTableBuilder::new()
    }
}

/// Builds a [`ResultVTable`]
pub struct ResultVTableBuilder {
    is_ok_fn: Option<ResultIsOkFn>,
    get_ok_fn: Option<ResultGetOkFn>,
    get_err_fn: Option<ResultGetErrFn>,
    init_ok_fn: Option<ResultInitOkFn>,
    init_err_fn: Option<ResultInitErrFn>,
}

impl ResultVTableBuilder {
    /// Creates a new [`ResultVTableBuilder`] with all fields set to `None`.
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            is_ok_fn: None,
            get_ok_fn: None,
            get_err_fn: None,
            init_ok_fn: None,
            init_err_fn: None,
        }
    }

    /// Sets the is_ok_fn field
    pub const fn is_ok(mut self, f: ResultIsOkFn) -> Self {
        self.is_ok_fn = Some(f);
        self
    }

    /// Sets the get_ok_fn field
    pub const fn get_ok(mut self, f: ResultGetOkFn) -> Self {
        self.get_ok_fn = Some(f);
        self
    }

    /// Sets the get_err_fn field
    pub const fn get_err(mut self, f: ResultGetErrFn) -> Self {
        self.get_err_fn = Some(f);
        self
    }

    /// Sets the init_ok_fn field
    pub const fn init_ok(mut self, f: ResultInitOkFn) -> Self {
        self.init_ok_fn = Some(f);
        self
    }

    /// Sets the init_err_fn field
    pub const fn init_err(mut self, f: ResultInitErrFn) -> Self {
        self.init_err_fn = Some(f);
        self
    }

    /// Builds the [`ResultVTable`] from the current state of the builder.
    ///
    /// # Panics
    ///
    /// This method will panic if any of the required fields are `None`.
    pub const fn build(self) -> ResultVTable {
        ResultVTable {
            is_ok_fn: self.is_ok_fn.unwrap(),
            get_ok_fn: self.get_ok_fn.unwrap(),
            get_err_fn: self.get_err_fn.unwrap(),
            init_ok_fn: self.init_ok_fn.unwrap(),
            init_err_fn: self.init_err_fn.unwrap(),
        }
    }
}
