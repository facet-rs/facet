//! Parse TOML strings into Rust values.

#[cfg(not(feature = "alloc"))]
compile_error!("feature `alloc` is required");

mod error;
mod to_scalar;

use alloc::{
    borrow::Cow,
    string::{String, ToString},
};
pub use error::{TomlDeError, TomlDeErrorKind};
use facet_core::{Characteristic, Def, Facet, FieldFlags, StructKind, Type, UserType};
use facet_reflect::{Partial, ReflectError, ScalarType};
use log::trace;
use owo_colors::OwoColorize;
use toml_edit::{ImDocument, Item, TomlError};

macro_rules! reflect {
    ($wip:expr, $toml:expr, $span:expr, $($tt:tt)*) => {
        match $wip.$($tt)* {
            Ok(_) => {},
            Err(e) => {
                return Err(TomlDeError::new(
                    $toml,
                    TomlDeErrorKind::GenericReflect(e),
                    $span,
                    $wip.path()
                ));
            }
        }
    };
}

/// Deserializes a TOML string into a value of type `T` that implements `Facet`.
pub fn from_str<'input, 'facet: 'shape, 'shape, T: Facet<'facet>>(
    toml: &'input str,
) -> Result<T, TomlDeError<'input, 'shape>> {
    trace!("Parsing TOML");

    // Allocate the type
    let mut partial = Partial::alloc::<T>().map_err(|e| {
        TomlDeError::new(
            toml,
            TomlDeErrorKind::GenericReflect(e),
            None,
            "$".to_string(),
        )
    })?;

    // Parse the TOML document
    let docs: ImDocument<String> = toml.parse().map_err(|e: TomlError| {
        TomlDeError::new(
            toml,
            TomlDeErrorKind::GenericTomlError(e.message().to_string()),
            e.span(),
            partial.path(),
        )
    })?;

    trace!("Starting deserialization");

    // Deserialize it with facet reflection
    deserialize_item(toml, partial.inner_mut(), docs.as_item())?;

    // Build the result
    let result = partial.build().map_err(|e| {
        TomlDeError::new(
            toml,
            TomlDeErrorKind::GenericReflect(e),
            None,
            "$".to_string(),
        )
    })?;

    trace!("Finished deserialization");

    Ok(*result)
}

fn deserialize_item<'input, 'facet, 'shape>(
    toml: &'input str,
    wip: &mut Partial<'facet, 'shape>,
    item: &Item,
) -> Result<(), TomlDeError<'input, 'shape>> {
    // Check for Option before anything else, since it's a special case
    // Option is an enum in Rust, but we handle it specially
    if let Def::Option(_) = wip.shape().def {
        return deserialize_as_option(toml, wip, item);
    }

    // First check the type system (Type)
    if let Type::User(UserType::Struct(struct_def)) = &wip.shape().ty {
        return deserialize_as_struct(toml, wip, struct_def, item);
    }

    // Check for enum in the type system
    if let Type::User(UserType::Enum(_)) = &wip.shape().ty {
        deserialize_as_enum(toml, wip, item)?;
        return Ok(());
    }

    // Fall back to the def system for other types
    match wip.shape().def {
        Def::Scalar(_) => deserialize_as_scalar(toml, wip, item)?,
        Def::List(_) => deserialize_as_list(toml, wip, item)?,
        Def::Map(_) => deserialize_as_map(toml, wip, item)?,
        Def::SmartPointer(_) => deserialize_as_smartpointer(toml, wip, item)?,
        _ => todo!(),
    }
    Ok(())
}

