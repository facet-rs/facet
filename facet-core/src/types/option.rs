use super::Shape;
use crate::ptr::{PtrConst, PtrMut, PtrUninit};

/// Describes an Option — including a vtable to query and alter its state,
/// and the inner shape (the `T` in `Option<T>`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct OptionDef {
    /// vtable for interacting with the option
    pub vtable: &'static OptionVTable,

    /// shape of the inner type of the option
    pub t: fn() -> &'static Shape,
}

impl OptionDef {
    /// Returns a builder for OptionDef
    pub const fn builder() -> OptionDefBuilder {
        OptionDefBuilder::new()
    }

    /// Returns the inner type shape of the option
    pub fn t(&self) -> &'static Shape {
        (self.t)()
    }
}

/// Builder for OptionDef
pub struct OptionDefBuilder {
    vtable: Option<&'static OptionVTable>,
    t: Option<fn() -> &'static Shape>,
}

impl OptionDefBuilder {
    /// Creates a new OptionDefBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            vtable: None,
            t: None,
        }
    }

    /// Sets the vtable for the OptionDef
    pub const fn vtable(mut self, vtable: &'static OptionVTable) -> Self {
        self.vtable = Some(vtable);
        self
    }

    /// Sets the inner type shape for the OptionDef
    pub const fn t(mut self, t: fn() -> &'static Shape) -> Self {
        self.t = Some(t);
        self
    }

    /// Builds the OptionDef
    pub const fn build(self) -> OptionDef {
        OptionDef {
            vtable: self.vtable.unwrap(),
            t: self.t.unwrap(),
        }
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
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[non_exhaustive]
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
    /// Returns a builder for OptionVTable
    pub const fn builder() -> OptionVTableBuilder {
        OptionVTableBuilder::new()
    }
}

/// Builds an [`OptionVTable`]
pub struct OptionVTableBuilder {
    is_some_fn: Option<OptionIsSomeFn>,
    get_value_fn: Option<OptionGetValueFn>,
    init_some_fn: Option<OptionInitSomeFn>,
    init_none_fn: Option<OptionInitNoneFn>,
    replace_with_fn: Option<OptionReplaceWithFn>,
}

impl OptionVTableBuilder {
    /// Creates a new [`OptionVTableBuilder`] with all fields set to `None`.
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            is_some_fn: None,
            get_value_fn: None,
            init_some_fn: None,
            init_none_fn: None,
            replace_with_fn: None,
        }
    }

    /// Sets the is_some_fn field
    pub const fn is_some(mut self, f: OptionIsSomeFn) -> Self {
        self.is_some_fn = Some(f);
        self
    }

    /// Sets the get_value_fn field
    pub const fn get_value(mut self, f: OptionGetValueFn) -> Self {
        self.get_value_fn = Some(f);
        self
    }

    /// Sets the init_some_fn field
    pub const fn init_some(mut self, f: OptionInitSomeFn) -> Self {
        self.init_some_fn = Some(f);
        self
    }

    /// Sets the init_none_fn field
    pub const fn init_none(mut self, f: OptionInitNoneFn) -> Self {
        self.init_none_fn = Some(f);
        self
    }

    /// Sets the replace_with_fn field
    pub const fn replace_with(mut self, f: OptionReplaceWithFn) -> Self {
        self.replace_with_fn = Some(f);
        self
    }

    /// Builds the [`OptionVTable`] from the current state of the builder.
    ///
    /// # Panics
    ///
    /// This method will panic if any of the required fields are `None`.
    pub const fn build(self) -> OptionVTable {
        OptionVTable {
            is_some_fn: self.is_some_fn.unwrap(),
            get_value_fn: self.get_value_fn.unwrap(),
            init_some_fn: self.init_some_fn.unwrap(),
            init_none_fn: self.init_none_fn.unwrap(),
            replace_with_fn: self.replace_with_fn.unwrap(),
        }
    }
}
