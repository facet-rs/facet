//////////////////////////////////////////////////////////////////////
// VTable types
//////////////////////////////////////////////////////////////////////

use crate::{OxPtrConst, OxPtrMut, PtrMut};
use alloc::string::String;
use core::{cmp, fmt, hash::Hasher, marker::PhantomData, mem::transmute};

//////////////////////////////////////////////////////////////////////
// TypeNameOpts - options for formatting type names
//////////////////////////////////////////////////////////////////////

/// Options for formatting the name of a type.
///
/// Controls recursion depth when printing nested types like `Option<Vec<String>>`.
#[derive(Clone, Copy)]
pub struct TypeNameOpts {
    /// As long as this is > 0, keep formatting the type parameters.
    /// When it reaches 0, format type parameters as `…`.
    /// If negative, all type parameters are formatted (infinite recursion).
    pub recurse_ttl: isize,
}

impl Default for TypeNameOpts {
    #[inline]
    fn default() -> Self {
        Self { recurse_ttl: -1 }
    }
}

impl TypeNameOpts {
    /// Create options where no type parameters are formatted (just `…`).
    #[inline]
    pub const fn none() -> Self {
        Self { recurse_ttl: 0 }
    }

    /// Create options where only direct children are formatted.
    #[inline]
    pub const fn one() -> Self {
        Self { recurse_ttl: 1 }
    }

    /// Create options where all type parameters are formatted (infinite depth).
    #[inline]
    pub const fn infinite() -> Self {
        Self { recurse_ttl: -1 }
    }

    /// Decrease the `recurse_ttl` for child type parameters.
    ///
    /// Returns `None` if you should render `…` instead of type parameters.
    /// Returns `Some(opts)` with decremented TTL to pass to children.
    #[inline]
    pub const fn for_children(&self) -> Option<Self> {
        if self.recurse_ttl == 0 {
            None
        } else if self.recurse_ttl < 0 {
            Some(*self)
        } else {
            Some(Self {
                recurse_ttl: self.recurse_ttl - 1,
            })
        }
    }
}

/// Function pointer type for formatting type names.
///
/// Takes the shape (for accessing type params) and formatting options.
/// This lives on `Shape`, not in the vtable, because it's about the type itself,
/// not about values of the type.
pub type TypeNameFn =
    fn(shape: &'static crate::Shape, f: &mut fmt::Formatter<'_>, opts: TypeNameOpts) -> fmt::Result;

//////////////////////////////////////////////////////////////////////
// HashProxy - Type-erased Hasher for vtable use
//////////////////////////////////////////////////////////////////////

/// A proxy type that wraps `&mut dyn Hasher` and implements `Hasher`.
///
/// This allows storing a concrete `Hash::hash::<HashProxy>` function pointer
/// in the vtable, avoiding the generic `H: Hasher` parameter.
///
/// # Example
///
/// ```ignore
/// // In vtable builder:
/// .hash(<MyType as Hash>::hash::<HashProxy>)
///
/// // At call site:
/// let mut proxy = HashProxy::new(&mut my_hasher);
/// unsafe { (vtable.hash.unwrap())(ptr, &mut proxy) };
/// ```
pub struct HashProxy<'a> {
    inner: &'a mut dyn Hasher,
}

impl<'a> HashProxy<'a> {
    /// Create a new HashProxy wrapping a hasher.
    #[inline]
    pub fn new(hasher: &'a mut dyn Hasher) -> Self {
        Self { inner: hasher }
    }
}

impl Hasher for HashProxy<'_> {
    #[inline]
    fn finish(&self) -> u64 {
        self.inner.finish()
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        self.inner.write(bytes)
    }

    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.inner.write_u8(i)
    }

    #[inline]
    fn write_u16(&mut self, i: u16) {
        self.inner.write_u16(i)
    }

    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.inner.write_u32(i)
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.inner.write_u64(i)
    }

    #[inline]
    fn write_u128(&mut self, i: u128) {
        self.inner.write_u128(i)
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.inner.write_usize(i)
    }

    #[inline]
    fn write_i8(&mut self, i: i8) {
        self.inner.write_i8(i)
    }

    #[inline]
    fn write_i16(&mut self, i: i16) {
        self.inner.write_i16(i)
    }

    #[inline]
    fn write_i32(&mut self, i: i32) {
        self.inner.write_i32(i)
    }

    #[inline]
    fn write_i64(&mut self, i: i64) {
        self.inner.write_i64(i)
    }

    #[inline]
    fn write_i128(&mut self, i: i128) {
        self.inner.write_i128(i)
    }

    #[inline]
    fn write_isize(&mut self, i: isize) {
        self.inner.write_isize(i)
    }
}

//////////////////////////////////////////////////////////////////////
// VTableDirect - For concrete types
//////////////////////////////////////////////////////////////////////

