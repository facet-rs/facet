//! Definition for dynamic value types that can hold any value at runtime.
//!
//! This is used for types like `facet_value::Value` or `serde_json::Value`
//! that determine their structure at runtime rather than compile time.

use crate::ptr::{PtrConst, PtrMut, PtrUninit};

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
    /// Returns a builder for DynamicValueDef
    pub const fn builder() -> DynamicValueDefBuilder {
        DynamicValueDefBuilder::new()
    }
}

/// Builder for DynamicValueDef
pub struct DynamicValueDefBuilder {
    vtable: Option<&'static DynamicValueVTable>,
}

impl DynamicValueDefBuilder {
    /// Creates a new DynamicValueDefBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self { vtable: None }
    }

    /// Sets the vtable for the DynamicValueDef
    pub const fn vtable(mut self, vtable: &'static DynamicValueVTable) -> Self {
        self.vtable = Some(vtable);
        self
    }

    /// Builds the DynamicValueDef
    pub const fn build(self) -> DynamicValueDef {
        DynamicValueDef {
            vtable: self.vtable.unwrap(),
        }
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
pub type DynSetNullFn = unsafe fn(dst: PtrUninit<'_>);

/// Set the value to a boolean.
///
/// # Safety
///
/// `dst` must point to uninitialized memory of the correct size and alignment.
/// After this call, `dst` is fully initialized.
pub type DynSetBoolFn = unsafe fn(dst: PtrUninit<'_>, value: bool);

/// Set the value to a signed 64-bit integer.
///
/// # Safety
///
/// `dst` must point to uninitialized memory of the correct size and alignment.
/// After this call, `dst` is fully initialized.
pub type DynSetI64Fn = unsafe fn(dst: PtrUninit<'_>, value: i64);

/// Set the value to an unsigned 64-bit integer.
///
/// # Safety
///
/// `dst` must point to uninitialized memory of the correct size and alignment.
/// After this call, `dst` is fully initialized.
pub type DynSetU64Fn = unsafe fn(dst: PtrUninit<'_>, value: u64);

/// Set the value to a 64-bit float.
///
/// # Safety
///
/// `dst` must point to uninitialized memory of the correct size and alignment.
/// After this call, `dst` is fully initialized.
/// Returns `false` if the value is not representable (e.g., NaN/Infinity when not supported).
pub type DynSetF64Fn = unsafe fn(dst: PtrUninit<'_>, value: f64) -> bool;

/// Set the value to a string.
///
/// # Safety
///
/// `dst` must point to uninitialized memory of the correct size and alignment.
/// After this call, `dst` is fully initialized.
pub type DynSetStrFn = unsafe fn(dst: PtrUninit<'_>, value: &str);

/// Set the value to bytes.
///
/// # Safety
///
/// `dst` must point to uninitialized memory of the correct size and alignment.
/// After this call, `dst` is fully initialized.
pub type DynSetBytesFn = unsafe fn(dst: PtrUninit<'_>, value: &[u8]);

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
    dst: PtrUninit<'_>,
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
    unsafe fn(value: PtrConst<'_>) -> Option<(i32, u8, u8, u8, u8, u8, u32, DynDateTimeKind)>;

// ============================================================================
// Array operations
// ============================================================================

/// Initialize the value as an empty array.
///
/// # Safety
///
/// `dst` must point to uninitialized memory of the correct size and alignment.
/// After this call, `dst` is initialized as an empty array.
pub type DynBeginArrayFn = unsafe fn(dst: PtrUninit<'_>);

/// Push an element to the array.
///
/// The element is moved out of `element` (not dropped, but the memory can be deallocated).
///
/// # Safety
///
/// `array` must point to an initialized dynamic value that is an array.
/// `element` must point to an initialized dynamic value to push.
pub type DynPushArrayElementFn = unsafe fn(array: PtrMut<'_>, element: PtrMut<'_>);

/// Finalize the array (optional, for shrinking capacity etc.).
///
/// # Safety
///
/// `array` must point to an initialized dynamic value that is an array.
pub type DynEndArrayFn = unsafe fn(array: PtrMut<'_>);

// ============================================================================
// Object operations
// ============================================================================

/// Initialize the value as an empty object.
///
/// # Safety
///
/// `dst` must point to uninitialized memory of the correct size and alignment.
/// After this call, `dst` is initialized as an empty object.
pub type DynBeginObjectFn = unsafe fn(dst: PtrUninit<'_>);

/// Insert a key-value pair into the object.
///
/// The value is moved out of `value` (not dropped, but the memory can be deallocated).
///
/// # Safety
///
/// `object` must point to an initialized dynamic value that is an object.
/// `value` must point to an initialized dynamic value to insert.
pub type DynInsertObjectEntryFn = unsafe fn(object: PtrMut<'_>, key: &str, value: PtrMut<'_>);

/// Finalize the object (optional, for shrinking capacity etc.).
///
/// # Safety
///
/// `object` must point to an initialized dynamic value that is an object.
pub type DynEndObjectFn = unsafe fn(object: PtrMut<'_>);

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
pub type DynGetKindFn = unsafe fn(value: PtrConst<'_>) -> DynValueKind;

/// Get a boolean value. Returns None if not a bool.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
pub type DynGetBoolFn = unsafe fn(value: PtrConst<'_>) -> Option<bool>;

/// Get a signed 64-bit integer value. Returns None if not representable as i64.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
pub type DynGetI64Fn = unsafe fn(value: PtrConst<'_>) -> Option<i64>;

/// Get an unsigned 64-bit integer value. Returns None if not representable as u64.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
pub type DynGetU64Fn = unsafe fn(value: PtrConst<'_>) -> Option<u64>;

/// Get a 64-bit float value. Returns None if not a number.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
pub type DynGetF64Fn = unsafe fn(value: PtrConst<'_>) -> Option<f64>;

/// Get a string reference. Returns None if not a string.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
/// The returned reference is valid for the lifetime of the value.
pub type DynGetStrFn = for<'a> unsafe fn(value: PtrConst<'a>) -> Option<&'a str>;

/// Get a bytes reference. Returns None if not bytes.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
/// The returned reference is valid for the lifetime of the value.
pub type DynGetBytesFn = for<'a> unsafe fn(value: PtrConst<'a>) -> Option<&'a [u8]>;

/// Get the length of an array. Returns None if not an array.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
pub type DynArrayLenFn = unsafe fn(value: PtrConst<'_>) -> Option<usize>;

/// Get an element from an array by index. Returns None if not an array or index out of bounds.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
pub type DynArrayGetFn = unsafe fn(value: PtrConst<'_>, index: usize) -> Option<PtrConst<'_>>;

/// Get the length of an object. Returns None if not an object.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
pub type DynObjectLenFn = unsafe fn(value: PtrConst<'_>) -> Option<usize>;

/// Get a key-value pair from an object by index. Returns None if not an object or index out of bounds.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
pub type DynObjectGetEntryFn =
    for<'a> unsafe fn(value: PtrConst<'a>, index: usize) -> Option<(&'a str, PtrConst<'a>)>;

/// Get a value from an object by key. Returns None if not an object or key not found.
///
/// # Safety
///
/// `value` must point to an initialized dynamic value.
pub type DynObjectGetFn = for<'a> unsafe fn(value: PtrConst<'a>, key: &str) -> Option<PtrConst<'a>>;

/// Get a mutable reference to a value from an object by key.
/// Returns None if not an object or key not found.
///
/// This is used for navigating into existing object entries during deserialization
/// (e.g., TOML implicit tables like `[a]` followed by `[a.b.c]`).
///
/// # Safety
///
/// `value` must point to an initialized dynamic value that is an object.
pub type DynObjectGetMutFn = for<'a> unsafe fn(value: PtrMut<'a>, key: &str) -> Option<PtrMut<'a>>;

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

impl DynamicValueVTable {
    /// Returns a builder for DynamicValueVTable
    pub const fn builder() -> DynamicValueVTableBuilder {
        DynamicValueVTableBuilder::new()
    }
}

/// Builder for DynamicValueVTable
pub struct DynamicValueVTableBuilder {
    set_null: Option<DynSetNullFn>,
    set_bool: Option<DynSetBoolFn>,
    set_i64: Option<DynSetI64Fn>,
    set_u64: Option<DynSetU64Fn>,
    set_f64: Option<DynSetF64Fn>,
    set_str: Option<DynSetStrFn>,
    set_bytes: Option<DynSetBytesFn>,
    set_datetime: Option<DynSetDateTimeFn>,
    begin_array: Option<DynBeginArrayFn>,
    push_array_element: Option<DynPushArrayElementFn>,
    end_array: Option<DynEndArrayFn>,
    begin_object: Option<DynBeginObjectFn>,
    insert_object_entry: Option<DynInsertObjectEntryFn>,
    end_object: Option<DynEndObjectFn>,
    get_kind: Option<DynGetKindFn>,
    get_bool: Option<DynGetBoolFn>,
    get_i64: Option<DynGetI64Fn>,
    get_u64: Option<DynGetU64Fn>,
    get_f64: Option<DynGetF64Fn>,
    get_str: Option<DynGetStrFn>,
    get_bytes: Option<DynGetBytesFn>,
    get_datetime: Option<DynGetDateTimeFn>,
    array_len: Option<DynArrayLenFn>,
    array_get: Option<DynArrayGetFn>,
    object_len: Option<DynObjectLenFn>,
    object_get_entry: Option<DynObjectGetEntryFn>,
    object_get: Option<DynObjectGetFn>,
    object_get_mut: Option<DynObjectGetMutFn>,
}

impl DynamicValueVTableBuilder {
    /// Creates a new DynamicValueVTableBuilder with all fields set to None
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            set_null: None,
            set_bool: None,
            set_i64: None,
            set_u64: None,
            set_f64: None,
            set_str: None,
            set_bytes: None,
            set_datetime: None,
            begin_array: None,
            push_array_element: None,
            end_array: None,
            begin_object: None,
            insert_object_entry: None,
            end_object: None,
            get_kind: None,
            get_bool: None,
            get_i64: None,
            get_u64: None,
            get_f64: None,
            get_str: None,
            get_bytes: None,
            get_datetime: None,
            array_len: None,
            array_get: None,
            object_len: None,
            object_get_entry: None,
            object_get: None,
            object_get_mut: None,
        }
    }

    /// Sets the set_null function
    pub const fn set_null(mut self, f: DynSetNullFn) -> Self {
        self.set_null = Some(f);
        self
    }

    /// Sets the set_bool function
    pub const fn set_bool(mut self, f: DynSetBoolFn) -> Self {
        self.set_bool = Some(f);
        self
    }

    /// Sets the set_i64 function
    pub const fn set_i64(mut self, f: DynSetI64Fn) -> Self {
        self.set_i64 = Some(f);
        self
    }

    /// Sets the set_u64 function
    pub const fn set_u64(mut self, f: DynSetU64Fn) -> Self {
        self.set_u64 = Some(f);
        self
    }

    /// Sets the set_f64 function
    pub const fn set_f64(mut self, f: DynSetF64Fn) -> Self {
        self.set_f64 = Some(f);
        self
    }

    /// Sets the set_str function
    pub const fn set_str(mut self, f: DynSetStrFn) -> Self {
        self.set_str = Some(f);
        self
    }

    /// Sets the set_bytes function
    pub const fn set_bytes(mut self, f: DynSetBytesFn) -> Self {
        self.set_bytes = Some(f);
        self
    }

    /// Sets the set_datetime function
    pub const fn set_datetime(mut self, f: DynSetDateTimeFn) -> Self {
        self.set_datetime = Some(f);
        self
    }

    /// Sets the begin_array function
    pub const fn begin_array(mut self, f: DynBeginArrayFn) -> Self {
        self.begin_array = Some(f);
        self
    }

    /// Sets the push_array_element function
    pub const fn push_array_element(mut self, f: DynPushArrayElementFn) -> Self {
        self.push_array_element = Some(f);
        self
    }

    /// Sets the end_array function
    pub const fn end_array(mut self, f: DynEndArrayFn) -> Self {
        self.end_array = Some(f);
        self
    }

    /// Sets the begin_object function
    pub const fn begin_object(mut self, f: DynBeginObjectFn) -> Self {
        self.begin_object = Some(f);
        self
    }

    /// Sets the insert_object_entry function
    pub const fn insert_object_entry(mut self, f: DynInsertObjectEntryFn) -> Self {
        self.insert_object_entry = Some(f);
        self
    }

    /// Sets the end_object function
    pub const fn end_object(mut self, f: DynEndObjectFn) -> Self {
        self.end_object = Some(f);
        self
    }

    /// Sets the get_kind function
    pub const fn get_kind(mut self, f: DynGetKindFn) -> Self {
        self.get_kind = Some(f);
        self
    }

    /// Sets the get_bool function
    pub const fn get_bool(mut self, f: DynGetBoolFn) -> Self {
        self.get_bool = Some(f);
        self
    }

    /// Sets the get_i64 function
    pub const fn get_i64(mut self, f: DynGetI64Fn) -> Self {
        self.get_i64 = Some(f);
        self
    }

    /// Sets the get_u64 function
    pub const fn get_u64(mut self, f: DynGetU64Fn) -> Self {
        self.get_u64 = Some(f);
        self
    }

    /// Sets the get_f64 function
    pub const fn get_f64(mut self, f: DynGetF64Fn) -> Self {
        self.get_f64 = Some(f);
        self
    }

    /// Sets the get_str function
    pub const fn get_str(mut self, f: DynGetStrFn) -> Self {
        self.get_str = Some(f);
        self
    }

    /// Sets the get_bytes function
    pub const fn get_bytes(mut self, f: DynGetBytesFn) -> Self {
        self.get_bytes = Some(f);
        self
    }

    /// Sets the get_datetime function
    pub const fn get_datetime(mut self, f: DynGetDateTimeFn) -> Self {
        self.get_datetime = Some(f);
        self
    }

    /// Sets the array_len function
    pub const fn array_len(mut self, f: DynArrayLenFn) -> Self {
        self.array_len = Some(f);
        self
    }

    /// Sets the array_get function
    pub const fn array_get(mut self, f: DynArrayGetFn) -> Self {
        self.array_get = Some(f);
        self
    }

    /// Sets the object_len function
    pub const fn object_len(mut self, f: DynObjectLenFn) -> Self {
        self.object_len = Some(f);
        self
    }

    /// Sets the object_get_entry function
    pub const fn object_get_entry(mut self, f: DynObjectGetEntryFn) -> Self {
        self.object_get_entry = Some(f);
        self
    }

    /// Sets the object_get function
    pub const fn object_get(mut self, f: DynObjectGetFn) -> Self {
        self.object_get = Some(f);
        self
    }

    /// Sets the object_get_mut function
    pub const fn object_get_mut(mut self, f: DynObjectGetMutFn) -> Self {
        self.object_get_mut = Some(f);
        self
    }

    /// Builds the DynamicValueVTable
    ///
    /// # Panics
    ///
    /// Panics if required fields are not set.
    pub const fn build(self) -> DynamicValueVTable {
        DynamicValueVTable {
            set_null: self.set_null.unwrap(),
            set_bool: self.set_bool.unwrap(),
            set_i64: self.set_i64.unwrap(),
            set_u64: self.set_u64.unwrap(),
            set_f64: self.set_f64.unwrap(),
            set_str: self.set_str.unwrap(),
            set_bytes: self.set_bytes,
            set_datetime: self.set_datetime,
            begin_array: self.begin_array.unwrap(),
            push_array_element: self.push_array_element.unwrap(),
            end_array: self.end_array,
            begin_object: self.begin_object.unwrap(),
            insert_object_entry: self.insert_object_entry.unwrap(),
            end_object: self.end_object,
            get_kind: self.get_kind.unwrap(),
            get_bool: self.get_bool.unwrap(),
            get_i64: self.get_i64.unwrap(),
            get_u64: self.get_u64.unwrap(),
            get_f64: self.get_f64.unwrap(),
            get_str: self.get_str.unwrap(),
            get_bytes: self.get_bytes,
            get_datetime: self.get_datetime,
            array_len: self.array_len.unwrap(),
            array_get: self.array_get.unwrap(),
            object_len: self.object_len.unwrap(),
            object_get_entry: self.object_get_entry.unwrap(),
            object_get: self.object_get.unwrap(),
            object_get_mut: self.object_get_mut,
        }
    }
}
