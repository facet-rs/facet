#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![deny(unsafe_code)]
#![doc = include_str!("../README.md")]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use facet_core::{Def, Facet, Field, ShapeAttribute, StructKind};
use facet_reflect::{HasFields, Peek, PeekList, PeekMap, PeekStruct};
use log::debug;

mod debug_serializer;

fn variant_is_newtype_like(variant: &facet_core::Variant) -> bool {
    variant.data.kind == facet_core::StructKind::Tuple && variant.data.fields.len() == 1
}

// --- Serializer Trait Definition ---

/// A trait for implementing format-specific serialization logic.
/// The core iterative serializer uses this trait to output data.
pub trait Serializer {
    /// The error type returned by serialization methods
    type Error;

    /// Serialize an unsigned 64-bit integer.
    fn serialize_u64(&mut self, value: u64) -> Result<(), Self::Error>;

    /// Serialize an unsigned 128-bit integer.
    fn serialize_u128(&mut self, value: u128) -> Result<(), Self::Error>;

    /// Serialize a signed 64-bit integer.
    fn serialize_i64(&mut self, value: i64) -> Result<(), Self::Error>;

    /// Serialize a signed 128-bit integer.
    fn serialize_i128(&mut self, value: i128) -> Result<(), Self::Error>;

    /// Serialize a double-precision floating-point value.
    fn serialize_f64(&mut self, value: f64) -> Result<(), Self::Error>;

    /// Serialize a boolean value.
    fn serialize_bool(&mut self, value: bool) -> Result<(), Self::Error>;

    /// Serialize a character.
    fn serialize_char(&mut self, value: char) -> Result<(), Self::Error>;

    /// Serialize a UTF-8 string slice.
    fn serialize_str(&mut self, value: &str) -> Result<(), Self::Error>;

    /// Serialize a raw byte slice.
    fn serialize_bytes(&mut self, value: &[u8]) -> Result<(), Self::Error>;

    // Special values

    /// Serialize a `None` variant of an Option type.
    fn serialize_none(&mut self) -> Result<(), Self::Error>;

    /// Serialize a unit value `()`.
    fn serialize_unit(&mut self) -> Result<(), Self::Error>;

    // Enum specific values

    /// Serialize a unit variant of an enum (no data).
    ///
    /// # Arguments
    ///
    /// * `variant_index` - The index of the variant.
    /// * `variant_name` - The name of the variant.
    fn serialize_unit_variant(
        &mut self,
        variant_index: usize,
        variant_name: &'static str,
    ) -> Result<(), Self::Error>;

    /// Begin serializing an object/map-like value.
    ///
    /// # Arguments
    ///
    /// * `len` - The number of fields, if known.
    fn start_object(&mut self, len: Option<usize>) -> Result<(), Self::Error>;

    /// Signal the end of serializing an object/map-like value.
    fn end_object(&mut self) -> Result<(), Self::Error>;

    /// Begin serializing an array/sequence-like value.
    ///
    /// # Arguments
    ///
    /// * `len` - The number of elements, if known.
    fn start_array(&mut self, len: Option<usize>) -> Result<(), Self::Error>;

    /// Signal the end of serializing an array/sequence-like value.
    fn end_array(&mut self) -> Result<(), Self::Error>;

    /// Begin serializing a map/dictionary-like value.
    ///
    /// # Arguments
    ///
    /// * `len` - The number of entries, if known.
    fn start_map(&mut self, len: Option<usize>) -> Result<(), Self::Error>;

    /// Signal the end of serializing a map/dictionary-like value.
    fn end_map(&mut self) -> Result<(), Self::Error>;

    /// Serialize an unsigned 8-bit integer.
    #[inline(always)]
    fn serialize_u8(&mut self, value: u8) -> Result<(), Self::Error> {
        self.serialize_u64(value as u64)
    }

