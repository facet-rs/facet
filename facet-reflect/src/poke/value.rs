use crate::ScalarType;
use facet_core::{Facet, Opaque, OpaqueUninit, Shape, ValueVTable};

use super::PokeValueUninit;

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
    #[allow(clippy::should_implement_trait)]
    pub fn as_ref<T: Facet>(&self) -> &T {
        self.shape.assert_type::<T>();
        unsafe { self.data.as_ref::<T>() }
    }

    /// Attempt to clone this value. Returns None if the value is not cloneable.
    #[allow(clippy::should_implement_trait)]
    pub fn clone(&self) -> Option<Self> {
        let clone_fn = self.vtable().clone_into?;
        let uninit_data = self.shape.allocate();
        // Create an opaque const reference to the source data for cloning
        let source_data = self.data.as_const();
        // Call clone_fn to actually clone the data
        let initialized_data = unsafe { clone_fn(source_data, uninit_data) };
        Some(PokeValue {
            data: initialized_data,
            shape: self.shape,
        })
    }

    /// Deallocates the value's data, without dropping it
    pub(crate) fn dealloc(self) {
        unsafe { alloc::alloc::dealloc(self.data.as_mut(), self.shape.layout) };
    }

    /// Moves out the value's data as a T.
    pub(crate) fn move_out<T: Facet>(self) -> T {
        self.shape.assert_type::<T>();
        let t = unsafe { self.data.read::<T>() };
        self.dealloc();
        t
    }

    /// Goes back to a partially-initialized value
    pub fn into_uninit(self) -> PokeValueUninit<'mem> {
        PokeValueUninit {
            data: OpaqueUninit::new(self.data.as_mut_byte_ptr()),
            shape: self.shape,
        }
    }
}
