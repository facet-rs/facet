//! Helper functions that JIT-compiled code calls back into.
//!
//! These are extern "C" functions that provide a stable ABI for the JIT code
//! to interact with Rust's `FormatParser` trait and handle value writing.

use std::borrow::Cow;

use crate::{FormatParser, ParseEvent, ScalarValue};

/// Raw event representation for FFI.
///
/// This is a simplified representation of `ParseEvent` that can be passed
/// across the FFI boundary.
#[repr(C)]
pub struct RawEvent {
    /// Event type tag
    pub tag: EventTag,
    /// Scalar type tag (only valid when tag == Scalar)
    pub scalar_tag: ScalarTag,
    /// Payload (interpretation depends on tag)
    pub payload: EventPayload,
}

/// Event type tags for FFI
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventTag {
    /// Struct/object start
    StructStart = 0,
    /// Struct/object end
    StructEnd = 1,
    /// Array/sequence start
    ArrayStart = 2,
    /// Array/sequence end
    ArrayEnd = 3,
    /// Field key (payload contains string pointer)
    FieldKey = 4,
    /// Scalar value (payload contains scalar data)
    Scalar = 5,
    /// Error occurred
    Error = 255,
}

/// Event payload union for FFI
#[repr(C)]
pub union EventPayload {
    /// For FieldKey: pointer to field name string
    pub field_name: FieldNamePayload,
    /// For Scalar: the scalar value
    pub scalar: ScalarPayload,
    /// For Error: error code
    pub error_code: i32,
    /// Empty (for StructStart, StructEnd, etc.)
    pub empty: (),
}

/// Field name payload
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FieldNamePayload {
    /// Pointer to UTF-8 string data
    pub ptr: *const u8,
    /// Length in bytes
    pub len: usize,
}

/// Scalar value payload for FFI
#[repr(C)]
#[derive(Clone, Copy)]
pub union ScalarPayload {
    /// Boolean value
    pub bool_val: bool,
    /// i64 value (also used for smaller signed integers)
    pub i64_val: i64,
    /// u64 value (also used for smaller unsigned integers)
    pub u64_val: u64,
    /// f64 value (also used for f32)
    pub f64_val: f64,
    /// String value
    pub string_val: StringPayload,
    /// Null indicator
    pub is_null: bool,
}

/// String payload for FFI
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StringPayload {
    /// Pointer to UTF-8 string data
    pub ptr: *const u8,
    /// Length in bytes
    pub len: usize,
    /// Whether the string is owned (needs to be freed)
    pub owned: bool,
}

/// Scalar type tag
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarTag {
    /// Not a scalar (used for non-scalar events)
    None = 0,
    Null = 1,
    Bool = 2,
    I64 = 3,
    U64 = 4,
    F64 = 5,
    Str = 6,
    Bytes = 7,
}

// =============================================================================
// Error codes
// =============================================================================

/// Success
pub const OK: i32 = 0;
/// Expected struct start
pub const ERR_EXPECTED_STRUCT: i32 = -1;
/// Expected field key or struct end
pub const ERR_EXPECTED_FIELD_OR_END: i32 = -2;
/// Expected scalar value
pub const ERR_EXPECTED_SCALAR: i32 = -3;
/// Parser error
pub const ERR_PARSER: i32 = -4;

// =============================================================================
// Parser VTable (for calling trait methods from JIT code)
// =============================================================================

/// VTable for parser operations.
///
/// This allows JIT code to call parser methods through function pointers,
/// avoiding the need for generic JIT code.
#[repr(C)]
pub struct ParserVTable {
    /// Get the next event: fn(parser: *mut (), out: *mut RawEvent) -> i32
    pub next_event: unsafe extern "C" fn(*mut (), *mut RawEvent) -> i32,
    /// Skip the current value: fn(parser: *mut ()) -> i32
    pub skip_value: unsafe extern "C" fn(*mut ()) -> i32,
}

/// Create a vtable for a specific parser type.
///
/// This is called at monomorphization time to create concrete function pointers.
pub fn make_vtable<'de, P: FormatParser<'de>>() -> ParserVTable {
    ParserVTable {
        next_event: next_event_wrapper::<P>,
        skip_value: skip_value_wrapper::<P>,
    }
}

/// Wrapper for `parser.next_event()` that converts to RawEvent.
unsafe extern "C" fn next_event_wrapper<'de, P: FormatParser<'de>>(
    parser: *mut (),
    out: *mut RawEvent,
) -> i32 {
    let parser = unsafe { &mut *(parser as *mut P) };

    match parser.next_event() {
        Ok(event) => {
            unsafe { *out = convert_event_to_raw(event) };
            OK
        }
        Err(_) => {
            unsafe {
                *out = RawEvent {
                    tag: EventTag::Error,
                    scalar_tag: ScalarTag::None,
                    payload: EventPayload {
                        error_code: ERR_PARSER,
                    },
                };
            }
            ERR_PARSER
        }
    }
}

