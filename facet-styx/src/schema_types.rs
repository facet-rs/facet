//! Schema type definitions derived from the meta-schema.
//!
//! These types are deserialized from STYX schema files using facet-styx.

use std::collections::HashMap;

use facet::Facet;

/// A complete schema file.
#[derive(Facet, Debug, Clone)]
pub struct SchemaFile {
    /// Schema metadata (required).
    pub meta: Meta,
    /// External schema imports (optional).
    /// Maps namespace prefixes to external schema locations.
    #[facet(skip_serializing_if = Option::is_none)]
    pub imports: Option<HashMap<String, String>>,
    /// Type definitions.
    /// Keys are type names, or `None` for the document root (serialized as `@`).
    pub schema: HashMap<Option<String>, Schema>,
}

/// Schema metadata.
#[derive(Facet, Debug, Clone)]
pub struct Meta {
    /// Unique identifier for the schema (e.g., `crate:myapp-config@1`).
    pub id: String,
    /// Schema version (semver).
    #[facet(skip_serializing_if = Option::is_none)]
    pub version: Option<String>,
    /// CLI binary name for schema discovery.
    #[facet(skip_serializing_if = Option::is_none)]
    pub cli: Option<String>,
    /// Human-readable description.
    #[facet(skip_serializing_if = Option::is_none)]
    pub description: Option<String>,
    /// LSP extension configuration.
    #[facet(skip_serializing_if = Option::is_none)]
    pub lsp: Option<LspExtensionConfig>,
}

/// Configuration for LSP extensions.
#[derive(Facet, Debug, Clone)]
pub struct LspExtensionConfig {
    /// Command to launch the extension: (command arg1 arg2 ...)
    /// e.g., (dibs lsp-extension --stdio)
    pub launch: Vec<String>,
    /// Capabilities the extension supports (optional, discovered at runtime if omitted).
    #[facet(skip_serializing_if = Option::is_none)]
    pub capabilities: Option<Vec<String>>,
}

/// A type constraint (corresponds to Schema @enum{...} in the meta-schema).
///
/// This is a tagged enum - each variant corresponds to a STYX tag like
/// `@string`, `@int`, `@object`, `@seq`, etc.
#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "lowercase")]
#[repr(u8)]
pub enum Schema {
    // =========================================================================
    // Built-in scalar types with optional constraints
    // =========================================================================
    /// String type: @string or @string{minLen, maxLen, pattern}
    String(Option<StringConstraints>),

    /// Integer type: @int or @int{min, max}
    Int(Option<IntConstraints>),

    /// Float type: @float or @float{min, max}
    Float(Option<FloatConstraints>),

    /// Boolean type: @bool (no constraints)
    Bool,

    /// Unit type: @unit (the value must be unit `@`)
    Unit,

    /// Any type: @any (accepts any value)
    Any,

    // =========================================================================
    // Structural types
    // =========================================================================
    /// Object schema: @object{field @type, @ @type}
    Object(ObjectSchema),

    /// Sequence schema: @seq(@type)
    Seq(SeqSchema),

    /// Map schema: @map(@V) or @map(@K @V)
    Map(MapSchema),

    // =========================================================================
    // Combinators
    // =========================================================================
    /// Union: @union(@A @B ...)
    Union(UnionSchema),

    /// Optional: @optional(@T)
    Optional(OptionalSchema),

    /// Enum: @enum{variant @type, ...}
    Enum(EnumSchema),

    /// Value constraint: @one-of(@type value1 value2 ...)
    /// Constrains values to a finite set. First element is base type, rest are allowed values.
    #[facet(rename = "one-of")]
    OneOf(OneOfSchema),

    /// Flatten: @flatten(@Type) - inline fields from another type
    Flatten(FlattenSchema),

    // =========================================================================
    // Wrappers / modifiers
    // =========================================================================
    /// Default value: @default(value @type)
    Default(DefaultSchema),

    /// Deprecated field: @deprecated("reason" @type)
    Deprecated(DeprecatedSchema),

    // =========================================================================
    // Other
    // =========================================================================
    /// Literal value constraint (must match exactly)
    Literal(String),

    /// User-defined type reference (fallback for unknown tags)
    /// e.g., @MyCustomType becomes Type { name: "MyCustomType" }
    #[facet(other)]
    Type {
        #[facet(tag)]
        name: Option<String>,
    },
}

// =============================================================================
// Constraint types
// =============================================================================

