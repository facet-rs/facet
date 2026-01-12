use facet_core::{ResultDef, ResultVTable};

/// Lets you read from a result (implements read-only result operations)
#[derive(Clone, Copy)]
pub struct PeekResult<'mem, 'facet> {
    /// the underlying value
    pub(crate) value: crate::Peek<'mem, 'facet>,

    /// the definition of the result
    pub(crate) def: ResultDef,
}

impl<'mem, 'facet> PeekResult<'mem, 'facet> {
    /// Returns the result definition
    #[inline(always)]
    pub const fn def(self) -> ResultDef {
        self.def
    }

    /// Returns the result vtable
    #[inline(always)]
    pub const fn vtable(self) -> &'static ResultVTable {
        self.def.vtable
    }

    /// Returns whether the result is Ok
    #[inline]
    pub fn is_ok(self) -> bool {
        unsafe { (self.vtable().is_ok)(self.value.data()) }
    }

    /// Returns whether the result is Err
    #[inline]
    pub fn is_err(self) -> bool {
        !self.is_ok()
    }

    /// Returns the Ok value as a Peek if the result is Ok, None otherwise
    #[inline]
    pub fn ok(self) -> Option<crate::Peek<'mem, 'facet>> {
        unsafe {
            (self.vtable().get_ok)(self.value.data())
                .map(|inner_data| crate::Peek::unchecked_new(inner_data, self.def.t()))
        }
    }

    /// Returns the Err value as a Peek if the result is Err, None otherwise
    #[inline]
    pub fn err(self) -> Option<crate::Peek<'mem, 'facet>> {
        unsafe {
            (self.vtable().get_err)(self.value.data())
                .map(|inner_data| crate::Peek::unchecked_new(inner_data, self.def.e()))
        }
    }
}
