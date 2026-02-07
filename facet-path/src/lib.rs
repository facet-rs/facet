#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use facet_core::{Def, Field, Shape, Type, UserType};

pub mod access;
pub use access::PathAccessError;

pub mod walk;
pub use walk::{ShapeVisitor, VisitDecision, WalkStatus, walk_shape};

/// A single step in a path through a type structure.
///
/// Each step records an index that can be used to navigate
/// back through a [`Shape`] to reconstruct field names and types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PathStep {
    /// Navigate to a struct field by index
    Field(u32),
    /// Navigate to a list/array element by index
    Index(u32),
    /// Navigate to an enum variant by index
    Variant(u32),
    /// Navigate into a map key at a specific entry index.
    /// The entry index distinguishes paths for different map keys' inner frames.
    MapKey(u32),
    /// Navigate into a map value at a specific entry index.
    /// The entry index distinguishes paths for different map values' inner frames.
    MapValue(u32),
    /// Navigate into `Some` of an Option
    OptionSome,
    /// Navigate through a pointer/reference
    Deref,
    /// Navigate into a transparent inner type (e.g., `NonZero<T>` -> T)
    Inner,
    /// Navigate into a proxy type (e.g., `Inner` with `#[facet(proxy = InnerProxy)]`)
    ///
    /// This step distinguishes a proxy frame from its parent in the deferred
    /// processing path, so both can be stored without path collisions.
    Proxy,
}

/// A path through a type structure, recorded as a series of steps.
///
/// This is a lightweight representation that only stores indices.
/// The actual field names and type information can be reconstructed
/// by replaying these steps against the original [`Shape`].
#[derive(Debug, Clone)]
pub struct Path {
    /// The root [`Shape`] from which this path originates.
    pub shape: &'static Shape,

    /// The sequence of [`PathStep`]s representing navigation through the type structure.
    pub steps: Vec<PathStep>,
}

impl PartialEq for Path {
    fn eq(&self, other: &Self) -> bool {
        // Compare shapes by pointer address (they're static references)
        core::ptr::eq(self.shape, other.shape) && self.steps == other.steps
    }
}

impl Eq for Path {}

impl PartialOrd for Path {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Path {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // Compare shapes by pointer address first, then by steps
        let shape_cmp =
            (self.shape as *const Shape as usize).cmp(&(other.shape as *const Shape as usize));
        if shape_cmp != core::cmp::Ordering::Equal {
            return shape_cmp;
        }
        self.steps.cmp(&other.steps)
    }
}

impl core::hash::Hash for Path {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        // Hash the shape pointer address
        (self.shape as *const Shape as usize).hash(state);
        self.steps.hash(state);
    }
}

impl Path {
    /// Create a new empty path.
    pub const fn new(shape: &'static Shape) -> Self {
        Self {
            shape,
            steps: Vec::new(),
        }
    }

