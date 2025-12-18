//! Postcard-specific JIT format emitter.
//!
//! Implements `JitFormat` to generate Cranelift IR for direct postcard byte parsing.
//!
//! Postcard is a binary format with NO trivia (whitespace/comments), which means
//! `emit_skip_ws` and similar operations are no-ops. Sequences use length-prefix
//! encoding rather than delimiters, so "end" detection is state-based.

use facet_format::jit::{
    BlockArg, FunctionBuilder, InstBuilder, IntCC, JITBuilder, JitCursor, JitFormat,
    JitStringValue, MemFlags, Value, types,
};

use super::helpers;

/// Postcard format JIT emitter.
///
/// A zero-sized type that implements `JitFormat` for postcard binary syntax.
/// Helper functions are defined in this crate's `jit::helpers` module.
#[derive(Debug, Clone, Copy, Default)]
pub struct PostcardJitFormat;

/// Error codes for postcard JIT parsing.
pub mod error {
    pub use super::helpers::error::*;
}

impl PostcardJitFormat {
    /// Emit inline IR to decode a LEB128 varint.
    ///
    /// Returns `(value: i64, error: i32)` where:
    /// - `value` is the decoded u64 (as i64)
    /// - `error` is 0 on success, negative on error
    ///
    /// Updates `cursor.pos` to point past the varint.
    fn emit_varint_decode(builder: &mut FunctionBuilder, cursor: &mut JitCursor) -> (Value, Value) {
        // Variables for the loop
        let result_var = builder.declare_var(types::I64);
        let shift_var = builder.declare_var(types::I32);
        let error_var = builder.declare_var(types::I32);
        let value_var = builder.declare_var(types::I64);

        let zero_i64 = builder.ins().iconst(types::I64, 0);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        builder.def_var(result_var, zero_i64);
        builder.def_var(shift_var, zero_i32);
        builder.def_var(error_var, zero_i32);
        builder.def_var(value_var, zero_i64);

        // Create blocks
        let loop_header = builder.create_block();
        let load_byte = builder.create_block();
        let process_byte = builder.create_block();
        let check_continue = builder.create_block();
        let check_overflow = builder.create_block();
        let done = builder.create_block();
        let eof_error = builder.create_block();
        let overflow_error = builder.create_block();
        let merge = builder.create_block();

        builder.ins().jump(loop_header, &[]);

        // loop_header: check bounds
        builder.switch_to_block(loop_header);
        // Don't seal yet - has back edge from check_overflow

        let current_pos = builder.use_var(cursor.pos);
        let have_byte = builder
            .ins()
            .icmp(IntCC::UnsignedLessThan, current_pos, cursor.len);
        builder
            .ins()
            .brif(have_byte, load_byte, &[], eof_error, &[]);

        // load_byte: load byte and advance pos
        builder.switch_to_block(load_byte);
        builder.seal_block(load_byte);
        let addr = builder.ins().iadd(cursor.input_ptr, current_pos);
        let byte = builder.ins().load(types::I8, MemFlags::trusted(), addr, 0);
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let next_pos = builder.ins().iadd(current_pos, one);
        builder.def_var(cursor.pos, next_pos);
        builder.ins().jump(process_byte, &[]);

        // process_byte: extract data bits and add to result
        builder.switch_to_block(process_byte);
        builder.seal_block(process_byte);

        // data = byte & 0x7F (zero-extended to i64)
        let byte_i64 = builder.ins().uextend(types::I64, byte);
        let mask_7f = builder.ins().iconst(types::I64, 0x7F);
        let data = builder.ins().band(byte_i64, mask_7f);

        // result |= data << shift
        let shift = builder.use_var(shift_var);
        let shift_i64 = builder.ins().uextend(types::I64, shift);
        let shifted_data = builder.ins().ishl(data, shift_i64);
        let result = builder.use_var(result_var);
        let new_result = builder.ins().bor(result, shifted_data);
        builder.def_var(result_var, new_result);

        builder.ins().jump(check_continue, &[]);

        // check_continue: check continuation bit
        builder.switch_to_block(check_continue);
        builder.seal_block(check_continue);
        let mask_80 = builder.ins().iconst(types::I8, 0x80u8 as i64);
        let cont_bit = builder.ins().band(byte, mask_80);
        let has_more = builder.ins().icmp_imm(IntCC::NotEqual, cont_bit, 0);
        builder.ins().brif(has_more, check_overflow, &[], done, &[]);

        // check_overflow: increment shift and check for overflow
        builder.switch_to_block(check_overflow);
        builder.seal_block(check_overflow);
        let seven = builder.ins().iconst(types::I32, 7);
        let new_shift = builder.ins().iadd(shift, seven);
        builder.def_var(shift_var, new_shift);
        let overflow_limit = builder.ins().iconst(types::I32, 64);
        let is_overflow =
            builder
                .ins()
                .icmp(IntCC::UnsignedGreaterThanOrEqual, new_shift, overflow_limit);
        builder
            .ins()
            .brif(is_overflow, overflow_error, &[], loop_header, &[]);

        // Now seal loop_header since its back edge is declared
        builder.seal_block(loop_header);

        // eof_error
        builder.switch_to_block(eof_error);
        builder.seal_block(eof_error);
        let eof_err = builder
            .ins()
            .iconst(types::I32, error::UNEXPECTED_EOF as i64);
        builder.def_var(error_var, eof_err);
        builder.ins().jump(merge, &[]);

        // overflow_error
        builder.switch_to_block(overflow_error);
        builder.seal_block(overflow_error);
        let overflow_err = builder
            .ins()
            .iconst(types::I32, error::VARINT_OVERFLOW as i64);
        builder.def_var(error_var, overflow_err);
        builder.ins().jump(merge, &[]);

        // done: store final value
        builder.switch_to_block(done);
        builder.seal_block(done);
        let final_result = builder.use_var(result_var);
        builder.def_var(value_var, final_result);
        builder.ins().jump(merge, &[]);

        // merge: return value and error
        builder.switch_to_block(merge);
        builder.seal_block(merge);

        let value = builder.use_var(value_var);
        let err = builder.use_var(error_var);
        (value, err)
    }
}

