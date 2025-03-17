use std::{alloc::Layout, any::TypeId, ptr::NonNull};

mod pretty_print;

/// Schema for reflection of a type
#[derive(Clone, Copy)]
pub struct Shape {
    /// A descriptive name for the type, e.g. `u64`, or `Person`
    pub name: NameFn,

    /// The typeid of the underlying type
    pub typeid: TypeId,

    /// Size & alignment
    pub layout: Layout,

    /// Details/contents of the value
    pub innards: Innards,

    /// Set the value at a given address to the default value for this type
    pub set_to_default: Option<SetToDefaultFn>,

    /// Drop the value at a given address
    ///
    /// # Safety
    ///
    /// This function should be called only for initialized values.
    /// It's the caller's responsibility to ensure the address points to a valid value.
    pub drop_in_place: Option<DropFn>,
}

pub type NameFn = fn(f: &mut std::fmt::Formatter) -> std::fmt::Result;

// Helper struct to format the name for display
impl std::fmt::Display for Shape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        (self.name)(f)
    }
}

/// A function that sets a value to its default at a specific memory address
pub type SetToDefaultFn = unsafe fn(*mut u8);

/// A function that drops a value at a specific memory address
pub type DropFn = unsafe fn(*mut u8);

impl PartialEq for Shape {
    fn eq(&self, other: &Self) -> bool {
        self.typeid == other.typeid
    }
}

impl Eq for Shape {}

impl std::hash::Hash for Shape {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.typeid.hash(state);
    }
}

impl Shape {
    const INDENT: usize = 2;

    /// Extract the scalar contents from a shape and a pointer
    ///
    /// # Safety
    ///
    /// This function is unsafe because it reads from raw memory.
    /// The caller must ensure that:
    /// 1. The pointer points to a valid, initialized value of the correct type
    /// 2. The memory is properly aligned for the type
    /// 3. The memory is not mutated while the returned ScalarContents is in use
    pub unsafe fn get_scalar_contents<'a>(&self, ptr: *const u8) -> crate::ScalarContents<'a> {
        match self.innards {
            Innards::Scalar(scalar) => unsafe { scalar.get_contents(ptr) },
            _ => panic!("Expected a scalar shape"),
        }
    }

    /// Returns a slice of statically known fields. Fields that are not in there might still be inserted if it's a dynamic collection.
    pub fn known_fields(&self) -> &'static [Field] {
        match self.innards {
            Innards::Struct { fields } => fields,
            _ => &[],
        }
    }

    /// Returns a reference to a field with the given name, if it exists
    pub fn field_by_name(&self, name: &str) -> Option<&Field> {
        self.known_fields().iter().find(|field| field.name == name)
    }

    /// Returns a reference to a field with the given index, if it exists
    pub fn field_by_index(&self, index: usize) -> Result<&Field, FieldError> {
        match self.innards {
            Innards::Struct { fields } => fields.get(index).ok_or(FieldError::IndexOutOfBounds),
            _ => Err(FieldError::NotAStruct),
        }
    }

    /// Returns a dangling pointer for this shape.
    ///
    /// This is useful for zero-sized types (ZSTs) which don't need actual memory allocation,
    /// but still need a properly aligned "some address".
    ///
    /// # Safety
    ///
    /// This function returns a dangling pointer. It should only be used in contexts where
    /// a non-null pointer is required but no actual memory access will occur, such as for ZSTs.
    pub fn dangling(&self) -> NonNull<u8> {
        let dang = NonNull::dangling();
        let offset = dang.align_offset(self.layout.align());
        unsafe { dang.byte_add(offset) }
    }

    /// Returns a slice of enum variants, if this shape represents an enum
    pub fn variants(&self) -> &'static [Variant] {
        match self.innards {
            Innards::Enum { variants, repr: _ } => variants,
            _ => &[],
        }
    }

    /// Returns a reference to a variant with the given name, if it exists
    pub fn variant_by_name(&self, name: &str) -> Option<&Variant> {
        self.variants().iter().find(|variant| variant.name == name)
    }

    /// Returns a reference to a variant with the given index, if it exists
    pub fn variant_by_index(&self, index: usize) -> Result<&Variant, VariantError> {
        match self.innards {
            Innards::Enum { variants, repr: _ } => {
                variants.get(index).ok_or(VariantError::IndexOutOfBounds)
            }
            _ => Err(VariantError::NotAnEnum),
        }
    }

    /// Returns the enum representation, if this shape represents an enum
    pub fn enum_repr(&self) -> Option<EnumRepr> {
        match self.innards {
            Innards::Enum { variants: _, repr } => Some(repr),
            _ => None,
        }
    }
}

