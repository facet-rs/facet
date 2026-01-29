//! TypePlan: Precomputed deserialization plans for types.
//!
//! Instead of repeatedly inspecting Shape/Def at runtime during deserialization,
//! we build a plan tree once that encodes all the decisions we'll make.
//!
//! This design:
//! - Avoids self-referential structs (TypePlan owns its arenas)
//! - Allows reusing the plan across multiple deserializations
//! - Uses 32-bit indices for compact representation

use alloc::sync::Arc;
use alloc::vec::Vec;

use facet_core::{
    Characteristic, ConstTypeId, Def, DefaultInPlaceFn, DefaultSource, EnumType, Field, ProxyDef,
    ScalarType, SequenceType, Shape, StructType, Type, UserType, ValidatorFn, Variant,
};
use hashbrown::HashMap;
use smallvec::SmallVec;

use super::arena::{Arena, Idx, SliceRange};
use crate::AllocError;

/// Type alias for node indices in the TypePlan.
pub type NodeId = Idx<TypePlanNode>;

/// Type alias for field plan slice ranges.
pub type FieldRange = SliceRange<FieldPlan>;

/// Type alias for variant plan slice ranges.
pub type VariantRange = SliceRange<VariantPlanMeta>;

/// Type alias for validator slice ranges.
pub type ValidatorRange = SliceRange<PrecomputedValidator>;

/// Precomputed deserialization plan tree for a type.
///
/// Built once from a Shape, this encodes all decisions needed during deserialization
/// without repeated runtime lookups. The `TypePlan` owns all its allocations through
/// internal arenas.
///
/// The type parameter `T` is phantom and provides compile-time type safety:
/// you cannot accidentally pass a `TypePlan<Foo>` where `TypePlan<Bar>` is expected.
/// There is no public way to erase the type parameter.
#[derive(Debug)]
pub struct TypePlan<T: ?Sized> {
    /// The actual plan data (type-erased internally)
    core: Arc<TypePlanCore>,
    /// Phantom type parameter for compile-time type safety
    _marker: core::marker::PhantomData<fn() -> T>,
}

/// Type-erased plan data that owns all allocations.
///
/// This is what `Partial` actually stores a reference to. The type safety comes from
/// the `TypePlan<T>` wrapper at the API boundary.
///
/// Users should build plans using `TypePlan::<T>::build()` which provides
/// type safety.
#[derive(Debug)]
pub struct TypePlanCore {
    /// Arena of type plan nodes
    nodes: Arena<TypePlanNode>,
    /// Arena of field plans (for structs and enum variants)
    fields: Arena<FieldPlan>,
    /// Arena of variant metadata (for enums)
    variants: Arena<VariantPlanMeta>,
    /// Arena of precomputed validators
    validators: Arena<PrecomputedValidator>,
    /// Arena of field lookup entries
    field_entries: Arena<FieldLookupEntry>,
    /// Arena of bucket tuples for prefix-based lookup
    buckets: Arena<(u64, u32, u32)>,
    /// Sorted lookup table for resolving BackRef nodes by TypeId.
    node_lookup: Vec<(ConstTypeId, NodeId)>,
    /// Root node index
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
}

/// Precomputed deserialization strategy with all data needed to execute it.
///
/// This is denormalized: we store node indices, proxy defs, etc. directly so the
/// deserializer can follow the plan without chasing pointers through Shape/vtable.
#[derive(Debug)]
pub enum DeserStrategy {
    /// Container-level proxy: the type itself has `#[facet(proxy = X)]`
    ContainerProxy {
        /// The proxy definition containing conversion functions
        proxy_def: &'static ProxyDef,
        /// The shape of the proxy type (what we deserialize)
        proxy_shape: &'static Shape,
        /// Child node representing the proxy type's structure
        proxy_node: NodeId,
    },
    /// Field-level proxy: the field has `#[facet(proxy = X)]` but the type doesn't
    FieldProxy {
        /// The proxy definition containing conversion functions
        proxy_def: &'static ProxyDef,
        /// The shape of the proxy type (what we deserialize)
        proxy_shape: &'static Shape,
        /// Child node representing the proxy type's structure
        proxy_node: NodeId,
    },
    /// Smart pointer (Box, Arc, Rc) with known pointee type
    Pointer {
        /// The pointee type's plan
        pointee_node: NodeId,
    },
    /// Opaque smart pointer (`#[facet(opaque)]`) - cannot be deserialized, only set wholesale
    OpaquePointer,
    /// Opaque type (`Opaque<T>`) - cannot be deserialized, only set wholesale via proxy
    Opaque,
    /// Transparent wrapper with try_from (like NonZero)
    TransparentConvert {
        /// The inner type's plan
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
        /// The Some variant's inner type plan
        some_node: NodeId,
    },
    /// `Result<T, E>`
    Result {
        /// The Ok variant's type plan
        ok_node: NodeId,
        /// The Err variant's type plan
        err_node: NodeId,
    },
    /// List (Vec, VecDeque, etc.)
    List {
        /// The item type's plan
        item_node: NodeId,
        /// Whether this is specifically `Vec<u8>` (for optimized byte sequence handling)
        is_byte_vec: bool,
    },
    /// Map (HashMap, BTreeMap, etc.)
    Map {
        /// The key type's plan
        key_node: NodeId,
        /// The value type's plan
        value_node: NodeId,
    },
    /// Set (HashSet, BTreeSet, etc.)
    Set {
        /// The item type's plan
        item_node: NodeId,
    },
    /// Fixed-size array [T; N]
    Array {
        /// Array length
        len: usize,
        /// The item type's plan
        item_node: NodeId,
    },
    /// DynamicValue (like `facet_value::Value`)
    DynamicValue,
    /// Metadata container (like `Spanned<T>`, `Documented<T>`)
    /// These require special field-by-field handling for metadata population
    MetadataContainer,
    /// BackRef to recursive type - resolved via TypePlan::resolve_backref()
    BackRef {
        /// The TypeId of the target node
        target_type_id: ConstTypeId,
    },
}

/// The specific kind of type and its deserialization strategy.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)] // Struct/Enum variants are intentionally large
pub enum TypePlanNodeKind {
    /// Scalar types (integers, floats, bool, char, strings)
    Scalar,

    /// Struct types with named or positional fields
    Struct(StructPlan),

    /// Enum types with variants
    Enum(EnumPlan),

    /// `Option<T>` - special handling for None/Some
    Option,

