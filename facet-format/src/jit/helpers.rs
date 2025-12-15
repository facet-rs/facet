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
    Null = 0,
    Bool = 1,
    I64 = 2,
    U64 = 3,
    F64 = 4,
    String = 5,
}

/// Context passed to JIT-compiled functions.
///
/// Contains function pointers for the specific parser type.
#[repr(C)]
pub struct JitContext<P> {
    /// Pointer to the parser
    pub parser: *mut P,
    /// Cached current event (if any)
    pub current_event: Option<RawEvent>,
}

/// Get the next event from a parser.
///
/// # Safety
/// - `ctx` must be a valid pointer to a `JitContext<P>`
/// - The parser must be in a valid state
pub unsafe extern "C" fn jit_next_event<'de, P: FormatParser<'de>>(
    ctx: *mut JitContext<P>,
    out: *mut RawEvent,
) -> i32 {
    let ctx = unsafe { &mut *ctx };
    let parser = unsafe { &mut *ctx.parser };

    match parser.next_event() {
        Ok(event) => {
            let raw = convert_event_to_raw(event);
            unsafe { *out = raw };
            0 // success
        }
        Err(_e) => {
            unsafe {
                *out = RawEvent {
                    tag: EventTag::Error,
                    payload: EventPayload { error_code: -1 },
                };
            }
            -1 // error
        }
    }
}

/// Skip the current value in the parser.
///
/// # Safety
/// - `ctx` must be a valid pointer to a `JitContext<P>`
pub unsafe extern "C" fn jit_skip_value<'de, P: FormatParser<'de>>(ctx: *mut JitContext<P>) -> i32 {
    let ctx = unsafe { &mut *ctx };
    let parser = unsafe { &mut *ctx.parser };

    match parser.skip_value() {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Convert a ParseEvent to a RawEvent for FFI.
fn convert_event_to_raw(event: ParseEvent<'_>) -> RawEvent {
    match event {
        ParseEvent::StructStart(_) => RawEvent {
            tag: EventTag::StructStart,
            payload: EventPayload { empty: () },
        },
        ParseEvent::StructEnd => RawEvent {
            tag: EventTag::StructEnd,
            payload: EventPayload { empty: () },
        },
        ParseEvent::SequenceStart(_) => RawEvent {
            tag: EventTag::ArrayStart,
            payload: EventPayload { empty: () },
        },
        ParseEvent::SequenceEnd => RawEvent {
            tag: EventTag::ArrayEnd,
            payload: EventPayload { empty: () },
        },
        ParseEvent::VariantTag(_) => RawEvent {
            // Variant tags are handled by the solver, not JIT
            tag: EventTag::Error,
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
                payload: EventPayload {
                    field_name: FieldNamePayload { ptr, len },
                },
            }
        }
        ParseEvent::Scalar(scalar) => {
            let payload = match scalar {
                ScalarValue::Null => EventPayload {
                    scalar: ScalarPayload { is_null: true },
                },
                ScalarValue::Bool(b) => EventPayload {
                    scalar: ScalarPayload { bool_val: b },
                },
                ScalarValue::I64(n) => EventPayload {
                    scalar: ScalarPayload { i64_val: n },
                },
                ScalarValue::U64(n) => EventPayload {
                    scalar: ScalarPayload { u64_val: n },
                },
                ScalarValue::F64(n) => EventPayload {
                    scalar: ScalarPayload { f64_val: n },
                },
                ScalarValue::Str(s) => {
                    let (ptr, len, owned) = match &s {
                        Cow::Borrowed(s) => (s.as_ptr(), s.len(), false),
                        Cow::Owned(s) => (s.as_ptr(), s.len(), true),
                    };
                    EventPayload {
                        scalar: ScalarPayload {
                            string_val: StringPayload { ptr, len, owned },
                        },
                    }
                }
                ScalarValue::Bytes(b) => {
                    let (ptr, len, owned) = match &b {
                        Cow::Borrowed(b) => (b.as_ptr(), b.len(), false),
                        Cow::Owned(b) => (b.as_ptr(), b.len(), true),
                    };
                    EventPayload {
                        scalar: ScalarPayload {
                            string_val: StringPayload { ptr, len, owned },
                        },
                    }
                }
            };
            RawEvent {
                tag: EventTag::Scalar,
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
