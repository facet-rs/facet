//! Definition for dynamic value types that can hold any value at runtime.
//!
//! This is used for types like `facet_value::Value` or `serde_json::Value`
//! that determine their structure at runtime rather than compile time.

use super::{PtrConst, PtrMut, PtrUninit};

/// Definition for dynamic value types.
///
/// Unlike other `Def` variants that describe a fixed structure, `DynamicValueDef`
/// describes a type that can hold any of: null, bool, number, string, bytes,
/// array, or object - determined at runtime.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct DynamicValueDef {
    /// Vtable for interacting with the dynamic value
    pub vtable: &'static DynamicValueVTable,
}

impl DynamicValueDef {
    /// Construct a `DynamicValueDef` from its vtable.
    pub const fn new(vtable: &'static DynamicValueVTable) -> Self {
        Self { vtable }
    }
}

// ============================================================================
// Scalar setters
// ============================================================================

/// Set the value to null.
///
/// # Safety
///
/// `dst` must point to uninitialized memory of the correct size and alignment.
/// After this call, `dst` is fully initialized.
pub type DynSetNullFn = unsafe fn(dst: PtrUninit);

/// Set the value to a boolean.
///
/// # Safety
///
/// `dst` must point to uninitialized memory of the correct size and alignment.
/// After this call, `dst` is fully initialized.
pub type DynSetBoolFn = unsafe fn(dst: PtrUninit, value: bool);

/// Set the value to a signed 64-bit integer.
///
/// # Safety
///
/// `dst` must point to uninitialized memory of the correct size and alignment.
/// After this call, `dst` is fully initialized.
pub type DynSetI64Fn = unsafe fn(dst: PtrUninit, value: i64);

/// Set the value to an unsigned 64-bit integer.
///
/// # Safety
///
/// `dst` must point to uninitialized memory of the correct size and alignment.
/// After this call, `dst` is fully initialized.
pub type DynSetU64Fn = unsafe fn(dst: PtrUninit, value: u64);

/// Set the value to a 64-bit float.
///
/// # Safety
///
/// `dst` must point to uninitialized memory of the correct size and alignment.
/// After this call, `dst` is fully initialized.
/// Returns `false` if the value is not representable (e.g., NaN/Infinity when not supported).
pub type DynSetF64Fn = unsafe fn(dst: PtrUninit, value: f64) -> bool;

/// Set the value to a string.
///
/// # Safety
///
/// `dst` must point to uninitialized memory of the correct size and alignment.
/// After this call, `dst` is fully initialized.
pub type DynSetStrFn = unsafe fn(dst: PtrUninit, value: &str);

/// Set the value to bytes.
///
/// # Safety
///
/// `dst` must point to uninitialized memory of the correct size and alignment.
/// After this call, `dst` is fully initialized.
pub type DynSetBytesFn = unsafe fn(dst: PtrUninit, value: &[u8]);

/// The kind of datetime value (for dynamic value datetime support).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynDateTimeKind {
    /// Offset date-time with UTC offset in minutes.
    Offset {
        /// Offset from UTC in minutes. Range: -1440 to +1440 (±24 hours).
        offset_minutes: i16,
    },
    /// Local date-time without offset (civil time).
    LocalDateTime,
    /// Local date only.
    LocalDate,
    /// Local time only.
    LocalTime,
}

/// Set the value to a datetime.
///
/// # Safety
///
/// `dst` must point to uninitialized memory of the correct size and alignment.
/// After this call, `dst` is fully initialized.
pub type DynSetDateTimeFn = unsafe fn(
    dst: PtrUninit,
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    nanos: u32,
    kind: DynDateTimeKind,
);

/// Get datetime components from a datetime value.
///
/// Returns `(year, month, day, hour, minute, second, nanos, kind)` if the value is a datetime.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
pub type DynGetDateTimeFn =
    unsafe fn(value: PtrConst) -> Option<(i32, u8, u8, u8, u8, u8, u32, DynDateTimeKind)>;

