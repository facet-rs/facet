use core::ops::Range;

use facet_core::{Field, FieldFlags};

use crate::Peek;
use alloc::{vec, vec::Vec};

use super::{PeekEnum, PeekStruct, PeekTuple};

/// Trait for types that have field methods
///
/// This trait allows code to be written generically over both structs and enums
/// that provide field access and iteration capabilities.
pub trait HasFields<'mem, 'facet, 'shape> {
    /// Iterates over all fields in this type, providing both field metadata and value
    fn fields(&self) -> FieldIter<'mem, 'facet, 'shape>;

    /// Iterates over fields in this type that should be included when it is serialized
    fn fields_for_serialize(&self) -> FieldsForSerializeIter<'mem, 'facet, 'shape> {
        FieldsForSerializeIter {
            stack: vec![self.fields()],
        }
    }
}

/// An iterator over all the fields of a struct or enum. See [`HasFields::fields`]
pub struct FieldIter<'mem, 'facet, 'shape> {
    state: FieldIterState<'mem, 'facet, 'shape>,
    range: Range<usize>,
}

enum FieldIterState<'mem, 'facet, 'shape> {
    Struct(PeekStruct<'mem, 'facet, 'shape>),
    Tuple(PeekTuple<'mem, 'facet, 'shape>),
    Enum {
        peek_enum: PeekEnum<'mem, 'facet, 'shape>,
        fields: &'shape [Field<'shape>],
    },
    FlattenedEnum {
        field: Field<'shape>,
        value: Peek<'mem, 'facet, 'shape>,
    },
}

impl<'mem, 'facet, 'shape> FieldIter<'mem, 'facet, 'shape> {
    pub(crate) fn new_struct(struct_: PeekStruct<'mem, 'facet, 'shape>) -> Self {
        Self {
            range: 0..struct_.ty.fields.len(),
            state: FieldIterState::Struct(struct_),
        }
    }

    pub(crate) fn new_enum(enum_: PeekEnum<'mem, 'facet, 'shape>) -> Self {
        // Get the fields of the active variant
        let variant = match enum_.active_variant() {
            Ok(v) => v,
            Err(e) => panic!("Cannot get active variant: {:?}", e),
        };
        let fields = &variant.data.fields;

        Self {
            range: 0..fields.len(),
            state: FieldIterState::Enum {
                peek_enum: enum_,
                fields,
            },
        }
    }

    pub(crate) fn new_tuple(tuple: PeekTuple<'mem, 'facet, 'shape>) -> Self {
        Self {
            range: 0..tuple.len(),
            state: FieldIterState::Tuple(tuple),
        }
    }

    fn get_field_by_index(
        &self,
        index: usize,
    ) -> Option<(Field<'shape>, Peek<'mem, 'facet, 'shape>)> {
        match self.state {
            FieldIterState::Struct(peek_struct) => {
                let field = peek_struct.ty.fields.get(index).copied()?;
                let value = peek_struct.field(index).ok()?;
                Some((field, value))
            }
            FieldIterState::Tuple(peek_tuple) => {
                let field = peek_tuple.ty.fields.get(index).copied()?;
                let value = peek_tuple.field(index)?;
                Some((field, value))
            }
            FieldIterState::Enum { peek_enum, fields } => {
                // Get the field definition
                let field = fields[index];
                // Get the field value
                let field_value = match peek_enum.field(index) {
                    Ok(Some(v)) => v,
                    Ok(None) => return None,
                    Err(e) => panic!("Cannot get field: {:?}", e),
                };
                // Return the field definition and value
                Some((field, field_value))
            }
            FieldIterState::FlattenedEnum { field, value } => {
                if index == 0 {
                    Some((field, value))
                } else {
                    None
                }
            }
        }
    }
}

impl<'mem, 'facet, 'shape> Iterator for FieldIter<'mem, 'facet, 'shape> {
    type Item = (Field<'shape>, Peek<'mem, 'facet, 'shape>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let index = self.range.next()?;

            let Some(field) = self.get_field_by_index(index) else {
                continue;
            };

            return Some(field);
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.range.size_hint()
    }
}

impl DoubleEndedIterator for FieldIter<'_, '_, '_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        loop {
            let index = self.range.next_back()?;

            let Some(field) = self.get_field_by_index(index) else {
                continue;
            };

            return Some(field);
        }
    }
}

impl ExactSizeIterator for FieldIter<'_, '_, '_> {}

/// An iterator over the fields of a struct or enum that should be serialized. See [`HasFields::fields_for_serialize`]
pub struct FieldsForSerializeIter<'mem, 'facet, 'shape> {
    stack: Vec<FieldIter<'mem, 'facet, 'shape>>,
}

impl<'mem, 'facet, 'shape> Iterator for FieldsForSerializeIter<'mem, 'facet, 'shape> {
    type Item = (Field<'shape>, Peek<'mem, 'facet, 'shape>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let mut fields = self.stack.pop()?;
            let Some((mut field, peek)) = fields.next() else {
                continue;
            };
            self.stack.push(fields);

            let Some(data) = peek.data().thin() else {
                continue;
            };
            let should_skip = unsafe { field.should_skip_serializing(data) };

            if should_skip {
                continue;
            }

            if field.flags.contains(FieldFlags::FLATTEN) && !field.flattened {
                if let Ok(struct_peek) = peek.into_struct() {
                    self.stack.push(FieldIter::new_struct(struct_peek))
                } else if let Ok(enum_peek) = peek.into_enum() {
                    // normally we'd serialize to something like:
                    //
                    //   {
                    //     "field_on_struct": {
                    //       "VariantName": { "field_on_variant": "foo" }
                    //     }
                    //   }
                    //
                    // But since `field_on_struct` is flattened, instead we do:
                    //
                    //   {
                    //     "VariantName": { "field_on_variant": "foo" }
                    //   }
                    field.name = enum_peek
                        .active_variant()
                        .expect("Failed to get active variant")
                        .name;
                    field.flattened = true;
                    self.stack.push(FieldIter {
                        range: 0..1,
                        state: FieldIterState::FlattenedEnum { field, value: peek },
                    });
                } else {
                    // TODO: fail more gracefully
                    panic!("cannot flatten a {}", field.shape())
                }
            } else {
                return Some((field, peek));
            }
        }
    }
}
