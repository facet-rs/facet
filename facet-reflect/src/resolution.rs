//! Resolution types for representing resolved type configurations.
//!
//! A [`Resolution`] represents one possible "shape" a type can take after
//! all enum variants in flatten paths have been selected. It contains all
//! the fields that exist in that configuration, along with their paths.

extern crate alloc;

use alloc::borrow::Cow;
use alloc::collections::BTreeMap;
use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::fmt;

use facet_core::{Field, Shape};

/// A path of serialized key names for probing.
/// Unlike FieldPath which tracks the internal type structure (including variant selections),
/// KeyPath only tracks the keys as they appear in the serialized format.
pub type KeyPath = Vec<&'static str>;

/// A segment in a field path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PathSegment {
    /// A regular struct field
    Field(&'static str),
    /// An enum variant selection (field_name, variant_name)
    Variant(&'static str, &'static str),
}

/// A path through the type tree to a field.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldPath {
    segments: Vec<PathSegment>,
}

impl fmt::Debug for FieldPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FieldPath(")?;
        for (i, seg) in self.segments.iter().enumerate() {
            if i > 0 {
                write!(f, ".")?;
            }
            match seg {
                PathSegment::Field(name) => write!(f, "{name}")?,
                PathSegment::Variant(field, variant) => write!(f, "{field}::{variant}")?,
            }
        }
        write!(f, ")")
    }
}

impl fmt::Display for FieldPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for seg in &self.segments {
            match seg {
                PathSegment::Field(name) => {
                    if !first {
                        write!(f, ".")?;
                    }
                    write!(f, "{name}")?;
                    first = false;
                }
                PathSegment::Variant(_, _) => {
                    // Skip variant segments in display path - they're internal
                }
            }
        }
        Ok(())
    }
}

impl FieldPath {
    /// Create an empty path (root level).
    pub const fn empty() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    /// Get the depth of this path.
    pub const fn depth(&self) -> usize {
        self.segments.len()
    }

    /// Push a field segment onto the path.
    pub fn push_field(&self, name: &'static str) -> Self {
        let mut new = self.clone();
        new.segments.push(PathSegment::Field(name));
        new
    }

    /// Push a variant segment onto the path.
    pub fn push_variant(&self, field_name: &'static str, variant_name: &'static str) -> Self {
        let mut new = self.clone();
        new.segments
            .push(PathSegment::Variant(field_name, variant_name));
        new
    }

    /// Get the parent path (all segments except the last).
    pub fn parent(&self) -> Self {
        let mut new = self.clone();
        new.segments.pop();
        new
    }

    /// Get the segments of this path.
    pub fn segments(&self) -> &[PathSegment] {
        &self.segments
    }

    /// Get the last segment, if any.
    pub fn last(&self) -> Option<&PathSegment> {
        self.segments.last()
    }
}

/// Records that a specific enum field has a specific variant selected.
#[derive(Debug, Clone)]
pub struct VariantSelection {
    /// Path to the enum field from root
    pub path: FieldPath,
    /// Name of the enum type (e.g., "MessagePayload")
    pub enum_name: &'static str,
    /// Name of the selected variant (e.g., "Text")
    pub variant_name: &'static str,
}

/// Information about a single field in a resolution.
#[derive(Debug, Clone)]
pub struct FieldInfo {
    /// The name as it appears in the serialized format
    pub serialized_name: &'static str,

    /// Full path from root to this field
    pub path: FieldPath,

    /// Whether this field is required (not Option, no default)
    pub required: bool,

    /// The shape of this field's value
    pub value_shape: &'static Shape,

    /// The original field definition (for accessing flags, attributes, etc.)
    pub field: &'static Field,
}

impl PartialEq for FieldInfo {
    fn eq(&self, other: &Self) -> bool {
        self.serialized_name == other.serialized_name
            && self.path == other.path
            && self.required == other.required
            && core::ptr::eq(self.value_shape, other.value_shape)
            && core::ptr::eq(self.field, other.field)
    }
}

impl Eq for FieldInfo {}

