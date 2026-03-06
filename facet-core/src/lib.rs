#![cfg_attr(not(feature = "std"), no_std)]
// Enable portable_simd when available (detected via autocfg in build.rs)
#![cfg_attr(has_portable_simd, feature(portable_simd))]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
// Allow uncommon unicode in the 𝟋 prelude module
#![allow(uncommon_codepoints)]
//!
//! [![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-core/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
//! [![crates.io](https://img.shields.io/crates/v/facet-core.svg)](https://crates.io/crates/facet-core)
//! [![documentation](https://docs.rs/facet-core/badge.svg)](https://docs.rs/facet-core)
//! [![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-core.svg)](./LICENSE)
//! [![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)
//!
//! Defines the `Facet` trait and implements it for a lot (a LOT) of builtin types.
//!
//! This crate is foundational to facet's reflection capabilities, providing the type system that enables runtime type manipulation.
//!
#![doc = include_str!("../../docs/readme-footer.md")]

#[cfg(feature = "alloc")]
extern crate alloc;

// Core type definitions (merged from facet-core-types)
mod types;
pub use types::*;

// Write trait for serializers
mod write;
pub use write::Write;

// Implementations of the Shape trait
mod impls;
#[cfg(feature = "semver")]
pub use semver;

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
    /// The shape of this type, including: whether it's a Struct, an Enum, something else?
    ///
    /// All its fields, with their names, types, attributes, doc comments, etc.
    /// VTables for list operations, set operations, map operations, option operations,
    /// and implementations for Display, Debug, etc.—marker traits like Send, Sync, Copy, Eq,
    /// and probably other things I'm forgetting.
    const SHAPE: &'static Shape;
}

/// Returns the shape of a type as a function pointer.
///
/// This is a helper for lazy shape initialization in field definitions.
/// Using a function pointer instead of a direct reference moves const
/// evaluation from compile time to runtime, improving compile times.
///
/// # Example
///
/// ```ignore
/// use facet_core::{FieldBuilder, shape_of};
///
/// // In field definitions:
/// FieldBuilder::new("my_field", shape_of::<i32>, 0)
/// ```
#[inline]
pub const fn shape_of<'a, T: Facet<'a>>() -> &'static Shape {
    T::SHAPE
}

