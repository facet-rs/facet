use alloc::alloc::Layout;

use crate::{
    Attr, ConstTypeId, DeclId, Def, MarkerTraits, ProxyDef, Shape, ShapeFlags, ShapeLayout, Type,
    TypeNameFn, TypeOps, TypeOpsDirect, TypeOpsIndirect, TypeParam, VTableDirect, VTableErased,
    VTableIndirect, VarianceDesc,
};

/// Builder for creating [`Shape`] instances.
///
/// This builder provides a convenient way to construct Shape values with
/// sensible defaults. Many fields can be inferred or have reasonable defaults:
///
/// ```ignore
/// ShapeBuilder::for_sized::<MyType>("MyType")
///     .def(Def::Scalar)
///     .vtable(my_vtable)
///     .build()
/// ```
pub struct ShapeBuilder {
    shape: Shape,
}

const EMPTY_VESSEL: Shape = Shape {
    id: ConstTypeId::of::<()>(),
    decl_id: DeclId::new(0),
    layout: ShapeLayout::Sized(Layout::new::<()>()),
    vtable: VTableErased::Direct(&VTableDirect::empty()),
    type_ops: None,
    marker_traits: MarkerTraits::empty(),
    ty: Type::Undefined,
    def: Def::Undefined,
    type_identifier: "‹undefined›",
    module_path: None,
    source_file: None,
    source_line: None,
    source_column: None,
    type_params: &[],
    doc: &[],
    attributes: &[],
    type_tag: None,
    inner: None,
    builder_shape: None,
    type_name: None,
    proxy: None,
    // Default to bivariant - types with no lifetime parameters impose no
    // constraints on lifetimes. Types that need specific variance must set it explicitly.
    variance: VarianceDesc::BIVARIANT,
    flags: ShapeFlags::empty(),
    tag: None,
    content: None,
};

impl Shape {
    /// Create a new builder for a sized type.
    ///
    /// The `id` and `layout` are derived from the type parameter.
    #[inline]
    pub const fn builder_for_sized<T>(type_identifier: &'static str) -> ShapeBuilder {
        ShapeBuilder::for_sized::<T>(type_identifier)
    }

    /// Create a new builder for an unsized type.
    #[inline]
    pub const fn builder_for_unsized<T: ?Sized>(type_identifier: &'static str) -> ShapeBuilder {
        ShapeBuilder::for_unsized::<T>(type_identifier)
    }
}

impl ShapeBuilder {
    /// Create a new builder for a sized type.
    ///
    /// The `id` and `layout` are derived from the type parameter.
    #[inline]
    pub const fn for_sized<T>(type_identifier: &'static str) -> Self {
        Self {
            shape: Shape {
                id: ConstTypeId::of::<T>(),
                layout: ShapeLayout::Sized(Layout::new::<T>()),
                type_identifier,
                ..EMPTY_VESSEL
            },
        }
    }

    /// Create a new builder for an unsized type.
    #[inline]
    pub const fn for_unsized<T: ?Sized>(type_identifier: &'static str) -> Self {
        Self {
            shape: Shape {
                id: ConstTypeId::of::<T>(),
                layout: ShapeLayout::Unsized,
                type_identifier,
                ..EMPTY_VESSEL
            },
        }
    }

    /// Set the vtable (type-erased).
    #[inline]
    pub const fn vtable(mut self, vtable: VTableErased) -> Self {
        self.shape.vtable = vtable;
        self
    }

    /// Set the vtable from a direct vtable reference.
    #[inline]
    pub const fn vtable_direct(mut self, vtable: &'static VTableDirect) -> Self {
        self.shape.vtable = VTableErased::Direct(vtable);
        self
    }

    /// Set the vtable from an indirect vtable reference.
    #[inline]
    pub const fn vtable_indirect(mut self, vtable: &'static VTableIndirect) -> Self {
        self.shape.vtable = VTableErased::Indirect(vtable);
        self
    }

    /// Set the per-type operations (drop, default, clone) using the erased enum.
    ///
    /// For generic containers, use this to provide the monomorphized operations
    /// while sharing the main vtable across all instantiations.
    #[inline]
    pub const fn type_ops(mut self, type_ops: TypeOps) -> Self {
        self.shape.type_ops = Some(type_ops);
        self
    }

    /// Set per-type operations for concrete types (uses thin pointers).
    ///
    /// Use this for scalars, String, and derived structs/enums.
    #[inline]
    pub const fn type_ops_direct(mut self, type_ops: &'static TypeOpsDirect) -> Self {
        self.shape.type_ops = Some(TypeOps::Direct(type_ops));
        self
    }

