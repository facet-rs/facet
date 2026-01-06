use core::{cmp::Ordering, marker::PhantomData, ptr::NonNull};
#[cfg(feature = "alloc")]
use facet_core::Field;
use facet_core::{
    Def, Facet, PointerType, PtrConst, Shape, StructKind, Type, TypeNameOpts, UserType,
    VTableErased, Variance,
};

use crate::{PeekNdArray, PeekSet, ReflectError, ScalarType};

use super::{
    ListLikeDef, PeekDynamicValue, PeekEnum, PeekList, PeekListLike, PeekMap, PeekOption,
    PeekPointer, PeekResult, PeekStruct, PeekTuple, tuple::TupleType,
};

#[cfg(feature = "alloc")]
use super::OwnedPeek;

/// A unique identifier for a peek value
#[derive(Clone, Copy, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub struct ValueId {
    pub(crate) shape: &'static Shape,
    pub(crate) ptr: *const u8,
}

impl ValueId {
    #[inline]
    pub(crate) fn new(shape: &'static Shape, ptr: *const u8) -> Self {
        Self { shape, ptr }
    }
}

impl core::fmt::Display for ValueId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}@{:p}", self.shape, self.ptr)
    }
}

impl core::fmt::Debug for ValueId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}

/// A read-only view into a value with runtime type information.
///
/// `Peek` provides reflection capabilities for reading values at runtime.
/// If the value is a struct, you can read its fields; if it's an enum,
/// you can determine which variant is selected; if it's a scalar, you can
/// extract a concrete value.
///
/// # Lifetime Parameters
///
/// - `'mem`: The memory lifetime - how long the underlying data is valid
/// - `'facet`: The type's lifetime parameter (for types like `&'a str`)
///
/// # Variance and Soundness
///
/// `Peek` is **invariant** over `'facet`. This is required for soundness:
/// if `Peek` were covariant, it would be possible to launder lifetimes
/// through reflection, leading to use-after-free bugs with types like
/// `fn(&'a str)`. See [issue #1168](https://github.com/facet-rs/facet/issues/1168).
///
/// The underlying type's variance is tracked in [`Shape::variance`], which
/// can be used for future variance-aware APIs.
#[allow(clippy::type_complexity)]
#[derive(Clone, Copy)]
pub struct Peek<'mem, 'facet> {
    /// Underlying data
    pub(crate) data: PtrConst,

    /// Shape of the value
    pub(crate) shape: &'static Shape,

    // Invariant over 'facet: Peek<'mem, 'a> cannot be cast to Peek<'mem, 'b> even if 'a: 'b.
    //
    // This is REQUIRED for soundness! If Peek were covariant over 'facet, we could:
    // 1. Create Peek<'mem, 'static> from FnWrapper<'static> (contains fn(&'static str))
    // 2. Use covariance to cast it to Peek<'mem, 'short>
    // 3. Call get::<FnWrapper<'short>>() to get &FnWrapper<'short>
    // 4. This would allow calling the function with a &'short str that goes out of scope
    //    while the original function pointer still holds it as 'static
    //
    // The fn(&'a ()) -> &'a () pattern makes this type invariant over 'facet.
    // The &'mem () makes this type covariant over 'mem (safe because we only read through it).
    // See: https://github.com/facet-rs/facet/issues/1168
    _invariant: PhantomData<(&'mem (), fn(&'facet ()) -> &'facet ())>,
}

