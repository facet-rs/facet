#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

pub mod error;
mod to_scalar;

use core::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use error::{TomlError, TomlErrorKind};
use facet_core::{Def, Facet, Struct, StructKind};
use facet_reflect::{ScalarType, Wip};
use log::trace;
use toml_edit::{DocumentMut, Item};
use yansi::Paint as _;

/// Deserializes a TOML string into a value of type `T` that implements `Facet`.
pub fn from_str<'input, T: Facet>(toml: &'input str) -> Result<T, TomlError<'input>> {
    trace!("Starting deserialization");

    let wip = Wip::alloc::<T>();

    let docs: DocumentMut = toml
        .parse()
        .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericTomlError(e), None))?;
    let wip = deserialize_item(toml, wip, docs.as_item())?;

    let heap_value = wip
        .build()
        .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), None))?;
    let result = heap_value
        .materialize::<T>()
        .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), None))?;

    trace!("Finished deserialization");

    Ok(result)
}

fn deserialize_item<'input, 'a>(
    toml: &'input str,
    wip: Wip<'a>,
    item: &Item,
) -> Result<Wip<'a>, TomlError<'input>> {
    trace!("Deserializing {}", item.type_name().blue());

    match wip.shape().def {
        Def::Scalar(_) => deserialize_as_scalar(toml, wip, item),
        Def::List(_) => deserialize_as_list(toml, wip, item),
        Def::Map(_) => deserialize_as_map(toml, wip, item),
        Def::Struct(def) => deserialize_as_struct(toml, wip, def, item),
        Def::Enum(_) => deserialize_as_enum(toml, wip, item),
        Def::Option(_) => deserialize_as_option(toml, wip, item),
        Def::SmartPointer(_) => deserialize_as_smartpointer(toml, wip, item),
        _ => todo!(),
    }
}

fn deserialize_as_struct<'input, 'a>(
    toml: &'input str,
    mut wip: Wip<'a>,
    def: Struct,
    item: &Item,
) -> Result<Wip<'a>, TomlError<'input>> {
    trace!("Deserializing {}", "struct".blue());

    // Parse as a the inner struct type if item is a single value and the struct is a unit struct
    if item.is_value() && !item.is_inline_table() {
        // Only allow unit structs
        let shape = wip.shape();
        if let Def::Struct(def) = shape.def {
            if def.fields.len() > 1 {
                return Err(TomlError::new(
                    toml,
                    TomlErrorKind::ParseSingleValueAsMultipleFieldStruct,
                    item.span(),
                ));
            }
        }

        wip = wip
            .field(0)
            .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), item.span()))?;

        wip = deserialize_item(toml, wip, item)?;

        wip = wip
            .pop()
            .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), item.span()))?;

        return Ok(wip);
    }

    // Otherwise we expect a table
    let table = item.as_table_like().ok_or_else(|| {
        TomlError::new(
            toml,
            TomlErrorKind::ExpectedType {
                expected: "table like structure",
                got: item.type_name(),
            },
            item.span(),
        )
    })?;

    for field in def.fields {
        wip = wip
            .field_named(field.name)
            .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), item.span()))?;

        // Find the matching TOML field
        let field_item = table.get(field.name);
        match field_item {
            Some(field_item) => wip = deserialize_item(toml, wip, field_item)?,
            None => {
                if let Def::Option(..) = field.shape().def {
                    // Default of `Option<T>` is `None`
                    wip = wip.put_default().map_err(|e| {
                        TomlError::new(toml, TomlErrorKind::GenericReflect(e), item.span())
                    })?;
                } else {
                    return Err(TomlError(format!("Expected field '{}'", field.name)));
                }
            }
        }
        wip = wip
            .pop()
            .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), item.span()))?;
    }

    trace!("Finished deserializing {}", "struct".blue());

    Ok(wip)
}

