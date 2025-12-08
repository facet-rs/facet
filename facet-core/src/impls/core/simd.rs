//! Facet implementations for `core::simd` types.
//!
//! Requires:
//! - The `simd` feature on `facet-core`
//! - Nightly Rust with `portable_simd` feature available
//!
//! This module adds 84 type implementations which increases compile time by ~100ms.

// Requires both the `simd` feature AND nightly portable_simd (detected via autocfg)
#![cfg(all(feature = "simd", has_portable_simd))]

use core::simd::Simd;

use crate::{
    Def, Facet, Shape, ShapeBuilder, Type, TypeOpsDirect, UserType, VTableDirect, type_ops_direct,
    vtable_direct,
};

/// Implements `Facet` for a specific SIMD type alias.
///
/// SIMD types are treated as opaque scalars since they have special alignment
/// requirements and should be treated atomically for reflection purposes.
///
/// Traits are split into:
/// - VTable traits: Debug, Display, Hash, PartialEq, PartialOrd, Ord
/// - TypeOps traits: Default, Clone
macro_rules! impl_facet_for_simd {
    // With Hash (for integer types)
    ($elem:ty, $lanes:expr, $alias:ident => with_hash) => {
        unsafe impl Facet<'_> for Simd<$elem, $lanes> {
            const SHAPE: &'static Shape = &const {
                const VTABLE: VTableDirect = vtable_direct!(Simd<$elem, $lanes> =>
                    Debug, PartialEq, PartialOrd, Hash,
                );
                const TYPE_OPS: TypeOpsDirect = type_ops_direct!(Simd<$elem, $lanes> =>
                    Default, Clone,
                );

                ShapeBuilder::for_sized::<Simd<$elem, $lanes>>(stringify!($alias))
                    .ty(Type::User(UserType::Opaque))
                    .def(Def::Scalar)
                    .vtable_direct(&VTABLE)
                    .type_ops_direct(&TYPE_OPS)
                    .copy()
                    .send()
                    .sync()
                    .build()
            };
        }
    };

    // Without Hash (for float types)
    ($elem:ty, $lanes:expr, $alias:ident => no_hash) => {
        unsafe impl Facet<'_> for Simd<$elem, $lanes> {
            const SHAPE: &'static Shape = &const {
                const VTABLE: VTableDirect = vtable_direct!(Simd<$elem, $lanes> =>
                    Debug, PartialEq, PartialOrd,
                );
                const TYPE_OPS: TypeOpsDirect = type_ops_direct!(Simd<$elem, $lanes> =>
                    Default, Clone,
                );

                ShapeBuilder::for_sized::<Simd<$elem, $lanes>>(stringify!($alias))
                    .ty(Type::User(UserType::Opaque))
                    .def(Def::Scalar)
                    .vtable_direct(&VTABLE)
                    .type_ops_direct(&TYPE_OPS)
                    .copy()
                    .send()
                    .sync()
                    .build()
            };
        }
    };
}

// f32 SIMD types (no Hash - floats don't implement Hash)
impl_facet_for_simd!(f32, 1, f32x1 => no_hash);
impl_facet_for_simd!(f32, 2, f32x2 => no_hash);
impl_facet_for_simd!(f32, 4, f32x4 => no_hash);
impl_facet_for_simd!(f32, 8, f32x8 => no_hash);
impl_facet_for_simd!(f32, 16, f32x16 => no_hash);
impl_facet_for_simd!(f32, 32, f32x32 => no_hash);
impl_facet_for_simd!(f32, 64, f32x64 => no_hash);

// f64 SIMD types (no Hash - floats don't implement Hash)
impl_facet_for_simd!(f64, 1, f64x1 => no_hash);
impl_facet_for_simd!(f64, 2, f64x2 => no_hash);
impl_facet_for_simd!(f64, 4, f64x4 => no_hash);
impl_facet_for_simd!(f64, 8, f64x8 => no_hash);
impl_facet_for_simd!(f64, 16, f64x16 => no_hash);
impl_facet_for_simd!(f64, 32, f64x32 => no_hash);
impl_facet_for_simd!(f64, 64, f64x64 => no_hash);

// i8 SIMD types
impl_facet_for_simd!(i8, 1, i8x1 => with_hash);
impl_facet_for_simd!(i8, 2, i8x2 => with_hash);
impl_facet_for_simd!(i8, 4, i8x4 => with_hash);
impl_facet_for_simd!(i8, 8, i8x8 => with_hash);
impl_facet_for_simd!(i8, 16, i8x16 => with_hash);
impl_facet_for_simd!(i8, 32, i8x32 => with_hash);
impl_facet_for_simd!(i8, 64, i8x64 => with_hash);