impl<'mem, 'facet> Peek<'mem, 'facet> {
    /// Returns a read-only view over a `T` value.
    pub fn new<T: Facet<'facet> + ?Sized>(t: &'mem T) -> Self {
        Self {
            data: PtrConst::new(NonNull::from(t).as_ptr()),
            shape: T::SHAPE,
            _invariant: PhantomData,
        }
    }

    /// Returns a read-only view over a value (given its shape), trusting you
    /// that those two match.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it doesn't check if the provided data
    /// and shape are compatible. The caller must ensure that the data is valid
    /// for the given shape.
    pub unsafe fn unchecked_new(data: PtrConst, shape: &'static Shape) -> Self {
        Self {
            data,
            shape,
            _invariant: PhantomData,
        }
    }

    // =============================================================================
    // Variance-aware lifetime transformation methods
    // =============================================================================

    /// Returns the computed variance of the underlying type.
    ///
    /// This walks the type's fields to determine if the type is covariant,
    /// contravariant, or invariant over its lifetime parameter.
    #[inline]
    pub fn variance(&self) -> Variance {
        self.shape.computed_variance()
    }

    /// Shrinks the `'facet` lifetime parameter.
    ///
    /// This is safe for covariant types: if data is valid for `'static`,
    /// it's also valid for any shorter lifetime `'shorter`.
    ///
    /// # Panics
    ///
    /// Panics if the type is not covariant (i.e., if shrinking would be unsound).
    #[inline]
    pub fn shrink_lifetime<'shorter>(self) -> Peek<'mem, 'shorter>
    where
        'facet: 'shorter,
    {
        self.try_shrink_lifetime()
            .expect("shrink_lifetime requires a covariant type")
    }

    /// Tries to shrink the `'facet` lifetime parameter.
    ///
    /// Returns `Some` if the type is covariant (shrinking is safe),
    /// or `None` if the type is invariant or contravariant.
    #[inline]
    pub fn try_shrink_lifetime<'shorter>(self) -> Option<Peek<'mem, 'shorter>>
    where
        'facet: 'shorter,
    {
        if self.variance() == Variance::Covariant {
            Some(Peek {
                data: self.data,
                shape: self.shape,
                _invariant: PhantomData,
            })
        } else {
            None
        }
    }

    /// Grows the `'facet` lifetime parameter.
    ///
    /// This is safe for contravariant types: if a function accepts `'short`,
    /// it can also accept `'longer` (a longer lifetime is more restrictive).
    ///
    /// # Panics
    ///
    /// Panics if the type is not contravariant (i.e., if growing would be unsound).
    #[inline]
    pub fn grow_lifetime<'longer>(self) -> Peek<'mem, 'longer>
    where
        'longer: 'facet,
    {
        self.try_grow_lifetime()
            .expect("grow_lifetime requires a contravariant type")
    }

    /// Tries to grow the `'facet` lifetime parameter.
    ///
    /// Returns `Some` if the type is contravariant (growing is safe),
    /// or `None` if the type is invariant or covariant.
    #[inline]
    pub fn try_grow_lifetime<'longer>(self) -> Option<Peek<'mem, 'longer>>
    where
        'longer: 'facet,
    {
        if self.variance() == Variance::Contravariant {
            Some(Peek {
                data: self.data,
                shape: self.shape,
                _invariant: PhantomData,
            })
        } else {
            None
        }
    }

    /// Returns the vtable
    #[inline(always)]
    pub fn vtable(&self) -> VTableErased {
        self.shape.vtable
    }

    /// Returns a unique identifier for this value, usable for cycle detection
    #[inline]
    pub fn id(&self) -> ValueId {
        ValueId::new(self.shape, self.data.raw_ptr())
    }

    /// Returns true if the two values are pointer-equal
    #[inline]
    pub fn ptr_eq(&self, other: &Peek<'_, '_>) -> bool {
        self.data.raw_ptr() == other.data.raw_ptr()
    }

    /// Returns true if this scalar is equal to the other scalar
    ///
    /// # Returns
    ///
    /// `false` if equality comparison is not supported for this scalar type
    #[inline]
    pub fn partial_eq(&self, other: &Peek<'_, '_>) -> Result<bool, ReflectError> {
        if self.shape != other.shape {
            return Err(ReflectError::WrongShape {
                expected: self.shape,
                actual: other.shape,
            });
        }

        if let Some(result) = unsafe { self.shape.call_partial_eq(self.data, other.data) } {
            return Ok(result);
        }

        Err(ReflectError::OperationFailed {
            shape: self.shape(),
            operation: "partial_eq",
        })
    }

    /// Compares this scalar with another and returns their ordering
    ///
    /// # Returns
    ///
    /// `None` if comparison is not supported for this scalar type
    #[inline]
    pub fn partial_cmp(&self, other: &Peek<'_, '_>) -> Result<Option<Ordering>, ReflectError> {
        if self.shape != other.shape {
            return Err(ReflectError::WrongShape {
                expected: self.shape,
                actual: other.shape,
            });
        }

        if let Some(result) = unsafe { self.shape.call_partial_cmp(self.data, other.data) } {
            return Ok(result);
        }

        Err(ReflectError::OperationFailed {
            shape: self.shape(),
            operation: "partial_cmp",
        })
    }

    /// Hashes this scalar using the vtable hash function.
    ///
    /// # Returns
    ///
    /// `Err` if hashing is not supported for this scalar type, `Ok` otherwise
    #[inline(always)]
    pub fn hash(&self, hasher: &mut dyn core::hash::Hasher) -> Result<(), ReflectError> {
        let mut proxy = facet_core::HashProxy::new(hasher);
        if unsafe { self.shape.call_hash(self.data, &mut proxy) }.is_some() {
            return Ok(());
        }

        Err(ReflectError::OperationFailed {
            shape: self.shape(),
            operation: "hash",
        })
    }

    /// Computes a structural hash of this value.
    ///
    /// Unlike [`hash`](Self::hash), this method recursively traverses the structure
    /// and hashes each component, making it work for types that don't implement `Hash`.
    ///
    /// For scalars with a vtable hash function, it uses that. For compound types
    /// (structs, enums, lists, etc.), it recursively hashes the structure.
    ///
    /// This is useful for Merkle-tree style hashing where you want to compare
    /// subtrees for equality based on their structural content.
    pub fn structural_hash<H: core::hash::Hasher>(&self, hasher: &mut H) {
        use core::hash::Hash;

        // First, hash the shape's type identifier for type discrimination
        self.shape.id.hash(hasher);

        // Try vtable hash first for scalars
        let mut proxy = facet_core::HashProxy::new(hasher);
        if unsafe { self.shape.call_hash(self.data, &mut proxy) }.is_some() {
            return;
        }

        // Otherwise, traverse the structure recursively
        match self.shape.ty {
            Type::User(UserType::Struct(struct_type)) => {
                // Hash struct kind
                (struct_type.kind as u8).hash(hasher);

                // Hash each field, skipping metadata fields
                for field in struct_type.fields {
                    // Skip metadata fields - they don't affect structural identity
                    if field.is_metadata() {
                        continue;
                    }

                    // Hash field name
                    field.name.hash(hasher);

                    // Get field value and hash it recursively
                    let field_offset = field.offset;
                    let field_shape = field.shape();
                    let field_ptr = unsafe { self.data.field(field_offset) };
                    let field_peek = unsafe { Peek::unchecked_new(field_ptr, field_shape) };
                    field_peek.structural_hash(hasher);
                }
            }

            Type::User(UserType::Enum(_enum_type)) => {
                // Get the discriminant and variant
                if let Ok(peek_enum) = self.into_enum()
                    && let Ok(variant) = peek_enum.active_variant()
                {
                    // Hash variant name
                    variant.name.hash(hasher);

                    // Hash variant payload based on kind
                    match variant.data.kind {
                        StructKind::Unit => {
                            // No payload to hash
                        }
                        StructKind::TupleStruct | StructKind::Tuple => {
                            // Hash tuple fields (no names)
                            use super::HasFields;
                            for (_field, peek) in peek_enum.fields() {
                                peek.structural_hash(hasher);
                            }
                        }
                        StructKind::Struct => {
                            // Hash named fields
                            use super::HasFields;
                            for (field, peek) in peek_enum.fields() {
                                field.name.hash(hasher);
                                peek.structural_hash(hasher);
                            }
                        }
                    }
                }
            }

            _ => {
                // Handle Def-based types
                match self.shape.def {
                    Def::List(_) | Def::Array(_) | Def::Slice(_) => {
                        if let Ok(list_like) = self.into_list_like() {
                            // Hash length
                            list_like.len().hash(hasher);

                            // Hash each element
                            for elem in list_like.iter() {
                                elem.structural_hash(hasher);
                            }
                        }
                    }

                    Def::Map(_) => {
                        if let Ok(map) = self.into_map() {
                            // Hash length
                            map.len().hash(hasher);

                            // Hash each key-value pair
                            for (key, value) in map.iter() {
                                key.structural_hash(hasher);
                                value.structural_hash(hasher);
                            }
                        }
                    }

                    Def::Set(_) => {
                        if let Ok(set) = self.into_set() {
                            // Hash length
                            set.len().hash(hasher);

                            // Hash each element
                            for elem in set.iter() {
                                elem.structural_hash(hasher);
                            }
                        }
                    }

                    Def::Option(_) => {
                        if let Ok(opt) = self.into_option() {
                            if let Some(inner) = opt.value() {
                                true.hash(hasher);
                                inner.structural_hash(hasher);
                            } else {
                                false.hash(hasher);
                            }
                        }
                    }

                    Def::Result(_) => {
                        if let Ok(result) = self.into_result() {
                            if result.is_ok() {
                                0u8.hash(hasher);
                                if let Some(ok_val) = result.ok() {
                                    ok_val.structural_hash(hasher);
                                }
                            } else {
                                1u8.hash(hasher);
                                if let Some(err_val) = result.err() {
                                    err_val.structural_hash(hasher);
                                }
                            }
                        }
                    }

                    Def::Pointer(_) => {
                        if let Ok(ptr) = self.into_pointer()
                            && let Some(inner) = ptr.borrow_inner()
                        {
                            inner.structural_hash(hasher);
                        }
                    }

                    Def::DynamicValue(_) => {
                        if let Ok(dyn_val) = self.into_dynamic_value() {
                            // Hash based on dynamic value kind
                            dyn_val.structural_hash_inner(hasher);
                        }
                    }

                    Def::NdArray(_) => {
                        // For ndarray, hash the dimensions and data
                        if let Ok(arr) = self.into_ndarray() {
                            let n_dim = arr.n_dim();
                            n_dim.hash(hasher);
                            for i in 0..n_dim {
                                if let Some(dim) = arr.dim(i) {
                                    dim.hash(hasher);
                                }
                            }
                            // Hash each element
                            let count = arr.count();
                            for i in 0..count {
                                if let Some(elem) = arr.get(i) {
                                    elem.structural_hash(hasher);
                                }
                            }
                        }
                    }

                    Def::Scalar | Def::Undefined | _ => {
                        // Try to handle f32/f64 by hashing their bit representation
                        match self.scalar_type() {
                            Some(ScalarType::F32) => {
                                if let Ok(v) = self.get::<f32>() {
                                    v.to_bits().hash(hasher);
                                    return;
                                }
                            }
                            Some(ScalarType::F64) => {
                                if let Ok(v) = self.get::<f64>() {
                                    v.to_bits().hash(hasher);
                                    return;
                                }
                            }
                            _ => {}
                        }
                        panic!(
                            "structural_hash: type {} has no Hash impl and cannot be structurally hashed",
                            self.shape
                        );
                    }
                }
            }
        }
    }

    /// Returns the type name of this scalar
    ///
    /// # Arguments
    ///
    /// * `f` - A mutable reference to a `core::fmt::Formatter`
    /// * `opts` - The `TypeNameOpts` to use for formatting
    ///
    /// # Returns
    ///
    /// The result of the type name formatting
    #[inline(always)]
    pub fn type_name(
        &self,
        f: &mut core::fmt::Formatter<'_>,
        opts: TypeNameOpts,
    ) -> core::fmt::Result {
        if let Some(type_name_fn) = self.shape.type_name {
            type_name_fn(self.shape, f, opts)
        } else {
            write!(f, "{}", self.shape.type_identifier)
        }
    }

    /// Returns the shape
    #[inline(always)]
    pub const fn shape(&self) -> &'static Shape {
        self.shape
    }

    /// Returns the data
    #[inline(always)]
    pub const fn data(&self) -> PtrConst {
        self.data
    }

    /// Get the scalar type if set.
    #[inline]
    pub fn scalar_type(&self) -> Option<ScalarType> {
        ScalarType::try_from_shape(self.shape)
    }

    /// Read the value from memory into a Rust value.
    ///
    /// # Panics
    ///
    /// Panics if the shape doesn't match the type `T`.
    #[inline]
    pub fn get<T: Facet<'facet> + ?Sized>(&self) -> Result<&'mem T, ReflectError> {
        if self.shape != T::SHAPE {
            Err(ReflectError::WrongShape {
                expected: self.shape,
                actual: T::SHAPE,
            })
        } else {
            Ok(unsafe { self.data.get::<T>() })
        }
    }

    /// Try to get the value as a string if it's a string type
    /// Returns None if the value is not a string or couldn't be extracted
    pub fn as_str(&self) -> Option<&'mem str> {
        let peek = self.innermost_peek();
        // ScalarType::Str matches both bare `str` and `&str`.
        // For bare `str` (not a pointer), data points to str bytes directly.
        // For `&str`, let it fall through to the pointer handler below.
        if let Some(ScalarType::Str) = peek.scalar_type()
            && !matches!(peek.shape.ty, Type::Pointer(_))
        {
            // Bare `str`: data is a wide pointer to str bytes.
            // get::<str>() creates a &str reference to that data.
            return unsafe { Some(peek.data.get::<str>()) };
        }
        #[cfg(feature = "alloc")]
        if let Some(ScalarType::String) = peek.scalar_type() {
            return unsafe { Some(peek.data.get::<alloc::string::String>().as_str()) };
        }
        #[cfg(feature = "alloc")]
        if let Some(ScalarType::CowStr) = peek.scalar_type() {
            return unsafe { Some(peek.data.get::<alloc::borrow::Cow<'mem, str>>().as_ref()) };
        }

        // Handle references, including nested references like &&str
        if let Type::Pointer(PointerType::Reference(vpt)) = peek.shape.ty {
            let target_shape = vpt.target;

            // Check if this is a nested reference (&&str) first
            if let Type::Pointer(PointerType::Reference(inner_vpt)) = target_shape.ty {
                let inner_target_shape = inner_vpt.target;
                if let Some(ScalarType::Str) = ScalarType::try_from_shape(inner_target_shape) {
                    // For &&str, we need to dereference twice.
                    // Read the outer reference (8 bytes) as a pointer to &str, then dereference
                    let outer_ptr: *const *const &str =
                        unsafe { peek.data.as_ptr::<*const &str>() };
                    let inner_ref: &str = unsafe { **outer_ptr };
                    return Some(inner_ref);
                }
            } else if let Some(ScalarType::Str) = ScalarType::try_from_shape(target_shape)
                && !matches!(target_shape.ty, Type::Pointer(_))
            {
                // Simple case: &str (but only if target is not a pointer itself)
                return unsafe { Some(peek.data.get::<&str>()) };
            }
        }

        // Handle smart pointer types like Box<str>, Arc<str>, Rc<str>
        // These have Def::Pointer with pointee = str::SHAPE and a borrow_fn
        #[cfg(feature = "alloc")]
        if let Def::Pointer(ptr_def) = peek.shape.def
            && let Some(pointee_shape) = ptr_def.pointee
            && let Some(ScalarType::Str) = ScalarType::try_from_shape(pointee_shape)
            && let Some(borrow_fn) = ptr_def.vtable.borrow_fn
        {
            // borrow_fn returns a PtrConst pointing to the inner str
            let inner_ptr = unsafe { borrow_fn(peek.data) };
            // The inner ptr is a wide pointer to str
            return unsafe { Some(inner_ptr.get::<str>()) };
        }

        None
    }

    /// Try to get the value as a byte slice if it's a &[u8] type
    /// Returns None if the value is not a byte slice or couldn't be extracted
    #[inline]
    pub fn as_bytes(&self) -> Option<&'mem [u8]> {
        // Check if it's a direct &[u8]
        if let Type::Pointer(PointerType::Reference(vpt)) = self.shape.ty {
            let target_shape = vpt.target;
            if let Def::Slice(sd) = target_shape.def
                && sd.t().is_type::<u8>()
            {
                unsafe { return Some(self.data.get::<&[u8]>()) }
            }
        }
        None
    }

    /// Tries to identify this value as a struct
    #[inline]
    pub fn into_struct(self) -> Result<PeekStruct<'mem, 'facet>, ReflectError> {
        if let Type::User(UserType::Struct(ty)) = self.shape.ty {
            Ok(PeekStruct { value: self, ty })
        } else {
            Err(ReflectError::WasNotA {
                expected: "struct",
                actual: self.shape,
            })
        }
    }

    /// Tries to identify this value as an enum
    #[inline]
    pub fn into_enum(self) -> Result<PeekEnum<'mem, 'facet>, ReflectError> {
        if let Type::User(UserType::Enum(ty)) = self.shape.ty {
            Ok(PeekEnum { value: self, ty })
        } else {
            Err(ReflectError::WasNotA {
                expected: "enum",
                actual: self.shape,
            })
        }
    }

    /// Tries to identify this value as a map
    #[inline]
    pub fn into_map(self) -> Result<PeekMap<'mem, 'facet>, ReflectError> {
        if let Def::Map(def) = self.shape.def {
            // SAFETY: The MapDef comes from self.shape.def, where self.shape is obtained
            // from a trusted source (either T::SHAPE from the Facet trait, or validated
            // through other safe constructors). The vtable is therefore trusted.
            Ok(unsafe { PeekMap::new(self, def) })
        } else {
            Err(ReflectError::WasNotA {
                expected: "map",
                actual: self.shape,
            })
        }
    }

    /// Tries to identify this value as a set
    #[inline]
    pub fn into_set(self) -> Result<PeekSet<'mem, 'facet>, ReflectError> {
        if let Def::Set(def) = self.shape.def {
            // SAFETY: The SetDef comes from self.shape.def, where self.shape is obtained
            // from a trusted source (either T::SHAPE from the Facet trait, or validated
            // through other safe constructors). The vtable is therefore trusted.
            Ok(unsafe { PeekSet::new(self, def) })
        } else {
            Err(ReflectError::WasNotA {
                expected: "set",
                actual: self.shape,
            })
        }
    }

    /// Tries to identify this value as a list
    #[inline]
    pub fn into_list(self) -> Result<PeekList<'mem, 'facet>, ReflectError> {
        if let Def::List(def) = self.shape.def {
            // SAFETY: The ListDef comes from self.shape.def, where self.shape is obtained
            // from a trusted source (either T::SHAPE from the Facet trait, or validated
            // through other safe constructors). The vtable is therefore trusted.
            return Ok(unsafe { PeekList::new(self, def) });
        }

        Err(ReflectError::WasNotA {
            expected: "list",
            actual: self.shape,
        })
    }

    /// Tries to identify this value as a ndarray
    #[inline]
    pub fn into_ndarray(self) -> Result<PeekNdArray<'mem, 'facet>, ReflectError> {
        if let Def::NdArray(def) = self.shape.def {
            // SAFETY: The NdArrayDef comes from self.shape.def, where self.shape is obtained
            // from a trusted source (either T::SHAPE from the Facet trait, or validated
            // through other safe constructors). The vtable is therefore trusted.
            return Ok(unsafe { PeekNdArray::new(self, def) });
        }

        Err(ReflectError::WasNotA {
            expected: "ndarray",
            actual: self.shape,
        })
    }

    /// Tries to identify this value as a list, array or slice
    #[inline]
    pub fn into_list_like(self) -> Result<PeekListLike<'mem, 'facet>, ReflectError> {
        match self.shape.def {
            Def::List(def) => {
                // SAFETY: The ListDef comes from self.shape.def, where self.shape is obtained
                // from a trusted source (either T::SHAPE from the Facet trait, or validated
                // through other safe constructors). The vtable is therefore trusted.
                Ok(unsafe { PeekListLike::new(self, ListLikeDef::List(def)) })
            }
            Def::Array(def) => {
                // SAFETY: The ArrayDef comes from self.shape.def, where self.shape is obtained
                // from a trusted source (either T::SHAPE from the Facet trait, or validated
                // through other safe constructors). The vtable is therefore trusted.
                Ok(unsafe { PeekListLike::new(self, ListLikeDef::Array(def)) })
            }
            Def::Slice(def) => {
                // When we have a bare slice shape with a wide pointer,
                // it means we have a reference to a slice (e.g., from Arc<[T]>::borrow_inner)
                // SAFETY: The SliceDef comes from self.shape.def, where self.shape is obtained
                // from a trusted source (either T::SHAPE from the Facet trait, or validated
                // through other safe constructors). The vtable is therefore trusted.
                Ok(unsafe { PeekListLike::new(self, ListLikeDef::Slice(def)) })
            }
            _ => {
                // &[i32] is actually a _pointer_ to a slice.
                match self.shape.ty {
                    Type::Pointer(ptr) => match ptr {
                        PointerType::Reference(vpt) | PointerType::Raw(vpt) => {
                            let target = vpt.target;
                            match target.def {
                                Def::Slice(def) => {
                                    let ptr = unsafe { self.data.as_ptr::<*const [()]>() };
                                    let ptr = PtrConst::new(unsafe {
                                        NonNull::new_unchecked((*ptr) as *mut [()]).as_ptr()
                                    });
                                    let peek = unsafe { Peek::unchecked_new(ptr, def.t) };

                                    // SAFETY: The SliceDef comes from target.def, where target is obtained
                                    // from self.shape which comes from a trusted source. The vtable is therefore trusted.
                                    return Ok(unsafe {
                                        PeekListLike::new(peek, ListLikeDef::Slice(def))
                                    });
                                }
                                _ => {
                                    // well it's not list-like then
                                }
                            }
                        }
                        PointerType::Function(_) => {
                            // well that's not a list-like
                        }
                    },
                    _ => {
                        // well that's not a list-like either
                    }
                }

                Err(ReflectError::WasNotA {
                    expected: "list, array or slice",
                    actual: self.shape,
                })
            }
        }
    }

    /// Tries to identify this value as a pointer
    #[inline]
    pub fn into_pointer(self) -> Result<PeekPointer<'mem, 'facet>, ReflectError> {
        if let Def::Pointer(def) = self.shape.def {
            Ok(PeekPointer { value: self, def })
        } else {
            Err(ReflectError::WasNotA {
                expected: "smart pointer",
                actual: self.shape,
            })
        }
    }

    /// Tries to identify this value as an option
    #[inline]
    pub fn into_option(self) -> Result<PeekOption<'mem, 'facet>, ReflectError> {
        if let Def::Option(def) = self.shape.def {
            Ok(PeekOption { value: self, def })
        } else {
            Err(ReflectError::WasNotA {
                expected: "option",
                actual: self.shape,
            })
        }
    }

    /// Tries to identify this value as a result
    #[inline]
    pub fn into_result(self) -> Result<PeekResult<'mem, 'facet>, ReflectError> {
        if let Def::Result(def) = self.shape.def {
            Ok(PeekResult { value: self, def })
        } else {
            Err(ReflectError::WasNotA {
                expected: "result",
                actual: self.shape,
            })
        }
    }

    /// Tries to identify this value as a tuple
    #[inline]
    pub fn into_tuple(self) -> Result<PeekTuple<'mem, 'facet>, ReflectError> {
        if let Type::User(UserType::Struct(struct_type)) = self.shape.ty {
            if struct_type.kind == StructKind::Tuple {
                Ok(PeekTuple {
                    value: self,
                    ty: TupleType {
                        fields: struct_type.fields,
                    },
                })
            } else {
                Err(ReflectError::WasNotA {
                    expected: "tuple",
                    actual: self.shape,
                })
            }
        } else {
            Err(ReflectError::WasNotA {
                expected: "tuple",
                actual: self.shape,
            })
        }
    }

    /// Tries to identify this value as a dynamic value (like `facet_value::Value`)
    #[inline]
    pub fn into_dynamic_value(self) -> Result<PeekDynamicValue<'mem, 'facet>, ReflectError> {
        if let Def::DynamicValue(def) = self.shape.def {
            Ok(PeekDynamicValue { value: self, def })
        } else {
            Err(ReflectError::WasNotA {
                expected: "dynamic value",
                actual: self.shape,
            })
        }
    }

    /// Tries to return the innermost value — useful for serialization. For example, we serialize a `NonZero<u8>` the same
    /// as a `u8`. Similarly, we serialize a `Utf8PathBuf` the same as a `String.
    ///
    /// Returns a `Peek` to the innermost value, unwrapping transparent wrappers recursively.
    /// For example, this will peel through newtype wrappers or smart pointers that have an `inner`.
    pub fn innermost_peek(self) -> Self {
        let mut current_peek = self;
        while let Some(inner_shape) = current_peek.shape.inner {
            // Try to borrow the inner value
            let result = unsafe { current_peek.shape.call_try_borrow_inner(current_peek.data) };
            match result {
                Some(Ok(inner_data)) => {
                    current_peek = Peek {
                        data: inner_data.as_const(),
                        shape: inner_shape,
                        _invariant: PhantomData,
                    };
                }
                Some(Err(e)) => {
                    panic!(
                        "innermost_peek: try_borrow_inner returned an error! was trying to go from {} to {}. error: {e}",
                        current_peek.shape, inner_shape
                    );
                }
                None => {
                    // No try_borrow_inner function, stop here
                    break;
                }
            }
        }
        current_peek
    }

    /// Performs custom serialization of the current peek using the provided field's metadata.
    ///
    /// Returns an `OwnedPeek` that points to the final type that should be serialized in place
    /// of the current peek.
    #[cfg(feature = "alloc")]
    pub fn custom_serialization(&self, field: Field) -> Result<OwnedPeek<'mem>, ReflectError> {
        let Some(proxy_def) = field.proxy() else {
            return Err(ReflectError::OperationFailed {
                shape: self.shape,
                operation: "field does not have a proxy definition",
            });
        };

        let target_shape = proxy_def.shape;
        let tptr = target_shape.allocate().map_err(|_| ReflectError::Unsized {
            shape: target_shape,
            operation: "Not a Sized type",
        })?;
        let ser_res = unsafe { (proxy_def.convert_out)(self.data(), tptr) };
        let err = match ser_res {
            Ok(rptr) => {
                if rptr.as_uninit() != tptr {
                    ReflectError::CustomSerializationError {
                        message: "convert_out did not return the expected pointer".into(),
                        src_shape: self.shape,
                        dst_shape: target_shape,
                    }
                } else {
                    return Ok(OwnedPeek {
                        shape: target_shape,
                        data: rptr,
                        _phantom: PhantomData,
                    });
                }
            }
            Err(message) => ReflectError::CustomSerializationError {
                message,
                src_shape: self.shape,
                dst_shape: target_shape,
            },
        };
        // if we reach here we have an error and we need to deallocate the target allocation
        unsafe {
            // SAFETY: unwrap should be ok since the allocation was ok
            target_shape.deallocate_uninit(tptr).unwrap()
        };
        Err(err)
    }

    /// Returns an `OwnedPeek` using the shape's container-level proxy for serialization.
    ///
    /// This is used when a type has `#[facet(proxy = ProxyType)]` at the container level.
    /// Unlike field-level proxies which are checked via `custom_serialization(field)`,
    /// this method checks the Shape itself for a proxy definition.
    ///
    /// Returns `None` if the shape has no container-level proxy.
    #[cfg(feature = "alloc")]
    pub fn custom_serialization_from_shape(&self) -> Result<Option<OwnedPeek<'mem>>, ReflectError> {
        let Some(proxy_def) = self.shape.proxy else {
            return Ok(None);
        };

        let target_shape = proxy_def.shape;
        let tptr = target_shape.allocate().map_err(|_| ReflectError::Unsized {
            shape: target_shape,
            operation: "Not a Sized type",
        })?;

        let ser_res = unsafe { (proxy_def.convert_out)(self.data(), tptr) };
        let err = match ser_res {
            Ok(rptr) => {
                if rptr.as_uninit() != tptr {
                    ReflectError::CustomSerializationError {
                        message: "proxy convert_out did not return the expected pointer".into(),
                        src_shape: self.shape,
                        dst_shape: target_shape,
                    }
                } else {
                    return Ok(Some(OwnedPeek {
                        shape: target_shape,
                        data: rptr,
                        _phantom: PhantomData,
                    }));
                }
            }
            Err(message) => ReflectError::CustomSerializationError {
                message,
                src_shape: self.shape,
                dst_shape: target_shape,
            },
        };

        // if we reach here we have an error and we need to deallocate the target allocation
        unsafe {
            // SAFETY: unwrap should be ok since the allocation was ok
            target_shape.deallocate_uninit(tptr).unwrap()
        };
        Err(err)
    }
}

