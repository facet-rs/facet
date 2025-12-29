use core::ops::Range;

use alloc::borrow::Cow;
use facet_core::Field;

use crate::Peek;
use alloc::{string::String, vec, vec::Vec};

use super::{PeekEnum, PeekStruct, PeekTuple};

/// A field item with runtime state for serialization.
///
/// This wraps a static `Field` with additional runtime state that can be modified
/// during iteration (e.g., for flattened enums where the field name becomes the variant name,
/// or for flattened maps where entries become synthetic fields).
#[derive(Clone, Debug)]
pub struct FieldItem {
    /// The underlying static field definition (None for flattened map entries)
    pub field: Option<Field>,
    /// Runtime-determined name (may differ from field.name for flattened enums/maps)
    pub name: Cow<'static, str>,
    /// Whether this field was flattened from an enum (variant name used as key)
    pub flattened: bool,
}

impl FieldItem {
    /// Create a new FieldItem from a Field, using the field's name
    #[inline]
    pub fn new(field: Field) -> Self {
        Self {
            name: Cow::Borrowed(field.name),
            field: Some(field),
            flattened: false,
        }
    }

    /// Create a flattened enum field item with a custom name (the variant name)
    #[inline]
    pub fn flattened_enum(field: Field, variant_name: &'static str) -> Self {
        Self {
            name: Cow::Borrowed(variant_name),
            field: Some(field),
            flattened: true,
        }
    }

    /// Create a flattened map entry field item with a dynamic key
    #[inline]
    pub fn flattened_map_entry(key: String) -> Self {
        Self {
            name: Cow::Owned(key),
            field: None,
            flattened: true,
        }
    }
}

/// Trait for types that have field methods
///
/// This trait allows code to be written generically over both structs and enums
/// that provide field access and iteration capabilities.
pub trait HasFields<'mem, 'facet> {
    /// Iterates over all fields in this type, providing both field metadata and value
    fn fields(&self) -> FieldIter<'mem, 'facet>;

    /// Iterates over fields in this type that should be included when it is serialized
    fn fields_for_serialize(&self) -> FieldsForSerializeIter<'mem, 'facet> {
        FieldsForSerializeIter {
            stack: vec![FieldsForSerializeIterState::Fields(self.fields())],
        }
    }
}

/// An iterator over all the fields of a struct or enum. See [`HasFields::fields`]
pub struct FieldIter<'mem, 'facet> {
    state: FieldIterState<'mem, 'facet>,
    range: Range<usize>,
}

enum FieldIterState<'mem, 'facet> {
    Struct(PeekStruct<'mem, 'facet>),
    Tuple(PeekTuple<'mem, 'facet>),
    Enum {
        peek_enum: PeekEnum<'mem, 'facet>,
        fields: &'static [Field],
    },
}

impl<'mem, 'facet> FieldIter<'mem, 'facet> {
    #[inline]
    pub(crate) fn new_struct(struct_: PeekStruct<'mem, 'facet>) -> Self {
        Self {
            range: 0..struct_.ty.fields.len(),
            state: FieldIterState::Struct(struct_),
        }
    }

    #[inline]
    pub(crate) fn new_enum(enum_: PeekEnum<'mem, 'facet>) -> Self {
        // Get the fields of the active variant
        let variant = match enum_.active_variant() {
            Ok(v) => v,
            Err(e) => panic!("Cannot get active variant: {e:?}"),
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

    #[inline]
    pub(crate) fn new_tuple(tuple: PeekTuple<'mem, 'facet>) -> Self {
        Self {
            range: 0..tuple.len(),
            state: FieldIterState::Tuple(tuple),
        }
    }

    fn get_field_by_index(&self, index: usize) -> Option<(Field, Peek<'mem, 'facet>)> {
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
                    Err(e) => panic!("Cannot get field: {e:?}"),
                };
                // Return the field definition and value
                Some((field, field_value))
            }
        }
    }
}

impl<'mem, 'facet> Iterator for FieldIter<'mem, 'facet> {
    type Item = (Field, Peek<'mem, 'facet>);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let index = self.range.next()?;

            let Some(field) = self.get_field_by_index(index) else {
                continue;
            };

            return Some(field);
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.range.size_hint()
    }
}

