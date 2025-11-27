//! Create and/or write TOML strings from Rust values.

#[cfg(not(feature = "alloc"))]
compile_error!("feature `alloc` is required");

mod array_of_tables;
mod error;

use alloc::{
    borrow::Cow,
    string::{String, ToString},
    vec::Vec,
};
use facet_core::{Def, Facet, StructKind, Type, UserType};
use facet_reflect::{HasFields, Peek, ScalarType};
use log::trace;
use toml_edit::{Array, DocumentMut, InlineTable, Item, Value};

pub use error::TomlSerError;

/// Serialize any `Facet` type to a TOML string.
#[cfg(feature = "alloc")]
pub fn to_string<'a, T: Facet<'a>>(value: &'a T) -> Result<String, TomlSerError> {
    let peek = Peek::new(value);

    // Check if the root is a struct with fields that are arrays of tables
    if let Ok(struct_peek) = peek.into_struct() {
        let mut doc = DocumentMut::new();

        // Process each field
        for (field, field_value) in struct_peek.fields_for_serialize() {
            // Skip None values - TOML doesn't have null
            if let Def::Option(_) = field_value.shape().def {
                let opt = field_value.into_option().unwrap();
                if let Some(inner) = opt.value() {
                    // Skip unit types inside Option
                    if is_unit_like(&inner) {
                        continue;
                    }
                    // Check if this field is an array of tables
                    if array_of_tables::is_array_of_tables(&inner) {
                        let list = inner.into_list_like().unwrap();
                        let aot = array_of_tables::serialize_array_of_tables(list)?;
                        doc.insert(field.name, Item::ArrayOfTables(aot));
                    } else {
                        let item = serialize_to_item(inner)?;
                        doc.insert(field.name, item);
                    }
                }
                // Skip None
                continue;
            }

            // Skip unit types - TOML doesn't support unit
            if is_unit_like(&field_value) {
                continue;
            }

            // Check if this field is an array of tables
            if array_of_tables::is_array_of_tables(&field_value) {
                // Handle array of tables specially
                let list = field_value.into_list_like().unwrap();
                let aot = array_of_tables::serialize_array_of_tables(list)?;
                doc.insert(field.name, Item::ArrayOfTables(aot));
            } else {
                // Normal field serialization
                let item = serialize_to_item(field_value)?;
                doc.insert(field.name, item);
            }
        }

        Ok(doc.to_string())
    } else {
        // Not a struct at root - TOML requires root to be a table
        Err(TomlSerError::RootMustBeStruct)
    }
}

/// Check if a Peek value represents a unit type ((), unit struct, or empty tuple)
/// or a list/array of unit types (which also can't be represented in TOML)
fn is_unit_like(peek: &Peek<'_, '_>) -> bool {
    match peek.scalar_type() {
        Some(ScalarType::Unit) => return true,
        _ => {}
    }
    match (peek.shape().def, peek.shape().ty) {
        (_, Type::User(UserType::Struct(sd))) => {
            if sd.kind == StructKind::Unit {
                return true;
            }
            // Empty tuple struct
            if (sd.kind == StructKind::Tuple || sd.kind == StructKind::TupleStruct)
                && sd.fields.is_empty()
            {
                return true;
            }
        }
        // Check if it's a list/array of unit types
        (Def::List(ld), _) => {
            if let Type::User(UserType::Struct(sd)) = ld.t().ty {
                if sd.kind == StructKind::Unit {
                    return true;
                }
            }
        }
        (Def::Array(ad), _) => {
            if let Type::User(UserType::Struct(sd)) = ad.t().ty {
                if sd.kind == StructKind::Unit {
                    return true;
                }
            }
        }
        _ => {}
    }
    false
}

/// Serialize a Peek value to a TOML Item
pub(crate) fn serialize_to_item(peek: Peek<'_, '_>) -> Result<Item, TomlSerError> {
    serialize_value(peek).map(Item::Value)
}

