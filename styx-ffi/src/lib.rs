#![doc = include_str!("../README.md")]
//! C bindings for the Styx configuration language parser.
//!
//! This crate provides a C-compatible API for parsing Styx documents.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

use styx_tree::{BuildError, Document, Object, Payload, Sequence, Value};

/// Opaque handle to a parsed Styx document.
pub struct StyxDocument {
    inner: Document,
}

/// Opaque handle to a Styx value.
pub struct StyxValue {
    #[allow(dead_code)]
    inner: *const Value,
}

/// Opaque handle to a Styx object.
pub struct StyxObject {
    #[allow(dead_code)]
    inner: *const Object,
}

/// Opaque handle to a Styx sequence.
pub struct StyxSequence {
    #[allow(dead_code)]
    inner: *const Sequence,
}

/// Result of a parse operation.
#[repr(C)]
pub struct StyxParseResult {
    /// The parsed document (null if error).
    pub document: *mut StyxDocument,
    /// Error message (null if success). Must be freed with `styx_free_string`.
    pub error: *mut c_char,
}

/// Type of a Styx value's payload.
#[repr(C)]
pub enum StyxPayloadKind {
    /// No payload (unit or tag-only).
    None,
    /// Scalar text.
    Scalar,
    /// Sequence of values.
    Sequence,
    /// Object (key-value pairs).
    Object,
}

// =============================================================================
// Parsing
// =============================================================================

/// Parse a Styx document from a UTF-8 string.
///
/// # Safety
/// - `source` must be a valid null-terminated UTF-8 string.
/// - The returned `StyxParseResult` must have its fields freed appropriately:
///   - If `document` is non-null, free it with `styx_free_document`.
///   - If `error` is non-null, free it with `styx_free_string`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn styx_parse(source: *const c_char) -> StyxParseResult {
    if source.is_null() {
        let error = CString::new("source is null").unwrap();
        return StyxParseResult {
            document: ptr::null_mut(),
            error: error.into_raw(),
        };
    }

    let source = match unsafe { CStr::from_ptr(source) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            let error = CString::new("source is not valid UTF-8").unwrap();
            return StyxParseResult {
                document: ptr::null_mut(),
                error: error.into_raw(),
            };
        }
    };

    match Document::parse(source) {
        Ok(doc) => {
            let boxed = Box::new(StyxDocument { inner: doc });
            StyxParseResult {
                document: Box::into_raw(boxed),
                error: ptr::null_mut(),
            }
        }
        Err(e) => {
            let error_msg = format_error(&e);
            let error =
                CString::new(error_msg).unwrap_or_else(|_| CString::new("unknown error").unwrap());
            StyxParseResult {
                document: ptr::null_mut(),
                error: error.into_raw(),
            }
        }
    }
}

fn format_error(e: &BuildError) -> String {
    match e {
        BuildError::UnexpectedEvent(msg) => format!("unexpected event: {}", msg),
        BuildError::UnclosedStructure => "unclosed structure".to_string(),
        BuildError::EmptyDocument => "empty document".to_string(),
        BuildError::Parse(kind, span) => {
            format!("parse error at {}-{}: {}", span.start, span.end, kind)
        }
    }
}

/// Free a parsed document.
///
/// # Safety
/// - `doc` must be a valid pointer returned by `styx_parse`, or null.
/// - After calling this function, `doc` is invalid and must not be used.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn styx_free_document(doc: *mut StyxDocument) {
    if !doc.is_null() {
        drop(unsafe { Box::from_raw(doc) });
    }
}

/// Free a string returned by the library.
///
/// # Safety
/// - `s` must be a valid pointer returned by a styx_* function, or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn styx_free_string(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { CString::from_raw(s) });
    }
}

// =============================================================================
// Document access
// =============================================================================

/// Get the root object of a document.
///
/// # Safety
/// - `doc` must be a valid pointer to a `StyxDocument`.
/// - The returned pointer is valid as long as `doc` is valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn styx_document_root(doc: *const StyxDocument) -> *const StyxObject {
    if doc.is_null() {
        return ptr::null();
    }
    let doc = unsafe { &*doc };
    &doc.inner.root as *const Object as *const StyxObject
}

