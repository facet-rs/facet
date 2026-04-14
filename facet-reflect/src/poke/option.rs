use core::mem::ManuallyDrop;

use facet_core::{Facet, OptionDef, OptionVTable, PtrMut};

use crate::{ReflectError, ReflectErrorKind};

use super::Poke;

/// Lets you mutate an option (implements mutable option operations)
pub struct PokeOption<'mem, 'facet> {
    value: Poke<'mem, 'facet>,
    def: OptionDef,
}

impl<'mem, 'facet> core::fmt::Debug for PokeOption<'mem, 'facet> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PokeOption").finish_non_exhaustive()
    }
}

impl<'mem, 'facet> PokeOption<'mem, 'facet> {
    /// Creates a new poke option
    ///
    /// # Safety
    ///
    /// The caller must ensure that `def` contains valid vtable function pointers that
    /// correctly implement the option operations for the actual type, and that the
    /// inner type matches `def.t()`.
    #[inline]
    pub const unsafe fn new(value: Poke<'mem, 'facet>, def: OptionDef) -> Self {
        Self { value, def }
    }

    fn err(&self, kind: ReflectErrorKind) -> ReflectError {
        self.value.err(kind)
    }

    /// Returns the option definition
    #[inline(always)]
    pub const fn def(&self) -> OptionDef {
        self.def
    }

    /// Returns the option vtable
    #[inline(always)]
    pub const fn vtable(&self) -> &'static OptionVTable {
        self.def.vtable
    }

    /// Returns whether the option is Some
    #[inline]
    pub fn is_some(&self) -> bool {
        unsafe { (self.vtable().is_some)(self.value.data()) }
    }

    /// Returns whether the option is None
    #[inline]
    pub fn is_none(&self) -> bool {
        !self.is_some()
    }

    /// Returns the inner value as a read-only `Peek` if the option is Some, `None` otherwise.
    #[inline]
    pub fn value(&self) -> Option<crate::Peek<'_, 'facet>> {
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

    /// Returns the inner value as a mutable `Poke` if the option is Some, `None` otherwise.
    #[inline]
    pub fn value_mut(&mut self) -> Option<Poke<'_, 'facet>> {
        unsafe {
            let inner_data = (self.vtable().get_value)(self.value.data());
            if inner_data.is_null() {
                None
            } else {
                // The option is Some — compute the offset from the option's base to
                // the inner value and construct a PtrMut from the option's mutable data.
                let offset = inner_data.offset_from(self.value.data().as_byte_ptr()) as usize;
                let inner_data = self.value.data_mut().field(offset);
                Some(Poke::from_raw_parts(inner_data, self.def.t()))
            }
        }
    }

    /// Sets the option to `Some(value)`, dropping the previous value.
    pub fn set_some<T: Facet<'facet>>(&mut self, value: T) -> Result<(), ReflectError> {
        if self.def.t() != T::SHAPE {
            return Err(self.err(ReflectErrorKind::WrongShape {
                expected: self.def.t(),
                actual: T::SHAPE,
            }));
        }

        let mut value = ManuallyDrop::new(value);
        unsafe {
            (self.vtable().replace_with)(
                self.value.data_mut(),
                &mut value as *mut ManuallyDrop<T> as *mut u8,
            );
        }
        Ok(())
    }

    /// Sets the option to `None`, dropping the previous value.
    pub fn set_none(&mut self) {
        unsafe {
            (self.vtable().replace_with)(self.value.data_mut(), core::ptr::null_mut());
        }
    }

    /// Replace the option with either `Some(value)` or `None`, dropping the previous value.
    ///
    /// Pass `None` to set the option to `None`. Pass a `Poke<T>` whose shape matches the
    /// option's inner type to set it to `Some(value)`. The value is moved out of the `Poke`
    /// storage — the caller must ensure the original memory is not dropped afterwards.
    ///
    /// # Safety
    ///
    /// - If `value` is `Some(poke)`, the underlying storage for `poke` must not be dropped
    ///   after this call (its ownership has been transferred to the option).
    pub unsafe fn replace_with_raw(&mut self, value: Option<PtrMut>) {
        let ptr = match value {
            Some(p) => p.as_mut_byte_ptr(),
            None => core::ptr::null_mut(),
        };
        unsafe { (self.vtable().replace_with)(self.value.data_mut(), ptr) };
    }

    /// Converts this `PokeOption` back into a `Poke`
    #[inline]
    pub const fn into_inner(self) -> Poke<'mem, 'facet> {
        self.value
    }

    /// Returns a read-only `PeekOption` view
    #[inline]
    pub fn as_peek_option(&self) -> crate::PeekOption<'_, 'facet> {
        crate::PeekOption {
            value: self.value.as_peek(),
            def: self.def,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poke_option_is_some_is_none() {
        let mut x: Option<i32> = Some(42);
        let poke = Poke::new(&mut x);
        let opt = poke.into_option().unwrap();
        assert!(opt.is_some());
        assert!(!opt.is_none());

        let mut y: Option<i32> = None;
        let poke = Poke::new(&mut y);
        let opt = poke.into_option().unwrap();
        assert!(opt.is_none());
    }

    #[test]
    fn poke_option_set_some_then_none() {
        let mut x: Option<i32> = None;
        let poke = Poke::new(&mut x);
        let mut opt = poke.into_option().unwrap();

        opt.set_some(42i32).unwrap();
        assert_eq!(x, Some(42));

        let poke = Poke::new(&mut x);
        let mut opt = poke.into_option().unwrap();
        opt.set_none();
        assert_eq!(x, None);
    }

    #[test]
    fn poke_option_value_mut() {
        let mut x: Option<i32> = Some(1);
        let poke = Poke::new(&mut x);
        let mut opt = poke.into_option().unwrap();

        {
            let mut inner = opt.value_mut().unwrap();
            inner.set(100i32).unwrap();
        }
        assert_eq!(x, Some(100));
    }
}