fn deserialize_as_struct<'input, 'a, 'shape>(
    toml: &'input str,
    wip: &mut Partial<'a, 'shape>,
    def: &facet_core::StructType<'shape>,
    item: &Item,
) -> Result<(), TomlDeError<'input, 'shape>> {
    trace!(
        "Deserializing {} as {}",
        item.type_name().cyan(),
        "struct".blue()
    );

    // Parse as a the inner struct type if item is a single value and the struct is a unit struct
    if item.is_value() && !item.is_inline_table() {
        // Only allow unit structs
        let shape = wip.shape();
        if let Type::User(UserType::Struct(struct_def)) = &shape.ty {
            if struct_def.fields.len() > 1 {
                return Err(TomlDeError::new(
                    toml,
                    TomlDeErrorKind::ParseSingleValueAsMultipleFieldStruct,
                    item.span(),
                    wip.path(),
                ));
            }
        }

        reflect!(wip, toml, item.span(), begin_nth_field(0));

        deserialize_item(toml, wip, item)?;

        reflect!(wip, toml, item.span(), end());

        return Ok(());
    }

    // Otherwise we expect a table
    let table = item.as_table_like().ok_or_else(|| {
        TomlDeError::new(
            toml,
            TomlDeErrorKind::ExpectedType {
                expected: "table like structure",
                got: item.type_name(),
            },
            item.span(),
            wip.path(),
        )
    })?;

    for field in def.fields {
        reflect!(wip, toml, item.span(), begin_field(field.name));

        // Find the matching TOML field
        let field_item = table.get(field.name);
        match field_item {
            Some(field_item) => deserialize_item(toml, wip, field_item)?,
            None => {
                if let Def::Option(..) = field.shape().def {
                    // Default of `Option<T>` is `None`
                    reflect!(wip, toml, item.span(), set_default());
                } else if field.flags.contains(FieldFlags::DEFAULT) {
                    // Handle the default - set_default internally handles custom default functions safely
                    if field.shape().is(Characteristic::Default)
                        || field.vtable.default_fn.is_some()
                    {
                        reflect!(wip, toml, item.span(), set_default());
                    } else {
                        // Throw an error when there's a "default" attribute but no implementation for the type
                        return Err(TomlDeError::new(
                            toml,
                            TomlDeErrorKind::GenericReflect(
                                ReflectError::DefaultAttrButNoDefaultImpl {
                                    shape: field.shape(),
                                },
                            ),
                            item.span(),
                            wip.path(),
                        ));
                    }
                } else if field.shape().is_type::<()>() {
                    // Default of `()` is `()`
                    reflect!(wip, toml, item.span(), set_default());
                } else {
                    return Err(TomlDeError::new(
                        toml,
                        TomlDeErrorKind::ExpectedFieldWithName(field.name),
                        item.span(),
                        wip.path(),
                    ));
                }
            }
        }

        reflect!(wip, toml, item.span(), end());
    }

    trace!("Finished deserializing {}", "struct".blue());

    Ok(())
}

fn deserialize_as_enum<'input, 'a, 'shape>(
    toml: &'input str,
    wip: &mut Partial<'a, 'shape>,
    item: &Item,
) -> Result<(), TomlDeError<'input, 'shape>> {
    trace!(
        "Deserializing {} as {}",
        item.type_name().cyan(),
        "enum".blue()
    );

    match item {
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
                        return Err(TomlDeError::new(
                            toml,
                            TomlDeErrorKind::ExpectedExactlyOneField,
                            inline_table.span(),
                            wip.path(),
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
                    return Err(TomlDeError::new(
                        toml,
                        TomlDeErrorKind::ExpectedAtLeastOneField,
                        inline_table.span(),
                        wip.path(),
                    ));
                }
            }

            let variant_name = value.as_str().ok_or_else(|| {
                TomlDeError::new(
                    toml,
                    TomlDeErrorKind::ExpectedType {
                        expected: "string",
                        got: value.type_name(),
                    },
                    value.span(),
                    wip.path(),
                )
            })?;

            build_enum_from_variant_name(toml, wip, variant_name, item)?
        }

        Item::Table(table) => {
            if let Some((key, field)) = table.iter().next() {
                trace!("Entering {} with key {}", "table".cyan(), key.cyan().bold());

                if table.len() > 1 {
                    return Err(TomlDeError::new(
                        toml,
                        TomlDeErrorKind::ExpectedExactlyOneField,
                        table.span(),
                        wip.path(),
                    ));
                } else {
                    build_enum_from_variant_name(toml, wip, key, field)?
                }
            } else {
                return Err(TomlDeError::new(
                    toml,
                    TomlDeErrorKind::ExpectedAtLeastOneField,
                    table.span(),
                    wip.path(),
                ));
            }
        }

        Item::ArrayOfTables(_array_of_tables) => todo!(),
    }

    trace!("Finished deserializing {}", "enum".blue());

    Ok(())
}