/// Get a value by path from a document.
///
/// # Safety
/// - `doc` must be a valid pointer to a `StyxDocument`.
/// - `path` must be a valid null-terminated UTF-8 string.
/// - The returned pointer is valid as long as `doc` is valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn styx_document_get(
    doc: *const StyxDocument,
    path: *const c_char,
) -> *const StyxValue {
    if doc.is_null() || path.is_null() {
        return ptr::null();
    }
    let doc = unsafe { &*doc };
    let path = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null(),
    };
    match doc.inner.get(path) {
        Some(value) => value as *const Value as *const StyxValue,
        None => ptr::null(),
    }
}

// =============================================================================
// Value access
// =============================================================================

/// Get the payload kind of a value.
///
/// # Safety
/// - `value` must be a valid pointer to a `StyxValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn styx_value_payload_kind(value: *const StyxValue) -> StyxPayloadKind {
    if value.is_null() {
        return StyxPayloadKind::None;
    }
    let value = unsafe { &*(value as *const Value) };
    match &value.payload {
        None => StyxPayloadKind::None,
        Some(Payload::Scalar(_)) => StyxPayloadKind::Scalar,
        Some(Payload::Sequence(_)) => StyxPayloadKind::Sequence,
        Some(Payload::Object(_)) => StyxPayloadKind::Object,
    }
}

/// Check if a value is unit (no tag, no payload).
///
/// # Safety
/// - `value` must be a valid pointer to a `StyxValue`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn styx_value_is_unit(value: *const StyxValue) -> bool {
    if value.is_null() {
        return false;
    }
    let value = unsafe { &*(value as *const Value) };
    value.is_unit()
}

/// Get the tag name of a value (null if no tag).
///
/// # Safety
/// - `value` must be a valid pointer to a `StyxValue`.
/// - The returned string must be freed with `styx_free_string`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn styx_value_tag(value: *const StyxValue) -> *mut c_char {
    if value.is_null() {
        return ptr::null_mut();
    }
    let value = unsafe { &*(value as *const Value) };
    match value.tag_name() {
        Some(name) => CString::new(name)
            .map(|s| s.into_raw())
            .unwrap_or(ptr::null_mut()),
        None => ptr::null_mut(),
    }
}

/// Get the scalar text of a value (null if not a scalar).
///
/// # Safety
/// - `value` must be a valid pointer to a `StyxValue`.
/// - The returned string must be freed with `styx_free_string`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn styx_value_scalar(value: *const StyxValue) -> *mut c_char {
    if value.is_null() {
        return ptr::null_mut();
    }
    let value = unsafe { &*(value as *const Value) };
    match value.scalar_text() {
        Some(text) => CString::new(text)
            .map(|s| s.into_raw())
            .unwrap_or(ptr::null_mut()),
        None => ptr::null_mut(),
    }
}

/// Get the object payload of a value (null if not an object).
///
/// # Safety
/// - `value` must be a valid pointer to a `StyxValue`.
/// - The returned pointer is valid as long as the parent document is valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn styx_value_as_object(value: *const StyxValue) -> *const StyxObject {
    if value.is_null() {
        return ptr::null();
    }
    let value = unsafe { &*(value as *const Value) };
    match &value.payload {
        Some(Payload::Object(obj)) => obj as *const Object as *const StyxObject,
        _ => ptr::null(),
    }
}

/// Get the sequence payload of a value (null if not a sequence).
///
/// # Safety
/// - `value` must be a valid pointer to a `StyxValue`.
/// - The returned pointer is valid as long as the parent document is valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn styx_value_as_sequence(value: *const StyxValue) -> *const StyxSequence {
    if value.is_null() {
        return ptr::null();
    }
    let value = unsafe { &*(value as *const Value) };
    match &value.payload {
        Some(Payload::Sequence(seq)) => seq as *const Sequence as *const StyxSequence,
        _ => ptr::null(),
    }
}

