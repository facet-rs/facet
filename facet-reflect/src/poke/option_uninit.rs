/// Allows initializing an uninitialized option
pub struct PokeOptionUninit<'mem> {
    data: OpaqueUninit<'mem>,
    shape: &'static Shape,
    def: OptionDef,
}

impl<'mem> PokeOptionUninit<'mem> {
    /// Creates a new uninitialized option poke
    ///
    /// # Safety
    ///
    /// `data` must be properly aligned and sized for this shape.
    pub(crate) unsafe fn new(
        data: OpaqueUninit<'mem>,
        shape: &'static Shape,
        def: OptionDef,
    ) -> Self {
        Self { data, shape, def }
    }

    /// Returns the shape of this option
    pub fn shape(&self) -> &'static Shape {
        self.shape
    }

    /// Returns the option definition
    pub fn def(&self) -> OptionDef {
        self.def
    }

    /// Returns the option vtable
    pub fn vtable(&self) -> &'static OptionVTable {
        self.def.vtable
    }

    /// Get a reference to the underlying PokeValue
    #[inline(always)]
    pub fn into_value(self) -> crate::PokeValueUninit<'mem> {
        unsafe { crate::PokeValueUninit::new(self.data, self.shape) }
    }

    /// Initialize the option as None
    ///
    /// # Safety
    ///
    /// Caller must ensure that all safety requirements for initializing this option are met.
    pub unsafe fn init_none(self) -> PokeOption<'mem> {
        unsafe {
            let inited = (self.vtable().init_none_fn)(self.data);
            PokeOption::new(inited, self.shape, self.def)
        }
    }

    /// Initialize the option as Some, taking ownership of the given value
    ///
    /// # Safety
    ///
    /// Caller must ensure that all safety requirements for initializing this option are met
    /// and that the value type matches what the option expects.
    ///
    /// Caller must free the memory pointed to by `value` after the option is initialized,
    /// but must not drop it in place — it's been copied bitwise into the option.
    pub unsafe fn write<'a>(self, value: facet_core::OpaqueConst<'a>) -> PokeOption<'mem> {
        unsafe {
            // Initialize the option as Some
            let inited = (self.vtable().init_some_fn)(self.data, value);
            PokeOption::new(inited, self.shape, self.def)
        }
    }

    /// Initialize the option by providing a value of type `T`
    ///
    /// # Safety
    ///
    /// Caller must ensure that `T` matches the expected type of the option
    /// and that all safety requirements for initializing this option are met.
    pub unsafe fn put<T>(self, value: T) -> PokeOption<'mem> {
        let value_opaque = facet_core::OpaqueConst::new(&raw const value);
        let result = unsafe { self.write(value_opaque) };
        core::mem::forget(value);
        result
    }
}