/// Constraints for @string type.
#[derive(Facet, Debug, Clone, Default)]
#[facet(rename_all = "camelCase")]
pub struct StringConstraints {
    /// Minimum length (inclusive).
    pub min_len: Option<usize>,
    /// Maximum length (inclusive).
    pub max_len: Option<usize>,
    /// Regex pattern the string must match.
    pub pattern: Option<String>,
}

/// Constraints for @int type.
#[derive(Facet, Debug, Clone, Default)]
pub struct IntConstraints {
    /// Minimum value (inclusive).
    pub min: Option<i128>,
    /// Maximum value (inclusive).
    pub max: Option<i128>,
}

/// Constraints for @float type.
#[derive(Facet, Debug, Clone, Default)]
pub struct FloatConstraints {
    /// Minimum value (inclusive).
    pub min: Option<f64>,
    /// Maximum value (inclusive).
    pub max: Option<f64>,
}

// =============================================================================
// Structural schema types
// =============================================================================

/// A key in an object schema.
///
/// Object keys can be:
/// - Named fields: `name` → value = Some("name"), tag = None
/// - Type patterns: `@string` → value = None, tag = Some("string")
/// - Unit catch-all: `@` → value = None, tag = Some("")
///
/// This is a metadata container - `tag` is captured from the parser's FieldKey
/// via `#[facet(metadata = "tag")]`.
#[derive(Facet, Debug, Clone)]
#[facet(metadata_container)]
pub struct ObjectKey {
    /// The field name for named keys, or None for tag-based keys.
    pub value: Option<String>,
    /// The tag name for type patterns (`@string` → "string", `@` → "").
    #[facet(metadata = "tag")]
    pub tag: Option<String>,
}

impl ObjectKey {
    /// Create a named field key.
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            value: Some(name.into()),
            tag: None,
        }
    }

    /// Create a type pattern key (e.g., `@string`).
    pub fn typed(tag: impl Into<String>) -> Self {
        Self {
            value: None,
            tag: Some(tag.into()),
        }
    }

    /// Create a unit catch-all key (`@`).
    pub fn unit() -> Self {
        Self {
            value: None,
            tag: Some(String::new()),
        }
    }

    /// Returns true if this is a named field (not a tag pattern).
    pub fn is_named(&self) -> bool {
        self.value.is_some()
    }

    /// Returns true if this is a type pattern (e.g., `@string`).
    pub fn is_typed(&self) -> bool {
        self.tag.is_some() && !self.tag.as_ref().unwrap().is_empty()
    }

    /// Returns true if this is the unit catch-all (`@`).
    pub fn is_unit(&self) -> bool {
        self.tag.as_ref().is_some_and(|t| t.is_empty())
    }

    /// Get the field name if this is a named key.
    pub fn name(&self) -> Option<&str> {
        self.value.as_deref()
    }

    /// Get the tag name if this is a type pattern.
    pub fn tag_name(&self) -> Option<&str> {
        self.tag.as_deref().filter(|t| !t.is_empty())
    }
}

// Hash and Eq based on both value and tag
impl std::hash::Hash for ObjectKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state);
        self.tag.hash(state);
    }
}

impl PartialEq for ObjectKey {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value && self.tag == other.tag
    }
}

impl Eq for ObjectKey {}

/// Object schema: @object{field @Schema, @string @Schema}.
/// Maps field keys to their type constraints.
/// Keys can be named fields or type patterns (like `@string` for catch-all).
/// Keys are wrapped in `Documented<ObjectKey>` to carry field documentation.
#[derive(Facet, Debug, Clone)]
#[repr(transparent)]
pub struct ObjectSchema(pub HashMap<Documented<ObjectKey>, Schema>);

/// Sequence schema: @seq(@Schema).
/// All elements must match the inner schema.
#[derive(Facet, Debug, Clone)]
#[repr(transparent)]
pub struct SeqSchema(pub (Documented<Box<Schema>>,));

/// Map schema: @map(@V) or @map(@K @V).
/// Vec contains 1 element (value type, key defaults to @string) or 2 elements (key, value).
#[derive(Facet, Debug, Clone)]
#[repr(transparent)]
pub struct MapSchema(pub Vec<Documented<Schema>>);

// =============================================================================
// Combinator schema types
// =============================================================================

/// Union schema: @union(@A @B ...).
/// Value must match one of the listed types.
#[derive(Facet, Debug, Clone)]
#[repr(transparent)]
pub struct UnionSchema(pub Vec<Documented<Schema>>);

