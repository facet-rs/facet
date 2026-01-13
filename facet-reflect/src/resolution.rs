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

/// Category of a field in DOM formats (XML, HTML).
///
/// This categorizes how a field is represented in tree-based formats where
/// attributes, child elements, and text content are distinct concepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum FieldCategory {
    /// Field is an attribute (`#[facet(attribute)]`, `xml::attribute`, `html::attribute`)
    Attribute,
    /// Field is a child element (default for structs, or explicit `xml::element`)
    Element,
    /// Field captures text content (`xml::text`, `html::text`)
    Text,
    /// Field captures the tag name (`xml::tag`, `html::tag`)
    Tag,
    /// Field captures all unmatched children (`xml::elements`)
    Elements,
}

impl FieldCategory {
    /// Determine the category of a field based on its attributes.
    ///
    /// Returns `None` for flattened fields (they don't have a single category)
    /// or fields that capture unknown content (maps).
    pub fn from_field(field: &Field) -> Option<Self> {
        if field.is_flattened() {
            // Flattened fields don't have a category - their children do
            return None;
        }
        if field.is_attribute() {
            Some(FieldCategory::Attribute)
        } else if field.is_text() {
            Some(FieldCategory::Text)
        } else if field.is_tag() {
            Some(FieldCategory::Tag)
        } else if field.is_elements() {
            Some(FieldCategory::Elements)
        } else {
            // Default: child element
            Some(FieldCategory::Element)
        }
    }
}

/// A key for field lookup in schemas and solvers.
///
/// For flat formats (JSON, TOML), keys are just field names.
/// For DOM formats (XML, HTML), keys include a category (attribute, element, text).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FieldKey<'a> {
    /// Flat format key - just a name (for JSON, TOML, YAML, etc.)
    Flat(Cow<'a, str>),
    /// DOM format key - category + name (for XML, HTML)
    Dom(FieldCategory, Cow<'a, str>),
}

impl<'a> FieldKey<'a> {
    /// Create a flat key from a string.
    pub fn flat(name: impl Into<Cow<'a, str>>) -> Self {
        FieldKey::Flat(name.into())
    }

    /// Create a DOM attribute key.
    pub fn attribute(name: impl Into<Cow<'a, str>>) -> Self {
        FieldKey::Dom(FieldCategory::Attribute, name.into())
    }

    /// Create a DOM element key.
    pub fn element(name: impl Into<Cow<'a, str>>) -> Self {
        FieldKey::Dom(FieldCategory::Element, name.into())
    }

    /// Create a DOM text key.
    pub fn text() -> Self {
        FieldKey::Dom(FieldCategory::Text, Cow::Borrowed(""))
    }

    /// Create a DOM tag key.
    pub fn tag() -> Self {
        FieldKey::Dom(FieldCategory::Tag, Cow::Borrowed(""))
    }

    /// Create a DOM elements key (catch-all for unmatched children).
    pub fn elements() -> Self {
        FieldKey::Dom(FieldCategory::Elements, Cow::Borrowed(""))
    }

    /// Get the name portion of the key.
    pub fn name(&self) -> &str {
        match self {
            FieldKey::Flat(name) => name.as_ref(),
            FieldKey::Dom(_, name) => name.as_ref(),
        }
    }

    /// Get the category if this is a DOM key.
    pub fn category(&self) -> Option<FieldCategory> {
        match self {
            FieldKey::Flat(_) => None,
            FieldKey::Dom(cat, _) => Some(*cat),
        }
    }

    /// Convert to an owned version with 'static lifetime.
    pub fn into_owned(self) -> FieldKey<'static> {
        match self {
            FieldKey::Flat(name) => FieldKey::Flat(Cow::Owned(name.into_owned())),
            FieldKey::Dom(cat, name) => FieldKey::Dom(cat, Cow::Owned(name.into_owned())),
        }
    }
}

