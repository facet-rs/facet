//! Format-specific JIT code generation trait.
//!
//! This module defines the `JitFormat` trait that format crates implement
//! to provide Cranelift IR generation for format-specific parsing.

use cranelift::prelude::*;
use cranelift_module::Module;

/// How a format encodes struct fields.
///
/// This determines which compilation strategy the JIT uses for struct deserialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructEncoding {
    /// Map-based encoding: fields are keyed by name.
    ///
    /// Used by text formats like JSON, YAML, TOML where struct fields appear as
    /// key-value pairs (e.g., `{"name": "Alice", "age": 30}`).
    ///
    /// The JIT compiler generates a key-dispatch loop that:
    /// 1. Reads field names via `emit_map_read_key`
    /// 2. Matches keys to fields via dispatch table
    /// 3. Handles missing/extra fields
    Map,

    /// Positional encoding: fields appear in declaration order without keys.
    ///
    /// Used by binary formats like postcard, msgpack (array mode) where struct
    /// fields are encoded back-to-back in schema order with no field names.
    ///
    /// The JIT compiler generates straight-line code that:
    /// 1. Parses each field in declaration order
    /// 2. Does NOT call `emit_map_*` methods
    /// 3. Requires all fields to be present (no missing field handling)
    ///
    /// Note: `#[facet(flatten)]` is not supported with positional encoding.
    Positional,
}

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
    /// Pointer to JitScratch for error reporting and scratch space
    pub scratch_ptr: Value,
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
    /// Whether the output was initialized (used for cleanup on error)
    /// 0 = not initialized, 1 = initialized
    pub output_initialized: u8,

    // String scratch buffer for reuse during parsing.
    // This avoids allocating a new Vec for each escaped string.
    /// Pointer to string scratch buffer data
    pub string_scratch_ptr: *mut u8,
    /// Current length of data in the scratch buffer
    pub string_scratch_len: usize,
    /// Capacity of the scratch buffer
    pub string_scratch_cap: usize,
}

impl Default for JitScratch {
    fn default() -> Self {
        Self {
            error_code: 0,
            error_pos: 0,
            output_initialized: 0,
            string_scratch_ptr: std::ptr::null_mut(),
            string_scratch_len: 0,
            string_scratch_cap: 0,
        }
    }
}

impl Drop for JitScratch {
    fn drop(&mut self) {
        // Free the string scratch buffer if allocated
        if !self.string_scratch_ptr.is_null() && self.string_scratch_cap > 0 {
            unsafe {
                let _ = Vec::from_raw_parts(
                    self.string_scratch_ptr,
                    self.string_scratch_len,
                    self.string_scratch_cap,
                );
            }
            // Vec drop will deallocate
        }
    }
}

/// Offset of `error_code` field in `JitScratch`.
pub const JIT_SCRATCH_ERROR_CODE_OFFSET: i32 = std::mem::offset_of!(JitScratch, error_code) as i32;

/// Offset of `error_pos` field in `JitScratch`.
pub const JIT_SCRATCH_ERROR_POS_OFFSET: i32 = std::mem::offset_of!(JitScratch, error_pos) as i32;

/// Offset of `output_initialized` field in `JitScratch`.
pub const JIT_SCRATCH_OUTPUT_INITIALIZED_OFFSET: i32 =
    std::mem::offset_of!(JitScratch, output_initialized) as i32;

/// Offset of `string_scratch_ptr` field in `JitScratch`.
/// Reserved for future JIT code that directly accesses scratch buffer.
#[allow(dead_code)]
pub const JIT_SCRATCH_STRING_PTR_OFFSET: i32 =
    std::mem::offset_of!(JitScratch, string_scratch_ptr) as i32;

/// Offset of `string_scratch_len` field in `JitScratch`.
/// Reserved for future JIT code that directly accesses scratch buffer.
#[allow(dead_code)]
pub const JIT_SCRATCH_STRING_LEN_OFFSET: i32 =
    std::mem::offset_of!(JitScratch, string_scratch_len) as i32;

/// Offset of `string_scratch_cap` field in `JitScratch`.
/// Reserved for future JIT code that directly accesses scratch buffer.
#[allow(dead_code)]
pub const JIT_SCRATCH_STRING_CAP_OFFSET: i32 =
    std::mem::offset_of!(JitScratch, string_scratch_cap) as i32;

