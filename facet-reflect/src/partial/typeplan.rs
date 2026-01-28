//! TypePlan: Precomputed deserialization plans for types.
//!
//! Instead of repeatedly inspecting Shape/Def at runtime during deserialization,
//! we build a plan tree once that encodes all the decisions we'll make.
//!
//! Uses indextree for arena-based allocation, which naturally handles recursive
//! types by storing NodeId back-references instead of causing infinite recursion.

use alloc::vec::Vec;
use facet_core::{
    Characteristic, ConstTypeId, Def, DefaultInPlaceFn, DefaultSource, EnumType, Field, ProxyDef,
    ScalarType, SequenceType, Shape, StructType, Type, UserType, ValidatorFn, Variant,
};
use hashbrown::HashMap;
use indextree::Arena;

use crate::AllocError;

// Re-export NodeId for use by other modules
pub use indextree::NodeId;

/// Precomputed deserialization plan tree for a type.
///
/// Built once from a Shape, this encodes all decisions needed during deserialization
/// without repeated runtime lookups. Uses arena allocation to handle recursive types.
#[derive(Debug)]
pub struct TypePlan {
    /// Arena that owns all plan nodes
    arena: Arena<TypePlanNode>,
    /// Root node of the plan tree
    root: NodeId,
}

/// A node in the TypePlan tree.
#[derive(Debug)]
pub struct TypePlanNode {
    /// The shape this node was built from
    pub shape: &'static Shape,
    /// What kind of type this is and how to deserialize it
    pub kind: TypePlanNodeKind,
    /// Precomputed deserialization strategy - tells facet-format exactly what to do
    pub strategy: DeserStrategy,
    /// Whether this type has a Default implementation
    pub has_default: bool,
    /// Precomputed proxy for this shape (format-specific or generic)
    pub proxy: Option<&'static ProxyDef>,
    /// Precomputed field initialization plans - one entry per field.
    /// Every field is either Required or Defaultable.
    /// For structs: populated with all fields.
    /// For enums: empty (use `EnumPlan.variants[idx].field_init_plans` instead).
    /// For other types: empty.
    pub field_init_plans: Vec<FieldInitPlan>,
}

/// Precomputed deserialization strategy with all data needed to execute it.
///
/// This is denormalized: we store NodeIds, proxy defs, etc. directly so the
/// deserializer can follow the plan without chasing pointers through Shape/vtable.
#[derive(Debug, Clone)]
pub enum DeserStrategy {
    /// Container-level proxy: the type itself has `#[facet(proxy = X)]`
    ContainerProxy {
        /// The proxy definition containing conversion functions
        proxy_def: &'static ProxyDef,
        /// The shape of the proxy type (what we deserialize)
        proxy_shape: &'static Shape,
        /// NodeId of the child node representing the proxy type's structure
        proxy_node: NodeId,
    },
    /// Field-level proxy: the field has `#[facet(proxy = X)]` but the type doesn't
    FieldProxy {
        /// The proxy definition containing conversion functions
        proxy_def: &'static ProxyDef,
        /// The shape of the proxy type (what we deserialize)
        proxy_shape: &'static Shape,
        /// NodeId of the child node representing the proxy type's structure
        proxy_node: NodeId,
    },
    /// Smart pointer (Box, Arc, Rc) with known pointee type
    Pointer {
        /// NodeId of the pointee type's plan
        pointee_node: NodeId,
    },
    /// Opaque smart pointer (`#[facet(opaque)]`) - cannot be deserialized, only set wholesale
    OpaquePointer,
    /// Opaque type (`Opaque<T>`) - cannot be deserialized, only set wholesale via proxy
    Opaque,
    /// Transparent wrapper with try_from (like NonZero)
    TransparentConvert {
        /// NodeId of the inner type's plan
        inner_node: NodeId,
    },
    /// Scalar with FromStr
    Scalar {
        /// Precomputed scalar type for fast hint dispatch.
        /// None for opaque scalars that need parser-specific handling.
        scalar_type: Option<ScalarType>,
        /// Whether this scalar type implements FromStr (for string parsing fallback)
        is_from_str: bool,
    },
    /// Named struct
    Struct,
    /// Tuple or tuple struct
    Tuple {
        /// Number of fields in the tuple
        field_count: usize,
        /// Whether this is a single-field transparent wrapper that can accept values directly
        is_single_field_transparent: bool,
    },
    /// Enum
    Enum,
    /// `Option<T>`
    Option {
        /// NodeId of the Some variant's inner type plan
        some_node: NodeId,
    },
    /// `Result<T, E>`
    Result {
        /// NodeId of the Ok variant's type plan
        ok_node: NodeId,
        /// NodeId of the Err variant's type plan
        err_node: NodeId,
    },
    /// List (Vec, VecDeque, etc.)
    List {
        /// NodeId of the item type's plan
        item_node: NodeId,
        /// Whether this is specifically `Vec<u8>` (for optimized byte sequence handling)
        is_byte_vec: bool,
    },
    /// Map (HashMap, BTreeMap, etc.)
    Map {
        /// NodeId of the key type's plan
        key_node: NodeId,
        /// NodeId of the value type's plan
        value_node: NodeId,
    },
    /// Set (HashSet, BTreeSet, etc.)
    Set {
        /// NodeId of the item type's plan
        item_node: NodeId,
    },
    /// Fixed-size array [T; N]
    Array {
        /// Array length
        len: usize,
        /// NodeId of the item type's plan
        item_node: NodeId,
    },
    /// DynamicValue (like `facet_value::Value`)
    DynamicValue,
    /// Metadata container (like `Spanned<T>`, `Documented<T>`)
    /// These require special field-by-field handling for metadata population
    MetadataContainer,
    /// BackRef to recursive type - deser_strategy() resolves this
    BackRef {
        /// NodeId of the target node this backref points to
        target: NodeId,
    },
}

/// The specific kind of type and its deserialization strategy.
///
/// Children are stored via indextree's parent-child relationships, not inline.
/// Use the TypePlan methods to navigate to children.
#[derive(Debug)]
pub enum TypePlanNodeKind {
    /// Scalar types (integers, floats, bool, char, strings)
    Scalar,

    /// Struct types with named or positional fields
    Struct(StructPlan),

    /// Enum types with variants
    Enum(EnumPlan),

    /// `Option<T>` - special handling for None/Some
    /// Child: inner type T
    Option,

    /// `Result<T, E>` - special handling for Ok/Err
    /// Children: [ok type T, err type E]
    Result,

    /// `Vec<T>`, `VecDeque<T>`, etc.
    /// Child: item type T
    List,

    /// Slice types `[T]` (unsized, used via smart pointers like `Arc<[T]>`)
    /// Child: item type T
    Slice,

    /// `HashMap<K, V>`, `BTreeMap<K, V>`, etc.
    /// Children: [key type K, value type V]
    Map,

    /// `HashSet<T>`, `BTreeSet<T>`, etc.
    /// Child: item type T
    Set,

    /// Fixed-size arrays `[T; N]`
    /// Child: item type T
    Array {
        /// Array length N
        len: usize,
    },

    /// Smart pointers: `Box<T>`, `Arc<T>`, `Rc<T>`
    /// Child: pointee type T
    Pointer,

    /// Opaque smart pointers (`#[facet(opaque)]`)
    /// No child - the pointee type is unknown/opaque
    OpaquePointer,

    /// Opaque types (`Opaque<T>`) - can only be set wholesale, not deserialized
    Opaque,

    /// DynamicValue (like `serde_json::Value`)
    DynamicValue,

    /// Transparent wrappers (newtypes)
    /// Child: inner type
    Transparent,

    /// Back-reference to an ancestor node (for recursive types)
    BackRef(NodeId),
}

/// Precomputed plan for struct deserialization.
#[derive(Debug)]
pub struct StructPlan {
    /// Reference to the struct type definition
    pub struct_def: &'static StructType,
    /// Plans for each field, indexed by field position
    /// (child NodeIds are stored in indextree, these are metadata only)
    pub fields: Vec<FieldPlanMeta>,
    /// Fast field lookup by name
    pub field_lookup: FieldLookup,
    /// Whether any field has #[facet(flatten)]
    pub has_flatten: bool,
    /// Whether to reject unknown fields (precomputed from `#[facet(deny_unknown_fields)]`)
    pub deny_unknown_fields: bool,
}

/// Precomputed plan for initializing/validating a single field.
///
/// This combines what was previously separate logic in `fill_defaults` and
/// `require_full_initialization` into a single data structure that can be
/// processed in one pass.
#[derive(Debug, Clone)]
pub struct FieldInitPlan {
    /// Field index in the struct (for ISet tracking)
    pub index: usize,
    /// Field offset in bytes from struct base (for calculating field pointer)
    pub offset: usize,
    /// Field name (for error messages)
    pub name: &'static str,
    /// The field's type shape (for reading values during validation)
    pub field_shape: &'static Shape,
    /// How to handle this field if not set during deserialization
    pub fill_rule: FillRule,
    /// Validators to run after the field is set (precomputed from attributes)
    pub validators: Vec<PrecomputedValidator>,
}

