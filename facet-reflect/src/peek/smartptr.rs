use facet_core::SmartPointerDef;

use super::Peek;

/// Represents a smart pointer that can be peeked at during memory inspection.
///
/// This struct holds the value being pointed to and the definition of the smart pointer type.
pub struct PeekSmartPointer<'mem> {
    /// The value being pointed to by this smart pointer.
    #[expect(dead_code)]
    pub(crate) value: Peek<'mem>,

    /// The definition of this smart pointer type.
    pub(crate) def: SmartPointerDef,
}

impl PeekSmartPointer<'_> {
    /// Returns a reference to the smart pointer definition.
    #[must_use]
    pub fn def(&self) -> &SmartPointerDef {
        &self.def
    }
}
