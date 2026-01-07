mod shape_layout;
pub use shape_layout::*;

mod shape_fmt;

mod shape_builder;
pub use shape_builder::*;

use core::alloc::Layout;

use crate::{
    Attr, ConstTypeId, Def, Facet, MAX_VARIANCE_DEPTH, MarkerTraits, TruthyFn, Type, TypeOps,
    UserType, VTableErased, Variance,
};

/// Stack-based visited set for variance computation.
///
/// Tracks types currently being computed to detect cycles.
/// Uses a fixed-size array since we're limited by MAX_VARIANCE_DEPTH anyway.
struct VarianceVisited {
    /// Type IDs currently being computed (forms a stack)
    ids: [ConstTypeId; MAX_VARIANCE_DEPTH],
    /// Number of valid entries in `ids`
    len: usize,
}

impl VarianceVisited {
    /// Create an empty visited set.
    #[inline]
    const fn new() -> Self {
        Self {
            // Initialize with dummy values - they won't be read before being written
            ids: [ConstTypeId::of::<()>(); MAX_VARIANCE_DEPTH],
            len: 0,
        }
    }

    /// Check if a type ID is in the visited set (currently being computed).
    #[inline]
    fn contains(&self, id: ConstTypeId) -> bool {
        for i in 0..self.len {
            if self.ids[i] == id {
                return true;
            }
        }
        false
    }

    /// Push a type ID onto the visited stack.
    /// Returns false if the stack is full (depth limit reached).
    #[inline]
    fn push(&mut self, id: ConstTypeId) -> bool {
        if self.len >= MAX_VARIANCE_DEPTH {
            return false;
        }
        self.ids[self.len] = id;
        self.len += 1;
        true
    }

    /// Pop a type ID from the visited stack.
    #[inline]
    fn pop(&mut self) {
        debug_assert!(self.len > 0);
        self.len -= 1;
    }
}
#[cfg(feature = "alloc")]
use crate::{PtrMut, PtrUninit, UnsizedError};

crate::bitflags! {
    /// Bit flags for common shape-level attributes.
    ///
    /// These provide O(1) access to frequently-checked boolean attributes,
    /// avoiding the O(n) linear scan through the attributes slice.
    pub struct ShapeFlags: u16 {
        /// Enum is untagged (no discriminant in serialized form).
        /// Set by `#[facet(untagged)]`.
        const UNTAGGED = 1 << 0;

        /// Serializes/Deserializers enum to/from integer based on variant discriminant,
        /// Set by `#[facet(is_numeric)]`.
        const NUMERIC = 1 << 1;

        /// Plain Old Data - type has no invariants and any combination of valid
        /// field values produces a valid instance.
        ///
        /// This enables safe mutation through reflection (poke operations).
        /// Set by `#[facet(pod)]`.
        const POD = 1 << 2;
    }
}

