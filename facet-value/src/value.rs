//! Core `Value` type implementation using tagged pointers.
//!
//! # Memory Layout
//!
//! `Value` is a single pointer that encodes both the type tag and the data:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                        64-bit pointer                       │
//! ├──────────────────────────────────────────────────────┬──────┤
//! │                   payload (61 bits)                  │ tag  │
//! │                                                      │(3bit)│
//! └──────────────────────────────────────────────────────┴──────┘
//! ```
//!
//! ## Inline vs Heap Values
//!
//! We distinguish inline values from heap pointers primarily by checking if `ptr < 8`.
//! Additionally, some tag patterns (like inline short strings) are treated as inline even if
//! the encoded pointer is ≥ 8 because their payload lives directly in the pointer bits.
//!
//! - **Inline values**: Either `ptr < 8` (null/booleans) or the tag explicitly denotes an inline
//!   payload (e.g. short strings).
//! - **Heap pointers** (ptr ≥ 8 without an inline tag): Value is `aligned_address | tag`
//!
//! Since heap addresses are 8-byte aligned (≥ 8) and tags are < 8, heap pointers
//! are always ≥ 8 after OR-ing in the tag.
//!
//! ```text
//! NULL:   ptr = 1                      → 1 < 8  → inline, tag=1 → Null
//! String: ptr = 0x7f8a2000 | 1 = ...001  → ≥8  → heap,   tag=1 → String
//!                                    └─ tag in low bits
//! ```
//!
//! ## Tag Allocation
//!
//! | Tag | Inline (ptr < 8) | Heap (ptr ≥ 8) |
//! |-----|------------------|----------------|
//! | 0   | (invalid)        | Number         |
//! | 1   | Null             | String         |
//! | 2   | False            | Bytes          |
//! | 3   | True             | Array          |
//! | 4   | (invalid)        | Object         |
//! | 5   | (invalid)        | DateTime       |
//! | 6   | (inline short string payload) | (inline short string payload) |
//! | 7   | reserved        | reserved       |

use core::fmt::{self, Debug, Formatter};
use core::hash::{Hash, Hasher};
use core::mem;
use core::ptr::{self, NonNull};

use crate::array::VArray;
use crate::bytes::VBytes;
use crate::datetime::VDateTime;
use crate::number::VNumber;
use crate::object::VObject;
use crate::other::{OtherKind, VQName, VUuid, get_other_kind};
use crate::string::{VSafeString, VString};

/// Alignment for heap-allocated values. Using 8-byte alignment gives us 3 tag bits.
pub(crate) const ALIGNMENT: usize = 8;

/// Type tags encoded in the low 3 bits of the pointer.
#[repr(usize)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum TypeTag {
    /// Number type (always heap-allocated for now)
    Number = 0,
    /// String (pointer) or Null (inline when ptr < ALIGNMENT)
    StringOrNull = 1,
    /// Bytes (pointer) or False (inline when ptr < ALIGNMENT)
    BytesOrFalse = 2,
    /// Array (pointer) or True (inline when ptr < ALIGNMENT)
    ArrayOrTrue = 3,
    /// Object type
    Object = 4,
    /// DateTime type
    DateTime = 5,
    /// Inline short string payload (data encoded directly in the pointer bits)
    InlineString = 6,
    /// Extensible "Other" types with secondary discriminant on the heap
    Other = 7,
}

impl From<usize> for TypeTag {
    fn from(other: usize) -> Self {
        // Safety: We mask to 3 bits, values 0-7 are all valid
        match other & 0b111 {
            0 => TypeTag::Number,
            1 => TypeTag::StringOrNull,
            2 => TypeTag::BytesOrFalse,
            3 => TypeTag::ArrayOrTrue,
            4 => TypeTag::Object,
            5 => TypeTag::DateTime,
            6 => TypeTag::InlineString,
            7 => TypeTag::Other,
            _ => unreachable!(),
        }
    }
}

/// Enum distinguishing the value types.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValueType {
    /// Null value
    Null,
    /// Boolean value
    Bool,
    /// Number (integers and floats)
    Number,
    /// String (UTF-8)
    String,
    /// Binary data (useful for binary formats)
    Bytes,
    /// Array
    Array,
    /// Object (key-value map)
    Object,
    /// DateTime (offset, local datetime, local date, or local time)
    DateTime,
    /// Qualified name (namespace + local name, for XML namespaces)
    QName,
    /// UUID (128-bit universally unique identifier)
    Uuid,
}

