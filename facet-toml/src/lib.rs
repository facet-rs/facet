#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

pub mod error;
mod to_scalar;

use core::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use error::AnyErr;
use facet_ansi::Stylize as _;
use facet_core::{Def, Facet, StructKind};
use facet_reflect::{ScalarType, Wip};
use log::trace;
use toml_edit::{DocumentMut, Item, TomlError};

/// Deserializes a TOML string into a value of type `T` that implements `Facet`.
pub fn from_str<T: Facet>(toml: &str) -> Result<T, AnyErr> {
    trace!("Starting deserialization");

    let wip = Wip::alloc::<T>();

    let docs: DocumentMut = toml.parse().map_err(|e| TomlError::to_string(&e))?;
    let wip = deserialize_item(wip, docs.as_item())?;

    let heap_value = wip.build().map_err(|e| AnyErr(e.to_string()))?;
    let result = heap_value
        .materialize::<T>()
        .map_err(|e| AnyErr(e.to_string()))?;

    trace!("Finished deserialization");

    Ok(result)
}

fn deserialize_item<'a>(wip: Wip<'a>, item: &Item) -> Result<Wip<'a>, AnyErr> {
    trace!("Deserializing {}", item.type_name().blue());

    match wip.shape().def {
        Def::Scalar(_) => deserialize_as_scalar(wip, item),
        Def::List(_) => deserialize_as_list(wip, item),
        Def::Map(_) => deserialize_as_map(wip, item),
        Def::Struct(_) => deserialize_as_struct(wip, item),
        Def::Enum(_) => deserialize_as_enum(wip, item),
        Def::Option(_) => deserialize_as_option(wip, item),
        Def::SmartPointer(_) => deserialize_as_smartpointer(wip, item),
        _ => Err(AnyErr(format!("Unsupported type: {:?}", wip.shape()))),
    }
}

fn deserialize_as_struct<'a>(mut wip: Wip<'a>, item: &Item) -> Result<Wip<'a>, AnyErr> {
    trace!("Deserializing {}", "struct".blue());

    // Parse as a the inner struct type if item is a single value and the struct is a unit struct
    if item.is_value() {
        // Only allow unit structs
        let shape = wip.shape();
        if let Def::Struct(def) = shape.def {
            if def.fields.len() > 1 {
                return Err(AnyErr(
                    "Failed trying to parse a single value as a struct with multiple fields".into(),
                ));
            }
        }

        let field_index = 0;
        wip = wip
            .field(field_index)
            .map_err(|e| AnyErr(format!("Unit struct is missing value: {}", e)))?;
        wip = deserialize_item(wip, item)?;
        wip = wip.pop().map_err(|e| AnyErr(e.to_string()))?;
        return Ok(wip);
    }

    // Otherwise we expect a table
    let table = item.as_table_like().ok_or_else(|| {
        AnyErr(format!(
            "Expected table like structure, got {}",
            item.type_name()
        ))
    })?;

    for (k, v) in table.iter() {
        let field_index = wip
            .field_index(k)
            .ok_or_else(|| AnyErr(format!("Field '{}' not found", k)))?;
        wip = wip
            .field(field_index)
            .map_err(|e| AnyErr(format!("Field '{}' error: {}", k, e)))?;
        wip = deserialize_item(wip, v)
            .map_err(|e| AnyErr(format!("Error deserializing field '{}': {}", k, e)))?;
        wip = wip.pop().map_err(|e| AnyErr(e.to_string()))?;
    }

    trace!("Finished deserializing {}", "struct".blue());

    Ok(wip)
}

