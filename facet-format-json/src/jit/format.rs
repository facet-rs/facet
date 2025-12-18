//! JSON-specific JIT format emitter.
//!
//! Implements `JitFormat` to generate Cranelift IR for direct JSON byte parsing.
//!
//! The emit_* methods generate inline Cranelift IR for parsing operations,
//! eliminating function call overhead in the hot path.

use facet_format::jit::{
    FunctionBuilder, InstBuilder, IntCC, JITBuilder, JitCursor, JitFormat, JitStringValue,
    MemFlags, Value, types,
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
        cursor: &mut JitCursor,
    ) -> (Value, Value) {
        // Inline bool parsing: check for "true" (4 bytes) or "false" (5 bytes)
        //
        // "true"  = 0x74 0x72 0x75 0x65 = little-endian u32: 0x65757274
        // "false" = 0x66 0x61 0x6c 0x73 0x65 = u32: 0x736c6166, then 0x65

        let pos = builder.use_var(cursor.pos);

        // Variables to hold results (used across blocks)
        let result_value_var = builder.declare_var(types::I8);
        let result_error_var = builder.declare_var(types::I32);
        let zero_i8 = builder.ins().iconst(types::I8, 0);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        builder.def_var(result_value_var, zero_i8);
        builder.def_var(result_error_var, zero_i32);

        // Check if we have at least 4 bytes for "true"
        let four = builder.ins().iconst(cursor.ptr_type, 4);
        let pos_plus_4 = builder.ins().iadd(pos, four);
        let have_4_bytes =
            builder
                .ins()
                .icmp(IntCC::UnsignedLessThanOrEqual, pos_plus_4, cursor.len);

        // Create blocks for the control flow
        let check_true = builder.create_block();
        let check_false = builder.create_block();
        let found_true = builder.create_block();
        let found_false = builder.create_block();
        let error_block = builder.create_block();
        let merge = builder.create_block();

        builder
            .ins()
            .brif(have_4_bytes, check_true, &[], error_block, &[]);

        // check_true: load 4 bytes and compare to "true"
        builder.switch_to_block(check_true);
        builder.seal_block(check_true);
        let addr = builder.ins().iadd(cursor.input_ptr, pos);
        let word = builder.ins().load(types::I32, MemFlags::trusted(), addr, 0);
        let true_const = builder.ins().iconst(types::I32, 0x65757274u32 as i64); // "true" LE
        let is_true = builder.ins().icmp(IntCC::Equal, word, true_const);
        builder
            .ins()
            .brif(is_true, found_true, &[], check_false, &[]);

        // found_true: set result (1, 0) and advance by 4
        builder.switch_to_block(found_true);
        builder.seal_block(found_true);
        let one_i8 = builder.ins().iconst(types::I8, 1);
        let zero_err = builder.ins().iconst(types::I32, 0);
        builder.def_var(result_value_var, one_i8);
        builder.def_var(result_error_var, zero_err);
        builder.def_var(cursor.pos, pos_plus_4);
        builder.ins().jump(merge, &[]);

        // check_false: check if we have 5 bytes for "false"
        builder.switch_to_block(check_false);
        builder.seal_block(check_false);
        let five = builder.ins().iconst(cursor.ptr_type, 5);
        let pos_plus_5 = builder.ins().iadd(pos, five);
        let have_5_bytes =
            builder
                .ins()
                .icmp(IntCC::UnsignedLessThanOrEqual, pos_plus_5, cursor.len);
        let check_false_content = builder.create_block();
        builder
            .ins()
            .brif(have_5_bytes, check_false_content, &[], error_block, &[]);

        // check_false_content: load and compare "fals" + "e"
        builder.switch_to_block(check_false_content);
        builder.seal_block(check_false_content);
        // Compare first 4 bytes to "fals" (0x736c6166)
        let fals_word = builder.ins().load(types::I32, MemFlags::trusted(), addr, 0);
        let fals_const = builder.ins().iconst(types::I32, 0x736c6166u32 as i64); // "fals" LE
        let is_fals = builder.ins().icmp(IntCC::Equal, fals_word, fals_const);
        let check_e = builder.create_block();
        builder.ins().brif(is_fals, check_e, &[], error_block, &[]);

        // check_e: check 5th byte is 'e'
        builder.switch_to_block(check_e);
        builder.seal_block(check_e);
        let e_byte = builder.ins().load(types::I8, MemFlags::trusted(), addr, 4);
        let e_const = builder.ins().iconst(types::I8, 0x65); // 'e'
        let is_e = builder.ins().icmp(IntCC::Equal, e_byte, e_const);
        builder.ins().brif(is_e, found_false, &[], error_block, &[]);

        // found_false: set result (0, 0) and advance by 5
        builder.switch_to_block(found_false);
        builder.seal_block(found_false);
        let zero_val = builder.ins().iconst(types::I8, 0);
        let zero_err2 = builder.ins().iconst(types::I32, 0);
        builder.def_var(result_value_var, zero_val);
        builder.def_var(result_error_var, zero_err2);
        builder.def_var(cursor.pos, pos_plus_5);
        builder.ins().jump(merge, &[]);

        // error_block: set error
        builder.switch_to_block(error_block);
        builder.seal_block(error_block);
        let err_val = builder.ins().iconst(types::I8, 0);
        let err_code = builder
            .ins()
            .iconst(types::I32, error::EXPECTED_BOOL as i64);
        builder.def_var(result_value_var, err_val);
        builder.def_var(result_error_var, err_code);
        // Don't update pos on error
        builder.ins().jump(merge, &[]);

        // merge: read results from variables
        builder.switch_to_block(merge);
        builder.seal_block(merge);
        let result_value = builder.use_var(result_value_var);
        let result_error = builder.use_var(result_error_var);

        (result_value, result_error)
    }

    fn emit_parse_u8(
        &self,
        builder: &mut FunctionBuilder,
        _cursor: &mut JitCursor,
    ) -> (Value, Value) {
        // JSON doesn't have raw byte parsing - numbers are text
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
        cursor: &mut JitCursor,
        _state_ptr: Value,
    ) -> (Value, Value) {
        // Inline seq_begin: skip whitespace, expect '[', skip whitespace after
        //
        // Returns (count: usize, error: I32):
        //   - count: Always 0 for JSON (delimiter-based, count unknown upfront)
        //   - error: 0 on success, negative on error
        //
        // Control flow:
        //   entry -> skip_leading_ws_loop
        //   skip_leading_ws_loop -> check_leading_ws | eof_error
        //   check_leading_ws -> skip_leading_ws_advance | check_bracket
        //   skip_leading_ws_advance -> skip_leading_ws_loop (back edge)
        //   check_bracket -> skip_trailing_ws_loop | not_bracket_error
        //   skip_trailing_ws_loop -> check_trailing_ws | merge (success)
        //   check_trailing_ws -> skip_trailing_ws_advance | merge (success)
        //   skip_trailing_ws_advance -> skip_trailing_ws_loop (back edge)
        //   eof_error -> merge (with error)
        //   not_bracket_error -> merge (with error)

        // Result variable (0 = success)
        let result_error_var = builder.declare_var(types::I32);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        // JSON doesn't know array length upfront, so count is always 0
        let zero_count = builder.ins().iconst(cursor.ptr_type, 0);
        builder.def_var(result_error_var, zero_i32);

        let one = builder.ins().iconst(cursor.ptr_type, 1);

        // Whitespace constants
        let space = builder.ins().iconst(types::I8, b' ' as i64);
        let tab = builder.ins().iconst(types::I8, b'\t' as i64);
        let newline = builder.ins().iconst(types::I8, b'\n' as i64);
        let cr = builder.ins().iconst(types::I8, b'\r' as i64);

        // Create blocks
        let skip_leading_ws_loop = builder.create_block();
        let check_leading_ws = builder.create_block();
        let skip_leading_ws_advance = builder.create_block();
        let check_bracket = builder.create_block();
        let skip_trailing_ws_loop = builder.create_block();
        let check_trailing_ws = builder.create_block();
        let skip_trailing_ws_advance = builder.create_block();
        let not_bracket_error = builder.create_block();
        let eof_error = builder.create_block();
        let merge = builder.create_block();

        // Entry: jump to leading whitespace loop
        builder.ins().jump(skip_leading_ws_loop, &[]);

        // === Skip leading whitespace loop ===
        builder.switch_to_block(skip_leading_ws_loop);
        // Has back edge from skip_leading_ws_advance
        let pos = builder.use_var(cursor.pos);
        let have_bytes = builder.ins().icmp(IntCC::UnsignedLessThan, pos, cursor.len);
        builder
            .ins()
            .brif(have_bytes, check_leading_ws, &[], eof_error, &[]);

        builder.switch_to_block(check_leading_ws);
        builder.seal_block(check_leading_ws);
        let addr = builder.ins().iadd(cursor.input_ptr, pos);
        let byte = builder.ins().load(types::I8, MemFlags::trusted(), addr, 0);

        let is_space = builder.ins().icmp(IntCC::Equal, byte, space);
        let is_tab = builder.ins().icmp(IntCC::Equal, byte, tab);
        let is_newline = builder.ins().icmp(IntCC::Equal, byte, newline);
        let is_cr = builder.ins().icmp(IntCC::Equal, byte, cr);
        let is_ws_1 = builder.ins().bor(is_space, is_tab);
        let is_ws_2 = builder.ins().bor(is_newline, is_cr);
        let is_ws = builder.ins().bor(is_ws_1, is_ws_2);

        builder
            .ins()
            .brif(is_ws, skip_leading_ws_advance, &[], check_bracket, &[]);

        builder.switch_to_block(skip_leading_ws_advance);
        builder.seal_block(skip_leading_ws_advance);
        let next_pos = builder.ins().iadd(pos, one);
        builder.def_var(cursor.pos, next_pos);
        builder.ins().jump(skip_leading_ws_loop, &[]);

        // Seal loop header after back edge
        builder.seal_block(skip_leading_ws_loop);

        // === Check for '[' ===
        builder.switch_to_block(check_bracket);
        builder.seal_block(check_bracket);
        let open_bracket = builder.ins().iconst(types::I8, b'[' as i64);
        let is_bracket = builder.ins().icmp(IntCC::Equal, byte, open_bracket);
        builder.ins().brif(
            is_bracket,
            skip_trailing_ws_loop,
            &[],
            not_bracket_error,
            &[],
        );

        // === Advance past '[' and skip trailing whitespace ===
        // skip_trailing_ws_loop is an intermediate block that advances past '['
        builder.switch_to_block(skip_trailing_ws_loop);
        builder.seal_block(skip_trailing_ws_loop);
        let pos2 = builder.use_var(cursor.pos);
        let pos_after_bracket = builder.ins().iadd(pos2, one);
        builder.def_var(cursor.pos, pos_after_bracket);

        // Create and jump to the actual ws skip loop
        let trailing_ws_check_bounds = builder.create_block();
        builder.ins().jump(trailing_ws_check_bounds, &[]);

        // === Trailing whitespace skip loop ===
        builder.switch_to_block(trailing_ws_check_bounds);
        // Has back edge from skip_trailing_ws_advance
        let pos3 = builder.use_var(cursor.pos);
        let have_bytes3 = builder
            .ins()
            .icmp(IntCC::UnsignedLessThan, pos3, cursor.len);
        // If EOF after '[', that's OK - seq_is_end will catch the missing ']'
        builder
            .ins()
            .brif(have_bytes3, check_trailing_ws, &[], merge, &[]);

        builder.switch_to_block(check_trailing_ws);
        builder.seal_block(check_trailing_ws);
        let addr3 = builder.ins().iadd(cursor.input_ptr, pos3);
        let byte3 = builder.ins().load(types::I8, MemFlags::trusted(), addr3, 0);

        let is_space3 = builder.ins().icmp(IntCC::Equal, byte3, space);
        let is_tab3 = builder.ins().icmp(IntCC::Equal, byte3, tab);
        let is_newline3 = builder.ins().icmp(IntCC::Equal, byte3, newline);
        let is_cr3 = builder.ins().icmp(IntCC::Equal, byte3, cr);
        let is_ws3_1 = builder.ins().bor(is_space3, is_tab3);
        let is_ws3_2 = builder.ins().bor(is_newline3, is_cr3);
        let is_ws3 = builder.ins().bor(is_ws3_1, is_ws3_2);

        builder
            .ins()
            .brif(is_ws3, skip_trailing_ws_advance, &[], merge, &[]);

        builder.switch_to_block(skip_trailing_ws_advance);
        builder.seal_block(skip_trailing_ws_advance);
        let next_pos3 = builder.ins().iadd(pos3, one);
        builder.def_var(cursor.pos, next_pos3);
        builder.ins().jump(trailing_ws_check_bounds, &[]);

        // Seal loop header after back edge
        builder.seal_block(trailing_ws_check_bounds);

        // === Not bracket error ===
        builder.switch_to_block(not_bracket_error);
        builder.seal_block(not_bracket_error);
        let err_not_bracket = builder
            .ins()
            .iconst(types::I32, error::EXPECTED_ARRAY_START as i64);
        builder.def_var(result_error_var, err_not_bracket);
        builder.ins().jump(merge, &[]);

        // === EOF error ===
        builder.switch_to_block(eof_error);
        builder.seal_block(eof_error);
        let err_eof = builder
            .ins()
            .iconst(types::I32, error::UNEXPECTED_EOF as i64);
        builder.def_var(result_error_var, err_eof);
        builder.ins().jump(merge, &[]);

        // === Merge: return result ===
        builder.switch_to_block(merge);
        builder.seal_block(merge);
        let result_error = builder.use_var(result_error_var);

        // Return (count=0, error) - JSON doesn't know array length upfront
        (zero_count, result_error)
    }

    fn emit_seq_is_end(
        &self,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
        _state_ptr: Value,
    ) -> (Value, Value) {
        // Inline seq_is_end: check if current byte is ']'
        //
        // Returns (is_end: I8, error: I32)
        // is_end = 1 if we found ']', 0 otherwise
        // error = 0 on success, negative on error

        let pos = builder.use_var(cursor.pos);

        // Variables for results
        let result_is_end_var = builder.declare_var(types::I8);
        let result_error_var = builder.declare_var(types::I32);
        let zero_i8 = builder.ins().iconst(types::I8, 0);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        builder.def_var(result_is_end_var, zero_i8);
        builder.def_var(result_error_var, zero_i32);

        // Create blocks
        let check_byte = builder.create_block();
        let found_end = builder.create_block();
        let skip_ws_loop = builder.create_block();
        let skip_ws_check = builder.create_block();
        let not_end = builder.create_block();
        let eof_error = builder.create_block();
        let merge = builder.create_block();

        // Check if pos < len
        let have_bytes = builder.ins().icmp(IntCC::UnsignedLessThan, pos, cursor.len);
        builder
            .ins()
            .brif(have_bytes, check_byte, &[], eof_error, &[]);

        // check_byte: load byte and compare to ']'
        builder.switch_to_block(check_byte);
        builder.seal_block(check_byte);
        let addr = builder.ins().iadd(cursor.input_ptr, pos);
        let byte = builder.ins().load(types::I8, MemFlags::trusted(), addr, 0);
        let close_bracket = builder.ins().iconst(types::I8, b']' as i64);
        let is_close = builder.ins().icmp(IntCC::Equal, byte, close_bracket);
        builder.ins().brif(is_close, found_end, &[], not_end, &[]);

        // found_end: advance past ']' and skip whitespace
        builder.switch_to_block(found_end);
        builder.seal_block(found_end);
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let pos_after_bracket = builder.ins().iadd(pos, one);
        builder.def_var(cursor.pos, pos_after_bracket);
        builder.ins().jump(skip_ws_loop, &[]);

        // skip_ws_loop: loop header for whitespace skipping
        builder.switch_to_block(skip_ws_loop);
        // Don't seal yet - has back edge from skip_ws_check
        let ws_pos = builder.use_var(cursor.pos);
        let ws_have_bytes = builder
            .ins()
            .icmp(IntCC::UnsignedLessThan, ws_pos, cursor.len);
        let ws_check_char = builder.create_block();
        let ws_done = builder.create_block();
        builder
            .ins()
            .brif(ws_have_bytes, ws_check_char, &[], ws_done, &[]);

        // ws_check_char: check if current byte is whitespace
        builder.switch_to_block(ws_check_char);
        builder.seal_block(ws_check_char);
        let ws_addr = builder.ins().iadd(cursor.input_ptr, ws_pos);
        let ws_byte = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), ws_addr, 0);

        // Check for space, tab, newline, carriage return
        let space = builder.ins().iconst(types::I8, b' ' as i64);
        let tab = builder.ins().iconst(types::I8, b'\t' as i64);
        let newline = builder.ins().iconst(types::I8, b'\n' as i64);
        let cr = builder.ins().iconst(types::I8, b'\r' as i64);

        let is_space = builder.ins().icmp(IntCC::Equal, ws_byte, space);
        let is_tab = builder.ins().icmp(IntCC::Equal, ws_byte, tab);
        let is_newline = builder.ins().icmp(IntCC::Equal, ws_byte, newline);
        let is_cr = builder.ins().icmp(IntCC::Equal, ws_byte, cr);

        let is_ws_1 = builder.ins().bor(is_space, is_tab);
        let is_ws_2 = builder.ins().bor(is_newline, is_cr);
        let is_ws = builder.ins().bor(is_ws_1, is_ws_2);

        builder.ins().brif(is_ws, skip_ws_check, &[], ws_done, &[]);

        // skip_ws_check: advance and loop back
        builder.switch_to_block(skip_ws_check);
        builder.seal_block(skip_ws_check);
        let ws_next = builder.ins().iadd(ws_pos, one);
        builder.def_var(cursor.pos, ws_next);
        builder.ins().jump(skip_ws_loop, &[]);

        // Now seal skip_ws_loop since all predecessors (found_end, skip_ws_check) are declared
        builder.seal_block(skip_ws_loop);

        // ws_done: finished skipping whitespace, set is_end=true
        builder.switch_to_block(ws_done);
        builder.seal_block(ws_done);
        let one_i8 = builder.ins().iconst(types::I8, 1);
        builder.def_var(result_is_end_var, one_i8);
        builder.def_var(result_error_var, zero_i32);
        builder.ins().jump(merge, &[]);

        // not_end: byte is not ']', return is_end=false
        builder.switch_to_block(not_end);
        builder.seal_block(not_end);
        // result_is_end already 0, result_error already 0
        builder.ins().jump(merge, &[]);

        // eof_error: pos >= len, return error
        builder.switch_to_block(eof_error);
        builder.seal_block(eof_error);
        let eof_err = builder
            .ins()
            .iconst(types::I32, error::UNEXPECTED_EOF as i64);
        builder.def_var(result_error_var, eof_err);
        builder.ins().jump(merge, &[]);

        // merge: read results
        builder.switch_to_block(merge);
        builder.seal_block(merge);
        let result_is_end = builder.use_var(result_is_end_var);
        let result_error = builder.use_var(result_error_var);

        (result_is_end, result_error)
    }

    fn emit_seq_next(
        &self,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
        _state_ptr: Value,
    ) -> Value {
        // Inline seq_next: skip whitespace, then handle ',' or ']'
        //
        // Returns error code (I32): 0 on success, negative on error
        // - If we find ',', skip it and trailing whitespace, return success
        // - If we find ']', don't consume it (seq_is_end handles it), return success
        // - Otherwise return EXPECTED_COMMA_OR_END error
        //
        // Control flow:
        //   entry -> skip_leading_ws_loop
        //   skip_leading_ws_loop -> check_leading_ws | eof_error
        //   check_leading_ws -> skip_leading_ws_advance | check_separator
        //   skip_leading_ws_advance -> skip_leading_ws_loop (back edge)
        //   check_separator -> handle_comma | not_comma
        //   not_comma -> handle_close_bracket | unexpected_char
        //   handle_comma -> skip_trailing_ws_loop
        //   skip_trailing_ws_loop -> check_trailing_ws | merge
        //   check_trailing_ws -> skip_trailing_ws_advance | merge
        //   skip_trailing_ws_advance -> skip_trailing_ws_loop (back edge)
        //   handle_close_bracket -> merge
        //   unexpected_char -> merge (with error)
        //   eof_error -> merge (with error)

        // Result variable (0 = success)
        let result_error_var = builder.declare_var(types::I32);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        builder.def_var(result_error_var, zero_i32);

        let one = builder.ins().iconst(cursor.ptr_type, 1);

        // Whitespace constants (reused in both loops)
        let space = builder.ins().iconst(types::I8, b' ' as i64);
        let tab = builder.ins().iconst(types::I8, b'\t' as i64);
        let newline = builder.ins().iconst(types::I8, b'\n' as i64);
        let cr = builder.ins().iconst(types::I8, b'\r' as i64);

        // Create all blocks upfront
        let skip_leading_ws_loop = builder.create_block();
        let check_leading_ws = builder.create_block();
        let skip_leading_ws_advance = builder.create_block();
        let check_separator = builder.create_block();
        let not_comma = builder.create_block();
        let handle_comma = builder.create_block();
        let skip_trailing_ws_loop = builder.create_block();
        let check_trailing_ws = builder.create_block();
        let skip_trailing_ws_advance = builder.create_block();
        let handle_close_bracket = builder.create_block();
        let unexpected_char = builder.create_block();
        let eof_error = builder.create_block();
        let merge = builder.create_block();

        // Entry: jump to leading whitespace loop
        builder.ins().jump(skip_leading_ws_loop, &[]);

        // === Skip leading whitespace loop ===
        builder.switch_to_block(skip_leading_ws_loop);
        // Has back edge from skip_leading_ws_advance, seal after that block
        let pos = builder.use_var(cursor.pos);
        let have_bytes = builder.ins().icmp(IntCC::UnsignedLessThan, pos, cursor.len);
        builder
            .ins()
            .brif(have_bytes, check_leading_ws, &[], eof_error, &[]);

        builder.switch_to_block(check_leading_ws);
        builder.seal_block(check_leading_ws);
        let addr = builder.ins().iadd(cursor.input_ptr, pos);
        let byte = builder.ins().load(types::I8, MemFlags::trusted(), addr, 0);

        let is_space = builder.ins().icmp(IntCC::Equal, byte, space);
        let is_tab = builder.ins().icmp(IntCC::Equal, byte, tab);
        let is_newline = builder.ins().icmp(IntCC::Equal, byte, newline);
        let is_cr = builder.ins().icmp(IntCC::Equal, byte, cr);
        let is_ws_1 = builder.ins().bor(is_space, is_tab);
        let is_ws_2 = builder.ins().bor(is_newline, is_cr);
        let is_ws = builder.ins().bor(is_ws_1, is_ws_2);

        builder
            .ins()
            .brif(is_ws, skip_leading_ws_advance, &[], check_separator, &[]);

        builder.switch_to_block(skip_leading_ws_advance);
        builder.seal_block(skip_leading_ws_advance);
        let next_pos = builder.ins().iadd(pos, one);
        builder.def_var(cursor.pos, next_pos);
        builder.ins().jump(skip_leading_ws_loop, &[]);

        // Seal loop header after back edge is declared
        builder.seal_block(skip_leading_ws_loop);

        // === Check separator character ===
        builder.switch_to_block(check_separator);
        builder.seal_block(check_separator);
        // byte value is still valid from check_leading_ws
        let comma = builder.ins().iconst(types::I8, b',' as i64);
        let close_bracket = builder.ins().iconst(types::I8, b']' as i64);
        let is_comma = builder.ins().icmp(IntCC::Equal, byte, comma);

        builder
            .ins()
            .brif(is_comma, handle_comma, &[], not_comma, &[]);

        // not_comma: check if it's a close bracket
        builder.switch_to_block(not_comma);
        builder.seal_block(not_comma);
        let is_close = builder.ins().icmp(IntCC::Equal, byte, close_bracket);
        builder
            .ins()
            .brif(is_close, handle_close_bracket, &[], unexpected_char, &[]);

        // === Handle comma: advance past it and skip trailing whitespace ===
        builder.switch_to_block(handle_comma);
        builder.seal_block(handle_comma);
        let pos_after_comma = builder.ins().iadd(pos, one);
        builder.def_var(cursor.pos, pos_after_comma);
        builder.ins().jump(skip_trailing_ws_loop, &[]);

        // === Skip trailing whitespace loop ===
        builder.switch_to_block(skip_trailing_ws_loop);
        // Has back edge from skip_trailing_ws_advance, seal after that block
        let pos2 = builder.use_var(cursor.pos);
        let have_bytes2 = builder
            .ins()
            .icmp(IntCC::UnsignedLessThan, pos2, cursor.len);
        // If EOF after comma, that's OK - next call to seq_is_end will catch it
        builder
            .ins()
            .brif(have_bytes2, check_trailing_ws, &[], merge, &[]);

        builder.switch_to_block(check_trailing_ws);
        builder.seal_block(check_trailing_ws);
        let addr2 = builder.ins().iadd(cursor.input_ptr, pos2);
        let byte2 = builder.ins().load(types::I8, MemFlags::trusted(), addr2, 0);

        let is_space2 = builder.ins().icmp(IntCC::Equal, byte2, space);
        let is_tab2 = builder.ins().icmp(IntCC::Equal, byte2, tab);
        let is_newline2 = builder.ins().icmp(IntCC::Equal, byte2, newline);
        let is_cr2 = builder.ins().icmp(IntCC::Equal, byte2, cr);
        let is_ws2_1 = builder.ins().bor(is_space2, is_tab2);
        let is_ws2_2 = builder.ins().bor(is_newline2, is_cr2);
        let is_ws2 = builder.ins().bor(is_ws2_1, is_ws2_2);

        builder
            .ins()
            .brif(is_ws2, skip_trailing_ws_advance, &[], merge, &[]);

        builder.switch_to_block(skip_trailing_ws_advance);
        builder.seal_block(skip_trailing_ws_advance);
        let next_pos2 = builder.ins().iadd(pos2, one);
        builder.def_var(cursor.pos, next_pos2);
        builder.ins().jump(skip_trailing_ws_loop, &[]);

        // Seal loop header after back edge is declared
        builder.seal_block(skip_trailing_ws_loop);

        // === Handle close bracket: don't consume, return success ===
        builder.switch_to_block(handle_close_bracket);
        builder.seal_block(handle_close_bracket);
        // result_error already 0
        builder.ins().jump(merge, &[]);

        // === Unexpected character error ===
        builder.switch_to_block(unexpected_char);
        builder.seal_block(unexpected_char);
        let err_unexpected = builder
            .ins()
            .iconst(types::I32, error::EXPECTED_COMMA_OR_END as i64);
        builder.def_var(result_error_var, err_unexpected);
        builder.ins().jump(merge, &[]);

        // === EOF error (hit EOF while skipping leading whitespace) ===
        builder.switch_to_block(eof_error);
        builder.seal_block(eof_error);
        let err_eof = builder
            .ins()
            .iconst(types::I32, error::UNEXPECTED_EOF as i64);
        builder.def_var(result_error_var, err_eof);
        builder.ins().jump(merge, &[]);

        // === Merge: return result ===
        builder.switch_to_block(merge);
        builder.seal_block(merge);
        let result_error = builder.use_var(result_error_var);

        result_error
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
