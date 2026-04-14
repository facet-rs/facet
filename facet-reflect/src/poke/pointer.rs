use facet_core::PointerDef;

use super::Poke;

/// Represents a pointer that can be mutated through reflection.
///
/// Note that the pointer vtable currently only exposes read-only access to the pointee
/// (via [`borrow_inner`](Self::borrow_inner)); this is mostly here for symmetry with
/// [`PeekPointer`](crate::PeekPointer).
pub struct PokePointer<'mem, 'facet> {
    pub(crate) value: Poke<'mem, 'facet>,
    pub(crate) def: PointerDef,
}

impl<'mem, 'facet> core::fmt::Debug for PokePointer<'mem, 'facet> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PokePointer").finish_non_exhaustive()
    }
}

impl<'mem, 'facet> PokePointer<'mem, 'facet> {
    /// Returns a reference to the pointer definition.
    #[must_use]
    #[inline]
    pub const fn def(&self) -> &PointerDef {
        &self.def
    }

    /// Borrows the inner value of the pointer as a read-only `Peek`.
    ///
    /// Returns `None` if the pointer doesn't have a borrow function or pointee shape.
    #[inline]
    pub fn borrow_inner(&self) -> Option<crate::Peek<'_, 'facet>> {
        let borrow_fn = self.def.vtable.borrow_fn?;
        let pointee_shape = self.def.pointee()?;
        let inner_ptr = unsafe { borrow_fn(self.value.data()) };
        Some(unsafe { crate::Peek::unchecked_new(inner_ptr, pointee_shape) })
    }

    /// Converts this back into the underlying `Poke`.
    #[inline]
    pub const fn into_inner(self) -> Poke<'mem, 'facet> {
        self.value
    }

    /// Returns a read-only `PeekPointer` view.
    #[inline]
    pub fn as_peek_pointer(&self) -> crate::PeekPointer<'_, 'facet> {
        crate::PeekPointer {
            value: self.value.as_peek(),
            def: self.def,
        }
    }
}