fn deserialize_as_enum<'input, 'a>(
    toml: &'input str,
    wip: Wip<'a>,
    item: &Item,
) -> Result<Wip<'a>, TomlError<'input>> {
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
                        return Err(TomlError(
                            "Cannot parse enum from inline table because it got multiple fields"
                                .to_string(),
                        ));
                    } else {
                        return build_enum_from_variant_name(
                            toml,
                            wip,
                            key,
                            // TODO: remove clone
                            &Item::Value(field.clone()),
                        );
                    }
                } else {
                    return Err(TomlError(
                        "Inline table doesn't have any fields to parse into enum variant"
                            .to_string(),
                    ));
                }
            }

            let variant_name = value
                .as_str()
                .ok_or_else(|| format!("Expected string, got: {}", value.type_name()))?;

            build_enum_from_variant_name(toml, wip, variant_name, item)?
        }

        Item::Table(table) => {
            if let Some((key, field)) = table.iter().next() {
                trace!("Entering {} with key {}", "table".cyan(), key.cyan().bold());

                if table.len() > 1 {
                    return Err(TomlError(
                        "Cannot parse enum from inline table because it got multiple fields"
                            .to_string(),
                    ));
                } else {
                    build_enum_from_variant_name(toml, wip, key, field)?
                }
            } else {
                return Err(TomlError(
                    "Inline table doesn't have any fields to parse into enum variant".to_string(),
                ));
            }
        }

        Item::ArrayOfTables(_array_of_tables) => todo!(),
    };

    trace!("Finished deserializing {}", "enum".blue());

    Ok(wip)
}

fn build_enum_from_variant_name<'input, 'a>(
    toml: &'input str,
    mut wip: Wip<'a>,
    variant_name: &str,
    item: &Item,
) -> Result<Wip<'a>, TomlError<'input>> {
    // Select the variant
    wip = wip
        .variant_named(variant_name)
        .map_err(|e| TomlError(e.to_string()))?;
    // Safe to unwrap because the variant got just selected
    let variant = wip.selected_variant().unwrap();

    if variant.data.kind == StructKind::Unit {
        // No need to do anything, we can just set the variant since it's a unit enum
        return Ok(wip);
    }

    // Whether it's a tuple so we need to use the index
    let is_tuple =
        variant.data.kind == StructKind::TupleStruct || variant.data.kind == StructKind::Tuple;

    // Push all fields
    for (index, field) in variant.data.fields.iter().enumerate() {
        wip = wip
            .field_named(field.name)
            .map_err(|e| format!("Field by name on enum does not exist: {e}"))?;

        // Try to get the TOML value as a table to extract the field
        if let Some(item) = item.as_table_like() {
            // Base the field name on what type of struct we are
            let field_name = if is_tuple {
                &index.to_string()
            } else {
                // It must be a struct field
                field.name
            };

            // Try to get the TOML field matching the Rust name
            match item.get(field_name) {
                // Field found, push it
                Some(field) => {
                    wip = deserialize_item(toml, wip, field)?;
                }
                // Push none if field not found and it's an option
                None if matches!(field.shape().def, Def::Option(_)) => {
                    // Default of `Option<T>` is `None`
                    wip = wip.put_default().map_err(|e| TomlError(e.to_string()))?;
                }
                None => {
                    return Err(format!("TOML field '{}' not found", field_name).into());
                }
            }
        } else if item.is_value() {
            wip = deserialize_item(toml, wip, item)?;
        } else {
            return Err(format!("TOML {} is not a recognized type", item.type_name()).into());
        }

        wip = wip.pop().map_err(|e| e.to_string())?;
    }

    Ok(wip)
}