fn deserialize_as_enum<'a>(wip: Wip<'a>, item: &Item) -> Result<Wip<'a>, AnyErr> {
    trace!("Deserializing {}", "enum".blue());

    let wip = match item {
        Item::None => todo!(),

        Item::Value(value) => {
            trace!("Entering {}", "value".cyan());

            // A value can be an inline table, so parse it as such
            if let Some(inline_table) = value.as_inline_table() {
                if let Some((key, field)) = inline_table.iter().next() {
                    trace!(
                        "Entering {} with key {}",
                        "inline table".cyan(),
                        key.cyan().bold()
                    );

                    if inline_table.len() > 1 {
                        return Err(AnyErr(
                            "Cannot parse enum from inline table because it got multiple fields"
                                .to_string(),
                        ));
                    } else {
                        return build_enum_from_variant_name(
                            wip,
                            key,
                            // TODO: remove clone
                            &Item::Value(field.clone()),
                        );
                    }
                } else {
                    return Err(AnyErr(
                        "Inline table doesn't have any fields to parse into enum variant"
                            .to_string(),
                    ));
                }
            }

            let variant_name = value
                .as_str()
                .ok_or_else(|| format!("Expected string, got: {}", value.type_name()))?;

            build_enum_from_variant_name(wip, variant_name, item)?
        }

        Item::Table(table) => {
            if let Some((key, field)) = table.iter().next() {
                trace!("Entering {} with key {}", "table".cyan(), key.cyan().bold());

                if table.len() > 1 {
                    return Err(AnyErr(
                        "Cannot parse enum from inline table because it got multiple fields"
                            .to_string(),
                    ));
                } else {
                    build_enum_from_variant_name(wip, key, field)?
                }
            } else {
                return Err(AnyErr(
                    "Inline table doesn't have any fields to parse into enum variant".to_string(),
                ));
            }
        }

        Item::ArrayOfTables(_array_of_tables) => todo!(),
    };

    trace!("Finished deserializing {}", "enum".blue());

    Ok(wip)
}

fn build_enum_from_variant_name<'a>(
    mut wip: Wip<'a>,
    variant_name: &str,
    item: &Item,
) -> Result<Wip<'a>, AnyErr> {
    // Select the variant
    wip = wip
        .variant_named(variant_name)
        .map_err(|e| AnyErr(e.to_string()))?;
    // Safe to unwrap because the variant got just selected
    let variant = wip.selected_variant().unwrap();

    if variant.data.kind == StructKind::Unit {
        // No need to do anything, we can just set the variant since it's a unit enum
        return Ok(wip);
    }

    // Push all fields
    for (index, field) in variant.data.fields.iter().enumerate() {
        wip = wip
            .field_named(field.name)
            .map_err(|e| format!("Field by name on enum does not exist: {e}"))?;

        // Try to get the TOML value as a table to extract the field
        if let Some(item) = item.as_table_like() {
            // Base the field name on what type of struct we are
            let field_name = if let StructKind::TupleStruct | StructKind::Tuple = variant.data.kind
            {
                &index.to_string()
            } else {
                // It must be a struct field
                field.name
            };

            // Try to get the TOML field matching the Rust name
            let Some(field) = item.get(field_name) else {
                return Err(format!("TOML field '{}' not found", field_name).into());
            };

            wip = deserialize_item(wip, field)?;

            wip = wip.pop().map_err(|e| AnyErr(e.to_string()))?;
        } else if item.is_value() {
            wip = deserialize_item(wip, item)?;
        } else {
            return Err(format!("TOML {} is not a recognized type", item.type_name()).into());
        }
    }

    Ok(wip)
}

fn deserialize_as_list<'a>(mut _wip: Wip<'a>, _item: &Item) -> Result<Wip<'a>, AnyErr> {
    trace!("Deserializing {}", "array".blue());

    trace!("Finished deserializing {}", "array".blue());

    todo!();
}

fn deserialize_as_map<'a>(mut _wip: Wip<'a>, _item: &Item) -> Result<Wip<'a>, AnyErr> {
    trace!("Deserializing {}", "ap".blue());

    trace!("Finished deserializing {}", "ap".blue());

    todo!();
}

fn deserialize_as_option<'a>(mut _wip: Wip<'a>, _item: &Item) -> Result<Wip<'a>, AnyErr> {
    trace!("Deserializing {}", "ption".blue());

    trace!("Finished deserializing {}", "ption".blue());

    todo!();
}

fn deserialize_as_smartpointer<'a>(mut _wip: Wip<'a>, _item: &Item) -> Result<Wip<'a>, AnyErr> {
    trace!("Deserializing {}", "smart".blue());

    trace!("Finished deserializing {}", "smart".blue());

    todo!();
}