/// Schema for reflection of a type — the core type in facet.
/// Contains everything needed to inspect, allocate, and manipulate values at runtime.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Shape {
    /// Unique type identifier from the compiler.
    /// Use this for type equality checks and hash map keys.
    pub id: ConstTypeId,

    /// Size and alignment — enough to allocate (but not initialize).
    /// Check `sized_layout()` for sized types, or handle `Unsized` for slices/dyn.
    pub layout: ShapeLayout,

    /// Erased vtable for display, debug, default, clone, hash, eq, ord, etc.
    /// More specific vtables (e.g. for structs, enums) live in [`Def`] variants.
    pub vtable: VTableErased,

    /// Per-type operations that must be monomorphized (drop, default, clone).
    ///
    /// For generic containers like `Vec<T>`, the main `vtable` can be shared
    /// across all instantiations (using type-erased operations), while `type_ops`
    /// contains the operations that must be specialized per-T.
    ///
    /// - `TypeOps::Direct` for concrete types (uses thin pointers)
    /// - `TypeOps::Indirect` for generic containers (uses wide pointers with shape)
    pub type_ops: Option<TypeOps>,

    /// Marker traits like Copy, Send, Sync, etc.
    pub marker_traits: MarkerTraits,

    /// Underlying type category: primitive, array, slice, tuple, pointer, user-defined.
    /// Follows the [Rust Reference](https://doc.rust-lang.org/reference/types.html).
    pub ty: Type,

    /// Type definition with variant-specific operations: scalar parsing,
    /// struct field access, enum variant iteration, map/list manipulation.
    pub def: Def,

    /// Type name without generic parameters (e.g. `Vec`, not `Vec<String>`).
    /// For the full name with generics, use `vtable.type_name`.
    pub type_identifier: &'static str,

    /// Generic type parameters (e.g. `T` in `Vec<T>`).
    /// Includes bounds and variance information.
    pub type_params: &'static [TypeParam],

    /// Doc comments from the original type definition.
    /// Collected by facet-macros; lines usually start with a space.
    pub doc: &'static [&'static str],

    /// Custom attributes applied to this type via `#[facet(...)]`.
    /// Use for validation, serialization hints, etc.
    pub attributes: &'static [Attr],

    /// Type tag for self-describing formats (e.g. JSON with type discriminators).
    /// Can be a qualified name, simple string, or integer depending on format.
    pub type_tag: Option<&'static str>,

    /// If set, this shape is a transparent wrapper around another shape.
    /// Newtypes (`NonZero`), path wrappers (`Utf8PathBuf`), smart pointers (`Arc<T>`).
    /// Serializes as the inner type: `NonZero<u8>` becomes `128`, not `{"value": 128}`.
    pub inner: Option<&'static Shape>,

    /// Optional builder type for immutable collections.
    /// If set, deserializers should build this type first, then convert to the target type.
    /// Examples: `Bytes` builds through `BytesMut`, `Arc<[T]>` builds through `Vec<T>`.
    pub builder_shape: Option<&'static Shape>,

    /// Custom type name formatter for generic types.
    /// If `None`, uses `type_identifier`. If `Some`, calls the function to format
    /// the full type name including generic parameters (e.g., `Vec<String>`).
    pub type_name: Option<crate::TypeNameFn>,

    /// Container-level proxy for custom serialization/deserialization.
    /// Set by `#[facet(proxy = ProxyType)]` on the container.
    #[cfg(feature = "alloc")]
    pub proxy: Option<&'static crate::ProxyDef>,

    /// Variance of this type with respect to its type/lifetime parameters.
    /// For derived types, use `Shape::computed_variance` which walks fields.
    /// For leaf types, use `Variance::COVARIANT`, `Variance::INVARIANT`, etc.
    pub variance: fn(&'static Shape) -> Variance,

    /// Bit flags for common boolean attributes.
    ///
    /// Provides O(1) access to frequently-checked attributes like `untagged`.
    /// These are set by the derive macro based on `#[facet(...)]` attributes
    /// with `#[storage(flag)]` in the grammar.
    pub flags: ShapeFlags,

    /// Tag field name for internally/adjacently tagged enums.
    /// Set by `#[facet(tag = "...")]`.
    pub tag: Option<&'static str>,

    /// Content field name for adjacently tagged enums.
    /// Set by `#[facet(content = "...")]`.
    pub content: Option<&'static str>,
}

impl PartialOrd for Shape {
    #[allow(clippy::non_canonical_partial_ord_impl)]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.id.get().partial_cmp(&other.id.get())
    }
}

impl Ord for Shape {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.id.get().cmp(&other.id.get())
    }
}

impl PartialEq for Shape {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Shape {}

impl core::hash::Hash for Shape {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        // Only hash id, consistent with PartialEq which only compares id.
        // The Hash trait requires: if a == b then hash(a) == hash(b).
        self.id.hash(state);
    }
}

impl Shape {
    /// Check if this shape is of the given type
    #[inline]
    pub fn is_shape(&self, other: &Shape) -> bool {
        self == other
    }

    /// Assert that this shape is equal to the given shape, panicking if it's not
    pub fn assert_shape(&self, other: &Shape) {
        assert!(
            self.is_shape(other),
            "Shape mismatch: expected {other}, found {self}",
        );
    }