// ============================================================================
// Array operations
// ============================================================================

/// Initialize the value as an empty array.
///
/// # Safety
///
/// `dst` must point to uninitialized memory of the correct size and alignment.
/// After this call, `dst` is initialized as an empty array.
pub type DynBeginArrayFn = unsafe fn(dst: PtrUninit);

/// Push an element to the array.
///
/// The element is moved out of `element` (not dropped, but the memory can be deallocated).
///
/// # Safety
///
/// `array` must point to an initialized dynamic value that is an array.
/// `element` must point to an initialized dynamic value to push.
pub type DynPushArrayElementFn = unsafe fn(array: PtrMut, element: PtrMut);

/// Finalize the array (optional, for shrinking capacity etc.).
///
/// # Safety
///
/// `array` must point to an initialized dynamic value that is an array.
pub type DynEndArrayFn = unsafe fn(array: PtrMut);

// ============================================================================
// Object operations
// ============================================================================

/// Initialize the value as an empty object.
///
/// # Safety
///
/// `dst` must point to uninitialized memory of the correct size and alignment.
/// After this call, `dst` is initialized as an empty object.
pub type DynBeginObjectFn = unsafe fn(dst: PtrUninit);

/// Insert a key-value pair into the object.
///
/// The value is moved out of `value` (not dropped, but the memory can be deallocated).
///
/// # Safety
///
/// `object` must point to an initialized dynamic value that is an object.
/// `value` must point to an initialized dynamic value to insert.
pub type DynInsertObjectEntryFn = unsafe fn(object: PtrMut, key: &str, value: PtrMut);

/// Finalize the object (optional, for shrinking capacity etc.).
///
/// # Safety
///
/// `object` must point to an initialized dynamic value that is an object.
pub type DynEndObjectFn = unsafe fn(object: PtrMut);

// ============================================================================
// Read operations (for Peek / serialization)
// ============================================================================

/// The kind of value stored in a dynamic value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynValueKind {
    /// Null value
    Null,
    /// Boolean value
    Bool,
    /// Number value (integer or float)
    Number,
    /// String value
    String,
    /// Binary data
    Bytes,
    /// Array of values
    Array,
    /// Object (string keys, dynamic values)
    Object,
    /// DateTime value
    DateTime,
    /// Qualified name (namespace + local name)
    QName,
    /// UUID (128-bit universally unique identifier)
    Uuid,
}

/// Get the kind of value stored.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
pub type DynGetKindFn = unsafe fn(value: PtrConst) -> DynValueKind;

/// Get a boolean value. Returns None if not a bool.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
pub type DynGetBoolFn = unsafe fn(value: PtrConst) -> Option<bool>;

/// Get a signed 64-bit integer value. Returns None if not representable as i64.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
pub type DynGetI64Fn = unsafe fn(value: PtrConst) -> Option<i64>;

/// Get an unsigned 64-bit integer value. Returns None if not representable as u64.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
pub type DynGetU64Fn = unsafe fn(value: PtrConst) -> Option<u64>;

/// Get a 64-bit float value. Returns None if not a number.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
pub type DynGetF64Fn = unsafe fn(value: PtrConst) -> Option<f64>;

/// Get a string reference. Returns None if not a string.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
/// The returned reference is valid for the lifetime of the value.
pub type DynGetStrFn = unsafe fn(value: PtrConst) -> Option<&'static str>;

/// Get a bytes reference. Returns None if not bytes.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
/// The returned reference is valid for the lifetime of the value.
pub type DynGetBytesFn = unsafe fn(value: PtrConst) -> Option<&'static [u8]>;

/// Get the length of an array. Returns None if not an array.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
pub type DynArrayLenFn = unsafe fn(value: PtrConst) -> Option<usize>;

/// Get an element from an array by index. Returns None if not an array or index out of bounds.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
pub type DynArrayGetFn = unsafe fn(value: PtrConst, index: usize) -> Option<PtrConst>;

