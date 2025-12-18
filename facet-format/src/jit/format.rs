//! Format-specific JIT code generation trait.
//!
//! This module defines the `JitFormat` trait that format crates implement
//! to provide Cranelift IR generation for format-specific parsing.

use cranelift::prelude::*;

/// Cursor state during JIT code generation.
///
/// Represents the position within the input buffer during parsing.
pub struct JitCursor {
    /// Pointer to the start of the input buffer (*const u8)
    pub input_ptr: Value,
    /// Length of the input buffer
    pub len: Value,
    /// Current position (mutable variable)
    pub pos: Variable,
    /// Platform pointer type (i64 on 64-bit)
    pub ptr_type: Type,
}

/// Represents a parsed string value during JIT codegen.
///
/// Strings can be either borrowed (pointing into input) or owned (heap allocated).
#[derive(Clone, Copy)]
pub struct JitStringValue {
    /// Pointer to string data (*const u8 or *mut u8)
    pub ptr: Value,
    /// Length in bytes
    pub len: Value,
    /// Capacity (only meaningful when owned)
    pub cap: Value,
    /// 1 if owned (needs drop), 0 if borrowed
    pub owned: Value,
}

/// Scratch space for error reporting from Tier-2 compiled functions.
#[repr(C)]
pub struct JitScratch {
    /// Error code (format-specific)
    pub error_code: i32,
    /// Byte position where error occurred
    pub error_pos: usize,
}

/// Offset of `error_code` field in `JitScratch`.
pub const JIT_SCRATCH_ERROR_CODE_OFFSET: i32 = std::mem::offset_of!(JitScratch, error_code) as i32;

/// Offset of `error_pos` field in `JitScratch`.
pub const JIT_SCRATCH_ERROR_POS_OFFSET: i32 = std::mem::offset_of!(JitScratch, error_pos) as i32;

