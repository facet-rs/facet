//! JIT-compiled JSON deserialization for facet.
//!
//! This module uses Cranelift to generate native code deserializers at runtime,
//! specialized for each type's exact memory layout.
//!
//! # Usage
//!
//! ```ignore
//! use facet::Facet;
//! use facet_json_legacy::jit;
//!
//! #[derive(Facet)]
//! struct Point { x: f64, y: f64 }
//!
//! let point: Point = jit::from_str(r#"{"x": 1.0, "y": 2.0}"#).unwrap();
//! ```
//!
//! # How it works
//!
//! On first call for a type, compiles a specialized deserializer using Cranelift.
//! Subsequent calls use the cached native code directly.

mod cache;
mod compiler;
mod helpers;

pub use cache::JitCache;
pub use compiler::JitCompiler;

use crate::JsonError;
use facet_core::Facet;
use std::any::TypeId;

/// JIT-accelerated JSON deserialization.
///
/// On first call for a type, compiles a specialized deserializer.
/// Subsequent calls use the cached native code directly.
pub fn from_str<T: Facet<'static>>(input: &str) -> Result<T, JsonError> {
    from_str_inner::<T>(input, false)
}

/// Deserialize with explicit fallback to interpreter.
///
/// Useful for types that can't be JIT compiled (e.g., unsupported features).
pub fn from_str_with_fallback<T: Facet<'static>>(input: &str) -> Result<T, JsonError> {
    from_str_inner::<T>(input, true)
}

fn from_str_inner<T: Facet<'static>>(input: &str, allow_fallback: bool) -> Result<T, JsonError> {
    let type_id = TypeId::of::<T>();

    // Check cache first
    if let Some(func) = cache::get(type_id) {
        return unsafe { func.call(input) };
    }

    // Try to compile
    if let Some(func) = compiler::try_compile(T::SHAPE) {
        cache::insert(type_id, func);
        return unsafe { func.call(input) };
    }

    // Fallback to interpreter if allowed
    if allow_fallback {
        crate::from_str(input)
    } else {
        panic!("JIT compilation not supported for this type and fallback disabled")
    }
}

#[cfg(test)]
mod tests;