    /// Serialize an unsigned 16-bit integer.
    #[inline(always)]
    fn serialize_u16(&mut self, value: u16) -> Result<(), Self::Error> {
        self.serialize_u64(value as u64)
    }

    /// Serialize an unsigned 32-bit integer.
    #[inline(always)]
    fn serialize_u32(&mut self, value: u32) -> Result<(), Self::Error> {
        self.serialize_u64(value as u64)
    }

    /// Serialize a `usize` integer.
    #[inline(always)]
    fn serialize_usize(&mut self, value: usize) -> Result<(), Self::Error> {
        // We assume `usize` will never be >64 bits
        self.serialize_u64(value as u64)
    }

    /// Serialize a signed 8-bit integer.
    #[inline(always)]
    fn serialize_i8(&mut self, value: i8) -> Result<(), Self::Error> {
        self.serialize_i64(value as i64)
    }

    /// Serialize a signed 16-bit integer.
    #[inline(always)]
    fn serialize_i16(&mut self, value: i16) -> Result<(), Self::Error> {
        self.serialize_i64(value as i64)
    }

    /// Serialize a signed 32-bit integer.
    #[inline(always)]
    fn serialize_i32(&mut self, value: i32) -> Result<(), Self::Error> {
        self.serialize_i64(value as i64)
    }

    /// Serialize an `isize` integer.
    #[inline(always)]
    fn serialize_isize(&mut self, value: isize) -> Result<(), Self::Error> {
        // We assume `isize` will never be >64 bits
        self.serialize_i64(value as i64)
    }

    /// Serialize a single-precision floating-point value.
    #[inline(always)]
    fn serialize_f32(&mut self, value: f32) -> Result<(), Self::Error> {
        self.serialize_f64(value as f64)
    }