/// Format-specific JIT code generation trait.
///
/// Implemented by format crates (e.g., `facet-format-json`) to provide
/// Cranelift IR generation for parsing their specific syntax.
///
/// The trait methods emit Cranelift IR that:
/// - Reads from `(input_ptr, len)` at position `cursor.pos`
/// - Updates `cursor.pos` as parsing advances
/// - Returns error codes (0 = success, negative = error)
///
/// ## Helper-based implementation
///
/// For formats that use external helper functions (Option B approach),
/// override the `helper_*` methods to return symbol names. The default
/// `emit_*` implementations will then generate calls to those helpers.
/// If a `helper_*` method returns `None`, that operation is unsupported.
pub trait JitFormat: Default + Copy + 'static {
    /// Register format-specific helper functions with the JIT builder.
    /// Called before compilation to register all helper symbols.
    fn register_helpers(builder: &mut cranelift_jit::JITBuilder);

    // =========================================================================
    // Helper symbol names (Option B: formats provide helper function names)
    // =========================================================================
    // These return the symbol names for helper functions that the format
    // registers via `register_helpers`. The compiler will import and call these.
    // Return `None` if the operation is not supported.

    /// Symbol name for seq_begin helper: fn(input, len, pos) -> (new_pos, error)
    fn helper_seq_begin() -> Option<&'static str> {
        None
    }

    /// Symbol name for seq_is_end helper: fn(input, len, pos) -> (packed_pos_end, error)
    /// packed_pos_end = (is_end << 63) | new_pos
    fn helper_seq_is_end() -> Option<&'static str> {
        None
    }

    /// Symbol name for seq_next helper: fn(input, len, pos) -> (new_pos, error)
    fn helper_seq_next() -> Option<&'static str> {
        None
    }

    /// Symbol name for parse_bool helper: fn(input, len, pos) -> (packed_pos_value, error)
    /// packed_pos_value = (value << 63) | new_pos
    fn helper_parse_bool() -> Option<&'static str> {
        None
    }

    /// Symbol name for parse_i64 helper: fn(input, len, pos) -> (new_pos, value, error)
    fn helper_parse_i64() -> Option<&'static str> {
        None
    }

    /// Symbol name for parse_u64 helper: fn(input, len, pos) -> (new_pos, value, error)
    fn helper_parse_u64() -> Option<&'static str> {
        None
    }

    /// Symbol name for parse_f64 helper: fn(input, len, pos) -> (new_pos, value, error)
    fn helper_parse_f64() -> Option<&'static str> {
        None
    }

    /// Symbol name for parse_string helper (format-specific signature)
    fn helper_parse_string() -> Option<&'static str> {
        None
    }

    /// Stack slot size for sequence (array) state, 0 if no state needed.
    const SEQ_STATE_SIZE: u32 = 0;
    /// Stack slot alignment for sequence state.
    const SEQ_STATE_ALIGN: u32 = 1;

    /// Stack slot size for map (object) state, 0 if no state needed.
    const MAP_STATE_SIZE: u32 = 0;
    /// Stack slot alignment for map state.
    const MAP_STATE_ALIGN: u32 = 1;

    // === Utility ===

    /// Emit code to skip whitespace/comments.
    /// Returns error code (0 = success).
    fn emit_skip_ws(&self, b: &mut FunctionBuilder, c: &mut JitCursor) -> Value;

    /// Emit code to skip an entire value (for unknown fields).
    /// Returns error code (0 = success).
    fn emit_skip_value(&self, b: &mut FunctionBuilder, c: &mut JitCursor) -> Value;

    // === Null / Option ===

    /// Emit code to peek whether the next value is null (without consuming).
    /// Returns (is_null: i8, error: i32).
    fn emit_peek_null(&self, b: &mut FunctionBuilder, c: &mut JitCursor) -> (Value, Value);

    /// Emit code to consume a null value (after peek_null returned true).
    /// Returns error code.
    fn emit_consume_null(&self, b: &mut FunctionBuilder, c: &mut JitCursor) -> Value;

    // === Scalars ===

    /// Emit code to parse a boolean.
    /// Returns (value: i8, error: i32).
    fn emit_parse_bool(&self, b: &mut FunctionBuilder, c: &mut JitCursor) -> (Value, Value);

    /// Emit code to parse an unsigned 8-bit integer (raw byte).
    /// Returns (value: i8, error: i32).
    fn emit_parse_u8(&self, b: &mut FunctionBuilder, c: &mut JitCursor) -> (Value, Value);

    /// Emit code to parse a signed 64-bit integer.
    /// Returns (value: i64, error: i32).
    fn emit_parse_i64(&self, b: &mut FunctionBuilder, c: &mut JitCursor) -> (Value, Value);

    /// Emit code to parse an unsigned 64-bit integer.
    /// Returns (value: u64 as i64, error: i32).
    fn emit_parse_u64(&self, b: &mut FunctionBuilder, c: &mut JitCursor) -> (Value, Value);

    /// Emit code to parse a 64-bit float.
    /// Returns (value: f64, error: i32).
    fn emit_parse_f64(&self, b: &mut FunctionBuilder, c: &mut JitCursor) -> (Value, Value);

    /// Emit code to parse a string.
    /// Returns (JitStringValue, error: i32).
    fn emit_parse_string(
        &self,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
    ) -> (JitStringValue, Value);

    // === Sequences (arrays) ===

    /// Emit code to expect and consume sequence start delimiter (e.g., '[').
    /// `state_ptr` points to SEQ_STATE_SIZE bytes of stack space.
    ///
    /// Returns `(count, error)` where:
    /// - `count`: The number of elements if known (for length-prefixed formats like postcard),
    ///   or 0 if unknown (for delimiter formats like JSON). Used for Vec preallocation.
    /// - `error`: Error code (0 = success, negative = error)
    fn emit_seq_begin(
        &self,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        state_ptr: Value,
    ) -> (Value, Value);

    /// Emit code to check if we're at sequence end (e.g., ']').
    /// Does NOT consume the delimiter.
    /// Returns (is_end: i8, error: i32).
    fn emit_seq_is_end(
        &self,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        state_ptr: Value,
    ) -> (Value, Value);

    /// Emit code to advance to next sequence element.
    /// Called after parsing an element, handles separator (e.g., ',').
    /// Returns error code.
    fn emit_seq_next(&self, b: &mut FunctionBuilder, c: &mut JitCursor, state_ptr: Value) -> Value;

    // === Maps (objects) ===

    /// Emit code to expect and consume map start delimiter (e.g., '{').
    /// Returns error code.
    fn emit_map_begin(&self, b: &mut FunctionBuilder, c: &mut JitCursor, state_ptr: Value)
    -> Value;

    /// Emit code to check if we're at map end (e.g., '}').
    /// Does NOT consume the delimiter.
    /// Returns (is_end: i8, error: i32).
    fn emit_map_is_end(
        &self,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        state_ptr: Value,
    ) -> (Value, Value);

    /// Emit code to read a map key.
    /// Returns (JitStringValue for key, error: i32).
    fn emit_map_read_key(
        &self,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        state_ptr: Value,
    ) -> (JitStringValue, Value);

    /// Emit code to consume key-value separator (e.g., ':').
    /// Returns error code.
    fn emit_map_kv_sep(
        &self,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        state_ptr: Value,
    ) -> Value;

    /// Emit code to advance to next map entry.
    /// Called after parsing a value, handles entry separator (e.g., ',').
    /// Returns error code.
    fn emit_map_next(&self, b: &mut FunctionBuilder, c: &mut JitCursor, state_ptr: Value) -> Value;

    /// Optional: normalize a key before field matching.
    /// Default is no-op. YAML/TOML may want case-folding.
    fn emit_key_normalize(&self, _b: &mut FunctionBuilder, _key: &mut JitStringValue) {}
}

/// Stub implementation for parsers that don't support format JIT.
#[derive(Default, Clone, Copy)]
pub struct NoFormatJit;

impl JitFormat for NoFormatJit {
    fn register_helpers(_builder: &mut cranelift_jit::JITBuilder) {}