/// Ultra-compact prelude for derive macro codegen (the "digamma" prelude).
///
/// All exports are prefixed with `𝟋` to avoid collisions after `use ::facet::𝟋::*;`
///
/// The `𝟋` character (U+1D4CB, Mathematical Script Small F, "digamma") was chosen because:
/// - It's a valid Rust identifier (XID_Start)
/// - It's visually distinctive ("this is internal macro stuff")
/// - It won't collide with any user-defined names
#[doc(hidden)]
#[allow(nonstandard_style)]
pub mod 𝟋 {
    // === Type aliases ===
    pub use crate::Attr as 𝟋Attr;
    pub use crate::Def as 𝟋Def;
    pub use crate::DefaultSource as 𝟋DS;
    pub use crate::EnumRepr as 𝟋ERpr;
    pub use crate::EnumType as 𝟋ETy;
    pub use crate::EnumTypeBuilder as 𝟋ETyB;
    pub use crate::Facet as 𝟋Fct;
    pub use crate::Field as 𝟋Fld;
    pub use crate::FieldBuilder as 𝟋FldB;
    pub use crate::FieldFlags as 𝟋FF;
    pub use crate::HashProxy as 𝟋HP;
    pub use crate::MarkerTraits as 𝟋Mt;
    pub use crate::Repr as 𝟋Repr;
    pub use crate::Shape as 𝟋Shp;
    pub use crate::ShapeBuilder as 𝟋ShpB;
    pub use crate::ShapeFlags as 𝟋ShpF;
    pub use crate::ShapeRef as 𝟋ShpR;
    pub use crate::StructKind as 𝟋Sk;
    pub use crate::StructType as 𝟋STy;
    pub use crate::StructTypeBuilder as 𝟋STyB;
    pub use crate::Type as 𝟋Ty;
    pub use crate::UserType as 𝟋UTy;
    pub use crate::VTableDirect as 𝟋VtD;
    pub use crate::VTableErased as 𝟋VtE;
    pub use crate::Variance as 𝟋Vnc;
    pub use crate::VarianceDesc as 𝟋VncD;
    pub use crate::Variant as 𝟋Var;
    pub use crate::VariantBuilder as 𝟋VarB;
    pub use ::core::option::Option::None as 𝟋None;
    pub use ::core::option::Option::Some as 𝟋Some;
    pub use ::core::result::Result::Err as 𝟋Err;
    pub use ::core::result::Result::Ok as 𝟋Ok;

    // === Core utility re-exports ===
    pub use ::core::mem::forget as 𝟋forget;
    pub use ::core::mem::transmute as 𝟋transmute;
    pub use ::core::ptr::drop_in_place as 𝟋drop_in_place;

    /// Helper to get shape of a type as a function - monomorphized per type
    pub use crate::shape_of as 𝟋shp;

    // === Declaration ID ===
    pub use crate::DeclId as 𝟋DId;
    pub use crate::decl_id_hash as 𝟋dih;

    // === Constants ===
    /// Empty attributes slice
    pub const 𝟋NOAT: &[crate::FieldAttribute] = &[];
    /// Empty doc slice
    pub const 𝟋NODOC: &[&str] = &[];
    /// Empty flags
    pub const 𝟋NOFL: crate::FieldFlags = crate::FieldFlags::empty();
    /// Computed variance (for non-opaque types) - bivariant base with field walking fallback
    pub const 𝟋CV: crate::VarianceDesc = crate::VarianceDesc::BIVARIANT;

    // === Type Aliases ===
    /// PhantomData type for shadow structs, invariant with respect to lifetime `'a`.
    pub type 𝟋Ph<'a> = ::core::marker::PhantomData<*mut &'a ()>;

    /// String type for proxy conversion errors (requires alloc feature).
    #[cfg(feature = "alloc")]
    pub type 𝟋Str = ::alloc::string::String;

    /// Fallback when alloc is not available - proxy requires alloc at runtime,
    /// but we need a type for compilation in no_std contexts.
    #[cfg(not(feature = "alloc"))]
    pub type 𝟋Str = &'static str;

    /// Result type alias for macro-generated code.
    pub type 𝟋Result<T, E> = ::core::result::Result<T, E>;

    // === Helper functions ===
    /// Returns `drop_in_place::<T>` as a function pointer for vtable construction.
    pub const fn 𝟋drop_for<T>() -> unsafe fn(*mut T) {
        ::core::ptr::drop_in_place::<T>
    }

    /// Returns a default_in_place function pointer for TypeOpsDirect.
    /// # Safety
    /// The pointer must point to uninitialized memory of sufficient size and alignment for T.
    pub const fn 𝟋default_for<T: Default>() -> unsafe fn(*mut T) {
        unsafe fn default_in_place<T: Default>(ptr: *mut T) {
            unsafe { ptr.write(T::default()) };
        }
        default_in_place::<T>
    }

    /// Returns a clone_into function pointer for TypeOpsDirect.
    /// # Safety
    /// - `src` must point to a valid, initialized value of type T
    /// - `dst` must point to uninitialized memory of sufficient size and alignment for T
    pub const fn 𝟋clone_for<T: Clone>() -> unsafe fn(*const T, *mut T) {
        unsafe fn clone_into<T: Clone>(src: *const T, dst: *mut T) {
            unsafe { dst.write((*src).clone()) };
        }
        clone_into::<T>
    }

    // === TypeOpsIndirect helpers ===
    // These take OxPtrMut/OxPtrConst and work with wide pointers

    /// Returns a drop_in_place function pointer for TypeOpsIndirect.
    pub const fn 𝟋indirect_drop_for<T>() -> unsafe fn(crate::OxPtrMut) {
        unsafe fn drop_in_place<T>(ox: crate::OxPtrMut) {
            unsafe { ::core::ptr::drop_in_place(ox.ptr().as_ptr::<T>() as *mut T) };
        }
        drop_in_place::<T>
    }

    /// Returns a default_in_place function pointer for TypeOpsIndirect.
    pub const fn 𝟋indirect_default_for<T: Default>() -> unsafe fn(crate::OxPtrUninit) -> bool {
        unsafe fn default_in_place<T: Default>(ox: crate::OxPtrUninit) -> bool {
            unsafe { ox.put(T::default()) };
            true
        }
        default_in_place::<T>
    }

    /// Returns a clone_into function pointer for TypeOpsIndirect.
    pub const fn 𝟋indirect_clone_for<T: Clone>() -> unsafe fn(crate::OxPtrConst, crate::OxPtrMut) {
        unsafe fn clone_into<T: Clone>(src: crate::OxPtrConst, dst: crate::OxPtrMut) {
            let src_val = unsafe { &*(src.ptr().as_byte_ptr() as *const T) };
            unsafe { dst.ptr().as_uninit().put(src_val.clone()) };
        }
        clone_into::<T>
    }

    // === Specialization ===
    pub use crate::types::specialization::impls;
    pub use crate::types::specialization::{
        Spez, SpezCloneIntoNo, SpezCloneIntoYes, SpezDebugNo, SpezDebugYes, SpezDefaultInPlaceNo,
        SpezDefaultInPlaceYes, SpezDisplayNo, SpezDisplayYes, SpezEmpty, SpezHashNo, SpezHashYes,
        SpezOrdNo, SpezOrdYes, SpezParseNo, SpezParseYes, SpezPartialEqNo, SpezPartialEqYes,
        SpezPartialOrdNo, SpezPartialOrdYes,
    };
}
