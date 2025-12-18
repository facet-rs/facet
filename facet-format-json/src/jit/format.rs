//! JSON-specific JIT format emitter.
//!
//! Implements `JitFormat` to generate Cranelift IR for direct JSON byte parsing.
//!
//! The actual parsing logic is implemented in helper functions in this crate's
//! `jit::helpers` module. The `helper_*()` methods return symbol names that the
//! format compiler uses to declare and call these helpers.

use facet_format::jit::{
    FunctionBuilder, InstBuilder, JITBuilder, JitCursor, JitFormat, JitStringValue, Value, types,
};

use super::helpers;

/// JSON format JIT emitter.
///
/// A zero-sized type that implements `JitFormat` for JSON syntax.
/// Helper functions are defined in this crate's `jit::helpers` module.
#[derive(Debug, Clone, Copy, Default)]
pub struct JsonJitFormat;

/// Error codes for JSON JIT parsing.
pub mod error {
    pub use super::helpers::error::*;
}

impl JitFormat for JsonJitFormat {
    fn register_helpers(builder: &mut JITBuilder) {
        // Register JSON-specific helper functions
        builder.symbol("json_jit_skip_ws", helpers::json_jit_skip_ws as *const u8);
        builder.symbol(
            "json_jit_seq_begin",
            helpers::json_jit_seq_begin as *const u8,
        );
        builder.symbol(
            "json_jit_seq_is_end",
            helpers::json_jit_seq_is_end as *const u8,
        );
        builder.symbol("json_jit_seq_next", helpers::json_jit_seq_next as *const u8);
        builder.symbol(
            "json_jit_parse_bool",
            helpers::json_jit_parse_bool as *const u8,
        );
    }

