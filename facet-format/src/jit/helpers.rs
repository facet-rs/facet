//! Helper functions that JIT-compiled code calls back into.
//!
//! These are extern "C" functions that provide a stable ABI for the JIT code
//! to interact with Rust's `FormatParser` trait and handle value writing.

#![allow(clippy::missing_safety_doc)] // Safety docs are in function comments

use std::borrow::Cow;
use std::cell::RefCell;

use crate::{FormatParser, ParseEvent, ScalarValue};
use facet_core::Shape;

// Thread-local storage for owned field names that need to be freed.
// We keep owned field names alive until the next event is processed.
thread_local! {
    static PENDING_FIELD_NAME: RefCell<Option<(*mut u8, usize, usize)>> = const { RefCell::new(None) };
}

/// Raw event representation for FFI.
///
/// This is a simplified representation of `ParseEvent` that can be passed
/// across the FFI boundary.
#[repr(C)]
#[derive(Copy, Clone)]
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
#[derive(Copy, Clone)]
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
    /// Capacity in bytes (only valid if owned)
    pub capacity: usize,
    /// Whether the string is owned (needs to be freed)
    pub owned: bool,
}

/// Decompose a String into raw parts for FFI transfer.
/// This is equivalent to the nightly-only `String::into_raw_parts()`.
fn string_into_raw_parts(s: String) -> (*mut u8, usize, usize) {
    let mut s = std::mem::ManuallyDrop::new(s);
    (s.as_mut_ptr(), s.len(), s.capacity())
}

/// Decompose a `Vec<u8>` into raw parts for FFI transfer.
fn vec_into_raw_parts(v: Vec<u8>) -> (*mut u8, usize, usize) {
    let mut v = std::mem::ManuallyDrop::new(v);
    (v.as_mut_ptr(), v.len(), v.capacity())
}

/// Scalar type tag
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarTag {
    /// Not a scalar (used for non-scalar events)
    None = 0,
    /// Null value
    Null = 1,
    /// Boolean value
    Bool = 2,
    /// Signed 64-bit integer
    I64 = 3,
    /// Unsigned 64-bit integer
    U64 = 4,
    /// 64-bit floating point
    F64 = 5,
    /// String value
    Str = 6,
    /// Binary data
    Bytes = 7,
}

impl ScalarTag {
    /// Convert from u8 value
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => ScalarTag::None,
            1 => ScalarTag::Null,
            2 => ScalarTag::Bool,
            3 => ScalarTag::I64,
            4 => ScalarTag::U64,
            5 => ScalarTag::F64,
            6 => ScalarTag::Str,
            7 => ScalarTag::Bytes,
            _ => ScalarTag::None,
        }
    }
}

// =============================================================================
// Error codes
// =============================================================================

/// Success
pub const OK: i32 = 0;
/// Expected struct start
#[allow(dead_code)]
pub const ERR_EXPECTED_STRUCT: i32 = -1;
/// Expected field key or struct end
#[allow(dead_code)]
pub const ERR_EXPECTED_FIELD_OR_END: i32 = -2;
/// Expected scalar value
#[allow(dead_code)]
pub const ERR_EXPECTED_SCALAR: i32 = -3;
/// Parser error
pub const ERR_PARSER: i32 = -4;

// List deserialization error codes (-10x range)
/// Expected array start
pub const ERR_EXPECTED_ARRAY: i32 = -10;

// Struct deserialization error codes (-30x range)
/// Missing required field (non-Option field not present in input)
pub const ERR_MISSING_REQUIRED_FIELD: i32 = -300;

// List deserialization error codes (-20x range)
/// Not a list type (shape.def is not Def::List)
pub const ERR_LIST_NOT_LIST_TYPE: i32 = -200;
/// No init_in_place_with_capacity function
pub const ERR_LIST_NO_INIT_FN: i32 = -201;
/// No push function
pub const ERR_LIST_NO_PUSH_FN: i32 = -202;
/// Unsupported scalar type in list element
pub const ERR_LIST_UNSUPPORTED_SCALAR: i32 = -203;
/// Unsupported element type (not scalar, not list, not struct)
pub const ERR_LIST_UNSUPPORTED_ELEMENT: i32 = -204;
/// Element type is unsized
pub const ERR_LIST_UNSIZED_ELEMENT: i32 = -205;

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

/// Peek at the next event without consuming it (uses JitContext buffer).
///
/// # Safety
/// - `ctx` must be a valid JitContext pointer
/// - `out` must be a valid pointer to a RawEvent to write the peeked event
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_peek_event(ctx: *mut JitContext, out: *mut RawEvent) -> i32 {
    let ctx = unsafe { &mut *ctx };

    // If we already have a peeked event, return it
    if let Some(peeked) = ctx.peeked_event {
        unsafe { *out = peeked };
        return OK;
    }

    // Otherwise, call next_event and buffer it
    let result = unsafe { jit_next_event(ctx, out) };
    if result == OK {
        ctx.peeked_event = Some(unsafe { *out });
    }
    result
}

/// Get the next event, either from buffer or by calling parser.
///
/// # Safety
/// - `ctx` must be a valid JitContext pointer
/// - `out` must be a valid pointer to a RawEvent
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_next_event(ctx: *mut JitContext, out: *mut RawEvent) -> i32 {
    let ctx_ref = unsafe { &mut *ctx };

    // If we have a peeked event, return it and clear the buffer
    if let Some(peeked) = ctx_ref.peeked_event.take() {
        unsafe { *out = peeked };
        return OK;
    }

    // Call the vtable's next_event function
    let vtable = unsafe { &*ctx_ref.vtable };
    let next_event_fn = vtable.next_event;
    unsafe { next_event_fn(ctx_ref.parser, out) }
}

