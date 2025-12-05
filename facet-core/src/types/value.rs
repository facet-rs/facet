use crate::{PtrConst, PtrMut, PtrUninit, TypedPtrConst, TypedPtrMut, TypedPtrUninit};
use core::{cmp::Ordering, marker::PhantomData, mem};

use crate::Shape;

use super::UnsizedError;

//======== Type Information ========

/// A function that formats the name of a type.
///
/// This helps avoid allocations, and it takes options.
pub type TypeNameFn = fn(f: &mut core::fmt::Formatter, opts: TypeNameOpts) -> core::fmt::Result;

/// Options for formatting the name of a type
#[derive(Clone, Copy)]
pub struct TypeNameOpts {
    /// as long as this is > 0, keep formatting the type parameters
    /// when it reaches 0, format type parameters as `...`
    /// if negative, all type parameters are formatted
    pub recurse_ttl: isize,
}

impl Default for TypeNameOpts {
    #[inline]
    fn default() -> Self {
        Self { recurse_ttl: -1 }
    }
}

impl TypeNameOpts {
    /// Create a new `NameOpts` for which none of the type parameters are formatted
    #[inline]
    pub fn none() -> Self {
        Self { recurse_ttl: 0 }
    }

    /// Create a new `NameOpts` for which only the direct children are formatted
    #[inline]
    pub fn one() -> Self {
        Self { recurse_ttl: 1 }
    }

    /// Create a new `NameOpts` for which all type parameters are formatted
    #[inline]
    pub fn infinite() -> Self {
        Self { recurse_ttl: -1 }
    }

    /// Decrease the `recurse_ttl` — if it's != 0, returns options to pass when
    /// formatting children type parameters.
    ///
    /// If this returns `None` and you have type parameters, you should render a
    /// `…` (unicode ellipsis) character instead of your list of types.
    ///
    /// See the implementation for `Vec` for examples.
    #[inline]
    pub fn for_children(&self) -> Option<Self> {
        match self.recurse_ttl.cmp(&0) {
            Ordering::Greater => Some(Self {
                recurse_ttl: self.recurse_ttl - 1,
            }),
            Ordering::Less => Some(Self {
                recurse_ttl: self.recurse_ttl,
            }),
            Ordering::Equal => None,
        }
    }
}

//======== Invariants ========

/// Function to validate the invariants of a value. If it returns false, the value is considered invalid.
///
/// # Safety
///
/// The `value` parameter must point to aligned, initialized memory of the correct type.
pub type InvariantsFn = for<'mem> unsafe fn(value: PtrConst<'mem>) -> bool;

/// Function to validate the invariants of a value. If it returns false, the value is considered invalid.
pub type InvariantsFnTyped<T> = fn(value: TypedPtrConst<'_, T>) -> bool;

//======== Memory Management ========

/// Function to drop a value
///
/// # Safety
///
/// The `value` parameter must point to aligned, initialized memory of the correct type.
/// After calling this function, the memory pointed to by `value` should not be accessed again
/// until it is properly reinitialized.
pub type DropInPlaceFn = for<'mem> unsafe fn(value: PtrMut<'mem>) -> PtrUninit<'mem>;

/// Function to clone a value into another already-allocated value
///
/// # Safety
///
/// The `source` parameter must point to aligned, initialized memory of the correct type.
/// The `target` parameter has the correct layout and alignment, but points to
/// uninitialized memory. The function returns the same pointer wrapped in an [`PtrMut`].
pub type CloneIntoFn =
    for<'src, 'dst> unsafe fn(source: PtrConst<'src>, target: PtrUninit<'dst>) -> PtrMut<'dst>;

/// Function to clone a value into another already-allocated value
pub type CloneIntoFnTyped<T> = for<'src, 'dst> fn(
    source: TypedPtrConst<'src, T>,
    target: TypedPtrUninit<'dst, T>,
) -> TypedPtrMut<'dst, T>;

/// Function to set a value to its default in-place
///
/// # Safety
///
/// The `target` parameter has the correct layout and alignment, but points to
/// uninitialized memory. The function returns the same pointer wrapped in an [`PtrMut`].
pub type DefaultInPlaceFn = for<'mem> unsafe fn(target: PtrUninit<'mem>) -> PtrMut<'mem>;
/// Function to set a value to its default in-place
pub type DefaultInPlaceFnTyped<T> =
    for<'mem> fn(target: TypedPtrUninit<'mem, T>) -> TypedPtrMut<'mem, T>;

//======== Conversion ========

/// Function to parse a value from a string.
///
/// If both [`DisplayFn`] and [`ParseFn`] are set, we should be able to round-trip the value.
///
/// # Safety
///
/// The `target` parameter has the correct layout and alignment, but points to
/// uninitialized memory. If this function succeeds, it should return `Ok` with the
/// same pointer wrapped in an [`PtrMut`]. If parsing fails, it returns `Err` with an error.
pub type ParseFn =
    for<'mem> unsafe fn(s: &str, target: PtrUninit<'mem>) -> Result<PtrMut<'mem>, ParseError>;

/// Function to parse a value from a string.
///
/// If both [`DisplayFn`] and [`ParseFn`] are set, we should be able to round-trip the value.
pub type ParseFnTyped<T> = for<'mem> fn(
    s: &str,
    target: TypedPtrUninit<'mem, T>,
) -> Result<TypedPtrMut<'mem, T>, ParseError>;

