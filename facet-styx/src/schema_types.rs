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

/// Object schema: @object{field @Schema, @ @Schema}.
/// Maps field names to their type constraints.
/// The key `None` represents additional fields (catch-all `@`).
#[derive(Facet, Debug, Clone)]
#[repr(transparent)]
pub struct ObjectSchema(pub HashMap<Option<String>, Schema>);

/// Sequence schema: @seq(@Schema).
/// All elements must match the inner schema.
#[derive(Facet, Debug, Clone)]
#[repr(transparent)]
pub struct SeqSchema(pub (Box<Schema>,));

/// Map schema: @map(@V) or @map(@K @V).
/// Vec contains 1 element (value type, key defaults to @string) or 2 elements (key, value).
#[derive(Facet, Debug, Clone)]
#[repr(transparent)]
pub struct MapSchema(pub Vec<Schema>);

// =============================================================================
// Combinator schema types
// =============================================================================

/// Union schema: @union(@A @B ...).
/// Value must match one of the listed types.
#[derive(Facet, Debug, Clone)]
#[repr(transparent)]
pub struct UnionSchema(pub Vec<Schema>);

/// Optional schema: @optional(@T).
/// Field can be absent or match the inner type.
#[derive(Facet, Debug, Clone)]
#[repr(transparent)]
pub struct OptionalSchema(pub (Box<Schema>,));

/// Enum schema: @enum{variant @Type, variant @object{...}}.
/// Maps variant names to their payload schemas.
#[derive(Facet, Debug, Clone)]
#[repr(transparent)]
pub struct EnumSchema(pub HashMap<String, Schema>);

/// Flatten schema: @flatten(@Type).
/// Inlines fields from another type into the containing object.
#[derive(Facet, Debug, Clone)]
#[repr(transparent)]
pub struct FlattenSchema(pub (Box<Schema>,));

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
pub struct DefaultSchema(pub (RawStyx, Box<Schema>));

/// Deprecated wrapper: @deprecated("reason" @type).
/// Marks a field as deprecated; validation warns but doesn't fail.
/// Tuple is (reason, inner_schema).
#[derive(Facet, Debug, Clone)]
#[repr(transparent)]
pub struct DeprecatedSchema(pub (String, Box<Schema>));