/// VTable for concrete types with compile-time known traits.
///
/// Uses thin pointers (`*const ()`, `*mut ()`) as receivers.
/// Used for scalars, String, user-defined structs/enums, etc.
///
/// ## Per-type operations
///
/// Note that `drop_in_place`, `default_in_place`, and `clone_into` are NOT in this struct.
/// These operations must be monomorphized per-type and live in [`TypeOps`] on the [`crate::Shape`].
/// This allows vtables to be shared across generic instantiations (e.g., one vtable for all `Vec<T>`).
///
/// ## Safety
///
/// All function pointers are `unsafe fn` because they operate on raw pointers.
/// Callers must ensure:
/// - The pointer points to a valid instance of the expected type
/// - The pointer has the correct alignment for the type
/// - For mutable operations, the caller has exclusive access to the data
/// - The lifetime of the data extends for the duration of the operation
#[allow(clippy::type_complexity)]
#[derive(Clone, Copy)]
pub struct VTableDirect {
    /// Display function - formats value using Display trait.
    pub display: Option<unsafe fn(*const (), &mut fmt::Formatter<'_>) -> fmt::Result>,

    /// Debug function - formats value using Debug trait.
    pub debug: Option<unsafe fn(*const (), &mut fmt::Formatter<'_>) -> fmt::Result>,

    /// Hash function - hashes value using Hash trait via HashProxy.
    pub hash: Option<unsafe fn(*const (), &mut HashProxy<'_>)>,

    /// Invariants function - checks type invariants.
    pub invariants: Option<unsafe fn(*const ()) -> Result<(), String>>,

    /// Parse function - parses value from string into destination.
    pub parse: Option<unsafe fn(&str, *mut ()) -> Result<(), crate::ParseError>>,

    /// Parse bytes function - parses value from byte slice into destination.
    /// Used for binary formats where types have a more efficient representation.
    pub parse_bytes: Option<unsafe fn(&[u8], *mut ()) -> Result<(), crate::ParseError>>,

    /// Try from function - converts from another value type.
    ///
    /// # Arguments
    /// - `dst`: Destination pointer where the converted value will be written
    /// - `src_shape`: Shape of the source type
    /// - `src_ptr`: Pointer to the source value
    ///
    /// # Return Value
    ///
    /// Returns [`TryFromOutcome`] which encodes both the result and ownership semantics:
    ///
    /// - [`TryFromOutcome::Converted`]: Success. Source was consumed.
    /// - [`TryFromOutcome::Unsupported`]: Source type not supported. Source was NOT consumed.
    /// - [`TryFromOutcome::Failed`]: Conversion failed. Source WAS consumed.
    ///
    /// This design allows callers to attempt multiple `try_from` conversions in sequence
    /// until one succeeds, without losing the source value on type mismatches.
    ///
    /// # Implementation Guidelines
    ///
    /// Implementations should follow this pattern:
    /// ```ignore
    /// // Check type BEFORE consuming - only consume supported types
    /// if src_shape.id == <String as Facet>::SHAPE.id {
    ///     let value = src.read::<String>();  // Consume the value
    ///     match convert(value) {
    ///         Ok(converted) => {
    ///             unsafe { dst.write(converted) };
    ///             TryFromOutcome::Converted
    ///         }
    ///         Err(e) => TryFromOutcome::Failed(e.into()),
    ///     }
    /// } else if src_shape.id == <&str as Facet>::SHAPE.id {
    ///     // Copy types can use get() since they're trivially copyable
    ///     let value: &str = *src.get::<&str>();
    ///     match convert_str(value) {
    ///         Ok(converted) => {
    ///             unsafe { dst.write(converted) };
    ///             TryFromOutcome::Converted
    ///         }
    ///         Err(e) => TryFromOutcome::Failed(e.into()),
    ///     }
    /// } else {
    ///     // Unsupported type - return WITHOUT consuming
    ///     TryFromOutcome::Unsupported
    /// }
    /// ```
    ///
    /// # Safety
    ///
    /// - `dst` must be valid for writes and properly aligned for the destination type
    /// - `src_ptr` must point to valid, initialized memory of the type described by `src_shape`
    pub try_from: Option<
        unsafe fn(*mut (), &'static crate::Shape, crate::PtrConst) -> crate::TryFromOutcome,
    >,

    /// Try into inner function - extracts inner value (consuming).
    pub try_into_inner: Option<unsafe fn(*mut ()) -> Result<PtrMut, String>>,

    /// Try borrow inner function - borrows inner value.
    pub try_borrow_inner: Option<unsafe fn(*const ()) -> Result<PtrMut, String>>,

    /// PartialEq function - tests equality with another value.
    pub partial_eq: Option<unsafe fn(*const (), *const ()) -> bool>,

    /// PartialOrd function - compares with another value.
    pub partial_cmp: Option<unsafe fn(*const (), *const ()) -> Option<cmp::Ordering>>,

    /// Ord function - total ordering comparison.
    pub cmp: Option<unsafe fn(*const (), *const ()) -> cmp::Ordering>,
}

impl Default for VTableDirect {
    fn default() -> Self {
        Self::empty()
    }
}

impl VTableDirect {
    /// Create an empty VTableDirect with all fields set to None.
    pub const fn empty() -> Self {
        Self {
            display: None,
            debug: None,
            hash: None,
            invariants: None,
            parse: None,
            parse_bytes: None,
            try_from: None,
            try_into_inner: None,
            try_borrow_inner: None,
            partial_eq: None,
            partial_cmp: None,
            cmp: None,
        }
    }

    /// Start building a new VTableDirect (untyped).
    pub const fn builder() -> Self {
        Self::empty()
    }
}

//////////////////////////////////////////////////////////////////////
// VTableIndirect - For generic containers
//////////////////////////////////////////////////////////////////////

/// VTable for generic containers with runtime trait resolution.
///
/// Uses `OxPtrConst`/`OxPtrMut` as receivers to access inner type's shape at runtime.
/// Used for `Vec<T>`, `Option<T>`, `Arc<T>`, etc.
///
/// Returns `Option` to indicate whether the operation is supported
/// (the inner type may not implement the required trait).
///
/// ## Per-type operations
///
/// Note that `drop_in_place`, `default_in_place`, and `clone_into` are NOT in this struct.
/// These operations must be monomorphized per-type and live in [`TypeOps`] on the [`crate::Shape`].
/// This allows vtables to be shared across generic instantiations (e.g., one vtable for all `Vec<T>`).
///
/// ## Safety
///
/// All function pointers are `unsafe fn` because they operate on raw pointers.
/// Callers must ensure:
/// - The pointer points to a valid instance of the expected type
/// - The pointer has the correct alignment for the type
/// - For mutable operations, the caller has exclusive access to the data
/// - The lifetime of the data extends for the duration of the operation
#[allow(clippy::type_complexity)]
#[derive(Clone, Copy)]
pub struct VTableIndirect {
    /// Display function - formats value using Display trait.
    pub display: Option<unsafe fn(OxPtrConst, &mut fmt::Formatter<'_>) -> Option<fmt::Result>>,

    /// Debug function - formats value using Debug trait.
    pub debug: Option<unsafe fn(OxPtrConst, &mut fmt::Formatter<'_>) -> Option<fmt::Result>>,

    /// Hash function - hashes value using Hash trait via HashProxy.
    pub hash: Option<unsafe fn(OxPtrConst, &mut HashProxy<'_>) -> Option<()>>,

    /// Invariants function - checks type invariants.
    pub invariants: Option<unsafe fn(OxPtrConst) -> Option<Result<(), String>>>,

    /// Parse function - parses value from string into destination.
    pub parse: Option<unsafe fn(&str, OxPtrMut) -> Option<Result<(), crate::ParseError>>>,

    /// Parse bytes function - parses value from byte slice into destination.
    /// Used for binary formats where types have a more efficient representation.
    pub parse_bytes: Option<unsafe fn(&[u8], OxPtrMut) -> Option<Result<(), crate::ParseError>>>,

    /// Try from function - converts from another value type.
    ///
    /// # Arguments
    /// - `dst`: Destination pointer where the converted value will be written
    /// - `src_shape`: Shape of the source type
    /// - `src_ptr`: Pointer to the source value
    ///
    /// # Return Value
    ///
    /// Returns [`TryFromOutcome`] which encodes both the result and ownership semantics:
    ///
    /// - [`TryFromOutcome::Converted`]: Success. Source was consumed.
    /// - [`TryFromOutcome::Unsupported`]: Source type not supported. Source was NOT consumed.
    /// - [`TryFromOutcome::Failed`]: Conversion failed. Source WAS consumed.
    ///
    /// See [`VTableDirect::try_from`] for implementation patterns.
    ///
    /// # Safety
    ///
    /// - `dst` must be valid for writes and properly aligned for the destination type
    /// - `src_ptr` must point to valid, initialized memory of the type described by `src_shape`
    pub try_from: Option<
        unsafe fn(OxPtrMut, &'static crate::Shape, crate::PtrConst) -> crate::TryFromOutcome,
    >,

    /// Try into inner function - extracts inner value (consuming).
    pub try_into_inner: Option<unsafe fn(OxPtrMut) -> Option<Result<PtrMut, String>>>,

    /// Try borrow inner function - borrows inner value.
    pub try_borrow_inner: Option<unsafe fn(OxPtrConst) -> Option<Result<PtrMut, String>>>,

    /// PartialEq function - tests equality with another value.
    pub partial_eq: Option<unsafe fn(OxPtrConst, OxPtrConst) -> Option<bool>>,

    /// PartialOrd function - compares with another value.
    pub partial_cmp: Option<unsafe fn(OxPtrConst, OxPtrConst) -> Option<Option<cmp::Ordering>>>,

    /// Ord function - total ordering comparison.
    pub cmp: Option<unsafe fn(OxPtrConst, OxPtrConst) -> Option<cmp::Ordering>>,
}

impl Default for VTableIndirect {
    fn default() -> Self {
        Self::empty()
    }
}

impl VTableIndirect {
    /// An empty VTableIndirect with all fields set to None.
    pub const EMPTY: Self = Self {
        display: None,
        debug: None,
        hash: None,
        invariants: None,
        parse: None,
        parse_bytes: None,
        try_from: None,
        try_into_inner: None,
        try_borrow_inner: None,
        partial_eq: None,
        partial_cmp: None,
        cmp: None,
    };

    /// Returns an empty VTableIndirect with all fields set to None.
    pub const fn empty() -> Self {
        Self::EMPTY
    }
}

//////////////////////////////////////////////////////////////////////
// Typed builder for VTableDirect
//////////////////////////////////////////////////////////////////////

/// Type-safe builder for VTableDirect.
///
/// Generic over `T` at the type level, ensuring all function pointers
/// are for the same type. The transmute to erased pointers happens
/// inside the builder methods.
pub struct TypedVTableDirectBuilder<T> {
    vtable: VTableDirect,
    _marker: PhantomData<T>,
}

impl VTableDirect {
    /// Create a typed builder for type T.
    ///
    /// The builder ensures all function pointers are for the same type T.
    pub const fn builder_for<T>() -> TypedVTableDirectBuilder<T> {
        TypedVTableDirectBuilder {
            vtable: VTableDirect::empty(),
            _marker: PhantomData,
        }
    }
}

impl<T> TypedVTableDirectBuilder<T> {
    /// Set the display function.
    pub const fn display(mut self, f: fn(&T, &mut fmt::Formatter<'_>) -> fmt::Result) -> Self {
        self.vtable.display = Some(unsafe {
            transmute::<
                fn(&T, &mut fmt::Formatter<'_>) -> fmt::Result,
                unsafe fn(*const (), &mut fmt::Formatter<'_>) -> fmt::Result,
            >(f)
        });
        self
    }

    /// Set the debug function.
    pub const fn debug(mut self, f: fn(&T, &mut fmt::Formatter<'_>) -> fmt::Result) -> Self {
        self.vtable.debug = Some(unsafe {
            transmute::<
                fn(&T, &mut fmt::Formatter<'_>) -> fmt::Result,
                unsafe fn(*const (), &mut fmt::Formatter<'_>) -> fmt::Result,
            >(f)
        });
        self
    }

    /// Set the hash function.
    ///
    /// Pass `<T as Hash>::hash::<HashProxy>` to use the type's Hash impl.
    pub const fn hash(mut self, f: fn(&T, &mut HashProxy<'static>)) -> Self {
        self.vtable.hash = Some(unsafe {
            transmute::<fn(&T, &mut HashProxy<'static>), unsafe fn(*const (), &mut HashProxy<'_>)>(
                f,
            )
        });
        self
    }

    /// Set the invariants function.
    pub const fn invariants(mut self, f: fn(&T) -> Result<(), String>) -> Self {
        self.vtable.invariants = Some(unsafe {
            transmute::<fn(&T) -> Result<(), String>, unsafe fn(*const ()) -> Result<(), String>>(f)
        });
        self
    }

    /// Set the parse function.
    pub const fn parse(
        mut self,
        f: unsafe fn(&str, *mut T) -> Result<(), crate::ParseError>,
    ) -> Self {
        self.vtable.parse = Some(unsafe {
            transmute::<
                unsafe fn(&str, *mut T) -> Result<(), crate::ParseError>,
                unsafe fn(&str, *mut ()) -> Result<(), crate::ParseError>,
            >(f)
        });
        self
    }

    /// Set the parse_bytes function.
    ///
    /// For types with efficient binary representations (e.g., UUID as 16 bytes).
    pub const fn parse_bytes(
        mut self,
        f: unsafe fn(&[u8], *mut T) -> Result<(), crate::ParseError>,
    ) -> Self {
        self.vtable.parse_bytes = Some(unsafe {
            transmute::<
                unsafe fn(&[u8], *mut T) -> Result<(), crate::ParseError>,
                unsafe fn(&[u8], *mut ()) -> Result<(), crate::ParseError>,
            >(f)
        });
        self
    }

    /// Set the try_from function.
    /// Arguments: (dst, src_shape, src_ptr)
    pub const fn try_from(
        mut self,
        f: unsafe fn(*mut T, &'static crate::Shape, crate::PtrConst) -> crate::TryFromOutcome,
    ) -> Self {
        self.vtable.try_from = Some(unsafe {
            transmute::<
                unsafe fn(*mut T, &'static crate::Shape, crate::PtrConst) -> crate::TryFromOutcome,
                unsafe fn(*mut (), &'static crate::Shape, crate::PtrConst) -> crate::TryFromOutcome,
            >(f)
        });
        self
    }

    /// Set the try_into_inner function.
    ///
    /// For transparent types, this extracts the inner value (consuming).
    /// Takes a pointer to the wrapper type, returns a pointer to the inner value.
    pub const fn try_into_inner(mut self, f: unsafe fn(*mut T) -> Result<PtrMut, String>) -> Self {
        self.vtable.try_into_inner = Some(unsafe {
            transmute::<
                unsafe fn(*mut T) -> Result<PtrMut, String>,
                unsafe fn(*mut ()) -> Result<PtrMut, String>,
            >(f)
        });
        self
    }

    /// Set the try_borrow_inner function.
    ///
    /// For transparent types, this borrows the inner value.
    /// Takes a pointer to the wrapper type, returns a pointer to the inner value.
    pub const fn try_borrow_inner(
        mut self,
        f: unsafe fn(*const T) -> Result<PtrMut, String>,
    ) -> Self {
        self.vtable.try_borrow_inner = Some(unsafe {
            transmute::<
                unsafe fn(*const T) -> Result<PtrMut, String>,
                unsafe fn(*const ()) -> Result<PtrMut, String>,
            >(f)
        });
        self
    }

    /// Set the partial_eq function.
    pub const fn partial_eq(mut self, f: fn(&T, &T) -> bool) -> Self {
        self.vtable.partial_eq = Some(unsafe {
            transmute::<fn(&T, &T) -> bool, unsafe fn(*const (), *const ()) -> bool>(f)
        });
        self
    }

    /// Set the partial_cmp function.
    pub const fn partial_cmp(mut self, f: fn(&T, &T) -> Option<cmp::Ordering>) -> Self {
        self.vtable.partial_cmp = Some(unsafe {
            transmute::<
                fn(&T, &T) -> Option<cmp::Ordering>,
                unsafe fn(*const (), *const ()) -> Option<cmp::Ordering>,
            >(f)
        });
        self
    }

    /// Set the cmp function.
    pub const fn cmp(mut self, f: fn(&T, &T) -> cmp::Ordering) -> Self {
        self.vtable.cmp = Some(unsafe {
            transmute::<fn(&T, &T) -> cmp::Ordering, unsafe fn(*const (), *const ()) -> cmp::Ordering>(
                f,
            )
        });
        self
    }

    /// Build the VTable.
    pub const fn build(self) -> VTableDirect {
        self.vtable
    }
}

// VTableIndirect uses struct literals directly - no builder needed

//////////////////////////////////////////////////////////////////////
// VTableErased
//////////////////////////////////////////////////////////////////////

/// Type-erased VTable that can hold either Direct or Indirect style.
///
/// | Variant | Use Case |
/// |---------|----------|
/// | Direct | Concrete types: scalars, String, derived types |
/// | Indirect | Generic containers: `Vec<T>`, `Option<T>`, `Arc<T>` |
#[derive(Clone, Copy)]
pub enum VTableErased {
    /// For concrete types with compile-time known traits.
    Direct(&'static VTableDirect),

    /// For generic containers with runtime trait resolution.
    Indirect(&'static VTableIndirect),
}

impl From<&'static VTableDirect> for VTableErased {
    fn from(vt: &'static VTableDirect) -> Self {
        VTableErased::Direct(vt)
    }
}

impl From<&'static VTableIndirect> for VTableErased {
    fn from(vt: &'static VTableIndirect) -> Self {
        VTableErased::Indirect(vt)
    }
}

impl fmt::Debug for VTableErased {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VTableErased::Direct(_) => f.debug_tuple("Direct").field(&"...").finish(),
            VTableErased::Indirect(_) => f.debug_tuple("Indirect").field(&"...").finish(),
        }
    }
}

impl VTableErased {
    /// Check if this vtable has a display function.
    #[inline]
    pub const fn has_display(&self) -> bool {
        match self {
            VTableErased::Direct(vt) => vt.display.is_some(),
            VTableErased::Indirect(vt) => vt.display.is_some(),
        }
    }

    /// Check if this vtable has a debug function.
    #[inline]
    pub const fn has_debug(&self) -> bool {
        match self {
            VTableErased::Direct(vt) => vt.debug.is_some(),
            VTableErased::Indirect(vt) => vt.debug.is_some(),
        }
    }

    /// Check if this vtable has a hash function.
    #[inline]
    pub const fn has_hash(&self) -> bool {
        match self {
            VTableErased::Direct(vt) => vt.hash.is_some(),
            VTableErased::Indirect(vt) => vt.hash.is_some(),
        }
    }

    /// Check if this vtable has a partial_eq function.
    #[inline]
    pub const fn has_partial_eq(&self) -> bool {
        match self {
            VTableErased::Direct(vt) => vt.partial_eq.is_some(),
            VTableErased::Indirect(vt) => vt.partial_eq.is_some(),
        }
    }

    /// Check if this vtable has a partial_cmp function.
    #[inline]
    pub const fn has_partial_ord(&self) -> bool {
        match self {
            VTableErased::Direct(vt) => vt.partial_cmp.is_some(),
            VTableErased::Indirect(vt) => vt.partial_cmp.is_some(),
        }
    }

    /// Check if this vtable has a cmp function.
    #[inline]
    pub const fn has_ord(&self) -> bool {
        match self {
            VTableErased::Direct(vt) => vt.cmp.is_some(),
            VTableErased::Indirect(vt) => vt.cmp.is_some(),
        }
    }

    /// Check if this vtable has a parse function.
    #[inline]
    pub const fn has_parse(&self) -> bool {
        match self {
            VTableErased::Direct(vt) => vt.parse.is_some(),
            VTableErased::Indirect(vt) => vt.parse.is_some(),
        }
    }

    /// Check if this vtable has a parse_bytes function.
    #[inline]
    pub const fn has_parse_bytes(&self) -> bool {
        match self {
            VTableErased::Direct(vt) => vt.parse_bytes.is_some(),
            VTableErased::Indirect(vt) => vt.parse_bytes.is_some(),
        }
    }

    /// Check if this vtable has a try_from function.
    #[inline]
    pub const fn has_try_from(&self) -> bool {
        match self {
            VTableErased::Direct(vt) => vt.try_from.is_some(),
            VTableErased::Indirect(vt) => vt.try_from.is_some(),
        }
    }

    /// Check if this vtable has a try_borrow_inner function.
    #[inline]
    pub const fn has_try_borrow_inner(&self) -> bool {
        match self {
            VTableErased::Direct(vt) => vt.try_borrow_inner.is_some(),
            VTableErased::Indirect(vt) => vt.try_borrow_inner.is_some(),
        }
    }

    /// Check if this vtable has an invariants function.
    #[inline]
    pub const fn has_invariants(&self) -> bool {
        match self {
            VTableErased::Direct(vt) => vt.invariants.is_some(),
            VTableErased::Indirect(vt) => vt.invariants.is_some(),
        }
    }
}

//////////////////////////////////////////////////////////////////////
// vtable_direct! macro
//////////////////////////////////////////////////////////////////////

/// Creates a VTableDirect for a type by specifying which traits it implements.
///
/// Note: `drop_in_place`, `default_in_place`, and `clone_into` are NOT set by this macro.
/// These per-type operations belong in [`TypeOps`] on the [`crate::Shape`], which allows
/// vtables to be shared across generic instantiations.
///
/// Standard traits (auto-generated from trait method references):
/// - `Display` -> `.display(<T as Display>::fmt)`
/// - `Debug` -> `.debug(<T as Debug>::fmt)`
/// - `Hash` -> `.hash(<T as Hash>::hash::<HashProxy>)`
/// - `PartialEq` -> `.partial_eq(<T as PartialEq>::eq)`
/// - `PartialOrd` -> `.partial_cmp(<T as PartialOrd>::partial_cmp)`
/// - `Ord` -> `.cmp(<T as Ord>::cmp)`
/// - `FromStr` -> `.parse(...)` (generates a parse function)
///
/// Custom functions (passed directly):
/// - `[parse = fn_name]`
/// - `[invariants = fn_name]`
/// - `[try_from = fn_name]`
/// - `[try_into_inner = fn_name]`
/// - `[try_borrow_inner = fn_name]`
///
/// # Example
///
/// ```ignore
/// const VTABLE: VTableDirect = vtable_direct!(char =>
///     [parse = parse],
///     Display,
///     Debug,
///     Hash,
///     PartialEq,
///     PartialOrd,
///     Ord,
/// );
/// ```
#[macro_export]
macro_rules! vtable_direct {
    // TT-muncher: start
    ($ty:ty => $($rest:tt)*) => {
        $crate::vtable_direct!(@build $ty, $crate::VTableDirect::builder_for::<$ty>(), $($rest)*)
    };

    // Base case: no more tokens - just build
    (@build $ty:ty, $builder:expr, $(,)?) => {
        $builder.build()
    };

    // Standard traits
    (@build $ty:ty, $builder:expr, Display $(, $($rest:tt)*)?) => {
        $crate::vtable_direct!(@build $ty, $builder.display(<$ty as core::fmt::Display>::fmt) $(, $($rest)*)?)
    };
    (@build $ty:ty, $builder:expr, Debug $(, $($rest:tt)*)?) => {
        $crate::vtable_direct!(@build $ty, $builder.debug(<$ty as core::fmt::Debug>::fmt) $(, $($rest)*)?)
    };
    (@build $ty:ty, $builder:expr, Hash $(, $($rest:tt)*)?) => {
        $crate::vtable_direct!(@build $ty, $builder.hash(<$ty as core::hash::Hash>::hash::<$crate::HashProxy>) $(, $($rest)*)?)
    };
    (@build $ty:ty, $builder:expr, PartialEq, $($rest:tt)*) => {
        $crate::vtable_direct!(@build $ty, $builder.partial_eq(<$ty as PartialEq>::eq), $($rest)*)
    };
    (@build $ty:ty, $builder:expr, PartialEq $(,)?) => {
        $crate::vtable_direct!(@build $ty, $builder.partial_eq(<$ty as PartialEq>::eq),)
    };
    (@build $ty:ty, $builder:expr, PartialOrd $(, $($rest:tt)*)?) => {
        $crate::vtable_direct!(@build $ty, $builder.partial_cmp(<$ty as PartialOrd>::partial_cmp) $(, $($rest)*)?)
    };
    (@build $ty:ty, $builder:expr, Ord $(, $($rest:tt)*)?) => {
        $crate::vtable_direct!(@build $ty, $builder.cmp(<$ty as Ord>::cmp) $(, $($rest)*)?)
    };

    // FromStr trait - generates parse function
    // Note: We use a static string for the error since parse error types
    // don't implement Facet. In the future, we could add Facet impls for them.
    (@build $ty:ty, $builder:expr, FromStr $(, $($rest:tt)*)?) => {
        $crate::vtable_direct!(@build $ty, $builder.parse({
            /// # Safety
            /// `dst` must be valid for writes and properly aligned
            unsafe fn parse(s: &str, dst: *mut $ty) -> Result<(), $crate::ParseError> {
                match s.parse::<$ty>() {
                    Ok(value) => {
                        unsafe { dst.write(value) };
                        Ok(())
                    }
                    Err(_) => Err($crate::ParseError::from_str(
                        const { concat!("failed to parse ", stringify!($ty)) }
                    )),
                }
            }
            parse
        }) $(, $($rest)*)?)
    };

    // Custom functions - use [name = expr] syntax
    (@build $ty:ty, $builder:expr, [parse = $f:expr] $(, $($rest:tt)*)?) => {
        $crate::vtable_direct!(@build $ty, $builder.parse($f) $(, $($rest)*)?)
    };
    (@build $ty:ty, $builder:expr, [invariants = $f:expr] $(, $($rest:tt)*)?) => {
        $crate::vtable_direct!(@build $ty, $builder.invariants($f) $(, $($rest)*)?)
    };
    (@build $ty:ty, $builder:expr, [try_from = $f:expr] $(, $($rest:tt)*)?) => {
        $crate::vtable_direct!(@build $ty, $builder.try_from($f) $(, $($rest)*)?)
    };
    (@build $ty:ty, $builder:expr, [try_into_inner = $f:expr] $(, $($rest:tt)*)?) => {
        $crate::vtable_direct!(@build $ty, $builder.try_into_inner($f) $(, $($rest)*)?)
    };
    (@build $ty:ty, $builder:expr, [try_borrow_inner = $f:expr] $(, $($rest:tt)*)?) => {
        $crate::vtable_direct!(@build $ty, $builder.try_borrow_inner($f) $(, $($rest)*)?)
    };
}

//////////////////////////////////////////////////////////////////////
// vtable_indirect! macro
//////////////////////////////////////////////////////////////////////

/// Creates a VTableIndirect for generic container types by specifying which traits it implements.
///
/// Note: `drop_in_place`, `default_in_place`, and `clone_into` are NOT set by this macro.
/// These per-type operations belong in [`TypeOps`] on the [`crate::Shape`], which allows
/// vtables to be shared across generic instantiations.
///
/// This macro generates wrapper functions that:
/// 1. Extract `&T` from `OxPtrConst` via `ox.get::<T>()`
/// 2. Call the trait method
/// 3. Wrap the result in `Some(...)`
///
/// ## Standard traits
///
/// - `Display` -> generates display fn calling `<T as Display>::fmt`
/// - `Debug` -> generates debug fn calling `<T as Debug>::fmt`
/// - `Hash` -> generates hash fn calling `<T as Hash>::hash`
/// - `PartialEq` -> generates partial_eq fn calling `<T as PartialEq>::eq`
/// - `PartialOrd` -> generates partial_cmp fn calling `<T as PartialOrd>::partial_cmp`
/// - `Ord` -> generates cmp fn calling `<T as Ord>::cmp`
///
/// ## Example
///
/// ```ignore
/// // Simple usage with standard traits
/// const VTABLE: VTableIndirect = vtable_indirect!(std::path::Path =>
///     Debug,
///     Hash,
///     PartialEq,
///     PartialOrd,
///     Ord,
/// );
/// ```
#[macro_export]
macro_rules! vtable_indirect {
    // Entry point - process traits one at a time
    ($ty:ty => $($traits:ident),* $(,)?) => {{
        $crate::VTableIndirect {
            display: $crate::vtable_indirect!(@display $ty; $($traits),*),
            debug: $crate::vtable_indirect!(@debug $ty; $($traits),*),
            hash: $crate::vtable_indirect!(@hash $ty; $($traits),*),
            invariants: None,
            parse: None,
            parse_bytes: None,
            try_from: None,
            try_into_inner: None,
            try_borrow_inner: None,
            partial_eq: $crate::vtable_indirect!(@partial_eq $ty; $($traits),*),
            partial_cmp: $crate::vtable_indirect!(@partial_cmp $ty; $($traits),*),
            cmp: $crate::vtable_indirect!(@cmp $ty; $($traits),*),
        }
    }};

    // Display - match or None
    (@display $ty:ty; Display $(, $($rest:ident),*)?) => {
        Some({
            unsafe fn display(ox: $crate::OxPtrConst, f: &mut core::fmt::Formatter<'_>) -> Option<core::fmt::Result> {
                let v: &$ty = unsafe { ox.ptr().get::<$ty>() };
                Some(<$ty as core::fmt::Display>::fmt(v, f))
            }
            display
        })
    };
    (@display $ty:ty; $other:ident $(, $($rest:ident),*)?) => {
        $crate::vtable_indirect!(@display $ty; $($($rest),*)?)
    };
    (@display $ty:ty;) => { None };

    // Debug - match or None
    (@debug $ty:ty; Debug $(, $($rest:ident),*)?) => {
        Some({
            unsafe fn debug(ox: $crate::OxPtrConst, f: &mut core::fmt::Formatter<'_>) -> Option<core::fmt::Result> {
                let v: &$ty = unsafe { ox.ptr().get::<$ty>() };
                Some(<$ty as core::fmt::Debug>::fmt(v, f))
            }
            debug
        })
    };
    (@debug $ty:ty; $other:ident $(, $($rest:ident),*)?) => {
        $crate::vtable_indirect!(@debug $ty; $($($rest),*)?)
    };
    (@debug $ty:ty;) => { None };

    // Hash - match or None
    (@hash $ty:ty; Hash $(, $($rest:ident),*)?) => {
        Some({
            unsafe fn hash(ox: $crate::OxPtrConst, hasher: &mut $crate::HashProxy<'_>) -> Option<()> {
                let v: &$ty = unsafe { ox.ptr().get::<$ty>() };
                <$ty as core::hash::Hash>::hash(v, hasher);
                Some(())
            }
            hash
        })
    };
    (@hash $ty:ty; $other:ident $(, $($rest:ident),*)?) => {
        $crate::vtable_indirect!(@hash $ty; $($($rest),*)?)
    };
    (@hash $ty:ty;) => { None };

    // PartialEq - match or None
    (@partial_eq $ty:ty; PartialEq $(, $($rest:ident),*)?) => {
        Some({
            unsafe fn partial_eq(a: $crate::OxPtrConst, b: $crate::OxPtrConst) -> Option<bool> {
                let a: &$ty = unsafe { a.ptr().get::<$ty>() };
                let b: &$ty = unsafe { b.ptr().get::<$ty>() };
                Some(<$ty as PartialEq>::eq(a, b))
            }
            partial_eq
        })
    };
    (@partial_eq $ty:ty; $other:ident $(, $($rest:ident),*)?) => {
        $crate::vtable_indirect!(@partial_eq $ty; $($($rest),*)?)
    };
    (@partial_eq $ty:ty;) => { None };

    // PartialOrd - match or None
    (@partial_cmp $ty:ty; PartialOrd $(, $($rest:ident),*)?) => {
        Some({
            unsafe fn partial_cmp(a: $crate::OxPtrConst, b: $crate::OxPtrConst) -> Option<Option<core::cmp::Ordering>> {
                let a: &$ty = unsafe { a.ptr().get::<$ty>() };
                let b: &$ty = unsafe { b.ptr().get::<$ty>() };
                Some(<$ty as PartialOrd>::partial_cmp(a, b))
            }
            partial_cmp
        })
    };
    (@partial_cmp $ty:ty; $other:ident $(, $($rest:ident),*)?) => {
        $crate::vtable_indirect!(@partial_cmp $ty; $($($rest),*)?)
    };
    (@partial_cmp $ty:ty;) => { None };

    // Ord - match or None
    (@cmp $ty:ty; Ord $(, $($rest:ident),*)?) => {
        Some({
            unsafe fn cmp(a: $crate::OxPtrConst, b: $crate::OxPtrConst) -> Option<core::cmp::Ordering> {
                let a: &$ty = unsafe { a.ptr().get::<$ty>() };
                let b: &$ty = unsafe { b.ptr().get::<$ty>() };
                Some(<$ty as Ord>::cmp(a, b))
            }
            cmp
        })
    };
    (@cmp $ty:ty; $other:ident $(, $($rest:ident),*)?) => {
        $crate::vtable_indirect!(@cmp $ty; $($($rest),*)?)
    };
    (@cmp $ty:ty;) => { None };
}

//////////////////////////////////////////////////////////////////////
// Type aliases for macro-generated attribute code
//////////////////////////////////////////////////////////////////////

/// Function type for default initialization in-place.
/// Used by the `#[facet(default)]` attribute.
pub type DefaultInPlaceFn = unsafe fn(target: crate::PtrUninit) -> crate::PtrMut;

/// Function type for type invariant validation.
/// Used by the `#[facet(invariants = fn)]` attribute.
pub type InvariantsFn = unsafe fn(value: crate::PtrConst) -> bool;

/// Function type for truthiness checks used by skip_unless_truthy-style helpers.
pub type TruthyFn = unsafe fn(value: crate::PtrConst) -> bool;

//////////////////////////////////////////////////////////////////////
// type_ops_direct! macro
//////////////////////////////////////////////////////////////////////

/// Creates a TypeOpsDirect for a type by specifying which traits it implements.
///
/// ## Supported traits
///
/// - `Default` -> generates default_in_place function
/// - `Clone` -> generates clone_into function
///
/// Note: `drop_in_place` is always generated automatically using `core::ptr::drop_in_place`.
///
/// ## Example
///
/// ```ignore
/// const TYPE_OPS: TypeOpsDirect = type_ops_direct!(u32 => Default, Clone);
/// ```
#[macro_export]
macro_rules! type_ops_direct {
    // Neither Default nor Clone
    ($ty:ty =>) => {{
        #[allow(clippy::useless_transmute)]
        $crate::TypeOpsDirect {
            drop_in_place: unsafe { core::mem::transmute::<unsafe fn(*mut $ty), unsafe fn(*mut ())>(core::ptr::drop_in_place::<$ty>) },
            default_in_place: None,
            clone_into: None,
            is_truthy: None,
        }
    }};

    // Default only
    ($ty:ty => Default $(,)?) => {{
        #[allow(clippy::useless_transmute)]
        $crate::TypeOpsDirect {
            drop_in_place: unsafe { core::mem::transmute::<unsafe fn(*mut $ty), unsafe fn(*mut ())>(core::ptr::drop_in_place::<$ty>) },
            default_in_place: Some(unsafe { core::mem::transmute::<unsafe fn(*mut $ty), unsafe fn(*mut ())>($crate::𝟋::𝟋default_for::<$ty>()) }),
            clone_into: None,
            is_truthy: None,
        }
    }};

    // Clone only
    ($ty:ty => Clone $(,)?) => {{
        #[allow(clippy::useless_transmute)]
        $crate::TypeOpsDirect {
            drop_in_place: unsafe { core::mem::transmute::<unsafe fn(*mut $ty), unsafe fn(*mut ())>(core::ptr::drop_in_place::<$ty>) },
            default_in_place: None,
            clone_into: Some(unsafe { core::mem::transmute::<unsafe fn(*const $ty, *mut $ty), unsafe fn(*const (), *mut ())>($crate::𝟋::𝟋clone_for::<$ty>()) }),
            is_truthy: None,
        }
    }};

    // Both Default and Clone (either order)
    ($ty:ty => Default, Clone $(,)?) => {{
        #[allow(clippy::useless_transmute)]
        $crate::TypeOpsDirect {
            drop_in_place: unsafe { core::mem::transmute::<unsafe fn(*mut $ty), unsafe fn(*mut ())>(core::ptr::drop_in_place::<$ty>) },
            default_in_place: Some(unsafe { core::mem::transmute::<unsafe fn(*mut $ty), unsafe fn(*mut ())>($crate::𝟋::𝟋default_for::<$ty>()) }),
            clone_into: Some(unsafe { core::mem::transmute::<unsafe fn(*const $ty, *mut $ty), unsafe fn(*const (), *mut ())>($crate::𝟋::𝟋clone_for::<$ty>()) }),
            is_truthy: None,
        }
    }};

    ($ty:ty => Clone, Default $(,)?) => {
        $crate::type_ops_direct!($ty => Default, Clone)
    };
}

//////////////////////////////////////////////////////////////////////
// TypeOps - Per-type operations that must be monomorphized
//////////////////////////////////////////////////////////////////////

/// Type-specific operations for concrete types (uses thin pointers).
///
/// Used for scalars, String, user-defined structs/enums, etc.
///
/// These operations must be monomorphized per-type because they need to know
/// the concrete type `T` at compile time:
/// - `drop_in_place`: Needs to call `T`'s destructor
/// - `default_in_place`: Needs to construct `T::default()`
/// - `clone_into`: Needs to call `T::clone()`
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct TypeOpsDirect {
    /// Drop the value in place.
    ///
    /// # Safety
    /// The pointer must point to a valid, initialized value of the correct type.
    pub drop_in_place: unsafe fn(*mut ()),

    /// Construct a default value in place.
    ///
    /// Returns `None` if the type doesn't implement `Default`.
    ///
    /// # Safety
    /// The pointer must point to uninitialized memory of sufficient size and alignment.
    pub default_in_place: Option<unsafe fn(*mut ())>,

    /// Clone a value into uninitialized memory.
    ///
    /// Returns `None` if the type doesn't implement `Clone`.
    ///
    /// # Safety
    /// - `src` must point to a valid, initialized value
    /// - `dst` must point to uninitialized memory of sufficient size and alignment
    pub clone_into: Option<unsafe fn(src: *const (), dst: *mut ())>,

    /// Truthiness predicate for this type. When absent, the type is never considered truthy.
    pub is_truthy: Option<TruthyFn>,
}

// TypeOpsDirect uses struct literals directly - no builder needed

/// Type-specific operations for generic containers (uses wide pointers with shape).
///
/// Used for `Vec<T>`, `Option<T>`, `Arc<T>`, etc.
///
/// These operations must be monomorphized per-type because they need to know
/// the concrete type `T` at compile time.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct TypeOpsIndirect {
    /// Drop the value in place.
    ///
    /// # Safety
    /// The pointer must point to a valid, initialized value of the correct type.
    pub drop_in_place: unsafe fn(OxPtrMut),

    /// Construct a default value in place.
    ///
    /// Returns `None` if the type doesn't implement `Default`.
    ///
    /// # Safety
    /// The pointer must point to uninitialized memory of sufficient size and alignment.
    pub default_in_place: Option<unsafe fn(OxPtrMut)>,

    /// Clone a value into uninitialized memory.
    ///
    /// Returns `None` if the type doesn't implement `Clone`.
    ///
    /// # Safety
    /// - `src` must point to a valid, initialized value
    /// - `dst` must point to uninitialized memory of sufficient size and alignment
    pub clone_into: Option<unsafe fn(src: OxPtrConst, dst: OxPtrMut)>,

    /// Truthiness predicate for this type. When absent, the type is never considered truthy.
    pub is_truthy: Option<TruthyFn>,
}

// TypeOpsIndirect uses struct literals directly - no builder needed

/// Type-erased TypeOps that can hold either Direct or Indirect style.
///
/// | Variant | Use Case |
/// |---------|----------|
/// | Direct | Concrete types: scalars, String, derived types |
/// | Indirect | Generic containers: `Vec<T>`, `Option<T>`, `Arc<T>` |
#[derive(Clone, Copy, Debug)]
pub enum TypeOps {
    /// For concrete types with thin pointers.
    Direct(&'static TypeOpsDirect),

    /// For generic containers with wide pointers (includes shape).
    Indirect(&'static TypeOpsIndirect),
}

impl From<&'static TypeOpsDirect> for TypeOps {
    fn from(ops: &'static TypeOpsDirect) -> Self {
        TypeOps::Direct(ops)
    }
}

impl From<&'static TypeOpsIndirect> for TypeOps {
    fn from(ops: &'static TypeOpsIndirect) -> Self {
        TypeOps::Indirect(ops)
    }
}

impl TypeOps {
    /// Check if this type has a clone_into operation.
    #[inline]
    pub const fn has_clone_into(&self) -> bool {
        match self {
            TypeOps::Direct(ops) => ops.clone_into.is_some(),
            TypeOps::Indirect(ops) => ops.clone_into.is_some(),
        }
    }

    /// Check if this type has a default_in_place operation.
    #[inline]
    pub const fn has_default_in_place(&self) -> bool {
        match self {
            TypeOps::Direct(ops) => ops.default_in_place.is_some(),
            TypeOps::Indirect(ops) => ops.default_in_place.is_some(),
        }
    }

    /// Returns the truthiness predicate for this type, if any.
    #[inline]
    pub const fn truthiness_fn(&self) -> Option<TruthyFn> {
        match self {
            TypeOps::Direct(ops) => ops.is_truthy,
            TypeOps::Indirect(ops) => ops.is_truthy,
        }
    }
}