/// Wrapper for `parser.skip_value()`.
unsafe extern "C" fn skip_value_wrapper<'de, P: FormatParser<'de>>(parser: *mut ()) -> i32 {
    let parser = unsafe { &mut *(parser as *mut P) };

    match parser.skip_value() {
        Ok(()) => OK,
        Err(_) => ERR_PARSER,
    }
}

// =============================================================================
// JIT Context
// =============================================================================

/// Context passed to JIT-compiled functions.
///
/// Contains the parser pointer and vtable for calling parser methods.
#[repr(C)]
pub struct JitContext {
    /// Opaque pointer to the parser
    pub parser: *mut (),
    /// Vtable for parser operations
    pub vtable: *const ParserVTable,
}

/// Convert a ParseEvent to a RawEvent for FFI.
fn convert_event_to_raw(event: ParseEvent<'_>) -> RawEvent {
    match event {
        ParseEvent::StructStart(_) => RawEvent {
            tag: EventTag::StructStart,
            scalar_tag: ScalarTag::None,
            payload: EventPayload { empty: () },
        },
        ParseEvent::StructEnd => RawEvent {
            tag: EventTag::StructEnd,
            scalar_tag: ScalarTag::None,
            payload: EventPayload { empty: () },
        },
        ParseEvent::SequenceStart(_) => RawEvent {
            tag: EventTag::ArrayStart,
            scalar_tag: ScalarTag::None,
            payload: EventPayload { empty: () },
        },
        ParseEvent::SequenceEnd => RawEvent {
            tag: EventTag::ArrayEnd,
            scalar_tag: ScalarTag::None,
            payload: EventPayload { empty: () },
        },
        ParseEvent::VariantTag(_) => RawEvent {
            // Variant tags are handled by the solver, not JIT
            tag: EventTag::Error,
            scalar_tag: ScalarTag::None,
            payload: EventPayload { error_code: -2 },
        },
        ParseEvent::FieldKey(key) => {
            let name = key.name;
            let (ptr, len) = match &name {
                Cow::Borrowed(s) => (s.as_ptr(), s.len()),
                Cow::Owned(s) => (s.as_ptr(), s.len()),
            };
            // Note: We need to be careful about lifetimes here.
            // The string data must outlive the RawEvent.
            RawEvent {
                tag: EventTag::FieldKey,
                scalar_tag: ScalarTag::None,
                payload: EventPayload {
                    field_name: FieldNamePayload { ptr, len },
                },
            }
        }
        ParseEvent::Scalar(scalar) => {
            let (scalar_tag, payload) = match scalar {
                ScalarValue::Null => (
                    ScalarTag::Null,
                    EventPayload {
                        scalar: ScalarPayload { is_null: true },
                    },
                ),
                ScalarValue::Bool(b) => (
                    ScalarTag::Bool,
                    EventPayload {
                        scalar: ScalarPayload { bool_val: b },
                    },
                ),
                ScalarValue::I64(n) => (
                    ScalarTag::I64,
                    EventPayload {
                        scalar: ScalarPayload { i64_val: n },
                    },
                ),
                ScalarValue::U64(n) => (
                    ScalarTag::U64,
                    EventPayload {
                        scalar: ScalarPayload { u64_val: n },
                    },
                ),
                ScalarValue::F64(n) => (
                    ScalarTag::F64,
                    EventPayload {
                        scalar: ScalarPayload { f64_val: n },
                    },
                ),
                ScalarValue::Str(s) => {
                    let (ptr, len, owned) = match &s {
                        Cow::Borrowed(s) => (s.as_ptr(), s.len(), false),
                        Cow::Owned(s) => (s.as_ptr(), s.len(), true),
                    };
                    (
                        ScalarTag::Str,
                        EventPayload {
                            scalar: ScalarPayload {
                                string_val: StringPayload { ptr, len, owned },
                            },
                        },
                    )
                }
                ScalarValue::Bytes(b) => {
                    let (ptr, len, owned) = match &b {
                        Cow::Borrowed(b) => (b.as_ptr(), b.len(), false),
                        Cow::Owned(b) => (b.as_ptr(), b.len(), true),
                    };
                    (
                        ScalarTag::Bytes,
                        EventPayload {
                            scalar: ScalarPayload {
                                string_val: StringPayload { ptr, len, owned },
                            },
                        },
                    )
                }
            };
            RawEvent {
                tag: EventTag::Scalar,
                scalar_tag,
                payload,
            }
        }
    }
}

/// Write a u8 value to a struct field.
///
/// # Safety
/// - `out` must be a valid pointer to the struct
/// - `offset` must be a valid offset within the struct
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_write_u8(out: *mut u8, offset: usize, value: u8) {
    unsafe {
        *out.add(offset) = value;
    }
}

/// Write a u16 value to a struct field.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_write_u16(out: *mut u8, offset: usize, value: u16) {
    unsafe {
        let ptr = out.add(offset) as *mut u16;
        *ptr = value;
    }
}