/// Result of matching input fields against a resolution.
#[derive(Debug)]
pub enum MatchResult {
    /// All required fields present, all fields known
    Exact,
    /// All required fields present, some optional fields missing
    WithOptionalMissing(Vec<&'static str>),
    /// Does not match
    NoMatch {
        /// Required fields that are missing
        missing_required: Vec<&'static str>,
        /// Fields that are not known in this resolution
        unknown: Vec<String>,
    },
}

/// One possible "shape" the flattened type could take.
///
/// Represents a specific choice of variants for all enums in the flatten tree.
/// This is the "resolution" of all ambiguity in the type — all enum variants
/// have been selected, all fields are known.
#[derive(Debug, Clone)]
pub struct Resolution {
    /// For each enum in the flatten path, which variant is selected.
    /// The key is the path to the enum field, value is the variant.
    variant_selections: Vec<VariantSelection>,

    /// All fields in this configuration, keyed by serialized name.
    fields: BTreeMap<&'static str, FieldInfo>,

    /// Set of required field names (for quick matching)
    required_field_names: BTreeSet<&'static str>,

    /// All known key paths at all depths (for depth-aware probing).
    /// Each path is a sequence of serialized key names from root.
    /// E.g., for `{payload: {content: "hi"}}`, contains `["payload"]` and `["payload", "content"]`.
    known_paths: BTreeSet<KeyPath>,
}

/// Error when building a resolution.
#[derive(Debug, Clone)]
pub struct DuplicateFieldError {
    /// The duplicate field name
    pub field_name: &'static str,
    /// The first path where this field was found
    pub first_path: FieldPath,
    /// The second path where this field was found
    pub second_path: FieldPath,
}

impl fmt::Display for DuplicateFieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "duplicate field '{}': found at {} and {}",
            self.field_name, self.first_path, self.second_path
        )
    }
}

impl Resolution {
    /// Create a new empty resolution.
    pub const fn new() -> Self {
        Self {
            variant_selections: Vec::new(),
            fields: BTreeMap::new(),
            required_field_names: BTreeSet::new(),
            known_paths: BTreeSet::new(),
        }
    }

    /// Add a key path (for depth-aware probing).
    pub fn add_key_path(&mut self, path: KeyPath) {
        self.known_paths.insert(path);
    }

    /// Add a field to this resolution.
    ///
    /// Returns an error if a field with the same serialized name already exists
    /// but comes from a different source (different path). This catches duplicate
    /// field name conflicts between parent structs and flattened fields.
    pub fn add_field(&mut self, info: FieldInfo) -> Result<(), DuplicateFieldError> {
        if let Some(existing) = self.fields.get(info.serialized_name)
            && existing.path != info.path
        {
            return Err(DuplicateFieldError {
                field_name: info.serialized_name,
                first_path: existing.path.clone(),
                second_path: info.path,
            });
        }
        if info.required {
            self.required_field_names.insert(info.serialized_name);
        }
        self.fields.insert(info.serialized_name, info);
        Ok(())
    }

    /// Add a variant selection to this resolution.
    pub fn add_variant_selection(
        &mut self,
        path: FieldPath,
        enum_name: &'static str,
        variant_name: &'static str,
    ) {
        self.variant_selections.push(VariantSelection {
            path,
            enum_name,
            variant_name,
        });
    }

    /// Merge another resolution into this one.
    ///
    /// Returns an error if a field with the same serialized name already exists
    /// but comes from a different source (different path). This catches duplicate
    /// field name conflicts between parent structs and flattened fields.
    pub fn merge(&mut self, other: &Resolution) -> Result<(), DuplicateFieldError> {
        for (name, info) in &other.fields {
            if let Some(existing) = self.fields.get(*name)
                && existing.path != info.path
            {
                return Err(DuplicateFieldError {
                    field_name: name,
                    first_path: existing.path.clone(),
                    second_path: info.path.clone(),
                });
            }
            self.fields.insert(*name, info.clone());
            if info.required {
                self.required_field_names.insert(*name);
            }
        }
        for vs in &other.variant_selections {
            self.variant_selections.push(vs.clone());
        }
        for path in &other.known_paths {
            self.known_paths.insert(path.clone());
        }
        Ok(())
    }

    /// Mark all fields as optional (required = false).
    /// Used when a flattened field is wrapped in `Option<T>`.
    pub fn mark_all_optional(&mut self) {
        self.required_field_names.clear();
        for info in self.fields.values_mut() {
            info.required = false;
        }
    }