    /// Returns true if this shape requires eager materialization.
    ///
    /// Shapes that require eager materialization cannot have their construction
    /// deferred because they need all their data available at once. Examples include:
    ///
    /// - `Arc<[T]>`, `Box<[T]>`, `Rc<[T]>` - slice-based smart pointers that need
    ///   all elements to compute the final allocation
    ///
    /// This is used by deferred validation mode in `Partial` to determine which
    /// shapes must be fully materialized before proceeding.
    #[inline]
    pub fn requires_eager_materialization(&self) -> bool {
        // Check if this is a pointer type with slice_builder_vtable
        // (indicates Arc<[T]>, Box<[T]>, Rc<[T]>, etc.)
        if let Ok(ptr_def) = self.def.into_pointer()
            && ptr_def.vtable.slice_builder_vtable.is_some()
        {
            return true;
        }
        false
    }
}

impl Shape {
    /// Heap-allocate a value of this shape
    #[cfg(feature = "alloc")]
    #[inline]
    pub fn allocate(&self) -> Result<crate::PtrUninit, UnsizedError> {
        let layout = self.layout.sized_layout()?;

        Ok(crate::PtrUninit::new(if layout.size() == 0 {
            core::ptr::null_mut::<u8>().wrapping_byte_add(layout.align())
        } else {
            // SAFETY: We have checked that layout's size is non-zero
            let ptr = unsafe { alloc::alloc::alloc(layout) };
            if ptr.is_null() {
                alloc::alloc::handle_alloc_error(layout)
            }
            ptr
        }))
    }

    /// Deallocate a heap-allocated value of this shape
    ///
    /// # Safety
    ///
    /// - `ptr` must have been allocated using [`Self::allocate`] and be aligned for this shape.
    /// - `ptr` must point to a region that is not already deallocated.
    #[cfg(feature = "alloc")]
    #[inline]
    pub unsafe fn deallocate_mut(&self, ptr: PtrMut) -> Result<(), UnsizedError> {
        use alloc::alloc::dealloc;

        let layout = self.layout.sized_layout()?;

        if layout.size() == 0 {
            // Nothing to deallocate
            return Ok(());
        }
        // SAFETY: The user guarantees ptr is valid and from allocate, we checked size isn't 0
        unsafe { dealloc(ptr.as_mut_byte_ptr(), layout) }

        Ok(())
    }

    /// Deallocate a heap-allocated, uninitialized value of this shape.
    ///
    /// # Safety
    ///
    /// - `ptr` must have been allocated using [`Self::allocate`] (or equivalent) for this shape.
    /// - `ptr` must not have been already deallocated.
    /// - `ptr` must be properly aligned for this shape.
    #[cfg(feature = "alloc")]
    #[inline]
    pub unsafe fn deallocate_uninit(&self, ptr: PtrUninit) -> Result<(), UnsizedError> {
        use alloc::alloc::dealloc;

        let layout = self.layout.sized_layout()?;

        if layout.size() == 0 {
            // Nothing to deallocate
            return Ok(());
        }
        // SAFETY: The user guarantees ptr is valid and from allocate; layout is nonzero
        unsafe { dealloc(ptr.as_mut_byte_ptr(), layout) };

        Ok(())
    }
}

impl Shape {
    /// Returns a const type ID for type `T`.
    #[inline]
    pub const fn id_of<T: ?Sized>() -> ConstTypeId {
        ConstTypeId::of::<T>()
    }

    /// Returns the sized layout for type `T`.
    #[inline]
    pub const fn layout_of<T>() -> ShapeLayout {
        ShapeLayout::Sized(Layout::new::<T>())
    }

    /// Returns the unsized layout marker.
    pub const UNSIZED_LAYOUT: ShapeLayout = ShapeLayout::Unsized;

    /// Returns true if this shape has the `deny_unknown_fields` builtin attribute.
    #[inline]
    pub fn has_deny_unknown_fields_attr(&self) -> bool {
        self.has_builtin_attr("deny_unknown_fields")
    }

    /// Returns true if this shape has the `default` builtin attribute.
    #[inline]
    pub fn has_default_attr(&self) -> bool {
        self.has_builtin_attr("default")
    }

    /// Returns true if this shape has a builtin attribute with the given key.
    #[inline]
    pub fn has_builtin_attr(&self, key: &str) -> bool {
        self.attributes
            .iter()
            .any(|attr| attr.ns.is_none() && attr.key == key)
    }

