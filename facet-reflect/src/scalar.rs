//! Re-export of [`ScalarType`] from `facet_core`.
//!
//! This module re-exports scalar type functionality from `facet_core` for backwards
//! compatibility. New code should import directly from `facet_core`.

pub use facet_core::ScalarType;

#[cfg(test)]
mod tests {
    use super::*;

    use core::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    use facet_core::{ConstTypeId, Facet};

    /// Simple check to ensure every scalar type can be loaded from a shape.
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
            ScalarType::try_from_shape(alloc::borrow::Cow::<str>::SHAPE).unwrap()
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

    /// Test that Shape::scalar_type() method works correctly
    #[test]
    fn test_shape_scalar_type_method() {
        assert_eq!(bool::SHAPE.scalar_type(), Some(ScalarType::Bool));
        assert_eq!(u32::SHAPE.scalar_type(), Some(ScalarType::U32));
        assert_eq!(f64::SHAPE.scalar_type(), Some(ScalarType::F64));
        assert_eq!(<()>::SHAPE.scalar_type(), Some(ScalarType::Unit));

        #[cfg(feature = "std")]
        assert_eq!(String::SHAPE.scalar_type(), Some(ScalarType::String));

        // Test non-scalar types return None
        assert_eq!(alloc::vec::Vec::<u8>::SHAPE.scalar_type(), None);
    }
}
