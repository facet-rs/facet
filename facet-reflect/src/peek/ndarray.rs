use super::Peek;
use core::fmt::Debug;
use facet_core::{NdArrayDef, PtrConst};

/// Lets you read from an n-dimensional array (implements read-only [`facet_core::NdArrayVTable`] proxies)
#[derive(Clone, Copy)]
pub struct PeekNdArray<'mem, 'facet> {
    value: Peek<'mem, 'facet>,
    def: NdArrayDef,
}

impl Debug for PeekNdArray<'_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PeekNdArray").finish_non_exhaustive()
    }
}

/// Error that can occur when trying to access an n-dimensional array as strided
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StrideError {
    /// Error indicating that the array is not strided.
    NotStrided,
}

impl core::fmt::Display for StrideError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StrideError::NotStrided => {
                write!(f, "array is not strided")
            }
        }
    }
}

impl core::fmt::Debug for StrideError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StrideError::NotStrided => {
                write!(f, "StrideError::NotStrided: array is not strided")
            }
        }
    }
}
impl<'mem, 'facet> PeekNdArray<'mem, 'facet> {
    /// Creates a new peek array
    ///
    /// # Safety
    ///
    /// The caller must ensure that `def` contains valid vtable function pointers that:
    /// - Correctly implement the ndarray operations for the actual type
    /// - Do not cause undefined behavior when called
    /// - Return pointers within valid memory bounds
    /// - Match the element type specified in `def.t()`
    ///
    /// Violating these requirements can lead to memory safety issues.
    #[inline]
    pub unsafe fn new(value: Peek<'mem, 'facet>, def: NdArrayDef) -> Self {
        Self { value, def }
    }

    /// Get the number of elements in the array
    #[inline]
    pub fn count(&self) -> usize {
        unsafe { (self.def.vtable.count)(self.value.data()) }
    }

    /// Get the number of elements in the array
    #[inline]
    pub fn n_dim(&self) -> usize {
        unsafe { (self.def.vtable.n_dim)(self.value.data()) }
    }

    /// Get the i-th dimension of the array
    #[inline]
    pub fn dim(&self, i: usize) -> Option<usize> {
        unsafe { (self.def.vtable.dim)(self.value.data(), i) }
    }

    /// Get an item from the array at the specified index
    #[inline]
    pub fn get(&self, index: usize) -> Option<Peek<'mem, 'facet>> {
        let item = unsafe { (self.def.vtable.get)(self.value.data(), index)? };

        Some(unsafe { Peek::unchecked_new(item, self.def.t()) })
    }

    /// Get a pointer to the start of the array
    #[inline]
    pub fn as_ptr(&self) -> Result<PtrConst, StrideError> {
        let Some(as_ptr) = self.def.vtable.as_ptr else {
            return Err(StrideError::NotStrided);
        };
        let ptr = unsafe { as_ptr(self.value.data()) };
        Ok(ptr)
    }

    /// Get the i-th stride of the array in bytes
    #[inline]
    pub fn byte_stride(&self, i: usize) -> Result<Option<isize>, StrideError> {
        let Some(byte_stride) = self.def.vtable.byte_stride else {
            return Err(StrideError::NotStrided);
        };
        Ok(unsafe { byte_stride(self.value.data(), i) })
    }

    /// Peek value getter
    #[inline]
    pub fn value(&self) -> Peek<'mem, 'facet> {
        self.value
    }

    /// Def getter
    #[inline]
    pub fn def(&self) -> NdArrayDef {
        self.def
    }
}