/// Wrapper for `parser.next_event()` that converts to RawEvent.
unsafe extern "C" fn next_event_wrapper<'de, P: FormatParser<'de>>(
    parser: *mut (),
    out: *mut RawEvent,
) -> i32 {
    // Free the previous owned field name if any.
    // By the time we're processing a new event, the JIT code is done with the previous one.
    PENDING_FIELD_NAME.with(|cell| {
        if let Some((ptr, len, cap)) = cell.borrow_mut().take() {
            unsafe {
                // Reconstruct and drop the String to free it
                let _ = String::from_raw_parts(ptr, len, cap);
            }
        }
    });

    let parser = unsafe { &mut *(parser as *mut P) };

    match parser.next_event() {
        Ok(event) => {
            let raw = convert_event_to_raw(event);
            #[cfg(debug_assertions)]
            {
                if raw.tag == EventTag::Scalar && raw.scalar_tag == ScalarTag::I64 {
                    eprintln!(
                        "[JIT] next_event: Scalar(I64({})) -> writing to {:p}",
                        unsafe { raw.payload.scalar.i64_val },
                        out
                    );
                } else if raw.tag == EventTag::Scalar && raw.scalar_tag == ScalarTag::Str {
                    let payload = unsafe { raw.payload.scalar.string_val };
                    let s = unsafe {
                        std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                            payload.ptr,
                            payload.len,
                        ))
                    };
                    eprintln!(
                        "[JIT] next_event: Scalar(Str(\"{}\")) -> writing to {:p}",
                        s, out
                    );
                } else {
                    eprintln!("[JIT] next_event: tag={:?}", raw.tag);
                }
            }
            unsafe { *out = raw };
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
    /// Peeked event buffer (for implementing peek without vtable changes)
    pub peeked_event: Option<RawEvent>,
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
            let (ptr, len) = match name {
                Cow::Borrowed(s) => (s.as_ptr(), s.len()),
                Cow::Owned(s) => {
                    // Use into_raw_parts to prevent the string from being dropped.
                    // We store the raw parts in thread-local storage and free them
                    // on the next call to next_event_wrapper.
                    let (ptr, len, cap) = string_into_raw_parts(s);
                    PENDING_FIELD_NAME.with(|cell| {
                        *cell.borrow_mut() = Some((ptr, len, cap));
                    });
                    (ptr as *const u8, len)
                }
            };
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
                    let (ptr, len, capacity, owned) = match s {
                        Cow::Borrowed(s) => (s.as_ptr(), s.len(), 0, false),
                        Cow::Owned(s) => {
                            let (ptr, len, cap) = string_into_raw_parts(s);
                            (ptr as *const u8, len, cap, true)
                        }
                    };
                    (
                        ScalarTag::Str,
                        EventPayload {
                            scalar: ScalarPayload {
                                string_val: StringPayload {
                                    ptr,
                                    len,
                                    capacity,
                                    owned,
                                },
                            },
                        },
                    )
                }
                ScalarValue::Bytes(b) => {
                    let (ptr, len, capacity, owned) = match b {
                        Cow::Borrowed(b) => (b.as_ptr(), b.len(), 0, false),
                        Cow::Owned(b) => {
                            let (ptr, len, cap) = vec_into_raw_parts(b);
                            (ptr as *const u8, len, cap, true)
                        }
                    };
                    (
                        ScalarTag::Bytes,
                        EventPayload {
                            scalar: ScalarPayload {
                                string_val: StringPayload {
                                    ptr,
                                    len,
                                    capacity,
                                    owned,
                                },
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
    #[cfg(debug_assertions)]
    eprintln!("[JIT] write_u64: value={} to {:p}+{}", value, out, offset);
    unsafe {
        let ptr = out.add(offset) as *mut u64;
        std::ptr::write_unaligned(ptr, value);
    }
}

/// Write an i8 value to a struct field.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_write_i8(out: *mut u8, offset: usize, value: i8) {
    unsafe {
        let ptr = out.add(offset) as *mut i8;
        std::ptr::write_unaligned(ptr, value);
    }
}

/// Write an i16 value to a struct field.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_write_i16(out: *mut u8, offset: usize, value: i16) {
    unsafe {
        let ptr = out.add(offset) as *mut i16;
        std::ptr::write_unaligned(ptr, value);
    }
}

/// Write an i32 value to a struct field.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_write_i32(out: *mut u8, offset: usize, value: i32) {
    unsafe {
        let ptr = out.add(offset) as *mut i32;
        std::ptr::write_unaligned(ptr, value);
    }
}

/// Write an i64 value to a struct field.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_write_i64(out: *mut u8, offset: usize, value: i64) {
    unsafe {
        let ptr = out.add(offset) as *mut i64;
        std::ptr::write_unaligned(ptr, value);
    }
}

/// Write an f32 value to a struct field.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_write_f32(out: *mut u8, offset: usize, value: f32) {
    unsafe {
        let ptr = out.add(offset) as *mut f32;
        std::ptr::write_unaligned(ptr, value);
    }
}

/// Write an f64 value to a struct field.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_write_f64(out: *mut u8, offset: usize, value: f64) {
    unsafe {
        let ptr = out.add(offset) as *mut f64;
        std::ptr::write_unaligned(ptr, value);
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
    capacity: usize,
    owned: bool,
) {
    #[cfg(debug_assertions)]
    {
        let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
        let s = std::str::from_utf8(slice).unwrap_or("<invalid utf8>");
        let target = (out as usize + offset) as *const u8;
        let addr = target as usize;

        // Safely truncate the string preview at a char boundary
        let preview: String = s.chars().take(50).collect();
        eprintln!(
            "[JIT] jit_write_string: out={:p}, offset={}, len={}, owned={}, cap={}, string=\"{}\"",
            out, offset, len, owned, capacity, preview
        );
        eprintln!("  [JIT]   -> src_ptr={:p}, target={:p}", ptr, target);

        // Check if source pointer looks valid
        if ptr.is_null() {
            eprintln!("  [JIT]   -> ERROR: Source pointer is NULL!");
        } else if (ptr as usize) < 0x100000000 {
            eprintln!(
                "  [JIT]   -> WARNING: Source pointer 0x{:x} looks suspicious!",
                ptr as usize
            );
        }

        // For owned strings, validate the allocation looks sane
        if owned {
            if capacity < len {
                eprintln!(
                    "  [JIT]   -> ERROR: capacity ({}) < len ({})!",
                    capacity, len
                );
            }
            if capacity == 0 && len > 0 {
                eprintln!("  [JIT]   -> ERROR: capacity is 0 but len is {}!", len);
            }
        }
    }

    let string = if owned {
        // Take ownership - reconstruct the String
        // Safety: The caller guarantees this was allocated as a String via string_into_raw_parts
        #[cfg(debug_assertions)]
        eprintln!("  [JIT]   -> taking ownership via from_raw_parts");
        unsafe { String::from_raw_parts(ptr as *mut u8, len, capacity) }
    } else {
        // Clone from borrowed data
        #[cfg(debug_assertions)]
        {
            eprintln!("  [JIT]   -> cloning borrowed data");
            // Validate the pointer before dereferencing
            if (ptr as usize) > 0x7000000000 {
                eprintln!(
                    "  [JIT]   -> ERROR: src_ptr {:p} is suspiciously high!",
                    ptr
                );
                eprintln!("  [JIT]   -> This suggests memory corruption or use-after-free");
            }
        }
        let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
        let s = std::str::from_utf8(slice).unwrap_or("");
        s.to_string()
    };

    #[cfg(debug_assertions)]
    {
        // Log the newly created/cloned string's internal pointers
        let new_ptr = string.as_ptr();
        let new_len = string.len();
        let new_cap = string.capacity();
        eprintln!(
            "  [JIT]   -> new String: ptr={:p}, len={}, cap={}",
            new_ptr, new_len, new_cap
        );
    }

    unsafe {
        let field_ptr = out.add(offset) as *mut String;

        #[cfg(debug_assertions)]
        eprintln!("  [JIT]   -> writing to {:p}", field_ptr);

        std::ptr::write(field_ptr, string);

        #[cfg(debug_assertions)]
        eprintln!("  [JIT]   -> write complete");
    }
}

