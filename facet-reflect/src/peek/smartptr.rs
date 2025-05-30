use facet_core::SmartPointerDef;

use super::Peek;

/// Represents a smart pointer that can be peeked at during memory inspection.
///
/// This struct holds the value being pointed to and the definition of the smart pointer type.
pub struct PeekSmartPointer<'mem, 'facet, 'shape> {
    /// The value being pointed to by this smart pointer.
    pub(crate) value: Peek<'mem, 'facet, 'shape>,

    /// The definition of this smart pointer type.
    pub(crate) def: SmartPointerDef<'shape>,
}

impl<'mem, 'facet, 'shape> PeekSmartPointer<'mem, 'facet, 'shape> {
    /// Returns a reference to the smart pointer definition.
    #[must_use]
    pub fn def(&self) -> &SmartPointerDef<'shape> {
        &self.def
    }

    /// Borrows the inner value of the smart pointer.
    ///
    /// Returns `None` if the smart pointer doesn't have a borrow function or pointee shape.
    pub fn borrow_inner(&self) -> Option<Peek<'mem, 'facet, 'shape>> {
        let borrow_fn = self.def.vtable.borrow_fn?;
        let pointee_shape = self.def.pointee()?;

        // SAFETY: We have a valid smart pointer and borrow_fn is provided by the vtable
        let inner_ptr = unsafe { borrow_fn(self.value.data) };

        // SAFETY: The borrow_fn returns a valid pointer to the inner value with the correct shape
        let inner_peek = unsafe { Peek::unchecked_new(inner_ptr, pointee_shape) };

        Some(inner_peek)
    }
}