/// Error returned by [`ParseFn`]
#[derive(Debug)]
pub enum ParseError {
    /// Generic error message
    Generic(&'static str),
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ParseError::Generic(msg) => write!(f, "Parse failed: {msg}"),
        }
    }
}

impl core::error::Error for ParseError {}

/// Function to try converting from another type
///
/// # Safety
///
/// The `target` parameter has the correct layout and alignment, but points to
/// uninitialized memory. If this function succeeds, it should return `Ok` with the
/// same pointer wrapped in an [`PtrMut`]. If conversion fails, it returns `Err` with an error.
pub type TryFromFn = for<'src, 'mem, 'shape> unsafe fn(
    source: PtrConst<'src>,
    source_shape: &'static Shape,
    target: PtrUninit<'mem>,
) -> Result<PtrMut<'mem>, TryFromError>;

/// Function to try converting from another type
pub type TryFromFnTyped<T> = for<'src, 'mem, 'shape> fn(
    source: TypedPtrConst<'src, T>,
    source_shape: &'static Shape,
    target: TypedPtrUninit<'mem, T>,
) -> Result<&'mem mut T, TryFromError>;

/// Error type for TryFrom conversion failures
#[derive(Debug, PartialEq, Clone)]
pub enum TryFromError {
    /// Generic conversion error
    Generic(&'static str),

    /// The target shape doesn't implement conversion from any source shape (no try_from in vtable)
    Unimplemented,

    /// The target shape has a conversion implementation, but it doesn't support converting from this specific source shape
    UnsupportedSourceShape {
        /// The source shape that failed to convert
        src_shape: &'static Shape,

        /// The shapes that the `TryFrom` implementation supports
        expected: &'static [&'static Shape],
    },

    /// `!Sized` type
    Unsized,
}

impl core::fmt::Display for TryFromError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TryFromError::Generic(msg) => write!(f, "{msg}"),
            TryFromError::Unimplemented => write!(
                f,
                "Shape doesn't implement any conversions (no try_from function)",
            ),
            TryFromError::UnsupportedSourceShape {
                src_shape: source_shape,
                expected,
            } => {
                write!(f, "Incompatible types: {source_shape} (expected one of ")?;
                for (index, sh) in expected.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{sh}")?;
                }
                write!(f, ")")?;
                Ok(())
            }
            TryFromError::Unsized => write!(f, "Unsized type"),
        }
    }
}

impl core::error::Error for TryFromError {}

impl From<UnsizedError> for TryFromError {
    #[inline]
    fn from(_value: UnsizedError) -> Self {
        Self::Unsized
    }
}

/// Function to convert a transparent/newtype wrapper into its inner type.
///
/// This is used for types that wrap another type (like smart pointers, newtypes, etc.)
/// where the wrapper can be unwrapped to access the inner value. Primarily used during serialization.
///
/// # Safety
///
/// This function is unsafe because it operates on raw pointers.
///
/// The `src_ptr` must point to a valid, initialized instance of the wrapper type.
/// The `dst` pointer must point to valid, uninitialized memory suitable for holding an instance
/// of the inner type.
///
/// The function will return a pointer to the initialized inner value.
pub type TryIntoInnerFn = for<'src, 'dst> unsafe fn(
    src_ptr: PtrMut<'src>,
    dst: PtrUninit<'dst>,
) -> Result<PtrMut<'dst>, TryIntoInnerError>;
/// Function to convert a transparent/newtype wrapper into its inner type.
///
/// This is used for types that wrap another type (like smart pointers, newtypes, etc.)
/// where the wrapper can be unwrapped to access the inner value. Primarily used during serialization.
pub type TryIntoInnerFnTyped<T> = for<'src, 'dst> fn(
    src_ptr: TypedPtrConst<'src, T>,
    dst: TypedPtrUninit<'dst, T>,
) -> Result<&'dst mut T, TryIntoInnerError>;

/// Error type returned by [`TryIntoInnerFn`] when attempting to extract
/// the inner value from a wrapper type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TryIntoInnerError {
    /// Indicates that the inner value cannot be extracted at this time,
    /// such as when a mutable borrow is already active.
    Unavailable,
    /// Indicates that another unspecified error occurred during extraction.
    Other(&'static str),
}

impl core::fmt::Display for TryIntoInnerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TryIntoInnerError::Unavailable => {
                write!(f, "inner value is unavailable for extraction")
            }
            TryIntoInnerError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl core::error::Error for TryIntoInnerError {}

/// Function to borrow the inner value from a transparent/newtype wrapper without copying.
///
/// This is used for types that wrap another type (like smart pointers, newtypes, etc.)
/// to efficiently access the inner value without transferring ownership.
///
/// # Safety
///
/// This function is unsafe because it operates on raw pointers.
///
/// The `src_ptr` must point to a valid, initialized instance of the wrapper type.
/// The returned pointer points to memory owned by the wrapper and remains valid
/// as long as the wrapper is valid and not mutated.
pub type TryBorrowInnerFn =
    for<'src> unsafe fn(src_ptr: PtrConst<'src>) -> Result<PtrConst<'src>, TryBorrowInnerError>;