    /// Returns true if this shape is transparent.
    ///
    /// A type is transparent if it has `#[repr(transparent)]` or is marked
    /// with `#[facet(transparent)]`.
    #[inline]
    pub fn is_transparent(&self) -> bool {
        // Check for #[facet(transparent)] attribute
        if self.has_builtin_attr("transparent") {
            return true;
        }
        // Check for #[repr(transparent)] via the Repr in StructType
        if let Type::User(UserType::Struct(st)) = &self.ty
            && st.repr.base == crate::BaseRepr::Transparent
        {
            return true;
        }
        false
    }

    /// Returns true if this enum is untagged.
    ///
    /// Untagged enums serialize their content directly without any discriminant.
    /// This checks the `UNTAGGED` flag (O(1)).
    #[inline]
    pub fn is_untagged(&self) -> bool {
        self.flags.contains(ShapeFlags::UNTAGGED)
    }

    /// Returns true if this enum is numeric.
    ///
    /// This checks the `NUMERIC` flag (O(1)).
    #[inline]
    pub fn is_numeric(&self) -> bool {
        self.flags.contains(ShapeFlags::NUMERIC)
    }

    /// Returns true if this type is Plain Old Data.
    ///
    /// POD types have no invariants - any combination of valid field values
    /// produces a valid instance. This enables safe mutation through reflection
    /// (poke operations).
    ///
    /// This returns true if:
    /// - The type is a primitive (implicitly POD), OR
    /// - The type has the `POD` flag set via `#[facet(pod)]`
    ///
    /// Note: POD is NOT an auto-trait. A struct with all POD fields is NOT
    /// automatically POD - it must be explicitly marked. This is because the
    /// struct might have semantic invariants that aren't expressed in the type
    /// system (e.g., "these two fields must be in sync").
    ///
    /// Containers like `Vec<T>` and `Option<T>` don't need POD marking - they
    /// are manipulated through their vtables which maintain their invariants.
    /// The POD-ness of the element type `T` matters when mutating elements.
    #[inline]
    pub fn is_pod(&self) -> bool {
        // Primitives are implicitly POD - any value of the type is valid
        matches!(self.ty, Type::Primitive(_)) || self.flags.contains(ShapeFlags::POD)
    }