/// Write a u32 value to a struct field.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_write_u32(out: *mut u8, offset: usize, value: u32) {
    unsafe {
        let ptr = out.add(offset) as *mut u32;
        *ptr = value;
    }
}

/// Write a u64 value to a struct field.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_write_u64(out: *mut u8, offset: usize, value: u64) {
    unsafe {
        let ptr = out.add(offset) as *mut u64;
        *ptr = value;
    }
}

/// Write an i8 value to a struct field.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_write_i8(out: *mut u8, offset: usize, value: i8) {
    unsafe {
        let ptr = out.add(offset) as *mut i8;
        *ptr = value;
    }
}

/// Write an i16 value to a struct field.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_write_i16(out: *mut u8, offset: usize, value: i16) {
    unsafe {
        let ptr = out.add(offset) as *mut i16;
        *ptr = value;
    }
}

/// Write an i32 value to a struct field.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_write_i32(out: *mut u8, offset: usize, value: i32) {
    unsafe {
        let ptr = out.add(offset) as *mut i32;
        *ptr = value;
    }
}

/// Write an i64 value to a struct field.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_write_i64(out: *mut u8, offset: usize, value: i64) {
    unsafe {
        let ptr = out.add(offset) as *mut i64;
        *ptr = value;
    }
}

/// Write an f32 value to a struct field.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_write_f32(out: *mut u8, offset: usize, value: f32) {
    unsafe {
        let ptr = out.add(offset) as *mut f32;
        *ptr = value;
    }
}

/// Write an f64 value to a struct field.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_write_f64(out: *mut u8, offset: usize, value: f64) {
    unsafe {
        let ptr = out.add(offset) as *mut f64;
        *ptr = value;
    }
}

/// Write a bool value to a struct field.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_write_bool(out: *mut u8, offset: usize, value: bool) {
    unsafe {
        *out.add(offset) = value as u8;
    }
}

/// Write a String value to a struct field.
///
/// This takes ownership of the string data if `owned` is true,
/// otherwise it clones from the borrowed data.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_write_string(
    out: *mut u8,
    offset: usize,
    ptr: *const u8,
    len: usize,
    owned: bool,
) {
    let string = if owned {
        // Take ownership - reconstruct the String
        // Safety: The caller guarantees this was allocated as a String
        unsafe { String::from_raw_parts(ptr as *mut u8, len, len) }
    } else {
        // Clone from borrowed data
        let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
        let s = std::str::from_utf8(slice).unwrap_or("");
        s.to_string()
    };

    unsafe {
        let field_ptr = out.add(offset) as *mut String;
        std::ptr::write(field_ptr, string);
    }
}

/// Compare a field name from an event with an expected field name.
///
/// Returns 1 if the names match, 0 otherwise.
///
/// # Safety
/// - `name_ptr` and `expected_ptr` must be valid pointers to UTF-8 data
/// - `name_len` and `expected_len` must be the correct lengths
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_field_matches(
    name_ptr: *const u8,
    name_len: usize,
    expected_ptr: *const u8,
    expected_len: usize,
) -> i32 {
    if name_len != expected_len {
        return 0;
    }
    let name = unsafe { std::slice::from_raw_parts(name_ptr, name_len) };
    let expected = unsafe { std::slice::from_raw_parts(expected_ptr, expected_len) };
    if name == expected { 1 } else { 0 }
}

// =============================================================================
// Layout constants for JIT code generation
// =============================================================================

/// Size of RawEvent in bytes.
pub const RAW_EVENT_SIZE: usize = std::mem::size_of::<RawEvent>();

/// Offset of the `tag` field in RawEvent.
pub const RAW_EVENT_TAG_OFFSET: usize = 0;

/// Offset of the `scalar_tag` field in RawEvent.
pub const RAW_EVENT_SCALAR_TAG_OFFSET: usize = 1;

/// Offset of the `payload` field in RawEvent.
pub const RAW_EVENT_PAYLOAD_OFFSET: usize = std::mem::offset_of!(RawEvent, payload);

/// Offset of `parser` in JitContext.
pub const JIT_CONTEXT_PARSER_OFFSET: usize = std::mem::offset_of!(JitContext, parser);

/// Offset of `vtable` in JitContext.
pub const JIT_CONTEXT_VTABLE_OFFSET: usize = std::mem::offset_of!(JitContext, vtable);

/// Offset of `next_event` in ParserVTable.
pub const VTABLE_NEXT_EVENT_OFFSET: usize = std::mem::offset_of!(ParserVTable, next_event);

/// Offset of `skip_value` in ParserVTable.
pub const VTABLE_SKIP_VALUE_OFFSET: usize = std::mem::offset_of!(ParserVTable, skip_value);

/// Offset of `ptr` in FieldNamePayload.
pub const FIELD_NAME_PTR_OFFSET: usize = std::mem::offset_of!(FieldNamePayload, ptr);

/// Offset of `len` in FieldNamePayload.
pub const FIELD_NAME_LEN_OFFSET: usize = std::mem::offset_of!(FieldNamePayload, len);