/// How to fill a field that wasn't set during deserialization.
#[derive(Debug, Clone)]
pub enum FillRule {
    /// Field has a default - call this function if not set.
    /// The function writes the default value to an uninitialized pointer.
    Defaultable(FieldDefault),
    /// Field is required - error if not set.
    Required,
}

/// Source of default value for a field.
#[derive(Debug, Clone, Copy)]
pub enum FieldDefault {
    /// Use a custom default function (from `#[facet(default = expr)]`)
    Custom(DefaultInPlaceFn),
    /// Use the type's Default trait (via shape.call_default_in_place)
    /// We store the shape so we can call its default_in_place
    FromTrait(&'static Shape),
}

/// A precomputed validator extracted from field attributes.
#[derive(Debug, Clone)]
pub struct PrecomputedValidator {
    /// The validator kind with any associated data
    pub kind: ValidatorKind,
}

impl PrecomputedValidator {
    /// Run this validator on an initialized field value.
    ///
    /// # Safety
    /// `field_ptr` must point to initialized memory of the type specified by the validator's scalar_type.
    #[allow(unsafe_code)]
    pub fn run(
        &self,
        field_ptr: facet_core::PtrConst,
        field_name: &'static str,
        container_shape: &'static Shape,
    ) -> Result<(), crate::ReflectErrorKind> {
        use crate::ReflectErrorKind;
        use alloc::format;

        let result: Result<(), alloc::string::String> = match self.kind {
            ValidatorKind::Custom(validator_fn) => {
                // SAFETY: caller guarantees field_ptr points to valid data
                unsafe { validator_fn(field_ptr) }
            }
            ValidatorKind::Min { limit, scalar_type } => {
                Self::validate_min(field_ptr, limit, scalar_type)
            }
            ValidatorKind::Max { limit, scalar_type } => {
                Self::validate_max(field_ptr, limit, scalar_type)
            }
            ValidatorKind::MinLength { limit, scalar_type } => {
                let len = Self::get_string_length(field_ptr, scalar_type);
                if len < limit {
                    Err(format!("length must be >= {}, got {}", limit, len))
                } else {
                    Ok(())
                }
            }
            ValidatorKind::MaxLength { limit, scalar_type } => {
                let len = Self::get_string_length(field_ptr, scalar_type);
                if len > limit {
                    Err(format!("length must be <= {}, got {}", limit, len))
                } else {
                    Ok(())
                }
            }
            ValidatorKind::Email { scalar_type } => {
                let s = unsafe { Self::get_string(field_ptr, scalar_type) };
                if Self::is_valid_email(s) {
                    Ok(())
                } else {
                    Err(format!("'{}' is not a valid email address", s))
                }
            }
            ValidatorKind::Url { scalar_type } => {
                let s = unsafe { Self::get_string(field_ptr, scalar_type) };
                if Self::is_valid_url(s) {
                    Ok(())
                } else {
                    Err(format!("'{}' is not a valid URL", s))
                }
            }
            ValidatorKind::Regex {
                pattern,
                scalar_type,
            } => {
                let s = unsafe { Self::get_string(field_ptr, scalar_type) };
                if Self::matches_pattern(s, pattern) {
                    Ok(())
                } else {
                    Err(format!("'{}' does not match pattern '{}'", s, pattern))
                }
            }
            ValidatorKind::Contains {
                needle,
                scalar_type,
            } => {
                let s = unsafe { Self::get_string(field_ptr, scalar_type) };
                if s.contains(needle) {
                    Ok(())
                } else {
                    Err(format!("'{}' does not contain '{}'", s, needle))
                }
            }
        };

        result.map_err(|message| ReflectErrorKind::ValidationFailed {
            shape: container_shape,
            field_name,
            message,
        })
    }

    #[allow(unsafe_code)]
    fn validate_min(
        field_ptr: facet_core::PtrConst,
        limit: i64,
        scalar_type: ScalarType,
    ) -> Result<(), alloc::string::String> {
        use alloc::format;
        match scalar_type {
            ScalarType::I8 => {
                let v = unsafe { *field_ptr.get::<i8>() } as i64;
                if v < limit {
                    Err(format!("must be >= {}, got {}", limit, v))
                } else {
                    Ok(())
                }
            }
            ScalarType::I16 => {
                let v = unsafe { *field_ptr.get::<i16>() } as i64;
                if v < limit {
                    Err(format!("must be >= {}, got {}", limit, v))
                } else {
                    Ok(())
                }
            }
            ScalarType::I32 => {
                let v = unsafe { *field_ptr.get::<i32>() } as i64;
                if v < limit {
                    Err(format!("must be >= {}, got {}", limit, v))
                } else {
                    Ok(())
                }
            }
            ScalarType::I64 => {
                let v = unsafe { *field_ptr.get::<i64>() };
                if v < limit {
                    Err(format!("must be >= {}, got {}", limit, v))
                } else {
                    Ok(())
                }
            }
            ScalarType::U8 => {
                let v = unsafe { *field_ptr.get::<u8>() } as i64;
                if v < limit {
                    Err(format!("must be >= {}, got {}", limit, v))
                } else {
                    Ok(())
                }
            }
            ScalarType::U16 => {
                let v = unsafe { *field_ptr.get::<u16>() } as i64;
                if v < limit {
                    Err(format!("must be >= {}, got {}", limit, v))
                } else {
                    Ok(())
                }
            }
            ScalarType::U32 => {
                let v = unsafe { *field_ptr.get::<u32>() } as i64;
                if v < limit {
                    Err(format!("must be >= {}, got {}", limit, v))
                } else {
                    Ok(())
                }
            }
            ScalarType::U64 => {
                let v = unsafe { *field_ptr.get::<u64>() };
                if v > i64::MAX as u64 {
                    Ok(()) // Value too large to compare as i64, assume valid for min
                } else if (v as i64) < limit {
                    Err(format!("must be >= {}, got {}", limit, v))
                } else {
                    Ok(())
                }
            }
            _ => Ok(()), // Non-numeric type - should not happen if TypePlan is built correctly
        }
    }

    #[allow(unsafe_code)]
    fn validate_max(
        field_ptr: facet_core::PtrConst,
        limit: i64,
        scalar_type: ScalarType,
    ) -> Result<(), alloc::string::String> {
        use alloc::format;
        match scalar_type {
            ScalarType::I8 => {
                let v = unsafe { *field_ptr.get::<i8>() } as i64;
                if v > limit {
                    Err(format!("must be <= {}, got {}", limit, v))
                } else {
                    Ok(())
                }
            }
            ScalarType::I16 => {
                let v = unsafe { *field_ptr.get::<i16>() } as i64;
                if v > limit {
                    Err(format!("must be <= {}, got {}", limit, v))
                } else {
                    Ok(())
                }
            }
            ScalarType::I32 => {
                let v = unsafe { *field_ptr.get::<i32>() } as i64;
                if v > limit {
                    Err(format!("must be <= {}, got {}", limit, v))
                } else {
                    Ok(())
                }
            }
            ScalarType::I64 => {
                let v = unsafe { *field_ptr.get::<i64>() };
                if v > limit {
                    Err(format!("must be <= {}, got {}", limit, v))
                } else {
                    Ok(())
                }
            }
            ScalarType::U8 => {
                let v = unsafe { *field_ptr.get::<u8>() } as i64;
                if v > limit {
                    Err(format!("must be <= {}, got {}", limit, v))
                } else {
                    Ok(())
                }
            }
            ScalarType::U16 => {
                let v = unsafe { *field_ptr.get::<u16>() } as i64;
                if v > limit {
                    Err(format!("must be <= {}, got {}", limit, v))
                } else {
                    Ok(())
                }
            }
            ScalarType::U32 => {
                let v = unsafe { *field_ptr.get::<u32>() } as i64;
                if v > limit {
                    Err(format!("must be <= {}, got {}", limit, v))
                } else {
                    Ok(())
                }
            }
            ScalarType::U64 => {
                let v = unsafe { *field_ptr.get::<u64>() };
                // Check if v exceeds limit: either v > i64::MAX (always fails for positive limit)
                // or v fits in i64 and exceeds limit
                if v > i64::MAX as u64 || (v as i64) > limit {
                    Err(format!("must be <= {}, got {}", limit, v))
                } else {
                    Ok(())
                }
            }
            _ => Ok(()), // Non-numeric type - should not happen if TypePlan is built correctly
        }
    }

    /// Get string from field pointer using precomputed scalar type.
    ///
    /// # Safety
    /// The field_ptr must point to valid, initialized memory of the type indicated by scalar_type.
    /// The returned reference is valid as long as the underlying memory is valid.
    #[allow(unsafe_code)]
    unsafe fn get_string<'a>(field_ptr: facet_core::PtrConst, scalar_type: ScalarType) -> &'a str {
        match scalar_type {
            ScalarType::String => {
                let s: &alloc::string::String = unsafe { field_ptr.get() };
                s.as_str()
            }
            ScalarType::Str => {
                let s: &&str = unsafe { field_ptr.get() };
                s
            }
            ScalarType::CowStr => {
                let s: &alloc::borrow::Cow<'_, str> = unsafe { field_ptr.get() };
                s.as_ref()
            }
            _ => "", // Should not happen if TypePlan is built correctly
        }
    }