    /// Returns the tag field name for internally/adjacently tagged enums.
    ///
    /// This is the direct field access (O(1)), not an attribute lookup.
    #[inline]
    pub fn get_tag_attr(&self) -> Option<&'static str> {
        self.tag
    }

    /// Returns the content field name for adjacently tagged enums.
    ///
    /// This is the direct field access (O(1)), not an attribute lookup.
    #[inline]
    pub fn get_content_attr(&self) -> Option<&'static str> {
        self.content
    }

    /// Gets a builtin attribute value by key.
    ///
    /// This is a helper for attributes with simple payload types like `&'static str`.
    #[inline]
    pub fn get_builtin_attr_value<'a, T: Facet<'a> + Copy + 'static>(
        &self,
        key: &str,
    ) -> Option<T> {
        self.attributes.iter().find_map(|attr| {
            if attr.ns.is_none() && attr.key == key {
                // Try to get the data as the requested type
                // Safety: We're checking that the shape matches T::SHAPE
                unsafe { attr.data.get_as::<T>(T::SHAPE).copied() }
            } else {
                None
            }
        })
    }

    /// Compute the variance of this type by walking its fields recursively.
    ///
    /// This method walks struct fields and enum variants to determine the
    /// combined variance. For leaf types (scalars, etc.), it delegates to
    /// the `variance` field.
    ///
    /// The implementation tracks visited types to:
    /// 1. Detect cycles (recursive types) - returns Covariant for cycles since
    ///    they don't contribute new variance information
    /// 2. Prevent exponential blowup for types with multiple recursive fields
    ///    (e.g., `Node(&Node, &Node, &Node, &Node)` would be O(4^depth) without this)
    pub fn computed_variance(&'static self) -> Variance {
        let mut visited = VarianceVisited::new();
        self.computed_variance_impl(&mut visited)
    }

    /// Internal implementation with visited set for cycle detection.
    fn computed_variance_impl(&'static self, visited: &mut VarianceVisited) -> Variance {
        // If we're already computing this type's variance, we've hit a cycle.
        // Cycles don't contribute new variance information - the variance is
        // determined by the non-cyclic parts of the type. Return Covariant
        // as the neutral element for variance combination.
        //
        // Example: `struct Node(&'static Node)` - the self-reference doesn't
        // add any new variance constraints, so Node is covariant (like &'static T).
        if visited.contains(self.id) {
            return Variance::Covariant;
        }

        // Depth limit reached - return Invariant as the conservative choice.
        // This shouldn't normally happen since the visited set prevents cycles,
        // but serves as a safety net for pathological cases.
        if !visited.push(self.id) {
            return Variance::Invariant;
        }

        let result = self.computed_variance_inner(visited);

        visited.pop();
        result
    }

    /// Core variance computation logic, called after cycle detection.
    fn computed_variance_inner(&'static self, visited: &mut VarianceVisited) -> Variance {
        match &self.ty {
            Type::User(UserType::Struct(s)) => {
                let mut v = Variance::Covariant;
                for field in s.fields {
                    let field_shape = field.shape();
                    v = v.combine(field_shape.computed_variance_impl(visited));
                    // Early termination: Invariant combined with anything is Invariant
                    if v == Variance::Invariant {
                        return Variance::Invariant;
                    }
                }
                v
            }
            Type::User(UserType::Enum(e)) => {
                let mut v = Variance::Covariant;
                for variant in e.variants {
                    for field in variant.data.fields {
                        let field_shape = field.shape();
                        v = v.combine(field_shape.computed_variance_impl(visited));
                        // Early termination: Invariant combined with anything is Invariant
                        if v == Variance::Invariant {
                            return Variance::Invariant;
                        }
                    }
                }
                v
            }
            // For types with an inner shape, check if they use computed_variance.
            // If they have a different variance function (like INVARIANT for *mut T),
            // use that directly. Otherwise recurse into the inner type.
            _ if self.inner.is_some() => {
                if core::ptr::eq(
                    self.variance as *const (),
                    Self::computed_variance as *const (),
                ) {
                    // This type delegates to computed_variance, recurse into inner
                    let inner = self.inner.unwrap();
                    inner.computed_variance_impl(visited)
                } else {
                    // This type has its own variance declaration (e.g., *mut T is INVARIANT)
                    (self.variance)(self)
                }
            }
            // Types that don't have .inner set - check if they delegate to computed_variance.
            // If not, respect their declared variance. If so, fall back to Def-based lookup.
            _ => {
                // Check if this type has a custom variance function.
                // If so, use it directly - the type knows its own variance.
                if !core::ptr::eq(
                    self.variance as *const (),
                    Self::computed_variance as *const (),
                ) {
                    return (self.variance)(self);
                }

                // Type delegates to computed_variance - use Def-based lookup.
                match &self.def {
                    // Map<K, V> has two type parameters - combine both variances
                    Def::Map(map_def) => {
                        let k_var = map_def.k().computed_variance_impl(visited);
                        // Early termination
                        if k_var == Variance::Invariant {
                            return Variance::Invariant;
                        }
                        let v_var = map_def.v().computed_variance_impl(visited);
                        k_var.combine(v_var)
                    }
                    // Result<T, E> has two type parameters - combine both variances
                    Def::Result(result_def) => {
                        let t_var = result_def.t.computed_variance_impl(visited);
                        // Early termination
                        if t_var == Variance::Invariant {
                            return Variance::Invariant;
                        }
                        let e_var = result_def.e.computed_variance_impl(visited);
                        t_var.combine(e_var)
                    }
                    // Single-parameter containers - variance propagates from element type
                    Def::List(list_def) => list_def.t().computed_variance_impl(visited),
                    Def::Array(array_def) => array_def.t().computed_variance_impl(visited),
                    Def::Set(set_def) => set_def.t().computed_variance_impl(visited),
                    Def::Slice(slice_def) => slice_def.t().computed_variance_impl(visited),
                    Def::NdArray(ndarray_def) => ndarray_def.t().computed_variance_impl(visited),
                    Def::Pointer(pointer_def) => {
                        if let Some(pointee) = pointer_def.pointee {
                            pointee.computed_variance_impl(visited)
                        } else {
                            // Opaque pointer with no pointee info - use declared variance
                            (self.variance)(self)
                        }
                    }
                    Def::Option(option_def) => option_def.t.computed_variance_impl(visited),
                    // Leaf types with no type parameters - use declared variance
                    Def::Scalar | Def::Undefined | Def::DynamicValue(_) => (self.variance)(self),
                }
            }
        }
    }
}