impl<'mem, 'facet> core::fmt::Display for Peek<'mem, 'facet> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(result) = unsafe { self.shape.call_display(self.data, f) } {
            return result;
        }
        write!(f, "⟨{}⟩", self.shape)
    }
}

impl<'mem, 'facet> core::fmt::Debug for Peek<'mem, 'facet> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(result) = unsafe { self.shape.call_debug(self.data, f) } {
            return result;
        }

        write!(f, "⟨{}⟩", self.shape)
    }
}

impl<'mem, 'facet> core::cmp::PartialEq for Peek<'mem, 'facet> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.partial_eq(other).unwrap_or(false)
    }
}

impl<'mem, 'facet> core::cmp::PartialOrd for Peek<'mem, 'facet> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.partial_cmp(other).unwrap_or(None)
    }
}

impl<'mem, 'facet> core::hash::Hash for Peek<'mem, 'facet> {
    fn hash<H: core::hash::Hasher>(&self, hasher: &mut H) {
        self.hash(hasher)
            .expect("Hashing is not supported for this shape");
    }
}

/// A covariant wrapper around [`Peek`] for types that are covariant over their lifetime parameter.
///
/// Unlike [`Peek`], which is invariant over `'facet` for soundness reasons,
/// `CovariantPeek` is **covariant** over `'facet`. This means a `CovariantPeek<'mem, 'static>`
/// can be used where a `CovariantPeek<'mem, 'a>` is expected.
///
/// # When to Use
///
/// Use `CovariantPeek` when you need to:
/// - Store multiple `Peek` values with different lifetimes in a single collection
/// - Pass `Peek` values to functions expecting shorter lifetimes
/// - Build data structures that wrap `Peek` without forcing invariance on the wrapper
///
/// # Safety
///
/// `CovariantPeek` can only be constructed from types that are actually covariant.
/// The constructor verifies this at runtime by checking [`Shape::computed_variance`].
/// This ensures that lifetime shrinking is always safe.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_reflect::{Peek, CovariantPeek};
///
/// #[derive(Facet)]
/// struct Data<'a> {
///     value: &'a str,
/// }
///
/// // Data<'a> is covariant in 'a because &'a str is covariant
/// let data = Data { value: "hello" };
/// let peek: Peek<'_, 'static> = Peek::new(&data);
///
/// // Convert to CovariantPeek - this verifies covariance
/// let covariant = CovariantPeek::new(peek).expect("Data is covariant");
///
/// // Now we can use it where shorter lifetimes are expected
/// fn use_shorter<'a>(p: CovariantPeek<'_, 'a>) {
///     let _ = p;
/// }
/// use_shorter(covariant);
/// ```
#[derive(Clone, Copy)]
pub struct CovariantPeek<'mem, 'facet> {
    /// Underlying data
    data: PtrConst,

