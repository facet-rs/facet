//! # trame
//!
//! Partial value construction for facet - v2 API with tree-based tracking.

// Enable experimental features for Kani loop contracts
#![cfg_attr(kani, feature(stmt_expr_attributes))]
#![cfg_attr(kani, feature(proc_macro_hygiene))]

// --- arena ---
pub(crate) mod arena;

// --- slab ---
pub(crate) mod slab;

// --- shape_desc ---
pub(crate) mod shape_desc;
pub use shape_desc::{ShapeDesc, Tuple2Shape, tuple2};

// --- errors ---
mod errors;
pub use errors::{ErrorLocation, ReflectError, ReflectErrorKind};

// --- frame ---
pub(crate) mod frame;

// --- enum helpers ---
pub(crate) mod enum_helpers;

// --- ops ---
mod ops;
pub use ops::{Imm, Op, OpBatch, Path, PathSegment, Source};

// --- partial ---
mod partial;
pub use partial::Partial;