    /// Get length of string field using precomputed scalar type.
    #[allow(unsafe_code)]
    fn get_string_length(field_ptr: facet_core::PtrConst, scalar_type: ScalarType) -> usize {
        unsafe { Self::get_string(field_ptr, scalar_type) }.len()
    }

    /// Simple email validation (no regex dependency).
    fn is_valid_email(s: &str) -> bool {
        // Basic check: has exactly one @, something before and after, and a dot after @
        let at_pos = s.find('@');
        if let Some(at) = at_pos {
            if at == 0 || at == s.len() - 1 {
                return false;
            }
            let domain = &s[at + 1..];
            domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
        } else {
            false
        }
    }

    /// Simple URL validation (no regex dependency).
    fn is_valid_url(s: &str) -> bool {
        s.starts_with("http://") || s.starts_with("https://")
    }

    /// Pattern matching - requires regex feature.
    #[cfg(feature = "regex")]
    fn matches_pattern(s: &str, pattern: &str) -> bool {
        regex::Regex::new(pattern)
            .map(|re| re.is_match(s))
            .unwrap_or(false)
    }

    #[cfg(not(feature = "regex"))]
    fn matches_pattern(_s: &str, _pattern: &str) -> bool {
        // Without regex feature, pattern matching always fails
        // This should be caught at compile time by not enabling the feature
        false
    }
}

/// Kinds of validators with their precomputed data.
///
/// Each variant stores not just the constraint but also the `ScalarType` needed
/// to read the value. This is determined at TypePlan build time, eliminating
/// runtime type detection during validation.
#[derive(Debug, Clone, Copy)]
pub enum ValidatorKind {
    /// Custom validator function
    Custom(ValidatorFn),
    /// Minimum value (for numeric types)
    Min {
        /// The minimum allowed value
        limit: i64,
        /// How to read the value from memory
        scalar_type: ScalarType,
    },
    /// Maximum value (for numeric types)
    Max {
        /// The maximum allowed value
        limit: i64,
        /// How to read the value from memory
        scalar_type: ScalarType,
    },
    /// Minimum length (for strings)
    MinLength {
        /// The minimum allowed length
        limit: usize,
        /// How to read the string from memory
        scalar_type: ScalarType,
    },
    /// Maximum length (for strings)
    MaxLength {
        /// The maximum allowed length
        limit: usize,
        /// How to read the string from memory
        scalar_type: ScalarType,
    },
    /// Must be valid email
    Email {
        /// How to read the string from memory
        scalar_type: ScalarType,
    },
    /// Must be valid URL
    Url {
        /// How to read the string from memory
        scalar_type: ScalarType,
    },
    /// Must match regex pattern
    Regex {
        /// The regex pattern to match
        pattern: &'static str,
        /// How to read the string from memory
        scalar_type: ScalarType,
    },
    /// Must contain substring
    Contains {
        /// The substring to search for
        needle: &'static str,
        /// How to read the string from memory
        scalar_type: ScalarType,
    },
}

/// Metadata for a single field (without child plan - that's in the tree).
#[derive(Debug, Clone)]
pub struct FieldPlanMeta {
    /// Reference to the field definition
    pub field: &'static Field,
    /// Field name (for path tracking in deferred mode)
    pub name: &'static str,
    /// The name to match in input (considers rename)
    pub effective_name: &'static str,
    /// Alias if any
    pub alias: Option<&'static str>,
    /// Whether this field has a default value
    pub has_default: bool,
    /// Whether this field is required (no default, not Option)
    pub is_required: bool,
    /// Whether this field is flattened
    pub is_flattened: bool,
    /// NodeId of this field's type plan in the arena
    pub type_node: NodeId,
}

/// Precomputed plan for enum deserialization.
#[derive(Debug)]
pub struct EnumPlan {
    /// Reference to the enum type definition
    pub enum_def: &'static EnumType,
    /// Plans for each variant (metadata only, child NodeIds in tree)
    pub variants: Vec<VariantPlanMeta>,
    /// Fast variant lookup by name
    pub variant_lookup: VariantLookup,
    /// Number of variants
    pub num_variants: usize,
    /// Index of the `#[facet(other)]` variant, if any
    pub other_variant_idx: Option<usize>,
}

/// Metadata for a single enum variant.
#[derive(Debug, Clone)]
pub struct VariantPlanMeta {
    /// Reference to the variant definition
    pub variant: &'static Variant,
    /// Variant name
    pub name: &'static str,
    /// Field metadata for this variant
    pub fields: Vec<FieldPlanMeta>,
    /// Fast field lookup for this variant
    pub field_lookup: FieldLookup,
    /// Whether any field in this variant has #[facet(flatten)]
    pub has_flatten: bool,
    /// Precomputed initialization plans for variant fields - one per field
    pub field_init_plans: Vec<FieldInitPlan>,
}

/// Fast lookup from field name to field index.
///
/// Uses different strategies based on field count:
/// - Small (≤8 fields): linear scan (cache-friendly, no hashing overhead)
/// - Large (>8 fields): prefix-based dispatch (like JIT) - group by first N bytes
#[derive(Debug, Clone)]
pub enum FieldLookup {
    /// For small structs: just store (name, index) pairs
    Small(Vec<FieldLookupEntry>),
    /// For larger structs: prefix-based buckets
    /// Entries are grouped by their N-byte prefix, buckets sorted by prefix for binary search
    PrefixBuckets {
        /// Prefix length in bytes (4 or 8)
        prefix_len: usize,
        /// All entries, grouped by prefix
        entries: Vec<FieldLookupEntry>,
        /// (prefix, start_index, count) sorted by prefix
        buckets: Vec<(u64, u32, u32)>,
    },
}

/// An entry in the field lookup table.
#[derive(Debug, Clone)]
pub struct FieldLookupEntry {
    /// The name to match (effective_name or alias)
    pub name: &'static str,
    /// The field index
    pub index: usize,
    /// Whether this is an alias (vs primary name)
    pub is_alias: bool,
}

/// Fast lookup from variant name to variant index.
#[derive(Debug, Clone)]
pub enum VariantLookup {
    /// For small enums: linear scan
    Small(Vec<(&'static str, usize)>),
    /// For larger enums: sorted for binary search
    Sorted(Vec<(&'static str, usize)>),
}

// Threshold for switching from linear to prefix-based lookup
const LOOKUP_THRESHOLD: usize = 8;

/// Compute prefix from field name (first N bytes as little-endian u64).
/// Matches JIT's compute_field_prefix.
#[inline]
fn compute_prefix(name: &str, prefix_len: usize) -> u64 {
    let bytes = name.as_bytes();
    let actual_len = bytes.len().min(prefix_len);
    let mut prefix: u64 = 0;
    for (i, &byte) in bytes.iter().take(actual_len).enumerate() {
        prefix |= (byte as u64) << (i * 8);
    }
    prefix
}

impl FieldLookup {
    /// Create a new field lookup from field metadata.
    pub fn new(fields: &[FieldPlanMeta]) -> Self {
        let mut entries = Vec::with_capacity(fields.len() * 2); // room for aliases

        for (index, field_meta) in fields.iter().enumerate() {
            // Add primary name
            entries.push(FieldLookupEntry {
                name: field_meta.effective_name,
                index,
                is_alias: false,
            });

            // Add alias if present
            if let Some(alias) = field_meta.alias {
                entries.push(FieldLookupEntry {
                    name: alias,
                    index,
                    is_alias: true,
                });
            }
        }

        Self::from_entries(entries)
    }

    /// Create a field lookup directly from a struct type definition.
    ///
    /// This is a lightweight alternative to building a full `StructPlan` when
    /// only field lookup is needed - it doesn't recursively build TypePlans
    /// for child fields.
    pub fn from_struct_type(struct_def: &'static StructType) -> Self {
        let mut entries = Vec::with_capacity(struct_def.fields.len() * 2);

        for (index, field) in struct_def.fields.iter().enumerate() {
            // Add primary name (effective_name considers rename)
            entries.push(FieldLookupEntry {
                name: field.effective_name(),
                index,
                is_alias: false,
            });

            // Add alias if present
            if let Some(alias) = field.alias {
                entries.push(FieldLookupEntry {
                    name: alias,
                    index,
                    is_alias: true,
                });
            }
        }

        Self::from_entries(entries)
    }

    /// Build lookup structure from entries.
    fn from_entries(entries: Vec<FieldLookupEntry>) -> Self {
        if entries.len() <= LOOKUP_THRESHOLD {
            return FieldLookup::Small(entries);
        }

        // Choose prefix length: 8 bytes if most keys are long, otherwise 4
        // Short keys get zero-padded, which is fine - they'll have unique prefixes
        let long_key_count = entries.iter().filter(|e| e.name.len() >= 8).count();
        let prefix_len = if long_key_count > entries.len() / 2 {
            8
        } else {
            4
        };

        // Group entries by prefix
        let mut prefix_map: hashbrown::HashMap<u64, Vec<FieldLookupEntry>> =
            hashbrown::HashMap::new();
        for entry in entries {
            let prefix = compute_prefix(entry.name, prefix_len);
            prefix_map.entry(prefix).or_default().push(entry);
        }

        // Build sorted bucket list and flattened entries
        let mut bucket_list: Vec<_> = prefix_map.into_iter().collect();
        bucket_list.sort_by_key(|(prefix, _)| *prefix);

        let mut all_entries = Vec::new();
        let mut buckets = Vec::with_capacity(bucket_list.len());

        for (prefix, bucket_entries) in bucket_list {
            let start = all_entries.len() as u32;
            let count = bucket_entries.len() as u32;
            buckets.push((prefix, start, count));
            all_entries.extend(bucket_entries);
        }

        FieldLookup::PrefixBuckets {
            prefix_len,
            entries: all_entries,
            buckets,
        }
    }

    /// Find a field index by name.
    #[inline]
    pub fn find(&self, name: &str) -> Option<usize> {
        match self {
            FieldLookup::Small(entries) => entries.iter().find(|e| e.name == name).map(|e| e.index),
            FieldLookup::PrefixBuckets {
                prefix_len,
                entries,
                buckets,
            } => {
                let prefix = compute_prefix(name, *prefix_len);

                // Binary search for bucket
                let bucket_idx = buckets.binary_search_by_key(&prefix, |(p, _, _)| *p).ok()?;
                let (_, start, count) = buckets[bucket_idx];

                // Linear scan within bucket
                let bucket_entries = &entries[start as usize..(start + count) as usize];
                bucket_entries
                    .iter()
                    .find(|e| e.name == name)
                    .map(|e| e.index)
            }
        }
    }

    /// Check if empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        match self {
            FieldLookup::Small(entries) => entries.is_empty(),
            FieldLookup::PrefixBuckets { entries, .. } => entries.is_empty(),
        }
    }
}

impl VariantLookup {
    /// Create a new variant lookup from variant metadata.
    pub fn new(variants: &[VariantPlanMeta]) -> Self {
        let mut entries: Vec<_> = variants
            .iter()
            .enumerate()
            .map(|(i, v)| (v.name, i))
            .collect();

        if entries.len() <= LOOKUP_THRESHOLD {
            VariantLookup::Small(entries)
        } else {
            entries.sort_by_key(|(name, _)| *name);
            VariantLookup::Sorted(entries)
        }
    }

    /// Create a variant lookup directly from an enum type definition.
    ///
    /// This is a lightweight alternative to building a full `EnumPlan` when
    /// only variant lookup is needed.
    pub fn from_enum_type(enum_def: &'static EnumType) -> Self {
        let mut entries: Vec<_> = enum_def
            .variants
            .iter()
            .enumerate()
            .map(|(i, v)| (v.name, i))
            .collect();

        if entries.len() <= LOOKUP_THRESHOLD {
            VariantLookup::Small(entries)
        } else {
            entries.sort_by_key(|(name, _)| *name);
            VariantLookup::Sorted(entries)
        }
    }

    /// Find a variant index by name.
    #[inline]
    pub fn find(&self, name: &str) -> Option<usize> {
        match self {
            VariantLookup::Small(entries) => {
                entries.iter().find(|(n, _)| *n == name).map(|(_, i)| *i)
            }
            VariantLookup::Sorted(entries) => entries
                .binary_search_by_key(&name, |(n, _)| *n)
                .ok()
                .map(|i| entries[i].1),
        }
    }
}

/// Builder context for TypePlan construction.
struct TypePlanBuilder {
    arena: Arena<TypePlanNode>,
    /// Map from TypeId to NodeId for cycle detection
    /// If a type is in this map, we're currently building it (ancestor on call stack)
    building: HashMap<ConstTypeId, NodeId>,
    /// Format namespace for resolving format-specific proxies (e.g., "json", "xml")
    format_namespace: Option<&'static str>,
}

impl TypePlanBuilder {
    fn new(format_namespace: Option<&'static str>) -> Self {
        Self {
            arena: Arena::new(),
            building: HashMap::new(),
            format_namespace,
        }
    }

    /// Build a node for a shape, returning its NodeId.
    /// Uses the shape's own proxy if present (container-level proxy).
    fn build_node(&mut self, shape: &'static Shape) -> Result<NodeId, AllocError> {
        // No field-level proxy when building directly - container proxy will be detected
        // inside build_node_with_proxy from the shape itself
        self.build_node_with_proxy(shape, None)
    }

    /// Build a node for a shape with an optional explicit proxy override.
    /// Used for field-level proxies where the proxy is on the field, not the type.
    ///
    /// If `explicit_proxy` is Some, it overrides the shape's own proxy.
    /// If `explicit_proxy` is None, we check the shape's own proxy.
    ///
    /// The node stores:
    /// - `shape`: the original type (conversion target, for metadata)
    /// - `kind`: built from the original shape (not the proxy)
    /// - `proxy`: proxy definition (for conversion detection)
    ///
    /// If a proxy is present, a child node is built for the proxy type, and the
    /// strategy includes the child's NodeId so the deserializer can navigate to it.
    fn build_node_with_proxy(
        &mut self,
        shape: &'static Shape,
        field_proxy: Option<&'static ProxyDef>,
    ) -> Result<NodeId, AllocError> {
        let type_id = shape.id;

        // Get container-level proxy (from the type itself)
        let container_proxy = shape.effective_proxy(self.format_namespace);

        // Field-level proxy takes precedence
        let effective_proxy = field_proxy.or(container_proxy);

        // Check if we're already building this type (cycle detected)
        // Use the original type_id for cycle detection
        if let Some(&existing_id) = self.building.get(&type_id) {
            // Create a BackRef node pointing to the existing node
            let backref_node = TypePlanNode {
                shape, // Original shape (conversion target)
                kind: TypePlanNodeKind::BackRef(existing_id),
                strategy: DeserStrategy::BackRef {
                    target: existing_id,
                },
                has_default: shape.is(Characteristic::Default),
                proxy: effective_proxy,
                field_init_plans: Vec::new(),
            };
            return Ok(self.arena.new_node(backref_node));
        }

        // Create placeholder node first so children can reference it.
        // We use Scalar as a dummy strategy - it will be overwritten before we return.
        let placeholder = TypePlanNode {
            shape,                          // Original shape (conversion target)
            kind: TypePlanNodeKind::Scalar, // Placeholder, will be replaced
            strategy: DeserStrategy::Scalar {
                scalar_type: None,
                is_from_str: false,
            }, // Placeholder, will be replaced
            has_default: shape.is(Characteristic::Default),
            proxy: effective_proxy,
            field_init_plans: Vec::new(), // Placeholder, will be replaced for structs
        };
        let node_id = self.arena.new_node(placeholder);

        // Mark this type as being built
        self.building.insert(type_id, node_id);

        // If there's a proxy, build a child node for the proxy type FIRST.
        // This child represents what we actually deserialize.
        let proxy_node = if let Some(proxy_def) = effective_proxy {
            // Build a node for the proxy type itself (no proxy inheritance)
            let proxy_shape = proxy_def.shape;
            // Recursively build the proxy type's node - it will have its own kind/strategy
            let proxy_child_id = self.build_node(proxy_shape)?;
            node_id.append(proxy_child_id, &mut self.arena);
            Some(proxy_child_id)
        } else {
            None
        };

        // Build the kind for the original shape (not proxy) - this is used for
        // non-proxy operations on the type
        let (kind, field_init_plans) = self.build_kind(shape, node_id)?;

        // Compute the deserialization strategy
        let strategy = self.compute_strategy(
            shape,
            &kind,
            container_proxy,
            field_proxy,
            proxy_node,
            node_id,
        )?;

        // Update the node with the real kind, strategy, and field init plans
        let node = self.arena.get_mut(node_id).unwrap().get_mut();
        node.kind = kind;
        node.strategy = strategy;
        node.field_init_plans = field_init_plans;

        // Remove from building set - we're done with this type
        // This ensures only ancestors are tracked, not all visited types
        self.building.remove(&type_id);

        Ok(node_id)
    }

    /// Compute the deserialization strategy with all data needed to execute it.
    ///
    /// `proxy_node` is the NodeId of the child node for the proxy type (if any).
    /// `parent_id` is used to find child NodeIds that were built by `build_kind`.
    fn compute_strategy(
        &self,
        shape: &'static Shape,
        kind: &TypePlanNodeKind,
        proxy: Option<&'static ProxyDef>,
        explicit_field_proxy: Option<&'static ProxyDef>,
        proxy_node: Option<NodeId>,
        parent_id: NodeId,
    ) -> Result<DeserStrategy, AllocError> {
        // Helper to get nth child (skipping proxy node if present)
        let child_offset = if proxy_node.is_some() { 1 } else { 0 };
        let nth_child = |n: usize| -> NodeId {
            parent_id
                .children(&self.arena)
                .nth(n + child_offset)
                .expect("child should exist")
        };
        let first_child = || nth_child(0);

        // Priority 1: Field-level proxy (field has proxy, type doesn't)
        if let Some(field_proxy) = explicit_field_proxy {
            return Ok(DeserStrategy::FieldProxy {
                proxy_def: field_proxy,
                proxy_shape: field_proxy.shape,
                proxy_node: proxy_node.expect("field proxy requires proxy_node"),
            });
        }

        // Priority 2: Container-level proxy (type itself has proxy)
        if let Some(container_proxy) = proxy {
            return Ok(DeserStrategy::ContainerProxy {
                proxy_def: container_proxy,
                proxy_shape: container_proxy.shape,
                proxy_node: proxy_node.expect("container proxy requires proxy_node"),
            });
        }

        // Priority 3: Smart pointers (Box, Arc, Rc)
        if matches!(kind, TypePlanNodeKind::Pointer) {
            return Ok(DeserStrategy::Pointer {
                pointee_node: first_child(),
            });
        }

        // Priority 4: Metadata containers (like Spanned<T>, Documented<T>)
        // These require field-by-field handling for metadata population
        if shape.is_metadata_container() {
            return Ok(DeserStrategy::MetadataContainer);
        }

        // Priority 5: Types with .inner and try_from (like NonZero<T>)
        if shape.inner.is_some()
            && shape.vtable.has_try_from()
            && !matches!(
                &shape.def,
                Def::List(_) | Def::Map(_) | Def::Set(_) | Def::Array(_)
            )
        {
            return Ok(DeserStrategy::TransparentConvert {
                inner_node: first_child(),
            });
        }

        // Priority 6: Transparent wrappers with try_from
        if matches!(kind, TypePlanNodeKind::Transparent) && shape.vtable.has_try_from() {
            return Ok(DeserStrategy::TransparentConvert {
                inner_node: first_child(),
            });
        }

        // Priority 7: Scalars with FromStr
        if matches!(&shape.def, Def::Scalar) && shape.vtable.has_parse() {
            return Ok(DeserStrategy::Scalar {
                scalar_type: shape.scalar_type(),
                is_from_str: shape.vtable.has_parse(),
            });
        }

        // Priority 8: Match on the kind
        Ok(match kind {
            TypePlanNodeKind::Scalar => {
                // Empty tuple has def: Scalar but ty: Struct(Tuple)
                if let Type::User(UserType::Struct(struct_def)) = &shape.ty {
                    use facet_core::StructKind;
                    if matches!(struct_def.kind, StructKind::Tuple | StructKind::TupleStruct) {
                        let field_count = struct_def.fields.len();
                        return Ok(DeserStrategy::Tuple {
                            field_count,
                            is_single_field_transparent: field_count == 1 && shape.is_transparent(),
                        });
                    }
                }
                DeserStrategy::Scalar {
                    scalar_type: shape.scalar_type(),
                    is_from_str: shape.vtable.has_parse(),
                }
            }
            TypePlanNodeKind::Struct(struct_plan) => {
                use facet_core::StructKind;
                match struct_plan.struct_def.kind {
                    StructKind::Tuple | StructKind::TupleStruct => {
                        let field_count = struct_plan.struct_def.fields.len();
                        DeserStrategy::Tuple {
                            field_count,
                            is_single_field_transparent: field_count == 1 && shape.is_transparent(),
                        }
                    }
                    StructKind::Struct | StructKind::Unit => DeserStrategy::Struct,
                }
            }
            TypePlanNodeKind::Enum(_) => DeserStrategy::Enum,
            TypePlanNodeKind::Option => DeserStrategy::Option {
                some_node: first_child(),
            },
            TypePlanNodeKind::Result => DeserStrategy::Result {
                ok_node: nth_child(0),
                err_node: nth_child(1),
            },
            TypePlanNodeKind::List | TypePlanNodeKind::Slice => {
                // Check if this is Vec<u8> for optimized byte sequence handling
                let is_byte_vec = *shape == *<alloc::vec::Vec<u8> as facet_core::Facet>::SHAPE;
                DeserStrategy::List {
                    item_node: first_child(),
                    is_byte_vec,
                }
            }
            TypePlanNodeKind::Map => DeserStrategy::Map {
                key_node: nth_child(0),
                value_node: nth_child(1),
            },
            TypePlanNodeKind::Set => DeserStrategy::Set {
                item_node: first_child(),
            },
            TypePlanNodeKind::Array { len } => DeserStrategy::Array {
                len: *len,
                item_node: first_child(),
            },
            TypePlanNodeKind::DynamicValue => DeserStrategy::DynamicValue,
            TypePlanNodeKind::Pointer => DeserStrategy::Pointer {
                pointee_node: first_child(),
            },
            TypePlanNodeKind::OpaquePointer => DeserStrategy::OpaquePointer,
            TypePlanNodeKind::Opaque => DeserStrategy::Opaque,
            TypePlanNodeKind::Transparent => {
                // Transparent wrapper without try_from - unsupported
                return Err(AllocError {
                    shape,
                    operation: "transparent wrapper requires try_from for deserialization",
                });
            }
            TypePlanNodeKind::BackRef(target) => DeserStrategy::BackRef { target: *target },
        })
    }

    /// Build the TypePlanNodeKind for a shape, attaching children to parent_id.
    /// Returns (kind, field_init_plans) - plans is non-empty only for structs.
    fn build_kind(
        &mut self,
        shape: &'static Shape,
        parent_id: NodeId,
    ) -> Result<(TypePlanNodeKind, Vec<FieldInitPlan>), AllocError> {
        // Check shape.def first - this tells us the semantic meaning of the type
        let kind = match &shape.def {
            Def::Scalar => {
                // For scalar types with shape.inner (like NonZero<T>), build a child node
                // for the inner type. This enables proper TypePlan navigation when
                // begin_inner() is called for transparent wrapper deserialization.
                if let Some(inner_shape) = shape.inner {
                    let inner_id = self.build_node(inner_shape)?;
                    parent_id.append(inner_id, &mut self.arena);
                }
                TypePlanNodeKind::Scalar
            }

            Def::Option(opt_def) => {
                let inner_id = self.build_node(opt_def.t())?;
                parent_id.append(inner_id, &mut self.arena);
                TypePlanNodeKind::Option
            }

            Def::Result(res_def) => {
                let ok_id = self.build_node(res_def.t())?;
                let err_id = self.build_node(res_def.e())?;
                parent_id.append(ok_id, &mut self.arena);
                parent_id.append(err_id, &mut self.arena);
                TypePlanNodeKind::Result
            }

            Def::List(list_def) => {
                let item_id = self.build_node(list_def.t())?;
                parent_id.append(item_id, &mut self.arena);
                TypePlanNodeKind::List
            }

            Def::Map(map_def) => {
                let key_id = self.build_node(map_def.k())?;
                let value_id = self.build_node(map_def.v())?;
                parent_id.append(key_id, &mut self.arena);
                parent_id.append(value_id, &mut self.arena);
                TypePlanNodeKind::Map
            }

            Def::Set(set_def) => {
                let item_id = self.build_node(set_def.t())?;
                parent_id.append(item_id, &mut self.arena);
                TypePlanNodeKind::Set
            }

            Def::Array(arr_def) => {
                let item_id = self.build_node(arr_def.t())?;
                parent_id.append(item_id, &mut self.arena);
                TypePlanNodeKind::Array { len: arr_def.n }
            }

            Def::Pointer(ptr_def) => {
                if let Some(pointee) = ptr_def.pointee() {
                    let pointee_id = self.build_node(pointee)?;
                    parent_id.append(pointee_id, &mut self.arena);
                    TypePlanNodeKind::Pointer
                } else {
                    // Opaque pointer - no pointee shape available
                    TypePlanNodeKind::OpaquePointer
                }
            }

            Def::DynamicValue(_) => TypePlanNodeKind::DynamicValue,

            _ => {
                // Check Type for struct/enum/slice - these have Def::Undefined but meaningful ty
                match &shape.ty {
                    Type::User(UserType::Struct(struct_type)) => {
                        let (struct_plan, field_init_plans) =
                            self.build_struct_plan(shape, struct_type, parent_id)?;
                        return Ok((TypePlanNodeKind::Struct(struct_plan), field_init_plans));
                    }
                    Type::User(UserType::Enum(enum_type)) => {
                        TypePlanNodeKind::Enum(self.build_enum_plan(enum_type, parent_id)?)
                    }
                    // Handle slices like lists - they have an element type
                    Type::Sequence(SequenceType::Slice(slice_type)) => {
                        let item_id = self.build_node(slice_type.t)?;
                        parent_id.append(item_id, &mut self.arena);
                        // Use Slice kind so we can distinguish from List if needed
                        TypePlanNodeKind::Slice
                    }
                    // Opaque types have Def::Undefined AND ty that doesn't match above
                    Type::User(UserType::Opaque) | Type::Undefined => TypePlanNodeKind::Opaque,
                    _ => {
                        // Check for transparent wrappers (newtypes) as fallback
                        if let Some(inner) = shape.inner {
                            let inner_id = self.build_node(inner)?;
                            parent_id.append(inner_id, &mut self.arena);
                            TypePlanNodeKind::Transparent
                        } else {
                            return Err(AllocError {
                                shape,
                                operation: "unsupported type for deserialization",
                            });
                        }
                    }
                }
            }
        };
        // For non-struct types, return empty field_init_plans
        Ok((kind, Vec::new()))
    }

    /// Build a StructPlan, attaching field children to parent_id.
    fn build_struct_plan(
        &mut self,
        shape: &'static Shape,
        struct_def: &'static StructType,
        parent_id: NodeId,
    ) -> Result<(StructPlan, Vec<FieldInitPlan>), AllocError> {
        let mut fields = Vec::with_capacity(struct_def.fields.len());
        let mut field_init_plans = Vec::new();

        // Check if the container struct has #[facet(default)]
        let container_has_default = shape.is(Characteristic::Default);

        for (index, field) in struct_def.fields.iter().enumerate() {
            // Build the type plan node for this field first
            let field_proxy = field.effective_proxy(self.format_namespace);
            let child_id = self.build_node_with_proxy(field.shape(), field_proxy)?;
            parent_id.append(child_id, &mut self.arena);

            // Now create field metadata with the NodeId
            let field_meta = FieldPlanMeta::new(field, child_id);
            fields.push(field_meta);

            // Build the field initialization plan
            field_init_plans.push(Self::build_field_init_plan(
                index,
                field,
                container_has_default,
            ));
        }

        let has_flatten = fields.iter().any(|f| f.is_flattened);
        let field_lookup = FieldLookup::new(&fields);
        // Precompute deny_unknown_fields from shape attributes (avoids runtime attribute scanning)
        let deny_unknown_fields = shape.has_deny_unknown_fields_attr();

        Ok((
            StructPlan {
                struct_def,
                fields,
                field_lookup,
                has_flatten,
                deny_unknown_fields,
            },
            field_init_plans,
        ))
    }

    /// Build a FieldInitPlan for a field. Every field gets a plan.
    fn build_field_init_plan(
        index: usize,
        field: &'static Field,
        container_has_default: bool,
    ) -> FieldInitPlan {
        let validators = Self::extract_validators(field);
        let fill_rule = Self::determine_fill_rule(field, container_has_default);

        FieldInitPlan {
            index,
            offset: field.offset,
            name: field.name,
            field_shape: field.shape(),
            fill_rule,
            validators,
        }
    }

    /// Determine how to fill a field that wasn't set.
    /// Every field is either Defaultable or Required.
    fn determine_fill_rule(field: &'static Field, container_has_default: bool) -> FillRule {
        let field_shape = field.shape();

        // Check for explicit default on the field (#[facet(default)] or #[facet(default = expr)])
        if let Some(default_source) = field.default_source() {
            let field_default = match default_source {
                DefaultSource::Custom(f) => FieldDefault::Custom(*f),
                DefaultSource::FromTrait => FieldDefault::FromTrait(field_shape),
            };
            return FillRule::Defaultable(field_default);
        }

        // Option<T> without explicit default implicitly defaults to None
        let is_option = matches!(field_shape.def, Def::Option(_));
        if is_option && field_shape.is(Characteristic::Default) {
            return FillRule::Defaultable(FieldDefault::FromTrait(field_shape));
        }

        // Skipped fields MUST have a default (they're never deserialized)
        // If the type implements Default, use that
        if field.should_skip_deserializing() && field_shape.is(Characteristic::Default) {
            return FillRule::Defaultable(FieldDefault::FromTrait(field_shape));
        }

        // Empty structs/tuples (like `()`) are trivially defaultable
        if let Type::User(UserType::Struct(struct_type)) = field_shape.ty
            && struct_type.fields.is_empty()
            && field_shape.is(Characteristic::Default)
        {
            return FillRule::Defaultable(FieldDefault::FromTrait(field_shape));
        }

        // If container has #[facet(default)] and the field's type implements Default,
        // use the type's Default impl
        if container_has_default && field_shape.is(Characteristic::Default) {
            return FillRule::Defaultable(FieldDefault::FromTrait(field_shape));
        }

        // Field is required - must be set during deserialization
        // Note: For skipped fields without Default, this will cause an error
        // at deserialization time (which is correct - it's a logic error)
        FillRule::Required
    }

    /// Extract validators from field attributes.
    fn extract_validators(field: &'static Field) -> Vec<PrecomputedValidator> {
        let mut validators = Vec::new();
        let field_shape = field.shape();
        // Precompute scalar type once - used by validators that need it
        let scalar_type = field_shape.scalar_type();

        for attr in field.attributes.iter() {
            if attr.ns != Some("validate") {
                continue;
            }

            let kind = match attr.key {
                "custom" => {
                    // SAFETY: validate::custom attribute stores a ValidatorFn
                    let validator_fn = unsafe { *attr.data.ptr().get::<ValidatorFn>() };
                    ValidatorKind::Custom(validator_fn)
                }
                "min" => {
                    let limit = *attr
                        .get_as::<i64>()
                        .expect("validate::min attribute must contain i64");
                    // For numeric validators, scalar_type must be a numeric type
                    let scalar_type =
                        scalar_type.expect("validate::min requires numeric field type");
                    ValidatorKind::Min { limit, scalar_type }
                }
                "max" => {
                    let limit = *attr
                        .get_as::<i64>()
                        .expect("validate::max attribute must contain i64");
                    let scalar_type =
                        scalar_type.expect("validate::max requires numeric field type");
                    ValidatorKind::Max { limit, scalar_type }
                }
                "min_length" => {
                    let limit = *attr
                        .get_as::<usize>()
                        .expect("validate::min_length attribute must contain usize");
                    let scalar_type =
                        scalar_type.expect("validate::min_length requires string field type");
                    ValidatorKind::MinLength { limit, scalar_type }
                }
                "max_length" => {
                    let limit = *attr
                        .get_as::<usize>()
                        .expect("validate::max_length attribute must contain usize");
                    let scalar_type =
                        scalar_type.expect("validate::max_length requires string field type");
                    ValidatorKind::MaxLength { limit, scalar_type }
                }
                "email" => {
                    let scalar_type =
                        scalar_type.expect("validate::email requires string field type");
                    ValidatorKind::Email { scalar_type }
                }
                "url" => {
                    let scalar_type =
                        scalar_type.expect("validate::url requires string field type");
                    ValidatorKind::Url { scalar_type }
                }
                "regex" => {
                    let pattern = *attr
                        .get_as::<&'static str>()
                        .expect("validate::regex attribute must contain &'static str");
                    let scalar_type =
                        scalar_type.expect("validate::regex requires string field type");
                    ValidatorKind::Regex {
                        pattern,
                        scalar_type,
                    }
                }
                "contains" => {
                    let needle = *attr
                        .get_as::<&'static str>()
                        .expect("validate::contains attribute must contain &'static str");
                    let scalar_type =
                        scalar_type.expect("validate::contains requires string field type");
                    ValidatorKind::Contains {
                        needle,
                        scalar_type,
                    }
                }
                _ => continue, // Unknown validator, skip
            };

            validators.push(PrecomputedValidator { kind });
        }

        validators
    }

    /// Build an EnumPlan, attaching variant field children appropriately.
    fn build_enum_plan(
        &mut self,
        enum_def: &'static EnumType,
        parent_id: NodeId,
    ) -> Result<EnumPlan, AllocError> {
        let mut variants = Vec::with_capacity(enum_def.variants.len());

        for variant in enum_def.variants.iter() {
            let mut variant_fields = Vec::with_capacity(variant.data.fields.len());
            let mut field_init_plans = Vec::new();

            for (index, field) in variant.data.fields.iter().enumerate() {
                // Build the type plan node for this field first
                let field_proxy = field.effective_proxy(self.format_namespace);
                let child_id = self.build_node_with_proxy(field.shape(), field_proxy)?;
                parent_id.append(child_id, &mut self.arena);

                // Now create field metadata with the NodeId
                let field_meta = FieldPlanMeta::new(field, child_id);
                variant_fields.push(field_meta);

                // Build the field initialization plan (enums don't have container-level default)
                field_init_plans.push(Self::build_field_init_plan(index, field, false));
            }

            let field_lookup = FieldLookup::new(&variant_fields);
            let has_flatten = variant_fields.iter().any(|f| f.is_flattened);

            variants.push(VariantPlanMeta {
                variant,
                name: variant.effective_name(),
                fields: variant_fields,
                field_lookup,
                has_flatten,
                field_init_plans,
            });
        }

        let variant_lookup = VariantLookup::new(&variants);
        let num_variants = variants.len();

        // Find the index of the #[facet(other)] variant, if any
        let other_variant_idx = variants.iter().position(|v| v.variant.is_other());

        Ok(EnumPlan {
            enum_def,
            variants,
            variant_lookup,
            num_variants,
            other_variant_idx,
        })
    }

    fn finish(self, root: NodeId) -> TypePlan {
        TypePlan {
            arena: self.arena,
            root,
        }
    }
}

impl FieldPlanMeta {
    /// Build field metadata from a Field and its type plan NodeId.
    fn new(field: &'static Field, type_node: NodeId) -> Self {
        let name = field.name;
        let effective_name = field.effective_name();
        let alias = field.alias;
        let has_default = field.has_default();
        let is_flattened = field.is_flattened();

        // A field is required if:
        // - It has no default
        // - It's not an Option type
        // - It's not flattened (flattened fields are handled specially)
        let is_option = matches!(field.shape().def, Def::Option(_));
        let is_required = !has_default && !is_option && !is_flattened;

        FieldPlanMeta {
            field,
            name,
            effective_name,
            alias,
            has_default,
            is_required,
            is_flattened,
            type_node,
        }
    }
}

impl TypePlan {
    /// Build a TypePlan from a Shape with no format-specific proxy resolution.
    ///
    /// This recursively builds plans for nested types, using arena allocation
    /// to handle recursive types without stack overflow.
    ///
    /// For format-specific proxy resolution, use [`build_for_format`](Self::build_for_format).
    /// Build a TypePlan from a Shape.
    ///
    /// Returns an error if the shape contains types that cannot be deserialized.
    pub fn build(shape: &'static Shape) -> Result<Self, crate::AllocError> {
        Self::build_for_format(shape, None)
    }

    /// Build a TypePlan from a Shape with format-specific proxy resolution.
    ///
    /// The `format_namespace` (e.g., "json", "xml", "toml") is used to resolve
    /// format-specific proxies via `#[facet(proxy(json = ...))]` attributes.
    ///
    /// Returns an error if the shape contains types that cannot be deserialized.
    pub fn build_for_format(
        shape: &'static Shape,
        format_namespace: Option<&'static str>,
    ) -> Result<Self, crate::AllocError> {
        let mut builder = TypePlanBuilder::new(format_namespace);
        let root = builder.build_node(shape)?;
        Ok(builder.finish(root))
    }

    /// Get the root NodeId.
    #[inline]
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Get a node by NodeId.
    #[inline]
    pub fn get(&self, id: NodeId) -> Option<&TypePlanNode> {
        self.arena.get(id).map(|n| n.get())
    }

    /// Get the root node.
    #[inline]
    pub fn root_node(&self) -> &TypePlanNode {
        self.arena.get(self.root).unwrap().get()
    }

    /// Get the first child NodeId of a node.
    #[inline]
    pub fn first_child(&self, id: NodeId) -> Option<NodeId> {
        self.arena.get(id)?.first_child()
    }

    /// Get the nth child NodeId of a node (0-indexed).
    #[inline]
    pub fn nth_child(&self, id: NodeId, n: usize) -> Option<NodeId> {
        let mut child = self.first_child(id)?;
        for _ in 0..n {
            child = self.arena.get(child)?.next_sibling()?;
        }
        Some(child)
    }

    /// Get children of a node as an iterator of NodeIds.
    pub fn children(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        id.children(&self.arena)
    }

    // Navigation helpers that return NodeId

    /// Get the child NodeId for a struct field by index.
    /// Follows BackRef nodes for recursive types.
    #[inline]
    pub fn struct_field_node(&self, parent: NodeId, idx: usize) -> Option<NodeId> {
        let resolved = self.resolve_backref(parent)?;
        let node = self.get(resolved)?;
        let struct_plan = match &node.kind {
            TypePlanNodeKind::Struct(p) => p,
            _ => return None,
        };
        Some(struct_plan.fields.get(idx)?.type_node)
    }

    /// Get the child NodeId for an enum variant's field.
    /// Follows BackRef nodes for recursive types.
    #[inline]
    pub fn enum_variant_field_node(
        &self,
        parent: NodeId,
        variant_idx: usize,
        field_idx: usize,
    ) -> Option<NodeId> {
        let resolved = self.resolve_backref(parent)?;
        let node = self.get(resolved)?;
        let enum_plan = match &node.kind {
            TypePlanNodeKind::Enum(p) => p,
            _ => return None,
        };
        let variant = enum_plan.variants.get(variant_idx)?;
        Some(variant.fields.get(field_idx)?.type_node)
    }

    /// Get the child NodeId for list/array items (first child).
    #[inline]
    pub fn list_item_node(&self, parent: NodeId) -> Option<NodeId> {
        let node = self.get(parent)?;
        match &node.strategy {
            DeserStrategy::List { item_node, .. } | DeserStrategy::Array { item_node, .. } => {
                Some(*item_node)
            }
            DeserStrategy::BackRef { target } => self.list_item_node(*target),
            _ => None,
        }
    }

    /// Get the child NodeId for set items.
    #[inline]
    pub fn set_item_node(&self, parent: NodeId) -> Option<NodeId> {
        let node = self.get(parent)?;
        match &node.strategy {
            DeserStrategy::Set { item_node } => Some(*item_node),
            DeserStrategy::BackRef { target } => self.set_item_node(*target),
            _ => None,
        }
    }

    /// Get the child NodeId for map keys.
    #[inline]
    pub fn map_key_node(&self, parent: NodeId) -> Option<NodeId> {
        let node = self.get(parent)?;
        match &node.strategy {
            DeserStrategy::Map { key_node, .. } => Some(*key_node),
            DeserStrategy::BackRef { target } => self.map_key_node(*target),
            _ => None,
        }
    }

    /// Get the child NodeId for map values.
    #[inline]
    pub fn map_value_node(&self, parent: NodeId) -> Option<NodeId> {
        let node = self.get(parent)?;
        match &node.strategy {
            DeserStrategy::Map { value_node, .. } => Some(*value_node),
            DeserStrategy::BackRef { target } => self.map_value_node(*target),
            _ => None,
        }
    }

    /// Get the child NodeId for Option inner type.
    #[inline]
    pub fn option_inner_node(&self, parent: NodeId) -> Option<NodeId> {
        let node = self.get(parent)?;
        match &node.strategy {
            DeserStrategy::Option { some_node } => Some(*some_node),
            DeserStrategy::BackRef { target } => self.option_inner_node(*target),
            _ => None,
        }
    }

    /// Get the child NodeId for Result Ok type.
    #[inline]
    pub fn result_ok_node(&self, parent: NodeId) -> Option<NodeId> {
        let node = self.get(parent)?;
        match &node.strategy {
            DeserStrategy::Result { ok_node, .. } => Some(*ok_node),
            DeserStrategy::BackRef { target } => self.result_ok_node(*target),
            _ => None,
        }
    }

    /// Get the child NodeId for Result Err type.
    #[inline]
    pub fn result_err_node(&self, parent: NodeId) -> Option<NodeId> {
        let node = self.get(parent)?;
        match &node.strategy {
            DeserStrategy::Result { err_node, .. } => Some(*err_node),
            DeserStrategy::BackRef { target } => self.result_err_node(*target),
            _ => None,
        }
    }

    /// Get the child NodeId for pointer pointee.
    #[inline]
    pub fn pointer_pointee_node(&self, parent: NodeId) -> Option<NodeId> {
        let node = self.get(parent)?;
        match &node.strategy {
            DeserStrategy::Pointer { pointee_node } => Some(*pointee_node),
            DeserStrategy::BackRef { target } => self.pointer_pointee_node(*target),
            _ => None,
        }
    }

    /// Get the child NodeId for transparent wrapper inner.
    #[inline]
    pub fn transparent_inner_node(&self, parent: NodeId) -> Option<NodeId> {
        let node = self.get(parent)?;
        match &node.strategy {
            DeserStrategy::TransparentConvert { inner_node } => Some(*inner_node),
            DeserStrategy::BackRef { target } => self.transparent_inner_node(*target),
            _ => None,
        }
    }

    /// Get the child NodeId for shape.inner navigation (used by begin_inner).
    ///
    /// This works for both Transparent nodes and Scalar nodes that have an inner
    /// type (like `NonZero<T>`). Returns the first child if present.
    #[inline]
    pub fn inner_node(&self, parent: NodeId) -> Option<NodeId> {
        let node = self.get(parent)?;
        // Check if the node has shape.inner - if so, the first child is the inner node
        if node.shape.inner.is_some() {
            self.first_child(parent)
        } else {
            None
        }
    }

    /// Resolve a BackRef to get the actual node it points to.
    #[inline]
    pub fn resolve_backref(&self, id: NodeId) -> Option<NodeId> {
        let node = self.get(id)?;
        match &node.kind {
            TypePlanNodeKind::BackRef(target) => Some(*target),
            _ => Some(id), // Not a backref, return self
        }
    }

    /// Get the StructPlan if a node is a struct type.
    /// Follows BackRef nodes for recursive types.
    #[inline]
    pub fn as_struct_plan(&self, id: NodeId) -> Option<&StructPlan> {
        let resolved = self.resolve_backref(id)?;
        let node = self.get(resolved)?;
        match &node.kind {
            TypePlanNodeKind::Struct(plan) => Some(plan),
            _ => None,
        }
    }

    /// Get the EnumPlan if a node is an enum type.
    /// Follows BackRef nodes for recursive types.
    #[inline]
    pub fn as_enum_plan(&self, id: NodeId) -> Option<&EnumPlan> {
        let resolved = self.resolve_backref(id)?;
        let node = self.get(resolved)?;
        match &node.kind {
            TypePlanNodeKind::Enum(plan) => Some(plan),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[derive(Facet)]
    struct TestStruct {
        name: String,
        age: u32,
        email: Option<String>,
    }

    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)] // Fields used for reflection testing
    enum TestEnum {
        Unit,
        Tuple(u32),
        Struct { value: String },
    }

    #[derive(Facet)]
    struct RecursiveStruct {
        value: u32,
        // Recursive: contains Option<Box<Self>>
        next: Option<Box<RecursiveStruct>>,
    }

    #[test]
    fn test_typeplan_struct() {
        let plan = TypePlan::build(TestStruct::SHAPE).unwrap();
        let root = plan.root_node();

        assert_eq!(root.shape, TestStruct::SHAPE);
        assert!(!root.has_default);

        match &root.kind {
            TypePlanNodeKind::Struct(struct_plan) => {
                assert_eq!(struct_plan.fields.len(), 3);
                assert!(!struct_plan.has_flatten);

                // Check field lookup
                assert_eq!(struct_plan.field_lookup.find("name"), Some(0));
                assert_eq!(struct_plan.field_lookup.find("age"), Some(1));
                assert_eq!(struct_plan.field_lookup.find("email"), Some(2));
                assert_eq!(struct_plan.field_lookup.find("unknown"), None);

                // Check field metadata
                assert_eq!(struct_plan.fields[0].name, "name");
                assert!(struct_plan.fields[0].is_required);

                assert_eq!(struct_plan.fields[1].name, "age");
                assert!(struct_plan.fields[1].is_required);

                assert_eq!(struct_plan.fields[2].name, "email");
                assert!(!struct_plan.fields[2].is_required); // Option has implicit default

                // Check child plan for Option field (field index 2 = third child)
                let email_child = plan.struct_field_node(plan.root(), 2).unwrap();
                let email_node = plan.get(email_child).unwrap();
                match &email_node.kind {
                    TypePlanNodeKind::Option => {
                        // inner should be String (scalar)
                        let inner = plan.option_inner_node(email_child).unwrap();
                        let inner_node = plan.get(inner).unwrap();
                        match &inner_node.kind {
                            TypePlanNodeKind::Scalar => {}
                            other => panic!("Expected Scalar for String, got {:?}", other),
                        }
                    }
                    other => panic!("Expected Option, got {:?}", other),
                }
            }
            other => panic!("Expected Struct, got {:?}", other),
        }
    }

    #[test]
    fn test_typeplan_enum() {
        let plan = TypePlan::build(TestEnum::SHAPE).unwrap();
        let root = plan.root_node();

        assert_eq!(root.shape, TestEnum::SHAPE);

        match &root.kind {
            TypePlanNodeKind::Enum(enum_plan) => {
                assert_eq!(enum_plan.num_variants, 3);

                // Check variant lookup
                assert_eq!(enum_plan.variant_lookup.find("Unit"), Some(0));
                assert_eq!(enum_plan.variant_lookup.find("Tuple"), Some(1));
                assert_eq!(enum_plan.variant_lookup.find("Struct"), Some(2));
                assert_eq!(enum_plan.variant_lookup.find("Unknown"), None);

                // Unit variant has no fields
                assert_eq!(enum_plan.variants[0].fields.len(), 0);

                // Tuple variant has 1 field
                assert_eq!(enum_plan.variants[1].fields.len(), 1);

                // Struct variant has 1 field
                assert_eq!(enum_plan.variants[2].fields.len(), 1);
                assert_eq!(enum_plan.variants[2].field_lookup.find("value"), Some(0));
            }
            other => panic!("Expected Enum, got {:?}", other),
        }
    }

    #[test]
    fn test_typeplan_list() {
        let plan = TypePlan::build(<Vec<u32> as Facet>::SHAPE).unwrap();
        let root = plan.root_node();

        match &root.kind {
            TypePlanNodeKind::List => {
                let item = plan.list_item_node(plan.root()).unwrap();
                let item_node = plan.get(item).unwrap();
                match &item_node.kind {
                    TypePlanNodeKind::Scalar => {}
                    other => panic!("Expected Scalar for u32, got {:?}", other),
                }
            }
            other => panic!("Expected List, got {:?}", other),
        }
    }

    #[test]
    fn test_typeplan_recursive() {
        // This should NOT stack overflow - indextree handles the cycle
        let plan = TypePlan::build(RecursiveStruct::SHAPE).unwrap();
        let root = plan.root_node();

        match &root.kind {
            TypePlanNodeKind::Struct(struct_plan) => {
                assert_eq!(struct_plan.fields.len(), 2);
                assert_eq!(struct_plan.fields[0].name, "value");
                assert_eq!(struct_plan.fields[1].name, "next");

                // The 'next' field is Option<Box<RecursiveStruct>>
                // Its child plan should eventually contain a BackRef
                let next_child = plan.struct_field_node(plan.root(), 1).unwrap();
                let next_node = plan.get(next_child).unwrap();

                // Should be Option
                assert!(matches!(next_node.kind, TypePlanNodeKind::Option));

                // Inner should be Pointer (Box)
                let inner = plan.option_inner_node(next_child).unwrap();
                let inner_node = plan.get(inner).unwrap();
                assert!(matches!(inner_node.kind, TypePlanNodeKind::Pointer));

                // Pointee should be BackRef to root (or a struct with BackRef)
                let pointee = plan.pointer_pointee_node(inner).unwrap();
                let pointee_node = plan.get(pointee).unwrap();

                // This should be a BackRef pointing to the root
                match &pointee_node.kind {
                    TypePlanNodeKind::BackRef(target) => {
                        assert_eq!(*target, plan.root());
                    }
                    _ => panic!(
                        "Expected BackRef for recursive type, got {:?}",
                        pointee_node.kind
                    ),
                }
            }
            other => panic!("Expected Struct, got {:?}", other),
        }
    }

    #[test]
    fn test_field_lookup_small() {
        let lookup = FieldLookup::Small(vec![
            FieldLookupEntry {
                name: "foo",
                index: 0,
                is_alias: false,
            },
            FieldLookupEntry {
                name: "bar",
                index: 1,
                is_alias: false,
            },
            FieldLookupEntry {
                name: "baz",
                index: 2,
                is_alias: false,
            },
        ]);

        assert_eq!(lookup.find("foo"), Some(0));
        assert_eq!(lookup.find("bar"), Some(1));
        assert_eq!(lookup.find("baz"), Some(2));
        assert_eq!(lookup.find("qux"), None);
    }

    #[test]
    fn test_field_lookup_prefix_buckets() {
        // Create enough entries to trigger PrefixBuckets (>8 entries)
        // Include short names like "id" to test zero-padding
        let entries = vec![
            FieldLookupEntry {
                name: "id",
                index: 0,
                is_alias: false,
            },
            FieldLookupEntry {
                name: "name",
                index: 1,
                is_alias: false,
            },
            FieldLookupEntry {
                name: "email",
                index: 2,
                is_alias: false,
            },
            FieldLookupEntry {
                name: "url",
                index: 3,
                is_alias: false,
            },
            FieldLookupEntry {
                name: "description",
                index: 4,
                is_alias: false,
            },
            FieldLookupEntry {
                name: "created_at",
                index: 5,
                is_alias: false,
            },
            FieldLookupEntry {
                name: "updated_at",
                index: 6,
                is_alias: false,
            },
            FieldLookupEntry {
                name: "status",
                index: 7,
                is_alias: false,
            },
            FieldLookupEntry {
                name: "type",
                index: 8,
                is_alias: false,
            },
            FieldLookupEntry {
                name: "metadata",
                index: 9,
                is_alias: false,
            },
        ];
        let lookup = FieldLookup::from_entries(entries);

        // Verify it's using PrefixBuckets
        assert!(matches!(lookup, FieldLookup::PrefixBuckets { .. }));

        // Test lookups - including short keys
        assert_eq!(lookup.find("id"), Some(0));
        assert_eq!(lookup.find("name"), Some(1));
        assert_eq!(lookup.find("email"), Some(2));
        assert_eq!(lookup.find("url"), Some(3));
        assert_eq!(lookup.find("description"), Some(4));
        assert_eq!(lookup.find("created_at"), Some(5));
        assert_eq!(lookup.find("updated_at"), Some(6));
        assert_eq!(lookup.find("status"), Some(7));
        assert_eq!(lookup.find("type"), Some(8));
        assert_eq!(lookup.find("metadata"), Some(9));
        assert_eq!(lookup.find("unknown"), None);
        assert_eq!(lookup.find("i"), None); // prefix of "id"
        assert_eq!(lookup.find("ide"), None); // not a field
    }

    #[test]
    fn test_variant_lookup_small() {
        let lookup = VariantLookup::Small(vec![("A", 0), ("B", 1), ("C", 2)]);

        assert_eq!(lookup.find("A"), Some(0));
        assert_eq!(lookup.find("B"), Some(1));
        assert_eq!(lookup.find("C"), Some(2));
        assert_eq!(lookup.find("D"), None);
    }

    #[test]
    fn test_field_lookup_from_struct_type() {
        use facet_core::{Type, UserType};

        // Get struct_def from TestStruct's shape
        let struct_def = match &TestStruct::SHAPE.ty {
            Type::User(UserType::Struct(def)) => def,
            _ => panic!("Expected struct type"),
        };

        let lookup = FieldLookup::from_struct_type(struct_def);

        // Should find all fields by their names
        assert_eq!(lookup.find("name"), Some(0));
        assert_eq!(lookup.find("age"), Some(1));
        assert_eq!(lookup.find("email"), Some(2));
        assert_eq!(lookup.find("unknown"), None);
    }

    #[test]
    fn test_variant_lookup_from_enum_type() {
        use facet_core::{Type, UserType};

        // Get enum_def from TestEnum's shape
        let enum_def = match &TestEnum::SHAPE.ty {
            Type::User(UserType::Enum(def)) => def,
            _ => panic!("Expected enum type"),
        };

        let lookup = VariantLookup::from_enum_type(enum_def);

        // Should find all variants by name
        assert_eq!(lookup.find("Unit"), Some(0));
        assert_eq!(lookup.find("Tuple"), Some(1));
        assert_eq!(lookup.find("Struct"), Some(2));
        assert_eq!(lookup.find("Unknown"), None);
    }
}
