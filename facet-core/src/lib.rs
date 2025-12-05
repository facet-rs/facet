#![cfg_attr(not(feature = "std"), no_std)]
// Enable portable_simd when available (detected via autocfg in build.rs)
#![cfg_attr(has_portable_simd, feature(portable_simd))]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
// Allow uncommon unicode in the 𝟋 prelude module
#![allow(uncommon_codepoints)]
#![doc = include_str!("../README.md")]

#[cfg(feature = "alloc")]
extern crate alloc;

mod macros;
pub use macros::*;

// Macros for vtable fields.

/// Includes vtable fields for Display/Debug.
#[macro_export]
macro_rules! vtable_fmt {
    ($($tt:tt)*) => { $($tt)* };
}

/// Includes vtable fields for PartialEq/PartialOrd/Ord.
#[macro_export]
macro_rules! vtable_cmp {
    ($($tt:tt)*) => { $($tt)* };
}

/// Includes vtable fields for Hash.
#[macro_export]
macro_rules! vtable_hash {
    ($($tt:tt)*) => { $($tt)* };
}

// Opaque pointer utilities
mod ptr;
pub use ptr::*;

// Opaque wrapper utility
mod opaque;
pub use opaque::*;

// Specialization utilities
pub mod spez;

// Definition for `core::` types
mod impls_core;

// Definition for `alloc::` types
#[cfg(feature = "alloc")]
mod impls_alloc;

// Definition for `std::` types (that aren't in `alloc` or `core)
#[cfg(feature = "std")]
mod impls_std;

#[cfg(feature = "bytes")]
mod impls_bytes;

#[cfg(feature = "camino")]
mod impls_camino;

#[cfg(feature = "ordered-float")]
mod impls_ordered_float;

#[cfg(feature = "uuid")]
mod impls_uuid;

#[cfg(feature = "ulid")]
mod impls_ulid;

#[cfg(feature = "time")]
mod impls_time;

#[cfg(feature = "chrono")]
mod impls_chrono;

#[cfg(feature = "url")]
mod impls_url;

#[cfg(feature = "jiff02")]
mod impls_jiff;

#[cfg(feature = "num-complex")]
mod impls_num_complex;

#[cfg(feature = "ruint")]
mod impls_ruint;

#[cfg(feature = "indexmap")]
mod impls_indexmap;

// Const type Id
mod typeid;
pub use typeid::*;

// Scalar type identification
mod scalar;
pub use scalar::*;

// Type definitions
mod types;
#[allow(unused_imports)] // wtf clippy? we're re-exporting?
pub use types::*;

/// Allows querying the [`Shape`] of a type, which in turn lets us inspect any fields, build a value of
/// this type progressively, etc.
///
/// The `'facet` lifetime allows `Facet` to be derived for types that borrow from something else.
///
/// # Safety
///
/// If you implement this wrong, all the safe abstractions in `facet-reflect`,
/// all the serializers, deserializers, the entire ecosystem is unsafe.
///
/// You're responsible for describing the type layout properly, and annotating all the invariants.
pub unsafe trait Facet<'facet>: 'facet {
    /// The shape of this type
    ///
    /// Shape embeds all other constants of this trait.
    const SHAPE: &'static Shape;
}

mod shape_util;

// Write trait for serializers
mod write;
pub use write::Write;

/// Re-export paste for use in macros
#[doc(hidden)]
pub use paste;

/// Ultra-compact prelude for derive macro codegen.
///
/// All exports are prefixed with `𝟋` to avoid collisions after `use ::facet::𝟋::*;`
///
/// The `𝟋` character (U+1D4CB, Mathematical Script Small F) was chosen because:
/// - It's a valid Rust identifier (XID_Start)
/// - It's visually distinctive ("this is internal macro stuff")
/// - It won't collide with any user-defined names
#[doc(hidden)]
#[allow(nonstandard_style)]
pub mod 𝟋 {
    // === Type aliases ===
    pub use crate::Def as 𝟋Def;
    pub use crate::EnumRepr as 𝟋ERpr;
    pub use crate::EnumType as 𝟋ETy;
    pub use crate::Facet as 𝟋Fct;
    pub use crate::Field as 𝟋Fld;
    pub use crate::MarkerTraits as 𝟋Mt;
    pub use crate::Repr as 𝟋Repr;
    pub use crate::Shape as 𝟋Shp;
    pub use crate::ShapeRef as 𝟋ShpR;
    pub use crate::StructKind as 𝟋Sk;
    pub use crate::StructType as 𝟋STy;
    pub use crate::Type as 𝟋Ty;
    pub use crate::UserType as 𝟋UTy;
    pub use crate::ValueVTable as 𝟋Vt;
    pub use crate::Variant as 𝟋Var;

    // === Builders ===
    pub use crate::EnumTypeBuilder as 𝟋ETyB;
    pub use crate::FieldBuilder as 𝟋FldB;
    pub use crate::ShapeBuilder as 𝟋ShpB;
    pub use crate::StructTypeBuilder as 𝟋STyB;
    pub use crate::ValueVTableBuilder as 𝟋VtB;
    pub use crate::VariantBuilder as 𝟋VarB;

    // === ShapeRef variants (for compact codegen) ===
    /// Static shape reference (default, most efficient) - use for most fields
    pub const fn 𝟋ShpS(shape: &'static crate::Shape) -> crate::ShapeRef {
        crate::ShapeRef::Static(shape)
    }
    /// Lazy shape reference (for recursive types) - use with #[facet(recursive_type)]
    pub const fn 𝟋ShpL(f: fn() -> &'static crate::Shape) -> crate::ShapeRef {
        crate::ShapeRef::Lazy(f)
    }

    // === Constants ===
    /// Empty attributes slice
    pub const 𝟋NOAT: &[crate::FieldAttribute] = &[];
    /// Empty doc slice
    pub const 𝟋NODOC: &[&str] = &[];

    // === Type Aliases ===
    /// PhantomData type for shadow structs, invariant in lifetime `'a`.
    pub type 𝟋Ph<'a> = ::core::marker::PhantomData<*mut &'a ()>;
}