// Allow &str to convert to flat key
impl<'a> From<&'a str> for FieldKey<'a> {
    fn from(s: &'a str) -> Self {
        FieldKey::Flat(Cow::Borrowed(s))
    }
}

impl From<String> for FieldKey<'static> {
    fn from(s: String) -> Self {
        FieldKey::Flat(Cow::Owned(s))
    }
}

impl<'a> From<Cow<'a, str>> for FieldKey<'a> {
    fn from(s: Cow<'a, str>) -> Self {
        FieldKey::Flat(s)
    }
}

impl fmt::Display for FieldKey<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FieldKey::Flat(name) => write!(f, "{}", name),
            FieldKey::Dom(cat, name) => write!(f, "{:?}:{}", cat, name),
        }
    }
}

/// A path of serialized key names for probing (flat formats).
/// Unlike FieldPath which tracks the internal type structure (including variant selections),
/// KeyPath only tracks the keys as they appear in the serialized format.
pub type KeyPath = Vec<&'static str>;

/// A path of serialized keys for probing (DOM-aware).
/// Each key includes the category (attribute vs element) for DOM formats.
pub type DomKeyPath = Vec<FieldKey<'static>>;

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

    /// Category for DOM formats (attribute, element, text, etc.)
    /// This is `None` for flat formats or when the category cannot be determined.
    pub category: Option<FieldCategory>,
}

impl PartialEq for FieldInfo {
    fn eq(&self, other: &Self) -> bool {
        self.serialized_name == other.serialized_name
            && self.path == other.path
            && self.required == other.required
            && core::ptr::eq(self.value_shape, other.value_shape)
            && core::ptr::eq(self.field, other.field)
            && self.category == other.category
    }
}

impl FieldInfo {
    /// Get the key for this field, used for map lookups.
    /// If category is set, returns a DOM key; otherwise returns a flat key.
    pub fn key(&self) -> FieldKey<'static> {
        match self.category {
            Some(cat) => FieldKey::Dom(cat, Cow::Borrowed(self.serialized_name)),
            None => FieldKey::Flat(Cow::Borrowed(self.serialized_name)),
        }
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

    /// All fields in this configuration, keyed by (category, name).
    /// For flat formats, category is None. For DOM formats, category distinguishes
    /// attributes from elements with the same name.
    fields: BTreeMap<FieldKey<'static>, FieldInfo>,

    /// Set of required field names (for quick matching)
    required_field_names: BTreeSet<&'static str>,

    /// All known key paths at all depths (for depth-aware probing, flat format).
    /// Each path is a sequence of serialized key names from root.
    /// E.g., for `{payload: {content: "hi"}}`, contains `["payload"]` and `["payload", "content"]`.
    known_paths: BTreeSet<KeyPath>,

