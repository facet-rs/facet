use core::fmt::Debug;
use facet_core::Field;

use super::{FieldIter, Peek};

/// Local representation of a tuple type for peek operations
#[derive(Clone, Copy, Debug)]
pub struct TupleType {
    /// Fields of the tuple, with offsets
    pub fields: &'static [Field],
}

/// Field index and associated peek value
pub type TupleField<'mem, 'facet> = (usize, Peek<'mem, 'facet>);

/// Lets you read from a tuple
#[derive(Clone, Copy)]
pub struct PeekTuple<'mem, 'facet> {
    /// Original peek value
    pub(crate) value: Peek<'mem, 'facet>,
    /// Tuple type information
    pub(crate) ty: TupleType,
}

impl Debug for PeekTuple<'_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PeekTuple")
            .field("type", &self.ty)
            .finish_non_exhaustive()
    }
}

impl<'mem, 'facet> PeekTuple<'mem, 'facet> {
    /// Get the number of fields in this tuple
    #[inline]
    pub const fn len(&self) -> usize {
        self.ty.fields.len()
    }

    /// Returns true if this tuple has no fields
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Access a field by index
    #[inline]
    pub fn field(&self, index: usize) -> Option<Peek<'mem, 'facet>> {
        if index >= self.len() {
            return None;
        }

        let field = &self.ty.fields[index];
        // We can safely use field operations here since this is within facet-reflect
        // which is allowed to use unsafe code
        let field_ptr = unsafe { self.value.data().field(field.offset) };
        let field_peek = unsafe { Peek::unchecked_new(field_ptr, field.shape()) };

        Some(field_peek)
    }

    /// Iterate over all fields
    #[inline]
    pub const fn fields(&self) -> FieldIter<'mem, 'facet> {
        FieldIter::new_tuple(*self)
    }

    /// Type information
    #[inline]
    pub const fn ty(&self) -> TupleType {
        self.ty
    }

    /// Internal peek value
    #[inline]
    pub const fn value(&self) -> Peek<'mem, 'facet> {
        self.value
    }
}