/// Function to borrow the inner value from a transparent/newtype wrapper without copying.
///
/// This is used for types that wrap another type (like smart pointers, newtypes, etc.)
/// to efficiently access the inner value without transferring ownership.
pub type TryBorrowInnerFnTyped<T> =
    for<'src> fn(src_ptr: TypedPtrConst<'src, T>) -> Result<PtrConst<'src>, TryBorrowInnerError>;

/// Error type returned by [`TryBorrowInnerFn`] when attempting to borrow
/// the inner value from a wrapper type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TryBorrowInnerError {
    /// Indicates that the inner value cannot be borrowed at this time,
    /// such as when a mutable borrow is already active.
    Unavailable,
    /// Indicates an other, unspecified error occurred during the borrow attempt.
    /// The contained string provides a description of the error.
    Other(&'static str),
}

impl core::fmt::Display for TryBorrowInnerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TryBorrowInnerError::Unavailable => {
                write!(f, "inner value is unavailable for borrowing")
            }
            TryBorrowInnerError::Other(msg) => {
                write!(f, "{msg}")
            }
        }
    }
}

impl core::error::Error for TryBorrowInnerError {}

//======== Comparison ========

/// Function to check if two values are partially equal
///
/// # Safety
///
/// Both `left` and `right` parameters must point to aligned, initialized memory of the correct type.
pub type PartialEqFn = for<'l, 'r> unsafe fn(left: PtrConst<'l>, right: PtrConst<'r>) -> bool;
/// Function to check if two values are partially equal
pub type PartialEqFnTyped<T> = fn(left: TypedPtrConst<'_, T>, right: TypedPtrConst<'_, T>) -> bool;

/// Function to compare two values and return their ordering if comparable
///
/// # Safety
///
/// Both `left` and `right` parameters must point to aligned, initialized memory of the correct type.
pub type PartialOrdFn =
    for<'l, 'r> unsafe fn(left: PtrConst<'l>, right: PtrConst<'r>) -> Option<Ordering>;
/// Function to compare two values and return their ordering if comparable
pub type PartialOrdFnTyped<T> =
    fn(left: TypedPtrConst<'_, T>, right: TypedPtrConst<'_, T>) -> Option<Ordering>;

/// Function to compare two values and return their ordering
///
/// # Safety
///
/// Both `left` and `right` parameters must point to aligned, initialized memory of the correct type.
pub type CmpFn = for<'l, 'r> unsafe fn(left: PtrConst<'l>, right: PtrConst<'r>) -> Ordering;
/// Function to compare two values and return their ordering
pub type CmpFnTyped<T> = fn(left: TypedPtrConst<'_, T>, right: TypedPtrConst<'_, T>) -> Ordering;

//======== Hashing ========

/// Function to hash a value
///
/// # Safety
///
/// The `value` parameter must point to aligned, initialized memory of the correct type.
/// The hasher pointer must be a valid pointer to a Hasher trait object.
pub type HashFn =
    for<'mem> unsafe fn(value: PtrConst<'mem>, hasher_this: &mut dyn core::hash::Hasher);

/// Function to hash a value
pub type HashFnTyped<T> =
    for<'mem> fn(value: TypedPtrConst<'mem, T>, hasher_this: &mut dyn core::hash::Hasher);

/// Function to write bytes to a hasher
///
/// # Safety
///
/// The `hasher_self` parameter must be a valid pointer to a hasher
pub type HasherWriteFn = for<'mem> unsafe fn(hasher_self: PtrMut<'mem>, bytes: &[u8]);
/// Function to write bytes to a hasher
pub type HasherWriteFnTyped<T> = for<'mem> fn(hasher_self: TypedPtrMut<'mem, T>, bytes: &[u8]);

/// Provides an implementation of [`core::hash::Hasher`] for a given hasher pointer and write function
///
/// See [`HashFn`] for more details on the parameters.
pub struct HasherProxy<'a> {
    hasher_this: PtrMut<'a>,
    hasher_write_fn: HasherWriteFn,
}

impl<'a> HasherProxy<'a> {
    /// Create a new `HasherProxy` from a hasher pointer and a write function
    ///
    /// # Safety
    ///
    /// The `hasher_this` parameter must be a valid pointer to a Hasher trait object.
    /// The `hasher_write_fn` parameter must be a valid function pointer.
    #[inline]
    pub unsafe fn new(hasher_this: PtrMut<'a>, hasher_write_fn: HasherWriteFn) -> Self {
        Self {
            hasher_this,
            hasher_write_fn,
        }
    }
}

impl core::hash::Hasher for HasherProxy<'_> {
    fn finish(&self) -> u64 {
        unimplemented!("finish is not needed for this implementation")
    }
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        unsafe { (self.hasher_write_fn)(self.hasher_this, bytes) }
    }
}

//======== Display and Debug ========

/// Function to format a value for display
///
/// If both [`DisplayFn`] and [`ParseFn`] are set, we should be able to round-trip the value.
///
/// # Safety
///
/// The `value` parameter must point to aligned, initialized memory of the correct type.
pub type DisplayFn =
    for<'mem> unsafe fn(value: PtrConst<'mem>, f: &mut core::fmt::Formatter) -> core::fmt::Result;