/// Errors encountered when calling `field_by_index` or `field_by_name`
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FieldError {
    /// `field_by_index` was called on a dynamic collection, that has no
    /// static fields. a HashMap doesn't have a "first field", it can only
    /// associate by keys.
    NoStaticFields,

    /// `field_by_name` was called on a struct, and there is no static field
    /// with the given key.
    NoSuchStaticField,

    /// `field_by_index` was called on a fixed-size collection (like a tuple,
    /// a struct, or a fixed-size array) and the index was out of bounds.
    IndexOutOfBounds,

    /// `field_by_index` or `field_by_name` was called on a non-struct type.
    NotAStruct,
}

impl std::error::Error for FieldError {}

impl std::fmt::Display for FieldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FieldError::NoStaticFields => write!(f, "No static fields available"),
            FieldError::NoSuchStaticField => write!(f, "No such static field"),
            FieldError::IndexOutOfBounds => write!(f, "Index out of bounds"),
            FieldError::NotAStruct => write!(f, "Not a struct"),
        }
    }
}

impl std::fmt::Debug for Shape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.pretty_print_recursive(f)
    }
}

/// The shape of a schema: is it more map-shaped, array-shaped, scalar-shaped?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Innards {
    /// Struct with statically-known fields
    Struct { fields: &'static [Field] },

    /// HashMap — keys are dynamic, values are homogeneous
    HashMap { value_shape: ShapeDesc },

    /// Ordered list of heterogenous values, variable size
    Array(ShapeDesc),

    /// Transparent — forwards to another known schema
    Transparent(ShapeDesc),

    /// Scalar — known based type
    Scalar(Scalar),

    /// Enum with variants
    Enum {
        variants: &'static [Variant],
        repr: EnumRepr,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Field {
    /// key for the map field
    pub name: &'static str,

    /// schema of the inner type
    pub shape: ShapeDesc,

    /// offset of the field in the map, if known.
    ///
    /// For example, when deserializing a self-descriptive format like JSON, we're going to get
    /// some map fields with dynamically discovered field names, and they're not going to have
    /// an offset.
    ///
    /// However, when deserializing formats that are non-self descriptive and working from an
    /// existing shape, then their map fields are probably going to have offsets, especially if
    /// they're using derived macros.
    pub offset: usize,

    /// Flags for the field (e.g. sensitive)
    pub flags: FieldFlags,
}

/// The outcome of trying to set a field on a map
#[derive(Debug, Clone, Copy)]
pub enum SetFieldOutcome {
    /// The field was successfully set
    Accepted,

    /// The field was rejected (unknown field set in a struct, for example)
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Scalar {
    // Valid utf-8
    String,

    // Not valid utf-8 🤷
    Bytes,

    I8,
    I16,
    I32,
    I64,
    I128,

    U8,
    U16,
    U32,
    U64,
    U128,

    F32,
    F64,

    Boolean,

    /// An empty tuple, null, undefined, whatever you wish
    Nothing,
}

/// A function that returns a shape. There should only be one of these per concrete type in a
#[derive(Clone, Copy)]
pub struct ShapeDesc(pub fn() -> Shape);

impl From<fn() -> Shape> for ShapeDesc {
    fn from(f: fn() -> Shape) -> Self {
        Self(f)
    }
}

impl ShapeDesc {
    /// Build the inner shape
    #[inline(always)]
    pub fn get(&self) -> Shape {
        (self.0)()
    }
}

impl PartialEq for ShapeDesc {
    fn eq(&self, other: &Self) -> bool {
        if std::ptr::eq(self.0 as *const (), other.0 as *const ()) {
            true
        } else {
            let self_shape = self.0();
            let other_shape = other.0();
            self_shape == other_shape
        }
    }
}

impl Eq for ShapeDesc {}

impl std::hash::Hash for ShapeDesc {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash the function pointer
        (self.0 as *const ()).hash(state);
    }
}

impl std::fmt::Debug for ShapeDesc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.get().fmt(f)
    }
}

