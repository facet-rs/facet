//! Scalar type identification for shapes.

use core::any::TypeId;

use crate::{ConstTypeId, Shape};

/// All scalar types supported out of the box by facet.
///
/// This enum allows identifying whether a [`Shape`] represents a known scalar type
/// (primitives, strings, network addresses, etc.), which is useful for serializers,
/// deserializers, and introspection code.
///
/// # Example
///
/// ```
/// use facet_core::{Facet, ScalarType};
///
/// assert_eq!(u32::SHAPE.scalar_type(), Some(ScalarType::U32));
/// assert_eq!(bool::SHAPE.scalar_type(), Some(ScalarType::Bool));
/// ```
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[non_exhaustive]
pub enum ScalarType {
    /// Unit tuple `()`.
    Unit,
    /// Primitive type `bool`.
    Bool,
    /// Primitive type `char`.
    Char,
    /// Primitive type `str` (or `&str`).
    Str,
    /// `alloc::string::String`.
    #[cfg(feature = "alloc")]
    String,
    /// `alloc::borrow::Cow<'_, str>`.
    #[cfg(feature = "alloc")]
    CowStr,
    /// Primitive type `f32`.
    F32,
    /// Primitive type `f64`.
    F64,
    /// Primitive type `u8`.
    U8,
    /// Primitive type `u16`.
    U16,
    /// Primitive type `u32`.
    U32,
    /// Primitive type `u64`.
    U64,
    /// Primitive type `u128`.
    U128,
    /// Primitive type `usize`.
    USize,
    /// Primitive type `i8`.
    I8,
    /// Primitive type `i16`.
    I16,
    /// Primitive type `i32`.
    I32,
    /// Primitive type `i64`.
    I64,
    /// Primitive type `i128`.
    I128,
    /// Primitive type `isize`.
    ISize,
    /// `core::net::SocketAddr`.
    #[cfg(feature = "net")]
    SocketAddr,
    /// `core::net::IpAddr`.
    #[cfg(feature = "net")]
    IpAddr,
    /// `core::net::Ipv4Addr`.
    #[cfg(feature = "net")]
    Ipv4Addr,
    /// `core::net::Ipv6Addr`.
    #[cfg(feature = "net")]
    Ipv6Addr,
    /// `facet_core::ConstTypeId`.
    ConstTypeId,
}

impl ScalarType {
    /// Infer the scalar type from a shape definition.
    ///
    /// Returns `Some(ScalarType)` if the shape represents a known scalar type,
    /// or `None` for non-scalar types like structs, enums, lists, or maps.
    #[inline]
    pub fn try_from_shape(shape: &Shape) -> Option<Self> {
        shape.scalar_type()
    }
}

impl Shape {
    /// Get the scalar type if this shape represents a scalar.
    ///
    /// Returns `Some(ScalarType)` if this shape corresponds to a known scalar type
    /// (primitives, `String`, `Cow<str>`, network address types, etc.),
    /// or `None` if it's a non-scalar type like a struct, enum, list, or map.
    ///
    /// # Example
    ///
    /// ```
    /// use facet_core::{Facet, ScalarType};
    ///
    /// assert_eq!(bool::SHAPE.scalar_type(), Some(ScalarType::Bool));
    /// assert_eq!(u32::SHAPE.scalar_type(), Some(ScalarType::U32));
    /// assert_eq!(f64::SHAPE.scalar_type(), Some(ScalarType::F64));
    /// ```
    #[inline]
    pub fn scalar_type(&self) -> Option<ScalarType> {
        let type_id = self.id.get();

        #[cfg(feature = "alloc")]
        {
            if type_id == TypeId::of::<alloc::string::String>() {
                return Some(ScalarType::String);
            } else if type_id == TypeId::of::<alloc::borrow::Cow<'_, str>>() {
                return Some(ScalarType::CowStr);
            }
        }

        #[cfg(feature = "net")]
        if type_id == TypeId::of::<core::net::SocketAddr>() {
            return Some(ScalarType::SocketAddr);
        }

        // Check for str type (both bare str and &str)
        if type_id == TypeId::of::<str>() || type_id == TypeId::of::<&str>() {
            return Some(ScalarType::Str);
        }

        if type_id == TypeId::of::<()>() {
            return Some(ScalarType::Unit);
        } else if type_id == TypeId::of::<bool>() {
            return Some(ScalarType::Bool);
        } else if type_id == TypeId::of::<char>() {
            return Some(ScalarType::Char);
        } else if type_id == TypeId::of::<f32>() {
            return Some(ScalarType::F32);
        } else if type_id == TypeId::of::<f64>() {
            return Some(ScalarType::F64);
        } else if type_id == TypeId::of::<u8>() {
            return Some(ScalarType::U8);
        } else if type_id == TypeId::of::<u16>() {
            return Some(ScalarType::U16);
        } else if type_id == TypeId::of::<u32>() {
            return Some(ScalarType::U32);
        } else if type_id == TypeId::of::<u64>() {
            return Some(ScalarType::U64);
        } else if type_id == TypeId::of::<u128>() {
            return Some(ScalarType::U128);
        } else if type_id == TypeId::of::<usize>() {
            return Some(ScalarType::USize);
        } else if type_id == TypeId::of::<i8>() {
            return Some(ScalarType::I8);
        } else if type_id == TypeId::of::<i16>() {
            return Some(ScalarType::I16);
        } else if type_id == TypeId::of::<i32>() {
            return Some(ScalarType::I32);
        } else if type_id == TypeId::of::<i64>() {
            return Some(ScalarType::I64);
        } else if type_id == TypeId::of::<i128>() {
            return Some(ScalarType::I128);
        } else if type_id == TypeId::of::<isize>() {
            return Some(ScalarType::ISize);
        } else if type_id == TypeId::of::<ConstTypeId>() {
            return Some(ScalarType::ConstTypeId);
        }

        #[cfg(feature = "net")]
        {
            use core::net::{IpAddr, Ipv4Addr, Ipv6Addr};

            if type_id == TypeId::of::<IpAddr>() {
                return Some(ScalarType::IpAddr);
            } else if type_id == TypeId::of::<Ipv4Addr>() {
                return Some(ScalarType::Ipv4Addr);
            } else if type_id == TypeId::of::<Ipv6Addr>() {
                return Some(ScalarType::Ipv6Addr);
            }
        }

        None
    }
}