    /// Shape of the value
    shape: &'static Shape,

    // Covariant over both 'mem and 'facet: CovariantPeek<'mem, 'static> can be used where
    // CovariantPeek<'mem, 'a> is expected.
    //
    // This is safe ONLY because we verify at construction time that the underlying
    // type is covariant. For covariant types, shrinking lifetimes is always safe.
    _covariant: PhantomData<(&'mem (), &'facet ())>,
}

impl<'mem, 'facet> CovariantPeek<'mem, 'facet> {
    /// Creates a new `CovariantPeek` from a `Peek`, verifying that the underlying type is covariant.
    ///
    /// Returns `None` if the type is not covariant (i.e., it's contravariant or invariant).
    ///
    /// # Example
    ///
    /// ```
    /// use facet::Facet;
    /// use facet_reflect::{Peek, CovariantPeek};
    ///
    /// // i32 has no lifetime parameters, so it's covariant
    /// let value = 42i32;
    /// let peek = Peek::new(&value);
    /// let covariant = CovariantPeek::new(peek);
    /// assert!(covariant.is_some());
    /// ```
    #[inline]
    pub fn new(peek: Peek<'mem, 'facet>) -> Option<Self> {
        if peek.variance() == Variance::Covariant {
            Some(Self {
                data: peek.data,
                shape: peek.shape,
                _covariant: PhantomData,
            })
        } else {
            None
        }
    }

