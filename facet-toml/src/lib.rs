#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

mod to_scalar;

use std::{
    borrow::Cow,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    num::NonZero,
};

use facet_core::{Facet, Opaque};
use facet_reflect::PokeUninit;
use toml_edit::{DocumentMut, Item, TomlError};

/// Deserializes a TOML string into a value of type `T` that implements `Facet`.
pub fn from_str<T: Facet>(toml: &str) -> Result<T, AnyErr> {
    let (poke, _guard) = PokeUninit::alloc::<T>();
    let opaque = from_str_opaque(poke, toml)?;
    Ok(unsafe { opaque.read::<T>() })
}

/// Any error
#[derive(Debug, Clone)]
pub struct AnyErr(String);

impl core::fmt::Display for AnyErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for AnyErr {}

impl From<String> for AnyErr {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for AnyErr {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

fn from_str_opaque<'mem>(poke: PokeUninit<'mem>, toml: &str) -> Result<Opaque<'mem>, AnyErr> {
    let docs: DocumentMut = toml.parse().map_err(|e| TomlError::to_string(&e))?;
    deserialize_item(poke, docs.as_item())
}

fn deserialize_item<'mem>(poke: PokeUninit<'mem>, item: &Item) -> Result<Opaque<'mem>, AnyErr> {
    let opaque = match poke {
        PokeUninit::Scalar(ps) => {
            let shape = ps.shape();
            if shape.is_type::<String>() {
                ps.put(to_scalar::string(item)?)
            } else if shape.is_type::<Cow<'_, str>>() {
                ps.put(Cow::Owned(to_scalar::string(item)?))
            } else if shape.is_type::<bool>() {
                ps.put(to_scalar::boolean(item)?)
            } else if shape.is_type::<f64>() {
                ps.put(to_scalar::number::<f64>(item)?)
            } else if shape.is_type::<f32>() {
                ps.put(to_scalar::number::<f32>(item)?)
            } else if shape.is_type::<usize>() {
                ps.put(to_scalar::number::<usize>(item)?)
            } else if shape.is_type::<u128>() {
                ps.put(to_scalar::number::<u128>(item)?)
            } else if shape.is_type::<u64>() {
                ps.put(to_scalar::number::<u64>(item)?)
            } else if shape.is_type::<u32>() {
                ps.put(to_scalar::number::<u32>(item)?)
            } else if shape.is_type::<u16>() {
                ps.put(to_scalar::number::<u16>(item)?)
            } else if shape.is_type::<u8>() {
                ps.put(to_scalar::number::<u8>(item)?)
            } else if shape.is_type::<isize>() {
                ps.put(to_scalar::number::<isize>(item)?)
            } else if shape.is_type::<i128>() {
                ps.put(to_scalar::number::<i128>(item)?)
            } else if shape.is_type::<i64>() {
                ps.put(to_scalar::number::<i64>(item)?)
            } else if shape.is_type::<i32>() {
                ps.put(to_scalar::number::<i32>(item)?)
            } else if shape.is_type::<i16>() {
                ps.put(to_scalar::number::<i16>(item)?)
            } else if shape.is_type::<i8>() {
                ps.put(to_scalar::number::<i8>(item)?)
            } else if shape.is_type::<NonZero<usize>>() {
                // TODO: create a to_scalar::nonzero_number method when we can use a trait to do so
                ps.put(
                    NonZero::new(to_scalar::number::<usize>(item)?).ok_or_else(|| {
                        AnyErr("Could not convert number to non-zero variant".to_string())
                    })?,
                )
            } else if shape.is_type::<NonZero<u128>>() {
                ps.put(
                    NonZero::new(to_scalar::number::<u128>(item)?).ok_or_else(|| {
                        AnyErr("Could not convert number to non-zero variant".to_string())
                    })?,
                )
            } else if shape.is_type::<NonZero<u64>>() {
                ps.put(
                    NonZero::new(to_scalar::number::<u64>(item)?).ok_or_else(|| {
                        AnyErr("Could not convert number to non-zero variant".to_string())
                    })?,
                )
            } else if shape.is_type::<NonZero<u32>>() {
                ps.put(
                    NonZero::new(to_scalar::number::<u32>(item)?).ok_or_else(|| {
                        AnyErr("Could not convert number to non-zero variant".to_string())
                    })?,
                )
            } else if shape.is_type::<NonZero<u16>>() {
                ps.put(
                    NonZero::new(to_scalar::number::<u16>(item)?).ok_or_else(|| {
                        AnyErr("Could not convert number to non-zero variant".to_string())
                    })?,
                )
            } else if shape.is_type::<NonZero<u8>>() {
                ps.put(NonZero::new(to_scalar::number::<u8>(item)?).ok_or_else(|| {
                    AnyErr("Could not convert number to non-zero variant".to_string())
                })?)
            } else if shape.is_type::<NonZero<isize>>() {
                ps.put(
                    NonZero::new(to_scalar::number::<isize>(item)?).ok_or_else(|| {
                        AnyErr("Could not convert number to non-zero variant".to_string())
                    })?,
                )
            } else if shape.is_type::<NonZero<i128>>() {
                ps.put(
                    NonZero::new(to_scalar::number::<i128>(item)?).ok_or_else(|| {
                        AnyErr("Could not convert number to non-zero variant".to_string())
                    })?,
                )
            } else if shape.is_type::<NonZero<i64>>() {
                ps.put(
                    NonZero::new(to_scalar::number::<i64>(item)?).ok_or_else(|| {
                        AnyErr("Could not convert number to non-zero variant".to_string())
                    })?,
                )
            } else if shape.is_type::<NonZero<i32>>() {
                ps.put(
                    NonZero::new(to_scalar::number::<i32>(item)?).ok_or_else(|| {
                        AnyErr("Could not convert number to non-zero variant".to_string())
                    })?,
                )
            } else if shape.is_type::<NonZero<i16>>() {
                ps.put(
                    NonZero::new(to_scalar::number::<i16>(item)?).ok_or_else(|| {
                        AnyErr("Could not convert number to non-zero variant".to_string())
                    })?,
                )
            } else if shape.is_type::<NonZero<i8>>() {
                ps.put(NonZero::new(to_scalar::number::<i8>(item)?).ok_or_else(|| {
                    AnyErr("Could not convert number to non-zero variant".to_string())
                })?)
            } else if shape.is_type::<SocketAddr>() {
                ps.put(to_scalar::from_str::<SocketAddr>(item, "socket address")?)
            } else if shape.is_type::<IpAddr>() {
                ps.put(to_scalar::from_str::<IpAddr>(item, "ip address")?)
            } else if shape.is_type::<Ipv4Addr>() {
                ps.put(to_scalar::from_str::<Ipv4Addr>(item, "ipv4 address")?)
            } else if shape.is_type::<Ipv6Addr>() {
                ps.put(to_scalar::from_str::<Ipv6Addr>(item, "ipv6 address")?)
            } else {
                return Err(format!("Unsupported scalar type: {}", ps.shape()).into());
            }
        }
        PokeUninit::List(_) => todo!(),
        PokeUninit::Map(_) => todo!(),
        PokeUninit::Struct(mut ps) => {
            let table = item.as_table_like().ok_or_else(|| {
                format!("Expected table like structure, got {}", item.type_name())
            })?;

            for (k, v) in table.iter() {
                let (index, field_poke) = ps
                    .field_by_name(k)
                    .map_err(|e| format!("Field '{}' error: {}", k, e))?;
                let _v = deserialize_item(field_poke, v)
                    .map_err(|e| format!("Error deserializing field '{}': {}", k, e))?;
                unsafe {
                    ps.mark_initialized(index);
                }
            }
            ps.build_in_place()
        }
        PokeUninit::Enum(_) => todo!(),
        _ => todo!("unsupported poke type"),
    };
    Ok(opaque)
}
