use facet_core::{Facet, FieldError, StructType};

use crate::ReflectError;

use super::Poke;

/// Lets you mutate a struct's fields.
pub struct PokeStruct<'mem, 'facet> {
    /// The underlying value
    pub(crate) value: Poke<'mem, 'facet>,

    /// The definition of the struct
    pub(crate) ty: StructType,
}

impl core::fmt::Debug for PokeStruct<'_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PokeStruct").finish_non_exhaustive()
    }
}

impl<'mem, 'facet> PokeStruct<'mem, 'facet> {
    /// Returns the struct definition.
    #[inline(always)]
    pub const fn ty(&self) -> &StructType {
        &self.ty
    }

    /// Returns the number of fields in this struct.
    #[inline(always)]
    pub const fn field_count(&self) -> usize {
        self.ty.fields.len()
    }

    /// Returns a `Poke` for the field at the given index.
    ///
    /// This always succeeds (for valid indices). The POD check happens when
    /// you try to mutate via [`Poke::set`] on the returned field poke, or
    /// when calling [`PokeStruct::set_field`] which checks the parent struct.
    pub fn field(&mut self, index: usize) -> Result<Poke<'_, 'facet>, ReflectError> {
        let field = self.ty.fields.get(index).ok_or(ReflectError::FieldError {
            shape: self.value.shape,
            field_error: FieldError::IndexOutOfBounds {
                index,
                bound: self.ty.fields.len(),
            },
        })?;

        let field_data = unsafe { self.value.data.field(field.offset) };
        let field_shape = field.shape();

        Ok(unsafe { Poke::from_raw_parts(field_data, field_shape) })
    }

    /// Returns a `Poke` for the field with the given name.
    ///
    /// Returns an error if the field is not found.
    pub fn field_by_name(&mut self, name: &str) -> Result<Poke<'_, 'facet>, ReflectError> {
        for (i, field) in self.ty.fields.iter().enumerate() {
            if field.name == name {
                return self.field(i);
            }
        }
        Err(ReflectError::FieldError {
            shape: self.value.shape,
            field_error: FieldError::NoSuchField,
        })
    }

    /// Sets the value of a field by index.
    ///
    /// The value type must match the field's type.
    ///
    /// Returns an error if the parent struct is not POD. Field mutation could
    /// violate struct-level invariants, so the struct must be marked with
    /// `#[facet(pod)]` to allow this.
    pub fn set_field<T: Facet<'facet>>(
        &mut self,
        index: usize,
        value: T,
    ) -> Result<(), ReflectError> {
        // Check that the parent struct is POD before allowing field mutation
        if !self.value.shape.is_pod() {
            return Err(ReflectError::NotPod {
                shape: self.value.shape,
            });
        }

        let field = self.ty.fields.get(index).ok_or(ReflectError::FieldError {
            shape: self.value.shape,
            field_error: FieldError::IndexOutOfBounds {
                index,
                bound: self.ty.fields.len(),
            },
        })?;

        let field_shape = field.shape();
        if field_shape != T::SHAPE {
            return Err(ReflectError::WrongShape {
                expected: field_shape,
                actual: T::SHAPE,
            });
        }

        unsafe {
            let field_ptr = self.value.data.field(field.offset);
            // Drop the old value and write the new one
            field_shape.call_drop_in_place(field_ptr);
            core::ptr::write(field_ptr.as_mut_byte_ptr() as *mut T, value);
        }

        Ok(())
    }

    /// Sets the value of a field by name.
    ///
    /// The value type must match the field's type.
    pub fn set_field_by_name<T: Facet<'facet>>(
        &mut self,
        name: &str,
        value: T,
    ) -> Result<(), ReflectError> {
        for (i, field) in self.ty.fields.iter().enumerate() {
            if field.name == name {
                return self.set_field(i, value);
            }
        }
        Err(ReflectError::FieldError {
            shape: self.value.shape,
            field_error: FieldError::NoSuchField,
        })
    }

    /// Gets a read-only view of a field by index.
    pub fn peek_field(&self, index: usize) -> Result<crate::Peek<'_, 'facet>, FieldError> {
        let field = self
            .ty
            .fields
            .get(index)
            .ok_or(FieldError::IndexOutOfBounds {
                index,
                bound: self.ty.fields.len(),
            })?;

        let field_data = unsafe { self.value.data.as_const().field(field.offset) };
        Ok(unsafe { crate::Peek::unchecked_new(field_data, field.shape()) })
    }

    /// Gets a read-only view of a field by name.
    pub fn peek_field_by_name(&self, name: &str) -> Result<crate::Peek<'_, 'facet>, FieldError> {
        for (i, field) in self.ty.fields.iter().enumerate() {
            if field.name == name {
                return self.peek_field(i);
            }
        }
        Err(FieldError::NoSuchField)
    }

    /// Converts this back into the underlying `Poke`.
    #[inline]
    pub const fn into_inner(self) -> Poke<'mem, 'facet> {
        self.value
    }

    /// Returns a read-only `PeekStruct` view.
    #[inline]
    pub fn as_peek_struct(&self) -> crate::PeekStruct<'_, 'facet> {
        crate::PeekStruct {
            value: self.value.as_peek(),
            ty: self.ty,
        }
    }
}

// Note: PokeStruct tests require custom structs with #[derive(Facet)] which can't be done
// in inline tests within facet-reflect. All PokeStruct tests are in the integration tests:
// facet-reflect/tests/poke/struct_.rs
