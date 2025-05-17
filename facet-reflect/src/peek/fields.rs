use facet_core::{Field, FieldFlags};

use crate::Peek;
use alloc::{vec, vec::Vec};

/// Trait for types that have field methods
///
/// This trait allows code to be written generically over both structs and enums
/// that provide field access and iteration capabilities.
pub trait HasFields<'mem, 'facet, 'shape> {
    /// Iterates over all fields in this type, providing both field metadata and value
    fn fields(
        &self,
    ) -> impl DoubleEndedIterator<Item = (Field<'shape>, Peek<'mem, 'facet, 'shape>)>;

    /// Iterates over fields in this type that should be included when it is serialized
    fn fields_for_serialize(
        &self,
    ) -> impl DoubleEndedIterator<Item = (Field<'shape>, Peek<'mem, 'facet, 'shape>)> {
        // This is a default implementation that filters out fields with `skip_serializing`
        // attribute and handles field flattening.
        self.fields()
            .filter(|(field, peek)| !unsafe { field.should_skip_serializing(peek.data()) })
            .flat_map(move |(mut field, peek)| {
                if field.flags.contains(FieldFlags::FLATTEN) {
                    let mut flattened = Vec::new();
                    if let Ok(struct_peek) = peek.into_struct() {
                        struct_peek
                            .fields_for_serialize()
                            .for_each(|item| flattened.push(item));
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
                        flattened.push((field, peek));
                    } else {
                        // TODO: fail more gracefully
                        panic!("cannot flatten a {}", field.shape())
                    }
                    flattened
                } else {
                    vec![(field, peek)]
                }
            })
    }
}