/// Function to format a value for display
///
/// If both [`DisplayFn`] and [`ParseFn`] are set, we should be able to round-trip the value.
pub type DisplayFnTyped<T> =
    fn(value: TypedPtrConst<'_, T>, f: &mut core::fmt::Formatter) -> core::fmt::Result;

/// Function to format a value for debug.
/// If this returns None, the shape did not implement Debug.
pub type DebugFn =
    for<'mem> unsafe fn(value: PtrConst<'mem>, f: &mut core::fmt::Formatter) -> core::fmt::Result;

//======== Grouped VTable Sub-structs ========

/// VTable for formatting traits (Display, Debug)
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct FormatVTable {
    /// cf. [`DisplayFn`]
    pub display: Option<DisplayFn>,
    /// cf. [`DebugFn`]
    pub debug: Option<DebugFn>,
}

impl FormatVTable {
    /// Create a new FormatVTable with all fields set to None
    pub const EMPTY: Self = Self {
        display: None,
        debug: None,
    };

    /// Set the display function
    #[inline]
    pub const fn with_display(mut self, f: DisplayFn) -> Self {
        self.display = Some(f);
        self
    }

    /// Set the debug function
    #[inline]
    pub const fn with_debug(mut self, f: DebugFn) -> Self {
        self.debug = Some(f);
        self
    }
}

impl Default for FormatVTable {
    fn default() -> Self {
        Self::EMPTY
    }
}

/// VTable for comparison traits (PartialEq, PartialOrd, Ord)
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct CmpVTable {
    /// cf. [`PartialEqFn`]
    pub partial_eq: Option<PartialEqFn>,
    /// cf. [`PartialOrdFn`]
    pub partial_ord: Option<PartialOrdFn>,
    /// cf. [`CmpFn`]
    pub ord: Option<CmpFn>,
}

impl CmpVTable {
    /// Create a new CmpVTable with all fields set to None
    pub const EMPTY: Self = Self {
        partial_eq: None,
        partial_ord: None,
        ord: None,
    };

    /// Set the partial_eq function
    #[inline]
    pub const fn with_partial_eq(mut self, f: PartialEqFn) -> Self {
        self.partial_eq = Some(f);
        self
    }

    /// Set the partial_ord function
    #[inline]
    pub const fn with_partial_ord(mut self, f: PartialOrdFn) -> Self {
        self.partial_ord = Some(f);
        self
    }

    /// Set the ord function
    #[inline]
    pub const fn with_ord(mut self, f: CmpFn) -> Self {
        self.ord = Some(f);
        self
    }
}

impl Default for CmpVTable {
    fn default() -> Self {
        Self::EMPTY
    }
}

/// VTable for Hash trait
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct HashVTable {
    /// cf. [`HashFn`]
    pub hash: Option<HashFn>,
}

impl HashVTable {
    /// Create a new HashVTable with all fields set to None
    pub const EMPTY: Self = Self { hash: None };

    /// Set the hash function
    #[inline]
    pub const fn with_hash(mut self, f: HashFn) -> Self {
        self.hash = Some(f);
        self
    }
}

impl Default for HashVTable {
    fn default() -> Self {
        Self::EMPTY
    }
}

/// Bitflags for marker traits
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(transparent)]
pub struct MarkerTraits(u8);

impl MarkerTraits {
    /// No marker traits
    pub const EMPTY: Self = Self(0);

    /// Type implements Copy
    pub const COPY: Self = Self(0b0000_0001);
    /// Type implements Send
    pub const SEND: Self = Self(0b0000_0010);
    /// Type implements Sync
    pub const SYNC: Self = Self(0b0000_0100);
    /// Type implements Eq (not just PartialEq)
    pub const EQ: Self = Self(0b0000_1000);
    /// Type implements Unpin
    pub const UNPIN: Self = Self(0b0001_0000);
    /// Type implements UnwindSafe
    pub const UNWIND_SAFE: Self = Self(0b0010_0000);
    /// Type implements RefUnwindSafe
    pub const REF_UNWIND_SAFE: Self = Self(0b0100_0000);