fn deserialize_as_scalar<'a>(mut wip: Wip<'a>, item: &Item) -> Result<Wip<'a>, AnyErr> {
    trace!("Deserializing {}", "scalar".blue());

    match ScalarType::try_from_shape(wip.shape())
        .ok_or_else(|| format!("Unsupported scalar type: {}", wip.shape()))?
    {
        ScalarType::Bool => {
            wip = wip
                .put(to_scalar::boolean(item)?)
                .map_err(|e| AnyErr(e.to_string()))?
        }
        #[cfg(feature = "std")]
        ScalarType::String => {
            wip = wip
                .put(to_scalar::string(item)?)
                .map_err(|e| AnyErr(e.to_string()))?
        }
        #[cfg(feature = "std")]
        ScalarType::CowStr => {
            wip = wip
                .put(std::borrow::Cow::Owned(to_scalar::string(item)?))
                .map_err(|e| AnyErr(e.to_string()))?
        }
        ScalarType::F32 => {
            wip = wip
                .put(to_scalar::number::<f32>(item)?)
                .map_err(|e| AnyErr(e.to_string()))?
        }
        ScalarType::F64 => {
            wip = wip
                .put(to_scalar::number::<f64>(item)?)
                .map_err(|e| AnyErr(e.to_string()))?
        }
        ScalarType::U8 => {
            wip = wip
                .put(to_scalar::number::<u8>(item)?)
                .map_err(|e| AnyErr(e.to_string()))?
        }
        ScalarType::U16 => {
            wip = wip
                .put(to_scalar::number::<u16>(item)?)
                .map_err(|e| AnyErr(e.to_string()))?
        }
        ScalarType::U32 => {
            wip = wip
                .put(to_scalar::number::<u32>(item)?)
                .map_err(|e| AnyErr(e.to_string()))?
        }
        ScalarType::U64 => {
            wip = wip
                .put(to_scalar::number::<u64>(item)?)
                .map_err(|e| AnyErr(e.to_string()))?
        }
        ScalarType::USize => {
            wip = wip
                .put(to_scalar::number::<usize>(item)?)
                .map_err(|e| AnyErr(e.to_string()))?
        }
        ScalarType::I8 => {
            wip = wip
                .put(to_scalar::number::<i8>(item)?)
                .map_err(|e| AnyErr(e.to_string()))?
        }
        ScalarType::I16 => {
            wip = wip
                .put(to_scalar::number::<i16>(item)?)
                .map_err(|e| AnyErr(e.to_string()))?
        }
        ScalarType::I32 => {
            wip = wip
                .put(to_scalar::number::<i32>(item)?)
                .map_err(|e| AnyErr(e.to_string()))?
        }
        ScalarType::I64 => {
            wip = wip
                .put(to_scalar::number::<i64>(item)?)
                .map_err(|e| AnyErr(e.to_string()))?
        }
        ScalarType::ISize => {
            wip = wip
                .put(to_scalar::number::<isize>(item)?)
                .map_err(|e| AnyErr(e.to_string()))?
        }
        #[cfg(feature = "std")]
        ScalarType::SocketAddr => {
            wip = wip
                .put(to_scalar::from_str::<std::net::SocketAddr>(
                    item,
                    "socket address",
                )?)
                .map_err(|e| AnyErr(e.to_string()))?
        }
        ScalarType::IpAddr => {
            wip = wip
                .put(to_scalar::from_str::<IpAddr>(item, "ip address")?)
                .map_err(|e| AnyErr(e.to_string()))?
        }
        ScalarType::Ipv4Addr => {
            wip = wip
                .put(to_scalar::from_str::<Ipv4Addr>(item, "ipv4 address")?)
                .map_err(|e| AnyErr(e.to_string()))?
        }
        ScalarType::Ipv6Addr => {
            wip = wip
                .put(to_scalar::from_str::<Ipv6Addr>(item, "ipv6 address")?)
                .map_err(|e| AnyErr(e.to_string()))?
        }
        _ => return Err(AnyErr(format!("Unsupported scalar type: {}", wip.shape()))),
    }

    trace!("Finished deserializing {}", "scalar".blue());

    Ok(wip)
}