/// Get a nested value by path.
///
/// # Safety
/// - `value` must be a valid pointer to a `StyxValue`.
/// - `path` must be a valid null-terminated UTF-8 string.
/// - The returned pointer is valid as long as the parent document is valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn styx_value_get(
    value: *const StyxValue,
    path: *const c_char,
) -> *const StyxValue {
    if value.is_null() || path.is_null() {
        return ptr::null();
    }
    let value = unsafe { &*(value as *const Value) };
    let path = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null(),
    };
    match value.get(path) {
        Some(v) => v as *const Value as *const StyxValue,
        None => ptr::null(),
    }
}

// =============================================================================
// Object access
// =============================================================================

/// Get the number of entries in an object.
///
/// # Safety
/// - `obj` must be a valid pointer to a `StyxObject`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn styx_object_len(obj: *const StyxObject) -> usize {
    if obj.is_null() {
        return 0;
    }
    let obj = unsafe { &*(obj as *const Object) };
    obj.len()
}

/// Get a value from an object by key.
///
/// # Safety
/// - `obj` must be a valid pointer to a `StyxObject`.
/// - `key` must be a valid null-terminated UTF-8 string.
/// - The returned pointer is valid as long as the parent document is valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn styx_object_get(
    obj: *const StyxObject,
    key: *const c_char,
) -> *const StyxValue {
    if obj.is_null() || key.is_null() {
        return ptr::null();
    }
    let obj = unsafe { &*(obj as *const Object) };
    let key = match unsafe { CStr::from_ptr(key) }.to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null(),
    };
    match obj.get(key) {
        Some(value) => value as *const Value as *const StyxValue,
        None => ptr::null(),
    }
}

/// Get the key at a given index in an object.
///
/// # Safety
/// - `obj` must be a valid pointer to a `StyxObject`.
/// - `index` must be less than `styx_object_len(obj)`.
/// - The returned pointer is valid as long as the parent document is valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn styx_object_key_at(
    obj: *const StyxObject,
    index: usize,
) -> *const StyxValue {
    if obj.is_null() {
        return ptr::null();
    }
    let obj = unsafe { &*(obj as *const Object) };
    match obj.entries.get(index) {
        Some(entry) => &entry.key as *const Value as *const StyxValue,
        None => ptr::null(),
    }
}

/// Get the value at a given index in an object.
///
/// # Safety
/// - `obj` must be a valid pointer to a `StyxObject`.
/// - `index` must be less than `styx_object_len(obj)`.
/// - The returned pointer is valid as long as the parent document is valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn styx_object_value_at(
    obj: *const StyxObject,
    index: usize,
) -> *const StyxValue {
    if obj.is_null() {
        return ptr::null();
    }
    let obj = unsafe { &*(obj as *const Object) };
    match obj.entries.get(index) {
        Some(entry) => &entry.value as *const Value as *const StyxValue,
        None => ptr::null(),
    }
}

// =============================================================================
// Sequence access
// =============================================================================

/// Get the number of items in a sequence.
///
/// # Safety
/// - `seq` must be a valid pointer to a `StyxSequence`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn styx_sequence_len(seq: *const StyxSequence) -> usize {
    if seq.is_null() {
        return 0;
    }
    let seq = unsafe { &*(seq as *const Sequence) };
    seq.len()
}

/// Get an item from a sequence by index.
///
/// # Safety
/// - `seq` must be a valid pointer to a `StyxSequence`.
/// - `index` must be less than `styx_sequence_len(seq)`.
/// - The returned pointer is valid as long as the parent document is valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn styx_sequence_get(
    seq: *const StyxSequence,
    index: usize,
) -> *const StyxValue {
    if seq.is_null() {
        return ptr::null();
    }
    let seq = unsafe { &*(seq as *const Sequence) };
    match seq.get(index) {
        Some(value) => value as *const Value as *const StyxValue,
        None => ptr::null(),
    }
}
