#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

pub mod error;
mod to_scalar;

use error::{TomlError, TomlErrorKind};
use facet_core::{Def, Facet, Struct, StructKind};
use facet_reflect::{ScalarType, Wip};
use log::trace;
use toml_edit::{ImDocument, Item, TomlError as TomlEditError};
use yansi::Paint as _;

/// Deserializes a TOML string into a value of type `T` that implements `Facet`.
pub fn from_str<T: Facet>(toml: &str) -> Result<T, TomlError<'_>> {
    trace!("Parsing TOML");

    // Parse the TOML document
    let docs: ImDocument<String> = toml.parse().map_err(|e: TomlEditError| {
        TomlError::new(
            toml,
            TomlErrorKind::GenericTomlError(e.message().to_string()),
            e.span(),
        )
    })?;

    trace!("Starting deserialization");

    // Deserialize it with facet reflection
    let wip = deserialize_item(toml, Wip::alloc::<T>(), docs.as_item())?;

    // Build the result
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
    trace!(
        "Deserializing {} as {}",
        item.type_name().cyan(),
        "struct".blue()
    );

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
                    return Err(TomlError::new(
                        toml,
                        TomlErrorKind::ExpectedFieldWithName(field.name),
                        item.span(),
                    ));
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
    trace!(
        "Deserializing {} as {}",
        item.type_name().cyan(),
        "enum".blue()
    );

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
                        return Err(TomlError::new(
                            toml,
                            TomlErrorKind::ExpectedExactlyOneField,
                            inline_table.span(),
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
                    return Err(TomlError::new(
                        toml,
                        TomlErrorKind::ExpectedAtLeastOneField,
                        inline_table.span(),
                    ));
                }
            }

            let variant_name = value.as_str().ok_or_else(|| {
                TomlError::new(
                    toml,
                    TomlErrorKind::ExpectedType {
                        expected: "string",
                        got: value.type_name(),
                    },
                    value.span(),
                )
            })?;

            build_enum_from_variant_name(toml, wip, variant_name, item)?
        }

        Item::Table(table) => {
            if let Some((key, field)) = table.iter().next() {
                trace!("Entering {} with key {}", "table".cyan(), key.cyan().bold());

                if table.len() > 1 {
                    return Err(TomlError::new(
                        toml,
                        TomlErrorKind::ExpectedExactlyOneField,
                        table.span(),
                    ));
                } else {
                    build_enum_from_variant_name(toml, wip, key, field)?
                }
            } else {
                return Err(TomlError::new(
                    toml,
                    TomlErrorKind::ExpectedAtLeastOneField,
                    table.span(),
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
        .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), item.span()))?;

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
            .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), item.span()))?;

        // Try to get the TOML value as a table to extract the field
        if let Some(table) = item.as_table_like() {
            // Base the field name on what type of struct we are
            let field_name = if is_tuple {
                &index.to_string()
            } else {
                // It must be a struct field
                field.name
            };

            // Try to get the TOML field matching the Rust name
            match table.get(field_name) {
                // Field found, push it
                Some(field) => {
                    wip = deserialize_item(toml, wip, field)?;
                }
                // Push none if field not found and it's an option
                None if matches!(field.shape().def, Def::Option(_)) => {
                    // Default of `Option<T>` is `None`
                    wip = wip.put_default().map_err(|e| {
                        TomlError::new(toml, TomlErrorKind::GenericReflect(e), item.span())
                    })?;
                }
                None => {
                    return Err(TomlError::new(
                        toml,
                        TomlErrorKind::ExpectedFieldWithName(field.name),
                        item.span(),
                    ));
                }
            }
        } else if item.is_value() {
            wip = deserialize_item(toml, wip, item)?;
        } else {
            return Err(TomlError::new(
                toml,
                TomlErrorKind::UnrecognizedType(item.type_name()),
                item.span(),
            ));
        }

        wip = wip
            .pop()
            .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), item.span()))?;
    }

    Ok(wip)
}

fn deserialize_as_list<'input, 'a>(
    toml: &'input str,
    mut wip: Wip<'a>,
    item: &Item,
) -> Result<Wip<'a>, TomlError<'input>> {
    trace!(
        "Deserializing {} as {}",
        item.type_name().cyan(),
        "list".blue()
    );

    // Get the TOML item as an array
    let Some(item) = item.as_array() else {
        return Err(TomlError::new(
            toml,
            TomlErrorKind::ExpectedType {
                expected: "array",
                got: item.type_name(),
            },
            item.span(),
        ));
    };

    if item.is_empty() {
        // Only put an empty list
        return wip
            .put_empty_list()
            .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), item.span()));
    }

    // Start the list
    wip = wip
        .begin_pushback()
        .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), item.span()))?;

    // Loop over all items in the TOML list
    for value in item.iter() {
        // Start the field
        wip = wip
            .push()
            .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), value.span()))?;

        wip = deserialize_item(
            toml,
            wip,
            // TODO: remove clone
            &Item::Value(value.clone()),
        )?;

        // Finish the field
        wip = wip
            .pop()
            .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), value.span()))?;
    }

    trace!("Finished deserializing {}", "list".blue());

    Ok(wip)
}

