use core::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use facet_core::{ConstTypeId, Shape};

/// All scalar types supported out of the box by peek and poke.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[non_exhaustive]
pub enum ScalarType {
    /// Unit tuple `()`.
    Unit,
    /// Primitive type `bool`.
    Bool,
    /// Primitive type `char`.
    Char,
    /// Primitive type `str`.
    Str,
    /// `std::string::String`.
    String,
    /// `std::borrow::Cow<'_, str>`.
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
    /// `facet_core::typeid::ConstTypeId`.
    ConstTypeId,
}

impl ScalarType {
    /// Infer the type from a shape definition.
    pub fn try_from_shape(shape: &'static Shape) -> Option<Self> {
        #[cfg(feature = "std")]
        if shape.id == ConstTypeId::of::<String>() {
            return Some(ScalarType::String);
        } else if shape.id == ConstTypeId::of::<alloc::borrow::Cow<'_, str>>() {
            return Some(ScalarType::CowStr);
        } else if shape.id == ConstTypeId::of::<core::net::SocketAddr>() {
            return Some(ScalarType::SocketAddr);
        }

        if shape.id == ConstTypeId::of::<()>() {
            Some(Self::Unit)
        } else if shape.id == ConstTypeId::of::<bool>() {
            Some(ScalarType::Bool)
        } else if shape.id == ConstTypeId::of::<char>() {
            Some(ScalarType::Char)
        } else if shape.id == ConstTypeId::of::<&str>() {
            Some(ScalarType::Str)
        } else if shape.id == ConstTypeId::of::<f32>() {
            Some(ScalarType::F32)
        } else if shape.id == ConstTypeId::of::<f64>() {
            Some(ScalarType::F64)
        } else if shape.id == ConstTypeId::of::<u8>() {
            Some(ScalarType::U8)
        } else if shape.id == ConstTypeId::of::<u16>() {
            Some(ScalarType::U16)
        } else if shape.id == ConstTypeId::of::<u32>() {
            Some(ScalarType::U32)
        } else if shape.id == ConstTypeId::of::<u64>() {
            Some(ScalarType::U64)
        } else if shape.id == ConstTypeId::of::<u128>() {
            Some(ScalarType::U128)
        } else if shape.id == ConstTypeId::of::<usize>() {
            Some(ScalarType::USize)
        } else if shape.id == ConstTypeId::of::<i8>() {
            Some(ScalarType::I8)
        } else if shape.id == ConstTypeId::of::<i16>() {
            Some(ScalarType::I16)
        } else if shape.id == ConstTypeId::of::<i32>() {
            Some(ScalarType::I32)
        } else if shape.id == ConstTypeId::of::<i64>() {
            Some(ScalarType::I64)
        } else if shape.id == ConstTypeId::of::<i128>() {
            Some(ScalarType::I128)
        } else if shape.id == ConstTypeId::of::<isize>() {
            Some(ScalarType::ISize)
        } else if shape.id == ConstTypeId::of::<IpAddr>() {
            Some(ScalarType::IpAddr)
        } else if shape.id == ConstTypeId::of::<Ipv4Addr>() {
            Some(ScalarType::Ipv4Addr)
        } else if shape.id == ConstTypeId::of::<Ipv6Addr>() {
            Some(ScalarType::Ipv6Addr)
        } else if shape.id == ConstTypeId::of::<ConstTypeId>() {
            Some(ScalarType::ConstTypeId)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use core::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    use facet_core::Facet;

    /// Simple check to ensure every can be loaded from a shape.
    #[test]
    fn test_ensure_try_from_shape() {
        assert_eq!(
            ScalarType::Unit,
            ScalarType::try_from_shape(<()>::SHAPE).unwrap()
        );
        assert_eq!(
            ScalarType::Bool,
            ScalarType::try_from_shape(bool::SHAPE).unwrap()
        );
        assert_eq!(
            ScalarType::Str,
            ScalarType::try_from_shape(<&str>::SHAPE).unwrap()
        );
        #[cfg(feature = "std")]
        assert_eq!(
            ScalarType::String,
            ScalarType::try_from_shape(String::SHAPE).unwrap()
        );
        #[cfg(feature = "std")]
        assert_eq!(
            ScalarType::CowStr,
            ScalarType::try_from_shape(alloc::borrow::Cow::SHAPE).unwrap()
        );
        assert_eq!(
            ScalarType::F32,
            ScalarType::try_from_shape(f32::SHAPE).unwrap()
        );
        assert_eq!(
            ScalarType::F64,
            ScalarType::try_from_shape(f64::SHAPE).unwrap()
        );
        assert_eq!(
            ScalarType::U8,
            ScalarType::try_from_shape(u8::SHAPE).unwrap()
        );
        assert_eq!(
            ScalarType::U16,
            ScalarType::try_from_shape(u16::SHAPE).unwrap()
        );
        assert_eq!(
            ScalarType::U32,
            ScalarType::try_from_shape(u32::SHAPE).unwrap()
        );
        assert_eq!(
            ScalarType::U64,
            ScalarType::try_from_shape(u64::SHAPE).unwrap()
        );
        assert_eq!(
            ScalarType::U128,
            ScalarType::try_from_shape(u128::SHAPE).unwrap()
        );
        assert_eq!(
            ScalarType::USize,
            ScalarType::try_from_shape(usize::SHAPE).unwrap()
        );
        assert_eq!(
            ScalarType::I8,
            ScalarType::try_from_shape(i8::SHAPE).unwrap()
        );
        assert_eq!(
            ScalarType::I16,
            ScalarType::try_from_shape(i16::SHAPE).unwrap()
        );
        assert_eq!(
            ScalarType::I32,
            ScalarType::try_from_shape(i32::SHAPE).unwrap()
        );
        assert_eq!(
            ScalarType::I64,
            ScalarType::try_from_shape(i64::SHAPE).unwrap()
        );
        assert_eq!(
            ScalarType::I128,
            ScalarType::try_from_shape(i128::SHAPE).unwrap()
        );
        assert_eq!(
            ScalarType::ISize,
            ScalarType::try_from_shape(isize::SHAPE).unwrap()
        );
        #[cfg(feature = "std")]
        assert_eq!(
            ScalarType::SocketAddr,
            ScalarType::try_from_shape(core::net::SocketAddr::SHAPE).unwrap()
        );
        assert_eq!(
            ScalarType::IpAddr,
            ScalarType::try_from_shape(IpAddr::SHAPE).unwrap()
        );
        assert_eq!(
            ScalarType::Ipv4Addr,
            ScalarType::try_from_shape(Ipv4Addr::SHAPE).unwrap()
        );
        assert_eq!(
            ScalarType::Ipv6Addr,
            ScalarType::try_from_shape(Ipv6Addr::SHAPE).unwrap()
        );
        assert_eq!(
            ScalarType::ConstTypeId,
            ScalarType::try_from_shape(ConstTypeId::SHAPE).unwrap()
        );
    }
}
