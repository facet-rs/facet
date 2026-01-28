//! TypePlan: Precomputed deserialization plans for types.
//!
//! Instead of repeatedly inspecting Shape/Def at runtime during deserialization,
//! we build a plan tree once that encodes all the decisions we'll make.

use alloc::{boxed::Box, vec::Vec};
use facet_core::{
    Characteristic, Def, EnumType, Field, Shape, StructType, Type, UserType, Variant,
};

/// Precomputed deserialization plan for a type.
///
/// Built once from a Shape, this encodes all decisions needed during deserialization
/// without repeated runtime lookups.
#[derive(Debug)]
pub struct TypePlan {
    /// The shape this plan was built from
    pub shape: &'static Shape,
    /// What kind of type this is and how to deserialize it
    pub kind: TypePlanKind,
    /// Whether this type has a Default implementation
    pub has_default: bool,
}

/// The specific kind of type and its deserialization strategy.
#[derive(Debug)]
pub enum TypePlanKind {
    /// Scalar types (integers, floats, bool, char, strings)
    Scalar,

    /// Struct types with named or positional fields
    Struct(StructPlan),

    /// Enum types with variants
    Enum(EnumPlan),

    /// `Option<T>` - special handling for None/Some
    Option {
        /// Plan for the inner type T
        inner: Box<TypePlan>,
    },

    /// `Result<T, E>` - special handling for Ok/Err
    Result {
        /// Plan for the Ok type T
        ok: Box<TypePlan>,
        /// Plan for the Err type E
        err: Box<TypePlan>,
    },

    /// `Vec<T>`, `VecDeque<T>`, etc.
    List {
        /// Plan for the item type T
        item: Box<TypePlan>,
    },

    /// `HashMap<K, V>`, `BTreeMap<K, V>`, etc.
    Map {
        /// Plan for the key type K
        key: Box<TypePlan>,
        /// Plan for the value type V
        value: Box<TypePlan>,
    },

    /// `HashSet<T>`, `BTreeSet<T>`, etc.
    Set {
        /// Plan for the item type T
        item: Box<TypePlan>,
    },

    /// Fixed-size arrays `[T; N]`
    Array {
        /// Plan for the item type T
        item: Box<TypePlan>,
        /// Array length N
        len: usize,
    },

    /// Smart pointers: `Box<T>`, `Arc<T>`, `Rc<T>`
    Pointer {
        /// Plan for the pointee type T
        pointee: Box<TypePlan>,
    },

    /// Transparent wrappers (newtypes)
    Transparent {
        /// Plan for the inner type
        inner: Box<TypePlan>,
    },

    /// Unknown/unsupported type - fall back to runtime dispatch
    Unknown,
}

/// Precomputed plan for struct deserialization.
#[derive(Debug)]
pub struct StructPlan {
    /// Reference to the struct type definition
    pub struct_def: &'static StructType,
    /// Plans for each field, indexed by field position
    pub fields: Vec<FieldPlan>,
    /// Fast field lookup by name
    pub field_lookup: FieldLookup,
    /// Whether any field has #[facet(flatten)]
    pub has_flatten: bool,
    /// Number of fields
    pub num_fields: usize,
}

/// Precomputed plan for a single field.
#[derive(Debug)]
pub struct FieldPlan {
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
    /// Plan for the field's type
    pub child_plan: Box<TypePlan>,
}

/// Precomputed plan for enum deserialization.
#[derive(Debug)]
pub struct EnumPlan {
    /// Reference to the enum type definition
    pub enum_def: &'static EnumType,
    /// Plans for each variant
    pub variants: Vec<VariantPlan>,
    /// Fast variant lookup by name
    pub variant_lookup: VariantLookup,
    /// Number of variants
    pub num_variants: usize,
}

/// Precomputed plan for a single enum variant.
#[derive(Debug)]
pub struct VariantPlan {
    /// Reference to the variant definition
    pub variant: &'static Variant,
    /// Variant name
    pub name: &'static str,
    /// Plans for variant fields (if any)
    pub fields: Vec<FieldPlan>,
    /// Fast field lookup for this variant
    pub field_lookup: FieldLookup,
    /// Number of fields in this variant
    pub num_fields: usize,
}

