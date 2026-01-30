//! # facet-reflect2
//!
//! Partial value construction for facet - v2 API with tree-based tracking.

// --- arena ---
pub(crate) mod arena;

// --- errors ---
mod errors;
pub use errors::{ErrorLocation, ReflectError, ReflectErrorKind};

// --- frame ---
pub(crate) mod frame;

// --- enum helpers ---
pub(crate) mod enum_helpers;

// --- ops ---
mod ops;
pub use ops::{Build, Imm, Op, Path, Source};

// --- partial ---
mod partial;
pub use partial::Partial;