    /// Create marker traits from raw bits
    #[inline]
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    /// Get the raw bits
    #[inline]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Check if a marker trait is set
    #[inline]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Set a marker trait
    #[inline]
    pub const fn insert(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Check if Copy is implemented
    #[inline]
    pub const fn is_copy(self) -> bool {
        self.contains(Self::COPY)
    }

    /// Check if Send is implemented
    #[inline]
    pub const fn is_send(self) -> bool {
        self.contains(Self::SEND)
    }

    /// Check if Sync is implemented
    #[inline]
    pub const fn is_sync(self) -> bool {
        self.contains(Self::SYNC)
    }

    /// Check if Eq is implemented
    #[inline]
    pub const fn is_eq(self) -> bool {
        self.contains(Self::EQ)
    }

    /// Check if Unpin is implemented
    #[inline]
    pub const fn is_unpin(self) -> bool {
        self.contains(Self::UNPIN)
    }

    /// Check if UnwindSafe is implemented
    #[inline]
    pub const fn is_unwind_safe(self) -> bool {
        self.contains(Self::UNWIND_SAFE)
    }

    /// Check if RefUnwindSafe is implemented
    #[inline]
    pub const fn is_ref_unwind_safe(self) -> bool {
        self.contains(Self::REF_UNWIND_SAFE)
    }

    /// Add Copy marker
    #[inline]
    pub const fn with_copy(self) -> Self {
        self.insert(Self::COPY)
    }

    /// Add Send marker
    #[inline]
    pub const fn with_send(self) -> Self {
        self.insert(Self::SEND)
    }

    /// Add Sync marker
    #[inline]
    pub const fn with_sync(self) -> Self {
        self.insert(Self::SYNC)
    }

    /// Add Eq marker
    #[inline]
    pub const fn with_eq(self) -> Self {
        self.insert(Self::EQ)
    }

    /// Add Unpin marker
    #[inline]
    pub const fn with_unpin(self) -> Self {
        self.insert(Self::UNPIN)
    }

    /// Add UnwindSafe marker
    #[inline]
    pub const fn with_unwind_safe(self) -> Self {
        self.insert(Self::UNWIND_SAFE)
    }

    /// Add RefUnwindSafe marker
    #[inline]
    pub const fn with_ref_unwind_safe(self) -> Self {
        self.insert(Self::REF_UNWIND_SAFE)
    }
}

/// Function to format a value for debug.
/// If this returns None, the shape did not implement Debug.
pub type DebugFnTyped<T> =
    fn(value: TypedPtrConst<'_, T>, f: &mut core::fmt::Formatter) -> core::fmt::Result;

/// A vtable representing the operations that can be performed on a type,
/// either for sized or unsized types.
///
/// This struct encapsulates the specific vtables allowing generic type-agnostic
/// dynamic dispatch for core capabilities (clone, drop, compare, hash, etc).
///
/// Trait-related fields are grouped into sub-structs:
/// - [`FormatVTable`] for Display and Debug
/// - [`CmpVTable`] for PartialEq, PartialOrd, and Ord
/// - [`HashVTable`] for Hash
/// - [`MarkerTraits`] for Copy, Send, Sync, Eq, Unpin, UnwindSafe, RefUnwindSafe
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ValueVTable {
    /// cf. [`TypeNameFn`]
    pub type_name: TypeNameFn,

    /// cf. [`DropInPlaceFn`] — if None, drops without side-effects
    pub drop_in_place: Option<DropInPlaceFn>,

    /// cf. [`InvariantsFn`]
    pub invariants: Option<InvariantsFn>,

    /// cf. [`DefaultInPlaceFn`]
    pub default_in_place: Option<DefaultInPlaceFn>,

    /// cf. [`CloneIntoFn`]
    pub clone_into: Option<CloneIntoFn>,

    /// cf. [`ParseFn`]
    pub parse: Option<ParseFn>,

    /// cf. [`TryFromFn`]
    ///
    /// This also acts as a "TryFromInner" — you can use it to go:
    ///
    ///   * `String` => `Utf8PathBuf`
    ///   * `String` => `Uuid`
    ///   * `T` => `Option<T>`
    ///   * `T` => `Arc<T>`
    ///   * `T` => `NonZero<T>`
    ///   * etc.
    ///
    pub try_from: Option<TryFromFn>,

    /// cf. [`TryIntoInnerFn`]
    ///
    /// This is used by transparent types to convert the wrapper type into its inner value.
    /// Primarily used during serialization.
    pub try_into_inner: Option<TryIntoInnerFn>,

    /// cf. [`TryBorrowInnerFn`]
    ///
    /// This is used by transparent types to efficiently access the inner value without copying.
    pub try_borrow_inner: Option<TryBorrowInnerFn>,

    /// Formatting traits (Display, Debug)
    pub format: FormatVTable,

    /// Comparison traits (PartialEq, PartialOrd, Ord)
    pub cmp: CmpVTable,

    /// Hash trait
    pub hash: HashVTable,

    /// Marker traits (Copy, Send, Sync, Eq, Unpin, UnwindSafe, RefUnwindSafe)
    pub markers: MarkerTraits,
}

impl ValueVTable {
    /// Get the type name fn of the type
    #[inline]
    pub const fn type_name(&self) -> TypeNameFn {
        self.type_name
    }

    /// Returns `true` if the type implements the [`Display`](core::fmt::Display) trait and the `display` function is available in the vtable.
    #[inline]
    pub const fn has_display(&self) -> bool {
        self.format.display.is_some()
    }

    /// Returns `true` if the type implements the [`Debug`] trait and the `debug` function is available in the vtable.
    #[inline]
    pub const fn has_debug(&self) -> bool {
        self.format.debug.is_some()
    }

    /// Returns `true` if the type implements the [`PartialEq`] trait and the `partial_eq` function is available in the vtable.
    #[inline]
    pub const fn has_partial_eq(&self) -> bool {
        self.cmp.partial_eq.is_some()
    }

    /// Returns `true` if the type implements the [`PartialOrd`] trait and the `partial_ord` function is available in the vtable.
    #[inline]
    pub const fn has_partial_ord(&self) -> bool {
        self.cmp.partial_ord.is_some()
    }

    /// Returns `true` if the type implements the [`Ord`] trait and the `ord` function is available in the vtable.
    #[inline]
    pub const fn has_ord(&self) -> bool {
        self.cmp.ord.is_some()
    }

    /// Returns `true` if the type implements the [`Hash`] trait and the `hash` function is available in the vtable.
    #[inline]
    pub const fn has_hash(&self) -> bool {
        self.hash.hash.is_some()
    }

    /// Returns `true` if the type supports default-in-place construction via the vtable.
    #[inline]
    pub const fn has_default_in_place(&self) -> bool {
        self.default_in_place.is_some()
    }

    /// Returns `true` if the type supports in-place cloning via the vtable.
    #[inline]
    pub const fn has_clone_into(&self) -> bool {
        self.clone_into.is_some()
    }

    /// Returns `true` if the type supports parsing from a string via the vtable.
    #[inline]
    pub const fn has_parse(&self) -> bool {
        self.parse.is_some()
    }

    /// Returns a `ValueVTable` with all fields set to `None`/empty except `type_name`.
    /// Use struct literal syntax with `..ValueVTable::new(type_name_fn)` for the rest.
    pub const fn new(type_name: TypeNameFn) -> Self {
        Self {
            type_name,
            drop_in_place: None,
            invariants: None,
            default_in_place: None,
            clone_into: None,
            parse: None,
            try_from: None,
            try_into_inner: None,
            try_borrow_inner: None,
            format: FormatVTable::EMPTY,
            cmp: CmpVTable::EMPTY,
            hash: HashVTable::EMPTY,
            markers: MarkerTraits::EMPTY,
        }
    }

    /// Returns the appropriate `drop_in_place` function for type `T`.
    /// Returns `None` if the type doesn't need dropping.
    pub const fn drop_in_place_for<T: ?Sized>() -> Option<DropInPlaceFn> {
        if mem::needs_drop::<T>() {
            Some(|value| unsafe { value.drop_in_place::<T>() })
        } else {
            None
        }
    }

    /// Create a builder for constructing a `ValueVTable`.
    ///
    /// This builder pattern allows constructing vtables in a way that's
    /// compatible with feature-gated fields. Methods for feature-gated
    /// fields become no-ops when their feature is disabled.
    #[inline]
    pub const fn builder(type_name: TypeNameFn) -> ValueVTableBuilder {
        ValueVTableBuilder::new(type_name)
    }
}

/// Builder for [`ValueVTable`].
///
/// This builder allows constructing vtables incrementally.
#[derive(Clone, Copy)]
pub struct ValueVTableBuilder {
    type_name: TypeNameFn,
    drop_in_place: Option<DropInPlaceFn>,
    invariants: Option<InvariantsFn>,
    default_in_place: Option<DefaultInPlaceFn>,
    clone_into: Option<CloneIntoFn>,
    parse: Option<ParseFn>,
    try_from: Option<TryFromFn>,
    try_into_inner: Option<TryIntoInnerFn>,
    try_borrow_inner: Option<TryBorrowInnerFn>,
    format: FormatVTable,
    cmp: CmpVTable,
    hash: HashVTable,
    markers: MarkerTraits,
}

impl ValueVTableBuilder {
    /// Create a new builder with the given type name function.
    #[inline]
    pub const fn new(type_name: TypeNameFn) -> Self {
        Self {
            type_name,
            drop_in_place: None,
            invariants: None,
            default_in_place: None,
            clone_into: None,
            parse: None,
            try_from: None,
            try_into_inner: None,
            try_borrow_inner: None,
            format: FormatVTable::EMPTY,
            cmp: CmpVTable::EMPTY,
            hash: HashVTable::EMPTY,
            markers: MarkerTraits::EMPTY,
        }
    }

    /// Set the drop_in_place function (takes Option because drop_in_place_for returns Option).
    #[inline]
    pub const fn drop_in_place(mut self, f: Option<DropInPlaceFn>) -> Self {
        self.drop_in_place = f;
        self
    }

    /// Set the invariants function.
    #[inline]
    pub const fn invariants(mut self, f: InvariantsFn) -> Self {
        self.invariants = Some(f);
        self
    }

    /// Conditionally set the invariants function.
    #[inline]
    pub const fn invariants_opt(mut self, f: Option<InvariantsFn>) -> Self {
        self.invariants = f;
        self
    }

    /// Set the display function.
    #[inline]
    pub const fn display(mut self, f: DisplayFn) -> Self {
        self.format.display = Some(f);
        self
    }

    /// Conditionally set the display function.
    #[inline]
    pub const fn display_opt(mut self, f: Option<DisplayFn>) -> Self {
        self.format.display = f;
        self
    }

    /// Set the debug function.
    #[inline]
    pub const fn debug(mut self, f: DebugFn) -> Self {
        self.format.debug = Some(f);
        self
    }

    /// Conditionally set the debug function.
    #[inline]
    pub const fn debug_opt(mut self, f: Option<DebugFn>) -> Self {
        self.format.debug = f;
        self
    }

    /// Set the entire format vtable.
    #[inline]
    pub const fn format(mut self, f: FormatVTable) -> Self {
        self.format = f;
        self
    }

    /// Set the default_in_place function.
    #[inline]
    pub const fn default_in_place(mut self, f: DefaultInPlaceFn) -> Self {
        self.default_in_place = Some(f);
        self
    }

    /// Conditionally set the default_in_place function.
    #[inline]
    pub const fn default_in_place_opt(mut self, f: Option<DefaultInPlaceFn>) -> Self {
        self.default_in_place = f;
        self
    }

    /// Set the clone_into function.
    #[inline]
    pub const fn clone_into(mut self, f: CloneIntoFn) -> Self {
        self.clone_into = Some(f);
        self
    }

    /// Conditionally set the clone_into function.
    #[inline]
    pub const fn clone_into_opt(mut self, f: Option<CloneIntoFn>) -> Self {
        self.clone_into = f;
        self
    }

    /// Set the partial_eq function.
    #[inline]
    pub const fn partial_eq(mut self, f: PartialEqFn) -> Self {
        self.cmp.partial_eq = Some(f);
        self
    }

    /// Conditionally set the partial_eq function.
    #[inline]
    pub const fn partial_eq_opt(mut self, f: Option<PartialEqFn>) -> Self {
        self.cmp.partial_eq = f;
        self
    }

    /// Set the partial_ord function.
    #[inline]
    pub const fn partial_ord(mut self, f: PartialOrdFn) -> Self {
        self.cmp.partial_ord = Some(f);
        self
    }

    /// Conditionally set the partial_ord function.
    #[inline]
    pub const fn partial_ord_opt(mut self, f: Option<PartialOrdFn>) -> Self {
        self.cmp.partial_ord = f;
        self
    }

    /// Set the ord function.
    #[inline]
    pub const fn ord(mut self, f: CmpFn) -> Self {
        self.cmp.ord = Some(f);
        self
    }

    /// Conditionally set the ord function.
    #[inline]
    pub const fn ord_opt(mut self, f: Option<CmpFn>) -> Self {
        self.cmp.ord = f;
        self
    }

    /// Set the entire comparison vtable.
    #[inline]
    pub const fn cmp(mut self, c: CmpVTable) -> Self {
        self.cmp = c;
        self
    }

    /// Set the hash function.
    #[inline]
    pub const fn hash(mut self, f: HashFn) -> Self {
        self.hash.hash = Some(f);
        self
    }

    /// Conditionally set the hash function.
    #[inline]
    pub const fn hash_opt(mut self, f: Option<HashFn>) -> Self {
        self.hash.hash = f;
        self
    }

    /// Set the entire hash vtable.
    #[inline]
    pub const fn hash_vtable(mut self, h: HashVTable) -> Self {
        self.hash = h;
        self
    }

    /// Set the marker traits.
    #[inline]
    pub const fn markers(mut self, m: MarkerTraits) -> Self {
        self.markers = m;
        self
    }

    /// Set the parse function.
    #[inline]
    pub const fn parse(mut self, f: ParseFn) -> Self {
        self.parse = Some(f);
        self
    }

    /// Conditionally set the parse function.
    #[inline]
    pub const fn parse_opt(mut self, f: Option<ParseFn>) -> Self {
        self.parse = f;
        self
    }

    /// Set the try_from function.
    #[inline]
    pub const fn try_from(mut self, f: TryFromFn) -> Self {
        self.try_from = Some(f);
        self
    }

    /// Conditionally set the try_from function.
    #[inline]
    pub const fn try_from_opt(mut self, f: Option<TryFromFn>) -> Self {
        self.try_from = f;
        self
    }

    /// Set the try_into_inner function.
    #[inline]
    pub const fn try_into_inner(mut self, f: TryIntoInnerFn) -> Self {
        self.try_into_inner = Some(f);
        self
    }

    /// Conditionally set the try_into_inner function.
    #[inline]
    pub const fn try_into_inner_opt(mut self, f: Option<TryIntoInnerFn>) -> Self {
        self.try_into_inner = f;
        self
    }

    /// Set the try_borrow_inner function.
    #[inline]
    pub const fn try_borrow_inner(mut self, f: TryBorrowInnerFn) -> Self {
        self.try_borrow_inner = Some(f);
        self
    }

    /// Conditionally set the try_borrow_inner function.
    #[inline]
    pub const fn try_borrow_inner_opt(mut self, f: Option<TryBorrowInnerFn>) -> Self {
        self.try_borrow_inner = f;
        self
    }

    /// Build the `ValueVTable`.
    #[inline]
    pub const fn build(self) -> ValueVTable {
        ValueVTable {
            type_name: self.type_name,
            drop_in_place: self.drop_in_place,
            invariants: self.invariants,
            default_in_place: self.default_in_place,
            clone_into: self.clone_into,
            parse: self.parse,
            try_from: self.try_from,
            try_into_inner: self.try_into_inner,
            try_borrow_inner: self.try_borrow_inner,
            format: self.format,
            cmp: self.cmp,
            hash: self.hash,
            markers: self.markers,
        }
    }
}

/// A typed view of a [`ValueVTable`].
#[derive(Debug)]
pub struct VTableView<T: ?Sized>(&'static ValueVTable, PhantomData<T>);

impl<'a, T: crate::Facet<'a> + ?Sized> VTableView<&'a mut T> {
    /// Fetches the vtable for the type.
    pub fn of_deref() -> Self {
        Self(const { &T::SHAPE.vtable }, PhantomData)
    }
}

impl<'a, T: crate::Facet<'a> + ?Sized> VTableView<&'a T> {
    /// Fetches the vtable for the type.
    pub fn of_deref() -> Self {
        Self(const { &T::SHAPE.vtable }, PhantomData)
    }
}

impl<'a, T: crate::Facet<'a> + ?Sized> VTableView<T> {
    /// Fetches the vtable for the type.
    pub const fn of() -> Self {
        let this = Self(const { &T::SHAPE.vtable }, PhantomData);

        if const { core::mem::size_of::<*const T>() == core::mem::size_of::<*const ()>() } {
            assert!(T::SHAPE.layout.sized_layout().is_ok());
            assert!(core::mem::size_of::<*const T>() == core::mem::size_of::<*const ()>());
        } else {
            assert!(T::SHAPE.layout.sized_layout().is_err());
            assert!(core::mem::size_of::<*const T>() == 2 * core::mem::size_of::<*const ()>());
        }

        this
    }

