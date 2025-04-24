use facet_core::{Field, FieldAttribute, FieldError, Struct};

use crate::Peek;

/// Lets you read from a struct (implements read-only struct operations)
#[derive(Clone, Copy)]
pub struct PeekStruct<'mem, 'facet_lifetime> {
    /// the underlying value
    pub(crate) value: Peek<'mem, 'facet_lifetime>,

    /// the definition of the struct!
    pub(crate) def: Struct,
}

impl<'mem, 'facet_lifetime> PeekStruct<'mem, 'facet_lifetime> {
    /// Returns the struct definition
    #[inline(always)]
    pub fn def(&self) -> &Struct {
        &self.def
    }

    /// Returns the number of fields in this struct
    #[inline(always)]
    pub fn field_count(&self) -> usize {
        self.def.fields.len()
    }

    /// Returns the value of the field at the given index
    #[inline(always)]
    pub fn field(&self, index: usize) -> Result<Peek<'mem, 'facet_lifetime>, FieldError> {
        self.def
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
        for (i, field) in self.def.fields.iter().enumerate() {
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
            let field = self.def.fields.get(i)?;
            let value = self.field(i).ok()?;
            Some((field, value))
        })
    }

    /// Iterates over thos fields in this struct that should be included when it is serialized.
    #[inline]
    pub fn fields_for_serialize(
        &self,
    ) -> impl Iterator<Item = (&'static Field, Peek<'mem, 'facet_lifetime>)> + '_ {
        self.fields().filter(|(field, peek)| {
            for attr in field.attributes {
                match attr {
                    FieldAttribute::SkipSerializingIf(fn_ptr) => {
                        if unsafe { fn_ptr(peek.data()) } {
                            return false;
                        }
                    }
                    FieldAttribute::SkipSerializing => {
                        return false;
                    }
                    _ => {}
                }
            }
            true
        })
    }
}
