use core::mem::ManuallyDrop;

use facet_core::{Facet, PtrUninit, ResultDef, ResultVTable};

use crate::{ReflectError, ReflectErrorKind};

use super::Poke;

/// Lets you mutate a result (implements mutable result operations)
pub struct PokeResult<'mem, 'facet> {
    value: Poke<'mem, 'facet>,
    def: ResultDef,
}

impl<'mem, 'facet> core::fmt::Debug for PokeResult<'mem, 'facet> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PokeResult").finish_non_exhaustive()
    }
}

impl<'mem, 'facet> PokeResult<'mem, 'facet> {
    /// Creates a new poke result
    ///
    /// # Safety
    ///
    /// The caller must ensure that `def` contains valid vtable function pointers that
    /// correctly implement the result operations for the actual type, and that the
    /// ok/err types match `def.t()` / `def.e()`.
    #[inline]
    pub const unsafe fn new(value: Poke<'mem, 'facet>, def: ResultDef) -> Self {
        Self { value, def }
    }

    fn err_reflect(&self, kind: ReflectErrorKind) -> ReflectError {
        self.value.err(kind)
    }

    /// Returns the result definition
    #[inline(always)]
    pub const fn def(&self) -> ResultDef {
        self.def
    }

    /// Returns the result vtable
    #[inline(always)]
    pub const fn vtable(&self) -> &'static ResultVTable {
        self.def.vtable
    }

    /// Returns whether the result is Ok
    #[inline]
    pub fn is_ok(&self) -> bool {
        unsafe { (self.vtable().is_ok)(self.value.data()) }
    }

    /// Returns whether the result is Err
    #[inline]
    pub fn is_err(&self) -> bool {
        !self.is_ok()
    }

    /// Returns the Ok value as a read-only `Peek` if the result is Ok, `None` otherwise.
    #[inline]
    pub fn ok(&self) -> Option<crate::Peek<'_, 'facet>> {
        unsafe {
            let inner_data = (self.vtable().get_ok)(self.value.data());
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

    /// Returns the Err value as a read-only `Peek` if the result is Err, `None` otherwise.
    #[inline]
    pub fn err(&self) -> Option<crate::Peek<'_, 'facet>> {
        unsafe {
            let inner_data = (self.vtable().get_err)(self.value.data());
            if inner_data.is_null() {
                None
            } else {
                Some(crate::Peek::unchecked_new(
                    facet_core::PtrConst::new_sized(inner_data),
                    self.def.e(),
                ))
            }
        }
    }

    /// Returns the Ok value as a mutable `Poke` if the result is Ok, `None` otherwise.
    #[inline]
    pub fn ok_mut(&mut self) -> Option<Poke<'_, 'facet>> {
        unsafe {
            let inner_data = (self.vtable().get_ok)(self.value.data());
            if inner_data.is_null() {
                return None;
            }
            let offset = inner_data.offset_from(self.value.data().as_byte_ptr()) as usize;
            let inner_data = self.value.data_mut().field(offset);
            Some(Poke::from_raw_parts(inner_data, self.def.t()))
        }
    }

    /// Returns the Err value as a mutable `Poke` if the result is Err, `None` otherwise.
    #[inline]
    pub fn err_mut(&mut self) -> Option<Poke<'_, 'facet>> {
        unsafe {
            let inner_data = (self.vtable().get_err)(self.value.data());
            if inner_data.is_null() {
                return None;
            }
            let offset = inner_data.offset_from(self.value.data().as_byte_ptr()) as usize;
            let inner_data = self.value.data_mut().field(offset);
            Some(Poke::from_raw_parts(inner_data, self.def.e()))
        }
    }

    /// Sets the result to `Ok(value)`, dropping the previous value.
    pub fn set_ok<T: Facet<'facet>>(&mut self, value: T) -> Result<(), ReflectError> {
        if self.def.t() != T::SHAPE {
            return Err(self.err_reflect(ReflectErrorKind::WrongShape {
                expected: self.def.t(),
                actual: T::SHAPE,
            }));
        }

        let mut value = ManuallyDrop::new(value);
        unsafe {
            // Drop the old value, then re-initialize in place.
            self.value.shape.call_drop_in_place(self.value.data_mut());
            let uninit = PtrUninit::new(self.value.data_mut().as_mut_byte_ptr());
            let value_ptr = facet_core::PtrMut::new(&mut value as *mut ManuallyDrop<T> as *mut u8);
            (self.vtable().init_ok)(uninit, value_ptr);
        }
        Ok(())
    }

    /// Sets the result to `Err(value)`, dropping the previous value.
    pub fn set_err<E: Facet<'facet>>(&mut self, value: E) -> Result<(), ReflectError> {
        if self.def.e() != E::SHAPE {
            return Err(self.err_reflect(ReflectErrorKind::WrongShape {
                expected: self.def.e(),
                actual: E::SHAPE,
            }));
        }

        let mut value = ManuallyDrop::new(value);
        unsafe {
            self.value.shape.call_drop_in_place(self.value.data_mut());
            let uninit = PtrUninit::new(self.value.data_mut().as_mut_byte_ptr());
            let value_ptr = facet_core::PtrMut::new(&mut value as *mut ManuallyDrop<E> as *mut u8);
            (self.vtable().init_err)(uninit, value_ptr);
        }
        Ok(())
    }

    /// Converts this `PokeResult` back into a `Poke`
    #[inline]
    pub const fn into_inner(self) -> Poke<'mem, 'facet> {
        self.value
    }

    /// Returns a read-only `PeekResult` view
    #[inline]
    pub fn as_peek_result(&self) -> crate::PeekResult<'_, 'facet> {
        crate::PeekResult {
            value: self.value.as_peek(),
            def: self.def,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poke_result_is_ok_is_err() {
        let mut x: Result<i32, String> = Ok(42);
        let poke = Poke::new(&mut x);
        let res = poke.into_result().unwrap();
        assert!(res.is_ok());
        assert!(!res.is_err());

        let mut y: Result<i32, String> = Err(String::from("nope"));
        let poke = Poke::new(&mut y);
        let res = poke.into_result().unwrap();
        assert!(res.is_err());
    }

    #[test]
    fn poke_result_set_ok_then_err() {
        let mut x: Result<i32, String> = Err(String::from("initial"));
        let poke = Poke::new(&mut x);
        let mut res = poke.into_result().unwrap();
        res.set_ok(7i32).unwrap();
        assert_eq!(x, Ok(7));

        let poke = Poke::new(&mut x);
        let mut res = poke.into_result().unwrap();
        res.set_err(String::from("oops")).unwrap();
        assert_eq!(x, Err(String::from("oops")));
    }

    #[test]
    fn poke_result_ok_mut() {
        let mut x: Result<i32, String> = Ok(1);
        let poke = Poke::new(&mut x);
        let mut res = poke.into_result().unwrap();

        {
            let mut inner = res.ok_mut().unwrap();
            inner.set(123i32).unwrap();
        }
        assert_eq!(x, Ok(123));
    }
}
