//! Create and/or write TOML strings from Rust values.

#[cfg(not(feature = "alloc"))]
compile_error!("feature `alloc` is required");

mod error;

use alloc::{borrow::Cow, string::String, vec::Vec};
use core::fmt::Write;
use facet_core::{Def, Facet, StructKind, Type, UserType};
use facet_reflect::{HasFields, Peek, PeekListLike, PeekStruct, ScalarType};
use log::trace;
use toml_writer::TomlWrite;

pub use error::TomlSerError;

/// Serialize any `Facet` type to a TOML string.
#[cfg(feature = "alloc")]
pub fn to_string<T: Facet<'static>>(value: &T) -> Result<String, TomlSerError> {
    let peek = Peek::new(value);

    // Check if the root is a struct with fields that are arrays of tables
    if let Ok(struct_peek) = peek.into_struct() {
        let mut output = String::new();
        serialize_root_struct(struct_peek, &mut output)?;
        Ok(output)
    } else {
        // Not a struct at root - TOML requires root to be a table
        Err(TomlSerError::RootMustBeStruct)
    }
}

/// Serialize any `Facet` type to a "pretty" TOML string.
///
/// Note: TOML is already a fairly readable format. This function currently
/// produces the same output as `to_string`. Future versions may add enhanced
/// formatting with table headers (e.g., `[section]`) instead of inline tables.
#[cfg(feature = "alloc")]
pub fn to_string_pretty<T: Facet<'static>>(value: &T) -> Result<String, TomlSerError> {
    // For now, TOML output is already fairly readable.
    // A future enhancement could use table headers instead of inline tables.
    to_string(value)
}

/// Serialize a root struct to TOML output
fn serialize_root_struct<'mem, 'facet, W: Write>(
    struct_peek: PeekStruct<'mem, 'facet>,
    output: &mut W,
) -> Result<(), TomlSerError> {
    // Collect fields into categories: simple key-value pairs vs array-of-tables
    let mut simple_fields: Vec<(&str, Peek<'mem, 'facet>)> = Vec::new();
    let mut aot_fields: Vec<(&str, PeekListLike<'mem, 'facet>)> = Vec::new();

    for (field, field_value) in struct_peek.fields_for_serialize() {
        // Handle Option fields
        let value_to_check = if let Def::Option(_) = field_value.shape().def {
            let opt = field_value.into_option().unwrap();
            if let Some(inner) = opt.value() {
                if is_unit_like(&inner) {
                    continue;
                }
                inner
            } else {
                // Skip None
                continue;
            }
        } else {
            if is_unit_like(&field_value) {
                continue;
            }
            field_value
        };

        // Check if this is an array of tables
        if is_array_of_tables(&value_to_check) {
            let list = value_to_check.into_list_like().unwrap();
            aot_fields.push((field.name, list));
        } else {
            simple_fields.push((field.name, value_to_check));
        }
    }

    // Write simple key-value pairs first
    let has_simple_fields = !simple_fields.is_empty();
    for (name, value) in simple_fields {
        output.key(name)?;
        output.space()?;
        output.keyval_sep()?;
        output.space()?;
        serialize_value(value, output)?;
        output.newline()?;
    }

    // Write array-of-tables after simple fields
    let mut is_first_aot = !has_simple_fields;
    for (name, list) in aot_fields {
        serialize_array_of_tables(name, list, output, is_first_aot)?;
        is_first_aot = false;
    }

    Ok(())
}

/// Check if a Peek value represents a unit type ((), unit struct, or empty tuple)
/// or a list/array of unit types (which also can't be represented in TOML)
fn is_unit_like(peek: &Peek<'_, '_>) -> bool {
    if let Some(ScalarType::Unit) = peek.scalar_type() {
        return true;
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
            if let Type::User(UserType::Struct(sd)) = ld.t().ty
                && sd.kind == StructKind::Unit
            {
                return true;
            }
        }
        (Def::Array(ad), _) => {
            if let Type::User(UserType::Struct(sd)) = ad.t().ty
                && sd.kind == StructKind::Unit
            {
                return true;
            }
        }
        _ => {}
    }
    false
}