    /// `Result<T, E>` - special handling for Ok/Err
    Result,

    /// `Vec<T>`, `VecDeque<T>`, etc.
    List,

    /// Slice types `[T]` (unsized, used via smart pointers like `Arc<[T]>`)
    Slice,

    /// `HashMap<K, V>`, `BTreeMap<K, V>`, etc.
    Map,

    /// `HashSet<T>`, `BTreeSet<T>`, etc.
    Set,

    /// Fixed-size arrays `[T; N]`
    Array {
        /// Array length N
        len: usize,
    },

    /// Smart pointers: `Box<T>`, `Arc<T>`, `Rc<T>`
    Pointer,

    /// Opaque smart pointers (`#[facet(opaque)]`)
    OpaquePointer,

    /// Opaque types (`Opaque<T>`) - can only be set wholesale, not deserialized
    Opaque,

    /// DynamicValue (like `serde_json::Value`)
    DynamicValue,

    /// Transparent wrappers (newtypes)
    Transparent,

    /// Back-reference to an ancestor node (for recursive types)
    /// Resolved via TypePlan::resolve_backref()
    BackRef(ConstTypeId),
}

/// Precomputed plan for struct deserialization.
#[derive(Debug)]
pub struct StructPlan {
    /// Reference to the struct type definition
    pub struct_def: &'static StructType,
    /// Complete plans for each field, indexed by field position.
    /// Combines matching metadata with initialization/validation info.
    pub fields: FieldRange,
    /// Fast field lookup by name
    pub field_lookup: FieldLookup,
    /// Whether any field has #[facet(flatten)]
    pub has_flatten: bool,
    /// Whether to reject unknown fields (precomputed from `#[facet(deny_unknown_fields)]`)
    pub deny_unknown_fields: bool,
}

/// Complete precomputed plan for a single field.
///
/// Combines field matching metadata (name, aliases, type node) with
/// initialization/validation info (fill rule, validators, offset).
#[derive(Debug, Clone)]
pub struct FieldPlan {
    // --- Metadata for matching/lookup ---
    /// Reference to the field definition
    pub field: &'static Field,
    /// Field name (for path tracking and error messages)
    pub name: &'static str,
    /// The name to match in input (considers rename)
    pub effective_name: &'static str,
    /// Alias if any
    pub alias: Option<&'static str>,
    /// Whether this field is flattened
    pub is_flattened: bool,
    /// This field's type plan node index
    pub type_node: NodeId,

    // --- Initialization/validation ---
    /// Field index in the struct (for ISet tracking)
    pub index: usize,
    /// Field offset in bytes from struct base (for calculating field pointer)
    pub offset: usize,
    /// The field's type shape (for reading values during validation)
    pub field_shape: &'static Shape,
    /// How to handle this field if not set during deserialization
    pub fill_rule: FillRule,
    /// Validators to run after the field is set (precomputed from attributes)
    pub validators: ValidatorRange,
}

impl FieldPlan {
    /// Returns true if this field has a default value.
    #[inline]
    pub fn has_default(&self) -> bool {
        matches!(self.fill_rule, FillRule::Defaultable(_))
    }

    /// Returns true if this field is required (no default, not Option).
    #[inline]
    pub fn is_required(&self) -> bool {
        matches!(self.fill_rule, FillRule::Required)
    }
}