/// A dynamic value that can represent null, booleans, numbers, strings, bytes, arrays, or objects.
///
/// `Value` is exactly one pointer in size and uses tagged pointers for efficient type discrimination.
/// Small values like null, booleans, and small integers are stored inline without heap allocation.
#[repr(transparent)]
pub struct Value {
    ptr: NonNull<u8>,
}

// Safety: Value's internal pointer is either a tagged inline value or points to
// Send+Sync heap data that we own.
unsafe impl Send for Value {}
unsafe impl Sync for Value {}

impl Value {
    // === Constants for inline values ===

    /// JSON `null` value.
    pub const NULL: Self = unsafe { Self::new_inline(TypeTag::StringOrNull) };

    /// JSON `false` value.
    pub const FALSE: Self = unsafe { Self::new_inline(TypeTag::BytesOrFalse) };

    /// JSON `true` value.
    pub const TRUE: Self = unsafe { Self::new_inline(TypeTag::ArrayOrTrue) };

    // === Internal constructors ===

    /// Create an inline value (for null, true, false).
    /// Safety: Tag must not be Number or Object (those require pointers).
    const unsafe fn new_inline(tag: TypeTag) -> Self {
        unsafe {
            Self {
                // Use without_provenance since inline values are data packed into
                // pointer bits, not actual pointers to memory.
                ptr: NonNull::new_unchecked(ptr::without_provenance_mut(tag as usize)),
            }
        }
    }

    /// Create a value from a heap pointer.
    /// Safety: Pointer must be non-null and aligned to at least ALIGNMENT.
    pub(crate) unsafe fn new_ptr(p: *mut u8, tag: TypeTag) -> Self {
        debug_assert!(!p.is_null());
        debug_assert!((p as usize).is_multiple_of(ALIGNMENT));
        unsafe {
            Self {
                ptr: NonNull::new_unchecked(p.wrapping_add(tag as usize)),
            }
        }
    }

    /// Create a value from a reference.
    /// Safety: Reference must be aligned to at least ALIGNMENT.
    #[allow(dead_code)]
    pub(crate) unsafe fn new_ref<T>(r: &T, tag: TypeTag) -> Self {
        unsafe { Self::new_ptr(r as *const T as *mut u8, tag) }
    }

    // === Internal accessors ===

    /// Raw constructor from inline data bits (e.g., inline short strings).
    /// Safety: `bits` must be non-zero and encode a valid inline representation.
    /// This is only for inline values - heap pointers should use `new_ptr`.
    pub(crate) unsafe fn from_bits(bits: usize) -> Self {
        debug_assert!(bits != 0);
        Self {
            // Use without_provenance since this is inline data packed into
            // pointer bits, not an actual pointer to memory.
            ptr: unsafe { NonNull::new_unchecked(ptr::without_provenance_mut(bits)) },
        }
    }

    pub(crate) fn ptr_usize(&self) -> usize {
        self.ptr.as_ptr().addr()
    }

    fn is_inline(&self) -> bool {
        self.ptr_usize() < ALIGNMENT || self.is_inline_string()
    }

    fn type_tag(&self) -> TypeTag {
        TypeTag::from(self.ptr_usize())
    }

    /// Returns `true` if the encoded value is an inline short string.
    #[inline]
    pub(crate) fn is_inline_string(&self) -> bool {
        matches!(self.type_tag(), TypeTag::InlineString)
    }

    /// Get the actual heap pointer (strips the tag bits).
    /// Safety: Must only be called on non-inline values.
    pub(crate) fn heap_ptr(&self) -> *const u8 {
        // Use map_addr to preserve provenance (strict provenance safe)
        self.ptr.as_ptr().map_addr(|a| a & !(ALIGNMENT - 1)) as *const u8
    }

    /// Get the actual heap pointer with mutable provenance (strips the tag bits).
    /// Safety: Must only be called on non-inline values.
    pub(crate) unsafe fn heap_ptr_mut(&mut self) -> *mut u8 {
        // Use map_addr to preserve provenance from the mutable reference
        self.ptr.as_ptr().map_addr(|a| a & !(ALIGNMENT - 1))
    }

    /// Update the heap pointer while preserving the tag.
    /// Safety: New pointer must be non-null and aligned to ALIGNMENT.
    pub(crate) unsafe fn set_ptr(&mut self, ptr: *mut u8) {
        let tag = self.type_tag();
        unsafe {
            self.ptr = NonNull::new_unchecked(ptr.wrapping_add(tag as usize));
        }
    }