impl DoubleEndedIterator for FieldIter<'_, '_> {
    #[inline]
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

impl ExactSizeIterator for FieldIter<'_, '_> {}

/// An iterator over the fields of a struct or enum that should be serialized. See [`HasFields::fields_for_serialize`]
pub struct FieldsForSerializeIter<'mem, 'facet> {
    stack: Vec<FieldsForSerializeIterState<'mem, 'facet>>,
}

enum FieldsForSerializeIterState<'mem, 'facet> {
    /// Normal field iteration
    Fields(FieldIter<'mem, 'facet>),
    /// A single flattened enum item to yield
    FlattenedEnum {
        field_item: Option<FieldItem>,
        value: Peek<'mem, 'facet>,
    },
    /// Iterating over a flattened map's entries
    FlattenedMap {
        map_iter: super::PeekMapIter<'mem, 'facet>,
    },
}

impl<'mem, 'facet> Iterator for FieldsForSerializeIter<'mem, 'facet> {
    type Item = (FieldItem, Peek<'mem, 'facet>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let state = self.stack.pop()?;

            match state {
                FieldsForSerializeIterState::FlattenedEnum { field_item, value } => {
                    // Yield the flattened enum item (only once)
                    if let Some(item) = field_item {
                        return Some((item, value));
                    }
                    // Already yielded, continue to next state
                    continue;
                }
                FieldsForSerializeIterState::FlattenedMap { mut map_iter } => {
                    // Iterate over map entries, yielding each as a synthetic field
                    if let Some((key_peek, value_peek)) = map_iter.next() {
                        // Push iterator back for more entries
                        self.stack
                            .push(FieldsForSerializeIterState::FlattenedMap { map_iter });
                        // Get the key as a string
                        if let Ok(key_str) = key_peek.get::<String>() {
                            let field_item = FieldItem::flattened_map_entry(key_str.clone());
                            return Some((field_item, value_peek));
                        }
                        // Skip entries with non-string keys
                        continue;
                    }
                    // Map exhausted, continue to next state
                    continue;
                }
                FieldsForSerializeIterState::Fields(mut fields) => {
                    let Some((field, peek)) = fields.next() else {
                        continue;
                    };
                    self.stack.push(FieldsForSerializeIterState::Fields(fields));

                    let data = peek.data();
                    let should_skip = unsafe { field.should_skip_serializing(data) };

                    if should_skip {
                        continue;
                    }

                    if field.is_flattened() {
                        if let Ok(struct_peek) = peek.into_struct() {
                            self.stack.push(FieldsForSerializeIterState::Fields(
                                FieldIter::new_struct(struct_peek),
                            ))
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
                            let variant_name = enum_peek
                                .active_variant()
                                .expect("Failed to get active variant")
                                .name;
                            let field_item = FieldItem::flattened_enum(field, variant_name);
                            self.stack.push(FieldsForSerializeIterState::FlattenedEnum {
                                field_item: Some(field_item),
                                value: peek,
                            });
                        } else if let Ok(map_peek) = peek.into_map() {
                            // Flattened map - emit entries as synthetic fields
                            self.stack.push(FieldsForSerializeIterState::FlattenedMap {
                                map_iter: map_peek.iter(),
                            });
                        } else if let Ok(option_peek) = peek.into_option() {
                            // Option<T> where T is a struct, enum, or map
                            // If Some, flatten the inner value; if None, skip entirely
                            if let Some(inner_peek) = option_peek.value() {
                                if let Ok(struct_peek) = inner_peek.into_struct() {
                                    self.stack.push(FieldsForSerializeIterState::Fields(
                                        FieldIter::new_struct(struct_peek),
                                    ))
                                } else if let Ok(enum_peek) = inner_peek.into_enum() {
                                    let variant_name = enum_peek
                                        .active_variant()
                                        .expect("Failed to get active variant")
                                        .name;
                                    let field_item = FieldItem::flattened_enum(field, variant_name);
                                    self.stack.push(FieldsForSerializeIterState::FlattenedEnum {
                                        field_item: Some(field_item),
                                        value: inner_peek,
                                    });
                                } else if let Ok(map_peek) = inner_peek.into_map() {
                                    self.stack.push(FieldsForSerializeIterState::FlattenedMap {
                                        map_iter: map_peek.iter(),
                                    });
                                } else {
                                    panic!(
                                        "cannot flatten Option<{}> - inner type must be struct, enum, or map",
                                        inner_peek.shape()
                                    )
                                }
                            }
                            // If None, we just skip - don't emit any fields
                        } else {
                            // TODO: fail more gracefully
                            panic!("cannot flatten a {}", field.shape())
                        }
                    } else {
                        return Some((FieldItem::new(field), peek));
                    }
                }
            }
        }
    }
}