/// Optional schema: @optional(@T).
/// Field can be absent or match the inner type.
#[derive(Facet, Debug, Clone)]
#[repr(transparent)]
pub struct OptionalSchema(pub (Documented<Box<Schema>>,));

/// Enum schema: @enum{variant @Type, variant @object{...}}.
/// Maps variant names to their payload schemas.
#[derive(Facet, Debug, Clone)]
#[repr(transparent)]
pub struct EnumSchema(pub HashMap<Documented<String>, Schema>);

/// One-of schema: @one-of(@type value1 value2 ...).
/// Constrains values to a finite set. Tuple is (base_type, allowed_values).
#[derive(Facet, Debug, Clone)]
#[repr(transparent)]
pub struct OneOfSchema(pub (Documented<Box<Schema>>, Vec<RawStyx>));

/// Flatten schema: @flatten(@Type).
/// Inlines fields from another type into the containing object.
#[derive(Facet, Debug, Clone)]
#[repr(transparent)]
pub struct FlattenSchema(pub (Documented<Box<Schema>>,));

// =============================================================================
// Wrapper schema types
// =============================================================================

/// A raw Styx value that serializes without quotes and deserializes any value as a string.
///
/// Used for embedding Styx expressions in schemas (e.g., default values).
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct RawStyx(pub String);

impl RawStyx {
    pub fn new(s: impl Into<String>) -> Self {
        RawStyx(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RawStyx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Default value wrapper: @default(value @type).
/// If the field is missing, use the default value.
/// Tuple is (default_value, inner_schema).
#[derive(Facet, Debug, Clone)]
#[repr(transparent)]
pub struct DefaultSchema(pub (RawStyx, Documented<Box<Schema>>));

/// Deprecated wrapper: @deprecated("reason" @type).
/// Marks a field as deprecated; validation warns but doesn't fail.
/// Tuple is (reason, inner_schema).
#[derive(Facet, Debug, Clone)]
#[repr(transparent)]
pub struct DeprecatedSchema(pub (String, Documented<Box<Schema>>));

// =============================================================================
// Metadata container types
// =============================================================================

/// A value with documentation metadata.
///
/// This is a metadata container - it serializes transparently as just the value,
/// but formats that support metadata (like Styx) can emit the doc comments.
///
/// # Example
///
/// ```ignore
/// let config = Config {
///     port: Documented {
///         value: 8080,
///         doc: Some(vec!["The port to listen on".into()]),
///     },
/// };
///
/// // JSON (no metadata support): {"port": 8080}
/// // Styx (with metadata support):
/// // /// The port to listen on
/// // port 8080
/// ```
#[derive(Facet, Debug, Clone)]
#[facet(metadata_container)]
pub struct Documented<T> {
    /// The actual value.
    pub value: T,
    /// Documentation lines (each line is a separate string).
    #[facet(metadata = "doc")]
    pub doc: Option<Vec<String>>,
}

impl<T> Documented<T> {
    /// Create a new documented value without any documentation.
    pub fn new(value: T) -> Self {
        Self { value, doc: None }
    }

    /// Create a new documented value with documentation.
    pub fn with_doc(value: T, doc: Vec<String>) -> Self {
        Self {
            value,
            doc: Some(doc),
        }
    }

    /// Create a new documented value with a single line of documentation.
    pub fn with_doc_line(value: T, line: impl Into<String>) -> Self {
        Self {
            value,
            doc: Some(vec![line.into()]),
        }
    }

    /// Get a reference to the inner value.
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Get a mutable reference to the inner value.
    pub fn value_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Unwrap into the inner value, discarding documentation.
    pub fn into_inner(self) -> T {
        self.value
    }

    /// Get the documentation lines, if any.
    pub fn doc(&self) -> Option<&[String]> {
        self.doc.as_deref()
    }

    /// Map the inner value to a new type.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Documented<U> {
        Documented {
            value: f(self.value),
            doc: self.doc,
        }
    }
}

impl<T: Default> Default for Documented<T> {
    fn default() -> Self {
        Self {
            value: T::default(),
            doc: None,
        }
    }
}

impl<T> std::ops::Deref for Documented<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> std::ops::DerefMut for Documented<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl<T> From<T> for Documented<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

// Hash and Eq only consider the value, not the documentation.
// Documentation is metadata and doesn't affect identity.

impl<T: std::hash::Hash> std::hash::Hash for Documented<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<T: PartialEq> PartialEq for Documented<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: Eq> Eq for Documented<T> {}