// i16 SIMD types
impl_facet_for_simd!(i16, 1, i16x1 => with_hash);
impl_facet_for_simd!(i16, 2, i16x2 => with_hash);
impl_facet_for_simd!(i16, 4, i16x4 => with_hash);
impl_facet_for_simd!(i16, 8, i16x8 => with_hash);
impl_facet_for_simd!(i16, 16, i16x16 => with_hash);
impl_facet_for_simd!(i16, 32, i16x32 => with_hash);
impl_facet_for_simd!(i16, 64, i16x64 => with_hash);

// i32 SIMD types
impl_facet_for_simd!(i32, 1, i32x1 => with_hash);
impl_facet_for_simd!(i32, 2, i32x2 => with_hash);
impl_facet_for_simd!(i32, 4, i32x4 => with_hash);
impl_facet_for_simd!(i32, 8, i32x8 => with_hash);
impl_facet_for_simd!(i32, 16, i32x16 => with_hash);
impl_facet_for_simd!(i32, 32, i32x32 => with_hash);
impl_facet_for_simd!(i32, 64, i32x64 => with_hash);

// i64 SIMD types
impl_facet_for_simd!(i64, 1, i64x1 => with_hash);
impl_facet_for_simd!(i64, 2, i64x2 => with_hash);
impl_facet_for_simd!(i64, 4, i64x4 => with_hash);
impl_facet_for_simd!(i64, 8, i64x8 => with_hash);
impl_facet_for_simd!(i64, 16, i64x16 => with_hash);
impl_facet_for_simd!(i64, 32, i64x32 => with_hash);
impl_facet_for_simd!(i64, 64, i64x64 => with_hash);

// isize SIMD types
impl_facet_for_simd!(isize, 1, isizex1 => with_hash);
impl_facet_for_simd!(isize, 2, isizex2 => with_hash);
impl_facet_for_simd!(isize, 4, isizex4 => with_hash);
impl_facet_for_simd!(isize, 8, isizex8 => with_hash);
impl_facet_for_simd!(isize, 16, isizex16 => with_hash);
impl_facet_for_simd!(isize, 32, isizex32 => with_hash);
impl_facet_for_simd!(isize, 64, isizex64 => with_hash);

// u8 SIMD types
impl_facet_for_simd!(u8, 1, u8x1 => with_hash);
impl_facet_for_simd!(u8, 2, u8x2 => with_hash);
impl_facet_for_simd!(u8, 4, u8x4 => with_hash);
impl_facet_for_simd!(u8, 8, u8x8 => with_hash);
impl_facet_for_simd!(u8, 16, u8x16 => with_hash);
impl_facet_for_simd!(u8, 32, u8x32 => with_hash);
impl_facet_for_simd!(u8, 64, u8x64 => with_hash);

// u16 SIMD types
impl_facet_for_simd!(u16, 1, u16x1 => with_hash);
impl_facet_for_simd!(u16, 2, u16x2 => with_hash);
impl_facet_for_simd!(u16, 4, u16x4 => with_hash);
impl_facet_for_simd!(u16, 8, u16x8 => with_hash);
impl_facet_for_simd!(u16, 16, u16x16 => with_hash);
impl_facet_for_simd!(u16, 32, u16x32 => with_hash);
impl_facet_for_simd!(u16, 64, u16x64 => with_hash);

// u32 SIMD types
impl_facet_for_simd!(u32, 1, u32x1 => with_hash);
impl_facet_for_simd!(u32, 2, u32x2 => with_hash);
impl_facet_for_simd!(u32, 4, u32x4 => with_hash);
impl_facet_for_simd!(u32, 8, u32x8 => with_hash);
impl_facet_for_simd!(u32, 16, u32x16 => with_hash);
impl_facet_for_simd!(u32, 32, u32x32 => with_hash);
impl_facet_for_simd!(u32, 64, u32x64 => with_hash);

// u64 SIMD types
impl_facet_for_simd!(u64, 1, u64x1 => with_hash);
impl_facet_for_simd!(u64, 2, u64x2 => with_hash);
impl_facet_for_simd!(u64, 4, u64x4 => with_hash);
impl_facet_for_simd!(u64, 8, u64x8 => with_hash);
impl_facet_for_simd!(u64, 16, u64x16 => with_hash);
impl_facet_for_simd!(u64, 32, u64x32 => with_hash);
impl_facet_for_simd!(u64, 64, u64x64 => with_hash);

// usize SIMD types
impl_facet_for_simd!(usize, 1, usizex1 => with_hash);
impl_facet_for_simd!(usize, 2, usizex2 => with_hash);
impl_facet_for_simd!(usize, 4, usizex4 => with_hash);
impl_facet_for_simd!(usize, 8, usizex8 => with_hash);
impl_facet_for_simd!(usize, 16, usizex16 => with_hash);
impl_facet_for_simd!(usize, 32, usizex32 => with_hash);
impl_facet_for_simd!(usize, 64, usizex64 => with_hash);