/// Serialize a Peek value to a TOML Value
fn serialize_value(peek: Peek<'_, '_>) -> Result<Value, TomlSerError> {
    trace!("Serializing value, shape is {}", peek.shape());

    match (peek.shape().def, peek.shape().ty) {
        (Def::Scalar, _) => {
            let peek = peek.innermost_peek();
            serialize_scalar(peek)
        }
        (Def::List(_), _) | (Def::Array(_), _) | (Def::Slice(_), _) => {
            let list = peek.into_list_like().unwrap();
            let mut array = Array::new();
            for item in list.iter() {
                array.push(serialize_value(item)?);
            }
            Ok(Value::Array(array))
        }
        (Def::Map(_), _) => {
            let map = peek.into_map().unwrap();
            let mut table = InlineTable::new();
            for (key, value) in map.iter() {
                let key_str = key
                    .as_str()
                    .ok_or_else(|| TomlSerError::InvalidKeyConversion {
                        toml_type: "non-string map key",
                    })?;
                table.insert(key_str, serialize_value(value)?);
            }
            Ok(Value::InlineTable(table))
        }
        (Def::Set(_), _) => {
            let set = peek.into_set().unwrap();
            let mut array = Array::new();
            for item in set.iter() {
                array.push(serialize_value(item)?);
            }
            Ok(Value::Array(array))
        }
        (Def::Option(_), _) => {
            let opt = peek.into_option().unwrap();
            if let Some(inner) = opt.value() {
                serialize_value(inner)
            } else {
                // TOML doesn't have null, so we skip None values at the field level
                // But if we're serializing a standalone Option, we need to handle it
                Err(TomlSerError::UnsupportedNone)
            }
        }
        (Def::Pointer(_), _) => {
            let ptr = peek.into_pointer().unwrap();
            if let Some(inner) = ptr.borrow_inner() {
                serialize_value(inner)
            } else {
                Err(TomlSerError::UnsupportedPointer)
            }
        }
        (_, Type::User(UserType::Struct(sd))) => {
            match sd.kind {
                StructKind::Unit => {
                    // Unit structs - TOML doesn't have a good representation
                    Err(TomlSerError::UnsupportedUnitStruct)
                }
                StructKind::Tuple | StructKind::TupleStruct => {
                    let ps = peek.into_struct().unwrap();
                    let fields: Vec<_> = ps.fields_for_serialize().collect();
                    match fields.len() {
                        0 => {
                            // Empty tuple () - TOML doesn't support unit type
                            Err(TomlSerError::UnsupportedUnit)
                        }
                        1 => {
                            // Newtype tuple struct - serialize as just the inner value
                            serialize_value(fields[0].1)
                        }
                        _ => {
                            // Multi-field tuple structs serialize as arrays
                            let mut array = Array::new();
                            for (_, field_value) in fields {
                                array.push(serialize_value(field_value)?);
                            }
                            Ok(Value::Array(array))
                        }
                    }
                }
                StructKind::Struct => {
                    // Regular structs serialize as inline tables
                    let ps = peek.into_struct().unwrap();
                    let mut table = InlineTable::new();
                    for (field, field_value) in ps.fields_for_serialize() {
                        // Skip None values
                        if let Def::Option(_) = field_value.shape().def {
                            let opt = field_value.into_option().unwrap();
                            if let Some(inner) = opt.value() {
                                table.insert(field.name, serialize_value(inner)?);
                            }
                            // Skip None
                        } else {
                            table.insert(field.name, serialize_value(field_value)?);
                        }
                    }
                    Ok(Value::InlineTable(table))
                }
            }
        }
        (_, Type::User(UserType::Enum(_))) => {
            let pe = peek.into_enum().unwrap();
            let variant = pe.active_variant().expect("Failed to get active variant");
            trace!("Serializing enum variant: {}", variant.name);

            if variant.data.fields.is_empty() {
                // Unit variant - serialize as string
                Ok(Value::String(toml_edit::Formatted::new(
                    variant.name.to_string(),
                )))
            } else {
                // Variants with data - serialize as inline table with variant name as key
                let mut outer = InlineTable::new();
                let inner = if variant.data.kind == StructKind::Tuple
                    || variant.data.kind == StructKind::TupleStruct
                {
                    // Tuple variant - value is array
                    let fields: Vec<_> = pe.fields_for_serialize().collect();
                    if fields.len() == 1 {
                        // Newtype variant - just the inner value
                        serialize_value(fields[0].1)?
                    } else {
                        let mut array = Array::new();
                        for (_, field_value) in fields {
                            array.push(serialize_value(field_value)?);
                        }
                        Value::Array(array)
                    }
                } else {
                    // Struct variant - value is inline table
                    let mut table = InlineTable::new();
                    for (field, field_value) in pe.fields_for_serialize() {
                        table.insert(field.name, serialize_value(field_value)?);
                    }
                    Value::InlineTable(table)
                };
                outer.insert(variant.name, inner);
                Ok(Value::InlineTable(outer))
            }
        }
        (_, Type::Pointer(_)) => {
            // Handle string types
            if let Some(s) = peek.as_str() {
                Ok(Value::String(toml_edit::Formatted::new(s.to_string())))
            } else {
                let innermost = peek.innermost_peek();
                if innermost.shape() != peek.shape() {
                    serialize_value(innermost)
                } else {
                    Err(TomlSerError::UnsupportedPointer)
                }
            }
        }
        _ => {
            trace!("Unhandled type: {:?}", peek.shape().ty);
            Err(TomlSerError::UnsupportedType {
                type_name: alloc::format!("{:?}", peek.shape().ty),
            })
        }
    }
}