    /// Creates a new `CovariantPeek` from a `Peek`, panicking if the type is not covariant.
    ///
    /// # Panics
    ///
    /// Panics if the underlying type is not covariant.
    ///
    /// # Example
    ///
    /// ```
    /// use facet::Facet;
    /// use facet_reflect::{Peek, CovariantPeek};
    ///
    /// let value = "hello";
    /// let peek = Peek::new(&value);
    /// let covariant = CovariantPeek::new_unchecked(peek); // Will succeed
    /// ```
    #[inline]
    pub fn new_unchecked(peek: Peek<'mem, 'facet>) -> Self {
        Self::new(peek).unwrap_or_else(|| {
            panic!(
                "CovariantPeek::new_unchecked called on non-covariant type {} (variance: {:?})",
                peek.shape,
                peek.variance()
            )
        })
    }

    /// Creates a `CovariantPeek` directly from a covariant `Facet` type.
    ///
    /// Returns `None` if the type is not covariant.
    ///
    /// # Example
    ///
    /// ```
    /// use facet::Facet;
    /// use facet_reflect::CovariantPeek;
    ///
    /// let value = 42i32;
    /// let covariant = CovariantPeek::from_ref(&value);
    /// assert!(covariant.is_some());
    /// ```
    #[inline]
    pub fn from_ref<T: Facet<'facet> + ?Sized>(t: &'mem T) -> Option<Self> {
        Self::new(Peek::new(t))
    }

    /// Returns the underlying `Peek`.
    ///
    /// Note that the returned `Peek` is invariant, so you cannot use it to
    /// shrink lifetimes directly. Use `CovariantPeek` for lifetime flexibility.
    #[inline]
    pub fn into_peek(self) -> Peek<'mem, 'facet> {
        Peek {
            data: self.data,
            shape: self.shape,
            _invariant: PhantomData,
        }
    }

    /// Returns the shape of the underlying value.
    #[inline]
    pub const fn shape(&self) -> &'static Shape {
        self.shape
    }

    /// Returns the data pointer.
    #[inline]
    pub const fn data(&self) -> PtrConst {
        self.data
    }
}