    fn emit_skip_ws(&self, b: &mut FunctionBuilder, _c: &mut JitCursor) -> Value {
        // Return error: unsupported
        b.ins().iconst(types::I32, -1)
    }

    fn emit_skip_value(&self, b: &mut FunctionBuilder, _c: &mut JitCursor) -> Value {
        b.ins().iconst(types::I32, -1)
    }

    fn emit_peek_null(&self, b: &mut FunctionBuilder, _c: &mut JitCursor) -> (Value, Value) {
        let zero = b.ins().iconst(types::I8, 0);
        let err = b.ins().iconst(types::I32, -1);
        (zero, err)
    }

    fn emit_consume_null(&self, b: &mut FunctionBuilder, _c: &mut JitCursor) -> Value {
        b.ins().iconst(types::I32, -1)
    }

    fn emit_parse_bool(&self, b: &mut FunctionBuilder, _c: &mut JitCursor) -> (Value, Value) {
        let zero = b.ins().iconst(types::I8, 0);
        let err = b.ins().iconst(types::I32, -1);
        (zero, err)
    }

    fn emit_parse_u8(&self, b: &mut FunctionBuilder, _c: &mut JitCursor) -> (Value, Value) {
        let zero = b.ins().iconst(types::I8, 0);
        let err = b.ins().iconst(types::I32, -1);
        (zero, err)
    }

    fn emit_parse_i64(&self, b: &mut FunctionBuilder, _c: &mut JitCursor) -> (Value, Value) {
        let zero = b.ins().iconst(types::I64, 0);
        let err = b.ins().iconst(types::I32, -1);
        (zero, err)
    }

    fn emit_parse_u64(&self, b: &mut FunctionBuilder, _c: &mut JitCursor) -> (Value, Value) {
        let zero = b.ins().iconst(types::I64, 0);
        let err = b.ins().iconst(types::I32, -1);
        (zero, err)
    }

    fn emit_parse_f64(&self, b: &mut FunctionBuilder, _c: &mut JitCursor) -> (Value, Value) {
        let zero = b.ins().f64const(0.0);
        let err = b.ins().iconst(types::I32, -1);
        (zero, err)
    }

    fn emit_parse_string(
        &self,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
    ) -> (JitStringValue, Value) {
        let null = b.ins().iconst(c.ptr_type, 0);
        let zero = b.ins().iconst(c.ptr_type, 0);
        let err = b.ins().iconst(types::I32, -1);
        (
            JitStringValue {
                ptr: null,
                len: zero,
                cap: zero,
                owned: b.ins().iconst(types::I8, 0),
            },
            err,
        )
    }

    fn emit_seq_begin(
        &self,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        _state_ptr: Value,
    ) -> (Value, Value) {
        let zero_count = b.ins().iconst(c.ptr_type, 0);
        let err = b.ins().iconst(types::I32, -1);
        (zero_count, err)
    }

    fn emit_seq_is_end(
        &self,
        b: &mut FunctionBuilder,
        _c: &mut JitCursor,
        _state_ptr: Value,
    ) -> (Value, Value) {
        let zero = b.ins().iconst(types::I8, 0);
        let err = b.ins().iconst(types::I32, -1);
        (zero, err)
    }

    fn emit_seq_next(
        &self,
        b: &mut FunctionBuilder,
        _c: &mut JitCursor,
        _state_ptr: Value,
    ) -> Value {
        b.ins().iconst(types::I32, -1)
    }

    fn emit_map_begin(
        &self,
        b: &mut FunctionBuilder,
        _c: &mut JitCursor,
        _state_ptr: Value,
    ) -> Value {
        b.ins().iconst(types::I32, -1)
    }

    fn emit_map_is_end(
        &self,
        b: &mut FunctionBuilder,
        _c: &mut JitCursor,
        _state_ptr: Value,
    ) -> (Value, Value) {
        let zero = b.ins().iconst(types::I8, 0);
        let err = b.ins().iconst(types::I32, -1);
        (zero, err)
    }

    fn emit_map_read_key(
        &self,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        _state_ptr: Value,
    ) -> (JitStringValue, Value) {
        let null = b.ins().iconst(c.ptr_type, 0);
        let zero = b.ins().iconst(c.ptr_type, 0);
        let err = b.ins().iconst(types::I32, -1);
        (
            JitStringValue {
                ptr: null,
                len: zero,
                cap: zero,
                owned: b.ins().iconst(types::I8, 0),
            },
            err,
        )
    }

    fn emit_map_kv_sep(
        &self,
        b: &mut FunctionBuilder,
        _c: &mut JitCursor,
        _state_ptr: Value,
    ) -> Value {
        b.ins().iconst(types::I32, -1)
    }

    fn emit_map_next(
        &self,
        b: &mut FunctionBuilder,
        _c: &mut JitCursor,
        _state_ptr: Value,
    ) -> Value {
        b.ins().iconst(types::I32, -1)
    }
}
