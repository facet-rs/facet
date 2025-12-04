// Only run these tests when both the feature is enabled and we're on nightly
#![cfg(all(feature = "nightly", nightly))]
#![feature(portable_simd)]

use core::simd::Simd;
use facet_core::{Def, Facet, Type, UserType};
use facet_testhelpers::test;

#[test]
fn simd_f32x4_shape() {
    let shape = <Simd<f32, 4> as Facet>::SHAPE;
    assert_eq!(shape.type_identifier, "f32x4");
    assert!(matches!(shape.ty, Type::User(UserType::Opaque)));
    assert!(matches!(shape.def, Def::Scalar));
}

#[test]
fn simd_i32x8_shape() {
    let shape = <Simd<i32, 8> as Facet>::SHAPE;
    assert_eq!(shape.type_identifier, "i32x8");
    assert!(matches!(shape.ty, Type::User(UserType::Opaque)));
    assert!(matches!(shape.def, Def::Scalar));
}

#[test]
fn simd_u8x16_shape() {
    let shape = <Simd<u8, 16> as Facet>::SHAPE;
    assert_eq!(shape.type_identifier, "u8x16");
    assert!(matches!(shape.ty, Type::User(UserType::Opaque)));
    assert!(matches!(shape.def, Def::Scalar));
}

#[test]
fn simd_f64x2_shape() {
    let shape = <Simd<f64, 2> as Facet>::SHAPE;
    assert_eq!(shape.type_identifier, "f64x2");
    assert!(matches!(shape.ty, Type::User(UserType::Opaque)));
    assert!(matches!(shape.def, Def::Scalar));
}

#[test]
fn simd_type_name() {
    let shape = <Simd<f32, 4> as Facet>::SHAPE;
    assert_eq!(shape.to_string(), "f32x4");
}