    /// Raw pointer equality (for comparing interned strings, etc.)
    #[allow(dead_code)]
    pub(crate) fn raw_eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr
    }

    /// Raw pointer hash
    #[allow(dead_code)]
    pub(crate) fn raw_hash<H: Hasher>(&self, state: &mut H) {
        self.ptr.hash(state);
    }

    // === Public type checking ===

    /// Returns the type of this value.
    #[must_use]
    pub fn value_type(&self) -> ValueType {
        match (self.type_tag(), self.is_inline()) {
            // Heap pointers
            (TypeTag::Number, false) => ValueType::Number,
            (TypeTag::StringOrNull, false) => ValueType::String,
            (TypeTag::BytesOrFalse, false) => ValueType::Bytes,
            (TypeTag::ArrayOrTrue, false) => ValueType::Array,
            (TypeTag::Object, false) => ValueType::Object,
            (TypeTag::DateTime, false) => ValueType::DateTime,
            (TypeTag::InlineString, false) => ValueType::String,
            (TypeTag::Other, false) => {
                // Read secondary discriminant from heap
                match unsafe { get_other_kind(self) } {
                    OtherKind::QName => ValueType::QName,
                    OtherKind::Uuid => ValueType::Uuid,
                }
            }

            // Inline values
            (TypeTag::StringOrNull, true) => ValueType::Null,
            (TypeTag::BytesOrFalse, true) => ValueType::Bool, // false
            (TypeTag::ArrayOrTrue, true) => ValueType::Bool,  // true
            (TypeTag::InlineString, true) => ValueType::String,

            // Invalid states (shouldn't happen)
            (TypeTag::Number, true)
            | (TypeTag::Object, true)
            | (TypeTag::DateTime, true)
            | (TypeTag::Other, true) => {
                // These tags require heap pointers
                unreachable!("invalid inline value with Number, Object, DateTime, or Other tag")
            }
        }
    }

    /// Returns `true` if this is the `null` value.
    #[must_use]
    pub fn is_null(&self) -> bool {
        self.ptr == Self::NULL.ptr
    }

    /// Returns `true` if this is a boolean.
    #[must_use]
    pub fn is_bool(&self) -> bool {
        self.ptr == Self::TRUE.ptr || self.ptr == Self::FALSE.ptr
    }

    /// Returns `true` if this is `true`.
    #[must_use]
    pub fn is_true(&self) -> bool {
        self.ptr == Self::TRUE.ptr
    }

    /// Returns `true` if this is `false`.
    #[must_use]
    pub fn is_false(&self) -> bool {
        self.ptr == Self::FALSE.ptr
    }

    /// Returns `true` if this is a number.
    #[must_use]
    pub fn is_number(&self) -> bool {
        self.type_tag() == TypeTag::Number && !self.is_inline()
    }

    /// Returns `true` if this is a string.
    #[must_use]
    pub fn is_string(&self) -> bool {
        match self.type_tag() {
            TypeTag::StringOrNull => !self.is_inline(),
            TypeTag::InlineString => true,
            _ => false,
        }
    }

    /// Returns `true` if this is bytes.
    #[must_use]
    pub fn is_bytes(&self) -> bool {
        self.type_tag() == TypeTag::BytesOrFalse && !self.is_inline()
    }

    /// Returns `true` if this is an array.
    #[must_use]
    pub fn is_array(&self) -> bool {
        self.type_tag() == TypeTag::ArrayOrTrue && !self.is_inline()
    }

    /// Returns `true` if this is an object.
    #[must_use]
    pub fn is_object(&self) -> bool {
        self.type_tag() == TypeTag::Object && !self.is_inline()
    }

    /// Returns `true` if this is a datetime.
    #[must_use]
    pub fn is_datetime(&self) -> bool {
        self.type_tag() == TypeTag::DateTime && !self.is_inline()
    }

    /// Returns `true` if this is a qualified name.
    #[must_use]
    pub fn is_qname(&self) -> bool {
        self.value_type() == ValueType::QName
    }

    /// Returns `true` if this is a UUID.
    #[must_use]
    pub fn is_uuid(&self) -> bool {
        self.value_type() == ValueType::Uuid
    }

    // === Conversions to concrete types ===

    /// Converts this value to a `bool`. Returns `None` if not a boolean.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        if self.is_bool() {
            Some(self.is_true())
        } else {
            None
        }
    }

    /// Gets a reference to this value as a `VNumber`. Returns `None` if not a number.
    #[must_use]
    pub fn as_number(&self) -> Option<&VNumber> {
        if self.is_number() {
            // Safety: We checked the type, and VNumber is repr(transparent) over Value
            Some(unsafe { &*(self as *const Value as *const VNumber) })
        } else {
            None
        }
    }

    /// Gets a mutable reference to this value as a `VNumber`.
    pub fn as_number_mut(&mut self) -> Option<&mut VNumber> {
        if self.is_number() {
            Some(unsafe { &mut *(self as *mut Value as *mut VNumber) })
        } else {
            None
        }
    }

    /// Gets a reference to this value as a `VString`. Returns `None` if not a string.
    #[must_use]
    pub fn as_string(&self) -> Option<&VString> {
        if self.is_string() {
            Some(unsafe { &*(self as *const Value as *const VString) })
        } else {
            None
        }
    }

    /// Gets a mutable reference to this value as a `VString`.
    pub fn as_string_mut(&mut self) -> Option<&mut VString> {
        if self.is_string() {
            Some(unsafe { &mut *(self as *mut Value as *mut VString) })
        } else {
            None
        }
    }

    /// Returns `true` if this is a safe string (marked as pre-escaped HTML, etc.).
    ///
    /// A safe string is a string with the safe flag set. Inline strings are never safe.
    #[must_use]
    pub fn is_safe_string(&self) -> bool {
        self.as_string().is_some_and(|s| s.is_safe())
    }

    /// Gets a reference to this value as a `VSafeString`. Returns `None` if not a safe string.
    #[must_use]
    pub fn as_safe_string(&self) -> Option<&VSafeString> {
        if self.is_safe_string() {
            Some(unsafe { &*(self as *const Value as *const VSafeString) })
        } else {
            None
        }
    }

    /// Gets a mutable reference to this value as a `VSafeString`.
    pub fn as_safe_string_mut(&mut self) -> Option<&mut VSafeString> {
        if self.is_safe_string() {
            Some(unsafe { &mut *(self as *mut Value as *mut VSafeString) })
        } else {
            None
        }
    }

    /// Gets a reference to this value as `VBytes`. Returns `None` if not bytes.
    #[must_use]
    pub fn as_bytes(&self) -> Option<&VBytes> {
        if self.is_bytes() {
            Some(unsafe { &*(self as *const Value as *const VBytes) })
        } else {
            None
        }
    }

    /// Gets a mutable reference to this value as `VBytes`.
    pub fn as_bytes_mut(&mut self) -> Option<&mut VBytes> {
        if self.is_bytes() {
            Some(unsafe { &mut *(self as *mut Value as *mut VBytes) })
        } else {
            None
        }
    }

    /// Gets a reference to this value as a `VArray`. Returns `None` if not an array.
    #[must_use]
    pub fn as_array(&self) -> Option<&VArray> {
        if self.is_array() {
            Some(unsafe { &*(self as *const Value as *const VArray) })
        } else {
            None
        }
    }

    /// Gets a mutable reference to this value as a `VArray`.
    pub fn as_array_mut(&mut self) -> Option<&mut VArray> {
        if self.is_array() {
            Some(unsafe { &mut *(self as *mut Value as *mut VArray) })
        } else {
            None
        }
    }

    /// Gets a reference to this value as a `VObject`. Returns `None` if not an object.
    #[must_use]
    pub fn as_object(&self) -> Option<&VObject> {
        if self.is_object() {
            Some(unsafe { &*(self as *const Value as *const VObject) })
        } else {
            None
        }
    }

    /// Gets a mutable reference to this value as a `VObject`.
    pub fn as_object_mut(&mut self) -> Option<&mut VObject> {
        if self.is_object() {
            Some(unsafe { &mut *(self as *mut Value as *mut VObject) })
        } else {
            None
        }
    }

    /// Gets a reference to this value as a `VDateTime`. Returns `None` if not a datetime.
    #[must_use]
    pub fn as_datetime(&self) -> Option<&VDateTime> {
        if self.is_datetime() {
            Some(unsafe { &*(self as *const Value as *const VDateTime) })
        } else {
            None
        }
    }

    /// Gets a mutable reference to this value as a `VDateTime`.
    pub fn as_datetime_mut(&mut self) -> Option<&mut VDateTime> {
        if self.is_datetime() {
            Some(unsafe { &mut *(self as *mut Value as *mut VDateTime) })
        } else {
            None
        }
    }

    /// Gets a reference to this value as a `VQName`. Returns `None` if not a qualified name.
    #[must_use]
    pub fn as_qname(&self) -> Option<&VQName> {
        if self.is_qname() {
            Some(unsafe { &*(self as *const Value as *const VQName) })
        } else {
            None
        }
    }

    /// Gets a mutable reference to this value as a `VQName`.
    pub fn as_qname_mut(&mut self) -> Option<&mut VQName> {
        if self.is_qname() {
            Some(unsafe { &mut *(self as *mut Value as *mut VQName) })
        } else {
            None
        }
    }

    /// Gets a reference to this value as a `VUuid`. Returns `None` if not a UUID.
    #[must_use]
    pub fn as_uuid(&self) -> Option<&VUuid> {
        if self.is_uuid() {
            Some(unsafe { &*(self as *const Value as *const VUuid) })
        } else {
            None
        }
    }

    /// Gets a mutable reference to this value as a `VUuid`.
    pub fn as_uuid_mut(&mut self) -> Option<&mut VUuid> {
        if self.is_uuid() {
            Some(unsafe { &mut *(self as *mut Value as *mut VUuid) })
        } else {
            None
        }
    }

    /// Takes this value, replacing it with `Value::NULL`.
    pub const fn take(&mut self) -> Value {
        mem::replace(self, Value::NULL)
    }
}