fn build_enum_from_variant_name<'input, 'a, 'shape>(
    toml: &'input str,
    wip: &mut Partial<'a, 'shape>,
    variant_name: &str,
    item: &Item,
) -> Result<(), TomlDeError<'input, 'shape>> {
    // Select the variant
    reflect!(wip, toml, item.span(), select_variant_named(variant_name));

    // Safe to unwrap because the variant got just selected
    let variant = wip.selected_variant().unwrap();

    if variant.data.kind == StructKind::Unit {
        // No need to do anything, we can just set the variant since it's a unit enum
        return Ok(());
    }

    // Whether it's a tuple so we need to use the index
    let is_tuple =
        variant.data.kind == StructKind::TupleStruct || variant.data.kind == StructKind::Tuple;

    // Push all fields
    for (index, field) in variant.data.fields.iter().enumerate() {
        reflect!(wip, toml, item.span(), begin_field(field.name));

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
                    deserialize_item(toml, wip, field)?;
                }
                // Push none if field not found and it's an option
                None if matches!(field.shape().def, Def::Option(_)) => {
                    // Default of `Option<T>` is `None`
                    reflect!(wip, toml, item.span(), set_default());
                }
                None => {
                    return Err(TomlDeError::new(
                        toml,
                        TomlDeErrorKind::ExpectedFieldWithName(field.name),
                        item.span(),
                        wip.path(),
                    ));
                }
            }
        } else if item.is_value() {
            deserialize_item(toml, wip, item)?;
        } else {
            return Err(TomlDeError::new(
                toml,
                TomlDeErrorKind::UnrecognizedType(item.type_name()),
                item.span(),
                wip.path(),
            ));
        }

        reflect!(wip, toml, item.span(), end());
    }

    Ok(())
}

fn deserialize_as_list<'input, 'a, 'shape>(
    toml: &'input str,
    wip: &mut Partial<'a, 'shape>,
    item: &Item,
) -> Result<(), TomlDeError<'input, 'shape>> {
    trace!(
        "Deserializing {} as {}",
        item.type_name().cyan(),
        "list".blue()
    );

    // Get the TOML item as an array
    let Some(item) = item.as_array() else {
        return Err(TomlDeError::new(
            toml,
            TomlDeErrorKind::ExpectedType {
                expected: "array",
                got: item.type_name(),
            },
            item.span(),
            wip.path(),
        ));
    };

    // Start the list
    reflect!(wip, toml, item.span(), begin_list());

    if item.is_empty() {
        // Empty list - nothing more to do
        return Ok(());
    }

    // Loop over all items in the TOML list
    for value in item.iter() {
        // Start the field
        reflect!(wip, toml, value.span(), begin_list_item());

        deserialize_item(
            toml,
            wip,
            // TODO: remove clone
            &Item::Value(value.clone()),
        )?;

        // Finish the field
        reflect!(wip, toml, value.span(), end());
    }

    trace!("Finished deserializing {}", "list".blue());

    Ok(())
}

fn deserialize_as_map<'input, 'a, 'shape>(
    toml: &'input str,
    wip: &mut Partial<'a, 'shape>,
    item: &Item,
) -> Result<(), TomlDeError<'input, 'shape>> {
    trace!(
        "Deserializing {} as {}",
        item.type_name().cyan(),
        "map".blue()
    );

    // We expect a table to fill a map
    let table = item.as_table_like().ok_or_else(|| {
        TomlDeError::new(
            toml,
            TomlDeErrorKind::ExpectedType {
                expected: "table like structure",
                got: item.type_name(),
            },
            item.span(),
            wip.path(),
        )
    })?;

    // Start the map
    reflect!(wip, toml, item.span(), begin_map());

    if table.is_empty() {
        // Empty map - nothing more to do
        return Ok(());
    }

    // Loop over all items in the TOML list
    for (k, v) in table.iter() {
        // Start the key
        reflect!(wip, toml, item.span(), begin_key());

        trace!("Push {} {}", "key".cyan(), k.cyan().bold());

        // Deserialize the key
        match ScalarType::try_from_shape(wip.shape()).ok_or_else(|| {
            TomlDeError::new(
                toml,
                TomlDeErrorKind::UnrecognizedScalar(wip.shape()),
                item.span(),
                wip.path(),
            )
        })? {
            ScalarType::String => {
                reflect!(wip, toml, item.span(), set(k.to_string()));
            }
            ScalarType::CowStr => {
                reflect!(wip, toml, item.span(), set(Cow::Owned(k.to_string())));
            }
            _ => {
                return Err(TomlDeError::new(
                    toml,
                    TomlDeErrorKind::InvalidKey(wip.shape()),
                    item.span(),
                    wip.path(),
                ));
            }
        };

        // End the key frame
        reflect!(wip, toml, item.span(), end());

        trace!("Push {}", "value".cyan());

        // Start the value
        reflect!(wip, toml, v.span(), begin_value());

        // Deserialize the value
        deserialize_item(toml, wip, v)?;

        // Finish the value
        reflect!(wip, toml, v.span(), end());
    }

    trace!("Finished deserializing {}", "map".blue());

    Ok(())
}

