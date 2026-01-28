//! TypePlan: Precomputed deserialization plans for types.
//!
//! Instead of repeatedly inspecting Shape/Def at runtime during deserialization,
//! we build a plan tree once that encodes all the decisions we'll make.
//!
//! Uses indextree for arena-based allocation, which naturally handles recursive
//! types by storing NodeId back-references instead of causing infinite recursion.

use alloc::vec::Vec;
use facet_core::{
    Characteristic, ConstTypeId, Def, EnumType, Field, Shape, StructType, Type, UserType, Variant,
};
use hashbrown::HashMap;
use indextree::Arena;

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
    /// Whether this type has a Default implementation
    pub has_default: bool,
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

    /// Transparent wrappers (newtypes)
    /// Child: inner type
    Transparent,

    /// Back-reference to an ancestor node (for recursive types)
    BackRef(NodeId),

    /// Unknown/unsupported type - fall back to runtime dispatch
    Unknown,
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
    /// Number of fields
    pub num_fields: usize,
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
    /// Number of fields in this variant
    pub num_fields: usize,
}

/// Fast lookup from field name to field index.
///
/// Uses different strategies based on field count:
/// - Small (≤8 fields): linear scan (cache-friendly, no hashing overhead)
/// - Large (>8 fields): sorted array with binary search
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
pub enum VariantLookup {
    /// For small enums: linear scan
    Small(Vec<(&'static str, usize)>),
    /// For larger enums: sorted for binary search
    Sorted(Vec<(&'static str, usize)>),
}

// Threshold for switching from linear to sorted lookup
const LOOKUP_THRESHOLD: usize = 8;

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

        if entries.len() <= LOOKUP_THRESHOLD {
            FieldLookup::Small(entries)
        } else {
            // Sort by name for binary search
            entries.sort_by_key(|e| e.name);
            FieldLookup::Sorted(entries)
        }
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

        if entries.len() <= LOOKUP_THRESHOLD {
            FieldLookup::Small(entries)
        } else {
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
}

impl TypePlanBuilder {
    fn new() -> Self {
        Self {
            arena: Arena::new(),
            building: HashMap::new(),
        }
    }

    /// Build a node for a shape, returning its NodeId.
    /// Handles cycles by returning BackRef when we detect recursion.
    fn build_node(&mut self, shape: &'static Shape) -> NodeId {
        let type_id = shape.id;

        // Check if we're already building this type (cycle detected)
        if let Some(&existing_id) = self.building.get(&type_id) {
            // Create a BackRef node pointing to the existing node
            let backref_node = TypePlanNode {
                shape,
                kind: TypePlanNodeKind::BackRef(existing_id),
                has_default: shape.is(Characteristic::Default),
            };
            return self.arena.new_node(backref_node);
        }

        // Create placeholder node first so children can reference it
        let placeholder = TypePlanNode {
            shape,
            kind: TypePlanNodeKind::Unknown, // Will be replaced
            has_default: shape.is(Characteristic::Default),
        };
        let node_id = self.arena.new_node(placeholder);

        // Mark this type as being built
        self.building.insert(type_id, node_id);

        // Build the actual kind and children
        let kind = self.build_kind(shape, node_id);

        // Update the node with the real kind
        self.arena.get_mut(node_id).unwrap().get_mut().kind = kind;

        // Remove from building set - we're done with this type
        // This ensures only ancestors are tracked, not all visited types
        self.building.remove(&type_id);

        node_id
    }

    /// Build the TypePlanNodeKind for a shape, attaching children to parent_id.
    fn build_kind(&mut self, shape: &'static Shape, parent_id: NodeId) -> TypePlanNodeKind {
        // Check shape.def first - this tells us the semantic meaning of the type
        match &shape.def {
            Def::Scalar => TypePlanNodeKind::Scalar,

            Def::Option(opt_def) => {
                let inner_id = self.build_node(opt_def.t());
                parent_id.append(inner_id, &mut self.arena);
                TypePlanNodeKind::Option
            }

            Def::Result(res_def) => {
                let ok_id = self.build_node(res_def.t());
                let err_id = self.build_node(res_def.e());
                parent_id.append(ok_id, &mut self.arena);
                parent_id.append(err_id, &mut self.arena);
                TypePlanNodeKind::Result
            }

            Def::List(list_def) => {
                let item_id = self.build_node(list_def.t());
                parent_id.append(item_id, &mut self.arena);
                TypePlanNodeKind::List
            }

            Def::Map(map_def) => {
                let key_id = self.build_node(map_def.k());
                let value_id = self.build_node(map_def.v());
                parent_id.append(key_id, &mut self.arena);
                parent_id.append(value_id, &mut self.arena);
                TypePlanNodeKind::Map
            }

            Def::Set(set_def) => {
                let item_id = self.build_node(set_def.t());
                parent_id.append(item_id, &mut self.arena);
                TypePlanNodeKind::Set
            }

            Def::Array(arr_def) => {
                let item_id = self.build_node(arr_def.t());
                parent_id.append(item_id, &mut self.arena);
                TypePlanNodeKind::Array { len: arr_def.n }
            }

            Def::Pointer(ptr_def) => {
                if let Some(pointee) = ptr_def.pointee() {
                    let pointee_id = self.build_node(pointee);
                    parent_id.append(pointee_id, &mut self.arena);
                    TypePlanNodeKind::Pointer
                } else {
                    TypePlanNodeKind::Unknown
                }
            }

            _ => {
                // Check Type for struct/enum
                match &shape.ty {
                    Type::User(UserType::Struct(struct_type)) => {
                        TypePlanNodeKind::Struct(self.build_struct_plan(struct_type, parent_id))
                    }
                    Type::User(UserType::Enum(enum_type)) => {
                        TypePlanNodeKind::Enum(self.build_enum_plan(enum_type, parent_id))
                    }
                    _ => {
                        // Check for transparent wrappers (newtypes) as fallback
                        if let Some(inner) = shape.inner {
                            let inner_id = self.build_node(inner);
                            parent_id.append(inner_id, &mut self.arena);
                            TypePlanNodeKind::Transparent
                        } else {
                            TypePlanNodeKind::Unknown
                        }
                    }
                }
            }
        }
    }

    /// Build a StructPlan, attaching field children to parent_id.
    fn build_struct_plan(
        &mut self,
        struct_def: &'static StructType,
        parent_id: NodeId,
    ) -> StructPlan {
        let mut fields = Vec::with_capacity(struct_def.fields.len());

        for field in struct_def.fields.iter() {
            let field_meta = FieldPlanMeta::from_field(field);
            fields.push(field_meta);

            // Build child plan for this field and attach to parent
            let child_id = self.build_node(field.shape());
            parent_id.append(child_id, &mut self.arena);
        }

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

    /// Build an EnumPlan, attaching variant field children appropriately.
    fn build_enum_plan(&mut self, enum_def: &'static EnumType, parent_id: NodeId) -> EnumPlan {
        let mut variants = Vec::with_capacity(enum_def.variants.len());

        for variant in enum_def.variants.iter() {
            let mut variant_fields = Vec::with_capacity(variant.data.fields.len());

            for field in variant.data.fields.iter() {
                let field_meta = FieldPlanMeta::from_field(field);
                variant_fields.push(field_meta);

                // Build child plan for this field and attach to parent
                let child_id = self.build_node(field.shape());
                parent_id.append(child_id, &mut self.arena);
            }

            let field_lookup = FieldLookup::new(&variant_fields);
            let num_fields = variant_fields.len();

            variants.push(VariantPlanMeta {
                variant,
                name: variant.name,
                fields: variant_fields,
                field_lookup,
                num_fields,
            });
        }

        let variant_lookup = VariantLookup::new(&variants);
        let num_variants = variants.len();

        EnumPlan {
            enum_def,
            variants,
            variant_lookup,
            num_variants,
        }
    }

    fn finish(self, root: NodeId) -> TypePlan {
        TypePlan {
            arena: self.arena,
            root,
        }
    }
}

impl FieldPlanMeta {
    /// Build field metadata from a Field.
    fn from_field(field: &'static Field) -> Self {
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
        }
    }
}

impl TypePlan {
    /// Build a TypePlan from a Shape.
    ///
    /// This recursively builds plans for nested types, using arena allocation
    /// to handle recursive types without stack overflow.
    pub fn build(shape: &'static Shape) -> Self {
        let mut builder = TypePlanBuilder::new();
        let root = builder.build_node(shape);
        builder.finish(root)
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
    #[inline]
    pub fn struct_field_node(&self, parent: NodeId, idx: usize) -> Option<NodeId> {
        let node = self.get(parent)?;
        if !matches!(node.kind, TypePlanNodeKind::Struct(_)) {
            return None;
        }
        self.nth_child(parent, idx)
    }

    /// Get the child NodeId for an enum variant's field.
    /// For enums, children are laid out as: [variant0_field0, variant0_field1, ..., variant1_field0, ...]
    #[inline]
    pub fn enum_variant_field_node(
        &self,
        parent: NodeId,
        variant_idx: usize,
        field_idx: usize,
    ) -> Option<NodeId> {
        let node = self.get(parent)?;
        let enum_plan = match &node.kind {
            TypePlanNodeKind::Enum(p) => p,
            _ => return None,
        };

        // Calculate the child offset
        let mut offset = 0;
        for i in 0..variant_idx {
            offset += enum_plan.variants.get(i)?.num_fields;
        }
        offset += field_idx;

        self.nth_child(parent, offset)
    }

    /// Get the child NodeId for list/array items (first child).
    #[inline]
    pub fn list_item_node(&self, parent: NodeId) -> Option<NodeId> {
        let node = self.get(parent)?;
        match &node.kind {
            TypePlanNodeKind::List | TypePlanNodeKind::Array { .. } => self.first_child(parent),
            _ => None,
        }
    }

    /// Get the child NodeId for set items (first child).
    #[inline]
    pub fn set_item_node(&self, parent: NodeId) -> Option<NodeId> {
        let node = self.get(parent)?;
        match &node.kind {
            TypePlanNodeKind::Set => self.first_child(parent),
            _ => None,
        }
    }

    /// Get the child NodeId for map keys (first child).
    #[inline]
    pub fn map_key_node(&self, parent: NodeId) -> Option<NodeId> {
        let node = self.get(parent)?;
        match &node.kind {
            TypePlanNodeKind::Map => self.first_child(parent),
            _ => None,
        }
    }

    /// Get the child NodeId for map values (second child).
    #[inline]
    pub fn map_value_node(&self, parent: NodeId) -> Option<NodeId> {
        let node = self.get(parent)?;
        match &node.kind {
            TypePlanNodeKind::Map => self.nth_child(parent, 1),
            _ => None,
        }
    }

    /// Get the child NodeId for Option inner type (first child).
    #[inline]
    pub fn option_inner_node(&self, parent: NodeId) -> Option<NodeId> {
        let node = self.get(parent)?;
        match &node.kind {
            TypePlanNodeKind::Option => self.first_child(parent),
            _ => None,
        }
    }

    /// Get the child NodeId for Result Ok type (first child).
    #[inline]
    pub fn result_ok_node(&self, parent: NodeId) -> Option<NodeId> {
        let node = self.get(parent)?;
        match &node.kind {
            TypePlanNodeKind::Result => self.first_child(parent),
            _ => None,
        }
    }

    /// Get the child NodeId for Result Err type (second child).
    #[inline]
    pub fn result_err_node(&self, parent: NodeId) -> Option<NodeId> {
        let node = self.get(parent)?;
        match &node.kind {
            TypePlanNodeKind::Result => self.nth_child(parent, 1),
            _ => None,
        }
    }

    /// Get the child NodeId for pointer pointee (first child).
    #[inline]
    pub fn pointer_pointee_node(&self, parent: NodeId) -> Option<NodeId> {
        let node = self.get(parent)?;
        match &node.kind {
            TypePlanNodeKind::Pointer => self.first_child(parent),
            _ => None,
        }
    }

    /// Get the child NodeId for transparent wrapper inner (first child).
    #[inline]
    pub fn transparent_inner_node(&self, parent: NodeId) -> Option<NodeId> {
        let node = self.get(parent)?;
        match &node.kind {
            TypePlanNodeKind::Transparent => self.first_child(parent),
            _ => None,
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
    #[inline]
    pub fn as_struct_plan(&self, id: NodeId) -> Option<&StructPlan> {
        let node = self.get(id)?;
        match &node.kind {
            TypePlanNodeKind::Struct(plan) => Some(plan),
            _ => None,
        }
    }

    /// Get the EnumPlan if a node is an enum type.
    #[inline]
    pub fn as_enum_plan(&self, id: NodeId) -> Option<&EnumPlan> {
        let node = self.get(id)?;
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
        let plan = TypePlan::build(TestStruct::SHAPE);
        let root = plan.root_node();

        assert_eq!(root.shape, TestStruct::SHAPE);
        assert!(!root.has_default);

        match &root.kind {
            TypePlanNodeKind::Struct(struct_plan) => {
                assert_eq!(struct_plan.num_fields, 3);
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
        let plan = TypePlan::build(TestEnum::SHAPE);
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
        let plan = TypePlan::build(RecursiveStruct::SHAPE);
        let root = plan.root_node();

        match &root.kind {
            TypePlanNodeKind::Struct(struct_plan) => {
                assert_eq!(struct_plan.num_fields, 2);
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
