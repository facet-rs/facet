use facet_core::{Facet, FieldError};

use crate::{ReflectError, ReflectErrorKind, peek::TupleType};

use super::Poke;

/// Lets you mutate a tuple's fields (by index).
///
/// Tuples are just tuple-struct types without names, so this is a thin wrapper that
/// exposes ordered field access.
pub struct PokeTuple<'mem, 'facet> {
    pub(crate) value: Poke<'mem, 'facet>,
    pub(crate) ty: TupleType,
}

impl<'mem, 'facet> core::fmt::Debug for PokeTuple<'mem, 'facet> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PokeTuple")
            .field("type", &self.ty)
            .finish_non_exhaustive()
    }
}

impl<'mem, 'facet> PokeTuple<'mem, 'facet> {
    fn err(&self, kind: ReflectErrorKind) -> ReflectError {
        self.value.err(kind)
    }

    /// Returns the number of fields in this tuple.
    #[inline]
    pub const fn len(&self) -> usize {
        self.ty.fields.len()
    }

    /// Returns true if this tuple has no fields.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Tuple type information.
    #[inline]
    pub const fn ty(&self) -> TupleType {
        self.ty
    }

    /// Returns a read-only `Peek` for the field at the given index.
    pub fn field(&self, index: usize) -> Option<crate::Peek<'_, 'facet>> {
        let field = self.ty.fields.get(index)?;
        let field_ptr = unsafe { self.value.data().field(field.offset) };
        Some(unsafe { crate::Peek::unchecked_new(field_ptr, field.shape()) })
    }

    /// Returns a mutable `Poke` for the field at the given index.
    pub fn field_mut(&mut self, index: usize) -> Result<Poke<'_, 'facet>, ReflectError> {
        let field = self.ty.fields.get(index).ok_or_else(|| {
            self.err(ReflectErrorKind::FieldError {
                shape: self.value.shape,
                field_error: FieldError::IndexOutOfBounds {
                    index,
                    bound: self.ty.fields.len(),
                },
            })
        })?;
        let field_data = unsafe { self.value.data_mut().field(field.offset) };
        Ok(unsafe { Poke::from_raw_parts(field_data, field.shape()) })
    }

    /// Sets the value of a field by index.
    ///
    /// The value type must match the field's type.
    ///
    /// Returns an error if the parent tuple is not POD. Field mutation could violate
    /// tuple-level invariants, so the tuple must be marked with `#[facet(pod)]` to allow this.
    pub fn set_field<T: Facet<'facet>>(
        &mut self,
        index: usize,
        value: T,
    ) -> Result<(), ReflectError> {
        if !self.value.shape.is_pod() {
            return Err(self.err(ReflectErrorKind::NotPod {
                shape: self.value.shape,
            }));
        }

        let field = self.ty.fields.get(index).ok_or_else(|| {
            self.err(ReflectErrorKind::FieldError {
                shape: self.value.shape,
                field_error: FieldError::IndexOutOfBounds {
                    index,
                    bound: self.ty.fields.len(),
                },
            })
        })?;

        let field_shape = field.shape();
        if field_shape != T::SHAPE {
            return Err(self.err(ReflectErrorKind::WrongShape {
                expected: field_shape,
                actual: T::SHAPE,
            }));
        }

        unsafe {
            let field_ptr = self.value.data_mut().field(field.offset);
            field_shape.call_drop_in_place(field_ptr);
            core::ptr::write(field_ptr.as_mut_byte_ptr() as *mut T, value);
        }
        Ok(())
    }

    /// Converts this back into the underlying `Poke`.
    #[inline]
    pub const fn into_inner(self) -> Poke<'mem, 'facet> {
        self.value
    }

    /// Returns a read-only `PeekTuple` view.
    #[inline]
    pub fn as_peek_tuple(&self) -> crate::PeekTuple<'_, 'facet> {
        crate::PeekTuple {
            value: self.value.as_peek(),
            ty: self.ty,
        }
    }
}