    /// cf. [`TypeNameFn`]
    #[inline(always)]
    pub const fn type_name(&self) -> TypeNameFn {
        self.0.type_name()
    }

    /// cf. [`InvariantsFn`]
    #[inline(always)]
    pub fn invariants(&self) -> Option<InvariantsFnTyped<T>> {
        self.0.invariants.map(|f| unsafe { mem::transmute(f) })
    }

    /// cf. [`DisplayFn`]
    #[inline(always)]
    pub fn display(&self) -> Option<DisplayFnTyped<T>> {
        self.0.format.display.map(|f| unsafe { mem::transmute(f) })
    }

    /// cf. [`DebugFn`]
    #[inline(always)]
    pub fn debug(&self) -> Option<DebugFnTyped<T>> {
        self.0.format.debug.map(|f| unsafe { mem::transmute(f) })
    }

    /// cf. [`PartialEqFn`] for equality comparison
    #[inline(always)]
    pub fn partial_eq(&self) -> Option<PartialEqFnTyped<T>> {
        self.0.cmp.partial_eq.map(|f| unsafe { mem::transmute(f) })
    }

    /// cf. [`PartialOrdFn`] for partial ordering comparison
    #[inline(always)]
    pub fn partial_ord(&self) -> Option<PartialOrdFnTyped<T>> {
        self.0.cmp.partial_ord.map(|f| unsafe { mem::transmute(f) })
    }

