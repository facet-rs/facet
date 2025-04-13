use crate::{ReflectError, ScalarType};
use facet_core::{Facet, Opaque, OpaqueUninit, Shape, ValueVTable};

use super::{HeapVal, PokeValueUninit};

/// Lets you modify an initialized value (implements read-write [`ValueVTable`] proxies)
pub struct PokeValue<'mem> {
    /// pointer to the value
    pub(crate) data: Opaque<'mem>,

    /// shape of the value
    pub(crate) shape: &'static Shape,
}

impl<'mem> PokeValue<'mem> {
    /// Shape getter
    #[inline(always)]
    pub fn shape(&self) -> &'static Shape {
        self.shape
    }

    /// Gets the vtable for the value
    #[inline(always)]
    fn vtable(&self) -> &'static ValueVTable {
        self.shape.vtable
    }

    /// Replace the current value with a new one of the same type
    ///
    /// This function replaces the existing value with a new one of type T,
    /// checking that T exactly matches the expected shape.
    pub fn replace<'src, T>(self, value: T) -> PokeValue<'mem>
    where
        T: Facet + 'src,
    {
        self.shape.assert_type::<T>();
        unsafe { self.data.replace(value) };
        self
    }

    /// Format the value using its Debug implementation
    pub fn debug_fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(debug_fn) = self.vtable().debug {
            unsafe { debug_fn(self.data.as_const(), f) }
        } else {
            f.write_str("<no debug impl>")
        }
    }

    /// Format the value using its Display implementation
    pub fn display_fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(display_fn) = self.vtable().display {
            unsafe { display_fn(self.data.as_const(), f) }
        } else {
            f.write_str("<no display impl>")
        }
    }

    /// Get the scalar type if set.
    pub fn scalar_type(&self) -> Option<ScalarType> {
        ScalarType::try_from_shape(self.shape)
    }

    /// Gets as a reference to `&T`
    pub fn get<T: Facet>(&self) -> &T {
        self.shape.assert_type::<T>();
        unsafe { self.data.get::<T>() }
    }

    /// Goes back to a partially-initialized value
    pub fn into_uninit(self) -> PokeValueUninit<'mem> {
        PokeValueUninit {
            data: OpaqueUninit::new(self.data.as_mut_byte_ptr()),
            shape: self.shape,
        }
    }
}

impl core::fmt::Display for PokeValue<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(display_fn) = self.vtable().display {
            unsafe { display_fn(self.data.as_const(), f) }
        } else {
            write!(f, "⟨{}⟩", self.shape)
        }
    }
}

impl core::fmt::Debug for PokeValue<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(debug_fn) = self.vtable().debug {
            unsafe { debug_fn(self.data.as_const(), f) }
        } else {
            write!(f, "⟨{}⟩", self.shape)
        }
    }
}

impl core::cmp::PartialEq for PokeValue<'_> {
    fn eq(&self, other: &Self) -> bool {
        if self.shape != other.shape {
            return false;
        }
        let eq_fn = match self.shape.vtable.eq {
            Some(eq_fn) => eq_fn,
            None => return false,
        };
        unsafe { eq_fn(self.data.as_const(), other.data.as_const()) }
    }
}

impl core::cmp::PartialOrd for PokeValue<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        if self.shape != other.shape {
            return None;
        }
        let partial_ord_fn = self.shape.vtable.partial_ord?;
        unsafe { partial_ord_fn(self.data.as_const(), other.data.as_const()) }
    }
}

impl core::hash::Hash for PokeValue<'_> {
    fn hash<H: core::hash::Hasher>(&self, hasher: &mut H) {
        if let Some(hash_fn) = self.shape.vtable.hash {
            let hasher_opaque = Opaque::new(hasher);
            unsafe {
                hash_fn(self.data.as_const(), hasher_opaque, |opaque, bytes| {
                    opaque.as_mut::<H>().write(bytes)
                })
            };
        } else {
            panic!("Hashing is not supported for this shape");
        }
    }
}

impl<'mem> HeapVal<PokeValue<'mem>> {
    /// Builds a value of type `U` out of this
    pub fn materialize<U: Facet>(self) -> Result<U, ReflectError> {
        if self.shape() != U::SHAPE {
            return Err(ReflectError::WrongShape {
                expected: U::SHAPE,
                actual: self.shape(),
            });
        }
        let u = unsafe { self.data.read::<U>() };
        // ok we've read from the value, now it has to be deallocated (but not dropped in place)
        // this will happen naturally as we drop
        Ok(u)
    }

    /// Builds a value of type `U` out of this
    pub fn materialize_boxed<U: Facet>(self) -> Result<Box<U>, ReflectError> {
        todo!()
    }
}

impl<'mem> Drop for PokeValue<'mem> {
    fn drop(&mut self) {
        if let Some(drop_fn) = self.shape.vtable.drop_in_place {
            unsafe { drop_fn(self.data) };
        }
    }
}