fn deserialize_as_option<'input, 'a, 'shape>(
    toml: &'input str,
    wip: &mut Partial<'a, 'shape>,
    item: &Item,
) -> Result<(), TomlDeError<'input, 'shape>> {
    trace!(
        "Deserializing {} as {}",
        item.type_name().cyan(),
        "option".blue()
    );

    // Use begin_some to initialize the Option as Some
    reflect!(wip, toml, item.span(), begin_some());

    // Deserialize the inner value
    deserialize_item(toml, wip, item)?;

    // End the Option
    reflect!(wip, toml, item.span(), end());

    trace!("Finished deserializing {}", "option".blue());

    Ok(())
}

fn deserialize_as_smartpointer<'input, 'a, 'shape>(
    _toml: &'input str,
    _wip: &mut Partial<'a, 'shape>,
    item: &Item,
) -> Result<(), TomlDeError<'input, 'shape>> {
    trace!(
        "Deserializing {} as {}",
        item.type_name().cyan(),
        "smart pointer".blue()
    );

    trace!("Finished deserializing {}", "smart pointer".blue());

    todo!();
}

fn deserialize_as_scalar<'input, 'a, 'shape>(
    toml: &'input str,
    wip: &mut Partial<'a, 'shape>,
    item: &Item,
) -> Result<(), TomlDeError<'input, 'shape>> {
    trace!(
        "Deserializing {} as {}",
        item.type_name().cyan(),
        "scalar".blue()
    );

    match ScalarType::try_from_shape(wip.shape()).ok_or_else(|| {
        TomlDeError::new(
            toml,
            TomlDeErrorKind::UnrecognizedScalar(wip.shape()),
            item.span(),
            wip.path(),
        )
    })? {
        ScalarType::Unit => {}

        ScalarType::Bool => to_scalar::put_boolean(toml, wip, item)?,
        ScalarType::Char => to_scalar::put_char(toml, wip, item)?,

        // Regular String and &str are handled by from_str
        #[cfg(feature = "alloc")]
        ScalarType::CowStr => to_scalar::put_string::<Cow<'_, str>>(toml, wip, item)?,

        ScalarType::F32 => to_scalar::put_number::<f32>(toml, wip, item)?,
        ScalarType::F64 => to_scalar::put_number::<f64>(toml, wip, item)?,
        ScalarType::U8 => to_scalar::put_number::<u8>(toml, wip, item)?,
        ScalarType::U16 => to_scalar::put_number::<u16>(toml, wip, item)?,
        ScalarType::U32 => to_scalar::put_number::<u32>(toml, wip, item)?,
        ScalarType::U64 => to_scalar::put_number::<u64>(toml, wip, item)?,
        ScalarType::U128 => to_scalar::put_number::<u128>(toml, wip, item)?,
        ScalarType::USize => to_scalar::put_number::<usize>(toml, wip, item)?,
        ScalarType::I8 => to_scalar::put_number::<i8>(toml, wip, item)?,
        ScalarType::I16 => to_scalar::put_number::<i16>(toml, wip, item)?,
        ScalarType::I32 => to_scalar::put_number::<i32>(toml, wip, item)?,
        ScalarType::I64 => to_scalar::put_number::<i64>(toml, wip, item)?,
        ScalarType::I128 => to_scalar::put_number::<i128>(toml, wip, item)?,
        ScalarType::ISize => to_scalar::put_number::<isize>(toml, wip, item)?,

        // Use the from_str method if available
        _ if wip.shape().is_from_str() => {
            // Try to parse it as a string
            to_scalar::put_from_str(toml, wip, item)?
        }

        _ => {
            return Err(TomlDeError::new(
                toml,
                TomlDeErrorKind::UnrecognizedScalar(wip.shape()),
                item.span(),
                wip.path(),
            ));
        }
    }

    trace!("Finished deserializing {}", "scalar".blue());

    Ok(())
}