/// Copy memory from src to dest.
///
/// # Safety
/// - `dest` and `src` must be valid pointers
/// - `len` bytes must be readable from src and writable to dest
/// - memory regions may overlap (uses memmove semantics)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_memcpy(dest: *mut u8, src: *const u8, len: usize) {
    unsafe {
        std::ptr::copy(src, dest, len);
    }
}

/// Write an error message to the scratch buffer.
///
/// For JIT-generated error messages (like duplicate variant keys).
/// Writes the error as a TypeMismatch variant with the message.
///
/// # Safety
/// - `scratch` must be a valid pointer to a `DeserializeError<E>` buffer
/// - `msg_ptr` must be valid for `msg_len` bytes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_write_error_string(
    scratch: *mut u8,
    msg_ptr: *const u8,
    msg_len: usize,
) {
    use crate::DeserializeError;
    use facet_reflect::ReflectError;

    let msg_slice = unsafe { std::slice::from_raw_parts(msg_ptr, msg_len) };
    let msg_str = std::str::from_utf8(msg_slice).unwrap_or("invalid utf8 in error message");

    // Create a reflection error with the message
    // This works for any `DeserializeError<E>` since Reflect variant doesn't depend on E
    // Using InvariantViolation since duplicate variant keys are an invariant violation
    let error: DeserializeError<()> =
        DeserializeError::Reflect(ReflectError::InvariantViolation { invariant: msg_str });

    unsafe {
        // Transmute to write as `DeserializeError<E>` where E is the actual parser error type
        // This is safe because we're using the Reflect variant which doesn't reference E
        let scratch_typed = scratch as *mut DeserializeError<()>;
        std::ptr::write(scratch_typed, error);
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
    #[cfg(debug_assertions)]
    {
        let name = unsafe { std::slice::from_raw_parts(name_ptr, name_len) };
        let expected = unsafe { std::slice::from_raw_parts(expected_ptr, expected_len) };
        let name_str = std::str::from_utf8(name).unwrap_or("<invalid>");
        let expected_str = std::str::from_utf8(expected).unwrap_or("<invalid>");
        let matches = if name_len == expected_len && name == expected {
            1
        } else {
            0
        };
        eprintln!(
            "[JIT] field_matches: '{}' == '{}' ? {}",
            name_str, expected_str, matches
        );
        return matches;
    }

    #[cfg(not(debug_assertions))]
    {
        if name_len != expected_len {
            return 0;
        }
        let name = unsafe { std::slice::from_raw_parts(name_ptr, name_len) };
        let expected = unsafe { std::slice::from_raw_parts(expected_ptr, expected_len) };
        if name == expected { 1 } else { 0 }
    }
}

/// Call a nested struct deserializer function.
///
/// # Safety
/// - `ctx` must be a valid JitContext pointer
/// - `out` must be a valid pointer to uninitialized memory for the nested struct
/// - `func_ptr` must be a valid compiled deserializer function pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_deserialize_nested(
    ctx: *mut JitContext,
    out: *mut u8,
    func_ptr: *const u8,
) -> i32 {
    #[cfg(debug_assertions)]
    {
        eprintln!(
            "[JIT] jit_deserialize_nested: out={:p}, func_ptr={:p}",
            out, func_ptr
        );

        // Validate function pointer looks reasonable
        // On ARM64 macOS, code typically starts at high addresses
        if func_ptr.is_null() {
            eprintln!("  [JIT]   -> ERROR: func_ptr is NULL!");
            panic!("Nested deserializer function pointer is NULL");
        }

        // Check if pointer looks like it could be code
        let addr = func_ptr as usize;
        if addr < 0x100000000 {
            eprintln!(
                "  [JIT]   -> WARNING: func_ptr looks suspicious (too low): {:#x}",
                addr
            );
        }
    }

    // Cast the function pointer to the correct type
    // Signature: fn(ctx: *mut JitContext, out: *mut T) -> i32
    type NestedFn = unsafe extern "C" fn(*mut JitContext, *mut u8) -> i32;
    let func: NestedFn = unsafe { std::mem::transmute(func_ptr) };

    // Call the nested deserializer
    #[cfg(debug_assertions)]
    eprintln!("  [JIT]   -> calling nested deserializer at {:p}", func_ptr);

    let result = unsafe { func(ctx, out) };

    #[cfg(debug_assertions)]
    eprintln!("  [JIT]   -> nested deserializer returned: {}", result);

    result
}

/// Initialize an Option field to None.
///
/// # Safety
/// - `out` must be a valid pointer to uninitialized memory for the Option
/// - `init_none_fn` must be a valid OptionInitNoneFn from the Option's vtable
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_option_init_none(out: *mut u8, init_none_fn: *const u8) {
    type InitNoneFn = unsafe extern "C" fn(*mut u8) -> *mut u8;
    let func: InitNoneFn = unsafe { std::mem::transmute(init_none_fn) };
    unsafe { func(out) };
}