/// Fast lookup from field name to field index.
///
/// Uses different strategies based on field count:
/// - Small (≤8 fields): linear scan (cache-friendly, no hashing overhead)
/// - Large (>8 fields): sorted array with binary search
#[derive(Debug)]
pub enum FieldLookup {
    /// For small structs: just store (name, index) pairs
    Small(Vec<FieldLookupEntry>),
    /// For larger structs: sorted by name for binary search
    Sorted(Vec<FieldLookupEntry>),
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
#[derive(Debug)]
pub enum VariantLookup {
    /// For small enums: linear scan
    Small(Vec<(&'static str, usize)>),
    /// For larger enums: sorted for binary search
    Sorted(Vec<(&'static str, usize)>),
}

// Threshold for switching from linear to sorted lookup
const LOOKUP_THRESHOLD: usize = 8;

impl FieldLookup {
    /// Create a new field lookup from field plans.
    pub fn new(fields: &[FieldPlan]) -> Self {
        let mut entries = Vec::with_capacity(fields.len() * 2); // room for aliases

        for (index, field_plan) in fields.iter().enumerate() {
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

        if entries.len() <= LOOKUP_THRESHOLD {
            FieldLookup::Small(entries)
        } else {
            // Sort by name for binary search
            entries.sort_by_key(|e| e.name);
            FieldLookup::Sorted(entries)
        }
    }

    /// Find a field index by name.
    #[inline]
    pub fn find(&self, name: &str) -> Option<usize> {
        match self {
            FieldLookup::Small(entries) => entries.iter().find(|e| e.name == name).map(|e| e.index),
            FieldLookup::Sorted(entries) => entries
                .binary_search_by_key(&name, |e| e.name)
                .ok()
                .map(|i| entries[i].index),
        }
    }

    /// Check if empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        match self {
            FieldLookup::Small(entries) | FieldLookup::Sorted(entries) => entries.is_empty(),
        }
    }
}

impl VariantLookup {
    /// Create a new variant lookup from variants.
    pub fn new(variants: &[VariantPlan]) -> Self {
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

impl TypePlan {
    /// Build a TypePlan from a Shape.
    ///
    /// This recursively builds plans for nested types.
    pub fn build(shape: &'static Shape) -> Self {
        let has_default = shape.is(Characteristic::Default);

        // Check shape.def first - this tells us the semantic meaning of the type
        let kind = match &shape.def {
            Def::Scalar => TypePlanKind::Scalar,

            Def::Option(opt_def) => TypePlanKind::Option {
                inner: Box::new(Self::build(opt_def.t())),
            },

            Def::Result(res_def) => TypePlanKind::Result {
                ok: Box::new(Self::build(res_def.t())),
                err: Box::new(Self::build(res_def.e())),
            },

            Def::List(list_def) => TypePlanKind::List {
                item: Box::new(Self::build(list_def.t())),
            },

            Def::Map(map_def) => TypePlanKind::Map {
                key: Box::new(Self::build(map_def.k())),
                value: Box::new(Self::build(map_def.v())),
            },

            Def::Set(set_def) => TypePlanKind::Set {
                item: Box::new(Self::build(set_def.t())),
            },

            Def::Array(arr_def) => TypePlanKind::Array {
                item: Box::new(Self::build(arr_def.t())),
                len: arr_def.n,
            },

            Def::Pointer(ptr_def) => {
                if let Some(pointee) = ptr_def.pointee() {
                    TypePlanKind::Pointer {
                        pointee: Box::new(Self::build(pointee)),
                    }
                } else {
                    TypePlanKind::Unknown
                }
            }

            _ => {
                // Check Type for struct/enum
                match &shape.ty {
                    Type::User(UserType::Struct(struct_type)) => {
                        TypePlanKind::Struct(StructPlan::build(struct_type))
                    }
                    Type::User(UserType::Enum(enum_type)) => {
                        TypePlanKind::Enum(EnumPlan::build(enum_type))
                    }
                    _ => {
                        // Check for transparent wrappers (newtypes) as fallback
                        if let Some(inner) = shape.inner {
                            TypePlanKind::Transparent {
                                inner: Box::new(Self::build(inner)),
                            }
                        } else {
                            TypePlanKind::Unknown
                        }
                    }
                }
            }
        };

        TypePlan {
            shape,
            kind,
            has_default,
        }
    }
}

impl StructPlan {
    /// Build a StructPlan from a StructType.
    pub fn build(struct_def: &'static StructType) -> Self {
        let fields: Vec<_> = struct_def.fields.iter().map(FieldPlan::build).collect();

        let has_flatten = fields.iter().any(|f| f.is_flattened);
        let field_lookup = FieldLookup::new(&fields);
        let num_fields = fields.len();

        StructPlan {
            struct_def,
            fields,
            field_lookup,
            has_flatten,
            num_fields,
        }
    }
}

impl FieldPlan {
    /// Build a FieldPlan from a Field.
    pub fn build(field: &'static Field) -> Self {
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

        let child_plan = Box::new(TypePlan::build(field.shape()));

        FieldPlan {
            field,
            name,
            effective_name,
            alias,
            has_default,
            is_required,
            is_flattened,
            child_plan,
        }
    }
}

impl EnumPlan {
    /// Build an EnumPlan from an EnumType.
    pub fn build(enum_def: &'static EnumType) -> Self {
        let variants: Vec<_> = enum_def.variants.iter().map(VariantPlan::build).collect();

        let variant_lookup = VariantLookup::new(&variants);
        let num_variants = variants.len();

        EnumPlan {
            enum_def,
            variants,
            variant_lookup,
            num_variants,
        }
    }
}

impl VariantPlan {
    /// Build a VariantPlan from a Variant.
    pub fn build(variant: &'static Variant) -> Self {
        let name = variant.name;

        let fields: Vec<_> = variant.data.fields.iter().map(FieldPlan::build).collect();

        let field_lookup = FieldLookup::new(&fields);
        let num_fields = fields.len();

        VariantPlan {
            variant,
            name,
            fields,
            field_lookup,
            num_fields,
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

    #[test]
    fn test_typeplan_struct() {
        let plan = TypePlan::build(TestStruct::SHAPE);

        assert_eq!(plan.shape, TestStruct::SHAPE);
        assert!(!plan.has_default);

        match &plan.kind {
            TypePlanKind::Struct(struct_plan) => {
                assert_eq!(struct_plan.num_fields, 3);
                assert!(!struct_plan.has_flatten);

                // Check field lookup
                assert_eq!(struct_plan.field_lookup.find("name"), Some(0));
                assert_eq!(struct_plan.field_lookup.find("age"), Some(1));
                assert_eq!(struct_plan.field_lookup.find("email"), Some(2));
                assert_eq!(struct_plan.field_lookup.find("unknown"), None);

                // Check field plans
                assert_eq!(struct_plan.fields[0].name, "name");
                assert!(struct_plan.fields[0].is_required);

                assert_eq!(struct_plan.fields[1].name, "age");
                assert!(struct_plan.fields[1].is_required);

                assert_eq!(struct_plan.fields[2].name, "email");
                assert!(!struct_plan.fields[2].is_required); // Option has implicit default

                // Check child plan for Option field
                match &struct_plan.fields[2].child_plan.kind {
                    TypePlanKind::Option { inner } => {
                        // inner should be String
                        match &inner.kind {
                            TypePlanKind::Scalar => {}
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
        let plan = TypePlan::build(TestEnum::SHAPE);

        assert_eq!(plan.shape, TestEnum::SHAPE);

        match &plan.kind {
            TypePlanKind::Enum(enum_plan) => {
                assert_eq!(enum_plan.num_variants, 3);

                // Check variant lookup
                assert_eq!(enum_plan.variant_lookup.find("Unit"), Some(0));
                assert_eq!(enum_plan.variant_lookup.find("Tuple"), Some(1));
                assert_eq!(enum_plan.variant_lookup.find("Struct"), Some(2));
                assert_eq!(enum_plan.variant_lookup.find("Unknown"), None);

                // Unit variant has no fields
                assert_eq!(enum_plan.variants[0].num_fields, 0);

                // Tuple variant has 1 field
                assert_eq!(enum_plan.variants[1].num_fields, 1);

                // Struct variant has 1 field
                assert_eq!(enum_plan.variants[2].num_fields, 1);
                assert_eq!(enum_plan.variants[2].field_lookup.find("value"), Some(0));
            }
            other => panic!("Expected Enum, got {:?}", other),
        }
    }

    #[test]
    fn test_typeplan_list() {
        let plan = TypePlan::build(<Vec<u32> as Facet>::SHAPE);

        match &plan.kind {
            TypePlanKind::List { item } => match &item.kind {
                TypePlanKind::Scalar => {}
                other => panic!("Expected Scalar for u32, got {:?}", other),
            },
            other => panic!("Expected List, got {:?}", other),
        }
    }

    #[test]
    fn test_field_lookup_small() {
        // Create some mock field plans for testing lookup
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
    fn test_field_lookup_sorted() {
        let lookup = FieldLookup::Sorted(vec![
            FieldLookupEntry {
                name: "alpha",
                index: 0,
                is_alias: false,
            },
            FieldLookupEntry {
                name: "beta",
                index: 1,
                is_alias: false,
            },
            FieldLookupEntry {
                name: "gamma",
                index: 2,
                is_alias: false,
            },
        ]);

        assert_eq!(lookup.find("alpha"), Some(0));
        assert_eq!(lookup.find("beta"), Some(1));
        assert_eq!(lookup.find("gamma"), Some(2));
        assert_eq!(lookup.find("delta"), None);
    }

    #[test]
    fn test_variant_lookup_small() {
        let lookup = VariantLookup::Small(vec![("A", 0), ("B", 1), ("C", 2)]);

        assert_eq!(lookup.find("A"), Some(0));
        assert_eq!(lookup.find("B"), Some(1));
        assert_eq!(lookup.find("C"), Some(2));
        assert_eq!(lookup.find("D"), None);
    }
}