/// Represents a lifetime parameter, e.g., `'a` or `'a: 'b + 'c`.
///
/// Note: these are subject to change — it's a bit too stringly-typed for now.
#[derive(Debug, Clone)]
pub struct TypeParam {
    /// The name of the type parameter (e.g., `T`).
    pub name: &'static str,

    /// The shape of the type parameter (e.g. `String`)
    pub shape: &'static Shape,
}

impl TypeParam {
    /// Returns the shape of the type parameter.
    #[inline]
    pub const fn shape(&self) -> &'static Shape {
        self.shape
    }
}

//////////////////////////////////////////////////////////////////////
// Unified vtable call helpers
//////////////////////////////////////////////////////////////////////

impl Shape {
    /// Call the debug function, regardless of vtable style.
    ///
    /// # Safety
    /// `ptr` must point to a valid value of this shape's type.
    #[inline]
    pub unsafe fn call_debug(
        &'static self,
        ptr: crate::PtrConst,
        f: &mut core::fmt::Formatter<'_>,
    ) -> Option<core::fmt::Result> {
        match self.vtable {
            VTableErased::Direct(vt) => {
                let debug_fn = vt.debug?;
                Some(unsafe { debug_fn(ptr.raw_ptr() as *const (), f) })
            }
            VTableErased::Indirect(vt) => {
                let debug_fn = vt.debug?;
                let ox = crate::OxPtrConst::new(ptr, self);
                unsafe { debug_fn(ox, f) }
            }
        }
    }

    /// Call the display function, regardless of vtable style.
    ///
    /// # Safety
    /// `ptr` must point to a valid value of this shape's type.
    #[inline]
    pub unsafe fn call_display(
        &'static self,
        ptr: crate::PtrConst,
        f: &mut core::fmt::Formatter<'_>,
    ) -> Option<core::fmt::Result> {
        match self.vtable {
            VTableErased::Direct(vt) => {
                let display_fn = vt.display?;
                Some(unsafe { display_fn(ptr.raw_ptr() as *const (), f) })
            }
            VTableErased::Indirect(vt) => {
                let display_fn = vt.display?;
                let ox = crate::OxPtrConst::new(ptr, self);
                unsafe { display_fn(ox, f) }
            }
        }
    }

    /// Call the hash function, regardless of vtable style.
    ///
    /// # Safety
    /// `ptr` must point to a valid value of this shape's type.
    #[inline]
    pub unsafe fn call_hash(
        &'static self,
        ptr: crate::PtrConst,
        hasher: &mut crate::HashProxy<'_>,
    ) -> Option<()> {
        match self.vtable {
            VTableErased::Direct(vt) => {
                let hash_fn = vt.hash?;
                unsafe { hash_fn(ptr.raw_ptr() as *const (), hasher) };
                Some(())
            }
            VTableErased::Indirect(vt) => {
                let hash_fn = vt.hash?;
                let ox = crate::OxPtrConst::new(ptr, self);
                unsafe { hash_fn(ox, hasher) }
            }
        }
    }

    /// Call the partial_eq function, regardless of vtable style.
    ///
    /// # Safety
    /// `a` and `b` must point to valid values of this shape's type.
    #[inline]
    pub unsafe fn call_partial_eq(
        &'static self,
        a: crate::PtrConst,
        b: crate::PtrConst,
    ) -> Option<bool> {
        match self.vtable {
            VTableErased::Direct(vt) => {
                let eq_fn = vt.partial_eq?;
                Some(unsafe { eq_fn(a.raw_ptr() as *const (), b.raw_ptr() as *const ()) })
            }
            VTableErased::Indirect(vt) => {
                let eq_fn = vt.partial_eq?;
                let ox_a = crate::OxPtrConst::new(a, self);
                let ox_b = crate::OxPtrConst::new(b, self);
                unsafe { eq_fn(ox_a, ox_b) }
            }
        }
    }