/// Check if a Peek value represents an array of structs/tables
fn is_array_of_tables(peek: &Peek) -> bool {
    match peek.shape().def {
        Def::List(ld) => {
            // Check if the element type is a struct (not tuple or unit)
            matches!(
                ld.t().ty,
                Type::User(UserType::Struct(sd)) if !matches!(sd.kind, StructKind::Tuple | StructKind::Unit)
            )
        }
        Def::Array(ad) => {
            // Check if the element type is a struct (not tuple or unit)
            matches!(
                ad.t().ty,
                Type::User(UserType::Struct(sd)) if !matches!(sd.kind, StructKind::Tuple | StructKind::Unit)
            )
        }
        _ => false,
    }
}

/// Serialize an array of tables using `[[name]]` syntax
fn serialize_array_of_tables<'mem, 'facet, W: Write>(
    name: &str,
    list: PeekListLike<'mem, 'facet>,
    output: &mut W,
    is_first: bool,
) -> Result<(), TomlSerError> {
    let mut is_first_item = is_first;
    for item in list.iter() {
        if let Ok(struct_peek) = item.into_struct() {
            // Write [[name]] header
            if !is_first_item {
                output.newline()?;
            }
            is_first_item = false;
            output.open_array_of_tables_header()?;
            output.key(name)?;
            output.close_array_of_tables_header()?;
            output.newline()?;

            // Write struct fields
            serialize_struct_fields(struct_peek, output)?;
        } else {
            return Err(TomlSerError::InvalidArrayOfTables);
        }
    }
    Ok(())
}

/// Serialize struct fields as key-value pairs
fn serialize_struct_fields<'mem, 'facet, W: Write>(
    struct_peek: PeekStruct<'mem, 'facet>,
    output: &mut W,
) -> Result<(), TomlSerError> {
    for (field, field_value) in struct_peek.fields_for_serialize() {
        // Handle Option fields
        let value_to_serialize = if let Def::Option(_) = field_value.shape().def {
            let opt = field_value.into_option().unwrap();
            if let Some(inner) = opt.value() {
                inner
            } else {
                // Skip None
                continue;
            }
        } else {
            field_value
        };

        output.key(field.name)?;
        output.space()?;
        output.keyval_sep()?;
        output.space()?;
        serialize_value(value_to_serialize, output)?;
        output.newline()?;
    }
    Ok(())
}