// === Clone ===

impl Clone for Value {
    fn clone(&self) -> Self {
        match self.value_type() {
            ValueType::Null | ValueType::Bool => {
                // Inline values can be trivially copied
                Self { ptr: self.ptr }
            }
            ValueType::Number => unsafe { self.as_number().unwrap_unchecked() }.clone_impl(),
            ValueType::String => unsafe { self.as_string().unwrap_unchecked() }.clone_impl(),
            ValueType::Bytes => unsafe { self.as_bytes().unwrap_unchecked() }.clone_impl(),
            ValueType::Array => unsafe { self.as_array().unwrap_unchecked() }.clone_impl(),
            ValueType::Object => unsafe { self.as_object().unwrap_unchecked() }.clone_impl(),
            ValueType::DateTime => unsafe { self.as_datetime().unwrap_unchecked() }.clone_impl(),
            ValueType::QName => unsafe { self.as_qname().unwrap_unchecked() }.clone_impl(),
            ValueType::Uuid => unsafe { self.as_uuid().unwrap_unchecked() }.clone_impl(),
        }
    }
}

// === Drop ===

impl Drop for Value {
    fn drop(&mut self) {
        match self.value_type() {
            ValueType::Null | ValueType::Bool => {
                // Inline values don't need dropping
            }
            ValueType::Number => unsafe { self.as_number_mut().unwrap_unchecked() }.drop_impl(),
            ValueType::String => unsafe { self.as_string_mut().unwrap_unchecked() }.drop_impl(),
            ValueType::Bytes => unsafe { self.as_bytes_mut().unwrap_unchecked() }.drop_impl(),
            ValueType::Array => unsafe { self.as_array_mut().unwrap_unchecked() }.drop_impl(),
            ValueType::Object => unsafe { self.as_object_mut().unwrap_unchecked() }.drop_impl(),
            ValueType::DateTime => unsafe { self.as_datetime_mut().unwrap_unchecked() }.drop_impl(),
            ValueType::QName => unsafe { self.as_qname_mut().unwrap_unchecked() }.drop_impl(),
            ValueType::Uuid => unsafe { self.as_uuid_mut().unwrap_unchecked() }.drop_impl(),
        }
    }
}