    fn helper_seq_begin() -> Option<&'static str> {
        Some("json_jit_seq_begin")
    }

    fn helper_seq_is_end() -> Option<&'static str> {
        Some("json_jit_seq_is_end")
    }

    fn helper_seq_next() -> Option<&'static str> {
        Some("json_jit_seq_next")
    }

    fn helper_parse_bool() -> Option<&'static str> {
        Some("json_jit_parse_bool")
    }

    const SEQ_STATE_SIZE: u32 = 0;
    const SEQ_STATE_ALIGN: u32 = 1;
    const MAP_STATE_SIZE: u32 = 0;
    const MAP_STATE_ALIGN: u32 = 1;

    fn emit_skip_ws(&self, builder: &mut FunctionBuilder, _cursor: &mut JitCursor) -> Value {
        // Return success - helpers handle whitespace internally
        builder.ins().iconst(types::I32, 0)
    }

    fn emit_skip_value(&self, builder: &mut FunctionBuilder, _cursor: &mut JitCursor) -> Value {
        builder.ins().iconst(types::I32, error::UNSUPPORTED as i64)
    }

    fn emit_peek_null(
        &self,
        builder: &mut FunctionBuilder,
        _cursor: &mut JitCursor,
    ) -> (Value, Value) {
        let zero = builder.ins().iconst(types::I8, 0);
        let err = builder.ins().iconst(types::I32, error::UNSUPPORTED as i64);
        (zero, err)
    }

    fn emit_consume_null(&self, builder: &mut FunctionBuilder, _cursor: &mut JitCursor) -> Value {
        builder.ins().iconst(types::I32, error::UNSUPPORTED as i64)
    }

    fn emit_parse_bool(
        &self,
        builder: &mut FunctionBuilder,
        _cursor: &mut JitCursor,
    ) -> (Value, Value) {
        // Placeholder - format_compiler calls json_jit_parse_bool directly
        let zero = builder.ins().iconst(types::I8, 0);
        let err = builder.ins().iconst(types::I32, error::UNSUPPORTED as i64);
        (zero, err)
    }

    fn emit_parse_i64(
        &self,
        builder: &mut FunctionBuilder,
        _cursor: &mut JitCursor,
    ) -> (Value, Value) {
        let zero = builder.ins().iconst(types::I64, 0);
        let err = builder.ins().iconst(types::I32, error::UNSUPPORTED as i64);
        (zero, err)
    }

    fn emit_parse_u64(
        &self,
        builder: &mut FunctionBuilder,
        _cursor: &mut JitCursor,
    ) -> (Value, Value) {
        let zero = builder.ins().iconst(types::I64, 0);
        let err = builder.ins().iconst(types::I32, error::UNSUPPORTED as i64);
        (zero, err)
    }

    fn emit_parse_f64(
        &self,
        builder: &mut FunctionBuilder,
        _cursor: &mut JitCursor,
    ) -> (Value, Value) {
        let zero = builder.ins().f64const(0.0);
        let err = builder.ins().iconst(types::I32, error::UNSUPPORTED as i64);
        (zero, err)
    }

    fn emit_parse_string(
        &self,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
    ) -> (JitStringValue, Value) {
        let zero = builder.ins().iconst(cursor.ptr_type, 0);
        let zero_i8 = builder.ins().iconst(types::I8, 0);
        let err = builder.ins().iconst(types::I32, error::UNSUPPORTED as i64);
        (
            JitStringValue {
                ptr: zero,
                len: zero,
                cap: zero,
                owned: zero_i8,
            },
            err,
        )
    }

    fn emit_seq_begin(
        &self,
        builder: &mut FunctionBuilder,
        _cursor: &mut JitCursor,
        _state_ptr: Value,
    ) -> Value {
        // Placeholder - format_compiler calls json_jit_seq_begin directly
        builder.ins().iconst(types::I32, error::UNSUPPORTED as i64)
    }

    fn emit_seq_is_end(
        &self,
        builder: &mut FunctionBuilder,
        _cursor: &mut JitCursor,
        _state_ptr: Value,
    ) -> (Value, Value) {
        // Placeholder - format_compiler calls json_jit_seq_is_end directly
        let zero = builder.ins().iconst(types::I8, 0);
        let err = builder.ins().iconst(types::I32, error::UNSUPPORTED as i64);
        (zero, err)
    }

    fn emit_seq_next(
        &self,
        builder: &mut FunctionBuilder,
        _cursor: &mut JitCursor,
        _state_ptr: Value,
    ) -> Value {
        // Placeholder - format_compiler calls json_jit_seq_next directly
        builder.ins().iconst(types::I32, error::UNSUPPORTED as i64)
    }

    fn emit_map_begin(
        &self,
        builder: &mut FunctionBuilder,
        _cursor: &mut JitCursor,
        _state_ptr: Value,
    ) -> Value {
        builder.ins().iconst(types::I32, error::UNSUPPORTED as i64)
    }

    fn emit_map_is_end(
        &self,
        builder: &mut FunctionBuilder,
        _cursor: &mut JitCursor,
        _state_ptr: Value,
    ) -> (Value, Value) {
        let zero = builder.ins().iconst(types::I8, 0);
        let err = builder.ins().iconst(types::I32, error::UNSUPPORTED as i64);
        (zero, err)
    }

    fn emit_map_read_key(
        &self,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
        _state_ptr: Value,
    ) -> (JitStringValue, Value) {
        let zero = builder.ins().iconst(cursor.ptr_type, 0);
        let zero_i8 = builder.ins().iconst(types::I8, 0);
        let err = builder.ins().iconst(types::I32, error::UNSUPPORTED as i64);
        (
            JitStringValue {
                ptr: zero,
                len: zero,
                cap: zero,
                owned: zero_i8,
            },
            err,
        )
    }

    fn emit_map_kv_sep(
        &self,
        builder: &mut FunctionBuilder,
        _cursor: &mut JitCursor,
        _state_ptr: Value,
    ) -> Value {
        builder.ins().iconst(types::I32, error::UNSUPPORTED as i64)
    }

    fn emit_map_next(
        &self,
        builder: &mut FunctionBuilder,
        _cursor: &mut JitCursor,
        _state_ptr: Value,
    ) -> Value {
        builder.ins().iconst(types::I32, error::UNSUPPORTED as i64)
    }
}
