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
use facet_core::{Facet, Opaque, VariantKind};
use facet_reflect::{PokeEnumNoVariant, PokeStructUninit, PokeUninit, PokeValue, PokeValueUninit};
use toml_edit::{DocumentMut, Item, TomlError};

/// Deserializes a TOML string into a value of type `T` that implements `Facet`.
pub fn from_str<T: Facet>(toml: &str) -> Result<T, AnyErr> {
    let (poke, _guard) = PokeUninit::alloc::<T>();
    let opaque = from_str_opaque(poke, toml)?;
    Ok(unsafe { opaque.read::<T>() })
}

fn from_str_opaque<'mem>(poke: PokeUninit<'mem>, toml: &str) -> Result<Opaque<'mem>, AnyErr> {
    let docs: DocumentMut = toml.parse().map_err(|e| TomlError::to_string(&e))?;
    deserialize_item(poke, docs.as_item())
}

fn deserialize_item<'mem>(poke: PokeUninit<'mem>, item: &Item) -> Result<Opaque<'mem>, AnyErr> {
    match poke {
        PokeUninit::Scalar(poke) => Ok(deserialize_as_scalar(poke, item)?.data()),
        PokeUninit::List(_) => todo!(),
        PokeUninit::Map(_) => todo!(),
        PokeUninit::Struct(poke) => deserialize_as_struct(poke, item),
        PokeUninit::Enum(poke) => deserialize_as_enum(poke, item),
        _ => todo!("unsupported poke type"),
    }
}

fn deserialize_as_struct<'mem>(
    mut poke: PokeStructUninit<'mem>,
    item: &Item,
) -> Result<Opaque<'mem>, AnyErr> {
    // Parse as a the inner struct type if item is a single value and the struct is a unit struct
    if item.is_value() {
        // Only allow unit structs
        if poke.def().fields.len() > 1 {
            return Err(
                "Failed trying to parse a single value as a struct with multiple fields".into(),
            );
        }

        return deserialize_item(
            poke.field(0)
                .map_err(|e| format!("Unit struct is missing value: {e}"))?,
            item,
        );
    }

    // Otherwise we expect a table
    let table = item
        .as_table_like()
        .ok_or_else(|| format!("Expected table like structure, got {}", item.type_name()))?;

    for (k, v) in table.iter() {
        let (index, field_poke) = poke
            .field_by_name(k)
            .map_err(|e| format!("Field '{}' error: {}", k, e))?;
        let _v = deserialize_item(field_poke, v)
            .map_err(|e| format!("Error deserializing field '{}': {}", k, e))?;
        unsafe {
            poke.assume_field_init(index);
        }
    }

    Ok(poke.build_in_place())
}

fn deserialize_as_enum<'mem>(
    poke: PokeEnumNoVariant<'mem>,
    item: &Item,
) -> Result<Opaque<'mem>, AnyErr> {
    // Item is a single value, try to get the variant with the name
    if item.is_value() {
        let value = item
            .as_str()
            .ok_or_else(|| format!("Expokected string, got: {}", item.type_name()))?;

        let variant = poke
            .variant_by_name(value)
            .ok_or_else(|| format!("Enum does not have a variant named '{value}'"))?;

        if variant.kind != VariantKind::Unit {
            return Err(format!("variant '{value}' is not a unit variant").into());
        }

        Ok(poke
            .set_variant_by_name(value)
            .map_err(|err| err.to_string())?
            .build_in_place())
    } else {
        todo!()
    }
}