    /// Check if this resolution matches the input fields.
    pub fn matches(&self, input_fields: &BTreeSet<Cow<'_, str>>) -> MatchResult {
        let mut missing_required = Vec::new();
        let mut missing_optional = Vec::new();

        for (name, info) in &self.fields {
            if !input_fields.iter().any(|k| k.as_ref() == *name) {
                if info.required {
                    missing_required.push(*name);
                } else {
                    missing_optional.push(*name);
                }
            }
        }

        // Check for unknown fields
        let unknown: Vec<String> = input_fields
            .iter()
            .filter(|f| !self.fields.contains_key(f.as_ref()))
            .map(|s| s.to_string())
            .collect();

        if !missing_required.is_empty() || !unknown.is_empty() {
            MatchResult::NoMatch {
                missing_required,
                unknown,
            }
        } else if missing_optional.is_empty() {
            MatchResult::Exact
        } else {
            MatchResult::WithOptionalMissing(missing_optional)
        }
    }

    /// Get a human-readable description of this resolution.
    ///
    /// Returns something like `MessagePayload::Text` or `Auth::Token + Transport::Tcp`
    /// for resolutions with multiple variant selections.
    pub fn describe(&self) -> String {
        if self.variant_selections.is_empty() {
            String::from("(no variants)")
        } else {
            let parts: Vec<_> = self
                .variant_selections
                .iter()
                .map(|vs| format!("{}::{}", vs.enum_name, vs.variant_name))
                .collect();
            parts.join(" + ")
        }
    }

    /// Get the fields in deserialization order (deepest first).
    pub fn deserialization_order(&self) -> Vec<&FieldInfo> {
        let mut fields: Vec<_> = self.fields.values().collect();
        fields.sort_by(|a, b| {
            // Deeper paths first
            b.path
                .depth()
                .cmp(&a.path.depth())
                // Then lexicographic for determinism
                .then_with(|| a.path.cmp(&b.path))
        });
        fields
    }

    /// Get a field by name.
    pub fn field(&self, name: &str) -> Option<&FieldInfo> {
        self.fields.get(name)
    }

    /// Get all fields.
    pub const fn fields(&self) -> &BTreeMap<&'static str, FieldInfo> {
        &self.fields
    }

    /// Get the set of required field names.
    pub const fn required_field_names(&self) -> &BTreeSet<&'static str> {
        &self.required_field_names
    }

    /// Get optional fields that were NOT provided in the input.
    ///
    /// This is useful for deserializers that need to initialize missing
    /// optional fields to `None` or their default value.
    pub fn missing_optional_fields<'a>(
        &'a self,
        seen_keys: &'a BTreeSet<Cow<'_, str>>,
    ) -> impl Iterator<Item = &'a FieldInfo> {
        self.fields.values().filter(move |info| {
            !info.required && !seen_keys.iter().any(|k| k.as_ref() == info.serialized_name)
        })
    }

    /// Get variant selections.
    pub fn variant_selections(&self) -> &[VariantSelection] {
        &self.variant_selections
    }

    /// Get all child fields (fields with the CHILD flag).
    ///
    /// This is useful for formats like XML where child elements need to be
    /// processed separately from attributes.
    pub fn child_fields(&self) -> impl Iterator<Item = &FieldInfo> {
        self.fields.values().filter(|f| f.field.is_child())
    }

    /// Get all property fields (fields without the child attribute).
    ///
    /// This is useful for formats like XML where attributes are processed
    /// separately from child elements.
    pub fn property_fields(&self) -> impl Iterator<Item = &FieldInfo> {
        self.fields.values().filter(|f| !f.field.is_child())
    }

    /// Get all known key paths (for depth-aware probing).
    pub const fn known_paths(&self) -> &BTreeSet<KeyPath> {
        &self.known_paths
    }

    /// Check if this resolution has a specific key path.
    /// Compares runtime strings against static schema paths.
    pub fn has_key_path(&self, path: &[&str]) -> bool {
        self.known_paths.iter().any(|known| {
            known.len() == path.len() && known.iter().zip(path.iter()).all(|(a, b)| *a == *b)
        })
    }
}

impl Default for Resolution {
    fn default() -> Self {
        Self::new()
    }
}