impl<'mem, 'facet> core::ops::Deref for CovariantPeek<'mem, 'facet> {
    type Target = Peek<'mem, 'facet>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        // SAFETY: CovariantPeek and Peek have the same memory layout for the
        // data and shape fields. The PhantomData fields don't affect layout.
        // We're creating a reference to a Peek that views the same data.
        //
        // This is safe because:
        // 1. We only construct CovariantPeek from covariant types
        // 2. The Peek reference we return has the same lifetime bounds
        // 3. We're not allowing mutation through this reference
        unsafe { &*(self as *const CovariantPeek<'mem, 'facet> as *const Peek<'mem, 'facet>) }
    }
}

impl<'mem, 'facet> core::fmt::Debug for CovariantPeek<'mem, 'facet> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CovariantPeek")
            .field("shape", &self.shape)
            .field("data", &self.data)
            .finish()
    }
}

impl<'mem, 'facet> core::fmt::Display for CovariantPeek<'mem, 'facet> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(&**self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test for issue #1082: UB in `Peek("").as_str()`
    /// Previously, `as_str()` used `get::<&str>()` which tried to read a fat pointer
    /// from the str data, causing UB for empty strings (reading 16 bytes from 0-byte allocation).
    #[test]
    fn test_peek_as_str_empty_string() {
        let peek = Peek::new("");
        assert_eq!(peek.as_str(), Some(""));
    }

    #[test]
    fn test_peek_as_str_non_empty_string() {
        let peek = Peek::new("hello");
        assert_eq!(peek.as_str(), Some("hello"));
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn test_peek_as_str_owned_string() {
        let s = alloc::string::String::from("owned string");
        let peek = Peek::new(&s);
        assert_eq!(peek.as_str(), Some("owned string"));
    }

    /// Regression test for issue #794: Peek::as_str() with double reference
    /// Previously, this would cause UB when trying to read &&str as &str
    #[test]
    fn test_peek_as_str_double_reference() {
        let value = &"hello";
        let peek = Peek::new(&value);
        assert_eq!(peek.as_str(), Some("hello"));
    }

    #[test]
    fn test_covariant_peek_from_covariant_type() {
        // i32 has no lifetime parameters, so it's covariant
        let value = 42i32;
        let peek = Peek::new(&value);
        let covariant = CovariantPeek::new(peek);
        assert!(covariant.is_some());

        // Verify we can access Peek methods through Deref
        let covariant = covariant.unwrap();
        assert_eq!(covariant.shape(), peek.shape());
    }

    #[test]
    fn test_covariant_peek_from_ref() {
        let value = 42i32;
        let covariant = CovariantPeek::from_ref(&value);
        assert!(covariant.is_some());
    }

    #[test]
    fn test_covariant_peek_deref_to_peek() {
        let value = "hello";
        let peek = Peek::new(&value);
        let covariant = CovariantPeek::new(peek).unwrap();

        // Test that Deref works - we can call Peek methods directly
        assert_eq!(covariant.as_str(), Some("hello"));
        assert_eq!(covariant.shape(), peek.shape());
    }

    #[test]
    fn test_covariant_peek_into_peek() {
        let value = 42i32;
        let original_peek = Peek::new(&value);
        let covariant = CovariantPeek::new(original_peek).unwrap();
        let recovered_peek = covariant.into_peek();

        assert_eq!(recovered_peek.shape(), original_peek.shape());
    }

    #[test]
    fn test_covariant_peek_lifetime_covariance() {
        // This test verifies that CovariantPeek is actually covariant over 'facet
        // by passing a CovariantPeek<'_, 'static> to a function expecting CovariantPeek<'_, 'a>
        fn use_shorter<'a>(_p: CovariantPeek<'_, 'a>) {}

        let value = 42i32;
        let covariant: CovariantPeek<'_, 'static> = CovariantPeek::from_ref(&value).unwrap();

        // This compiles because CovariantPeek is covariant over 'facet
        use_shorter(covariant);
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn test_covariant_peek_vec_type() {
        // Vec<T> is covariant in T
        let vec = alloc::vec![1i32, 2, 3];
        let peek = Peek::new(&vec);
        let covariant = CovariantPeek::new(peek);
        assert!(covariant.is_some());
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn test_covariant_peek_option_type() {
        // Option<T> is covariant in T
        let opt = Some(42i32);
        let peek = Peek::new(&opt);
        let covariant = CovariantPeek::new(peek);
        assert!(covariant.is_some());
    }

    #[test]
    fn test_spanned_structural_hash_ignores_span() {
        use crate::{Span, Spanned};
        use core::hash::Hasher;
        use std::hash::DefaultHasher;

        // Two Spanned values with same inner value but different spans
        let a = Spanned::new(42i32, Span::new(0, 10));
        let b = Spanned::new(42i32, Span::new(100, 20));

        // They should have the same structural hash
        let mut hasher_a = DefaultHasher::new();
        Peek::new(&a).structural_hash(&mut hasher_a);
        let hash_a = hasher_a.finish();

        let mut hasher_b = DefaultHasher::new();
        Peek::new(&b).structural_hash(&mut hasher_b);
        let hash_b = hasher_b.finish();

        assert_eq!(
            hash_a, hash_b,
            "Spanned values with same inner value should have same structural hash"
        );
    }

    #[test]
    fn test_spanned_structural_hash_differs_for_different_values() {
        use crate::{Span, Spanned};
        use core::hash::Hasher;
        use std::hash::DefaultHasher;

        // Two Spanned values with different inner values
        let a = Spanned::new(42i32, Span::new(0, 10));
        let b = Spanned::new(99i32, Span::new(0, 10));

        // They should have different structural hashes
        let mut hasher_a = DefaultHasher::new();
        Peek::new(&a).structural_hash(&mut hasher_a);
        let hash_a = hasher_a.finish();

        let mut hasher_b = DefaultHasher::new();
        Peek::new(&b).structural_hash(&mut hasher_b);
        let hash_b = hasher_b.finish();

        assert_ne!(
            hash_a, hash_b,
            "Spanned values with different inner values should have different structural hashes"
        );
    }

    #[test]
    fn test_spanned_field_metadata() {
        use crate::Spanned;
        use facet_core::{Type, UserType};

        // Get the shape for Spanned<i32>
        let shape = <Spanned<i32> as facet_core::Facet>::SHAPE;

        // Extract the struct type
        let struct_type = match shape.ty {
            Type::User(UserType::Struct(st)) => st,
            _ => panic!("Expected struct type"),
        };

        // Find the span field and verify it has metadata = "span"
        let span_field = struct_type
            .fields
            .iter()
            .find(|f| f.name == "span")
            .expect("Should have span field");

        assert!(
            span_field.is_metadata(),
            "span field should be marked as metadata"
        );
        assert_eq!(
            span_field.metadata_kind(),
            Some("span"),
            "span field should have metadata kind 'span'"
        );

        // Verify the value field is NOT metadata
        let value_field = struct_type
            .fields
            .iter()
            .find(|f| f.name == "value")
            .expect("Should have value field");

        assert!(
            !value_field.is_metadata(),
            "value field should not be marked as metadata"
        );
    }
}