    /// Create a path with pre-allocated capacity.
    pub fn with_capacity(shape: &'static Shape, capacity: usize) -> Self {
        Self {
            shape,
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
    pub const fn len(&self) -> usize {
        self.steps.len()
    }

    /// Check if this path is empty.
    pub const fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Format this path as a human-readable string using the stored root shape.
    ///
    /// Returns a path like `outer.inner.items[3].name`.
    pub fn format(&self) -> String {
        self.format_with_shape(self.shape)
    }

    /// Format this path as a human-readable string by walking the given shape.
    ///
    /// Returns a path like `outer.inner.items[3].name`.
    pub fn format_with_shape(&self, shape: &'static Shape) -> String {
        let mut result = String::new();
        let mut current_shape = shape;
        let mut current_variant_idx: Option<usize> = None;

        for step in &self.steps {
            match step {
                PathStep::Field(idx) => {
                    let idx = *idx as usize;
                    if let Some(field_name) =
                        get_field_name_with_variant(current_shape, idx, current_variant_idx)
                    {
                        if !result.is_empty() {
                            result.push('.');
                        }
                        result.push_str(field_name);
                    }
                    if let Some(field_shape) =
                        get_field_shape_with_variant(current_shape, idx, current_variant_idx)
                    {
                        current_shape = field_shape;
                    }
                    current_variant_idx = None;
                }
                PathStep::Index(idx) => {
                    write!(result, "[{}]", idx).unwrap();
                    if let Some(elem_shape) = get_element_shape(current_shape) {
                        current_shape = elem_shape;
                    }
                    current_variant_idx = None;
                }
                PathStep::Variant(idx) => {
                    let idx = *idx as usize;
                    if let Some(variant_name) = get_variant_name(current_shape, idx) {
                        result.push_str("::");
                        result.push_str(variant_name);
                    }
                    // Don't advance current_shape — the next Field step will
                    // use the variant index to look up fields within the enum.
                    current_variant_idx = Some(idx);
                }
                PathStep::MapKey(idx) => {
                    write!(result, "[key#{}]", idx).unwrap();
                    if let Some(key_shape) = get_map_key_shape(current_shape) {
                        current_shape = key_shape;
                    }
                    current_variant_idx = None;
                }
                PathStep::MapValue(idx) => {
                    write!(result, "[value#{}]", idx).unwrap();
                    if let Some(value_shape) = get_map_value_shape(current_shape) {
                        current_shape = value_shape;
                    }
                    current_variant_idx = None;
                }
                PathStep::OptionSome => {
                    result.push_str("::Some");
                    if let Some(inner_shape) = get_option_inner_shape(current_shape) {
                        current_shape = inner_shape;
                    }
                    current_variant_idx = None;
                }
                PathStep::Deref => {
                    if let Some(inner_shape) = get_pointer_inner_shape(current_shape) {
                        current_shape = inner_shape;
                    }
                    current_variant_idx = None;
                }
                PathStep::Inner => {
                    if let Some(inner_shape) = get_inner_shape(current_shape) {
                        current_shape = inner_shape;
                    }
                    current_variant_idx = None;
                }
                PathStep::Proxy => {
                    if let Some(proxy_def) = current_shape.effective_proxy(None) {
                        current_shape = proxy_def.shape;
                    }
                    current_variant_idx = None;
                }
            }
        }

        if result.is_empty() {
            result.push_str("<root>");
        }

        result
    }

    /// Resolve the field at the end of this path, if the path ends at a struct field.
    ///
    /// This navigates through the given shape following each step in the path,
    /// and returns the [`Field`] if the final step is a `PathStep::Field`.
    ///
    /// This is useful for accessing field metadata like attributes when handling
    /// errors that occur at a specific field location.
    ///
    /// # Returns
    ///
    /// - `Some(&Field)` if the path ends at a struct field
    /// - `None` if the path is empty, doesn't end at a field, or navigation fails
    pub fn resolve_leaf_field(&self, shape: &'static Shape) -> Option<&'static Field> {
        if self.steps.is_empty() {
            return None;
        }

        let mut current_shape = shape;
        let mut current_variant_idx: Option<usize> = None;

        // Navigate through all steps except the last one
        for step in &self.steps[..self.steps.len() - 1] {
            match step {
                PathStep::Field(idx) => {
                    let idx = *idx as usize;
                    current_shape =
                        get_field_shape_with_variant(current_shape, idx, current_variant_idx)?;
                    current_variant_idx = None;
                }
                PathStep::Index(_) => {
                    current_shape = get_element_shape(current_shape)?;
                    current_variant_idx = None;
                }
                PathStep::Variant(idx) => {
                    // Remember the variant for the next field lookup
                    current_variant_idx = Some(*idx as usize);
                }
                PathStep::MapKey(_) => {
                    current_shape = get_map_key_shape(current_shape)?;
                    current_variant_idx = None;
                }
                PathStep::MapValue(_) => {
                    current_shape = get_map_value_shape(current_shape)?;
                    current_variant_idx = None;
                }
                PathStep::OptionSome => {
                    current_shape = get_option_inner_shape(current_shape)?;
                    current_variant_idx = None;
                }
                PathStep::Deref => {
                    current_shape = get_pointer_inner_shape(current_shape)?;
                    current_variant_idx = None;
                }
                PathStep::Inner => {
                    current_shape = get_inner_shape(current_shape)?;
                    current_variant_idx = None;
                }
                PathStep::Proxy => {
                    let proxy_def = current_shape.effective_proxy(None)?;
                    current_shape = proxy_def.shape;
                    current_variant_idx = None;
                }
            }
        }

        // Check if the last step is a field
        if let Some(PathStep::Field(idx)) = self.steps.last() {
            let idx = *idx as usize;
            return get_field_with_variant(current_shape, idx, current_variant_idx);
        }

        None
    }
}

impl core::fmt::Display for Path {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.format())
    }
}

/// Get the field at the given index, handling both structs and enum variants.
fn get_field_with_variant(
    shape: &Shape,
    idx: usize,
    variant_idx: Option<usize>,
) -> Option<&'static Field> {
    match shape.ty {
        Type::User(UserType::Struct(sd)) => sd.fields.get(idx),
        Type::User(UserType::Enum(ed)) => {
            let variant_idx = variant_idx?;
            let variant = ed.variants.get(variant_idx)?;
            variant.data.fields.get(idx)
        }
        _ => None,
    }
}

/// Get the shape of a field at the given index, handling both structs and enum variants.
fn get_field_shape_with_variant(
    shape: &Shape,
    idx: usize,
    variant_idx: Option<usize>,
) -> Option<&'static Shape> {
    get_field_with_variant(shape, idx, variant_idx).map(|f| f.shape())
}

/// Get the name of a field at the given index, handling both structs and enum variants.
fn get_field_name_with_variant(
    shape: &Shape,
    idx: usize,
    variant_idx: Option<usize>,
) -> Option<&'static str> {
    get_field_with_variant(shape, idx, variant_idx).map(|f| f.name)
}

/// Get the element shape for a list/array.
const fn get_element_shape(shape: &Shape) -> Option<&'static Shape> {
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

/// Get the key shape for a map.
const fn get_map_key_shape(shape: &Shape) -> Option<&'static Shape> {
    match shape.def {
        Def::Map(md) => Some(md.k()),
        _ => None,
    }
}

/// Get the value shape for a map.
const fn get_map_value_shape(shape: &Shape) -> Option<&'static Shape> {
    match shape.def {
        Def::Map(md) => Some(md.v()),
        _ => None,
    }
}

/// Get the inner shape for an Option.
const fn get_option_inner_shape(shape: &Shape) -> Option<&'static Shape> {
    match shape.def {
        Def::Option(od) => Some(od.t()),
        _ => None,
    }
}

/// Get the inner shape for a pointer.
const fn get_pointer_inner_shape(shape: &Shape) -> Option<&'static Shape> {
    match shape.def {
        Def::Pointer(pd) => pd.pointee(),
        _ => None,
    }
}

/// Get the inner shape for a transparent type (e.g., `NonZero<T>`).
const fn get_inner_shape(shape: &Shape) -> Option<&'static Shape> {
    shape.inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_step_size() {
        // PathStep should be 8 bytes (discriminant + u32, aligned)
        assert_eq!(core::mem::size_of::<PathStep>(), 8);
    }
}