/// Flags that can be applied to fields to modify their behavior
///
/// # Examples
///
/// ```rust
/// use shapely_core::FieldFlags;
///
/// // Create flags with the sensitive bit set
/// let flags = FieldFlags::SENSITIVE;
/// assert!(flags.is_sensitive());
///
/// // Combine multiple flags using bitwise OR
/// let flags = FieldFlags::SENSITIVE | FieldFlags::EMPTY;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldFlags(u64);

impl FieldFlags {
    /// An empty set of flags
    pub const EMPTY: Self = Self(0);

    /// Flag indicating this field contains sensitive data that should not be displayed
    pub const SENSITIVE: Self = Self(1 << 0);

    /// Returns true if the sensitive flag is set
    #[inline]
    pub fn is_sensitive(&self) -> bool {
        self.0 & Self::SENSITIVE.0 != 0
    }

    /// Sets the sensitive flag and returns self for chaining
    #[inline]
    pub fn set_sensitive(&mut self) -> &mut Self {
        self.0 |= Self::SENSITIVE.0;
        self
    }

    /// Unsets the sensitive flag and returns self for chaining
    #[inline]
    pub fn unset_sensitive(&mut self) -> &mut Self {
        self.0 &= !Self::SENSITIVE.0;
        self
    }

    /// Creates a new FieldFlags with the sensitive flag set
    #[inline]
    pub const fn sensitive() -> Self {
        Self::SENSITIVE
    }
}

impl std::ops::BitOr for FieldFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for FieldFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl Default for FieldFlags {
    #[inline(always)]
    fn default() -> Self {
        Self::EMPTY
    }
}

impl std::fmt::Display for FieldFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 == 0 {
            return write!(f, "none");
        }

        // Define a vector of flag entries: (flag bit, name)
        let flags = [
            (Self::SENSITIVE.0, "sensitive"),
            // Future flags can be easily added here:
            // (Self::SOME_FLAG.0, "some_flag"),
            // (Self::ANOTHER_FLAG.0, "another_flag"),
        ];

        // Write all active flags with proper separators
        let mut is_first = true;
        for (bit, name) in flags {
            if self.0 & bit != 0 {
                if !is_first {
                    write!(f, ", ")?;
                }
                is_first = false;
                write!(f, "{}", name)?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VariantKind {
    /// Unit variant (e.g., `None` in Option)
    Unit,

    /// Tuple variant with unnamed fields (e.g., `Some(T)` in Option)
    Tuple { fields: &'static [Field] },

    /// Struct variant with named fields (e.g., `Struct { field: T }`)
    Struct { fields: &'static [Field] },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EnumRepr {
    /// Default representation (compiler-dependent)
    Default,
    /// u8 representation (#[repr(u8)])
    U8,
    /// u16 representation (#[repr(u16)])
    U16,
    /// u32 representation (#[repr(u32)])
    U32,
    /// u64 representation (#[repr(u64)])
    U64,
    /// usize representation (#[repr(usize)])
    USize,
    /// i8 representation (#[repr(i8)])
    I8,
    /// i16 representation (#[repr(i16)])
    I16,
    /// i32 representation (#[repr(i32)])
    I32,
    /// i64 representation (#[repr(i64)])
    I64,
    /// isize representation (#[repr(isize)])
    ISize,
}

impl Default for EnumRepr {
    fn default() -> Self {
        Self::Default
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Variant {
    /// Name of the variant
    pub name: &'static str,

    /// Discriminant value (if available)
    pub discriminant: Option<i64>,

    /// Kind of variant (unit, tuple, or struct)
    pub kind: VariantKind,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum VariantError {
    /// `variant_by_index` was called with an index that is out of bounds.
    IndexOutOfBounds,

    /// `variant_by_name` or `variant_by_index` was called on a non-enum type.
    NotAnEnum,

    /// `variant_by_name` was called with a name that doesn't match any variant.
    NoSuchVariant,
}

impl std::error::Error for VariantError {}

impl std::fmt::Display for VariantError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VariantError::IndexOutOfBounds => write!(f, "Variant index out of bounds"),
            VariantError::NotAnEnum => write!(f, "Not an enum"),
            VariantError::NoSuchVariant => write!(f, "No such variant"),
        }
    }
}
