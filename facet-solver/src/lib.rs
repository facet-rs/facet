#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::collections::BTreeSet;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use facet_core::{Def, Field, FieldFlags, Shape, StructType, Type, UserType, Variant};

/// Cached schema for a type that may contain flattened fields.
///
/// This is computed once per Shape and can be cached forever since
/// type information is static.
#[derive(Debug)]
pub struct Schema {
    /// The shape this schema is for (kept for future caching key)
    #[allow(dead_code)]
    shape: &'static Shape,

    /// All possible configurations of this type.
    /// For types with no enums in flatten paths, this has exactly 1 entry.
    /// For types with enums, this has one entry per valid combination of variants.
    configurations: Vec<Configuration>,

    /// Inverted index: field_name → bitmask of configuration indices.
    /// Bit i is set if `configurations[i]` contains this field.
    /// Uses a `Vec<u64>` to support arbitrary numbers of configurations.
    field_to_configs: BTreeMap<&'static str, ConfigSet>,
}

/// A set of configuration indices, stored as a bitmask for O(1) intersection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigSet {
    /// Bitmask where bit i indicates `configurations[i]` is in the set.
    /// For most types, a single u64 suffices (up to 64 configs).
    bits: Vec<u64>,
    /// Number of configurations in the set.
    count: usize,
}

impl ConfigSet {
    /// Create an empty config set.
    fn empty(num_configs: usize) -> Self {
        let num_words = num_configs.div_ceil(64);
        Self {
            bits: vec![0; num_words],
            count: 0,
        }
    }

    /// Create a full config set (all configs present).
    fn full(num_configs: usize) -> Self {
        let num_words = num_configs.div_ceil(64);
        let mut bits = vec![!0u64; num_words];
        // Clear bits beyond num_configs
        if num_configs % 64 != 0 {
            let last_word_bits = num_configs % 64;
            bits[num_words - 1] = (1u64 << last_word_bits) - 1;
        }
        Self {
            bits,
            count: num_configs,
        }
    }

    /// Insert a configuration index.
    fn insert(&mut self, idx: usize) {
        let word = idx / 64;
        let bit = idx % 64;
        if self.bits[word] & (1u64 << bit) == 0 {
            self.bits[word] |= 1u64 << bit;
            self.count += 1;
        }
    }

    /// Intersect with another config set in place.
    fn intersect_with(&mut self, other: &ConfigSet) {
        self.count = 0;
        for (a, b) in self.bits.iter_mut().zip(other.bits.iter()) {
            *a &= *b;
            self.count += a.count_ones() as usize;
        }
    }

    /// Get the number of configurations in the set.
    fn len(&self) -> usize {
        self.count
    }

    /// Check if empty.
    fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Get the first (lowest) configuration index in the set.
    fn first(&self) -> Option<usize> {
        for (word_idx, &word) in self.bits.iter().enumerate() {
            if word != 0 {
                return Some(word_idx * 64 + word.trailing_zeros() as usize);
            }
        }
        None
    }

    /// Iterate over configuration indices in the set.
    fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.bits.iter().enumerate().flat_map(|(word_idx, &word)| {
            (0..64).filter_map(move |bit| {
                if word & (1u64 << bit) != 0 {
                    Some(word_idx * 64 + bit)
                } else {
                    None
                }
            })
        })
    }
}

/// One possible "shape" the flattened type could take.
///
/// Represents a specific choice of variants for all enums in the flatten tree.
#[derive(Debug, Clone)]
pub struct Configuration {
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

/// A path of serialized key names for probing.
/// Unlike FieldPath which tracks the internal type structure (including variant selections),
/// KeyPath only tracks the keys as they appear in the serialized format.
pub type KeyPath = Vec<&'static str>;

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

/// Information about a single field in a configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldInfo {
    /// The name as it appears in the serialized format
    pub serialized_name: &'static str,

    /// Full path from root to this field
    pub path: FieldPath,

    /// Whether this field is required (not Option, no default)
    pub required: bool,

