//! Core type definitions for facet reflection.

/// Builder macro for generating builder patterns
mod builder_macro;

/// `Shape` definition
mod shape;
pub use shape::*;

/// `VTable` definition (with erased vtable, builders, etc.)
mod vtable;
pub use vtable::*;

/// `Attr` definition
mod attr;
pub use attr::*;

/// Opaque pointers definition
mod ptr;
pub use ptr::*;

/// Marker traits definition
mod marker_traits;
pub use marker_traits::*;

/// `Opaque` definition (for `#[facet(opaque)]` types)
/// and other builtins.
mod builtins;
pub use builtins::*;

/// `Def` enums
mod def;
pub use def::*;

/// `Type` enums
mod ty;
pub use ty::*;

// Specialization utilities
pub mod specialization;

// Homemade bitflags
mod bitflags;

// Const type Id
mod const_typeid;
pub use const_typeid::*;

// Declaration Id
mod decl_id;
pub use decl_id::*;

// Scalar type identification
mod scalar;
pub use scalar::*;

// Error types
mod error;
pub use error::*;

// Characteristic enum
mod characteristic;
pub use characteristic::*;

// Variance types
mod variance;
pub use variance::*;
