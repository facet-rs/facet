use core::mem::ManuallyDrop;

use facet_core::{Facet, OptionDef, OptionVTable};

use crate::{HeapValue, ReflectError, ReflectErrorKind};

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

    /// Type-erased [`set_some`](Self::set_some).
    ///
    /// Accepts a [`HeapValue`] whose shape must match the option's inner type. The value is
    /// moved out of the HeapValue into the option; the HeapValue's backing memory is freed
    /// without running drop (the vtable has already consumed the value via `ptr::read`).
    ///
    /// Use this when you hold a reflection-built value and can't produce a concrete `T`.
    pub fn set_some_from_heap<const BORROW: bool>(
        &mut self,
        value: HeapValue<'facet, BORROW>,
    ) -> Result<(), ReflectError> {
        if self.def.t() != value.shape() {
            return Err(self.err(ReflectErrorKind::WrongShape {
                expected: self.def.t(),
                actual: value.shape(),
            }));
        }

        let mut value = value;
        let guard = value
            .guard
            .take()
            .expect("HeapValue guard was already taken");
        unsafe {
            (self.vtable().replace_with)(self.value.data_mut(), guard.ptr.as_ptr());
        }
        drop(guard);
        Ok(())
    }

    /// Sets the option to `None`, dropping the previous value.
    pub fn set_none(&mut self) {
        unsafe {
            (self.vtable().replace_with)(self.value.data_mut(), core::ptr::null_mut());
        }
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

    #[test]
    fn poke_option_set_some_from_heap() {
        let mut x: Option<i32> = None;
        let poke = Poke::new(&mut x);
        let mut opt = poke.into_option().unwrap();

        let hv = crate::Partial::alloc::<i32>()
            .unwrap()
            .set(7i32)
            .unwrap()
            .build()
            .unwrap();
        opt.set_some_from_heap(hv).unwrap();
        assert_eq!(x, Some(7));
    }

    #[test]
    fn poke_option_set_some_from_heap_wrong_shape_fails() {
        let mut x: Option<i32> = None;
        let poke = Poke::new(&mut x);
        let mut opt = poke.into_option().unwrap();

        let hv = crate::Partial::alloc::<u32>()
            .unwrap()
            .set(7u32)
            .unwrap()
            .build()
            .unwrap();
        let res = opt.set_some_from_heap(hv);
        assert!(matches!(
            res,
            Err(ref err) if matches!(err.kind, ReflectErrorKind::WrongShape { .. })
        ));
    }
}