impl JitFormat for PostcardJitFormat {
    fn register_helpers(builder: &mut JITBuilder) {
        // Register postcard-specific helper functions
        builder.symbol(
            "postcard_jit_read_varint",
            helpers::postcard_jit_read_varint as *const u8,
        );
        builder.symbol(
            "postcard_jit_seq_begin",
            helpers::postcard_jit_seq_begin as *const u8,
        );
        builder.symbol(
            "postcard_jit_seq_is_end",
            helpers::postcard_jit_seq_is_end as *const u8,
        );
        builder.symbol(
            "postcard_jit_seq_next",
            helpers::postcard_jit_seq_next as *const u8,
        );
        builder.symbol(
            "postcard_jit_parse_bool",
            helpers::postcard_jit_parse_bool as *const u8,
        );
    }

    fn helper_seq_begin() -> Option<&'static str> {
        Some("postcard_jit_seq_begin")
    }

    fn helper_seq_is_end() -> Option<&'static str> {
        Some("postcard_jit_seq_is_end")
    }

    fn helper_seq_next() -> Option<&'static str> {
        Some("postcard_jit_seq_next")
    }

    fn helper_parse_bool() -> Option<&'static str> {
        Some("postcard_jit_parse_bool")
    }

    // Postcard sequences need state for the remaining element count
    const SEQ_STATE_SIZE: u32 = 8; // u64 for remaining count
    const SEQ_STATE_ALIGN: u32 = 8;

    // Map state would also be needed if we support maps
    const MAP_STATE_SIZE: u32 = 8;
    const MAP_STATE_ALIGN: u32 = 8;

    fn emit_skip_ws(&self, builder: &mut FunctionBuilder, _cursor: &mut JitCursor) -> Value {
        // Postcard has NO trivia - this is a no-op
        builder.ins().iconst(types::I32, 0)
    }

    fn emit_skip_value(&self, builder: &mut FunctionBuilder, _cursor: &mut JitCursor) -> Value {
        // Not yet implemented
        builder.ins().iconst(types::I32, error::UNSUPPORTED as i64)
    }

    fn emit_peek_null(
        &self,
        builder: &mut FunctionBuilder,
        _cursor: &mut JitCursor,
    ) -> (Value, Value) {
        // Postcard doesn't have a null concept in the same way JSON does
        // (Options are encoded differently)
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
        // Postcard bool is a single byte: 0 = false, 1 = true
        //
        // Inline implementation:
        // 1. Check bounds (pos < len)
        // 2. Load byte at pos
        // 3. Check if 0 or 1
        // 4. Advance pos by 1

        let pos = builder.use_var(cursor.pos);

        // Variables to hold results (used across blocks)
        let result_value_var = builder.declare_var(types::I8);
        let result_error_var = builder.declare_var(types::I32);
        let zero_i8 = builder.ins().iconst(types::I8, 0);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        builder.def_var(result_value_var, zero_i8);
        builder.def_var(result_error_var, zero_i32);

        // Check bounds
        let have_byte = builder.ins().icmp(IntCC::UnsignedLessThan, pos, cursor.len);

        // Create blocks
        let check_byte = builder.create_block();
        let valid_false = builder.create_block();
        let check_true = builder.create_block();
        let valid_true = builder.create_block();
        let invalid_bool = builder.create_block();
        let eof_error = builder.create_block();
        let merge = builder.create_block();

        builder
            .ins()
            .brif(have_byte, check_byte, &[], eof_error, &[]);

        // eof_error: set error and jump to merge
        builder.switch_to_block(eof_error);
        builder.seal_block(eof_error);
        let eof_err = builder
            .ins()
            .iconst(types::I32, error::UNEXPECTED_EOF as i64);
        builder.def_var(result_error_var, eof_err);
        builder.ins().jump(merge, &[]);

        // check_byte: load byte and check value
        builder.switch_to_block(check_byte);
        builder.seal_block(check_byte);
        let addr = builder.ins().iadd(cursor.input_ptr, pos);
        let byte = builder.ins().load(types::I8, MemFlags::trusted(), addr, 0);

        // Check if byte == 0
        let is_zero = builder.ins().icmp_imm(IntCC::Equal, byte, 0);
        builder
            .ins()
            .brif(is_zero, valid_false, &[], check_true, &[]);

        // valid_false: value = 0, advance pos
        builder.switch_to_block(valid_false);
        builder.seal_block(valid_false);
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let new_pos = builder.ins().iadd(pos, one);
        builder.def_var(cursor.pos, new_pos);
        builder.def_var(result_value_var, zero_i8);
        builder.def_var(result_error_var, zero_i32);
        builder.ins().jump(merge, &[]);

        // check_true: check if byte == 1
        builder.switch_to_block(check_true);
        builder.seal_block(check_true);
        let is_one = builder.ins().icmp_imm(IntCC::Equal, byte, 1);
        builder
            .ins()
            .brif(is_one, valid_true, &[], invalid_bool, &[]);

        // valid_true: value = 1, advance pos
        builder.switch_to_block(valid_true);
        builder.seal_block(valid_true);
        let one_val = builder.ins().iconst(types::I8, 1);
        let one_ptr = builder.ins().iconst(cursor.ptr_type, 1);
        let new_pos = builder.ins().iadd(pos, one_ptr);
        builder.def_var(cursor.pos, new_pos);
        builder.def_var(result_value_var, one_val);
        builder.def_var(result_error_var, zero_i32);
        builder.ins().jump(merge, &[]);

        // invalid_bool: byte is not 0 or 1
        builder.switch_to_block(invalid_bool);
        builder.seal_block(invalid_bool);
        let invalid_err = builder.ins().iconst(types::I32, error::INVALID_BOOL as i64);
        builder.def_var(result_error_var, invalid_err);
        builder.ins().jump(merge, &[]);

        // merge: return results
        builder.switch_to_block(merge);
        builder.seal_block(merge);

        let final_value = builder.use_var(result_value_var);
        let final_error = builder.use_var(result_error_var);
        (final_value, final_error)
    }

    fn emit_parse_u8(
        &self,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
    ) -> (Value, Value) {
        // Postcard u8 is a single raw byte (NOT varint encoded).
        // Simply read one byte and advance position.

        let pos = builder.use_var(cursor.pos);

        // Variables to hold results
        let result_value_var = builder.declare_var(types::I8);
        let result_error_var = builder.declare_var(types::I32);
        let zero_i8 = builder.ins().iconst(types::I8, 0);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        builder.def_var(result_value_var, zero_i8);
        builder.def_var(result_error_var, zero_i32);

        // Check bounds
        let have_byte = builder.ins().icmp(IntCC::UnsignedLessThan, pos, cursor.len);

        // Create blocks
        let read_byte = builder.create_block();
        let eof_error = builder.create_block();
        let merge = builder.create_block();

        builder
            .ins()
            .brif(have_byte, read_byte, &[], eof_error, &[]);

        // eof_error: set error and jump to merge
        builder.switch_to_block(eof_error);
        builder.seal_block(eof_error);
        let eof_err = builder
            .ins()
            .iconst(types::I32, error::UNEXPECTED_EOF as i64);
        builder.def_var(result_error_var, eof_err);
        builder.ins().jump(merge, &[]);

        // read_byte: load byte, advance pos
        builder.switch_to_block(read_byte);
        builder.seal_block(read_byte);
        let addr = builder.ins().iadd(cursor.input_ptr, pos);
        let byte = builder.ins().load(types::I8, MemFlags::trusted(), addr, 0);
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let new_pos = builder.ins().iadd(pos, one);
        builder.def_var(cursor.pos, new_pos);
        builder.def_var(result_value_var, byte);
        builder.def_var(result_error_var, zero_i32);
        builder.ins().jump(merge, &[]);

        // merge: return results
        builder.switch_to_block(merge);
        builder.seal_block(merge);

        let final_value = builder.use_var(result_value_var);
        let final_error = builder.use_var(result_error_var);
        (final_value, final_error)
    }

    fn emit_parse_i64(
        &self,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
    ) -> (Value, Value) {
        // Postcard signed integers use ZigZag encoding on top of LEB128.
        // First decode the varint, then ZigZag decode: (n >> 1) ^ -(n & 1)
        let (varint_val, err) = Self::emit_varint_decode(builder, cursor);

        // ZigZag decode: (n >> 1) ^ -(n & 1)
        // This converts: 0->0, 1->-1, 2->1, 3->-2, 4->2, etc.
        let one = builder.ins().iconst(types::I64, 1);
        let shifted = builder.ins().ushr(varint_val, one); // n >> 1
        let sign_bit = builder.ins().band(varint_val, one); // n & 1
        let neg_sign = builder.ins().ineg(sign_bit); // -(n & 1)
        let decoded = builder.ins().bxor(shifted, neg_sign); // (n >> 1) ^ -(n & 1)

        (decoded, err)
    }

    fn emit_parse_u64(
        &self,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
    ) -> (Value, Value) {
        // Postcard unsigned integers are LEB128 varints
        Self::emit_varint_decode(builder, cursor)
    }

    fn emit_parse_f64(
        &self,
        builder: &mut FunctionBuilder,
        _cursor: &mut JitCursor,
    ) -> (Value, Value) {
        // Not yet implemented
        let zero = builder.ins().f64const(0.0);
        let err = builder.ins().iconst(types::I32, error::UNSUPPORTED as i64);
        (zero, err)
    }

    fn emit_parse_string(
        &self,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
    ) -> (JitStringValue, Value) {
        // Not yet implemented
        let null = builder.ins().iconst(cursor.ptr_type, 0);
        let zero = builder.ins().iconst(cursor.ptr_type, 0);
        let err = builder.ins().iconst(types::I32, error::UNSUPPORTED as i64);
        (
            JitStringValue {
                ptr: null,
                len: zero,
                cap: zero,
                owned: builder.ins().iconst(types::I8, 0),
            },
            err,
        )
    }

    fn emit_seq_begin(
        &self,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
        state_ptr: Value,
    ) -> (Value, Value) {
        // Postcard sequences are length-prefixed with a varint.
        // Read the varint and store the count in state_ptr.
        let (count, err) = Self::emit_varint_decode(builder, cursor);

        // Store count to state_ptr (only meaningful if err == 0, but always store)
        builder
            .ins()
            .store(MemFlags::trusted(), count, state_ptr, 0);

        // Return (count, err) so the compiler can use count for preallocation
        (count, err)
    }

    fn emit_seq_is_end(
        &self,
        builder: &mut FunctionBuilder,
        _cursor: &mut JitCursor,
        state_ptr: Value,
    ) -> (Value, Value) {
        // For postcard, "end" is when the remaining count in state == 0
        // Load the count from state_ptr and check if it's zero

        let remaining = builder
            .ins()
            .load(types::I64, MemFlags::trusted(), state_ptr, 0);
        let is_zero = builder.ins().icmp_imm(IntCC::Equal, remaining, 0);
        // Convert bool to i8 using select
        let one_i8 = builder.ins().iconst(types::I8, 1);
        let zero_i8 = builder.ins().iconst(types::I8, 0);
        let is_end = builder.ins().select(is_zero, one_i8, zero_i8);
        let no_error = builder.ins().iconst(types::I32, 0);

        (is_end, no_error)
    }

    fn emit_seq_next(
        &self,
        builder: &mut FunctionBuilder,
        _cursor: &mut JitCursor,
        state_ptr: Value,
    ) -> Value {
        // Decrement the remaining count in state
        // Note: We don't touch any input bytes - postcard elements are back-to-back
        //
        // Safety check: verify remaining > 0 before decrementing.
        // The protocol should prevent this, but it's a cheap safety net.

        let remaining = builder
            .ins()
            .load(types::I64, MemFlags::trusted(), state_ptr, 0);

        // Check for underflow (remaining == 0)
        let is_zero = builder.ins().icmp_imm(IntCC::Equal, remaining, 0);

        let underflow_block = builder.create_block();
        let decrement_block = builder.create_block();
        let merge = builder.create_block();
        builder.append_block_param(merge, types::I32);

        builder
            .ins()
            .brif(is_zero, underflow_block, &[], decrement_block, &[]);

        // underflow_block: return error
        builder.switch_to_block(underflow_block);
        builder.seal_block(underflow_block);
        let underflow_err = builder
            .ins()
            .iconst(types::I32, error::SEQ_UNDERFLOW as i64);
        builder.ins().jump(merge, &[BlockArg::from(underflow_err)]);

        // decrement_block: decrement and store
        builder.switch_to_block(decrement_block);
        builder.seal_block(decrement_block);
        let one = builder.ins().iconst(types::I64, 1);
        let new_remaining = builder.ins().isub(remaining, one);
        builder
            .ins()
            .store(MemFlags::trusted(), new_remaining, state_ptr, 0);
        let success = builder.ins().iconst(types::I32, 0);
        builder.ins().jump(merge, &[BlockArg::from(success)]);

        // merge: return result
        builder.switch_to_block(merge);
        builder.seal_block(merge);

        builder.block_params(merge)[0]
    }

    fn emit_map_begin(
        &self,
        builder: &mut FunctionBuilder,
        _cursor: &mut JitCursor,
        _state_ptr: Value,
    ) -> Value {
        // Not yet implemented
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
        let null = builder.ins().iconst(cursor.ptr_type, 0);
        let zero = builder.ins().iconst(cursor.ptr_type, 0);
        let err = builder.ins().iconst(types::I32, error::UNSUPPORTED as i64);
        (
            JitStringValue {
                ptr: null,
                len: zero,
                cap: zero,
                owned: builder.ins().iconst(types::I8, 0),
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