/// Format-specific JIT code generation trait.
///
/// Implemented by format crates (e.g., `facet-json`) to provide
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

    /// Whether `emit_seq_begin` returns an accurate element count.
    ///
    /// - `true`: Format provides exact count (e.g., postcard with length prefix).
    ///   Enables direct-fill optimization where count=0 means empty array.
    /// - `false`: Format doesn't know count upfront (e.g., JSON with delimiters).
    ///   count=0 means "unknown", must use push-based loop.
    const PROVIDES_SEQ_COUNT: bool = false;

    /// Stack slot size for map (object) state, 0 if no state needed.
    const MAP_STATE_SIZE: u32 = 0;
    /// Stack slot alignment for map state.
    const MAP_STATE_ALIGN: u32 = 1;

    /// How this format encodes struct fields.
    ///
    /// - [`StructEncoding::Map`]: Fields are keyed by name (JSON, YAML, TOML).
    ///   The compiler uses `emit_map_*` methods and key dispatch.
    /// - [`StructEncoding::Positional`]: Fields are in declaration order (postcard, msgpack-array).
    ///   The compiler parses fields sequentially without key matching.
    ///
    /// Default is `Map` for backward compatibility with existing format implementations.
    const STRUCT_ENCODING: StructEncoding = StructEncoding::Map;

    // === Utility ===

    /// Emit code to skip whitespace/comments.
    /// Returns error code (0 = success).
    fn emit_skip_ws(
        &self,
        module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
    ) -> Value;

    /// Emit code to skip an entire value (for unknown fields).
    /// Returns error code (0 = success).
    fn emit_skip_value(
        &self,
        module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
    ) -> Value;

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
    fn emit_parse_bool(
        &self,
        module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
    ) -> (Value, Value);

    /// Emit code to parse an unsigned 8-bit integer (raw byte).
    /// Returns (value: i8, error: i32).
    fn emit_parse_u8(
        &self,
        module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
    ) -> (Value, Value);

    /// Emit code to parse a signed 8-bit integer.
    /// Returns (value: i8 as i8, error: i32).
    ///
    /// Default implementation reads a single byte and reinterprets as signed.
    /// Text formats should override to parse text representation.
    fn emit_parse_i8(
        &self,
        _module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
    ) -> (Value, Value) {
        // Default: read single byte as i8 (same as u8, reinterpreted)
        let (u8_val, err) = self.emit_parse_u8(_module, b, c);
        // u8 value is already in correct bit pattern for i8
        (u8_val, err)
    }

    /// Emit code to parse a signed 64-bit integer.
    /// Returns (value: i64, error: i32).
    fn emit_parse_i64(
        &self,
        module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
    ) -> (Value, Value);

    /// Emit code to parse an unsigned 64-bit integer.
    /// Returns (value: u64 as i64, error: i32).
    fn emit_parse_u64(
        &self,
        module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
    ) -> (Value, Value);

    /// Emit code to parse a 32-bit float.
    /// Returns (value: f32, error: i32).
    ///
    /// Default implementation calls `emit_parse_f64` and demotes to f32.
    /// Binary formats that encode f32 differently from f64 should override this.
    fn emit_parse_f32(
        &self,
        module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
    ) -> (Value, Value) {
        let (f64_val, err) = self.emit_parse_f64(module, b, c);
        let f32_val = b.ins().fdemote(types::F32, f64_val);
        (f32_val, err)
    }

    /// Emit code to parse a 64-bit float.
    /// Returns (value: f64, error: i32).
    fn emit_parse_f64(
        &self,
        module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
    ) -> (Value, Value);

    /// Emit code to parse a string.
    /// Returns (JitStringValue, error: i32).
    fn emit_parse_string(
        &self,
        module: &mut cranelift_jit::JITModule,
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
        module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        state_ptr: Value,
    ) -> (Value, Value);

    /// Emit code to check if we're at sequence end (e.g., ']').
    /// Does NOT consume the delimiter.
    /// Returns (is_end: i8, error: i32).
    fn emit_seq_is_end(
        &self,
        module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        state_ptr: Value,
    ) -> (Value, Value);

    /// Emit code to advance to next sequence element.
    /// Called after parsing an element, handles separator (e.g., ',').
    /// Returns error code.
    fn emit_seq_next(
        &self,
        module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        state_ptr: Value,
    ) -> Value;

    /// Optional: Emit code to bulk-copy a `Vec<u8>` sequence.
    ///
    /// For formats where byte sequences are stored contiguously without per-byte encoding
    /// (like postcard), this enables a memcpy fast-path instead of byte-by-byte parsing.
    ///
    /// Called AFTER `emit_seq_begin` has been called and returned `count > 0`.
    /// The cursor position is right after the length prefix.
    ///
    /// Parameters:
    /// - `count`: The number of bytes to copy (from `emit_seq_begin`)
    /// - `dest_ptr`: Destination buffer pointer (from `as_mut_ptr_typed`)
    ///
    /// Returns `Some(error_code)` if supported (0 = success, advances cursor by `count`),
    /// or `None` if this format doesn't support bulk byte copy.
    ///
    /// Default implementation returns `None` (not supported).
    fn emit_seq_bulk_copy_u8(
        &self,
        _b: &mut FunctionBuilder,
        _c: &mut JitCursor,
        _count: Value,
        _dest_ptr: Value,
    ) -> Option<Value> {
        None
    }

    /// Optional: Check for and consume an empty sequence (e.g., `[]` in JSON).
    ///
    /// This is a fast path optimization for empty arrays. If supported, it:
    /// 1. Checks if the current position has an empty sequence pattern
    /// 2. If yes: consumes it, skips trailing whitespace, returns (true, 0)
    /// 3. If no: leaves cursor unchanged and returns (false, 0)
    /// 4. On error: returns (false, error_code)
    ///
    /// Returns `Some((is_empty: i8, error: i32))` if the format supports this optimization,
    /// or `None` to use the normal seq_begin/seq_is_end path.
    ///
    /// Default implementation returns `None` (not supported).
    fn emit_try_empty_seq(
        &self,
        _b: &mut FunctionBuilder,
        _c: &mut JitCursor,
    ) -> Option<(Value, Value)> {
        None
    }

    // === Maps (objects) ===

    /// Optional: Check for and consume an empty map (e.g., `{}` in JSON).
    ///
    /// This is a fast path optimization for empty maps. If supported, it:
    /// 1. Checks if the current position has an empty map pattern
    /// 2. If yes: consumes it, skips trailing whitespace, returns (true, 0)
    /// 3. If no: leaves cursor unchanged and returns (false, 0)
    /// 4. On error: returns (false, error_code)
    ///
    /// Returns `Some((is_empty: i8, error: i32))` if the format supports this optimization,
    /// or `None` to use the normal map_begin/map_is_end path.
    ///
    /// Default implementation returns `None` (not supported).
    fn emit_try_empty_map(
        &self,
        _b: &mut FunctionBuilder,
        _c: &mut JitCursor,
    ) -> Option<(Value, Value)> {
        None
    }

    /// Emit code to expect and consume map start delimiter (e.g., '{').
    /// Returns error code.
    fn emit_map_begin(
        &self,
        module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        state_ptr: Value,
    ) -> Value;

    /// Emit code to check if we're at map end (e.g., '}').
    /// Does NOT consume the delimiter.
    /// Returns (is_end: i8, error: i32).
    fn emit_map_is_end(
        &self,
        module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        state_ptr: Value,
    ) -> (Value, Value);

    /// Emit code to read a map key.
    /// Returns (JitStringValue for key, error: i32).
    fn emit_map_read_key(
        &self,
        module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        state_ptr: Value,
    ) -> (JitStringValue, Value);

    /// Emit code to consume key-value separator (e.g., ':').
    /// Returns error code.
    fn emit_map_kv_sep(
        &self,
        module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        state_ptr: Value,
    ) -> Value;

    /// Emit code to advance to next map entry.
    /// Called after parsing a value, handles entry separator (e.g., ',').
    /// Returns error code.
    fn emit_map_next(
        &self,
        module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        state_ptr: Value,
    ) -> Value;

    /// Optional: normalize a key before field matching.
    /// Default is no-op. YAML/TOML may want case-folding.
    fn emit_key_normalize(&self, _b: &mut FunctionBuilder, _key: &mut JitStringValue) {}
}