    /// The shape of this field's value
    pub value_shape: &'static Shape,
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

/// A segment in a field path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PathSegment {
    /// A regular struct field
    Field(&'static str),
    /// An enum variant selection (field_name, variant_name)
    Variant(&'static str, &'static str),
}

impl FieldPath {
    /// Create an empty path (root level).
    pub fn empty() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    /// Get the depth of this path.
    pub fn depth(&self) -> usize {
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

/// Result of matching input fields against a configuration.
#[derive(Debug)]
pub enum MatchResult {
    /// All required fields present, all fields known
    Exact,
    /// All required fields present, some optional fields missing
    WithOptionalMissing(Vec<&'static str>),
    /// Does not match
    NoMatch {
        missing_required: Vec<&'static str>,
        unknown: Vec<String>,
    },
}

// ============================================================================
// Incremental Solver
// ============================================================================

/// Incremental constraint solver for streaming deserialization.
///
/// Unlike batch solving where all field names are known upfront, this solver
/// processes fields one at a time as they arrive. It minimizes deferred fields by
/// immediately setting fields that have the same path in all remaining
/// candidate configurations.
///
/// # Example
///
/// ```rust
/// use facet::Facet;
/// use facet_solver::{Schema, IncrementalSolver, FieldDecision};
///
/// #[derive(Facet)]
/// struct SimpleConfig { port: u16 }
///
/// #[derive(Facet)]
/// struct AdvancedConfig { host: String, port: u16 }
///
/// #[derive(Facet)]
/// #[repr(u8)]
/// enum Config {
///     Simple(SimpleConfig),
///     Advanced(AdvancedConfig),
/// }
///
/// #[derive(Facet)]
/// struct Server {
///     name: String,
///     #[facet(flatten)]
///     config: Config,
/// }
///
/// let schema = Schema::build(Server::SHAPE).unwrap();
/// let mut solver = IncrementalSolver::new(&schema);
///
/// // "name" exists in all configs at the same path - set directly
/// assert!(matches!(solver.see_key("name"), FieldDecision::SetDirectly(_)));
///
/// // "host" only exists in Advanced - disambiguates!
/// match solver.see_key("host") {
///     FieldDecision::Disambiguated { config, current_field } => {
///         assert_eq!(current_field.serialized_name, "host");
///         assert!(config.field("port").is_some()); // Advanced has port too
///     }
///     _ => panic!("expected disambiguation"),
/// }
/// ```
#[derive(Debug)]
pub struct IncrementalSolver<'a> {
    /// Reference to the schema for configuration lookup
    schema: &'a Schema,
    /// Bitmask of remaining candidate configuration indices
    candidates: ConfigSet,
    /// Keys that have been deferred (for tracking purposes)
    deferred_keys: Vec<&'a str>,
}

/// Result of presenting a key to the incremental solver.
#[derive(Debug)]
pub enum FieldDecision<'a> {
    /// Field has the same path in all remaining candidates.
    /// Set it directly - path is unambiguous.
    SetDirectly(&'a FieldInfo),

    /// Field exists in candidates but at different paths.
    /// Defer (mark position for replay after disambiguation).
    Defer,

    /// This field narrowed candidates to exactly one.
    /// Replay any deferred fields, then set this field.
    Disambiguated {
        /// The winning configuration
        config: &'a Configuration,
        /// Info for the field that caused disambiguation
        current_field: &'a FieldInfo,
    },

    /// Field not found in any remaining candidate.
    Unknown,
}

impl<'a> IncrementalSolver<'a> {
    /// Create a new incremental solver from a schema.
    pub fn new(schema: &'a Schema) -> Self {
        Self {
            schema,
            candidates: ConfigSet::full(schema.configurations.len()),
            deferred_keys: Vec::new(),
        }
    }

    /// Get the current candidate configurations.
    pub fn candidates(&self) -> Vec<&'a Configuration> {
        self.candidates
            .iter()
            .map(|idx| &self.schema.configurations[idx])
            .collect()
    }

    /// Process a field key. Returns what to do with the value.
    pub fn see_key(&mut self, key: &'a str) -> FieldDecision<'a> {
        // Use inverted index to find configs with this key - O(1) lookup!
        let configs_with_key = match self.schema.field_to_configs.get(key) {
            Some(set) => set,
            None => return FieldDecision::Unknown,
        };

        // Intersect with current candidates - O(num_words) = O(configs/64)
        let prev_count = self.candidates.len();
        self.candidates.intersect_with(configs_with_key);

        if self.candidates.is_empty() {
            return FieldDecision::Unknown;
        }

        // Did this narrow our candidates?
        let narrowed = self.candidates.len() < prev_count;

        // Check if we've disambiguated to exactly one (but only if we actually narrowed)
        if narrowed && self.candidates.len() == 1 {
            let idx = self.candidates.first().unwrap();
            let config = &self.schema.configurations[idx];
            let current_field = config.field(key).unwrap();
            return FieldDecision::Disambiguated {
                config,
                current_field,
            };
        }

        // Check if path is the same in all remaining candidates
        let mut paths = BTreeSet::new();
        for idx in self.candidates.iter() {
            let config = &self.schema.configurations[idx];
            if let Some(info) = config.field(key) {
                paths.insert(&info.path);
            }
        }

        if paths.len() == 1 {
            // Same path in all candidates - can set directly
            let idx = self.candidates.first().unwrap();
            FieldDecision::SetDirectly(self.schema.configurations[idx].field(key).unwrap())
        } else {
            // Different paths - defer until disambiguation
            self.deferred_keys.push(key);
            FieldDecision::Defer
        }
    }

    /// Get the keys that have been deferred.
    pub fn deferred_keys(&self) -> &[&'a str] {
        &self.deferred_keys
    }

    /// Clear the deferred keys (call after replaying).
    pub fn clear_deferred(&mut self) {
        self.deferred_keys.clear();
    }

    /// Finish solving. Returns error if still ambiguous or missing required fields.
    ///
    /// When multiple candidates remain, this filters out any that are missing
    /// required fields. This handles the case where one variant's fields are
    /// a subset of another's (e.g., `Http { url }` vs `Git { url, branch }`).
    /// If input only contains `url`, both match the "has this field" check,
    /// but `Git` is filtered out because it's missing required `branch`.
    pub fn finish(self, seen_keys: &BTreeSet<&str>) -> Result<&'a Configuration, SolverError> {
        if self.candidates.is_empty() {
            return Err(SolverError::NoMatch {
                input_fields: seen_keys.iter().map(|s| (*s).into()).collect(),
                missing_required: Vec::new(),
                missing_required_detailed: Vec::new(),
                unknown_fields: Vec::new(),
                closest_config: None,
            });
        }

        // Filter candidates to only those that have all required fields satisfied
        let satisfied: Vec<usize> = self
            .candidates
            .iter()
            .filter(|idx| {
                let config = &self.schema.configurations[*idx];
                config
                    .required_field_names
                    .iter()
                    .all(|f| seen_keys.contains(f))
            })
            .collect();

        match satisfied.len() {
            0 => {
                // No config has all required fields satisfied.
                // Find the "closest" match for a helpful error message.
                let first_idx = self.candidates.first().unwrap();
                let first_config = &self.schema.configurations[first_idx];
                let missing: Vec<_> = first_config
                    .required_field_names
                    .iter()
                    .filter(|f| !seen_keys.contains(*f))
                    .copied()
                    .collect();
                let missing_detailed: Vec<_> = missing
                    .iter()
                    .filter_map(|name| first_config.field(name))
                    .map(MissingFieldInfo::from_field_info)
                    .collect();
                Err(SolverError::NoMatch {
                    input_fields: seen_keys.iter().map(|s| (*s).into()).collect(),
                    missing_required: missing,
                    missing_required_detailed: missing_detailed,
                    unknown_fields: Vec::new(),
                    closest_config: Some(first_config.describe()),
                })
            }
            1 => {
                let idx = satisfied[0];
                Ok(&self.schema.configurations[idx])
            }
            _ => {
                // Multiple configs still match after filtering - true ambiguity
                // Find fields that could disambiguate (unique to only some configs)
                let disambiguating = find_disambiguating_fields(
                    &satisfied
                        .iter()
                        .map(|i| &self.schema.configurations[*i])
                        .collect::<Vec<_>>(),
                );
                Err(SolverError::Ambiguous {
                    candidates: satisfied
                        .iter()
                        .map(|idx| self.schema.configurations[*idx].describe())
                        .collect(),
                    disambiguating_fields: disambiguating,
                })
            }
        }
    }
}

/// Find fields that could disambiguate between configurations.
/// Returns fields that exist in some but not all configurations.
fn find_disambiguating_fields(configs: &[&Configuration]) -> Vec<String> {
    if configs.len() < 2 {
        return Vec::new();
    }

    // Collect all field names across all configs
    let mut all_fields: BTreeSet<&str> = BTreeSet::new();
    for config in configs {
        for name in config.fields.keys() {
            all_fields.insert(name);
        }
    }

    // Find fields that are in some but not all configs
    let mut disambiguating = Vec::new();
    for field in all_fields {
        let count = configs.iter().filter(|c| c.field(field).is_some()).count();
        if count > 0 && count < configs.len() {
            disambiguating.push(field.to_string());
        }
    }

    disambiguating
}

/// Information about a missing required field for error reporting.
#[derive(Debug, Clone)]
pub struct MissingFieldInfo {
    /// The serialized field name (as it appears in input)
    pub name: &'static str,
    /// Full path to the field (e.g., "backend.connection.port")
    pub path: String,
    /// The Rust type that defines this field
    pub defined_in: String,
}

impl MissingFieldInfo {
    /// Create from a FieldInfo
    fn from_field_info(info: &FieldInfo) -> Self {
        Self {
            name: info.serialized_name,
            path: info.path.to_string(),
            defined_in: info.value_shape.type_identifier.to_string(),
        }
    }
}

/// Errors that can occur when building a schema.
#[derive(Debug, Clone)]
pub enum SchemaError {
    /// A field name appears from multiple sources (parent struct and flattened struct)
    DuplicateField {
        /// The conflicting field name
        field_name: &'static str,
        /// Path to the first occurrence
        first_path: FieldPath,
        /// Path to the second occurrence
        second_path: FieldPath,
    },
}

impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SchemaError::DuplicateField {
                field_name,
                first_path,
                second_path,
            } => {
                write!(
                    f,
                    "Duplicate field name '{field_name}' from different sources: {first_path} vs {second_path}. \
                     This usually means a parent struct and a flattened struct both \
                     define a field with the same name."
                )
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SchemaError {}

/// Errors that can occur during flatten resolution.
#[derive(Debug)]
pub enum SolverError {
    /// No configuration matches the input fields
    NoMatch {
        /// The input fields that were provided
        input_fields: Vec<String>,
        /// Missing required fields (from the closest matching config) - simple names for backwards compat
        missing_required: Vec<&'static str>,
        /// Missing required fields with full path information
        missing_required_detailed: Vec<MissingFieldInfo>,
        /// Unknown fields that don't belong to any config
        unknown_fields: Vec<String>,
        /// Description of the closest matching configuration
        closest_config: Option<String>,
    },
    /// Multiple configurations match the input fields
    Ambiguous {
        /// Descriptions of the matching configurations
        candidates: Vec<String>,
        /// Fields that could disambiguate (unique to specific configs)
        disambiguating_fields: Vec<String>,
    },
}

impl fmt::Display for SolverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SolverError::NoMatch {
                input_fields,
                missing_required: _,
                missing_required_detailed,
                unknown_fields,
                closest_config,
            } => {
                write!(f, "No matching configuration for fields {input_fields:?}")?;
                if let Some(config) = closest_config {
                    write!(f, " (closest match: {config})")?;
                }
                if !missing_required_detailed.is_empty() {
                    write!(f, "; missing required fields:")?;
                    for info in missing_required_detailed {
                        write!(f, " {} (at path: {})", info.name, info.path)?;
                    }
                }
                if !unknown_fields.is_empty() {
                    write!(f, "; unknown: {unknown_fields:?}")?;
                }
                Ok(())
            }
            SolverError::Ambiguous {
                candidates,
                disambiguating_fields,
            } => {
                write!(
                    f,
                    "Ambiguous: multiple configurations match: {candidates:?}"
                )?;
                if !disambiguating_fields.is_empty() {
                    write!(
                        f,
                        "; try adding one of these fields to disambiguate: {disambiguating_fields:?}"
                    )?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SolverError {}

/// Compute a specificity score for a shape. Lower score = more specific.
///
/// This is used to disambiguate when a value could satisfy multiple types.
/// For example, the value `42` fits both `u8` and `u16`, but `u8` is more
/// specific (lower score), so it should be preferred.
fn specificity_score(shape: &'static Shape) -> u64 {
    // Use type_identifier to determine specificity
    // Smaller integer types are more specific
    match shape.type_identifier {
        "u8" | "i8" => 8,
        "u16" | "i16" => 16,
        "u32" | "i32" | "f32" => 32,
        "u64" | "i64" | "f64" => 64,
        "u128" | "i128" => 128,
        "usize" | "isize" => 64, // Treat as 64-bit
        // Other types get a high score (less specific)
        _ => 1000,
    }
}

// ============================================================================
// Solver (State Machine)
// ============================================================================

/// Result of reporting a key to the solver.
#[derive(Debug)]
pub enum KeyResult<'a> {
    /// All candidates have the same type for this key.
    /// The deserializer can parse the value directly.
    Unambiguous {
        /// The shape all candidates expect for this field
        shape: &'static Shape,
    },

    /// Candidates have different types for this key - need disambiguation.
    /// Deserializer should parse the value, determine which fields it can
    /// satisfy, and call `satisfy()` with the viable fields.
    ///
    /// **Important**: When multiple fields can be satisfied by the value,
    /// pick the one with the lowest score (most specific). Scores are assigned
    /// by specificity, e.g., `u8` has a lower score than `u16`.
    Ambiguous {
        /// The unique fields across remaining candidates (deduplicated by shape),
        /// paired with a specificity score. Lower score = more specific type.
        /// Deserializer should check which of these the value can satisfy,
        /// then pick the one with the lowest score.
        fields: Vec<(&'a FieldInfo, u64)>,
    },

    /// This key disambiguated to exactly one configuration.
    Solved(&'a Configuration),

    /// This key doesn't exist in any remaining candidate.
    Unknown,
}

/// Result of reporting which fields the value can satisfy.
#[derive(Debug)]
pub enum SatisfyResult<'a> {
    /// Continue - still multiple candidates, keep feeding keys.
    Continue,

    /// Solved to exactly one configuration.
    Solved(&'a Configuration),

    /// No configuration can accept the value (no fields were satisfied).
    NoMatch,
}

/// State machine solver for lazy value-based disambiguation.
///
/// This solver only requests value inspection when candidates disagree on type.
/// For keys where all candidates expect the same type, the deserializer can
/// skip detailed value analysis.
///
/// # Example
///
/// ```rust
/// use facet::Facet;
/// use facet_solver::{Schema, Solver, KeyResult, SatisfyResult};
///
/// #[derive(Facet)]
/// #[repr(u8)]
/// enum NumericValue {
///     Small(u8),
///     Large(u16),
/// }
///
/// #[derive(Facet)]
/// struct Container {
///     #[facet(flatten)]
///     value: NumericValue,
/// }
///
/// let schema = Schema::build(Container::SHAPE).unwrap();
/// let mut solver = Solver::new(&schema);
///
/// // The field "0" has different types (u8 vs u16) - solver needs disambiguation
/// match solver.see_key("0") {
///     KeyResult::Ambiguous { fields } => {
///         // Deserializer sees value "1000", checks which fields can accept it
///         // u8 can't hold 1000, u16 can - so only report the u16 field
///         // Fields come with specificity scores - lower = more specific
///         let satisfied: Vec<_> = fields.iter()
///             .filter(|(f, _score)| {
///                 // deserializer's logic: can this value parse as this field's type?
///                 f.value_shape.type_identifier == "u16"
///             })
///             .map(|(f, _)| *f)
///             .collect();
///
///         match solver.satisfy(&satisfied) {
///             SatisfyResult::Solved(config) => {
///                 assert!(config.describe().contains("Large"));
///             }
///             _ => panic!("expected solved"),
///         }
///     }
///     _ => panic!("expected Ambiguous"),
/// }
/// ```
#[derive(Debug)]
pub struct Solver<'a> {
    /// Reference to the schema for configuration lookup
    schema: &'a Schema,
    /// Bitmask of remaining candidate configuration indices
    candidates: ConfigSet,
    /// Set of seen keys for required field checking
    seen_keys: BTreeSet<&'a str>,
}

impl<'a> Solver<'a> {
    /// Create a new solver from a schema.
    pub fn new(schema: &'a Schema) -> Self {
        Self {
            schema,
            candidates: ConfigSet::full(schema.configurations.len()),
            seen_keys: BTreeSet::new(),
        }
    }

    /// Report a key. Returns what to do next.
    ///
    /// - `Unambiguous`: All candidates agree on the type - parse directly
    /// - `Ambiguous`: Types differ - check which fields the value can satisfy
    /// - `Solved`: Disambiguated to one config
    /// - `Unknown`: Key not found in any candidate
    pub fn see_key(&mut self, key: &'a str) -> KeyResult<'a> {
        self.seen_keys.insert(key);

        // Key-based filtering
        let configs_with_key = match self.schema.field_to_configs.get(key) {
            Some(set) => set,
            None => return KeyResult::Unknown,
        };

        self.candidates.intersect_with(configs_with_key);

        if self.candidates.is_empty() {
            return KeyResult::Unknown;
        }

        // Check if we've disambiguated to exactly one
        if self.candidates.len() == 1 {
            let idx = self.candidates.first().unwrap();
            return KeyResult::Solved(&self.schema.configurations[idx]);
        }

        // Collect unique fields (by shape pointer) across remaining candidates
        let mut unique_fields: Vec<&'a FieldInfo> = Vec::new();
        for idx in self.candidates.iter() {
            let config = &self.schema.configurations[idx];
            if let Some(info) = config.field(key) {
                // Deduplicate by shape pointer
                if !unique_fields
                    .iter()
                    .any(|f| core::ptr::eq(f.value_shape, info.value_shape))
                {
                    unique_fields.push(info);
                }
            }
        }

        if unique_fields.len() == 1 {
            // All candidates have the same type - unambiguous
            KeyResult::Unambiguous {
                shape: unique_fields[0].value_shape,
            }
        } else {
            // Different types - need disambiguation
            // Attach specificity scores so caller can pick most specific when multiple match
            let fields_with_scores: Vec<_> = unique_fields
                .into_iter()
                .map(|f| (f, specificity_score(f.value_shape)))
                .collect();
            KeyResult::Ambiguous {
                fields: fields_with_scores,
            }
        }
    }

    /// Report which fields the value can satisfy after `Ambiguous` result.
    ///
    /// The deserializer should pass the subset of fields (from the `Ambiguous` result)
    /// that the actual value can be parsed into.
    pub fn satisfy(&mut self, satisfied_fields: &[&FieldInfo]) -> SatisfyResult<'a> {
        let satisfied_shapes: Vec<_> = satisfied_fields.iter().map(|f| f.value_shape).collect();
        self.satisfy_shapes(&satisfied_shapes)
    }

    /// Report which shapes the value can satisfy after `Ambiguous` result from `probe_key`.
    ///
    /// This is the shape-based version of `satisfy`, used when disambiguating
    /// by nested field types. The deserializer should pass the shapes that
    /// the actual value can be parsed into.
    ///
    /// # Example
    ///
    /// ```rust
    /// use facet::Facet;
    /// use facet_solver::{Schema, Solver, KeyResult, SatisfyResult};
    ///
    /// #[derive(Facet)]
    /// struct SmallPayload { value: u8 }
    ///
    /// #[derive(Facet)]
    /// struct LargePayload { value: u16 }
    ///
    /// #[derive(Facet)]
    /// #[repr(u8)]
    /// enum PayloadKind {
    ///     Small { payload: SmallPayload },
    ///     Large { payload: LargePayload },
    /// }
    ///
    /// #[derive(Facet)]
    /// struct Container {
    ///     #[facet(flatten)]
    ///     inner: PayloadKind,
    /// }
    ///
    /// let schema = Schema::build(Container::SHAPE).unwrap();
    /// let mut solver = Solver::new(&schema);
    ///
    /// // Report nested key
    /// solver.probe_key(&[], "payload");
    ///
    /// // At payload.value, value is 1000 - doesn't fit u8
    /// // Get shapes at this path
    /// let shapes = solver.get_shapes_at_path(&["payload", "value"]);
    /// // Filter to shapes that can hold 1000
    /// let works: Vec<_> = shapes.iter()
    ///     .filter(|s| s.type_identifier == "u16")
    ///     .copied()
    ///     .collect();
    /// solver.satisfy_shapes(&works);
    /// ```
    pub fn satisfy_shapes(&mut self, satisfied_shapes: &[&'static Shape]) -> SatisfyResult<'a> {
        if satisfied_shapes.is_empty() {
            self.candidates = ConfigSet::empty(self.schema.configurations.len());
            return SatisfyResult::NoMatch;
        }

        let mut new_candidates = ConfigSet::empty(self.schema.configurations.len());
        for idx in self.candidates.iter() {
            let config = &self.schema.configurations[idx];
            // Check if any of this config's fields match the satisfied shapes
            for field in config.fields.values() {
                if satisfied_shapes
                    .iter()
                    .any(|s| core::ptr::eq(*s, field.value_shape))
                {
                    new_candidates.insert(idx);
                    break;
                }
            }
        }
        self.candidates = new_candidates;

        match self.candidates.len() {
            0 => SatisfyResult::NoMatch,
            1 => {
                let idx = self.candidates.first().unwrap();
                SatisfyResult::Solved(&self.schema.configurations[idx])
            }
            _ => SatisfyResult::Continue,
        }
    }

    /// Get the shapes at a nested path across all remaining candidates.
    ///
    /// This is useful when you have an `Ambiguous` result from `probe_key`
    /// and need to know what types are possible at that path.
    pub fn get_shapes_at_path(&self, path: &[&str]) -> Vec<&'static Shape> {
        let mut shapes: Vec<&'static Shape> = Vec::new();
        for idx in self.candidates.iter() {
            let config = &self.schema.configurations[idx];
            if let Some(shape) = self.get_shape_at_path(config, path) {
                if !shapes.iter().any(|s| core::ptr::eq(*s, shape)) {
                    shapes.push(shape);
                }
            }
        }
        shapes
    }

    /// Report which shapes at a nested path the value can satisfy.
    ///
    /// This is the path-aware version of `satisfy_shapes`, used when disambiguating
    /// by nested field types after `probe_key`.
    ///
    /// - `path`: The full path to the field (e.g., `["payload", "value"]`)
    /// - `satisfied_shapes`: The shapes that the value can be parsed into
    pub fn satisfy_at_path(
        &mut self,
        path: &[&str],
        satisfied_shapes: &[&'static Shape],
    ) -> SatisfyResult<'a> {
        if satisfied_shapes.is_empty() {
            self.candidates = ConfigSet::empty(self.schema.configurations.len());
            return SatisfyResult::NoMatch;
        }

        // Keep only candidates where the shape at this path is in the satisfied set
        let mut new_candidates = ConfigSet::empty(self.schema.configurations.len());
        for idx in self.candidates.iter() {
            let config = &self.schema.configurations[idx];
            if let Some(shape) = self.get_shape_at_path(config, path) {
                if satisfied_shapes.iter().any(|s| core::ptr::eq(*s, shape)) {
                    new_candidates.insert(idx);
                }
            }
        }
        self.candidates = new_candidates;

        match self.candidates.len() {
            0 => SatisfyResult::NoMatch,
            1 => {
                let idx = self.candidates.first().unwrap();
                SatisfyResult::Solved(&self.schema.configurations[idx])
            }
            _ => SatisfyResult::Continue,
        }
    }

    /// Get the current candidate configurations.
    pub fn candidates(&self) -> Vec<&'a Configuration> {
        self.candidates
            .iter()
            .map(|idx| &self.schema.configurations[idx])
            .collect()
    }

    /// Get the seen keys.
    pub fn seen_keys(&self) -> &BTreeSet<&'a str> {
        &self.seen_keys
    }

    /// Hint that a specific enum variant should be selected.
    ///
    /// This filters the candidates to only those configurations where at least one
    /// variant selection has the given variant name. This is useful for explicit
    /// type disambiguation via annotations (e.g., KDL type annotations like `(Http)node`).
    ///
    /// Returns `true` if at least one candidate remains after filtering, `false` if
    /// no candidates match the variant name (in which case candidates are unchanged).
    ///
    /// # Example
    ///
    /// ```rust
    /// use facet::Facet;
    /// use facet_solver::{Schema, Solver};
    ///
    /// #[derive(Facet)]
    /// struct HttpSource { url: String }
    ///
    /// #[derive(Facet)]
    /// struct GitSource { url: String, branch: String }
    ///
    /// #[derive(Facet)]
    /// #[repr(u8)]
    /// enum SourceKind {
    ///     Http(HttpSource),
    ///     Git(GitSource),
    /// }
    ///
    /// #[derive(Facet)]
    /// struct Source {
    ///     #[facet(flatten)]
    ///     kind: SourceKind,
    /// }
    ///
    /// let schema = Schema::build(Source::SHAPE).unwrap();
    /// let mut solver = Solver::new(&schema);
    ///
    /// // Without hint, both variants are candidates
    /// assert_eq!(solver.candidates().len(), 2);
    ///
    /// // Hint at Http variant
    /// assert!(solver.hint_variant("Http"));
    /// assert_eq!(solver.candidates().len(), 1);
    /// ```
    pub fn hint_variant(&mut self, variant_name: &str) -> bool {
        // Build a set of configs that have this variant name
        let mut matching = ConfigSet::empty(self.schema.configurations.len());

        for idx in self.candidates.iter() {
            let config = &self.schema.configurations[idx];
            // Check if any variant selection matches the given name
            if config
                .variant_selections()
                .iter()
                .any(|vs| vs.variant_name == variant_name)
            {
                matching.insert(idx);
            }
        }

        if matching.is_empty() {
            // No matches - keep candidates unchanged
            false
        } else {
            self.candidates = matching;
            true
        }
    }

    /// Mark a key as seen without filtering candidates.
    ///
    /// This is useful when the key is known to be present through means other than
    /// parsing (e.g., type annotations). Call this after `hint_variant` to mark
    /// the variant name as seen so that `finish()` doesn't report it as missing.
    pub fn mark_seen(&mut self, key: &'a str) {
        self.seen_keys.insert(key);
    }

    /// Report a key at a nested path. Returns what to do next.
    ///
    /// This is the depth-aware version of `see_key`. Use this when probing
    /// nested structures where disambiguation might require looking inside objects.
    ///
    /// - `path`: The ancestor keys (e.g., `["payload"]` when inside a payload object)
    /// - `key`: The key found at this level (e.g., `"value"`)
    ///
    /// # Example
    ///
    /// ```rust
    /// use facet::Facet;
    /// use facet_solver::{Schema, Solver, KeyResult};
    ///
    /// #[derive(Facet)]
    /// struct SmallPayload { value: u8 }
    ///
    /// #[derive(Facet)]
    /// struct LargePayload { value: u16 }
    ///
    /// #[derive(Facet)]
    /// #[repr(u8)]
    /// enum PayloadKind {
    ///     Small { payload: SmallPayload },
    ///     Large { payload: LargePayload },
    /// }
    ///
    /// #[derive(Facet)]
    /// struct Container {
    ///     #[facet(flatten)]
    ///     inner: PayloadKind,
    /// }
    ///
    /// let schema = Schema::build(Container::SHAPE).unwrap();
    /// let mut solver = Solver::new(&schema);
    ///
    /// // "payload" exists in both - keep going
    /// solver.probe_key(&[], "payload");
    ///
    /// // "value" inside payload - both have it but different types!
    /// match solver.probe_key(&["payload"], "value") {
    ///     KeyResult::Ambiguous { fields } => {
    ///         // fields is Vec<(&FieldInfo, u64)> - field + specificity score
    ///         // Deserializer checks: 1000 fits u16 but not u8
    ///         // When multiple match, pick the one with lowest score (most specific)
    ///     }
    ///     _ => {}
    /// }
    /// ```
    pub fn probe_key(&mut self, path: &[&str], key: &str) -> KeyResult<'a> {
        // Build full path
        let mut full_path: Vec<&str> = path.to_vec();
        full_path.push(key);

        // Filter candidates to only those that have this key path
        let mut new_candidates = ConfigSet::empty(self.schema.configurations.len());
        for idx in self.candidates.iter() {
            let config = &self.schema.configurations[idx];
            if config.has_key_path(&full_path) {
                new_candidates.insert(idx);
            }
        }
        self.candidates = new_candidates;

        if self.candidates.is_empty() {
            return KeyResult::Unknown;
        }

        // Check if we've disambiguated to exactly one
        if self.candidates.len() == 1 {
            let idx = self.candidates.first().unwrap();
            return KeyResult::Solved(&self.schema.configurations[idx]);
        }

        // Get the shape at this path for each remaining candidate
        // We need to traverse the type tree to find the actual field type
        let mut unique_shapes: Vec<(&'static Shape, usize)> = Vec::new(); // (shape, config_idx)

        for idx in self.candidates.iter() {
            let config = &self.schema.configurations[idx];
            if let Some(shape) = self.get_shape_at_path(config, &full_path) {
                // Deduplicate by shape pointer
                if !unique_shapes.iter().any(|(s, _)| core::ptr::eq(*s, shape)) {
                    unique_shapes.push((shape, idx));
                }
            }
        }

        match unique_shapes.len() {
            0 => KeyResult::Unknown,
            1 => {
                // All candidates have the same type at this path - unambiguous
                KeyResult::Unambiguous {
                    shape: unique_shapes[0].0,
                }
            }
            _ => {
                // Different types at this path - need disambiguation
                // Build FieldInfo with scores for each unique shape
                let fields: Vec<(&'a FieldInfo, u64)> = unique_shapes
                    .iter()
                    .filter_map(|(shape, idx)| {
                        let config = &self.schema.configurations[*idx];
                        // For nested paths, we need the parent field
                        // e.g., for ["payload", "value"], get the "payload" field
                        let field = if path.is_empty() {
                            config.field(key)
                        } else {
                            // Return the top-level field that contains this path
                            config.field(path[0])
                        }?;
                        Some((field, specificity_score(shape)))
                    })
                    .collect();

                KeyResult::Ambiguous { fields }
            }
        }
    }

    /// Get the shape at a nested path within a configuration.
    fn get_shape_at_path(
        &self,
        config: &'a Configuration,
        path: &[&str],
    ) -> Option<&'static Shape> {
        if path.is_empty() {
            return None;
        }

        // Start with the top-level field
        let top_field = config.field(path[0])?;
        let mut current_shape = top_field.value_shape;

        // Navigate through nested structs
        for &key in &path[1..] {
            current_shape = self.get_field_shape(current_shape, key)?;
        }

        Some(current_shape)
    }

    /// Get the shape of a field within a struct shape.
    fn get_field_shape(&self, shape: &'static Shape, field_name: &str) -> Option<&'static Shape> {
        use facet_core::{StructType, Type, UserType};

        match shape.ty {
            Type::User(UserType::Struct(StructType { fields, .. })) => {
                for field in fields {
                    if field.name == field_name {
                        return Some(field.shape());
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Finish solving. Call this after all keys have been processed.
    ///
    /// This method is necessary because key-based filtering alone cannot disambiguate
    /// when one variant's required fields are a subset of another's.
    ///
    /// # Why not just use `see_key()` results?
    ///
    /// `see_key()` returns `Solved` when a key *excludes* candidates down to one.
    /// But when the input is a valid subset of multiple variants, no key excludes
    /// anything — you need `finish()` to check which candidates have all their
    /// required fields satisfied.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// enum Source {
    ///     Http { url: String },                  // required: url
    ///     Git { url: String, branch: String },   // required: url, branch
    /// }
    /// ```
    ///
    /// | Input                  | `see_key` behavior                        | Resolution            |
    /// |------------------------|-------------------------------------------|-----------------------|
    /// | `{ "url", "branch" }`  | `branch` excludes `Http` → candidates = 1 | Early `Solved(Git)`   |
    /// | `{ "url" }`            | both have `url` → candidates = 2          | `finish()` → `Http`   |
    ///
    /// In the second case, no key ever excludes a candidate. Only `finish()` can
    /// determine that `Git` is missing its required `branch` field, leaving `Http`
    /// as the sole viable configuration.
    pub fn finish(self) -> Result<&'a Configuration, SolverError> {
        if self.candidates.is_empty() {
            return Err(SolverError::NoMatch {
                input_fields: self.seen_keys.iter().map(|s| (*s).into()).collect(),
                missing_required: Vec::new(),
                missing_required_detailed: Vec::new(),
                unknown_fields: Vec::new(),
                closest_config: None,
            });
        }

        // Filter candidates to only those that have all required fields satisfied
        let viable: Vec<usize> = self
            .candidates
            .iter()
            .filter(|idx| {
                let config = &self.schema.configurations[*idx];
                config
                    .required_field_names
                    .iter()
                    .all(|f| self.seen_keys.contains(f))
            })
            .collect();

        match viable.len() {
            0 => {
                let first_idx = self.candidates.first().unwrap();
                let first_config = &self.schema.configurations[first_idx];
                let missing: Vec<_> = first_config
                    .required_field_names
                    .iter()
                    .filter(|f| !self.seen_keys.contains(*f))
                    .copied()
                    .collect();
                let missing_detailed: Vec<_> = missing
                    .iter()
                    .filter_map(|name| first_config.field(name))
                    .map(MissingFieldInfo::from_field_info)
                    .collect();
                Err(SolverError::NoMatch {
                    input_fields: self.seen_keys.iter().map(|s| (*s).into()).collect(),
                    missing_required: missing,
                    missing_required_detailed: missing_detailed,
                    unknown_fields: Vec::new(),
                    closest_config: Some(first_config.describe()),
                })
            }
            _ => {
                // Return first viable (preserves variant declaration order)
                Ok(&self.schema.configurations[viable[0]])
            }
        }
    }
}

// ============================================================================
// Probing Solver (Depth-Aware)
// ============================================================================

/// Result of reporting a key to the probing solver.
#[derive(Debug)]
pub enum ProbeResult<'a> {
    /// Keep reporting keys - not yet disambiguated
    KeepGoing,
    /// Solved! Use this configuration
    Solved(&'a Configuration),
    /// No configuration matches the observed keys
    NoMatch,
}

/// Depth-aware probing solver for streaming deserialization.
///
/// Unlike the batch solver or incremental solver, this solver accepts
/// key reports at arbitrary depths. It's designed for the "peek" strategy:
///
/// 1. Deserializer scans keys (without parsing values) and reports them
/// 2. Solver filters candidates based on which configs have that key path
/// 3. Once one candidate remains, solver returns `Solved`
/// 4. Deserializer rewinds and parses into the resolved type
///
/// # Example
///
/// ```rust
/// use facet::Facet;
/// use facet_solver::{Schema, ProbingSolver, ProbeResult};
///
/// #[derive(Facet)]
/// struct TextPayload { content: String }
///
/// #[derive(Facet)]
/// struct BinaryPayload { bytes: Vec<u8> }
///
/// #[derive(Facet)]
/// #[repr(u8)]
/// enum MessageKind {
///     Text { payload: TextPayload },
///     Binary { payload: BinaryPayload },
/// }
///
/// #[derive(Facet)]
/// struct Message {
///     id: String,
///     #[facet(flatten)]
///     kind: MessageKind,
/// }
///
/// let schema = Schema::build(Message::SHAPE).unwrap();
/// let mut solver = ProbingSolver::new(&schema);
///
/// // "id" exists in both configs - keep going
/// assert!(matches!(solver.probe_key(&[], "id"), ProbeResult::KeepGoing));
///
/// // "payload" exists in both configs - keep going
/// assert!(matches!(solver.probe_key(&[], "payload"), ProbeResult::KeepGoing));
///
/// // "content" inside payload only exists in Text - solved!
/// match solver.probe_key(&["payload"], "content") {
///     ProbeResult::Solved(config) => {
///         assert!(config.has_key_path(&["payload", "content"]));
///     }
///     _ => panic!("expected Solved"),
/// }
/// ```
#[derive(Debug)]
pub struct ProbingSolver<'a> {
    /// Remaining candidate configurations
    candidates: Vec<&'a Configuration>,
}

impl<'a> ProbingSolver<'a> {
    /// Create a new probing solver from a schema.
    pub fn new(schema: &'a Schema) -> Self {
        Self {
            candidates: schema.configurations.iter().collect(),
        }
    }

    /// Create a new probing solver from configurations directly.
    pub fn from_configurations(configs: &'a [Configuration]) -> Self {
        Self {
            candidates: configs.iter().collect(),
        }
    }

    /// Report a key found at a path during probing.
    ///
    /// - `path`: The ancestor keys (e.g., `["payload"]` when inside the payload object)
    /// - `key`: The key found at this level (e.g., `"content"`)
    ///
    /// Returns what to do next.
    pub fn probe_key(&mut self, path: &[&str], key: &str) -> ProbeResult<'a> {
        // Build the full key path (runtime strings, compared against static schema)
        let mut full_path: Vec<&str> = path.to_vec();
        full_path.push(key);

        // Filter to candidates that have this key path
        self.candidates.retain(|c| c.has_key_path(&full_path));

        match self.candidates.len() {
            0 => ProbeResult::NoMatch,
            1 => ProbeResult::Solved(self.candidates[0]),
            _ => ProbeResult::KeepGoing,
        }
    }

    /// Get the current candidate configurations.
    pub fn candidates(&self) -> &[&'a Configuration] {
        &self.candidates
    }

    /// Finish probing - returns Solved if exactly one candidate remains.
    pub fn finish(&self) -> ProbeResult<'a> {
        match self.candidates.len() {
            0 => ProbeResult::NoMatch,
            1 => ProbeResult::Solved(self.candidates[0]),
            _ => ProbeResult::KeepGoing, // Still ambiguous
        }
    }
}

// ============================================================================
// Schema Builder
// ============================================================================

/// How enum variants are represented in the serialized format.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EnumRepr {
    /// Variant fields are flattened to the same level as other fields.
    /// Used by formats like KDL, TOML where all fields appear at one level.
    /// Example: `{"name": "...", "host": "...", "port": 8080}`
    #[default]
    Flattened,

    /// Variant name is a key, variant content is nested under it.
    /// Used by JSON with externally-tagged enums.
    /// Example: `{"name": "...", "Tcp": {"host": "...", "port": 8080}}`
    ExternallyTagged,
}

impl Schema {
    /// Build a schema for the given shape with flattened enum representation.
    ///
    /// Returns an error if the type definition contains conflicts, such as
    /// duplicate field names from parent and flattened structs.
    pub fn build(shape: &'static Shape) -> Result<Self, SchemaError> {
        Self::build_with_repr(shape, EnumRepr::Flattened)
    }

    /// Build a schema for externally-tagged enum representation (e.g., JSON).
    ///
    /// In this representation, the variant name appears as a key and the
    /// variant's content is nested under it. The solver will only expect
    /// to see the variant name as a top-level key, not the variant's fields.
    pub fn build_externally_tagged(shape: &'static Shape) -> Result<Self, SchemaError> {
        Self::build_with_repr(shape, EnumRepr::ExternallyTagged)
    }

    /// Build a schema with the specified enum representation.
    pub fn build_with_repr(shape: &'static Shape, repr: EnumRepr) -> Result<Self, SchemaError> {
        let builder = SchemaBuilder::new(shape, repr);
        builder.into_schema()
    }

    /// Get the configurations for this schema.
    pub fn configurations(&self) -> &[Configuration] {
        &self.configurations
    }
}

impl Configuration {
    /// Create a new empty configuration.
    fn new() -> Self {
        Self {
            variant_selections: Vec::new(),
            fields: BTreeMap::new(),
            required_field_names: BTreeSet::new(),
            known_paths: BTreeSet::new(),
        }
    }

    /// Add a key path (for depth-aware probing).
    fn add_key_path(&mut self, path: KeyPath) {
        self.known_paths.insert(path);
    }

    /// Add a field to this configuration.
    ///
    /// Returns an error if a field with the same serialized name already exists
    /// but comes from a different source (different path). This catches duplicate
    /// field name conflicts between parent structs and flattened fields.
    fn add_field(&mut self, info: FieldInfo) -> Result<(), SchemaError> {
        if let Some(existing) = self.fields.get(info.serialized_name) {
            if existing.path != info.path {
                return Err(SchemaError::DuplicateField {
                    field_name: info.serialized_name,
                    first_path: existing.path.clone(),
                    second_path: info.path,
                });
            }
        }
        if info.required {
            self.required_field_names.insert(info.serialized_name);
        }
        self.fields.insert(info.serialized_name, info);
        Ok(())
    }

    /// Add a variant selection to this configuration.
    fn add_variant_selection(
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

    /// Merge another configuration into this one.
    ///
    /// Returns an error if a field with the same serialized name already exists
    /// but comes from a different source (different path). This catches duplicate
    /// field name conflicts between parent structs and flattened fields.
    fn merge(&mut self, other: &Configuration) -> Result<(), SchemaError> {
        for (name, info) in &other.fields {
            if let Some(existing) = self.fields.get(*name) {
                if existing.path != info.path {
                    return Err(SchemaError::DuplicateField {
                        field_name: name,
                        first_path: existing.path.clone(),
                        second_path: info.path.clone(),
                    });
                }
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
    fn mark_all_optional(&mut self) {
        self.required_field_names.clear();
        for info in self.fields.values_mut() {
            info.required = false;
        }
    }

    /// Check if this configuration matches the input fields.
    pub fn matches(&self, input_fields: &BTreeSet<&str>) -> MatchResult {
        let mut missing_required = Vec::new();
        let mut missing_optional = Vec::new();

        for (name, info) in &self.fields {
            if !input_fields.contains(name) {
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
            .filter(|f| !self.fields.contains_key(*f))
            .map(|s| (*s).into())
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

    /// Get a human-readable description of this configuration.
    ///
    /// Returns something like `MessagePayload::Text` or `Auth::Token + Transport::Tcp`
    /// for configurations with multiple variant selections.
    pub fn describe(&self) -> String {
        if self.variant_selections.is_empty() {
            String::from("(no variants)")
        } else {
            let parts: Vec<_> = self
                .variant_selections
                .iter()
                .map(|vs| alloc::format!("{}::{}", vs.enum_name, vs.variant_name))
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
    pub fn fields(&self) -> &BTreeMap<&'static str, FieldInfo> {
        &self.fields
    }

    /// Get optional fields that were NOT provided in the input.
    ///
    /// This is useful for deserializers that need to initialize missing
    /// optional fields to `None` or their default value.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use facet::Facet;
    /// # use facet_solver::Schema;
    /// # use std::collections::BTreeSet;
    /// #[derive(Facet)]
    /// struct Config {
    ///     required_field: String,
    ///     optional_field: Option<u32>,
    /// }
    ///
    /// let schema = Schema::build(Config::SHAPE).unwrap();
    /// let config = &schema.configurations()[0];
    ///
    /// // If we only saw "required_field"
    /// let mut seen = BTreeSet::new();
    /// seen.insert("required_field");
    ///
    /// let missing: Vec<_> = config.missing_optional_fields(&seen).collect();
    /// assert_eq!(missing.len(), 1);
    /// assert_eq!(missing[0].serialized_name, "optional_field");
    /// ```
    pub fn missing_optional_fields<'a>(
        &'a self,
        seen_keys: &'a BTreeSet<&str>,
    ) -> impl Iterator<Item = &'a FieldInfo> {
        self.fields
            .values()
            .filter(move |info| !info.required && !seen_keys.contains(info.serialized_name))
    }

    /// Get variant selections.
    pub fn variant_selections(&self) -> &[VariantSelection] {
        &self.variant_selections
    }

    /// Get all known key paths (for depth-aware probing).
    pub fn known_paths(&self) -> &BTreeSet<KeyPath> {
        &self.known_paths
    }

    /// Check if this configuration has a specific key path.
    /// Compares runtime strings against static schema paths.
    pub fn has_key_path(&self, path: &[&str]) -> bool {
        self.known_paths.iter().any(|known| {
            known.len() == path.len() && known.iter().zip(path.iter()).all(|(a, b)| *a == *b)
        })
    }
}

struct SchemaBuilder {
    shape: &'static Shape,
    enum_repr: EnumRepr,
}

impl SchemaBuilder {
    fn new(shape: &'static Shape, enum_repr: EnumRepr) -> Self {
        Self { shape, enum_repr }
    }

    fn analyze(&self) -> Result<Vec<Configuration>, SchemaError> {
        self.analyze_shape(self.shape, FieldPath::empty(), Vec::new())
    }

    /// Analyze a shape and return all possible configurations.
    /// Returns a Vec because enums create multiple configurations.
    ///
    /// - `current_path`: The internal field path (for FieldInfo)
    /// - `key_prefix`: The serialized key path prefix (for known_paths)
    fn analyze_shape(
        &self,
        shape: &'static Shape,
        current_path: FieldPath,
        key_prefix: KeyPath,
    ) -> Result<Vec<Configuration>, SchemaError> {
        match shape.ty {
            Type::User(UserType::Struct(struct_type)) => {
                self.analyze_struct(struct_type, current_path, key_prefix)
            }
            Type::User(UserType::Enum(enum_type)) => {
                // Enum at root level: create one configuration per variant
                self.analyze_enum(shape, enum_type, current_path, key_prefix)
            }
            _ => {
                // For non-struct types at root level, return single empty config
                Ok(vec![Configuration::new()])
            }
        }
    }

    /// Analyze an enum and return one configuration per variant.
    ///
    /// - `current_path`: The internal field path (for FieldInfo)
    /// - `key_prefix`: The serialized key path prefix (for known_paths)
    fn analyze_enum(
        &self,
        shape: &'static Shape,
        enum_type: facet_core::EnumType,
        current_path: FieldPath,
        key_prefix: KeyPath,
    ) -> Result<Vec<Configuration>, SchemaError> {
        let enum_name = shape.type_identifier;
        let mut result = Vec::new();

        for variant in enum_type.variants {
            let mut config = Configuration::new();

            // Record this variant selection
            config.add_variant_selection(current_path.clone(), enum_name, variant.name);

            let variant_path = current_path.push_variant("", variant.name);

            // Get configurations from the variant's content
            let variant_configs =
                self.analyze_variant_content(variant, &variant_path, &key_prefix)?;

            // Merge each variant config into the base
            for variant_config in variant_configs {
                let mut final_config = config.clone();
                final_config.merge(&variant_config)?;
                result.push(final_config);
            }
        }

        Ok(result)
    }

    /// Analyze a struct and return all possible configurations.
    ///
    /// - `current_path`: The internal field path (for FieldInfo)
    /// - `key_prefix`: The serialized key path prefix (for known_paths)
    fn analyze_struct(
        &self,
        struct_type: StructType,
        current_path: FieldPath,
        key_prefix: KeyPath,
    ) -> Result<Vec<Configuration>, SchemaError> {
        // Start with one empty configuration
        let mut configs = vec![Configuration::new()];

        // Process each field, potentially multiplying configurations
        for field in struct_type.fields {
            configs =
                self.analyze_field_into_configs(field, &current_path, &key_prefix, configs)?;
        }

        Ok(configs)
    }

    /// Process a field and return updated configurations.
    /// If the field is a flattened enum, this may multiply the number of configs.
    ///
    /// - `parent_path`: The internal field path to the parent (for FieldInfo)
    /// - `key_prefix`: The serialized key path prefix (for known_paths)
    fn analyze_field_into_configs(
        &self,
        field: &'static Field,
        parent_path: &FieldPath,
        key_prefix: &KeyPath,
        mut configs: Vec<Configuration>,
    ) -> Result<Vec<Configuration>, SchemaError> {
        let is_flatten = field.flags.contains(FieldFlags::FLATTEN);

        if is_flatten {
            // Flattened: inner keys bubble up to current level (same key_prefix)
            self.analyze_flattened_field_into_configs(field, parent_path, key_prefix, configs)
        } else {
            // Regular field: add to ALL current configs
            let field_path = parent_path.push_field(field.name);
            let required =
                !field.flags.contains(FieldFlags::DEFAULT) && !is_option_type(field.shape());

            // Build the key path for this field
            let mut field_key_path = key_prefix.clone();
            field_key_path.push(field.name);

            let field_info = FieldInfo {
                serialized_name: field.name,
                path: field_path,
                required,
                value_shape: field.shape(),
            };

            for config in &mut configs {
                config.add_field(field_info.clone())?;
                // Add this field's key path
                config.add_key_path(field_key_path.clone());
            }

            // If the field's value is a struct, recurse to collect nested key paths
            // (for probing, not for flattening - these are nested in serialized format)
            // This may fork configurations if the nested struct contains flattened enums!
            configs =
                self.collect_nested_key_paths_for_shape(field.shape(), &field_key_path, configs)?;

            Ok(configs)
        }
    }

    /// Collect nested key paths from a shape into configurations.
    /// This handles the case where a non-flattened field contains a struct with flattened enums.
    /// Returns updated configurations (may fork if flattened enums are encountered).
    fn collect_nested_key_paths_for_shape(
        &self,
        shape: &'static Shape,
        key_prefix: &KeyPath,
        configs: Vec<Configuration>,
    ) -> Result<Vec<Configuration>, SchemaError> {
        match shape.ty {
            Type::User(UserType::Struct(struct_type)) => {
                self.collect_nested_key_paths_for_struct(struct_type, key_prefix, configs)
            }
            _ => Ok(configs),
        }
    }

    /// Collect nested key paths from a struct, potentially forking for flattened enums.
    fn collect_nested_key_paths_for_struct(
        &self,
        struct_type: StructType,
        key_prefix: &KeyPath,
        mut configs: Vec<Configuration>,
    ) -> Result<Vec<Configuration>, SchemaError> {
        for field in struct_type.fields {
            let is_flatten = field.flags.contains(FieldFlags::FLATTEN);
            let mut field_key_path = key_prefix.clone();

            if is_flatten {
                // Flattened field: keys bubble up to current level, may fork configs
                configs =
                    self.collect_nested_key_paths_for_flattened(field, key_prefix, configs)?;
            } else {
                // Regular field: add key path and recurse
                field_key_path.push(field.name);

                for config in &mut configs {
                    config.add_key_path(field_key_path.clone());
                }

                // Recurse into nested structs
                configs = self.collect_nested_key_paths_for_shape(
                    field.shape(),
                    &field_key_path,
                    configs,
                )?;
            }
        }
        Ok(configs)
    }

    /// Handle flattened fields when collecting nested key paths.
    /// This may fork configurations for flattened enums.
    fn collect_nested_key_paths_for_flattened(
        &self,
        field: &'static Field,
        key_prefix: &KeyPath,
        configs: Vec<Configuration>,
    ) -> Result<Vec<Configuration>, SchemaError> {
        let shape = field.shape();

        match shape.ty {
            Type::User(UserType::Struct(struct_type)) => {
                // Flattened struct: recurse with same key_prefix
                self.collect_nested_key_paths_for_struct(struct_type, key_prefix, configs)
            }
            Type::User(UserType::Enum(enum_type)) => {
                // Flattened enum: fork configurations
                // We need to match each config to its corresponding variant
                let mut result = Vec::new();

                for config in configs {
                    // Find which variant this config has selected for this field
                    let selected_variant = config
                        .variant_selections
                        .iter()
                        .find(|vs| {
                            // Match by the field name in the path
                            vs.path.segments.last() == Some(&PathSegment::Field(field.name))
                        })
                        .map(|vs| vs.variant_name);

                    if let Some(variant_name) = selected_variant {
                        // Find the variant and collect its key paths
                        if let Some(variant) =
                            enum_type.variants.iter().find(|v| v.name == variant_name)
                        {
                            let mut updated_config = config;
                            updated_config = self.collect_variant_key_paths(
                                variant,
                                key_prefix,
                                updated_config,
                            )?;
                            result.push(updated_config);
                        } else {
                            result.push(config);
                        }
                    } else {
                        result.push(config);
                    }
                }
                Ok(result)
            }
            _ => Ok(configs),
        }
    }

    /// Collect key paths from an enum variant's content.
    fn collect_variant_key_paths(
        &self,
        variant: &'static Variant,
        key_prefix: &KeyPath,
        mut config: Configuration,
    ) -> Result<Configuration, SchemaError> {
        // Check if this is a newtype variant (single unnamed field)
        if variant.data.fields.len() == 1 && variant.data.fields[0].name == "0" {
            let inner_field = &variant.data.fields[0];
            let inner_shape = inner_field.shape();

            // If the inner type is a struct, flatten its fields
            if let Type::User(UserType::Struct(inner_struct)) = inner_shape.ty {
                let configs = self.collect_nested_key_paths_for_struct(
                    inner_struct,
                    key_prefix,
                    vec![config],
                )?;
                return Ok(configs
                    .into_iter()
                    .next()
                    .unwrap_or_else(Configuration::new));
            }
        }

        // Named fields - process each
        for variant_field in variant.data.fields {
            let is_flatten = variant_field.flags.contains(FieldFlags::FLATTEN);

            if is_flatten {
                let configs = self.collect_nested_key_paths_for_flattened(
                    variant_field,
                    key_prefix,
                    vec![config],
                )?;
                config = configs
                    .into_iter()
                    .next()
                    .unwrap_or_else(Configuration::new);
            } else {
                let mut field_key_path = key_prefix.clone();
                field_key_path.push(variant_field.name);
                config.add_key_path(field_key_path.clone());

                let configs = self.collect_nested_key_paths_for_shape(
                    variant_field.shape(),
                    &field_key_path,
                    vec![config],
                )?;
                config = configs
                    .into_iter()
                    .next()
                    .unwrap_or_else(Configuration::new);
            }
        }
        Ok(config)
    }

    /// Collect ONLY key paths from a variant's content (no fields added).
    /// Used for externally-tagged enums where variant content is nested and
    /// will be parsed separately by the deserializer.
    fn collect_variant_key_paths_only(
        &self,
        variant: &'static Variant,
        key_prefix: &KeyPath,
        config: &mut Configuration,
    ) -> Result<(), SchemaError> {
        // Check if this is a newtype variant (single unnamed field)
        if variant.data.fields.len() == 1 && variant.data.fields[0].name == "0" {
            let inner_field = &variant.data.fields[0];
            let inner_shape = inner_field.shape();

            // If the inner type is a struct, add key paths for its fields
            if let Type::User(UserType::Struct(inner_struct)) = inner_shape.ty {
                Self::collect_struct_key_paths_only(inner_struct, key_prefix, config);
                return Ok(());
            }
        }

        // Named fields - add key paths for each
        for variant_field in variant.data.fields {
            let mut field_key_path = key_prefix.clone();
            field_key_path.push(variant_field.name);
            config.add_key_path(field_key_path.clone());

            // Recurse into nested structs
            if let Type::User(UserType::Struct(inner_struct)) = variant_field.shape().ty {
                Self::collect_struct_key_paths_only(inner_struct, &field_key_path, config);
            }
        }
        Ok(())
    }

    /// Recursively collect key paths from a struct (no fields added).
    fn collect_struct_key_paths_only(
        struct_type: StructType,
        key_prefix: &KeyPath,
        config: &mut Configuration,
    ) {
        for field in struct_type.fields {
            let is_flatten = field.flags.contains(FieldFlags::FLATTEN);

            if is_flatten {
                // Flattened field: keys bubble up to current level
                if let Type::User(UserType::Struct(inner_struct)) = field.shape().ty {
                    Self::collect_struct_key_paths_only(inner_struct, key_prefix, config);
                }
            } else {
                // Regular field: add its key path
                let mut field_key_path = key_prefix.clone();
                field_key_path.push(field.name);
                config.add_key_path(field_key_path.clone());

                // Recurse into nested structs
                if let Type::User(UserType::Struct(inner_struct)) = field.shape().ty {
                    Self::collect_struct_key_paths_only(inner_struct, &field_key_path, config);
                }
            }
        }
    }

    /// Process a flattened field, potentially forking configurations for enums.
    ///
    /// For flattened fields, the inner keys bubble up to the current level,
    /// so we pass the same key_prefix (not key_prefix + field.name).
    ///
    /// If the field is `Option<T>`, we unwrap to get T and mark all resulting
    /// fields as optional (since the entire flattened block can be omitted).
    fn analyze_flattened_field_into_configs(
        &self,
        field: &'static Field,
        parent_path: &FieldPath,
        key_prefix: &KeyPath,
        configs: Vec<Configuration>,
    ) -> Result<Vec<Configuration>, SchemaError> {
        let field_path = parent_path.push_field(field.name);
        let original_shape = field.shape();

        // Check if this is Option<T> - if so, unwrap and mark all fields optional
        let (shape, is_optional_flatten) = match unwrap_option_type(original_shape) {
            Some(inner) => (inner, true),
            None => (original_shape, false),
        };

        match shape.ty {
            Type::User(UserType::Struct(struct_type)) => {
                // Flatten a struct: get its configurations and merge into each of ours
                // Key prefix stays the same - inner keys bubble up
                let mut struct_configs =
                    self.analyze_struct(struct_type, field_path, key_prefix.clone())?;

                // If the flatten field was Option<T>, mark all inner fields as optional
                if is_optional_flatten {
                    for config in &mut struct_configs {
                        config.mark_all_optional();
                    }
                }

                // Each of our configs combines with each struct config
                // (usually struct_configs has 1 element unless it contains enums)
                let mut result = Vec::new();
                for base_config in configs {
                    for struct_config in &struct_configs {
                        let mut merged = base_config.clone();
                        merged.merge(struct_config)?;
                        result.push(merged);
                    }
                }
                Ok(result)
            }
            Type::User(UserType::Enum(enum_type)) => {
                // Fork: each existing config × each variant
                let mut result = Vec::new();
                let enum_name = shape.type_identifier;

                for base_config in configs {
                    for variant in enum_type.variants {
                        let mut forked = base_config.clone();
                        forked.add_variant_selection(field_path.clone(), enum_name, variant.name);

                        let variant_path = field_path.push_variant(field.name, variant.name);

                        match self.enum_repr {
                            EnumRepr::ExternallyTagged => {
                                // For externally tagged enums, the variant name is a key
                                // at the current level, and its content is nested underneath.
                                let mut variant_key_prefix = key_prefix.clone();
                                variant_key_prefix.push(variant.name);

                                // Add the variant name itself as a known key path
                                forked.add_key_path(variant_key_prefix.clone());

                                // Add the variant name as a field (the key that selects this variant)
                                let variant_field_info = FieldInfo {
                                    serialized_name: variant.name,
                                    path: variant_path.clone(),
                                    required: !is_optional_flatten,
                                    value_shape: shape, // The enum shape
                                };
                                forked.add_field(variant_field_info)?;

                                // For externally-tagged enums, we do NOT add the variant's
                                // inner fields to required fields. They're nested and will
                                // be parsed separately by the deserializer.
                                // Only add them to known_paths for depth-aware probing.
                                self.collect_variant_key_paths_only(
                                    variant,
                                    &variant_key_prefix,
                                    &mut forked,
                                )?;

                                result.push(forked);
                            }
                            EnumRepr::Flattened => {
                                // For flattened enums, the variant's fields appear at the
                                // same level as other fields. The variant name is NOT a key;
                                // only the variant's inner fields are keys.

                                // Get configurations from the variant's content
                                // Key prefix stays the same - inner keys bubble up
                                let mut variant_configs = self.analyze_variant_content(
                                    variant,
                                    &variant_path,
                                    key_prefix,
                                )?;

                                // If the flatten field was Option<T>, mark all inner fields as optional
                                if is_optional_flatten {
                                    for config in &mut variant_configs {
                                        config.mark_all_optional();
                                    }
                                }

                                // Merge each variant config into the forked base
                                for variant_config in variant_configs {
                                    let mut final_config = forked.clone();
                                    final_config.merge(&variant_config)?;
                                    result.push(final_config);
                                }
                            }
                        }
                    }
                }
                Ok(result)
            }
            _ => {
                // Can't flatten other types - treat as regular field
                // For Option<T> flatten, also consider optionality from the wrapper
                let required = !field.flags.contains(FieldFlags::DEFAULT)
                    && !is_option_type(shape)
                    && !is_optional_flatten;

                // For non-flattenable types, add the field with its key path
                let mut field_key_path = key_prefix.clone();
                field_key_path.push(field.name);

                let field_info = FieldInfo {
                    serialized_name: field.name,
                    path: field_path,
                    required,
                    value_shape: shape,
                };

                let mut result = configs;
                for config in &mut result {
                    config.add_field(field_info.clone())?;
                    config.add_key_path(field_key_path.clone());
                }
                Ok(result)
            }
        }
    }

    /// Analyze a variant's content and return configurations.
    ///
    /// - `variant_path`: The internal field path (for FieldInfo)
    /// - `key_prefix`: The serialized key path prefix (for known_paths)
    fn analyze_variant_content(
        &self,
        variant: &'static Variant,
        variant_path: &FieldPath,
        key_prefix: &KeyPath,
    ) -> Result<Vec<Configuration>, SchemaError> {
        // Check if this is a newtype variant (single unnamed field like `Foo(Bar)`)
        if variant.data.fields.len() == 1 && variant.data.fields[0].name == "0" {
            let inner_field = &variant.data.fields[0];
            let inner_shape = inner_field.shape();

            // If the inner type is a struct, flatten its fields
            // Key prefix stays the same - inner keys bubble up
            if let Type::User(UserType::Struct(inner_struct)) = inner_shape.ty {
                let inner_path = variant_path.push_field("0");
                return self.analyze_struct(inner_struct, inner_path, key_prefix.clone());
            }
        }

        // Named fields or multiple fields - analyze as a pseudo-struct
        let mut configs = vec![Configuration::new()];
        for variant_field in variant.data.fields {
            configs =
                self.analyze_field_into_configs(variant_field, variant_path, key_prefix, configs)?;
        }
        Ok(configs)
    }

    fn into_schema(self) -> Result<Schema, SchemaError> {
        let configurations = self.analyze()?;
        let num_configs = configurations.len();

        // Build inverted index: field_name → bitmask of config indices
        let mut field_to_configs: BTreeMap<&'static str, ConfigSet> = BTreeMap::new();
        for (idx, config) in configurations.iter().enumerate() {
            for field_name in config.fields.keys() {
                field_to_configs
                    .entry(*field_name)
                    .or_insert_with(|| ConfigSet::empty(num_configs))
                    .insert(idx);
            }
        }

        Ok(Schema {
            shape: self.shape,
            configurations,
            field_to_configs,
        })
    }
}

/// Check if a shape represents an Option type.
fn is_option_type(shape: &'static Shape) -> bool {
    matches!(shape.def, Def::Option(_))
}

/// If shape is `Option<T>`, returns `Some(T's shape)`. Otherwise returns `None`.
fn unwrap_option_type(shape: &'static Shape) -> Option<&'static Shape> {
    match shape.def {
        Def::Option(option_def) => Some(option_def.t),
        _ => None,
    }
}