fn deserialize_as_list<'input, 'a>(
    toml: &'input str,
    mut wip: Wip<'a>,
    item: &Item,
) -> Result<Wip<'a>, TomlError<'input>> {
    trace!("Deserializing {}", "list".blue());

    // Get the TOML item as an array
    let Some(item) = item.as_array() else {
        return Err(TomlError(format!(
            "Item is not an array but a {}",
            item.type_name()
        )));
    };

    if item.is_empty() {
        // Only put an empty list
        return wip
            .put_empty_list()
            .map_err(|e| TomlError(format!("Can not put empty list: {e}")));
    }

    // Start the list
    wip = wip
        .begin_pushback()
        .map_err(|e| format!("Can not start filling list: {e}"))?;

    // Loop over all items in the TOML list
    for value in item.iter() {
        // Start the field
        wip = wip
            .push()
            .map_err(|e| format!("Can not start field: {e}"))?;

        wip = deserialize_item(
            wip,
            // TODO: remove clone
            &Item::Value(value.clone()),
        )?;

        // Finish the field
        wip = wip
            .pop()
            .map_err(|e| format!("Can not finish field: {e}"))?;
    }

    trace!("Finished deserializing {}", "list".blue());

    Ok(wip)
}

fn deserialize_as_map<'input, 'a>(
    toml: &'input str,
    mut wip: Wip<'a>,
    item: &Item,
) -> Result<Wip<'a>, TomlError<'input>> {
    trace!("Deserializing {}", "map".blue());

    // We expect a table to fill a map
    let table = item.as_table_like().ok_or_else(|| {
        TomlError(format!(
            "Expected table like structure, got {}",
            item.type_name()
        ))
    })?;

    if table.is_empty() {
        // Only put an empty map
        return wip
            .put_empty_map()
            .map_err(|e| TomlError(format!("Can not put empty map: {e}")));
    }

    // Start the map
    wip = wip
        .begin_map_insert()
        .map_err(|e| format!("Can not start filling map: {e}"))?;

    // Loop over all items in the TOML list
    for (k, v) in table.iter() {
        // Start the key
        wip = wip
            .push_map_key()
            .map_err(|e| format!("Can not start field: {e}"))?;

        trace!("Push {} {}", "key".cyan(), k.cyan().bold());

        // Deserialize the key
        match ScalarType::try_from_shape(wip.shape())
            .ok_or_else(|| format!("Unsupported scalar type: {}", wip.shape()))?
        {
            #[cfg(feature = "std")]
            ScalarType::String => {
                wip = wip
                    .put(k.to_string())
                    .map_err(|e| TomlError(e.to_string()))?
            }
            #[cfg(feature = "std")]
            ScalarType::CowStr => {
                wip = wip
                    .put(std::borrow::Cow::Owned(k.to_string()))
                    .map_err(|e| TomlError(e.to_string()))?
            }
            _ => {
                return Err(TomlError(format!(
                    "Can not convert {} to map key",
                    wip.shape()
                )));
            }
        };

        trace!("Push {}", "value".cyan());

        // Start the value
        wip = wip
            .push_map_value()
            .map_err(|e| format!("Can not start value: {e}"))?;

        // Deserialize the value
        wip = deserialize_item(toml, wip, v)?;

        // Finish the key
        wip = wip
            .pop()
            .map_err(|e| format!("Can not finish value: {e}"))?;
    }

    trace!("Finished deserializing {}", "map".blue());

    Ok(wip)
}

fn deserialize_as_option<'input, 'a>(
    toml: &'input str,
    mut wip: Wip<'a>,
    item: &Item,
) -> Result<Wip<'a>, TomlError<'input>> {
    trace!("Deserializing {}", "option".blue());

    wip = wip.push_some().map_err(|e| e.to_string())?;

    wip = deserialize_item(toml, wip, item)?;

    wip = wip.pop().map_err(|e| e.to_string())?;

    trace!("Finished deserializing {}", "option".blue());

    Ok(wip)
}

fn deserialize_as_smartpointer<'input, 'a>(
    _toml: &'input str,
    mut _wip: Wip<'a>,
    _item: &Item,
) -> Result<Wip<'a>, TomlError<'input>> {
    trace!("Deserializing {}", "smart pointer".blue());

    trace!("Finished deserializing {}", "smart pointer".blue());

    todo!();
}