    /// cf. [`CmpFn`] for total ordering
    #[inline(always)]
    pub fn ord(&self) -> Option<CmpFnTyped<T>> {
        self.0.cmp.ord.map(|f| unsafe { mem::transmute(f) })
    }

    /// cf. [`HashFn`]
    #[inline(always)]
    pub fn hash(&self) -> Option<HashFnTyped<T>> {
        self.0.hash.hash.map(|f| unsafe { mem::transmute(f) })
    }

    /// Get the marker traits
    #[inline(always)]
    pub const fn markers(&self) -> MarkerTraits {
        self.0.markers
    }

    /// cf. [`TryBorrowInnerFn`]
    ///
    /// This is used by transparent types to efficiently access the inner value without copying.
    #[inline(always)]
    pub fn try_borrow_inner(&self) -> Option<TryBorrowInnerFnTyped<T>> {
        self.0
            .try_borrow_inner
            .map(|f| unsafe { mem::transmute(f) })
    }
}

impl<'a, T: crate::Facet<'a>> VTableView<T> {
    /// cf. [`DefaultInPlaceFn`]
    #[inline(always)]
    pub fn default_in_place(&self) -> Option<DefaultInPlaceFnTyped<T>> {
        self.0.default_in_place.map(|default_in_place| unsafe {
            mem::transmute::<DefaultInPlaceFn, DefaultInPlaceFnTyped<T>>(default_in_place)
        })
    }