/// Initialize an Option field to Some(value) where value is in a stack buffer.
///
/// # Safety
/// - `out` must be a valid pointer to uninitialized Option memory
/// - `value_ptr` must be a valid pointer to the inner value
/// - `init_some_fn` must be a valid OptionInitSomeFn from the Option's vtable
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_option_init_some_from_value(
    out: *mut u8,
    value_ptr: *const u8,
    init_some_fn: *const u8,
) {
    // Call init_some(option, value_ptr)
    // Signature: fn(option: PtrUninit, value: PtrConst) -> PtrMut
    use facet_core::{PtrConst, PtrUninit};
    type InitSomeFn = unsafe extern "C" fn(PtrUninit, PtrConst) -> facet_core::PtrMut;
    let init_some: InitSomeFn = unsafe { std::mem::transmute(init_some_fn) };
    unsafe { init_some(PtrUninit::new(out), PtrConst::new(value_ptr)) };
}

/// Initialize a Vec field with the given capacity.
///
/// # Safety
/// - `out` must be a valid pointer to uninitialized memory for the Vec
/// - `init_fn` must be a valid ListInitInPlaceWithCapacityFn from the Vec's vtable
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_vec_init_with_capacity(
    out: *mut u8,
    capacity: usize,
    init_fn: *const u8,
) {
    use facet_core::{PtrMut, PtrUninit};
    // ListInitInPlaceWithCapacityFn = unsafe fn(list: PtrUninit, capacity: usize) -> PtrMut
    type InitFn = unsafe fn(PtrUninit, usize) -> PtrMut;
    let func: InitFn = unsafe { std::mem::transmute(init_fn) };
    unsafe { func(PtrUninit::new(out), capacity) };
}

/// Initialize a Map field with the given capacity.
///
/// # Safety
/// - `out` must be a valid pointer to uninitialized memory for the Map
/// - `init_fn` must be a valid MapInitInPlaceWithCapacityFn from the Map's vtable
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_map_init_with_capacity(
    out: *mut u8,
    capacity: usize,
    init_fn: *const u8,
) {
    use facet_core::{PtrMut, PtrUninit};
    // MapInitInPlaceWithCapacityFn = unsafe fn(map: PtrUninit, capacity: usize) -> PtrMut
    type InitFn = unsafe fn(PtrUninit, usize) -> PtrMut;
    let func: InitFn = unsafe { std::mem::transmute(init_fn) };
    unsafe { func(PtrUninit::new(out), capacity) };
}

/// Drop a value in place using the Shape's drop_in_place vtable function.
///
/// This helper is called by JIT-compiled code to properly drop old values before
/// overwriting them with new values (e.g., when duplicate JSON keys appear).
///
/// # Safety
/// - `shape_ptr` must be a valid pointer to a Shape
/// - `ptr` must be a valid pointer to an initialized value of the type described by the shape
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_drop_in_place(shape_ptr: *const u8, ptr: *mut u8) {
    use facet_core::PtrMut;
    let shape: &Shape = unsafe { &*(shape_ptr as *const Shape) };
    unsafe { shape.call_drop_in_place(PtrMut::new(ptr)) };
}

/// Push an item to a Vec by deserializing it.
///
/// # Safety
/// - `ctx` must be a valid JitContext pointer
/// - `vec_ptr` must be a valid pointer to an initialized Vec
/// - `push_fn` must be a valid ListPushFn from the Vec's vtable
/// - `item_deserializer` must be a valid compiled deserializer for the element type
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_vec_push(
    ctx: *mut JitContext,
    vec_ptr: *mut u8,
    push_fn: *const u8,
    item_deserializer: *const u8,
) -> i32 {
    // Allocate stack space for the item
    // TODO: pass size as parameter or use alloca
    let mut item_buf: [u8; 256] = [0; 256];
    let item_ptr = item_buf.as_mut_ptr();

    // Deserialize the item
    type DeserializeFn = unsafe extern "C" fn(*mut JitContext, *mut u8) -> i32;
    let deserialize: DeserializeFn = unsafe { std::mem::transmute(item_deserializer) };
    let result = unsafe { deserialize(ctx, item_ptr) };

    if result != 0 {
        return result;
    }

    // Push the item to the Vec
    type PushFn = unsafe extern "C" fn(*mut u8, *mut u8);
    let push: PushFn = unsafe { std::mem::transmute(push_fn) };
    unsafe { push(vec_ptr, item_ptr) };

    0
}