// === PartialEq, Eq ===

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        let (t1, t2) = (self.value_type(), other.value_type());
        if t1 != t2 {
            return false;
        }

        match t1 {
            ValueType::Null | ValueType::Bool => self.ptr == other.ptr,
            ValueType::Number => unsafe {
                self.as_number().unwrap_unchecked() == other.as_number().unwrap_unchecked()
            },
            ValueType::String => unsafe {
                self.as_string().unwrap_unchecked() == other.as_string().unwrap_unchecked()
            },
            ValueType::Bytes => unsafe {
                self.as_bytes().unwrap_unchecked() == other.as_bytes().unwrap_unchecked()
            },
            ValueType::Array => unsafe {
                self.as_array().unwrap_unchecked() == other.as_array().unwrap_unchecked()
            },
            ValueType::Object => unsafe {
                self.as_object().unwrap_unchecked() == other.as_object().unwrap_unchecked()
            },
            ValueType::DateTime => unsafe {
                self.as_datetime().unwrap_unchecked() == other.as_datetime().unwrap_unchecked()
            },
            ValueType::QName => unsafe {
                self.as_qname().unwrap_unchecked() == other.as_qname().unwrap_unchecked()
            },
            ValueType::Uuid => unsafe {
                self.as_uuid().unwrap_unchecked() == other.as_uuid().unwrap_unchecked()
            },
        }
    }
}

impl Eq for Value {}

