use facet_core::{Field, FieldAttribute, FieldError, FieldFlags, StructType};

use crate::Peek;

/// Lets you read from a struct (implements read-only struct operations)
#[derive(Clone, Copy)]
pub struct PeekStruct<'mem, 'facet_lifetime> {
    /// the underlying value
    pub(crate) value: Peek<'mem, 'facet_lifetime>,

    /// the definition of the struct!
    pub(crate) ty: StructType,
}

impl core::fmt::Debug for PeekStruct<'_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PeekStruct").finish_non_exhaustive()
    }
}

impl<'mem, 'facet_lifetime> PeekStruct<'mem, 'facet_lifetime> {
    /// Returns the struct definition
    #[inline(always)]
    pub fn ty(&self) -> &StructType {
        &self.ty
    }

    /// Returns the number of fields in this struct
    #[inline(always)]
    pub fn field_count(&self) -> usize {
        self.ty.fields.len()
    }

    /// Returns the value of the field at the given index
    #[inline(always)]
    pub fn field(&self, index: usize) -> Result<Peek<'mem, 'facet_lifetime>, FieldError> {
        self.ty
            .fields
            .get(index)
            .map(|field| unsafe {
                let field_data = self.value.data().field(field.offset);
                Peek::unchecked_new(field_data, field.shape())
            })
            .ok_or(FieldError::IndexOutOfBounds)
    }

    /// Gets the value of the field with the given name
    #[inline]
    pub fn field_by_name(&self, name: &str) -> Result<Peek<'mem, 'facet_lifetime>, FieldError> {
        for (i, field) in self.ty.fields.iter().enumerate() {
            if field.name == name {
                return self.field(i);
            }
        }
        Err(FieldError::NoSuchField)
    }

    /// Iterates over all fields in this struct, providing both name and value
    #[inline]
    pub fn fields(
        &self,
    ) -> impl Iterator<Item = (&'static Field, Peek<'mem, 'facet_lifetime>)> + '_ {
        (0..self.field_count()).filter_map(|i| {
            let field = self.ty.fields.get(i)?;
            let value = self.field(i).ok()?;
            Some((field, value))
        })
    }

    /// Iterates over fields in this struct that should be included when it is serialized.
    #[inline]
    pub fn fields_for_serialize(
        &self,
    ) -> impl Iterator<Item = (&'static Field, Peek<'mem, 'facet_lifetime>)> + '_ {
        self.fields().filter(|(field, peek)| {
            if field.flags.contains(FieldFlags::SKIP_SERIALIZING) {
                return false;
            }

            for attr in field.attributes {
                if let FieldAttribute::SkipSerializingIf(fn_ptr) = attr {
                    if unsafe { fn_ptr(peek.data()) } {
                        return false;
                    }
                }
            }
            true
        })
    }
}