    /// Set per-type operations for generic containers (uses wide pointers).
    ///
    /// Use this for `Vec<T>`, `Option<T>`, `Arc<T>`, etc.
    #[inline]
    pub const fn type_ops_indirect(mut self, type_ops: &'static TypeOpsIndirect) -> Self {
        self.shape.type_ops = Some(TypeOps::Indirect(type_ops));
        self
    }

    /// Add a marker trait flag.
    #[inline]
    pub const fn add_marker_trait(mut self, trait_flag: MarkerTraits) -> Self {
        self.shape.marker_traits = self.shape.marker_traits.union(trait_flag);
        self
    }

    /// Set all marker traits at once using combined bitflags.
    #[inline]
    pub const fn marker_traits(mut self, traits: MarkerTraits) -> Self {
        self.shape.marker_traits = traits;
        self
    }

    /// Mark type as implementing `Eq`.
    #[inline]
    pub const fn eq(self) -> Self {
        self.add_marker_trait(MarkerTraits::EQ)
    }

    /// Mark type as implementing `Copy`.
    #[inline]
    pub const fn copy(self) -> Self {
        self.add_marker_trait(MarkerTraits::COPY)
    }

    /// Mark type as implementing `Send`.
    #[inline]
    pub const fn send(self) -> Self {
        self.add_marker_trait(MarkerTraits::SEND)
    }

    /// Mark type as implementing `Sync`.
    #[inline]
    pub const fn sync(self) -> Self {
        self.add_marker_trait(MarkerTraits::SYNC)
    }

    /// Mark type as implementing `Unpin`.
    #[inline]
    pub const fn unpin(self) -> Self {
        self.add_marker_trait(MarkerTraits::UNPIN)
    }

    /// Mark type as implementing `UnwindSafe`.
    #[inline]
    pub const fn unwind_safe(self) -> Self {
        self.add_marker_trait(MarkerTraits::UNWIND_SAFE)
    }

    /// Mark type as implementing `RefUnwindSafe`.
    #[inline]
    pub const fn ref_unwind_safe(self) -> Self {
        self.add_marker_trait(MarkerTraits::REF_UNWIND_SAFE)
    }

    /// Set the type.
    #[inline]
    pub const fn ty(mut self, ty: Type) -> Self {
        self.shape.ty = ty;
        self
    }

    /// Set the definition.
    #[inline]
    pub const fn def(mut self, def: Def) -> Self {
        self.shape.def = def;
        self
    }

    /// Set the module path where this type is defined.
    ///
    /// This is typically set to `module_path!()` by the derive macro,
    /// which expands to the module path at the definition site.
    #[inline]
    pub const fn module_path(mut self, module_path: &'static str) -> Self {
        self.shape.module_path = Some(module_path);
        self
    }

    /// Set the declaration identifier.
    ///
    /// The `DeclId` identifies the type declaration independent of type
    /// parameters. For derive macros, this is typically computed from the
    /// source location and type name. For foreign types, it's computed from
    /// the module path and type name.
    ///
    /// See [`DeclId`] for stability guarantees (spoiler: there are none).
    #[inline]
    pub const fn decl_id(mut self, decl_id: DeclId) -> Self {
        self.shape.decl_id = decl_id;
        self
    }

    /// Set the declaration identifier for a primitive type.
    ///
    /// Primitives use a simplified decl_id based on just the type identifier,
    /// since they don't have a module path.
    #[inline]
    pub const fn decl_id_prim(self) -> Self {
        // Use the type_identifier that was already set
        let hash = crate::decl_id_hash(self.shape.type_identifier);
        self.decl_id(DeclId::new(hash))
    }

    /// Set the source file where this type is defined.
    ///
    /// This is typically set to `file!()` by the derive macro
    /// when the `doc` feature is enabled.
    #[inline]
    pub const fn source_file(mut self, file: &'static str) -> Self {
        self.shape.source_file = Some(file);
        self
    }

    /// Set the source line number where this type is defined.
    ///
    /// This is typically set to `line!()` by the derive macro
    /// when the `doc` feature is enabled.
    #[inline]
    pub const fn source_line(mut self, line: u32) -> Self {
        self.shape.source_line = Some(line);
        self
    }

    /// Set the source column number where this type is defined.
    ///
    /// This is typically set to `column!()` by the derive macro
    /// when the `doc` feature is enabled.
    #[inline]
    pub const fn source_column(mut self, column: u32) -> Self {
        self.shape.source_column = Some(column);
        self
    }