// === PartialOrd ===

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        use core::cmp::Ordering;

        let (t1, t2) = (self.value_type(), other.value_type());

        // Different types: compare by type discriminant
        if t1 != t2 {
            return t1.partial_cmp(&t2);
        }

        // Same type: compare values
        match t1 {
            ValueType::Null => Some(Ordering::Equal),
            ValueType::Bool => self.is_true().partial_cmp(&other.is_true()),
            ValueType::Number => unsafe {
                self.as_number()
                    .unwrap_unchecked()
                    .partial_cmp(other.as_number().unwrap_unchecked())
            },
            ValueType::String => unsafe {
                self.as_string()
                    .unwrap_unchecked()
                    .partial_cmp(other.as_string().unwrap_unchecked())
            },
            ValueType::Bytes => unsafe {
                self.as_bytes()
                    .unwrap_unchecked()
                    .partial_cmp(other.as_bytes().unwrap_unchecked())
            },
            ValueType::Array => unsafe {
                self.as_array()
                    .unwrap_unchecked()
                    .partial_cmp(other.as_array().unwrap_unchecked())
            },
            // Objects don't have a natural ordering
            ValueType::Object => None,
            // DateTime comparison (returns None for different kinds)
            ValueType::DateTime => unsafe {
                self.as_datetime()
                    .unwrap_unchecked()
                    .partial_cmp(other.as_datetime().unwrap_unchecked())
            },
            // QNames don't have a natural ordering
            ValueType::QName => None,
            // UUIDs can be compared by their byte representation
            ValueType::Uuid => unsafe {
                self.as_uuid()
                    .unwrap_unchecked()
                    .as_bytes()
                    .partial_cmp(other.as_uuid().unwrap_unchecked().as_bytes())
            },
        }
    }
}

// === Hash ===

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash the type first
        self.value_type().hash(state);

        match self.value_type() {
            ValueType::Null => {}
            ValueType::Bool => self.is_true().hash(state),
            ValueType::Number => unsafe { self.as_number().unwrap_unchecked() }.hash(state),
            ValueType::String => unsafe { self.as_string().unwrap_unchecked() }.hash(state),
            ValueType::Bytes => unsafe { self.as_bytes().unwrap_unchecked() }.hash(state),
            ValueType::Array => unsafe { self.as_array().unwrap_unchecked() }.hash(state),
            ValueType::Object => unsafe { self.as_object().unwrap_unchecked() }.hash(state),
            ValueType::DateTime => unsafe { self.as_datetime().unwrap_unchecked() }.hash(state),
            ValueType::QName => unsafe { self.as_qname().unwrap_unchecked() }.hash(state),
            ValueType::Uuid => unsafe { self.as_uuid().unwrap_unchecked() }.hash(state),
        }
    }
}

// === Debug ===

impl Debug for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.value_type() {
            ValueType::Null => f.write_str("null"),
            ValueType::Bool => Debug::fmt(&self.is_true(), f),
            ValueType::Number => Debug::fmt(unsafe { self.as_number().unwrap_unchecked() }, f),
            ValueType::String => Debug::fmt(unsafe { self.as_string().unwrap_unchecked() }, f),
            ValueType::Bytes => Debug::fmt(unsafe { self.as_bytes().unwrap_unchecked() }, f),
            ValueType::Array => Debug::fmt(unsafe { self.as_array().unwrap_unchecked() }, f),
            ValueType::Object => Debug::fmt(unsafe { self.as_object().unwrap_unchecked() }, f),
            ValueType::DateTime => Debug::fmt(unsafe { self.as_datetime().unwrap_unchecked() }, f),
            ValueType::QName => Debug::fmt(unsafe { self.as_qname().unwrap_unchecked() }, f),
            ValueType::Uuid => Debug::fmt(unsafe { self.as_uuid().unwrap_unchecked() }, f),
        }
    }
}

// === Default ===

impl Default for Value {
    fn default() -> Self {
        Self::NULL
    }
}

// === From implementations ===

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        if b { Self::TRUE } else { Self::FALSE }
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(opt: Option<T>) -> Self {
        match opt {
            Some(v) => v.into(),
            None => Self::NULL,
        }
    }
}

// === FromIterator implementations ===

#[cfg(feature = "alloc")]
impl<T: Into<Value>> core::iter::FromIterator<T> for Value {
    /// Collect into an array Value.
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        VArray::from_iter(iter).into()
    }
}

#[cfg(feature = "alloc")]
impl<K: Into<VString>, V: Into<Value>> core::iter::FromIterator<(K, V)> for Value {
    /// Collect key-value pairs into an object Value.
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        VObject::from_iter(iter).into()
    }
}