/// Deserialize an entire `Vec<T>` from the parser.
///
/// This handles the complete Vec deserialization:
/// 1. Read ArrayStart
/// 2. Initialize Vec
/// 3. Loop reading elements and pushing
/// 4. Read ArrayEnd
///
/// # Safety
/// - `ctx` must be a valid JitContext pointer
/// - `out` must be a valid pointer to uninitialized Vec memory
/// - `init_fn` must be a valid ListInitInPlaceWithCapacityFn
/// - `push_fn` must be a valid ListPushFn
/// - `elem_size` must be the correct size of the element type
/// - `elem_deserializer` must be a valid deserializer fn for the element type, or null for primitives
/// - `scalar_tag` indicates the scalar type for primitive Vecs (only used if elem_deserializer is null)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_deserialize_vec(
    ctx: *mut JitContext,
    out: *mut u8,
    init_fn: *const u8,
    push_fn: *const u8,
    elem_size: usize,
    elem_deserializer: *const u8,
    scalar_tag: u8, // ScalarTag value for primitive elements
) -> i32 {
    // Read ArrayStart
    let mut raw_event = RawEvent {
        tag: EventTag::Error,
        scalar_tag: ScalarTag::I64,
        payload: EventPayload {
            scalar: ScalarPayload { i64_val: 0 },
        },
    };

    let ctx_ref = unsafe { &mut *ctx };
    let vtable = unsafe { &*ctx_ref.vtable };
    let result = unsafe { (vtable.next_event)(ctx_ref.parser, &mut raw_event) };
    if result != 0 {
        return result;
    }

    if raw_event.tag != EventTag::ArrayStart {
        return ERR_EXPECTED_STRUCT; // Reusing error code for "wrong event type"
    }

    // Initialize the Vec with capacity 0 (will grow as needed)
    type InitFn = unsafe extern "C" fn(*mut u8, usize) -> *mut u8;
    let init: InitFn = unsafe { std::mem::transmute(init_fn) };
    unsafe { init(out, 0) };

    // Allocate buffer for element
    // SAFETY: We use a fixed-size buffer and trust elem_size is correct
    let mut elem_buf: [u8; 1024] = [0; 1024];
    if elem_size > elem_buf.len() {
        // Element too large for our buffer
        return -100;
    }

    // Loop reading elements
    loop {
        // Peek next event
        let peeked = unsafe { jit_peek_event(ctx, &mut raw_event) };
        if peeked != 0 {
            return peeked;
        }

        // Check for ArrayEnd
        if raw_event.tag == EventTag::ArrayEnd {
            // Consume the ArrayEnd
            let result = unsafe { (vtable.next_event)(ctx_ref.parser, &mut raw_event) };
            if result != 0 {
                return result;
            }
            break;
        }

        // Deserialize element
        let elem_ptr = elem_buf.as_mut_ptr();

        if !elem_deserializer.is_null() {
            // Use provided deserializer (for structs or nested containers)
            type DeserializeFn = unsafe extern "C" fn(*mut JitContext, *mut u8) -> i32;
            let deserialize: DeserializeFn = unsafe { std::mem::transmute(elem_deserializer) };
            let result = unsafe { deserialize(ctx, elem_ptr) };
            if result != 0 {
                return result;
            }
        } else {
            // Handle primitive scalar element
            let result = unsafe { (vtable.next_event)(ctx_ref.parser, &mut raw_event) };
            if result != 0 {
                return result;
            }

            if raw_event.tag != EventTag::Scalar {
                return ERR_EXPECTED_SCALAR;
            }

            // Write scalar value to elem_buf based on expected type
            let scalar_tag_expected = ScalarTag::from_u8(scalar_tag);
            match scalar_tag_expected {
                ScalarTag::I64 => {
                    let val = unsafe { raw_event.payload.scalar.i64_val };
                    unsafe { *(elem_ptr as *mut i64) = val };
                }
                ScalarTag::U64 => {
                    let val = unsafe { raw_event.payload.scalar.u64_val };
                    unsafe { *(elem_ptr as *mut u64) = val };
                }
                ScalarTag::F64 => {
                    let val = unsafe { raw_event.payload.scalar.f64_val };
                    unsafe { *(elem_ptr as *mut f64) = val };
                }
                ScalarTag::Bool => {
                    let val = unsafe { raw_event.payload.scalar.bool_val };
                    unsafe { *(elem_ptr as *mut bool) = val };
                }
                _ => {
                    // Unsupported scalar type
                    return -101;
                }
            }
        }

        // Push element to Vec
        type PushFn = unsafe extern "C" fn(*mut u8, *mut u8);
        let push: PushFn = unsafe { std::mem::transmute(push_fn) };
        unsafe { push(out, elem_ptr) };
    }

    OK
}

/// Push a bool value to a `Vec<bool>`.
///
/// # Safety
/// - `vec_ptr` must be a valid pointer to an initialized `Vec<bool>`
/// - `push_fn` must be a valid ListPushFn
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_vec_push_bool(vec_ptr: *mut u8, push_fn: *const u8, value: bool) {
    let mut val = value;
    let val_ptr = &mut val as *mut bool as *mut u8;
    type PushFn = unsafe extern "C" fn(facet_core::PtrMut, facet_core::PtrMut);
    let push: PushFn = unsafe { std::mem::transmute(push_fn) };
    unsafe {
        push(
            facet_core::PtrMut::new(vec_ptr),
            facet_core::PtrMut::new(val_ptr),
        )
    };
}

/// Push a u8 value to a `Vec<u8>`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_vec_push_u8(vec_ptr: *mut u8, push_fn: *const u8, value: u8) {
    let mut val = value;
    let val_ptr = &mut val as *mut u8;
    type PushFn = unsafe extern "C" fn(facet_core::PtrMut, facet_core::PtrMut);
    let push: PushFn = unsafe { std::mem::transmute(push_fn) };
    unsafe {
        push(
            facet_core::PtrMut::new(vec_ptr),
            facet_core::PtrMut::new(val_ptr),
        )
    };
}

/// Push an i64 value to a `Vec<i64>`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_vec_push_i64(vec_ptr: *mut u8, push_fn: *const u8, value: i64) {
    let mut val = value;
    let val_ptr = &mut val as *mut i64 as *mut u8;
    type PushFn = unsafe extern "C" fn(facet_core::PtrMut, facet_core::PtrMut);
    let push: PushFn = unsafe { std::mem::transmute(push_fn) };
    unsafe {
        push(
            facet_core::PtrMut::new(vec_ptr),
            facet_core::PtrMut::new(val_ptr),
        )
    };
}

/// Push a u64 value to a `Vec<u64>`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_vec_push_u64(vec_ptr: *mut u8, push_fn: *const u8, value: u64) {
    let mut val = value;
    let val_ptr = &mut val as *mut u64 as *mut u8;
    type PushFn = unsafe extern "C" fn(facet_core::PtrMut, facet_core::PtrMut);
    let push: PushFn = unsafe { std::mem::transmute(push_fn) };
    unsafe {
        push(
            facet_core::PtrMut::new(vec_ptr),
            facet_core::PtrMut::new(val_ptr),
        )
    };
}

/// Push an f64 value to a `Vec<f64>`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_vec_push_f64(vec_ptr: *mut u8, push_fn: *const u8, value: f64) {
    let mut val = value;
    let val_ptr = &mut val as *mut f64 as *mut u8;
    type PushFn = unsafe extern "C" fn(facet_core::PtrMut, facet_core::PtrMut);
    let push: PushFn = unsafe { std::mem::transmute(push_fn) };
    unsafe {
        push(
            facet_core::PtrMut::new(vec_ptr),
            facet_core::PtrMut::new(val_ptr),
        )
    };
}

// =============================================================================
// Tier-2 Format JIT Helpers
// =============================================================================

