use super::{PtrConst, PtrMut, PtrUninit, Shape};

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
pub type OptionIsSomeFn = unsafe fn(option: PtrConst) -> bool;

/// Get the value contained in an option, if present
///
/// # Safety
///
/// The `option` parameter must point to aligned, initialized memory of the correct type.
pub type OptionGetValueFn = unsafe fn(option: PtrConst) -> Option<PtrConst>;

/// Initialize an option with Some(value)
///
/// # Safety
///
/// The `option` parameter must point to uninitialized memory of sufficient size.
/// The function must properly initialize the memory.
/// `value` is moved out of (with [`core::ptr::read`]) — it should be deallocated afterwards (e.g.
/// with [`core::mem::forget`]) but NOT dropped.
/// Note: `value` must be PtrMut (not PtrConst) because ownership is transferred and the value
/// may be dropped later, which requires mutable access.
pub type OptionInitSomeFn = unsafe fn(option: PtrUninit, value: PtrMut) -> PtrMut;

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
/// Note: `value` must be PtrMut (not PtrConst) because ownership is transferred and the value
/// may be dropped later, which requires mutable access.
pub type OptionReplaceWithFn = unsafe fn(option: PtrMut, value: Option<PtrMut>);

vtable_def! {
    /// Virtual table for `Option<T>`
    #[derive(Clone, Copy, Debug)]
    #[repr(C)]
    pub struct OptionVTable + OptionVTableBuilder {
        /// cf. [`OptionIsSomeFn`]
        pub is_some: OptionIsSomeFn,
        /// cf. [`OptionGetValueFn`]
        pub get_value: OptionGetValueFn,
        /// cf. [`OptionInitSomeFn`]
        pub init_some: OptionInitSomeFn,
        /// cf. [`OptionInitNoneFn`]
        pub init_none: OptionInitNoneFn,
        /// cf. [`OptionReplaceWithFn`]
        pub replace_with: OptionReplaceWithFn,
    }
}
