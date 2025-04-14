#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

pub mod error;
mod to_scalar;

use std::{
    borrow::Cow,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    num::NonZero,
};

use error::AnyErr;
use facet_core::{Def, Facet};
use facet_reflect::Wip;
use toml_edit::{DocumentMut, Item, TomlError};

/// Deserializes a TOML string into a value of type `T` that implements `Facet`.
pub fn from_str<T: Facet>(toml: &str) -> Result<T, AnyErr> {
    let wip = Wip::alloc::<T>();
    let wip = from_str_value(wip, toml)?;
    let heap_value = wip.build().map_err(|e| AnyErr(e.to_string()))?;
    heap_value
        .materialize::<T>()
        .map_err(|e| AnyErr(e.to_string()))
}

fn from_str_value<'a>(wip: Wip<'a>, toml: &str) -> Result<Wip<'a>, AnyErr> {
    let docs: DocumentMut = toml.parse().map_err(|e| TomlError::to_string(&e))?;
    deserialize_item(wip, docs.as_item())
}

fn deserialize_item<'a>(wip: Wip<'a>, item: &Item) -> Result<Wip<'a>, AnyErr> {
    let shape = wip.shape();
    match shape.def {
        Def::Scalar(_) => deserialize_as_scalar(wip, item),
        Def::List(_) => todo!(),
        Def::Map(_) => todo!(),
        Def::Struct(_) => deserialize_as_struct(wip, item),
        Def::Enum(_) => deserialize_as_enum(wip, item),
        _ => Err(AnyErr(format!("Unsupported type: {:?}", shape))),
    }
}

fn deserialize_as_struct<'a>(mut wip: Wip<'a>, item: &Item) -> Result<Wip<'a>, AnyErr> {
    // Parse as the inner struct type if item is a single value and the struct is a unit struct
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

    Ok(wip)
}

fn deserialize_as_enum<'a>(wip: Wip<'a>, item: &Item) -> Result<Wip<'a>, AnyErr> {
    if item.is_value() {
        let variant_name = item
            .as_str()
            .ok_or_else(|| AnyErr(format!("Expected string, got: {}", item.type_name())))?;

        // Use variant_named to select the variant by name
        wip.variant_named(variant_name).map_err(|e| {
            AnyErr(format!(
                "Error selecting enum variant '{}': {}",
                variant_name, e
            ))
        })
    } else {
        // TODO: Handle non-unit enum variants
        Err(AnyErr("Non-unit enum variants not yet supported".into()))
    }
}

fn deserialize_as_scalar<'a>(mut wip: Wip<'a>, item: &Item) -> Result<Wip<'a>, AnyErr> {
    let shape = wip.shape();

    if shape.is_type::<String>() {
        let val = to_scalar::string(item)?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<Cow<'_, str>>() {
        let val = Cow::Owned(to_scalar::string(item)?);
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<bool>() {
        let val = to_scalar::boolean(item)?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<f64>() {
        let val = to_scalar::number::<f64>(item)?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<f32>() {
        let val = to_scalar::number::<f32>(item)?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<usize>() {
        let val = to_scalar::number::<usize>(item)?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<u128>() {
        let val = to_scalar::number::<u128>(item)?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<u64>() {
        let val = to_scalar::number::<u64>(item)?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<u32>() {
        let val = to_scalar::number::<u32>(item)?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<u16>() {
        let val = to_scalar::number::<u16>(item)?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<u8>() {
        let val = to_scalar::number::<u8>(item)?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<isize>() {
        let val = to_scalar::number::<isize>(item)?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<i128>() {
        let val = to_scalar::number::<i128>(item)?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<i64>() {
        let val = to_scalar::number::<i64>(item)?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<i32>() {
        let val = to_scalar::number::<i32>(item)?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<i16>() {
        let val = to_scalar::number::<i16>(item)?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<i8>() {
        let val = to_scalar::number::<i8>(item)?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<NonZero<usize>>() {
        // TODO: create a to_scalar::nonzero_number method when we can use a trait to do so
        let val = NonZero::new(to_scalar::number::<usize>(item)?)
            .ok_or_else(|| AnyErr("Could not convert number to non-zero variant".into()))?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<NonZero<u128>>() {
        let val = NonZero::new(to_scalar::number::<u128>(item)?)
            .ok_or_else(|| AnyErr("Could not convert number to non-zero variant".into()))?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<NonZero<u64>>() {
        let val = NonZero::new(to_scalar::number::<u64>(item)?)
            .ok_or_else(|| AnyErr("Could not convert number to non-zero variant".into()))?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<NonZero<u32>>() {
        let val = NonZero::new(to_scalar::number::<u32>(item)?)
            .ok_or_else(|| AnyErr("Could not convert number to non-zero variant".into()))?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<NonZero<u16>>() {
        let val = NonZero::new(to_scalar::number::<u16>(item)?)
            .ok_or_else(|| AnyErr("Could not convert number to non-zero variant".into()))?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<NonZero<u8>>() {
        let val = NonZero::new(to_scalar::number::<u8>(item)?)
            .ok_or_else(|| AnyErr("Could not convert number to non-zero variant".into()))?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<NonZero<isize>>() {
        let val = NonZero::new(to_scalar::number::<isize>(item)?)
            .ok_or_else(|| AnyErr("Could not convert number to non-zero variant".into()))?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<NonZero<i128>>() {
        let val = NonZero::new(to_scalar::number::<i128>(item)?)
            .ok_or_else(|| AnyErr("Could not convert number to non-zero variant".into()))?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<NonZero<i64>>() {
        let val = NonZero::new(to_scalar::number::<i64>(item)?)
            .ok_or_else(|| AnyErr("Could not convert number to non-zero variant".into()))?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<NonZero<i32>>() {
        let val = NonZero::new(to_scalar::number::<i32>(item)?)
            .ok_or_else(|| AnyErr("Could not convert number to non-zero variant".into()))?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<NonZero<i16>>() {
        let val = NonZero::new(to_scalar::number::<i16>(item)?)
            .ok_or_else(|| AnyErr("Could not convert number to non-zero variant".into()))?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<NonZero<i8>>() {
        let val = NonZero::new(to_scalar::number::<i8>(item)?)
            .ok_or_else(|| AnyErr("Could not convert number to non-zero variant".into()))?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<SocketAddr>() {
        let val = to_scalar::from_str::<SocketAddr>(item, "socket address")?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<IpAddr>() {
        let val = to_scalar::from_str::<IpAddr>(item, "ip address")?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<Ipv4Addr>() {
        let val = to_scalar::from_str::<Ipv4Addr>(item, "ipv4 address")?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else if shape.is_type::<Ipv6Addr>() {
        let val = to_scalar::from_str::<Ipv6Addr>(item, "ipv6 address")?;
        wip = wip.put(val).map_err(|e| AnyErr(e.to_string()))?;
    } else {
        return Err(AnyErr(format!("Unsupported scalar type: {}", wip.shape())));
    }
    Ok(wip)
}