    /// Set the type parameters.
    #[inline]
    pub const fn type_params(mut self, type_params: &'static [TypeParam]) -> Self {
        self.shape.type_params = type_params;
        self
    }

    /// Set the documentation.
    #[inline]
    pub const fn doc(mut self, doc: &'static [&'static str]) -> Self {
        self.shape.doc = doc;
        self
    }

    /// Set the attributes.
    #[inline]
    pub const fn attributes(mut self, attributes: &'static [Attr]) -> Self {
        self.shape.attributes = attributes;
        self
    }

    /// Set the type tag.
    #[inline]
    pub const fn type_tag(mut self, type_tag: &'static str) -> Self {
        self.shape.type_tag = Some(type_tag);
        self
    }

    /// Set the inner shape (for transparent/newtype wrappers).
    #[inline]
    pub const fn inner(mut self, inner: &'static Shape) -> Self {
        self.shape.inner = Some(inner);
        self
    }

    /// Set the builder shape for immutable collections.
    ///
    /// If set, deserializers will build the value using the builder shape,
    /// then convert to the target type. Used for immutable collections like
    /// `Bytes` (builds through `BytesMut`) or `Arc<[T]>` (builds through `Vec<T>`).
    #[inline]
    pub const fn builder_shape(mut self, builder: &'static Shape) -> Self {
        self.shape.builder_shape = Some(builder);
        self
    }

    /// Set the type name function for formatting generic type names.
    ///
    /// For generic types like `Vec<T>`, this function formats the full name
    /// including type parameters (e.g., `Vec<String>`).
    #[inline]
    pub const fn type_name(mut self, type_name: TypeNameFn) -> Self {
        self.shape.type_name = Some(type_name);
        self
    }

    /// Set the container-level proxy for custom serialization/deserialization.
    ///
    /// When a proxy is set, the type will be serialized/deserialized through
    /// the proxy type instead of directly.
    #[inline]
    pub const fn proxy(mut self, proxy: &'static ProxyDef) -> Self {
        self.shape.proxy = Some(proxy);
        self
    }

    /// Set the variance description for this type.
    ///
    /// For types that propagate inner variance, use `VarianceDesc::propagate(inner_shape)`.
    /// For types with constant variance, use `VarianceDesc::BIVARIANT` or `VarianceDesc::INVARIANT`.
    /// For complex types like function pointers, construct with `VarianceDesc::new(base, deps)`.
    #[inline]
    pub const fn variance(mut self, variance: VarianceDesc) -> Self {
        self.shape.variance = variance;
        self
    }

    /// Set the flags for this shape.
    #[inline]
    pub const fn flags(mut self, flags: ShapeFlags) -> Self {
        self.shape.flags = flags;
        self
    }

    /// Mark this enum as untagged.
    ///
    /// Untagged enums serialize their content directly without any discriminant.
    #[inline]
    pub const fn untagged(mut self) -> Self {
        self.shape.flags = self.shape.flags.union(ShapeFlags::UNTAGGED);
        self
    }

    /// Set the tag field name for internally/adjacently tagged enums.
    #[inline]
    pub const fn tag(mut self, tag: &'static str) -> Self {
        self.shape.tag = Some(tag);
        self
    }

    /// Set the content field name for adjacently tagged enums.
    #[inline]
    pub const fn content(mut self, content: &'static str) -> Self {
        self.shape.content = Some(content);
        self
    }
    /// Mark this enum as numeric.
    ///
    /// Numeric enums serialize to the underlying discriminant
    #[inline]
    pub const fn is_numeric(self) -> Self {
        self.flags(ShapeFlags::NUMERIC)
    }

    /// Mark this type as Plain Old Data.
    ///
    /// POD types have no invariants - any combination of valid field values
    /// produces a valid instance. This enables safe mutation through reflection.
    #[inline]
    pub const fn pod(mut self) -> Self {
        self.shape.flags = self.shape.flags.union(ShapeFlags::POD);
        self
    }

    /// Build the Shape.
    ///
    /// If `ty` was not explicitly set (still `Type::Undefined`), it will be
    /// inferred from `def`.
    ///
    /// # Panics
    ///
    /// Panics if `decl_id` was not set. Every Shape must have a declaration ID.
    #[inline]
    pub const fn build(self) -> Shape {
        // Ensure decl_id was set - the default of 0 is invalid
        assert!(
            self.shape.decl_id.0 != 0,
            "decl_id must be set - use .decl_id() to set it"
        );

        let ty = match self.shape.ty {
            Type::Undefined => self.shape.def.default_type(),
            ty => ty,
        };

        Shape { ty, ..self.shape }
    }
}