    /// cf. [`CloneIntoFn`]
    #[inline(always)]
    pub fn clone_into(&self) -> Option<CloneIntoFnTyped<T>> {
        self.0.clone_into.map(|clone_into| unsafe {
            mem::transmute::<CloneIntoFn, CloneIntoFnTyped<T>>(clone_into)
        })
    }

    /// cf. [`ParseFn`]
    #[inline(always)]
    pub fn parse(&self) -> Option<ParseFnTyped<T>> {
        self.0
            .parse
            .map(|parse| unsafe { mem::transmute::<ParseFn, ParseFnTyped<T>>(parse) })
    }

    /// cf. [`TryFromFn`]
    ///
    /// This also acts as a "TryFromInner" — you can use it to go:
    ///
    ///   * `String` => `Utf8PathBuf`
    ///   * `String` => `Uuid`
    ///   * `T` => `Option<T>`
    ///   * `T` => `Arc<T>`
    ///   * `T` => `NonZero<T>`
    ///   * etc.
    ///
    #[inline(always)]
    pub fn try_from(&self) -> Option<TryFromFnTyped<T>> {
        self.0
            .try_from
            .map(|try_from| unsafe { mem::transmute::<TryFromFn, TryFromFnTyped<T>>(try_from) })
    }

    /// cf. [`TryIntoInnerFn`]
    ///
    /// This is used by transparent types to convert the wrapper type into its inner value.
    /// Primarily used during serialization.
    #[inline(always)]
    pub fn try_into_inner(&self) -> Option<TryIntoInnerFnTyped<T>> {
        self.0.try_into_inner.map(|try_into_inner| unsafe {
            mem::transmute::<TryIntoInnerFn, TryIntoInnerFnTyped<T>>(try_into_inner)
        })
    }
}
