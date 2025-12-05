use super::Shape;
use crate::ptr::{PtrConst, PtrMut, PtrUninit};

/// Describes an Option — including a vtable to query and alter its state,
/// and the inner shape (the `T` in `Option<T>`).
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct OptionDef {
    /// vtable for interacting with the option
    pub vtable: &'static OptionVTable,

    /// shape of the inner type of the option
    pub t: &'static Shape,
}

impl OptionDef {
    /// Const ctor.
    pub const fn new(vtable: &'static OptionVTable, t: &'static Shape) -> Self {
        Self { vtable, t }
    }

    /// Returns the inner type shape of the option
    pub const fn t(&self) -> &'static Shape {
        self.t
    }
}

/// Check if an option contains a value
///
/// # Safety
///
/// The `option` parameter must point to aligned, initialized memory of the correct type.
pub type OptionIsSomeFn = for<'option> unsafe fn(option: PtrConst<'option>) -> bool;

/// Get the value contained in an option, if present
///
/// # Safety
///
/// The `option` parameter must point to aligned, initialized memory of the correct type.
pub type OptionGetValueFn =
    for<'option> unsafe fn(option: PtrConst<'option>) -> Option<PtrConst<'option>>;

/// Initialize an option with Some(value)
///
/// # Safety
///
/// The `option` parameter must point to uninitialized memory of sufficient size.
/// The function must properly initialize the memory.
/// `value` is moved out of (with [`core::ptr::read`]) — it should be deallocated afterwards (e.g.
/// with [`core::mem::forget`]) but NOT dropped.
pub type OptionInitSomeFn =
    for<'option> unsafe fn(option: PtrUninit<'option>, value: PtrConst<'_>) -> PtrMut<'option>;

/// Initialize an option with None
///
/// # Safety
///
/// The `option` parameter must point to uninitialized memory of sufficient size.
/// The function must properly initialize the memory.
pub type OptionInitNoneFn = unsafe fn(option: PtrUninit) -> PtrMut;

/// Replace an existing option with a new value
///
/// # Safety
///
/// The `option` parameter must point to aligned, initialized memory of the correct type.
/// The old value will be dropped.
/// If replacing with Some, `value` is moved out of (with [`core::ptr::read`]) —
/// it should be deallocated afterwards but NOT dropped.
pub type OptionReplaceWithFn =
    for<'option> unsafe fn(option: PtrMut<'option>, value: Option<PtrConst<'_>>);

/// Virtual table for `Option<T>`
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct OptionVTable {
    /// cf. [`OptionIsSomeFn`]
    pub is_some_fn: OptionIsSomeFn,

    /// cf. [`OptionGetValueFn`]
    pub get_value_fn: OptionGetValueFn,

    /// cf. [`OptionInitSomeFn`]
    pub init_some_fn: OptionInitSomeFn,

    /// cf. [`OptionInitNoneFn`]
    pub init_none_fn: OptionInitNoneFn,

    /// cf. [`OptionReplaceWithFn`]
    pub replace_with_fn: OptionReplaceWithFn,
}

impl OptionVTable {
    /// Const ctor; all functions required.
    pub const fn new(
        is_some_fn: OptionIsSomeFn,
        get_value_fn: OptionGetValueFn,
        init_some_fn: OptionInitSomeFn,
        init_none_fn: OptionInitNoneFn,
        replace_with_fn: OptionReplaceWithFn,
    ) -> Self {
        Self {
            is_some_fn,
            get_value_fn,
            init_some_fn,
            init_none_fn,
            replace_with_fn,
        }
    }
}