    /// Begin serializing a map key value.
    #[inline(always)]
    fn begin_map_key(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Signal the end of serializing a map key value.
    #[inline(always)]
    fn end_map_key(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Begin serializing a map value.
    #[inline(always)]
    fn begin_map_value(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Signal the end of serializing a map value.
    #[inline(always)]
    fn end_map_value(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    // For objects/maps

    /// Serialize a field name (for objects and maps).
    ///
    /// # Arguments
    ///
    /// * `name` - The field or key name to serialize.
    fn serialize_field_name(&mut self, name: &'static str) -> Result<(), Self::Error>;

    /// Signal the end of serializing a field.
    #[inline(always)]
    fn end_field(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

// --- Iterative Serialization Logic ---

/// Task items for the serialization stack.
#[derive(Debug)]
enum SerializeTask<'mem, 'facet> {
    Value(Peek<'mem, 'facet>, Option<Field>),
    // End markers
    EndObject,
    EndArray,
    EndMap,
    EndMapKey,
    EndMapValue,
    EndField,
    // Tasks to push sub-elements onto the stack
    ObjectFields(PeekStruct<'mem, 'facet>),
    ArrayItems(PeekList<'mem, 'facet>),
    TupleStructFields(PeekStruct<'mem, 'facet>),
    MapEntries(PeekMap<'mem, 'facet>),
    // Field-related tasks
    SerializeFieldName(&'static str),
    SerializeMapKey(Peek<'mem, 'facet>),
    SerializeMapValue(Peek<'mem, 'facet>),
}

/// Serializes a `Peek` value using the provided `Serializer`.
///
/// This function uses an iterative approach with a stack to avoid recursion depth limits.
pub fn serialize_iterative<S>(peek: Peek<'_, '_>, serializer: &mut S) -> Result<(), S::Error>
where
    S: Serializer,
{
    let mut stack = Vec::new();
    stack.push(SerializeTask::Value(peek, None));

    while let Some(task) = stack.pop() {
        match task {
            SerializeTask::Value(mut cpeek, maybe_field) => {
                debug!("Serializing a value");

                if cpeek
                    .shape()
                    .attributes
                    .iter()
                    .any(|attr| matches!(attr, ShapeAttribute::Transparent))
                {
                    let old_shape = cpeek.shape();

                    // then serialize the inner shape instead
                    let ps = cpeek.into_struct().unwrap();
                    cpeek = ps.field(0).unwrap();

                    let new_shape = cpeek.shape();
                    debug!(
                        "{old_shape} is transparent, let's serialize the inner {new_shape} instead"
                    );
                }

                match cpeek.shape().def {
                    Def::Scalar(_) => {
                        let cpeek = cpeek.innermost_peek();

                        // Handle the unit type explicitly first if it's classified as Scalar
                        if cpeek.shape().is_type::<()>() {
                            serializer.serialize_unit()?
                        }
                        // Dispatch to appropriate scalar serialization method based on type
                        else if cpeek.shape().is_type::<bool>() {
                            let value = cpeek.get::<bool>().unwrap();
                            serializer.serialize_bool(*value)?
                        } else if cpeek.shape().is_type::<String>() {
                            let value = cpeek.get::<String>().unwrap();
                            serializer.serialize_str(value)?
                        } else if cpeek.shape().is_type::<alloc::borrow::Cow<'_, str>>() {
                            let value = cpeek.get::<alloc::borrow::Cow<'_, str>>().unwrap();
                            serializer.serialize_str(value.as_ref())?
                        } else if cpeek.shape().is_type::<&str>() {
                            let value = cpeek.get::<&str>().unwrap();
                            serializer.serialize_str(value)?
                        } else if cpeek.shape().is_type::<char>() {
                            let value = cpeek.get::<char>().unwrap();
                            serializer.serialize_char(*value)?
                        }
                        // Integer types
                        else if cpeek.shape().is_type::<u8>() {
                            let value = cpeek.get::<u8>().unwrap();
                            serializer.serialize_u8(*value)?
                        } else if cpeek.shape().is_type::<u16>() {
                            let value = cpeek.get::<u16>().unwrap();
                            serializer.serialize_u16(*value)?
                        } else if cpeek.shape().is_type::<u32>() {
                            let value = cpeek.get::<u32>().unwrap();
                            serializer.serialize_u32(*value)?
                        } else if cpeek.shape().is_type::<u64>() {
                            let value = cpeek.get::<u64>().unwrap();
                            serializer.serialize_u64(*value)?
                        } else if cpeek.shape().is_type::<u128>() {
                            let value = cpeek.get::<u128>().unwrap();
                            serializer.serialize_u128(*value)?
                        } else if cpeek.shape().is_type::<usize>() {
                            let value = cpeek.get::<usize>().unwrap();
                            serializer.serialize_usize(*value)?
                        } else if cpeek.shape().is_type::<i8>() {
                            let value = cpeek.get::<i8>().unwrap();
                            serializer.serialize_i8(*value)?
                        } else if cpeek.shape().is_type::<i16>() {
                            let value = cpeek.get::<i16>().unwrap();
                            serializer.serialize_i16(*value)?
                        } else if cpeek.shape().is_type::<i32>() {
                            let value = cpeek.get::<i32>().unwrap();
                            serializer.serialize_i32(*value)?
                        } else if cpeek.shape().is_type::<i64>() {
                            let value = cpeek.get::<i64>().unwrap();
                            serializer.serialize_i64(*value)?
                        } else if cpeek.shape().is_type::<i128>() {
                            let value = cpeek.get::<i128>().unwrap();
                            serializer.serialize_i128(*value)?
                        } else if cpeek.shape().is_type::<isize>() {
                            let value = cpeek.get::<isize>().unwrap();
                            serializer.serialize_isize(*value)?
                        }
                        // Float types
                        else if cpeek.shape().is_type::<f32>() {
                            let value = cpeek.get::<f32>().unwrap();
                            serializer.serialize_f32(*value)?
                        } else if cpeek.shape().is_type::<f64>() {
                            let value = cpeek.get::<f64>().unwrap();
                            serializer.serialize_f64(*value)?
                        } else {
                            panic!("Unsupported shape: {}", cpeek.shape());
                        }
                    }
                    Def::Struct(sd) => {
                        debug!("cpeek.shape(): {}", cpeek.shape());
                        match sd.kind {
                            StructKind::Unit => {
                                // Correctly handle unit struct type when encountered as a value
                                serializer.serialize_unit()?;
                            }
                            StructKind::Tuple | StructKind::TupleStruct => {
                                let peek_struct = cpeek.into_struct().unwrap();
                                let fields = peek_struct.fields_for_serialize().count();
                                serializer.start_array(Some(fields))?;
                                stack.push(SerializeTask::EndArray);
                                stack.push(SerializeTask::TupleStructFields(peek_struct));
                            }
                            StructKind::Struct => {
                                let peek_struct = cpeek.into_struct().unwrap();
                                let fields = peek_struct.fields_for_serialize().count();
                                serializer.start_object(Some(fields))?;
                                stack.push(SerializeTask::EndObject);
                                stack.push(SerializeTask::ObjectFields(peek_struct));
                            }
                            _ => {
                                unreachable!()
                            }
                        }
                    }
                    Def::Enum(_) => {
                        let peek_enum = cpeek.into_enum().unwrap();
                        let variant = peek_enum.active_variant();
                        let variant_index = peek_enum.variant_index();
                        let flattened = maybe_field.map(|f| f.flattened).unwrap_or_default();

                        if variant.data.fields.is_empty() {
                            // Unit variant
                            serializer.serialize_unit_variant(variant_index, variant.name)?;
                        } else {
                            if !flattened {
                                // For now, treat all enum variants with data as objects
                                serializer.start_object(Some(1))?;
                                stack.push(SerializeTask::EndObject);

                                // Serialize variant name as field name
                                serializer.serialize_field_name(variant.name)?;
                            }

                            if variant_is_newtype_like(variant) {
                                // Newtype variant - serialize the inner value directly
                                let fields = peek_enum.fields_for_serialize().collect::<Vec<_>>();
                                let (field, field_peek) = fields[0];
                                // TODO: error if `skip_serialize` is set?
                                stack.push(SerializeTask::Value(field_peek, Some(field)));
                            } else if variant.data.kind == StructKind::Tuple
                                || variant.data.kind == StructKind::TupleStruct
                            {
                                // Tuple variant - serialize as array
                                let fields = peek_enum.fields_for_serialize().count();
                                serializer.start_array(Some(fields))?;
                                stack.push(SerializeTask::EndArray);

                                // Push fields in reverse order for tuple variant
                                for (field, field_peek) in peek_enum.fields_for_serialize().rev() {
                                    stack.push(SerializeTask::Value(field_peek, Some(field)));
                                }
                            } else {
                                // Struct variant - serialize as object
                                let fields = peek_enum.fields_for_serialize().count();
                                serializer.start_object(Some(fields))?;
                                stack.push(SerializeTask::EndObject);

                                // Push fields in reverse order for struct variant
                                for (field, field_peek) in peek_enum.fields_for_serialize().rev() {
                                    stack.push(SerializeTask::EndField);
                                    stack.push(SerializeTask::Value(field_peek, Some(field)));
                                    stack.push(SerializeTask::SerializeFieldName(field.name));
                                }
                            }
                        }
                    }
                    Def::List(_) | Def::Array(_) | Def::Slice(_) => {
                        let peek_list = cpeek.into_list().unwrap();
                        let len = peek_list.len();
                        serializer.start_array(Some(len))?;
                        stack.push(SerializeTask::EndArray);
                        stack.push(SerializeTask::ArrayItems(peek_list));
                    }
                    Def::Map(_) => {
                        let peek_map = cpeek.into_map().unwrap();
                        let len = peek_map.len();
                        serializer.start_map(Some(len))?;
                        stack.push(SerializeTask::EndMap);
                        stack.push(SerializeTask::MapEntries(peek_map));
                    }
                    Def::Option(_) => {
                        let opt = cpeek.into_option().unwrap();
                        if let Some(inner_peek) = opt.value() {
                            stack.push(SerializeTask::Value(inner_peek, None));
                        } else {
                            serializer.serialize_none()?;
                        }
                    }
                    Def::SmartPointer(_) => {
                        let _sp = cpeek.into_smart_pointer().unwrap();
                        panic!("TODO: Implement serialization for smart pointers");
                    }
                    Def::FunctionPointer(_) => {
                        // Serialize function pointers as units or some special representation
                        serializer.serialize_unit()?;
                    }
                    _ => {
                        // Default case for any other definitions
                        serializer.serialize_unit()?;
                    }
                }
            }

            // --- Pushing sub-elements onto the stack ---
            SerializeTask::ObjectFields(peek_struct) => {
                // Push fields in reverse order for stack processing
                for (field, field_peek) in peek_struct.fields_for_serialize().rev() {
                    stack.push(SerializeTask::EndField);
                    stack.push(SerializeTask::Value(field_peek, Some(field)));
                    stack.push(SerializeTask::SerializeFieldName(field.name));
                }
            }
            SerializeTask::TupleStructFields(peek_struct) => {
                // Push fields in reverse order
                for (field, field_peek) in peek_struct.fields_for_serialize().rev() {
                    stack.push(SerializeTask::Value(field_peek, Some(field)));
                }
            }
            SerializeTask::ArrayItems(peek_list) => {
                // Push items in reverse order
                let items: Vec<_> = peek_list.iter().collect();
                for item_peek in items.into_iter().rev() {
                    stack.push(SerializeTask::Value(item_peek, None));
                }
            }
            SerializeTask::MapEntries(peek_map) => {
                // Push entries in reverse order (key, value pairs)
                let entries = peek_map.iter().collect::<Vec<_>>();
                for (key_peek, value_peek) in entries.into_iter().rev() {
                    stack.push(SerializeTask::SerializeMapValue(value_peek));
                    stack.push(SerializeTask::SerializeMapKey(key_peek));
                }
            }

            // --- Field name and map key/value handling ---
            SerializeTask::SerializeFieldName(name) => {
                serializer.serialize_field_name(name)?;
            }
            SerializeTask::SerializeMapKey(key_peek) => {
                stack.push(SerializeTask::EndMapKey);
                stack.push(SerializeTask::Value(key_peek, None));
                serializer.begin_map_key()?;
            }
            SerializeTask::SerializeMapValue(value_peek) => {
                stack.push(SerializeTask::EndMapValue);
                stack.push(SerializeTask::Value(value_peek, None));
                serializer.begin_map_value()?;
            }

            // --- End composite type tasks ---
            SerializeTask::EndObject => {
                serializer.end_object()?;
            }
            SerializeTask::EndArray => {
                serializer.end_array()?;
            }
            SerializeTask::EndMap => {
                serializer.end_map()?;
            }
            SerializeTask::EndMapKey => {
                serializer.end_map_key()?;
            }
            SerializeTask::EndMapValue => {
                serializer.end_map_value()?;
            }
            SerializeTask::EndField => {
                serializer.end_field()?;
            }
        }
    }

    // Successful completion
    Ok(())
}

// --- Helper Trait for Ergonomics ---

/// Extension trait to simplify calling the generic serializer.
pub trait Serialize<'a>: Facet<'a> {
    /// Serialize this value using the provided `Serializer`.
    fn serialize<S: Serializer>(&self, serializer: &mut S) -> Result<(), S::Error>;
}

impl<'a, T> Serialize<'a> for T
where
    T: Facet<'a>,
{
    /// Serialize this value using the provided `Serializer`.
    fn serialize<S: Serializer>(&self, serializer: &mut S) -> Result<(), S::Error> {
        let peek = Peek::new(self);
        serialize_iterative(peek, serializer)
    }
}