    /// Call the partial_cmp function, regardless of vtable style.
    ///
    /// # Safety
    /// `a` and `b` must point to valid values of this shape's type.
    #[inline]
    pub unsafe fn call_partial_cmp(
        &'static self,
        a: crate::PtrConst,
        b: crate::PtrConst,
    ) -> Option<Option<core::cmp::Ordering>> {
        match self.vtable {
            VTableErased::Direct(vt) => {
                let cmp_fn = vt.partial_cmp?;
                Some(unsafe { cmp_fn(a.raw_ptr() as *const (), b.raw_ptr() as *const ()) })
            }
            VTableErased::Indirect(vt) => {
                let cmp_fn = vt.partial_cmp?;
                let ox_a = crate::OxPtrConst::new(a, self);
                let ox_b = crate::OxPtrConst::new(b, self);
                unsafe { cmp_fn(ox_a, ox_b) }
            }
        }
    }

    /// Call the cmp function, regardless of vtable style.
    ///
    /// # Safety
    /// `a` and `b` must point to valid values of this shape's type.
    #[inline]
    pub unsafe fn call_cmp(
        &'static self,
        a: crate::PtrConst,
        b: crate::PtrConst,
    ) -> Option<core::cmp::Ordering> {
        match self.vtable {
            VTableErased::Direct(vt) => {
                let cmp_fn = vt.cmp?;
                Some(unsafe { cmp_fn(a.raw_ptr() as *const (), b.raw_ptr() as *const ()) })
            }
            VTableErased::Indirect(vt) => {
                let cmp_fn = vt.cmp?;
                let ox_a = crate::OxPtrConst::new(a, self);
                let ox_b = crate::OxPtrConst::new(b, self);
                unsafe { cmp_fn(ox_a, ox_b) }
            }
        }
    }

    /// Call the drop_in_place function from `type_ops`.
    ///
    /// # Safety
    /// `ptr` must point to a valid value of this shape's type that can be dropped.
    #[inline]
    pub unsafe fn call_drop_in_place(&'static self, ptr: crate::PtrMut) -> Option<()> {
        match self.type_ops? {
            TypeOps::Direct(ops) => {
                unsafe { (ops.drop_in_place)(ptr.as_mut_byte_ptr() as *mut ()) };
            }
            TypeOps::Indirect(ops) => {
                let ox = crate::OxPtrMut::new(ptr, self);
                unsafe { (ops.drop_in_place)(ox) };
            }
        }
        Some(())
    }

    /// Call the default_in_place function from `type_ops`.
    ///
    /// # Safety
    /// `ptr` must point to uninitialized memory suitable for this shape's type.
    #[inline]
    pub unsafe fn call_default_in_place(&'static self, ptr: crate::PtrMut) -> Option<()> {
        match self.type_ops? {
            TypeOps::Direct(ops) => {
                let default_fn = ops.default_in_place?;
                unsafe { default_fn(ptr.as_mut_byte_ptr() as *mut ()) };
            }
            TypeOps::Indirect(ops) => {
                let default_fn = ops.default_in_place?;
                let ox = crate::OxPtrMut::new(ptr, self);
                unsafe { default_fn(ox) };
            }
        }
        Some(())
    }

    /// Call the invariants function, regardless of vtable style.
    ///
    /// # Safety
    /// `ptr` must point to a valid value of this shape's type.
    #[inline]
    pub unsafe fn call_invariants(
        &'static self,
        ptr: crate::PtrConst,
    ) -> Option<Result<(), alloc::string::String>> {
        match self.vtable {
            VTableErased::Direct(vt) => {
                let invariants_fn = vt.invariants?;
                Some(unsafe { invariants_fn(ptr.raw_ptr() as *const ()) })
            }
            VTableErased::Indirect(vt) => {
                let invariants_fn = vt.invariants?;
                let ox = crate::OxPtrConst::new(ptr, self);
                unsafe { invariants_fn(ox) }
            }
        }
    }

    /// Call the parse function, regardless of vtable style.
    ///
    /// # Safety
    /// `dst` must point to uninitialized memory suitable for this shape's type.
    #[inline]
    pub unsafe fn call_parse(
        &'static self,
        s: &str,
        dst: crate::PtrMut,
    ) -> Option<Result<(), crate::ParseError>> {
        match self.vtable {
            VTableErased::Direct(vt) => {
                let parse_fn = vt.parse?;
                Some(unsafe { parse_fn(s, dst.data_ptr() as *mut ()) })
            }
            VTableErased::Indirect(vt) => {
                let parse_fn = vt.parse?;
                let ox = crate::OxPtrMut::new(dst, self);
                unsafe { parse_fn(s, ox) }
            }
        }
    }