/// Type alias for backwards compatibility with code expecting FieldInitPlan.
pub type FieldInitPlan = FieldPlan;

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

    #[allow(unsafe_code)]
    fn get_string_length(field_ptr: facet_core::PtrConst, scalar_type: ScalarType) -> usize {
        // SAFETY: caller guarantees field_ptr points to valid string data
        unsafe {
            match scalar_type {
                ScalarType::String => (*field_ptr.get::<alloc::string::String>()).len(),
                ScalarType::CowStr => (*field_ptr.get::<alloc::borrow::Cow<'_, str>>()).len(),
                _ => 0, // Non-string type
            }
        }
    }

    /// # Safety
    /// `field_ptr` must point to initialized memory of the given scalar_type.
    /// The returned reference is only valid while the pointed-to memory is valid.
    #[allow(unsafe_code)]
    unsafe fn get_string(field_ptr: facet_core::PtrConst, scalar_type: ScalarType) -> &'static str {
        // SAFETY: The caller guarantees field_ptr points to valid memory.
        // We transmute the lifetime to 'static because we need to return a reference
        // but don't have a lifetime to bind it to. The caller must ensure the reference
        // is not used after the pointed-to memory is invalidated.
        match scalar_type {
            ScalarType::String => {
                let s: &str = unsafe { (*field_ptr.get::<alloc::string::String>()).as_str() };
                // SAFETY: The caller ensures the pointed-to memory remains valid for the returned lifetime.
                unsafe { core::mem::transmute::<&str, &'static str>(s) }
            }
            ScalarType::CowStr => {
                let s: &str = unsafe { (*field_ptr.get::<alloc::borrow::Cow<'_, str>>()).as_ref() };
                // SAFETY: The caller ensures the pointed-to memory remains valid for the returned lifetime.
                unsafe { core::mem::transmute::<&str, &'static str>(s) }
            }
            _ => "", // Non-string type
        }
    }

    fn is_valid_email(s: &str) -> bool {
        // Simple email validation: contains exactly one @ with text on both sides
        let parts: Vec<&str> = s.split('@').collect();
        parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() && parts[1].contains('.')
    }

    fn is_valid_url(s: &str) -> bool {
        // Simple URL validation: starts with http:// or https://
        s.starts_with("http://") || s.starts_with("https://")
    }

    fn matches_pattern(s: &str, pattern: &str) -> bool {
        // Use regex crate if available, otherwise basic substring match
        #[cfg(feature = "regex")]
        {
            regex::Regex::new(pattern)
                .map(|re| re.is_match(s))
                .unwrap_or(false)
        }
        #[cfg(not(feature = "regex"))]
        {
            // Fallback: treat pattern as literal substring
            s.contains(pattern)
        }
    }
}

/// The kind of validation to perform on a field.
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

/// Precomputed plan for enum deserialization.
#[derive(Debug)]
pub struct EnumPlan {
    /// Reference to the enum type definition
    pub enum_def: &'static EnumType,
    /// Plans for each variant
    pub variants: VariantRange,
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
    /// Complete field plans for this variant
    pub fields: FieldRange,
    /// Fast field lookup for this variant
    pub field_lookup: FieldLookup,
    /// Whether any field in this variant has #[facet(flatten)]
    pub has_flatten: bool,
}

/// Fast lookup from field name to field index.
///
/// Uses different strategies based on field count:
/// - Small (≤8 fields): linear scan (cache-friendly, no hashing overhead)
/// - Large (>8 fields): prefix-based dispatch (like JIT) - group by first N bytes
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)] // SmallVec is intentionally inline
pub enum FieldLookup {
    /// For small structs: just store (name, index) pairs
    /// Capped at LOOKUP_THRESHOLD (8) entries, so we inline all of them.
    Small(SmallVec<FieldLookupEntry, 16>),
    /// For larger structs: prefix-based buckets
    /// Entries are grouped by their N-byte prefix, buckets sorted by prefix for binary search
    PrefixBuckets {
        /// Prefix length in bytes (4 or 8)
        prefix_len: usize,
        /// All entries, grouped by prefix (range into field_entries arena)
        entries: SliceRange<FieldLookupEntry>,
        /// (prefix, start_index, count) sorted by prefix (range into buckets arena)
        buckets: SliceRange<(u64, u32, u32)>,
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
#[allow(clippy::large_enum_variant)] // SmallVec is intentionally inline
pub enum VariantLookup {
    /// For small enums: linear scan (most enums have ≤8 variants)
    Small(SmallVec<(&'static str, usize), 8>),
    /// For larger enums: sorted for binary search (range into buckets arena repurposed)
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
    /// Find a field index by name.
    #[inline]
    pub fn find(&self, name: &str, core: &TypePlanCore) -> Option<usize> {
        match self {
            FieldLookup::Small(entries) => entries.iter().find(|e| e.name == name).map(|e| e.index),
            FieldLookup::PrefixBuckets {
                prefix_len,
                entries,
                buckets,
            } => {
                let prefix = compute_prefix(name, *prefix_len);

                // Get the bucket slice from arena
                let bucket_slice = core.buckets.get_slice(*buckets);
                let bucket_idx = bucket_slice
                    .binary_search_by_key(&prefix, |(p, _, _)| *p)
                    .ok()?;
                let (_, start, count) = bucket_slice[bucket_idx];

                // Get entries slice from arena
                let all_entries = core.field_entries.get_slice(*entries);
                let bucket_entries = &all_entries[start as usize..(start + count) as usize];
                bucket_entries
                    .iter()
                    .find(|e| e.name == name)
                    .map(|e| e.index)
            }
        }
    }

    /// Find a field index by name (standalone version for Small lookup only).
    /// For use in tests and contexts without TypePlanCore access.
    #[inline]
    pub fn find_small(&self, name: &str) -> Option<usize> {
        match self {
            FieldLookup::Small(entries) => entries.iter().find(|e| e.name == name).map(|e| e.index),
            FieldLookup::PrefixBuckets { .. } => {
                panic!("find_small called on PrefixBuckets - use find() with TypePlanCore")
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
    /// Arena for nodes
    nodes: Arena<TypePlanNode>,
    /// Arena for field plans
    fields: Arena<FieldPlan>,
    /// Arena for variant metadata
    variants: Arena<VariantPlanMeta>,
    /// Arena for validators
    validators: Arena<PrecomputedValidator>,
    /// Arena for field lookup entries
    field_entries: Arena<FieldLookupEntry>,
    /// Arena for bucket tuples
    buckets: Arena<(u64, u32, u32)>,
    /// Types we're currently building (for cycle detection).
    /// Added when we START building a node.
    building: hashbrown::HashSet<ConstTypeId>,
    /// Finished nodes, keyed by TypeId.
    /// Added when we FINISH building a node.
    finished: HashMap<ConstTypeId, NodeId>,
    /// Format namespace for resolving format-specific proxies (e.g., "json", "xml")
    format_namespace: Option<&'static str>,
}

impl TypePlanBuilder {
    fn new(format_namespace: Option<&'static str>) -> Self {
        Self {
            nodes: Arena::new(),
            fields: Arena::new(),
            variants: Arena::new(),
            validators: Arena::new(),
            field_entries: Arena::new(),
            buckets: Arena::new(),
            building: hashbrown::HashSet::new(),
            finished: HashMap::new(),
            format_namespace,
        }
    }

    /// Finalize the builder into a TypePlanCore.
    fn finish(self, root: NodeId) -> TypePlanCore {
        let mut node_lookup: Vec<_> = self.finished.into_iter().collect();
        node_lookup.sort_by_key(|(id, _)| *id);

        TypePlanCore {
            nodes: self.nodes,
            fields: self.fields,
            variants: self.variants,
            validators: self.validators,
            field_entries: self.field_entries,
            buckets: self.buckets,
            node_lookup,
            root,
        }
    }

    /// Build a node for a shape, returning its index.
    /// Uses the shape's own proxy if present (container-level proxy).
    fn build_node(&mut self, shape: &'static Shape) -> Result<NodeId, AllocError> {
        // No field-level proxy when building directly - container proxy will be detected
        // inside build_node_with_proxy from the shape itself
        self.build_node_with_proxy(shape, None)
    }

    /// Build a node for a shape with an optional explicit proxy override.
    /// Used for field-level proxies where the proxy is on the field, not the type.
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

        // Check if we're currently building this type (cycle detected)
        if self.building.contains(&type_id) {
            // Create a BackRef node with just the TypeId - resolved later via lookup
            let backref_node = TypePlanNode {
                shape,
                kind: TypePlanNodeKind::BackRef(type_id),
                strategy: DeserStrategy::BackRef {
                    target_type_id: type_id,
                },
                has_default: shape.is(Characteristic::Default),
                proxy: effective_proxy,
            };
            let idx = self.nodes.alloc(backref_node);
            return Ok(idx);
        }

        // Mark this type as being built (for cycle detection)
        self.building.insert(type_id);

        // Build children first - they may create BackRefs if they hit cycles
        let proxy_node = if let Some(proxy_def) = effective_proxy {
            Some(self.build_node(proxy_def.shape)?)
        } else {
            None
        };

        let (kind, children) = self.build_kind(shape)?;

        let strategy = self.compute_strategy(
            shape,
            &kind,
            container_proxy,
            field_proxy,
            proxy_node,
            &children,
        )?;

        // Now allocate the node with final values (no mutation needed!)
        let node = TypePlanNode {
            shape,
            kind,
            strategy,
            has_default: shape.is(Characteristic::Default),
            proxy: effective_proxy,
        };
        let idx = self.nodes.alloc(node);

        // Done building - move from building set to finished map
        self.building.remove(&type_id);
        self.finished.insert(type_id, idx);

        Ok(idx)
    }

    /// Compute the deserialization strategy with all data needed to execute it.
    fn compute_strategy(
        &self,
        shape: &'static Shape,
        kind: &TypePlanNodeKind,
        proxy: Option<&'static ProxyDef>,
        explicit_field_proxy: Option<&'static ProxyDef>,
        proxy_node: Option<NodeId>,
        children: &[NodeId],
    ) -> Result<DeserStrategy, AllocError> {
        let nth_child = |n: usize| -> NodeId { children[n] };
        let first_child = || children[0];

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
            TypePlanNodeKind::BackRef(type_id) => DeserStrategy::BackRef {
                target_type_id: *type_id,
            },
        })
    }

    /// Build the TypePlanNodeKind for a shape and return child node indices for compute_strategy.
    fn build_kind(
        &mut self,
        shape: &'static Shape,
    ) -> Result<(TypePlanNodeKind, Vec<NodeId>), AllocError> {
        let mut children = Vec::new();

        // Check shape.def first - this tells us the semantic meaning of the type
        let kind = match &shape.def {
            Def::Scalar => {
                // For scalar types with shape.inner (like NonZero<T>), build a child node
                // for the inner type. This enables proper TypePlan navigation when
                // begin_inner() is called for transparent wrapper deserialization.
                if let Some(inner_shape) = shape.inner {
                    children.push(self.build_node(inner_shape)?);
                }
                TypePlanNodeKind::Scalar
            }

            Def::Option(opt_def) => {
                children.push(self.build_node(opt_def.t())?);
                TypePlanNodeKind::Option
            }

            Def::Result(res_def) => {
                children.push(self.build_node(res_def.t())?);
                children.push(self.build_node(res_def.e())?);
                TypePlanNodeKind::Result
            }

            Def::List(list_def) => {
                children.push(self.build_node(list_def.t())?);
                TypePlanNodeKind::List
            }

            Def::Map(map_def) => {
                children.push(self.build_node(map_def.k())?);
                children.push(self.build_node(map_def.v())?);
                TypePlanNodeKind::Map
            }

            Def::Set(set_def) => {
                children.push(self.build_node(set_def.t())?);
                TypePlanNodeKind::Set
            }

            Def::Array(arr_def) => {
                children.push(self.build_node(arr_def.t())?);
                TypePlanNodeKind::Array { len: arr_def.n }
            }

            Def::Pointer(ptr_def) => {
                if let Some(pointee) = ptr_def.pointee() {
                    children.push(self.build_node(pointee)?);
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
                        let struct_plan = self.build_struct_plan(shape, struct_type)?;
                        // Struct fields store their NodeIds in FieldPlan, no children needed
                        return Ok((TypePlanNodeKind::Struct(struct_plan), Vec::new()));
                    }
                    Type::User(UserType::Enum(enum_type)) => {
                        // Enum variants store their NodeIds in VariantPlanMeta, no children needed
                        TypePlanNodeKind::Enum(self.build_enum_plan(enum_type)?)
                    }
                    // Handle slices like lists - they have an element type
                    Type::Sequence(SequenceType::Slice(slice_type)) => {
                        children.push(self.build_node(slice_type.t)?);
                        // Use Slice kind so we can distinguish from List if needed
                        TypePlanNodeKind::Slice
                    }
                    // Opaque types have Def::Undefined AND ty that doesn't match above
                    Type::User(UserType::Opaque) | Type::Undefined => TypePlanNodeKind::Opaque,
                    _ => {
                        // Check for transparent wrappers (newtypes) as fallback
                        if let Some(inner) = shape.inner {
                            children.push(self.build_node(inner)?);
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
        Ok((kind, children))
    }

    /// Build a StructPlan with all field plans.
    fn build_struct_plan(
        &mut self,
        shape: &'static Shape,
        struct_def: &'static StructType,
    ) -> Result<StructPlan, AllocError> {
        let mut field_plans = Vec::with_capacity(struct_def.fields.len());

        // Check if the container struct has #[facet(default)]
        let container_has_default = shape.is(Characteristic::Default);

        for (index, field) in struct_def.fields.iter().enumerate() {
            // Build the type plan node for this field first
            let field_proxy = field.effective_proxy(self.format_namespace);
            let child_node = self.build_node_with_proxy(field.shape(), field_proxy)?;

            // Build validators and fill rule
            let validators = self.extract_validators(field);
            let fill_rule = Self::determine_fill_rule(field, container_has_default);

            // Create unified field plan
            field_plans
                .push(self.create_field_plan(index, field, child_node, fill_rule, validators));
        }

        // Allocate fields into arena
        let has_flatten = field_plans.iter().any(|f| f.is_flattened);
        let fields = self.fields.alloc_extend(field_plans.iter().cloned());

        // Build field lookup
        let field_lookup = self.build_field_lookup(&field_plans);

        // Precompute deny_unknown_fields from shape attributes (avoids runtime attribute scanning)
        let deny_unknown_fields = shape.has_deny_unknown_fields_attr();

        Ok(StructPlan {
            struct_def,
            fields,
            field_lookup,
            has_flatten,
            deny_unknown_fields,
        })
    }

    /// Create a FieldPlan from its components.
    fn create_field_plan(
        &mut self,
        index: usize,
        field: &'static Field,
        type_node: NodeId,
        fill_rule: FillRule,
        validators: ValidatorRange,
    ) -> FieldPlan {
        let name = field.name;
        let effective_name = field.effective_name();
        let alias = field.alias;
        let is_flattened = field.is_flattened();

        FieldPlan {
            // Metadata for matching/lookup
            field,
            name,
            effective_name,
            alias,
            is_flattened,
            type_node,
            // Initialization/validation
            index,
            offset: field.offset,
            field_shape: field.shape(),
            fill_rule,
            validators,
        }
    }

    /// Build a field lookup from field plans.
    fn build_field_lookup(&mut self, field_plans: &[FieldPlan]) -> FieldLookup {
        let mut entries: Vec<FieldLookupEntry> = Vec::with_capacity(field_plans.len() * 2);

        for (index, field_plan) in field_plans.iter().enumerate() {
            // Add primary name
            entries.push(FieldLookupEntry {
                name: field_plan.effective_name,
                index,
                is_alias: false,
            });

            // Add alias if present
            if let Some(alias) = field_plan.alias {
                entries.push(FieldLookupEntry {
                    name: alias,
                    index,
                    is_alias: true,
                });
            }
        }

        self.build_field_lookup_from_entries(entries)
    }

    /// Build lookup structure from entries.
    fn build_field_lookup_from_entries(&mut self, entries: Vec<FieldLookupEntry>) -> FieldLookup {
        let total_entries = entries.len();
        if total_entries <= LOOKUP_THRESHOLD {
            return FieldLookup::Small(entries.into_iter().collect());
        }

        // Choose prefix length: 8 bytes if most keys are long, otherwise 4
        let long_key_count = entries.iter().filter(|e| e.name.len() >= 8).count();
        let prefix_len = if long_key_count > total_entries / 2 {
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

        let mut all_entries = Vec::with_capacity(total_entries);
        let mut bucket_data = Vec::with_capacity(bucket_list.len());

        for (prefix, bucket_entries) in bucket_list {
            let start = all_entries.len() as u32;
            let count = bucket_entries.len() as u32;
            bucket_data.push((prefix, start, count));
            all_entries.extend(bucket_entries);
        }

        // Allocate into arenas
        let entries_range = self.field_entries.alloc_extend(all_entries);
        let buckets_range = self.buckets.alloc_extend(bucket_data);

        FieldLookup::PrefixBuckets {
            prefix_len,
            entries: entries_range,
            buckets: buckets_range,
        }
    }

    /// Determine how to fill a field that wasn't set.
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
        FillRule::Required
    }

    /// Extract validators from field attributes.
    fn extract_validators(&mut self, field: &'static Field) -> ValidatorRange {
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

        self.validators.alloc_extend(validators)
    }

    /// Build an EnumPlan with all field plans for each variant.
    fn build_enum_plan(&mut self, enum_def: &'static EnumType) -> Result<EnumPlan, AllocError> {
        let mut variant_metas = Vec::with_capacity(enum_def.variants.len());

        for variant in enum_def.variants.iter() {
            let mut field_plans = Vec::with_capacity(variant.data.fields.len());

            for (index, field) in variant.data.fields.iter().enumerate() {
                // Build the type plan node for this field
                let field_proxy = field.effective_proxy(self.format_namespace);
                let child_node = self.build_node_with_proxy(field.shape(), field_proxy)?;

                // Build validators and fill rule (enums don't have container-level default)
                let validators = self.extract_validators(field);
                let fill_rule = Self::determine_fill_rule(field, false);

                // Create unified field plan
                field_plans
                    .push(self.create_field_plan(index, field, child_node, fill_rule, validators));
            }

            let has_flatten = field_plans.iter().any(|f| f.is_flattened);
            let fields = self.fields.alloc_extend(field_plans.iter().cloned());
            let field_lookup = self.build_field_lookup(&field_plans);

            variant_metas.push(VariantPlanMeta {
                variant,
                name: variant.effective_name(),
                fields,
                field_lookup,
                has_flatten,
            });
        }

        let variants = self.variants.alloc_extend(variant_metas.iter().cloned());
        let variant_lookup = self.build_variant_lookup(&variant_metas);
        let num_variants = variant_metas.len();

        // Find the index of the #[facet(other)] variant, if any
        let other_variant_idx = variant_metas.iter().position(|v| v.variant.is_other());

        Ok(EnumPlan {
            enum_def,
            variants,
            variant_lookup,
            num_variants,
            other_variant_idx,
        })
    }

    /// Build a variant lookup from variant metadata.
    fn build_variant_lookup(&self, variants: &[VariantPlanMeta]) -> VariantLookup {
        let entries: Vec<_> = variants
            .iter()
            .enumerate()
            .map(|(i, v)| (v.name, i))
            .collect();

        if entries.len() <= LOOKUP_THRESHOLD {
            VariantLookup::Small(entries.into_iter().collect())
        } else {
            let mut sorted = entries;
            sorted.sort_by_key(|(name, _)| *name);
            VariantLookup::Sorted(sorted)
        }
    }
}

impl<'facet, T: facet_core::Facet<'facet> + ?Sized> TypePlan<T> {
    /// Build a TypePlan for type `T`.
    ///
    /// The type parameter provides compile-time safety: you cannot accidentally
    /// pass a `TypePlan<Foo>` where `TypePlan<Bar>` is expected.
    ///
    /// Note: TypePlan can be built for any `Facet<'_>` type because the `SHAPE`
    /// is always `'static`. The lifetime parameter on `Facet` only affects the
    /// runtime deserialized values, not the type metadata.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use facet_reflect::TypePlan;
    ///
    /// let plan = TypePlan::<MyStruct>::build()?;
    /// ```
    pub fn build() -> Result<Self, AllocError> {
        Self::build_for_format(None)
    }

    /// Build a TypePlan with format-specific proxy resolution.
    ///
    /// The `format_namespace` (e.g., `Some("json")`, `Some("xml")`) is used to resolve
    /// format-specific proxies like `#[facet(json::proxy = ...)]`.
    pub fn build_for_format(format_namespace: Option<&'static str>) -> Result<Self, AllocError> {
        let mut builder = TypePlanBuilder::new(format_namespace);
        let root = builder.build_node(T::SHAPE)?;
        let core = Arc::new(builder.finish(root));

        Ok(TypePlan {
            core,
            _marker: core::marker::PhantomData,
        })
    }

    /// Get a reference to the internal core.
    #[inline]
    pub fn core(&self) -> Arc<TypePlanCore> {
        self.core.clone()
    }

    /// Get the root node.
    #[inline]
    pub fn root(&self) -> &TypePlanNode {
        self.core.root()
    }
}

impl TypePlanCore {
    /// Get the root node.
    #[inline]
    pub fn root(&self) -> &TypePlanNode {
        self.node(self.root)
    }

    /// Get the root node index.
    #[inline]
    pub fn root_id(&self) -> NodeId {
        self.root
    }

    /// Get a node by its index.
    #[inline]
    pub fn node(&self, idx: NodeId) -> &TypePlanNode {
        self.nodes.get(idx)
    }

    /// Get a field by its index.
    #[inline]
    pub fn field(&self, idx: Idx<FieldPlan>) -> &FieldPlan {
        self.fields.get(idx)
    }

    /// Get a slice of fields from a range.
    #[inline]
    pub fn fields(&self, range: FieldRange) -> &[FieldPlan] {
        self.fields.get_slice(range)
    }

    /// Get a variant by its index.
    #[inline]
    pub fn variant(&self, idx: Idx<VariantPlanMeta>) -> &VariantPlanMeta {
        self.variants.get(idx)
    }

    /// Get a slice of variants from a range.
    #[inline]
    pub fn variants(&self, range: VariantRange) -> &[VariantPlanMeta] {
        self.variants.get_slice(range)
    }

    /// Get a slice of validators from a range.
    #[inline]
    pub fn validators(&self, range: ValidatorRange) -> &[PrecomputedValidator] {
        self.validators.get_slice(range)
    }

    /// Look up a node by TypeId using binary search on the sorted lookup table.
    #[inline]
    fn lookup_node(&self, type_id: &ConstTypeId) -> Option<NodeId> {
        let idx = self
            .node_lookup
            .binary_search_by_key(type_id, |(id, _)| *id)
            .ok()?;
        Some(self.node_lookup[idx].1)
    }

    // Navigation helpers

    /// Get the child node for a struct field by index.
    /// Follows BackRef nodes for recursive types.
    #[inline]
    pub fn struct_field_node(&self, parent: &TypePlanNode, idx: usize) -> Option<&TypePlanNode> {
        let resolved = self.resolve_backref(parent);
        let struct_plan = match &resolved.kind {
            TypePlanNodeKind::Struct(p) => p,
            _ => return None,
        };
        let fields = self.fields(struct_plan.fields);
        Some(self.node(fields.get(idx)?.type_node))
    }

    /// Get the child node for an enum variant's field.
    /// Follows BackRef nodes for recursive types.
    #[inline]
    pub fn enum_variant_field_node(
        &self,
        parent: &TypePlanNode,
        variant_idx: usize,
        field_idx: usize,
    ) -> Option<&TypePlanNode> {
        let resolved = self.resolve_backref(parent);
        let enum_plan = match &resolved.kind {
            TypePlanNodeKind::Enum(p) => p,
            _ => return None,
        };
        let variants = self.variants(enum_plan.variants);
        let variant = variants.get(variant_idx)?;
        let fields = self.fields(variant.fields);
        Some(self.node(fields.get(field_idx)?.type_node))
    }

    /// Get the child node for list/array items.
    #[inline]
    pub fn list_item_node(&self, parent: &TypePlanNode) -> Option<&TypePlanNode> {
        match &parent.strategy {
            DeserStrategy::List { item_node, .. } | DeserStrategy::Array { item_node, .. } => {
                Some(self.node(*item_node))
            }
            DeserStrategy::BackRef { target_type_id } => {
                let target = self.lookup_node(target_type_id)?;
                self.list_item_node(self.node(target))
            }
            _ => None,
        }
    }

    /// Get the child node for set items.
    #[inline]
    pub fn set_item_node(&self, parent: &TypePlanNode) -> Option<&TypePlanNode> {
        match &parent.strategy {
            DeserStrategy::Set { item_node } => Some(self.node(*item_node)),
            DeserStrategy::BackRef { target_type_id } => {
                let target = self.lookup_node(target_type_id)?;
                self.set_item_node(self.node(target))
            }
            _ => None,
        }
    }

    /// Get the child node for map keys.
    #[inline]
    pub fn map_key_node(&self, parent: &TypePlanNode) -> Option<&TypePlanNode> {
        match &parent.strategy {
            DeserStrategy::Map { key_node, .. } => Some(self.node(*key_node)),
            DeserStrategy::BackRef { target_type_id } => {
                let target = self.lookup_node(target_type_id)?;
                self.map_key_node(self.node(target))
            }
            _ => None,
        }
    }

    /// Get the child node for map values.
    #[inline]
    pub fn map_value_node(&self, parent: &TypePlanNode) -> Option<&TypePlanNode> {
        match &parent.strategy {
            DeserStrategy::Map { value_node, .. } => Some(self.node(*value_node)),
            DeserStrategy::BackRef { target_type_id } => {
                let target = self.lookup_node(target_type_id)?;
                self.map_value_node(self.node(target))
            }
            _ => None,
        }
    }

    /// Get the child node for Option inner type.
    #[inline]
    pub fn option_inner_node(&self, parent: &TypePlanNode) -> Option<&TypePlanNode> {
        match &parent.strategy {
            DeserStrategy::Option { some_node } => Some(self.node(*some_node)),
            DeserStrategy::BackRef { target_type_id } => {
                let target = self.lookup_node(target_type_id)?;
                self.option_inner_node(self.node(target))
            }
            _ => None,
        }
    }

    /// Get the child node for Result Ok type.
    #[inline]
    pub fn result_ok_node(&self, parent: &TypePlanNode) -> Option<&TypePlanNode> {
        match &parent.strategy {
            DeserStrategy::Result { ok_node, .. } => Some(self.node(*ok_node)),
            DeserStrategy::BackRef { target_type_id } => {
                let target = self.lookup_node(target_type_id)?;
                self.result_ok_node(self.node(target))
            }
            _ => None,
        }
    }

    /// Get the child node for Result Err type.
    #[inline]
    pub fn result_err_node(&self, parent: &TypePlanNode) -> Option<&TypePlanNode> {
        match &parent.strategy {
            DeserStrategy::Result { err_node, .. } => Some(self.node(*err_node)),
            DeserStrategy::BackRef { target_type_id } => {
                let target = self.lookup_node(target_type_id)?;
                self.result_err_node(self.node(target))
            }
            _ => None,
        }
    }

    /// Get the child node for pointer pointee.
    #[inline]
    pub fn pointer_pointee_node(&self, parent: &TypePlanNode) -> Option<&TypePlanNode> {
        match &parent.strategy {
            DeserStrategy::Pointer { pointee_node } => Some(self.node(*pointee_node)),
            DeserStrategy::BackRef { target_type_id } => {
                let target = self.lookup_node(target_type_id)?;
                self.pointer_pointee_node(self.node(target))
            }
            _ => None,
        }
    }

    /// Get the child node for shape.inner navigation (used by begin_inner).
    #[inline]
    pub fn inner_node(&self, parent: &TypePlanNode) -> Option<&TypePlanNode> {
        if parent.shape.inner.is_some() {
            match &parent.strategy {
                DeserStrategy::TransparentConvert { inner_node } => Some(self.node(*inner_node)),
                _ => None,
            }
        } else {
            None
        }
    }

    /// Resolve a BackRef to get the actual node it points to.
    #[inline]
    pub fn resolve_backref<'a>(&'a self, node: &'a TypePlanNode) -> &'a TypePlanNode {
        match &node.kind {
            TypePlanNodeKind::BackRef(type_id) => self.node(
                self.lookup_node(type_id)
                    .expect("BackRef target must exist in node_lookup"),
            ),
            _ => node,
        }
    }

    /// Get the StructPlan if a node is a struct type.
    /// Follows BackRef nodes for recursive types.
    #[inline]
    pub fn as_struct_plan<'a>(&'a self, node: &'a TypePlanNode) -> Option<&'a StructPlan> {
        let resolved = self.resolve_backref(node);
        match &resolved.kind {
            TypePlanNodeKind::Struct(plan) => Some(plan),
            _ => None,
        }
    }

    /// Get the EnumPlan if a node is an enum type.
    /// Follows BackRef nodes for recursive types.
    #[inline]
    pub fn as_enum_plan<'a>(&'a self, node: &'a TypePlanNode) -> Option<&'a EnumPlan> {
        let resolved = self.resolve_backref(node);
        match &resolved.kind {
            TypePlanNodeKind::Enum(plan) => Some(plan),
            _ => None,
        }
    }

    /// Resolve a BackRef (by node ID) to get the actual node it points to.
    #[inline]
    pub fn resolve_backref_id(&self, node_id: NodeId) -> &TypePlanNode {
        let node = self.node(node_id);
        self.resolve_backref(node)
    }

    /// Get the StructPlan for a node ID, if it's a struct type.
    /// Follows BackRef nodes for recursive types.
    #[inline]
    pub fn struct_plan_by_id(&self, node_id: NodeId) -> Option<&StructPlan> {
        self.as_struct_plan(self.node(node_id))
    }

    /// Get the EnumPlan for a node ID, if it's an enum type.
    /// Follows BackRef nodes for recursive types.
    #[inline]
    pub fn enum_plan_by_id(&self, node_id: NodeId) -> Option<&EnumPlan> {
        self.as_enum_plan(self.node(node_id))
    }

    // Navigation helpers that work with NodeId (returning NodeId for child nodes)

    /// Get the child node ID for a struct field by index.
    /// Follows BackRef nodes for recursive types.
    #[inline]
    pub fn struct_field_node_id(&self, parent_id: NodeId, idx: usize) -> Option<NodeId> {
        let parent = self.node(parent_id);
        let resolved = self.resolve_backref(parent);
        let struct_plan = match &resolved.kind {
            TypePlanNodeKind::Struct(p) => p,
            _ => return None,
        };
        let fields = self.fields(struct_plan.fields);
        Some(fields.get(idx)?.type_node)
    }

    /// Get the child node ID for an enum variant's field.
    /// Follows BackRef nodes for recursive types.
    #[inline]
    pub fn enum_variant_field_node_id(
        &self,
        parent_id: NodeId,
        variant_idx: usize,
        field_idx: usize,
    ) -> Option<NodeId> {
        let parent = self.node(parent_id);
        let resolved = self.resolve_backref(parent);
        let enum_plan = match &resolved.kind {
            TypePlanNodeKind::Enum(p) => p,
            _ => return None,
        };
        let variants = self.variants(enum_plan.variants);
        let variant = variants.get(variant_idx)?;
        let fields = self.fields(variant.fields);
        Some(fields.get(field_idx)?.type_node)
    }

    /// Get the child node ID for list/array items.
    #[inline]
    pub fn list_item_node_id(&self, parent_id: NodeId) -> Option<NodeId> {
        let parent = self.node(parent_id);
        match &parent.strategy {
            DeserStrategy::List { item_node, .. } | DeserStrategy::Array { item_node, .. } => {
                Some(*item_node)
            }
            DeserStrategy::BackRef { target_type_id } => {
                let target = self.lookup_node(target_type_id)?;
                self.list_item_node_id(target)
            }
            _ => None,
        }
    }

    /// Get the child node ID for set items.
    #[inline]
    pub fn set_item_node_id(&self, parent_id: NodeId) -> Option<NodeId> {
        let parent = self.node(parent_id);
        match &parent.strategy {
            DeserStrategy::Set { item_node } => Some(*item_node),
            DeserStrategy::BackRef { target_type_id } => {
                let target = self.lookup_node(target_type_id)?;
                self.set_item_node_id(target)
            }
            _ => None,
        }
    }

    /// Get the child node ID for map keys.
    #[inline]
    pub fn map_key_node_id(&self, parent_id: NodeId) -> Option<NodeId> {
        let parent = self.node(parent_id);
        match &parent.strategy {
            DeserStrategy::Map { key_node, .. } => Some(*key_node),
            DeserStrategy::BackRef { target_type_id } => {
                let target = self.lookup_node(target_type_id)?;
                self.map_key_node_id(target)
            }
            _ => None,
        }
    }

    /// Get the child node ID for map values.
    #[inline]
    pub fn map_value_node_id(&self, parent_id: NodeId) -> Option<NodeId> {
        let parent = self.node(parent_id);
        match &parent.strategy {
            DeserStrategy::Map { value_node, .. } => Some(*value_node),
            DeserStrategy::BackRef { target_type_id } => {
                let target = self.lookup_node(target_type_id)?;
                self.map_value_node_id(target)
            }
            _ => None,
        }
    }

    /// Get the child node ID for Option's Some variant.
    #[inline]
    pub fn option_some_node_id(&self, parent_id: NodeId) -> Option<NodeId> {
        let parent = self.node(parent_id);
        match &parent.strategy {
            DeserStrategy::Option { some_node, .. } => Some(*some_node),
            DeserStrategy::BackRef { target_type_id } => {
                let target = self.lookup_node(target_type_id)?;
                self.option_some_node_id(target)
            }
            _ => None,
        }
    }

    /// Get the child node IDs for Result's Ok and Err variants.
    #[inline]
    pub fn result_nodes_id(&self, parent_id: NodeId) -> Option<(NodeId, NodeId)> {
        let parent = self.node(parent_id);
        match &parent.strategy {
            DeserStrategy::Result {
                ok_node, err_node, ..
            } => Some((*ok_node, *err_node)),
            DeserStrategy::BackRef { target_type_id } => {
                let target = self.lookup_node(target_type_id)?;
                self.result_nodes_id(target)
            }
            _ => None,
        }
    }

    /// Get the child node ID for smart pointer inner type.
    #[inline]
    pub fn pointer_inner_node_id(&self, parent_id: NodeId) -> Option<NodeId> {
        let parent = self.node(parent_id);
        match &parent.strategy {
            DeserStrategy::Pointer { pointee_node, .. } => Some(*pointee_node),
            DeserStrategy::BackRef { target_type_id } => {
                let target = self.lookup_node(target_type_id)?;
                self.pointer_inner_node_id(target)
            }
            _ => None,
        }
    }

    /// Get the child node ID for transparent convert inner type.
    #[inline]
    pub fn inner_node_id(&self, parent_id: NodeId) -> Option<NodeId> {
        let parent = self.node(parent_id);
        if parent.shape.inner.is_some() {
            match &parent.strategy {
                DeserStrategy::TransparentConvert { inner_node } => Some(*inner_node),
                _ => None,
            }
        } else {
            None
        }
    }

    pub(crate) fn empty() -> TypePlanCore {
        TypePlanCore {
            nodes: Arena::new(),
            fields: Arena::new(),
            variants: Arena::new(),
            validators: Arena::new(),
            field_entries: Arena::new(),
            buckets: Arena::new(),
            node_lookup: Vec::new(),
            root: NodeId::new(0),
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
        let plan = TypePlan::<TestStruct>::build().unwrap();
        let root = plan.root();
        let core = plan.core();

        assert_eq!(root.shape, TestStruct::SHAPE);
        assert!(!root.has_default);

        match &root.kind {
            TypePlanNodeKind::Struct(struct_plan) => {
                let fields = core.fields(struct_plan.fields);
                assert_eq!(fields.len(), 3);
                assert!(!struct_plan.has_flatten);

                // Check field lookup
                assert_eq!(struct_plan.field_lookup.find("name", core), Some(0));
                assert_eq!(struct_plan.field_lookup.find("age", core), Some(1));
                assert_eq!(struct_plan.field_lookup.find("email", core), Some(2));
                assert_eq!(struct_plan.field_lookup.find("unknown", core), None);

                // Check field metadata
                assert_eq!(fields[0].name, "name");
                assert!(fields[0].is_required());

                assert_eq!(fields[1].name, "age");
                assert!(fields[1].is_required());

                assert_eq!(fields[2].name, "email");
                assert!(!fields[2].is_required()); // Option has implicit default

                // Check child plan for Option field (field index 2 = third child)
                let email_node = core.struct_field_node(plan.root(), 2).unwrap();
                match &email_node.kind {
                    TypePlanNodeKind::Option => {
                        // inner should be String (scalar)
                        let inner_node = core.option_inner_node(email_node).unwrap();
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
        let plan = TypePlan::<TestEnum>::build().unwrap();
        let root = plan.root();
        let core = plan.core();

        assert_eq!(root.shape, TestEnum::SHAPE);

        match &root.kind {
            TypePlanNodeKind::Enum(enum_plan) => {
                let variants = core.variants(enum_plan.variants);
                assert_eq!(enum_plan.num_variants, 3);

                // Check variant lookup
                assert_eq!(enum_plan.variant_lookup.find("Unit"), Some(0));
                assert_eq!(enum_plan.variant_lookup.find("Tuple"), Some(1));
                assert_eq!(enum_plan.variant_lookup.find("Struct"), Some(2));
                assert_eq!(enum_plan.variant_lookup.find("Unknown"), None);

                // Unit variant has no fields
                assert!(core.fields(variants[0].fields).is_empty());

                // Tuple variant has 1 field
                assert_eq!(core.fields(variants[1].fields).len(), 1);

                // Struct variant has 1 field
                let struct_variant_fields = core.fields(variants[2].fields);
                assert_eq!(struct_variant_fields.len(), 1);
                assert_eq!(variants[2].field_lookup.find("value", core), Some(0));
            }
            other => panic!("Expected Enum, got {:?}", other),
        }
    }

    #[test]
    fn test_typeplan_list() {
        let plan = TypePlan::<Vec<u32>>::build().unwrap();
        let root = plan.root();
        let core = plan.core();

        match &root.kind {
            TypePlanNodeKind::List => {
                let item_node = core.list_item_node(plan.root()).unwrap();
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
        // This should NOT stack overflow - arena handles the cycle
        let plan = TypePlan::<RecursiveStruct>::build().unwrap();
        let root = plan.root();
        let core = plan.core();

        match &root.kind {
            TypePlanNodeKind::Struct(struct_plan) => {
                let fields = core.fields(struct_plan.fields);
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "value");
                assert_eq!(fields[1].name, "next");

                // The 'next' field is Option<Box<RecursiveStruct>>
                // Its child plan should eventually contain a BackRef
                let next_node = core.struct_field_node(plan.root(), 1).unwrap();

                // Should be Option
                assert!(matches!(next_node.kind, TypePlanNodeKind::Option));

                // Inner should be Pointer (Box)
                let inner_node = core.option_inner_node(next_node).unwrap();
                assert!(matches!(inner_node.kind, TypePlanNodeKind::Pointer));

                // Pointee should be BackRef to root (or a struct with BackRef)
                let pointee_node = core.pointer_pointee_node(inner_node).unwrap();

                // This should be a BackRef pointing to the root
                match &pointee_node.kind {
                    TypePlanNodeKind::BackRef(type_id) => {
                        // type_id should match the root's type
                        assert_eq!(type_id, &plan.root().shape.id);
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
        let lookup = FieldLookup::Small(smallvec::smallvec![
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

        assert_eq!(lookup.find_small("foo"), Some(0));
        assert_eq!(lookup.find_small("bar"), Some(1));
        assert_eq!(lookup.find_small("baz"), Some(2));
        assert_eq!(lookup.find_small("qux"), None);
    }

    #[test]
    fn test_variant_lookup_small() {
        let lookup = VariantLookup::Small(smallvec::smallvec![("A", 0), ("B", 1), ("C", 2)]);

        assert_eq!(lookup.find("A"), Some(0));
        assert_eq!(lookup.find("B"), Some(1));
        assert_eq!(lookup.find("C"), Some(2));
        assert_eq!(lookup.find("D"), None);
    }
}
