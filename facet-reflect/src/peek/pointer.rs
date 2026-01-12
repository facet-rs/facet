use facet_core::PointerDef;

use super::Peek;

/// Represents a pointer that can be peeked at during memory inspection.
///
/// This struct holds the value being pointed to and the definition of the pointer type.
pub struct PeekPointer<'mem, 'facet> {
    /// The value being pointed to by this pointer.
    pub(crate) value: Peek<'mem, 'facet>,

    /// The definition of this pointer type.
    pub(crate) def: PointerDef,
}

impl<'mem, 'facet> PeekPointer<'mem, 'facet> {
    /// Returns a reference to the pointer definition.
    #[must_use]
    #[inline]
    pub const fn def(&self) -> &PointerDef {
        &self.def
    }

    /// Borrows the inner value of the pointer.
    ///
    /// Returns `None` if the pointer doesn't have a borrow function or pointee shape.
    #[inline]
    pub fn borrow_inner(&self) -> Option<Peek<'mem, 'facet>> {
        let borrow_fn = self.def.vtable.borrow_fn?;
        let pointee_shape = self.def.pointee()?;

        // SAFETY: We have a valid pointer and borrow_fn is provided by the vtable
        let inner_ptr = unsafe { borrow_fn(self.value.data) };

        // SAFETY: The borrow_fn returns a valid pointer to the inner value with the correct shape
        let inner_peek = unsafe { Peek::unchecked_new(inner_ptr, pointee_shape) };

        Some(inner_peek)
    }
}