    /// Call the parse_bytes function, regardless of vtable style.
    ///
    /// For types with efficient binary representations (e.g., UUID as 16 bytes).
    ///
    /// # Safety
    /// `dst` must point to uninitialized memory suitable for this shape's type.
    #[inline]
    pub unsafe fn call_parse_bytes(
        &'static self,
        bytes: &[u8],
        dst: crate::PtrMut,
    ) -> Option<Result<(), crate::ParseError>> {
        match self.vtable {
            VTableErased::Direct(vt) => {
                let parse_fn = vt.parse_bytes?;
                Some(unsafe { parse_fn(bytes, dst.data_ptr() as *mut ()) })
            }
            VTableErased::Indirect(vt) => {
                let parse_fn = vt.parse_bytes?;
                let ox = crate::OxPtrMut::new(dst, self);
                unsafe { parse_fn(bytes, ox) }
            }
        }
    }

    /// Call the try_from function, regardless of vtable style.
    ///
    /// # Safety
    /// `src` must point to a valid value of the source type (described by `src_shape`).
    /// `dst` must point to uninitialized memory suitable for this shape's type.
    #[inline]
    pub unsafe fn call_try_from(
        &'static self,
        src_shape: &'static Shape,
        src: crate::PtrConst,
        dst: crate::PtrMut,
    ) -> Option<crate::TryFromOutcome> {
        match self.vtable {
            VTableErased::Direct(vt) => {
                let try_from_fn = vt.try_from?;
                Some(unsafe { try_from_fn(dst.data_ptr() as *mut (), src_shape, src) })
            }
            VTableErased::Indirect(vt) => {
                let try_from_fn = vt.try_from?;
                let ox_dst = crate::OxPtrMut::new(dst, self);
                Some(unsafe { try_from_fn(ox_dst, src_shape, src) })
            }
        }
    }

    /// Call the try_borrow_inner function, regardless of vtable style.
    ///
    /// # Safety
    /// `ptr` must point to a valid value of this shape's type.
    #[inline]
    pub unsafe fn call_try_borrow_inner(
        &'static self,
        ptr: crate::PtrConst,
    ) -> Option<Result<crate::PtrMut, alloc::string::String>> {
        match self.vtable {
            VTableErased::Direct(vt) => {
                let try_borrow_fn = vt.try_borrow_inner?;
                Some(unsafe { try_borrow_fn(ptr.raw_ptr() as *const ()) })
            }
            VTableErased::Indirect(vt) => {
                let try_borrow_fn = vt.try_borrow_inner?;
                let ox = crate::OxPtrConst::new(ptr, self);
                unsafe { try_borrow_fn(ox) }
            }
        }
    }

    /// Call the clone_into function from `type_ops`.
    ///
    /// # Safety
    /// `src` must point to a valid value of this shape's type.
    /// `dst` must point to uninitialized memory suitable for this shape's type.
    #[inline]
    pub unsafe fn call_clone_into(
        &'static self,
        src: crate::PtrConst,
        dst: crate::PtrMut,
    ) -> Option<()> {
        match self.type_ops? {
            TypeOps::Direct(ops) => {
                let clone_fn = ops.clone_into?;
                unsafe {
                    clone_fn(
                        src.as_byte_ptr() as *const (),
                        dst.as_mut_byte_ptr() as *mut (),
                    )
                };
            }
            TypeOps::Indirect(ops) => {
                let clone_fn = ops.clone_into?;
                let ox_src = crate::OxPtrConst::new(src, self);
                let ox_dst = crate::OxPtrMut::new(dst, self);
                unsafe { clone_fn(ox_src, ox_dst) };
            }
        }
        Some(())
    }

    /// Check if this shape represents the given type.
    #[inline]
    pub fn is_type<T: crate::Facet<'static>>(&self) -> bool {
        self.id == Self::id_of::<T>()
    }

    /// Returns the truthiness predicate stored on this shape, if any.
    #[inline]
    pub fn truthiness_fn(&self) -> Option<TruthyFn> {
        self.type_ops.and_then(|ops| ops.truthiness_fn())
    }
}