fn deserialize_as_scalar<'input, 'a>(
    toml: &'input str,
    mut wip: Wip<'a>,
    item: &Item,
) -> Result<Wip<'a>, TomlError<'input>> {
    trace!("Deserializing {}", "scalar".blue());

    match ScalarType::try_from_shape(wip.shape())
        .ok_or_else(|| format!("Unsupported scalar type: {}", wip.shape()))?
    {
        ScalarType::Bool => {
            wip = wip
                .put(to_scalar::boolean(toml, item)?)
                .map_err(|e| TomlError(e.to_string()))?
        }
        #[cfg(feature = "std")]
        ScalarType::String => {
            wip = wip
                .put(to_scalar::string(toml, item)?)
                .map_err(|e| TomlError(e.to_string()))?
        }
        #[cfg(feature = "std")]
        ScalarType::CowStr => {
            wip = wip
                .put(std::borrow::Cow::Owned(to_scalar::string(toml, item)?))
                .map_err(|e| TomlError(e.to_string()))?
        }
        ScalarType::F32 => {
            wip = wip
                .put(to_scalar::number::<f32>(item)?)
                .map_err(|e| TomlError(e.to_string()))?
        }
        ScalarType::F64 => {
            wip = wip
                .put(to_scalar::number::<f64>(item)?)
                .map_err(|e| TomlError(e.to_string()))?
        }
        ScalarType::U8 => {
            wip = wip
                .put(to_scalar::number::<u8>(item)?)
                .map_err(|e| TomlError(e.to_string()))?
        }
        ScalarType::U16 => {
            wip = wip
                .put(to_scalar::number::<u16>(item)?)
                .map_err(|e| TomlError(e.to_string()))?
        }
        ScalarType::U32 => {
            wip = wip
                .put(to_scalar::number::<u32>(item)?)
                .map_err(|e| TomlError(e.to_string()))?
        }
        ScalarType::U64 => {
            wip = wip
                .put(to_scalar::number::<u64>(item)?)
                .map_err(|e| TomlError(e.to_string()))?
        }
        ScalarType::USize => {
            wip = wip
                .put(to_scalar::number::<usize>(item)?)
                .map_err(|e| TomlError(e.to_string()))?
        }
        ScalarType::I8 => {
            wip = wip
                .put(to_scalar::number::<i8>(item)?)
                .map_err(|e| TomlError(e.to_string()))?
        }
        ScalarType::I16 => {
            wip = wip
                .put(to_scalar::number::<i16>(item)?)
                .map_err(|e| TomlError(e.to_string()))?
        }
        ScalarType::I32 => {
            wip = wip
                .put(to_scalar::number::<i32>(item)?)
                .map_err(|e| TomlError(e.to_string()))?
        }
        ScalarType::I64 => {
            wip = wip
                .put(to_scalar::number::<i64>(item)?)
                .map_err(|e| TomlError(e.to_string()))?
        }
        ScalarType::ISize => {
            wip = wip
                .put(to_scalar::number::<isize>(item)?)
                .map_err(|e| TomlError(e.to_string()))?
        }
        #[cfg(feature = "std")]
        ScalarType::SocketAddr => {
            wip = wip
                .put(to_scalar::from_str::<std::net::SocketAddr>(
                    item,
                    "socket address",
                )?)
                .map_err(|e| TomlError(e.to_string()))?
        }
        ScalarType::IpAddr => {
            wip = wip
                .put(to_scalar::from_str::<IpAddr>(item, "ip address")?)
                .map_err(|e| TomlError(e.to_string()))?
        }
        ScalarType::Ipv4Addr => {
            wip = wip
                .put(to_scalar::from_str::<Ipv4Addr>(item, "ipv4 address")?)
                .map_err(|e| TomlError(e.to_string()))?
        }
        ScalarType::Ipv6Addr => {
            wip = wip
                .put(to_scalar::from_str::<Ipv6Addr>(item, "ipv6 address")?)
                .map_err(|e| TomlError(e.to_string()))?
        }
        _ => {
            return Err(TomlError(format!(
                "Unsupported scalar type: {}",
                wip.shape()
            )));
        }
    }

    trace!("Finished deserializing {}", "scalar".blue());

    Ok(wip)
}