/// Drop an owned string that was allocated during Tier-2 parsing but not moved into output.
///
/// This is used for temporary strings like map keys that were decoded (e.g., with escapes)
/// but then not stored anywhere.
///
/// # Safety
/// - `ptr`, `len`, `cap` must be valid String raw parts from a previous allocation
/// - Must only be called for owned strings (where the parsing allocated memory)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_drop_owned_string(ptr: *mut u8, len: usize, cap: usize) {
    unsafe {
        drop(String::from_raw_parts(ptr, len, cap));
    }
}

/// Push a String value to a `Vec<String>`.
/// Takes ownership of the string if `owned` is true.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_vec_push_string(
    vec_ptr: *mut u8,
    push_fn: *const u8,
    ptr: *const u8,
    len: usize,
    capacity: usize,
    owned: bool,
) {
    let string = if owned {
        unsafe { String::from_raw_parts(ptr as *mut u8, len, capacity) }
    } else {
        let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
        std::str::from_utf8(slice).unwrap_or("").to_string()
    };
    let mut val = string;
    let val_ptr = &mut val as *mut String as *mut u8;
    type PushFn = unsafe extern "C" fn(facet_core::PtrMut, facet_core::PtrMut);
    let push: PushFn = unsafe { std::mem::transmute(push_fn) };
    unsafe {
        push(
            facet_core::PtrMut::new(vec_ptr),
            facet_core::PtrMut::new(val_ptr),
        )
    };
    // Don't drop val - ownership transferred to Vec
    std::mem::forget(val);
}

/// Set the length of a Vec (for direct-fill operations).
///
/// # Safety
/// - `vec_ptr` must be a valid pointer to an initialized Vec
/// - `set_len_fn` must be a valid ListSetLenFn from the Vec's vtable
/// - `len` must not exceed the Vec's capacity
/// - All elements at indices `0..len` must be properly initialized
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_vec_set_len(vec_ptr: *mut u8, len: usize, set_len_fn: *const u8) {
    use facet_core::PtrMut;
    // ListSetLenFn = unsafe fn(list: PtrMut, len: usize)
    type SetLenFn = unsafe fn(PtrMut, usize);
    let func: SetLenFn = unsafe { std::mem::transmute(set_len_fn) };
    unsafe { func(PtrMut::new(vec_ptr), len) };
}

/// Get a raw mutable pointer to the Vec's data buffer.
///
/// # Safety
/// - `vec_ptr` must be a valid pointer to an initialized Vec
/// - `as_mut_ptr_typed_fn` must be a valid ListAsMutPtrTypedFn from the Vec's vtable
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_vec_as_mut_ptr_typed(
    vec_ptr: *mut u8,
    as_mut_ptr_typed_fn: *const u8,
) -> *mut u8 {
    use facet_core::PtrMut;
    // ListAsMutPtrTypedFn = unsafe fn(list: PtrMut) -> *mut u8
    type AsMutPtrTypedFn = unsafe fn(PtrMut) -> *mut u8;
    let func: AsMutPtrTypedFn = unsafe { std::mem::transmute(as_mut_ptr_typed_fn) };
    unsafe { func(PtrMut::new(vec_ptr)) }
}