fn deserialize_as_map<'input, 'a>(
    toml: &'input str,
    mut wip: Wip<'a>,
    item: &Item,
) -> Result<Wip<'a>, TomlError<'input>> {
    trace!(
        "Deserializing {} as {}",
        item.type_name().cyan(),
        "map".blue()
    );

    // We expect a table to fill a map
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

    if table.is_empty() {
        // Only put an empty map
        return wip
            .put_empty_map()
            .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), item.span()));
    }

    // Start the map
    wip = wip
        .begin_map_insert()
        .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), item.span()))?;

    // Loop over all items in the TOML list
    for (k, v) in table.iter() {
        // Start the key
        wip = wip
            .push_map_key()
            .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), item.span()))?;

        trace!("Push {} {}", "key".cyan(), k.cyan().bold());

        // Deserialize the key
        match ScalarType::try_from_shape(wip.shape()).ok_or_else(|| {
            TomlError::new(
                toml,
                TomlErrorKind::UnrecognizedScalar(wip.shape()),
                item.span(),
            )
        })? {
            #[cfg(feature = "std")]
            ScalarType::String => {
                wip = wip.put(k.to_string()).map_err(|e| {
                    TomlError::new(toml, TomlErrorKind::GenericReflect(e), item.span())
                })?;
            }
            #[cfg(feature = "std")]
            ScalarType::CowStr => {
                wip = wip
                    .put(std::borrow::Cow::Owned(k.to_string()))
                    .map_err(|e| {
                        TomlError::new(toml, TomlErrorKind::GenericReflect(e), item.span())
                    })?
            }
            _ => {
                return Err(TomlError::new(
                    toml,
                    TomlErrorKind::InvalidKey(wip.shape()),
                    item.span(),
                ));
            }
        };

        trace!("Push {}", "value".cyan());

        // Start the value
        wip = wip
            .push_map_value()
            .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), v.span()))?;

        // Deserialize the value
        wip = deserialize_item(toml, wip, v)?;

        // Finish the value
        wip = wip
            .pop()
            .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), v.span()))?;
    }

    trace!("Finished deserializing {}", "map".blue());

    Ok(wip)
}

fn deserialize_as_option<'input, 'a>(
    toml: &'input str,
    mut wip: Wip<'a>,
    item: &Item,
) -> Result<Wip<'a>, TomlError<'input>> {
    trace!(
        "Deserializing {} as {}",
        item.type_name().cyan(),
        "option".blue()
    );

    wip = wip
        .push_some()
        .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), item.span()))?;

    wip = deserialize_item(toml, wip, item)?;

    wip = wip
        .pop()
        .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), item.span()))?;

    trace!("Finished deserializing {}", "option".blue());

    Ok(wip)
}

fn deserialize_as_smartpointer<'input, 'a>(
    _toml: &'input str,
    mut _wip: Wip<'a>,
    item: &Item,
) -> Result<Wip<'a>, TomlError<'input>> {
    trace!(
        "Deserializing {} as {}",
        item.type_name().cyan(),
        "smart pointer".blue()
    );

    trace!("Finished deserializing {}", "smart pointer".blue());

    todo!();
}

fn deserialize_as_scalar<'input, 'a>(
    toml: &'input str,
    mut wip: Wip<'a>,
    item: &Item,
) -> Result<Wip<'a>, TomlError<'input>> {
    trace!(
        "Deserializing {} as {}",
        item.type_name().cyan(),
        "scalar".blue()
    );

    wip = match ScalarType::try_from_shape(wip.shape()).ok_or_else(|| {
        TomlError::new(
            toml,
            TomlErrorKind::UnrecognizedScalar(wip.shape()),
            item.span(),
        )
    })? {
        ScalarType::Bool => to_scalar::put_boolean(toml, wip, item)?,

        // Regular String and &str are handled by from_str
        #[cfg(feature = "std")]
        ScalarType::CowStr => to_scalar::put_string::<std::borrow::Cow<'_, str>>(toml, wip, item)?,

        ScalarType::F32 => to_scalar::put_number::<f32>(toml, wip, item)?,
        ScalarType::F64 => to_scalar::put_number::<f64>(toml, wip, item)?,
        ScalarType::U8 => to_scalar::put_number::<u8>(toml, wip, item)?,
        ScalarType::U16 => to_scalar::put_number::<u16>(toml, wip, item)?,
        ScalarType::U32 => to_scalar::put_number::<u32>(toml, wip, item)?,
        ScalarType::U64 => to_scalar::put_number::<u64>(toml, wip, item)?,
        ScalarType::USize => to_scalar::put_number::<usize>(toml, wip, item)?,
        ScalarType::I8 => to_scalar::put_number::<i8>(toml, wip, item)?,
        ScalarType::I16 => to_scalar::put_number::<i16>(toml, wip, item)?,
        ScalarType::I32 => to_scalar::put_number::<i32>(toml, wip, item)?,
        ScalarType::I64 => to_scalar::put_number::<i64>(toml, wip, item)?,
        ScalarType::ISize => to_scalar::put_number::<isize>(toml, wip, item)?,

        // Use the from_str method if available
        _ if wip.shape().is_from_str() => {
            // Try to parse it as a string
            to_scalar::put_from_str(toml, wip, item)?
        }

        _ => {
            return Err(TomlError::new(
                toml,
                TomlErrorKind::UnrecognizedScalar(wip.shape()),
                item.span(),
            ));
        }
    };

    trace!("Finished deserializing {}", "scalar".blue());

    Ok(wip)
}
