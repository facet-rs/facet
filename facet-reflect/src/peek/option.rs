use facet_core::{OptionDef, OptionVTable};

/// Lets you read from an option (implements read-only option operations)
#[derive(Clone, Copy)]
pub struct PeekOption<'mem, 'facet> {
    /// the underlying value
    pub(crate) value: crate::Peek<'mem, 'facet>,

    /// the definition of the option
    pub(crate) def: OptionDef,
}

impl<'mem, 'facet> PeekOption<'mem, 'facet> {
    /// Returns the option definition
    #[inline(always)]
    pub const fn def(self) -> OptionDef {
        self.def
    }

    /// Returns the option vtable
    #[inline(always)]
    pub const fn vtable(self) -> &'static OptionVTable {
        self.def.vtable
    }

    /// Returns whether the option is Some
    #[inline]
    pub fn is_some(self) -> bool {
        unsafe { (self.vtable().is_some)(self.value.data()) }
    }

    /// Returns whether the option is None
    #[inline]
    pub fn is_none(self) -> bool {
        !self.is_some()
    }

    /// Returns the inner value as a Peek if the option is Some, None otherwise
    #[inline]
    pub fn value(self) -> Option<crate::Peek<'mem, 'facet>> {
        unsafe {
            let inner_data = (self.vtable().get_value)(self.value.data());
            if inner_data.is_null() {
                None
            } else {
                Some(crate::Peek::unchecked_new(
                    facet_core::PtrConst::new_sized(inner_data),
                    self.def.t(),
                ))
            }
        }
    }
}
