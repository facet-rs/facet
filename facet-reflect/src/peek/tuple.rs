use core::fmt::Debug;
use facet_core::TupleType;

use super::{FieldIter, Peek};

/// Field index and associated peek value
pub type TupleField<'mem, 'facet, 'shape> = (usize, Peek<'mem, 'facet, 'shape>);

/// Lets you read from a tuple
#[derive(Clone, Copy)]
pub struct PeekTuple<'mem, 'facet, 'shape> {
    /// Original peek value
    pub(crate) value: Peek<'mem, 'facet, 'shape>,
    /// Tuple type information
    pub(crate) ty: TupleType<'shape>,
}

impl Debug for PeekTuple<'_, '_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PeekTuple")
            .field("type", &self.ty)
            .finish_non_exhaustive()
    }
}

impl<'mem, 'facet, 'shape> PeekTuple<'mem, 'facet, 'shape> {
    /// Get the number of fields in this tuple
    pub fn len(&self) -> usize {
        self.ty.fields.len()
    }

    /// Returns true if this tuple has no fields
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Access a field by index
    pub fn field(&self, index: usize) -> Option<Peek<'mem, 'facet, 'shape>> {
        if index >= self.len() {
            return None;
        }

        let field = &self.ty.fields[index];
        // We can safely use field operations here since this is within facet-reflect
        // which is allowed to use unsafe code
        let field_ptr = unsafe { self.value.data().field(field.offset) };
        let field_peek = unsafe { Peek::unchecked_new(field_ptr, field.shape) };

        Some(field_peek)
    }

    /// Iterate over all fields
    pub fn fields(&self) -> FieldIter<'mem, 'facet, 'shape> {
        FieldIter::new_tuple(*self)
    }

    /// Type information
    pub fn ty(&self) -> TupleType<'shape> {
        self.ty
    }

    /// Internal peek value
    pub fn value(&self) -> Peek<'mem, 'facet, 'shape> {
        self.value
    }
}
