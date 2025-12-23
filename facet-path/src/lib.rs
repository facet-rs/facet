#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]
//! See README.md for documentation.

//! Path tracking for navigating Facet type structures.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use facet_core::{Def, Shape, StructKind, Type, UserType};

/// A single step in a path through a type structure.
///
/// Each step records an index that can be used to navigate
/// back through a [`Shape`] to reconstruct field names and types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathStep {
    /// Navigate to a struct field by index
    Field(u32),
    /// Navigate to a list/array element by index
    Index(u32),
    /// Navigate to an enum variant by index
    Variant(u32),
    /// Navigate into a map key
    MapKey,
    /// Navigate into a map value
    MapValue,
    /// Navigate into `Some` of an Option
    OptionSome,
    /// Navigate through a pointer/reference
    Deref,
}

/// A path through a type structure, recorded as a series of steps.
///
/// This is a lightweight representation that only stores indices.
/// The actual field names and type information can be reconstructed
/// by replaying these steps against the original [`Shape`].
#[derive(Debug, Clone, Default)]
pub struct Path {
    steps: Vec<PathStep>,
}

impl Path {
    /// Create a new empty path.
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    /// Create a path with pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            steps: Vec::with_capacity(capacity),
        }
    }

    /// Push a step onto the path.
    pub fn push(&mut self, step: PathStep) {
        self.steps.push(step);
    }

    /// Pop the last step from the path.
    pub fn pop(&mut self) -> Option<PathStep> {
        self.steps.pop()
    }

    /// Get the steps in this path.
    pub fn steps(&self) -> &[PathStep] {
        &self.steps
    }

    /// Get the length of this path.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Check if this path is empty.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Format this path as a human-readable string by walking the given shape.
    ///
    /// Returns a path like `outer.inner.items[3].name`.
    pub fn format_with_shape(&self, shape: &'static Shape) -> String {
        let mut result = String::new();
        let mut current_shape = shape;

        for step in &self.steps {
            match step {
                PathStep::Field(idx) => {
                    let idx = *idx as usize;
                    if let Some(field_name) = get_field_name(current_shape, idx) {
                        if !result.is_empty() {
                            result.push('.');
                        }
                        result.push_str(field_name);
                        if let Some(field_shape) = get_field_shape(current_shape, idx) {
                            current_shape = field_shape;
                        }
                    }
                }
                PathStep::Index(idx) => {
                    write!(result, "[{}]", idx).unwrap();
                    if let Some(elem_shape) = get_element_shape(current_shape) {
                        current_shape = elem_shape;
                    }
                }
                PathStep::Variant(idx) => {
                    let idx = *idx as usize;
                    if let Some(variant_name) = get_variant_name(current_shape, idx) {
                        result.push_str("::");
                        result.push_str(variant_name);
                        if let Some(variant_shape) = get_variant_shape(current_shape, idx) {
                            current_shape = variant_shape;
                        }
                    }
                }
                PathStep::MapKey => {
                    result.push_str("[key]");
                    if let Some(key_shape) = get_map_key_shape(current_shape) {
                        current_shape = key_shape;
                    }
                }
                PathStep::MapValue => {
                    result.push_str("[value]");
                    if let Some(value_shape) = get_map_value_shape(current_shape) {
                        current_shape = value_shape;
                    }
                }
                PathStep::OptionSome => {
                    if let Some(inner_shape) = get_option_inner_shape(current_shape) {
                        current_shape = inner_shape;
                    }
                }
                PathStep::Deref => {
                    if let Some(inner_shape) = get_pointer_inner_shape(current_shape) {
                        current_shape = inner_shape;
                    }
                }
            }
        }

        if result.is_empty() {
            result.push_str("<root>");
        }

        result
    }
}

/// Get the name of a field at the given index.
fn get_field_name(shape: &Shape, idx: usize) -> Option<&'static str> {
    match shape.ty {
        Type::User(UserType::Struct(sd)) => sd.fields.get(idx).map(|f| f.name),
        Type::User(UserType::Enum(_)) => {
            // For enums, we'd need the variant to get field names
            None
        }
        _ => None,
    }
}

/// Get the shape of a field at the given index.
fn get_field_shape(shape: &Shape, idx: usize) -> Option<&'static Shape> {
    match shape.ty {
        Type::User(UserType::Struct(sd)) => sd.fields.get(idx).map(|f| f.shape()),
        _ => None,
    }
}

/// Get the element shape for a list/array.
fn get_element_shape(shape: &Shape) -> Option<&'static Shape> {
    match shape.def {
        Def::List(ld) => Some(ld.t()),
        Def::Array(ad) => Some(ad.t()),
        Def::Slice(sd) => Some(sd.t()),
        _ => None,
    }
}

/// Get the name of a variant at the given index.
fn get_variant_name(shape: &Shape, idx: usize) -> Option<&'static str> {
    match shape.ty {
        Type::User(UserType::Enum(ed)) => ed.variants.get(idx).map(|v| v.name),
        _ => None,
    }
}

/// Get the "shape" for a variant - returns the first field's shape if present.
fn get_variant_shape(shape: &Shape, idx: usize) -> Option<&'static Shape> {
    match shape.ty {
        Type::User(UserType::Enum(ed)) => {
            let variant = ed.variants.get(idx)?;
            if variant.data.kind == StructKind::Unit {
                None
            } else {
                variant.data.fields.first().map(|f| f.shape())
            }
        }
        _ => None,
    }
}

/// Get the key shape for a map.
fn get_map_key_shape(shape: &Shape) -> Option<&'static Shape> {
    match shape.def {
        Def::Map(md) => Some(md.k()),
        _ => None,
    }
}

/// Get the value shape for a map.
fn get_map_value_shape(shape: &Shape) -> Option<&'static Shape> {
    match shape.def {
        Def::Map(md) => Some(md.v()),
        _ => None,
    }
}

/// Get the inner shape for an Option.
fn get_option_inner_shape(shape: &Shape) -> Option<&'static Shape> {
    match shape.def {
        Def::Option(od) => Some(od.t()),
        _ => None,
    }
}

/// Get the inner shape for a pointer.
fn get_pointer_inner_shape(shape: &Shape) -> Option<&'static Shape> {
    match shape.def {
        Def::Pointer(pd) => pd.pointee(),
        _ => None,
    }
}

#[cfg(feature = "pretty")]
mod pretty;

#[cfg(feature = "pretty")]
pub use pretty::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_step_size() {
        // PathStep should be 8 bytes (discriminant + u32, aligned)
        assert_eq!(core::mem::size_of::<PathStep>(), 8);
    }
}
