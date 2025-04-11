use facet_core::{OptionDef, OptionVTable, Shape};

/// Lets you read from an option (implements read-only option operations)
#[derive(Clone, Copy)]
pub struct PeekOption<'mem> {
    value: crate::PeekValue<'mem>,
    def: OptionDef,
}

/// Returns the option definition if the shape represents an option, None otherwise
pub fn peek_option(shape: &'static Shape) -> Option<OptionDef> {
    match shape.def {
        facet_core::Def::Option(option_def) => Some(option_def),
        _ => None,
    }
}

impl<'mem> core::ops::Deref for PeekOption<'mem> {
    type Target = crate::PeekValue<'mem>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'mem> PeekOption<'mem> {
    /// Create a new peek option
    pub(crate) fn new(value: crate::PeekValue<'mem>, def: OptionDef) -> Self {
        Self { value, def }
    }

    /// Returns the option definition
    #[inline(always)]
    pub fn def(self) -> OptionDef {
        self.def
    }

    /// Returns the option vtable
    #[inline(always)]
    pub fn vtable(self) -> &'static OptionVTable {
        self.def.vtable
    }

    /// Returns whether the option is Some
    #[inline]
    pub fn is_some(self) -> bool {
        unsafe { (self.vtable().is_some_fn)(self.value.data()) }
    }

    /// Returns whether the option is None
    #[inline]
    pub fn is_none(self) -> bool {
        !self.is_some()
    }

    /// Returns the inner value as a Peek if the option is Some, None otherwise
    pub fn value(self) -> Option<crate::Peek<'mem>> {
        unsafe {
            (self.vtable().get_value_fn)(self.value.data())
                .map(|inner_data| crate::Peek::unchecked_new(inner_data, self.def.t))
        }
    }
}