/// Enum for destructuring a `Value` by ownership.
#[derive(Debug, Clone, PartialEq)]
pub enum Destructured {
    /// Null value
    Null,
    /// Boolean value
    Bool(bool),
    /// Number value
    Number(VNumber),
    /// String value
    String(VString),
    /// Bytes value
    Bytes(VBytes),
    /// Array value
    Array(VArray),
    /// Object value
    Object(VObject),
    /// DateTime value
    DateTime(VDateTime),
    /// Qualified name value
    QName(VQName),
    /// UUID value
    Uuid(VUuid),
}

/// Enum for destructuring a `Value` by reference.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum DestructuredRef<'a> {
    /// Null value
    Null,
    /// Boolean value
    Bool(bool),
    /// Number value
    Number(&'a VNumber),
    /// String value
    String(&'a VString),
    /// Bytes value
    Bytes(&'a VBytes),
    /// Array value
    Array(&'a VArray),
    /// Object value
    Object(&'a VObject),
    /// DateTime value
    DateTime(&'a VDateTime),
    /// Qualified name value
    QName(&'a VQName),
    /// UUID value
    Uuid(&'a VUuid),
}

/// Enum for destructuring a `Value` by mutable reference.
#[derive(Debug)]
pub enum DestructuredMut<'a> {
    /// Null value
    Null,
    /// Boolean value (use the mutable reference to the Value itself to change it)
    Bool(bool),
    /// Number value
    Number(&'a mut VNumber),
    /// String value
    String(&'a mut VString),
    /// Bytes value
    Bytes(&'a mut VBytes),
    /// Array value
    Array(&'a mut VArray),
    /// Object value
    Object(&'a mut VObject),
    /// DateTime value
    DateTime(&'a mut VDateTime),
    /// Qualified name value
    QName(&'a mut VQName),
    /// UUID value
    Uuid(&'a mut VUuid),
}

impl Value {
    /// Destructure this value into an enum for pattern matching (by ownership).
    #[must_use]
    pub fn destructure(self) -> Destructured {
        match self.value_type() {
            ValueType::Null => Destructured::Null,
            ValueType::Bool => Destructured::Bool(self.is_true()),
            ValueType::Number => Destructured::Number(VNumber(self)),
            ValueType::String => Destructured::String(VString(self)),
            ValueType::Bytes => Destructured::Bytes(VBytes(self)),
            ValueType::Array => Destructured::Array(VArray(self)),
            ValueType::Object => Destructured::Object(VObject(self)),
            ValueType::DateTime => Destructured::DateTime(VDateTime(self)),
            ValueType::QName => Destructured::QName(VQName(self)),
            ValueType::Uuid => Destructured::Uuid(VUuid(self)),
        }
    }

    /// Destructure this value into an enum for pattern matching (by reference).
    #[must_use]
    pub fn destructure_ref(&self) -> DestructuredRef<'_> {
        match self.value_type() {
            ValueType::Null => DestructuredRef::Null,
            ValueType::Bool => DestructuredRef::Bool(self.is_true()),
            ValueType::Number => {
                DestructuredRef::Number(unsafe { self.as_number().unwrap_unchecked() })
            }
            ValueType::String => {
                DestructuredRef::String(unsafe { self.as_string().unwrap_unchecked() })
            }
            ValueType::Bytes => {
                DestructuredRef::Bytes(unsafe { self.as_bytes().unwrap_unchecked() })
            }
            ValueType::Array => {
                DestructuredRef::Array(unsafe { self.as_array().unwrap_unchecked() })
            }
            ValueType::Object => {
                DestructuredRef::Object(unsafe { self.as_object().unwrap_unchecked() })
            }
            ValueType::DateTime => {
                DestructuredRef::DateTime(unsafe { self.as_datetime().unwrap_unchecked() })
            }
            ValueType::QName => {
                DestructuredRef::QName(unsafe { self.as_qname().unwrap_unchecked() })
            }
            ValueType::Uuid => DestructuredRef::Uuid(unsafe { self.as_uuid().unwrap_unchecked() }),
        }
    }

    /// Destructure this value into an enum for pattern matching (by mutable reference).
    pub fn destructure_mut(&mut self) -> DestructuredMut<'_> {
        match self.value_type() {
            ValueType::Null => DestructuredMut::Null,
            ValueType::Bool => DestructuredMut::Bool(self.is_true()),
            ValueType::Number => {
                DestructuredMut::Number(unsafe { self.as_number_mut().unwrap_unchecked() })
            }
            ValueType::String => {
                DestructuredMut::String(unsafe { self.as_string_mut().unwrap_unchecked() })
            }
            ValueType::Bytes => {
                DestructuredMut::Bytes(unsafe { self.as_bytes_mut().unwrap_unchecked() })
            }
            ValueType::Array => {
                DestructuredMut::Array(unsafe { self.as_array_mut().unwrap_unchecked() })
            }
            ValueType::Object => {
                DestructuredMut::Object(unsafe { self.as_object_mut().unwrap_unchecked() })
            }
            ValueType::DateTime => {
                DestructuredMut::DateTime(unsafe { self.as_datetime_mut().unwrap_unchecked() })
            }
            ValueType::QName => {
                DestructuredMut::QName(unsafe { self.as_qname_mut().unwrap_unchecked() })
            }
            ValueType::Uuid => {
                DestructuredMut::Uuid(unsafe { self.as_uuid_mut().unwrap_unchecked() })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::string::VString;

    #[test]
    fn test_size() {
        assert_eq!(
            core::mem::size_of::<Value>(),
            core::mem::size_of::<usize>(),
            "Value should be pointer-sized"
        );
        assert_eq!(
            core::mem::size_of::<Option<Value>>(),
            core::mem::size_of::<usize>(),
            "Option<Value> should be pointer-sized (niche optimization)"
        );
    }

    #[test]
    fn test_null() {
        let v = Value::NULL;
        assert!(v.is_null());
        assert_eq!(v.value_type(), ValueType::Null);
        assert!(!v.is_bool());
        assert!(!v.is_number());
    }

    #[test]
    fn test_bool() {
        let t = Value::TRUE;
        let f = Value::FALSE;

        assert!(t.is_bool());
        assert!(t.is_true());
        assert!(!t.is_false());
        assert_eq!(t.as_bool(), Some(true));

        assert!(f.is_bool());
        assert!(f.is_false());
        assert!(!f.is_true());
        assert_eq!(f.as_bool(), Some(false));

        assert_eq!(Value::from(true), Value::TRUE);
        assert_eq!(Value::from(false), Value::FALSE);
    }

    #[test]
    fn test_clone_inline() {
        let v = Value::TRUE;
        let v2 = v.clone();
        assert_eq!(v, v2);
    }

    #[test]
    fn test_inline_short_string() {
        let v: Value = VString::new("inline").into();
        assert_eq!(v.value_type(), ValueType::String);
        assert!(v.is_string());
        assert!(v.is_inline());
    }

    #[test]
    fn short_strings_are_stored_inline() {
        for len in 0..=VString::INLINE_LEN_MAX {
            let data = "s".repeat(len);
            let v = Value::from(data.as_str());
            assert!(
                v.is_inline_string(),
                "expected inline string for length {len}, ptr={:#x}",
                v.ptr_usize()
            );
            assert!(
                v.is_inline(),
                "inline flag should be true for strings of length {len}"
            );
            assert_eq!(
                v.as_string().unwrap().as_str(),
                data,
                "round-trip mismatch for inline string"
            );
        }
    }

    #[test]
    fn long_strings_force_heap_storage() {
        let long = "l".repeat(VString::INLINE_LEN_MAX + 16);
        let v = Value::from(long.as_str());
        assert!(
            !v.is_inline_string(),
            "expected heap storage for long string ptr={:#x}",
            v.ptr_usize()
        );
        assert_eq!(
            v.as_string().unwrap().as_str(),
            long,
            "heap string should round-trip"
        );
    }

    #[test]
    fn clone_preserves_inline_string_representation() {
        let original = Value::from("inline");
        assert!(original.is_inline_string());
        let clone = original.clone();
        assert!(
            clone.is_inline_string(),
            "clone lost inline tag for ptr={:#x}",
            clone.ptr_usize()
        );
        assert_eq!(
            clone.as_string().unwrap().as_str(),
            "inline",
            "clone should preserve payload"
        );
    }

    #[test]
    fn string_mutations_transition_inline_and_heap() {
        let mut value = Value::from("seed");
        assert!(value.is_inline_string());

        // Grow the string beyond inline capacity.
        {
            let slot = value.as_string_mut().expect("string value");
            let mut owned = slot.to_string();
            while owned.len() <= VString::INLINE_LEN_MAX {
                owned.push('g');
            }
            // Ensure we crossed the boundary by at least 4 bytes for good measure.
            owned.push_str("OVERFLOW");
            *slot = VString::new(&owned);
        }
        assert!(
            !value.is_inline_string(),
            "string expected to spill to heap after grow"
        );

        // Shrink back to inline size.
        {
            let slot = value.as_string_mut().expect("string value");
            let mut owned = slot.to_string();
            owned.truncate(VString::INLINE_LEN_MAX);
            *slot = VString::new(&owned);
        }
        assert!(
            value.is_inline_string(),
            "string should return to inline storage after shrink"
        );
    }
}