/// Deserialize a list (Vec) by its Shape, handling nested Vecs recursively.
///
/// This is the preferred helper for Vec deserialization as it can handle
/// arbitrarily nested Vec types like `Vec<Vec<Vec<f64>>>`.
///
/// # Safety
/// - `ctx` must be a valid JitContext pointer
/// - `out` must point to uninitialized memory for the Vec
/// - `list_shape` must be a valid pointer to a Shape with Def::List
/// - `elem_struct_deserializer` is the compiled deserializer for struct elements (null for scalars/nested lists)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_deserialize_list_by_shape(
    ctx: *mut JitContext,
    out: *mut u8,
    list_shape: *const Shape,
    elem_struct_deserializer: *const u8,
) -> i32 {
    use facet_core::{Def, ScalarType};

    let shape = unsafe { &*list_shape };

    #[cfg(debug_assertions)]
    eprintln!(
        "[JIT] jit_deserialize_list_by_shape: type={}",
        shape.type_identifier
    );

    let Def::List(list_def) = &shape.def else {
        #[cfg(debug_assertions)]
        eprintln!(
            "[JIT] ERROR: not a list type, def={:?}",
            std::mem::discriminant(&shape.def)
        );
        return ERR_LIST_NOT_LIST_TYPE;
    };

    // Get element info
    let elem_shape = list_def.t;
    let elem_size = elem_shape
        .layout
        .sized_layout()
        .map(|l| l.size())
        .unwrap_or(0);

    #[cfg(debug_assertions)]
    eprintln!(
        "[JIT] list element: type={}, size={}",
        elem_shape.type_identifier, elem_size
    );

    if elem_size == 0 {
        #[cfg(debug_assertions)]
        eprintln!("[JIT] ERROR: unsized element type");
        return ERR_LIST_UNSIZED_ELEMENT;
    }

    // Get init and push functions
    let Some(init_fn) = list_def.init_in_place_with_capacity() else {
        #[cfg(debug_assertions)]
        eprintln!("[JIT] ERROR: no init function for list");
        return ERR_LIST_NO_INIT_FN;
    };
    let Some(push_fn) = list_def.push() else {
        #[cfg(debug_assertions)]
        eprintln!("[JIT] ERROR: no push function for list");
        return ERR_LIST_NO_PUSH_FN;
    };

    // Read ArrayStart
    let mut raw_event = RawEvent {
        tag: EventTag::Error,
        scalar_tag: ScalarTag::None,
        payload: EventPayload {
            scalar: ScalarPayload { i64_val: 0 },
        },
    };

    // Use jit_next_event to respect peek buffer
    let result = unsafe { jit_next_event(ctx, &mut raw_event) };
    if result != 0 {
        #[cfg(debug_assertions)]
        eprintln!(
            "[JIT] ERROR: failed to read ArrayStart, parser returned {}",
            result
        );
        return result;
    }

    if raw_event.tag != EventTag::ArrayStart {
        #[cfg(debug_assertions)]
        eprintln!("[JIT] ERROR: expected ArrayStart, got {:?}", raw_event.tag);
        return ERR_EXPECTED_ARRAY;
    }

    #[cfg(debug_assertions)]
    eprintln!("[JIT] list: got ArrayStart, initializing Vec");

    // Initialize the Vec with capacity 0
    let out_uninit = facet_core::PtrUninit::new(out);
    unsafe { init_fn(out_uninit, 0) };
    let out_mut = facet_core::PtrMut::new(out);

    // Allocate buffer for element (on stack for small elements, heap for large)
    let elem_buf: Vec<u8> = vec![0u8; elem_size];
    let elem_ptr = elem_buf.as_ptr() as *mut u8;

    // Determine element type
    let elem_scalar_type = elem_shape.scalar_type();
    let elem_is_list = matches!(&elem_shape.def, Def::List(_));
    let elem_is_struct = matches!(
        &elem_shape.ty,
        facet_core::Type::User(facet_core::UserType::Struct(_))
    );

    #[cfg(debug_assertions)]
    {
        eprintln!(
            "[JIT] list element classification: is_scalar={}, is_list={}, is_struct={}, has_deserializer={}",
            elem_scalar_type.is_some(),
            elem_is_list,
            elem_is_struct,
            !elem_struct_deserializer.is_null()
        );
        eprintln!(
            "[JIT] elem_buf address: {:p}, elem_ptr: {:p}",
            elem_buf.as_ptr(),
            elem_ptr
        );
    }

    // Loop reading elements
    loop {
        // Peek next event
        let peeked = unsafe { jit_peek_event(ctx, &mut raw_event) };
        if peeked != 0 {
            return peeked;
        }

        // Check for ArrayEnd
        if raw_event.tag == EventTag::ArrayEnd {
            // Consume the ArrayEnd (use jit_next_event to clear peek buffer)
            let result = unsafe { jit_next_event(ctx, &mut raw_event) };
            if result != 0 {
                return result;
            }
            break;
        }

        // Zero the element buffer
        unsafe { std::ptr::write_bytes(elem_ptr, 0, elem_size) };

        // Deserialize element based on type
        if elem_is_list {
            // Recursively deserialize nested list (pass null for nested struct deserializer)
            let result = unsafe {
                jit_deserialize_list_by_shape(
                    ctx,
                    elem_ptr,
                    elem_shape as *const Shape,
                    std::ptr::null(),
                )
            };
            if result != 0 {
                #[cfg(debug_assertions)]
                eprintln!(
                    "[JIT] ERROR: nested list deserialization failed with code {}",
                    result
                );
                return result;
            }
        } else if elem_is_struct && !elem_struct_deserializer.is_null() {
            // Deserialize struct element using compiled deserializer
            type DeserializeFn = unsafe extern "C" fn(*mut JitContext, *mut u8) -> i32;
            let deserialize: DeserializeFn =
                unsafe { std::mem::transmute(elem_struct_deserializer) };

            #[cfg(debug_assertions)]
            eprintln!(
                "[JIT] deserializing struct element using compiled deserializer at elem_ptr={:p}",
                elem_ptr
            );

            let result = unsafe { deserialize(ctx, elem_ptr) };
            if result != 0 {
                #[cfg(debug_assertions)]
                eprintln!(
                    "[JIT] ERROR: struct element deserialization failed with code {}",
                    result
                );
                return result;
            }
        } else if let Some(scalar_type) = elem_scalar_type {
            // Read scalar element (use jit_next_event to respect peek buffer)
            let result = unsafe { jit_next_event(ctx, &mut raw_event) };
            if result != 0 {
                return result;
            }

            if raw_event.tag != EventTag::Scalar {
                return ERR_EXPECTED_SCALAR;
            }

            // Write scalar value to elem_buf based on type
            match scalar_type {
                ScalarType::I8 => {
                    let val = unsafe { raw_event.payload.scalar.i64_val } as i8;
                    unsafe { *(elem_ptr as *mut i8) = val };
                }
                ScalarType::I16 => {
                    let val = unsafe { raw_event.payload.scalar.i64_val } as i16;
                    unsafe { *(elem_ptr as *mut i16) = val };
                }
                ScalarType::I32 => {
                    let val = unsafe { raw_event.payload.scalar.i64_val } as i32;
                    unsafe { *(elem_ptr as *mut i32) = val };
                }
                ScalarType::I64 => {
                    let val = unsafe { raw_event.payload.scalar.i64_val };
                    unsafe { *(elem_ptr as *mut i64) = val };
                }
                ScalarType::U8 => {
                    let val = unsafe { raw_event.payload.scalar.u64_val } as u8;
                    unsafe { *elem_ptr = val };
                }
                ScalarType::U16 => {
                    let val = unsafe { raw_event.payload.scalar.u64_val } as u16;
                    unsafe { *(elem_ptr as *mut u16) = val };
                }
                ScalarType::U32 => {
                    let val = unsafe { raw_event.payload.scalar.u64_val } as u32;
                    unsafe { *(elem_ptr as *mut u32) = val };
                }
                ScalarType::U64 => {
                    let val = unsafe { raw_event.payload.scalar.u64_val };
                    unsafe { *(elem_ptr as *mut u64) = val };
                }
                ScalarType::F32 => {
                    let val = unsafe { raw_event.payload.scalar.f64_val } as f32;
                    unsafe { *(elem_ptr as *mut f32) = val };
                }
                ScalarType::F64 => {
                    let val = unsafe { raw_event.payload.scalar.f64_val };
                    unsafe { *(elem_ptr as *mut f64) = val };
                }
                ScalarType::Bool => {
                    let val = unsafe { raw_event.payload.scalar.bool_val };
                    unsafe { *(elem_ptr as *mut bool) = val };
                }
                ScalarType::String => {
                    // Handle string element
                    let string_payload = unsafe { raw_event.payload.scalar.string_val };
                    let s = if string_payload.owned {
                        unsafe {
                            String::from_raw_parts(
                                string_payload.ptr as *mut u8,
                                string_payload.len,
                                string_payload.capacity,
                            )
                        }
                    } else {
                        let slice = unsafe {
                            std::slice::from_raw_parts(string_payload.ptr, string_payload.len)
                        };
                        std::str::from_utf8(slice).unwrap_or("").to_string()
                    };
                    unsafe { std::ptr::write(elem_ptr as *mut String, s) };
                }
                _ => {
                    #[cfg(debug_assertions)]
                    eprintln!(
                        "[JIT] ERROR: unsupported scalar type {:?} in list element",
                        scalar_type
                    );
                    return ERR_LIST_UNSUPPORTED_SCALAR;
                }
            }
        } else {
            // Unsupported element type (struct support would go here)
            // For now, structs in Vecs need the elem_deserializer path
            #[cfg(debug_assertions)]
            eprintln!(
                "[JIT] ERROR: unsupported element type in list: {}",
                elem_shape.type_identifier
            );
            return ERR_LIST_UNSUPPORTED_ELEMENT;
        }

        // Push element to Vec
        let elem_ptr_mut = facet_core::PtrMut::new(elem_ptr);
        unsafe { push_fn(out_mut, elem_ptr_mut) };
    }

    #[cfg(debug_assertions)]
    eprintln!("[JIT] list deserialization complete");

    OK
}