fn serialize_scalar(peek: Peek<'_, '_>) -> Result<Value, TomlSerError> {
    match peek.scalar_type() {
        Some(ScalarType::Unit) => Err(TomlSerError::UnsupportedUnit),
        Some(ScalarType::Bool) => {
            let v = *peek.get::<bool>().unwrap();
            Ok(Value::Boolean(toml_edit::Formatted::new(v)))
        }
        Some(ScalarType::Char) => {
            let c = *peek.get::<char>().unwrap();
            Ok(Value::String(toml_edit::Formatted::new(c.to_string())))
        }
        Some(ScalarType::Str) => {
            let s = peek.get::<str>().unwrap();
            Ok(Value::String(toml_edit::Formatted::new(s.to_string())))
        }
        Some(ScalarType::String) => {
            let s = peek.get::<String>().unwrap();
            Ok(Value::String(toml_edit::Formatted::new(s.clone())))
        }
        Some(ScalarType::CowStr) => {
            let s = peek.get::<Cow<'_, str>>().unwrap();
            Ok(Value::String(toml_edit::Formatted::new(s.to_string())))
        }
        Some(ScalarType::F32) => {
            let v = *peek.get::<f32>().unwrap();
            Ok(Value::Float(toml_edit::Formatted::new(v as f64)))
        }
        Some(ScalarType::F64) => {
            let v = *peek.get::<f64>().unwrap();
            Ok(Value::Float(toml_edit::Formatted::new(v)))
        }
        Some(ScalarType::U8) => {
            let v = *peek.get::<u8>().unwrap();
            Ok(Value::Integer(toml_edit::Formatted::new(v as i64)))
        }
        Some(ScalarType::U16) => {
            let v = *peek.get::<u16>().unwrap();
            Ok(Value::Integer(toml_edit::Formatted::new(v as i64)))
        }
        Some(ScalarType::U32) => {
            let v = *peek.get::<u32>().unwrap();
            Ok(Value::Integer(toml_edit::Formatted::new(v as i64)))
        }
        Some(ScalarType::U64) => {
            let v = *peek.get::<u64>().unwrap();
            let i = i64::try_from(v)
                .map_err(|_| TomlSerError::InvalidNumberToI64Conversion { source_type: "u64" })?;
            Ok(Value::Integer(toml_edit::Formatted::new(i)))
        }
        Some(ScalarType::U128) => {
            let v = *peek.get::<u128>().unwrap();
            let i = i64::try_from(v).map_err(|_| TomlSerError::InvalidNumberToI64Conversion {
                source_type: "u128",
            })?;
            Ok(Value::Integer(toml_edit::Formatted::new(i)))
        }
        Some(ScalarType::USize) => {
            let v = *peek.get::<usize>().unwrap();
            let i = i64::try_from(v).map_err(|_| TomlSerError::InvalidNumberToI64Conversion {
                source_type: "usize",
            })?;
            Ok(Value::Integer(toml_edit::Formatted::new(i)))
        }
        Some(ScalarType::I8) => {
            let v = *peek.get::<i8>().unwrap();
            Ok(Value::Integer(toml_edit::Formatted::new(v as i64)))
        }
        Some(ScalarType::I16) => {
            let v = *peek.get::<i16>().unwrap();
            Ok(Value::Integer(toml_edit::Formatted::new(v as i64)))
        }
        Some(ScalarType::I32) => {
            let v = *peek.get::<i32>().unwrap();
            Ok(Value::Integer(toml_edit::Formatted::new(v as i64)))
        }
        Some(ScalarType::I64) => {
            let v = *peek.get::<i64>().unwrap();
            Ok(Value::Integer(toml_edit::Formatted::new(v)))
        }
        Some(ScalarType::I128) => {
            let v = *peek.get::<i128>().unwrap();
            let i = i64::try_from(v).map_err(|_| TomlSerError::InvalidNumberToI64Conversion {
                source_type: "i128",
            })?;
            Ok(Value::Integer(toml_edit::Formatted::new(i)))
        }
        Some(ScalarType::ISize) => {
            let v = *peek.get::<isize>().unwrap();
            Ok(Value::Integer(toml_edit::Formatted::new(v as i64)))
        }
        Some(other) => Err(TomlSerError::UnsupportedScalarType {
            scalar_type: alloc::format!("{other:?}"),
        }),
        None => Err(TomlSerError::UnknownScalarShape {
            shape: alloc::format!("{}", peek.shape()),
        }),
    }
}