    /// All known key paths at all depths (for depth-aware probing, DOM format).
    /// Each path includes category information for each key.
    dom_known_paths: BTreeSet<DomKeyPath>,
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
            dom_known_paths: BTreeSet::new(),
        }
    }

    /// Add a key path (for depth-aware probing, flat format).
    pub fn add_key_path(&mut self, path: KeyPath) {
        self.known_paths.insert(path);
    }

    /// Add a DOM key path (for depth-aware probing, DOM format).
    pub fn add_dom_key_path(&mut self, path: DomKeyPath) {
        self.dom_known_paths.insert(path);
    }

    /// Add a field to this resolution.
    ///
    /// Returns an error if a field with the same key already exists
    /// but comes from a different source (different path). This catches duplicate
    /// field name conflicts between parent structs and flattened fields.
    pub fn add_field(&mut self, info: FieldInfo) -> Result<(), DuplicateFieldError> {
        let key = info.key();
        if let Some(existing) = self.fields.get(&key)
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
        self.fields.insert(key, info);
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
        for (key, info) in &other.fields {
            if let Some(existing) = self.fields.get(key)
                && existing.path != info.path
            {
                return Err(DuplicateFieldError {
                    field_name: info.serialized_name,
                    first_path: existing.path.clone(),
                    second_path: info.path.clone(),
                });
            }
            self.fields.insert(key.clone(), info.clone());
            if info.required {
                self.required_field_names.insert(info.serialized_name);
            }
        }
        for vs in &other.variant_selections {
            self.variant_selections.push(vs.clone());
        }
        for path in &other.known_paths {
            self.known_paths.insert(path.clone());
        }
        for path in &other.dom_known_paths {
            self.dom_known_paths.insert(path.clone());
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

        for (_key, info) in &self.fields {
            if !input_fields
                .iter()
                .any(|k| k.as_ref() == info.serialized_name)
            {
                if info.required {
                    missing_required.push(info.serialized_name);
                } else {
                    missing_optional.push(info.serialized_name);
                }
            }
        }

        // Check for unknown fields
        let unknown: Vec<String> = input_fields
            .iter()
            .filter(|f| {
                !self
                    .fields
                    .values()
                    .any(|info| info.serialized_name == f.as_ref())
            })
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

    /// Get a field by key.
    ///
    /// For runtime keys, use `field_by_key()` which accepts any lifetime.
    pub fn field(&self, key: &FieldKey<'static>) -> Option<&FieldInfo> {
        self.fields.get(key)
    }

    /// Get a field by key with any lifetime.
    ///
    /// This is less efficient than `field()` because it searches linearly,
    /// but works with runtime-constructed keys.
    pub fn field_by_key(&self, key: &FieldKey<'_>) -> Option<&FieldInfo> {
        self.fields.iter().find_map(|(k, v)| {
            // Compare structurally regardless of lifetime
            let matches = match (k, key) {
                (FieldKey::Flat(a), FieldKey::Flat(b)) => a.as_ref() == b.as_ref(),
                (FieldKey::Dom(cat_a, a), FieldKey::Dom(cat_b, b)) => {
                    cat_a == cat_b && a.as_ref() == b.as_ref()
                }
                _ => false,
            };
            if matches { Some(v) } else { None }
        })
    }

    /// Get a field by name (flat format lookup).
    /// For DOM format, use `field()` with a `FieldKey` instead.
    pub fn field_by_name(&self, name: &str) -> Option<&FieldInfo> {
        // Search by serialized name - works for both flat and DOM keys
        self.fields.values().find(|f| f.serialized_name == name)
    }

    /// Get all fields.
    pub const fn fields(&self) -> &BTreeMap<FieldKey<'static>, FieldInfo> {
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

    /// Check if this resolution has a specific key path (flat format).
    pub fn has_key_path(&self, path: &[&str]) -> bool {
        self.known_paths.iter().any(|known| {
            known.len() == path.len() && known.iter().zip(path.iter()).all(|(a, b)| *a == *b)
        })
    }

    /// Check if this resolution has a specific DOM key path.
    pub fn has_dom_key_path(&self, path: &[FieldKey<'_>]) -> bool {
        self.dom_known_paths.iter().any(|known| {
            known.len() == path.len()
                && known.iter().zip(path.iter()).all(|(a, b)| {
                    // Compare structurally regardless of lifetime
                    match (a, b) {
                        (FieldKey::Flat(sa), FieldKey::Flat(sb)) => sa.as_ref() == sb.as_ref(),
                        (FieldKey::Dom(ca, sa), FieldKey::Dom(cb, sb)) => {
                            ca == cb && sa.as_ref() == sb.as_ref()
                        }
                        _ => false,
                    }
                })
        })
    }

    /// Get all known DOM key paths (for depth-aware probing).
    pub const fn dom_known_paths(&self) -> &BTreeSet<DomKeyPath> {
        &self.dom_known_paths
    }
}

impl Default for Resolution {
    fn default() -> Self {
        Self::new()
    }
}