/// Stub implementation for parsers that don't support format JIT.
#[derive(Default, Clone, Copy)]
pub struct NoFormatJit;

impl JitFormat for NoFormatJit {
    fn register_helpers(_builder: &mut cranelift_jit::JITBuilder) {}

    fn emit_skip_ws(
        &self,
        _module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        _c: &mut JitCursor,
    ) -> Value {
        // Return error: unsupported
        b.ins().iconst(types::I32, -1)
    }

    fn emit_skip_value(
        &self,
        _module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        _c: &mut JitCursor,
    ) -> Value {
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

    fn emit_parse_bool(
        &self,
        _module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        _c: &mut JitCursor,
    ) -> (Value, Value) {
        let zero = b.ins().iconst(types::I8, 0);
        let err = b.ins().iconst(types::I32, -1);
        (zero, err)
    }

    fn emit_parse_u8(
        &self,
        _module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        _c: &mut JitCursor,
    ) -> (Value, Value) {
        let zero = b.ins().iconst(types::I8, 0);
        let err = b.ins().iconst(types::I32, -1);
        (zero, err)
    }

    fn emit_parse_i64(
        &self,
        _module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        _c: &mut JitCursor,
    ) -> (Value, Value) {
        let zero = b.ins().iconst(types::I64, 0);
        let err = b.ins().iconst(types::I32, -1);
        (zero, err)
    }

    fn emit_parse_u64(
        &self,
        _module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        _c: &mut JitCursor,
    ) -> (Value, Value) {
        let zero = b.ins().iconst(types::I64, 0);
        let err = b.ins().iconst(types::I32, -1);
        (zero, err)
    }

    fn emit_parse_f64(
        &self,
        _module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        _c: &mut JitCursor,
    ) -> (Value, Value) {
        let zero = b.ins().f64const(0.0);
        let err = b.ins().iconst(types::I32, -1);
        (zero, err)
    }

    fn emit_parse_string(
        &self,
        _module: &mut cranelift_jit::JITModule,
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
        _module: &mut cranelift_jit::JITModule,
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
        _module: &mut cranelift_jit::JITModule,
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
        _module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        _c: &mut JitCursor,
        _state_ptr: Value,
    ) -> Value {
        b.ins().iconst(types::I32, -1)
    }

    fn emit_map_begin(
        &self,
        _module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        _c: &mut JitCursor,
        _state_ptr: Value,
    ) -> Value {
        b.ins().iconst(types::I32, -1)
    }

    fn emit_map_is_end(
        &self,
        _module: &mut cranelift_jit::JITModule,
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
        _module: &mut cranelift_jit::JITModule,
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
        _module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        _c: &mut JitCursor,
        _state_ptr: Value,
    ) -> Value {
        b.ins().iconst(types::I32, -1)
    }

    fn emit_map_next(
        &self,
        _module: &mut cranelift_jit::JITModule,
        b: &mut FunctionBuilder,
        _c: &mut JitCursor,
        _state_ptr: Value,
    ) -> Value {
        b.ins().iconst(types::I32, -1)
    }
}

/// Returns the C ABI calling convention for the current platform.
///
/// This is necessary because Cranelift's `make_signature()` uses a default calling
/// convention that may not match `extern "C"` on all platforms. On Windows x64,
/// `extern "C"` uses the Microsoft x64 calling convention (WindowsFastcall),
/// while Cranelift may default to System V.
///
/// Use this when creating signatures for `call_indirect` to `extern "C"` helper functions.
#[inline]
pub const fn c_call_conv() -> cranelift::codegen::isa::CallConv {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        cranelift::codegen::isa::CallConv::WindowsFastcall
    }
    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    {
        // On non-Windows platforms, System V is the standard C ABI for x86_64
        // For other architectures, Cranelift's default is usually correct
        #[cfg(target_arch = "x86_64")]
        {
            cranelift::codegen::isa::CallConv::SystemV
        }
        #[cfg(target_arch = "aarch64")]
        {
            cranelift::codegen::isa::CallConv::AppleAarch64
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            // Fallback - let Cranelift decide
            cranelift::codegen::isa::CallConv::Fast
        }
    }
}

/// Creates a new signature with the correct C ABI calling convention for the current platform.
///
/// Use this instead of `module.make_signature()` when creating signatures for calls to
/// `extern "C"` functions. This ensures the correct calling convention is always set.
///
/// # Example
/// ```ignore
/// let sig = make_c_sig(module);
/// sig.params.push(AbiParam::new(pointer_type));
/// sig.returns.push(AbiParam::new(types::I32));
/// ```
#[inline]
pub fn make_c_sig(module: &cranelift_jit::JITModule) -> cranelift::codegen::ir::Signature {
    let mut sig = module.make_signature();
    sig.call_conv = c_call_conv();
    sig
}