fn deserialize_as_scalar<'mem>(
    poke: PokeValueUninit<'mem>,
    item: &Item,
) -> Result<PokeValue<'mem>, AnyErr> {
    let shape = poke.shape();

    Ok(if shape.is_type::<String>() {
        poke.put(to_scalar::string(item)?)
    } else if shape.is_type::<Cow<'_, str>>() {
        poke.put(Cow::Owned(to_scalar::string(item)?))
    } else if shape.is_type::<bool>() {
        poke.put(to_scalar::boolean(item)?)
    } else if shape.is_type::<f64>() {
        poke.put(to_scalar::number::<f64>(item)?)
    } else if shape.is_type::<f32>() {
        poke.put(to_scalar::number::<f32>(item)?)
    } else if shape.is_type::<usize>() {
        poke.put(to_scalar::number::<usize>(item)?)
    } else if shape.is_type::<u128>() {
        poke.put(to_scalar::number::<u128>(item)?)
    } else if shape.is_type::<u64>() {
        poke.put(to_scalar::number::<u64>(item)?)
    } else if shape.is_type::<u32>() {
        poke.put(to_scalar::number::<u32>(item)?)
    } else if shape.is_type::<u16>() {
        poke.put(to_scalar::number::<u16>(item)?)
    } else if shape.is_type::<u8>() {
        poke.put(to_scalar::number::<u8>(item)?)
    } else if shape.is_type::<isize>() {
        poke.put(to_scalar::number::<isize>(item)?)
    } else if shape.is_type::<i128>() {
        poke.put(to_scalar::number::<i128>(item)?)
    } else if shape.is_type::<i64>() {
        poke.put(to_scalar::number::<i64>(item)?)
    } else if shape.is_type::<i32>() {
        poke.put(to_scalar::number::<i32>(item)?)
    } else if shape.is_type::<i16>() {
        poke.put(to_scalar::number::<i16>(item)?)
    } else if shape.is_type::<i8>() {
        poke.put(to_scalar::number::<i8>(item)?)
    } else if shape.is_type::<NonZero<usize>>() {
        // TODO: create a to_scalar::nonzero_number method when we can use a trait to do so
        poke.put(
            NonZero::new(to_scalar::number::<usize>(item)?)
                .ok_or("Could not convert number to non-zero variant")?,
        )
    } else if shape.is_type::<NonZero<u128>>() {
        poke.put(
            NonZero::new(to_scalar::number::<u128>(item)?)
                .ok_or("Could not convert number to non-zero variant")?,
        )
    } else if shape.is_type::<NonZero<u64>>() {
        poke.put(
            NonZero::new(to_scalar::number::<u64>(item)?)
                .ok_or("Could not convert number to non-zero variant")?,
        )
    } else if shape.is_type::<NonZero<u32>>() {
        poke.put(
            NonZero::new(to_scalar::number::<u32>(item)?)
                .ok_or("Could not convert number to non-zero variant")?,
        )
    } else if shape.is_type::<NonZero<u16>>() {
        poke.put(
            NonZero::new(to_scalar::number::<u16>(item)?)
                .ok_or("Could not convert number to non-zero variant")?,
        )
    } else if shape.is_type::<NonZero<u8>>() {
        poke.put(
            NonZero::new(to_scalar::number::<u8>(item)?)
                .ok_or("Could not convert number to non-zero variant")?,
        )
    } else if shape.is_type::<NonZero<isize>>() {
        poke.put(
            NonZero::new(to_scalar::number::<isize>(item)?)
                .ok_or("Could not convert number to non-zero variant")?,
        )
    } else if shape.is_type::<NonZero<i128>>() {
        poke.put(
            NonZero::new(to_scalar::number::<i128>(item)?)
                .ok_or("Could not convert number to non-zero variant")?,
        )
    } else if shape.is_type::<NonZero<i64>>() {
        poke.put(
            NonZero::new(to_scalar::number::<i64>(item)?)
                .ok_or("Could not convert number to non-zero variant")?,
        )
    } else if shape.is_type::<NonZero<i32>>() {
        poke.put(
            NonZero::new(to_scalar::number::<i32>(item)?)
                .ok_or("Could not convert number to non-zero variant")?,
        )
    } else if shape.is_type::<NonZero<i16>>() {
        poke.put(
            NonZero::new(to_scalar::number::<i16>(item)?)
                .ok_or("Could not convert number to non-zero variant")?,
        )
    } else if shape.is_type::<NonZero<i8>>() {
        poke.put(
            NonZero::new(to_scalar::number::<i8>(item)?)
                .ok_or("Could not convert number to non-zero variant")?,
        )
    } else if shape.is_type::<SocketAddr>() {
        poke.put(to_scalar::from_str::<SocketAddr>(item, "socket address")?)
    } else if shape.is_type::<IpAddr>() {
        poke.put(to_scalar::from_str::<IpAddr>(item, "ip address")?)
    } else if shape.is_type::<Ipv4Addr>() {
        poke.put(to_scalar::from_str::<Ipv4Addr>(item, "ipv4 address")?)
    } else if shape.is_type::<Ipv6Addr>() {
        poke.put(to_scalar::from_str::<Ipv6Addr>(item, "ipv6 address")?)
    } else {
        return Err(format!("Unsupported scalar type: {}", poke.shape()).into());
    })
}