// =============================================================================
// Layout constants for JIT code generation
// =============================================================================

/// Size of RawEvent in bytes.
pub const RAW_EVENT_SIZE: usize = std::mem::size_of::<RawEvent>();

/// Offset of the `tag` field in RawEvent.
pub const RAW_EVENT_TAG_OFFSET: usize = 0;

/// Offset of the `payload` field in RawEvent.
pub const RAW_EVENT_PAYLOAD_OFFSET: usize = std::mem::offset_of!(RawEvent, payload);

/// Offset of `parser` in JitContext.
pub const JIT_CONTEXT_PARSER_OFFSET: usize = std::mem::offset_of!(JitContext, parser);

/// Offset of `vtable` in JitContext.
pub const JIT_CONTEXT_VTABLE_OFFSET: usize = std::mem::offset_of!(JitContext, vtable);

/// Offset of `skip_value` in ParserVTable.
pub const VTABLE_SKIP_VALUE_OFFSET: usize = std::mem::offset_of!(ParserVTable, skip_value);

/// Offset of `ptr` in FieldNamePayload.
pub const FIELD_NAME_PTR_OFFSET: usize = std::mem::offset_of!(FieldNamePayload, ptr);

/// Offset of `len` in FieldNamePayload.
pub const FIELD_NAME_LEN_OFFSET: usize = std::mem::offset_of!(FieldNamePayload, len);

/// Offset of `scalar_tag` in RawEvent.
pub const RAW_EVENT_SCALAR_TAG_OFFSET: usize = std::mem::offset_of!(RawEvent, scalar_tag);

/// Offset of scalar value within the payload (all scalar types are at offset 0 in the union).
pub const SCALAR_VALUE_OFFSET: usize = 0;

/// Offset of string ptr in StringPayload.
pub const STRING_PTR_OFFSET: usize = std::mem::offset_of!(StringPayload, ptr);
/// Offset of string len in StringPayload.
pub const STRING_LEN_OFFSET: usize = std::mem::offset_of!(StringPayload, len);
/// Offset of string capacity in StringPayload.
pub const STRING_CAPACITY_OFFSET: usize = std::mem::offset_of!(StringPayload, capacity);
/// Offset of string owned flag in StringPayload.
pub const STRING_OWNED_OFFSET: usize = std::mem::offset_of!(StringPayload, owned);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_layout() {
        eprintln!("RawEvent size: {}", std::mem::size_of::<RawEvent>());
        eprintln!("RawEvent align: {}", std::mem::align_of::<RawEvent>());
        eprintln!("EventPayload size: {}", std::mem::size_of::<EventPayload>());
        eprintln!(
            "ScalarPayload size: {}",
            std::mem::size_of::<ScalarPayload>()
        );
        eprintln!(
            "StringPayload size: {}",
            std::mem::size_of::<StringPayload>()
        );
        eprintln!("RAW_EVENT_TAG_OFFSET: {}", RAW_EVENT_TAG_OFFSET);
        eprintln!("RAW_EVENT_PAYLOAD_OFFSET: {}", RAW_EVENT_PAYLOAD_OFFSET);

        // Test that i64 values are stored correctly
        let raw = RawEvent {
            tag: EventTag::Scalar,
            scalar_tag: ScalarTag::I64,
            payload: EventPayload {
                scalar: ScalarPayload { i64_val: 42 },
            },
        };

        let ptr = &raw as *const RawEvent as *const u8;
        unsafe {
            let payload_ptr = ptr.add(RAW_EVENT_PAYLOAD_OFFSET);
            let value = *(payload_ptr as *const i64);
            eprintln!("Expected 42, got {}", value);
            assert_eq!(value, 42, "i64 value should be at offset 0 of payload");
        }
    }

    #[test]
    fn test_string_payload_layout() {
        // Verify the StringPayload layout matches what the JIT expects
        assert_eq!(
            std::mem::offset_of!(StringPayload, ptr),
            0,
            "ptr should be at offset 0"
        );
        assert_eq!(
            std::mem::offset_of!(StringPayload, len),
            8,
            "len should be at offset 8"
        );
        assert_eq!(
            std::mem::offset_of!(StringPayload, capacity),
            16,
            "capacity should be at offset 16"
        );
        assert_eq!(
            std::mem::offset_of!(StringPayload, owned),
            24,
            "owned should be at offset 24"
        );

        eprintln!("StringPayload offsets verified:");
        eprintln!("  ptr: {}", std::mem::offset_of!(StringPayload, ptr));
        eprintln!("  len: {}", std::mem::offset_of!(StringPayload, len));
        eprintln!(
            "  capacity: {}",
            std::mem::offset_of!(StringPayload, capacity)
        );
        eprintln!("  owned: {}", std::mem::offset_of!(StringPayload, owned));
    }

    #[test]
    fn test_string_into_raw_parts() {
        let s = String::from("hello world");
        let original_ptr = s.as_ptr();
        let original_len = s.len();
        let original_cap = s.capacity();

        let (ptr, len, cap) = string_into_raw_parts(s);

        assert_eq!(ptr as *const u8, original_ptr);
        assert_eq!(len, original_len);
        assert_eq!(cap, original_cap);

        // Reconstruct and drop to avoid leak
        unsafe {
            let _ = String::from_raw_parts(ptr, len, cap);
        }
    }
}
