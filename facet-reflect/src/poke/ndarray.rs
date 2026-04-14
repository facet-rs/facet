use core::fmt::Debug;
use facet_core::{NdArrayDef, PtrMut};

use crate::peek::StrideError;

use super::Poke;

/// Lets you mutate an n-dimensional array (implements mutable [`facet_core::NdArrayVTable`] proxies)
pub struct PokeNdArray<'mem, 'facet> {
    value: Poke<'mem, 'facet>,
    def: NdArrayDef,
}

impl Debug for PokeNdArray<'_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PokeNdArray").finish_non_exhaustive()
    }
}

impl<'mem, 'facet> PokeNdArray<'mem, 'facet> {
    /// Creates a new poke ndarray.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `def` contains valid vtable function pointers that
    /// correctly implement the ndarray operations for the actual type, and that the
    /// element type matches `def.t()`.
    #[inline]
    pub const unsafe fn new(value: Poke<'mem, 'facet>, def: NdArrayDef) -> Self {
        Self { value, def }
    }

    /// Get the total number of elements in the array.
    #[inline]
    pub fn count(&self) -> usize {
        unsafe { (self.def.vtable.count)(self.value.data()) }
    }

    /// Get the number of dimensions.
    #[inline]
    pub fn n_dim(&self) -> usize {
        unsafe { (self.def.vtable.n_dim)(self.value.data()) }
    }

    /// Get the i-th dimension.
    #[inline]
    pub fn dim(&self, i: usize) -> Option<usize> {
        unsafe { (self.def.vtable.dim)(self.value.data(), i) }
    }

    /// Get a read-only view of the item at the given flat index.
    #[inline]
    pub fn get(&self, index: usize) -> Option<crate::Peek<'_, 'facet>> {
        let item = unsafe { (self.def.vtable.get)(self.value.data(), index)? };
        Some(unsafe { crate::Peek::unchecked_new(item, self.def.t()) })
    }

    /// Get a mutable view of the item at the given flat index.
    ///
    /// Returns `None` if the underlying ndarray doesn't provide mutable access or
    /// if the index is out of bounds.
    pub fn get_mut(&mut self, index: usize) -> Option<Poke<'_, 'facet>> {
        let get_mut_fn = self.def.vtable.get_mut?;
        let item = unsafe { get_mut_fn(self.value.data_mut(), index)? };
        Some(unsafe { Poke::from_raw_parts(item, self.def.t()) })
    }

    /// Get a mutable pointer to the start of the data buffer (if the array is strided).
    #[inline]
    pub fn as_mut_ptr(&mut self) -> Result<PtrMut, StrideError> {
        let Some(as_mut_ptr) = self.def.vtable.as_mut_ptr else {
            return Err(StrideError::NotStrided);
        };
        Ok(unsafe { as_mut_ptr(self.value.data_mut()) })
    }

    /// Get the i-th stride in bytes.
    #[inline]
    pub fn byte_stride(&self, i: usize) -> Result<Option<isize>, StrideError> {
        let Some(byte_stride) = self.def.vtable.byte_stride else {
            return Err(StrideError::NotStrided);
        };
        Ok(unsafe { byte_stride(self.value.data(), i) })
    }

    /// Def getter.
    #[inline]
    pub const fn def(&self) -> NdArrayDef {
        self.def
    }

    /// Converts this `PokeNdArray` back into a `Poke`.
    #[inline]
    pub const fn into_inner(self) -> Poke<'mem, 'facet> {
        self.value
    }

    /// Returns a read-only `PeekNdArray` view.
    #[inline]
    pub fn as_peek_ndarray(&self) -> crate::PeekNdArray<'_, 'facet> {
        unsafe { crate::PeekNdArray::new(self.value.as_peek(), self.def) }
    }
}
