//! Scalar type identification for shapes.

use core::any::TypeId;
use core::net::{IpAddr, Ipv4Addr, Ipv6Addr};

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
    #[cfg(feature = "std")]
    SocketAddr,
    /// `core::net::IpAddr`.
    IpAddr,
    /// `core::net::Ipv4Addr`.
    Ipv4Addr,
    /// `core::net::Ipv6Addr`.
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

        #[cfg(feature = "std")]
        if type_id == TypeId::of::<core::net::SocketAddr>() {
            return Some(ScalarType::SocketAddr);
        }

        // Check for str type (both bare str and &str)
        if type_id == TypeId::of::<str>() || type_id == TypeId::of::<&str>() {
            return Some(ScalarType::Str);
        }

        if type_id == TypeId::of::<()>() {
            Some(ScalarType::Unit)
        } else if type_id == TypeId::of::<bool>() {
            Some(ScalarType::Bool)
        } else if type_id == TypeId::of::<char>() {
            Some(ScalarType::Char)
        } else if type_id == TypeId::of::<f32>() {
            Some(ScalarType::F32)
        } else if type_id == TypeId::of::<f64>() {
            Some(ScalarType::F64)
        } else if type_id == TypeId::of::<u8>() {
            Some(ScalarType::U8)
        } else if type_id == TypeId::of::<u16>() {
            Some(ScalarType::U16)
        } else if type_id == TypeId::of::<u32>() {
            Some(ScalarType::U32)
        } else if type_id == TypeId::of::<u64>() {
            Some(ScalarType::U64)
        } else if type_id == TypeId::of::<u128>() {
            Some(ScalarType::U128)
        } else if type_id == TypeId::of::<usize>() {
            Some(ScalarType::USize)
        } else if type_id == TypeId::of::<i8>() {
            Some(ScalarType::I8)
        } else if type_id == TypeId::of::<i16>() {
            Some(ScalarType::I16)
        } else if type_id == TypeId::of::<i32>() {
            Some(ScalarType::I32)
        } else if type_id == TypeId::of::<i64>() {
            Some(ScalarType::I64)
        } else if type_id == TypeId::of::<i128>() {
            Some(ScalarType::I128)
        } else if type_id == TypeId::of::<isize>() {
            Some(ScalarType::ISize)
        } else if type_id == TypeId::of::<IpAddr>() {
            Some(ScalarType::IpAddr)
        } else if type_id == TypeId::of::<Ipv4Addr>() {
            Some(ScalarType::Ipv4Addr)
        } else if type_id == TypeId::of::<Ipv6Addr>() {
            Some(ScalarType::Ipv6Addr)
        } else if type_id == TypeId::of::<ConstTypeId>() {
            Some(ScalarType::ConstTypeId)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Facet;

    #[test]
    fn test_scalar_type_primitives() {
        assert_eq!(<()>::SHAPE.scalar_type(), Some(ScalarType::Unit));
        assert_eq!(bool::SHAPE.scalar_type(), Some(ScalarType::Bool));
        assert_eq!(char::SHAPE.scalar_type(), Some(ScalarType::Char));
        assert_eq!(<&str>::SHAPE.scalar_type(), Some(ScalarType::Str));
        assert_eq!(f32::SHAPE.scalar_type(), Some(ScalarType::F32));
        assert_eq!(f64::SHAPE.scalar_type(), Some(ScalarType::F64));
    }

    #[test]
    fn test_scalar_type_integers() {
        assert_eq!(u8::SHAPE.scalar_type(), Some(ScalarType::U8));
        assert_eq!(u16::SHAPE.scalar_type(), Some(ScalarType::U16));
        assert_eq!(u32::SHAPE.scalar_type(), Some(ScalarType::U32));
        assert_eq!(u64::SHAPE.scalar_type(), Some(ScalarType::U64));
        assert_eq!(u128::SHAPE.scalar_type(), Some(ScalarType::U128));
        assert_eq!(usize::SHAPE.scalar_type(), Some(ScalarType::USize));
        assert_eq!(i8::SHAPE.scalar_type(), Some(ScalarType::I8));
        assert_eq!(i16::SHAPE.scalar_type(), Some(ScalarType::I16));
        assert_eq!(i32::SHAPE.scalar_type(), Some(ScalarType::I32));
        assert_eq!(i64::SHAPE.scalar_type(), Some(ScalarType::I64));
        assert_eq!(i128::SHAPE.scalar_type(), Some(ScalarType::I128));
        assert_eq!(isize::SHAPE.scalar_type(), Some(ScalarType::ISize));
    }

    #[test]
    fn test_scalar_type_network() {
        assert_eq!(IpAddr::SHAPE.scalar_type(), Some(ScalarType::IpAddr));
        assert_eq!(Ipv4Addr::SHAPE.scalar_type(), Some(ScalarType::Ipv4Addr));
        assert_eq!(Ipv6Addr::SHAPE.scalar_type(), Some(ScalarType::Ipv6Addr));
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_scalar_type_std() {
        assert_eq!(
            core::net::SocketAddr::SHAPE.scalar_type(),
            Some(ScalarType::SocketAddr)
        );
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn test_scalar_type_alloc() {
        assert_eq!(
            alloc::string::String::SHAPE.scalar_type(),
            Some(ScalarType::String)
        );
        assert_eq!(
            alloc::borrow::Cow::<str>::SHAPE.scalar_type(),
            Some(ScalarType::CowStr)
        );
    }

    #[test]
    fn test_scalar_type_const_type_id() {
        assert_eq!(
            ConstTypeId::SHAPE.scalar_type(),
            Some(ScalarType::ConstTypeId)
        );
    }

    #[test]
    fn test_try_from_shape() {
        // Test that ScalarType::try_from_shape works the same as shape.scalar_type()
        assert_eq!(
            ScalarType::try_from_shape(u32::SHAPE),
            u32::SHAPE.scalar_type()
        );
    }
}