/// Get the length of an object. Returns None if not an object.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
pub type DynObjectLenFn = unsafe fn(value: PtrConst) -> Option<usize>;

/// Get a key-value pair from an object by index. Returns None if not an object or index out of bounds.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
pub type DynObjectGetEntryFn =
    unsafe fn(value: PtrConst, index: usize) -> Option<(&'static str, PtrConst)>;

/// Get a value from an object by key. Returns None if not an object or key not found.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
pub type DynObjectGetFn = for<'a> unsafe fn(value: PtrConst, key: &str) -> Option<PtrConst>;

/// Get a mutable reference to a value from an object by key.
/// Returns None if not an object or key not found.
///
/// This is used for navigating into existing object entries during deserialization
/// (e.g., TOML implicit tables like `[a]` followed by `[a.b.c]`).
///
/// # Safety
///
/// `value` must point to an initialized dynamic value that is an object.
pub type DynObjectGetMutFn = for<'a> unsafe fn(value: PtrMut, key: &str) -> Option<PtrMut>;

// ============================================================================
// VTable
// ============================================================================

/// Virtual table for dynamic value types.
///
/// This provides all the operations needed to build and read dynamic values
/// without knowing their concrete type at compile time.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct DynamicValueVTable {
    // --- Scalar setters (required) ---
    /// Set to null
    pub set_null: DynSetNullFn,
    /// Set to boolean
    pub set_bool: DynSetBoolFn,
    /// Set to i64
    pub set_i64: DynSetI64Fn,
    /// Set to u64
    pub set_u64: DynSetU64Fn,
    /// Set to f64 (returns false if value not representable)
    pub set_f64: DynSetF64Fn,
    /// Set to string
    pub set_str: DynSetStrFn,
    /// Set to bytes (optional - not all dynamic value types support bytes)
    pub set_bytes: Option<DynSetBytesFn>,
    /// Set to datetime (optional - not all dynamic value types support datetime)
    pub set_datetime: Option<DynSetDateTimeFn>,

    // --- Array operations (required) ---
    /// Initialize as empty array
    pub begin_array: DynBeginArrayFn,
    /// Push element to array
    pub push_array_element: DynPushArrayElementFn,
    /// Finalize array (optional)
    pub end_array: Option<DynEndArrayFn>,

    // --- Object operations (required) ---
    /// Initialize as empty object
    pub begin_object: DynBeginObjectFn,
    /// Insert key-value pair
    pub insert_object_entry: DynInsertObjectEntryFn,
    /// Finalize object (optional)
    pub end_object: Option<DynEndObjectFn>,

    // --- Read operations (for serialization/Peek) ---
    /// Get the kind of value
    pub get_kind: DynGetKindFn,
    /// Get bool value
    pub get_bool: DynGetBoolFn,
    /// Get i64 value
    pub get_i64: DynGetI64Fn,
    /// Get u64 value
    pub get_u64: DynGetU64Fn,
    /// Get f64 value
    pub get_f64: DynGetF64Fn,
    /// Get string reference
    pub get_str: DynGetStrFn,
    /// Get bytes reference
    pub get_bytes: Option<DynGetBytesFn>,
    /// Get datetime components
    pub get_datetime: Option<DynGetDateTimeFn>,
    /// Get array length
    pub array_len: DynArrayLenFn,
    /// Get array element by index
    pub array_get: DynArrayGetFn,
    /// Get object length
    pub object_len: DynObjectLenFn,
    /// Get object entry by index
    pub object_get_entry: DynObjectGetEntryFn,
    /// Get object value by key
    pub object_get: DynObjectGetFn,
    /// Get mutable reference to object value by key (for navigating into existing entries)
    pub object_get_mut: Option<DynObjectGetMutFn>,
}

impl core::fmt::Debug for DynamicValueVTable {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DynamicValueVTable").finish_non_exhaustive()
    }
}