/// Serialize a Peek value to TOML output
fn serialize_value<W: Write>(peek: Peek<'_, '_>, output: &mut W) -> Result<(), TomlSerError> {
    trace!("Serializing value, shape is {}", peek.shape());

    match (peek.shape().def, peek.shape().ty) {
        (Def::Scalar, _) => {
            let peek = peek.innermost_peek();
            serialize_scalar(peek, output)
        }
        (Def::List(_), _) | (Def::Array(_), _) | (Def::Slice(_), _) => {
            let list = peek.into_list_like().unwrap();
            output.open_array()?;
            let mut first = true;
            for item in list.iter() {
                if !first {
                    output.val_sep()?;
                    output.space()?;
                }
                first = false;
                serialize_value(item, output)?;
            }
            output.close_array()?;
            Ok(())
        }
        (Def::Map(_), _) => {
            let map = peek.into_map().unwrap();
            output.open_inline_table()?;
            let mut first = true;
            for (key, value) in map.iter() {
                let key_str = key.as_str().ok_or(TomlSerError::InvalidKeyConversion {
                    toml_type: "non-string map key",
                })?;
                if !first {
                    output.val_sep()?;
                }
                output.space()?;
                output.key(key_str)?;
                output.space()?;
                output.keyval_sep()?;
                output.space()?;
                serialize_value(value, output)?;
                first = false;
            }
            if !first {
                output.space()?;
            }
            output.close_inline_table()?;
            Ok(())
        }
        (Def::Set(_), _) => {
            let set = peek.into_set().unwrap();
            output.open_array()?;
            let mut first = true;
            for item in set.iter() {
                if !first {
                    output.val_sep()?;
                    output.space()?;
                }
                first = false;
                serialize_value(item, output)?;
            }
            output.close_array()?;
            Ok(())
        }
        (Def::Option(_), _) => {
            let opt = peek.into_option().unwrap();
            if let Some(inner) = opt.value() {
                serialize_value(inner, output)
            } else {
                // TOML doesn't have null, so we skip None values at the field level
                // But if we're serializing a standalone Option, we need to handle it
                Err(TomlSerError::UnsupportedNone)
            }
        }
        (Def::Pointer(_), _) => {
            let ptr = peek.into_pointer().unwrap();
            if let Some(inner) = ptr.borrow_inner() {
                serialize_value(inner, output)
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
                            serialize_value(fields[0].1, output)
                        }
                        _ => {
                            // Multi-field tuple structs serialize as arrays
                            output.open_array()?;
                            let mut first = true;
                            for (_, field_value) in fields {
                                if !first {
                                    output.val_sep()?;
                                    output.space()?;
                                }
                                first = false;
                                serialize_value(field_value, output)?;
                            }
                            output.close_array()?;
                            Ok(())
                        }
                    }
                }
                StructKind::Struct => {
                    // Regular structs serialize as inline tables
                    let ps = peek.into_struct().unwrap();
                    output.open_inline_table()?;
                    let mut first = true;
                    for (field, field_value) in ps.fields_for_serialize() {
                        // Handle Option fields - skip None values
                        let value_to_serialize = if let Def::Option(_) = field_value.shape().def {
                            field_value.into_option().unwrap().value()
                        } else {
                            Some(field_value)
                        };

                        if let Some(value) = value_to_serialize {
                            if !first {
                                output.val_sep()?;
                            }
                            output.space()?;
                            output.key(field.name)?;
                            output.space()?;
                            output.keyval_sep()?;
                            output.space()?;
                            serialize_value(value, output)?;
                            first = false;
                        }
                    }
                    if !first {
                        output.space()?;
                    }
                    output.close_inline_table()?;
                    Ok(())
                }
            }
        }
        (_, Type::User(UserType::Enum(_))) => {
            let pe = peek.into_enum().unwrap();
            let variant = pe.active_variant().expect("Failed to get active variant");
            trace!("Serializing enum variant: {}", variant.name);

            if variant.data.fields.is_empty() {
                // Unit variant - serialize as string
                output.value(variant.name)?;
                Ok(())
            } else {
                // Variants with data - serialize as inline table with variant name as key
                output.open_inline_table()?;
                output.space()?;
                output.key(variant.name)?;
                output.space()?;
                output.keyval_sep()?;
                output.space()?;

                if variant.data.kind == StructKind::Tuple
                    || variant.data.kind == StructKind::TupleStruct
                {
                    // Tuple variant - value is array
                    let fields: Vec<_> = pe.fields_for_serialize().collect();
                    if fields.len() == 1 {
                        // Newtype variant - just the inner value
                        serialize_value(fields[0].1, output)?;
                    } else {
                        output.open_array()?;
                        let mut first = true;
                        for (_, field_value) in fields {
                            if !first {
                                output.val_sep()?;
                                output.space()?;
                            }
                            first = false;
                            serialize_value(field_value, output)?;
                        }
                        output.close_array()?;
                    }
                } else {
                    // Struct variant - value is inline table
                    output.open_inline_table()?;
                    let mut first = true;
                    for (field, field_value) in pe.fields_for_serialize() {
                        if !first {
                            output.val_sep()?;
                        }
                        output.space()?;
                        output.key(field.name)?;
                        output.space()?;
                        output.keyval_sep()?;
                        output.space()?;
                        serialize_value(field_value, output)?;
                        first = false;
                    }
                    if !first {
                        output.space()?;
                    }
                    output.close_inline_table()?;
                }
                output.space()?;
                output.close_inline_table()?;
                Ok(())
            }
        }
        (_, Type::Pointer(_)) => {
            // Handle string types
            if let Some(s) = peek.as_str() {
                output.value(s)?;
                Ok(())
            } else {
                let innermost = peek.innermost_peek();
                if innermost.shape() != peek.shape() {
                    serialize_value(innermost, output)
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

fn serialize_scalar<W: Write>(peek: Peek<'_, '_>, output: &mut W) -> Result<(), TomlSerError> {
    match peek.scalar_type() {
        Some(ScalarType::Unit) => Err(TomlSerError::UnsupportedUnit),
        Some(ScalarType::Bool) => {
            let v = *peek.get::<bool>().unwrap();
            output.value(v)?;
            Ok(())
        }
        Some(ScalarType::Char) => {
            let c = *peek.get::<char>().unwrap();
            output.value(c)?;
            Ok(())
        }
        Some(ScalarType::Str) => {
            let s = peek.get::<str>().unwrap();
            output.value(s)?;
            Ok(())
        }
        Some(ScalarType::String) => {
            let s = peek.get::<String>().unwrap();
            output.value(s.as_str())?;
            Ok(())
        }
        Some(ScalarType::CowStr) => {
            let s = peek.get::<Cow<'_, str>>().unwrap();
            output.value(s.as_ref())?;
            Ok(())
        }
        Some(ScalarType::F32) => {
            let v = *peek.get::<f32>().unwrap();
            output.value(v)?;
            Ok(())
        }
        Some(ScalarType::F64) => {
            let v = *peek.get::<f64>().unwrap();
            output.value(v)?;
            Ok(())
        }
        Some(ScalarType::U8) => {
            let v = *peek.get::<u8>().unwrap();
            output.value(v)?;
            Ok(())
        }
        Some(ScalarType::U16) => {
            let v = *peek.get::<u16>().unwrap();
            output.value(v)?;
            Ok(())
        }
        Some(ScalarType::U32) => {
            let v = *peek.get::<u32>().unwrap();
            output.value(v)?;
            Ok(())
        }
        Some(ScalarType::U64) => {
            let v = *peek.get::<u64>().unwrap();
            // TOML spec requires integers to fit in i64 range
            if v > i64::MAX as u64 {
                return Err(TomlSerError::InvalidNumberToI64Conversion { source_type: "u64" });
            }
            output.value(v)?;
            Ok(())
        }
        Some(ScalarType::U128) => {
            let v = *peek.get::<u128>().unwrap();
            // TOML spec requires integers to fit in i64 range
            if v > i64::MAX as u128 {
                return Err(TomlSerError::InvalidNumberToI64Conversion {
                    source_type: "u128",
                });
            }
            output.value(v)?;
            Ok(())
        }
        Some(ScalarType::USize) => {
            let v = *peek.get::<usize>().unwrap();
            output.value(v as u64)?;
            Ok(())
        }
        Some(ScalarType::I8) => {
            let v = *peek.get::<i8>().unwrap();
            output.value(v)?;
            Ok(())
        }
        Some(ScalarType::I16) => {
            let v = *peek.get::<i16>().unwrap();
            output.value(v)?;
            Ok(())
        }
        Some(ScalarType::I32) => {
            let v = *peek.get::<i32>().unwrap();
            output.value(v)?;
            Ok(())
        }
        Some(ScalarType::I64) => {
            let v = *peek.get::<i64>().unwrap();
            output.value(v)?;
            Ok(())
        }
        Some(ScalarType::I128) => {
            let v = *peek.get::<i128>().unwrap();
            // TOML spec requires integers to fit in i64 range
            if v > i64::MAX as i128 || v < i64::MIN as i128 {
                return Err(TomlSerError::InvalidNumberToI64Conversion {
                    source_type: "i128",
                });
            }
            output.value(v)?;
            Ok(())
        }
        Some(ScalarType::ISize) => {
            let v = *peek.get::<isize>().unwrap();
            output.value(v as i64)?;
            Ok(())
        }
        Some(other) => Err(TomlSerError::UnsupportedScalarType {
            scalar_type: alloc::format!("{other:?}"),
        }),
        None => Err(TomlSerError::UnknownScalarShape {
            shape: alloc::format!("{}", peek.shape()),
        }),
    }
}
