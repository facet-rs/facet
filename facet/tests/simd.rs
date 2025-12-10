// Only run these tests when the simd feature is enabled AND portable_simd is available
#![cfg(all(feature = "simd", has_portable_simd))]
#![feature(portable_simd)]

use core::simd::Simd;
use facet::{Def, Facet, Type, UserType};

#[test]
fn simd_f32x4_shape() {
    let shape = <Simd<f32, 4> as Facet>::SHAPE;
    assert_eq!(shape.type_identifier, "f32x4");
    assert!(matches!(shape.ty, Type::User(UserType::Opaque)));
    assert!(matches!(shape.def, Def::Scalar));
}

#[test]
fn simd_f64x8_shape() {
    let shape = <Simd<f64, 8> as Facet>::SHAPE;
    assert_eq!(shape.type_identifier, "f64x8");
    assert!(matches!(shape.ty, Type::User(UserType::Opaque)));
    assert!(matches!(shape.def, Def::Scalar));
}

#[test]
fn simd_f32x16_shape() {
    let shape = <Simd<f32, 16> as Facet>::SHAPE;
    assert_eq!(shape.type_identifier, "f32x16");
    assert!(matches!(shape.ty, Type::User(UserType::Opaque)));
    assert!(matches!(shape.def, Def::Scalar));
}
