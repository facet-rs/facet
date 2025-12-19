//! JSON-specific JIT format emitter.
//!
//! Implements `JitFormat` to generate Cranelift IR for direct JSON byte parsing.
//!
//! The emit_* methods generate inline Cranelift IR for parsing operations,
//! eliminating function call overhead in the hot path.

use facet_format::jit::{
    AbiParam, CallConv, ExtFuncData, ExternalName, FunctionBuilder, InstBuilder, IntCC, JITBuilder,
    JITModule, JitCursor, JitFormat, JitStringValue, Linkage, MemFlags, Module, Signature,
    UserExternalName, Value, types,
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
        builder.symbol(
            "json_jit_parse_f64",
            helpers::json_jit_parse_f64 as *const u8,
        );
        builder.symbol(
            "json_jit_parse_f64_out",
            helpers::json_jit_parse_f64_out as *const u8,
        );
        builder.symbol(
            "json_jit_parse_string",
            helpers::json_jit_parse_string as *const u8,
        );
        builder.symbol(
            "json_jit_skip_value",
            helpers::json_jit_skip_value as *const u8,
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

    fn emit_skip_ws(
        &self,
        _module: &mut JITModule,
        builder: &mut FunctionBuilder,
        _cursor: &mut JitCursor,
    ) -> Value {
        // Return success - helpers handle whitespace internally
        builder.ins().iconst(types::I32, 0)
    }

    fn emit_skip_value(
        &self,
        module: &mut JITModule,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
    ) -> Value {
        // Call the json_jit_skip_value helper function
        // Signature: fn(input: *const u8, len: usize, pos: usize) -> JsonJitPosError
        // JsonJitPosError { new_pos: usize, error: i32 }

        let pos = builder.use_var(cursor.pos);

        // Create the helper signature
        let helper_sig = {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(cursor.ptr_type)); // input
            sig.params.push(AbiParam::new(cursor.ptr_type)); // len
            sig.params.push(AbiParam::new(cursor.ptr_type)); // pos
            // Return: struct { new_pos: usize, error: i32 }
            sig.returns.push(AbiParam::new(cursor.ptr_type)); // new_pos
            sig.returns.push(AbiParam::new(types::I32)); // error
            sig
        };

        // Declare the function in the module
        let helper_func_id = module
            .declare_function("json_jit_skip_value", Linkage::Import, &helper_sig)
            .expect("failed to declare json_jit_skip_value");

        // Import it into this function
        let helper_ref = module.declare_func_in_func(helper_func_id, builder.func);

        // Call the helper
        let call = builder
            .ins()
            .call(helper_ref, &[cursor.input_ptr, cursor.len, pos]);
        let results = builder.inst_results(call);
        let new_pos = results[0];
        let error = results[1];

        // Update cursor position on success
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        let is_success = builder.ins().icmp(IntCC::Equal, error, zero_i32);

        let update_pos = builder.create_block();
        let merge = builder.create_block();

        builder.ins().brif(is_success, update_pos, &[], merge, &[]);

        builder.switch_to_block(update_pos);
        builder.seal_block(update_pos);
        builder.def_var(cursor.pos, new_pos);
        builder.ins().jump(merge, &[]);

        builder.switch_to_block(merge);
        builder.seal_block(merge);

        error
    }

    fn emit_peek_null(
        &self,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
    ) -> (Value, Value) {
        // Peek at whether the next value is "null" (don't consume)
        // "null" = 0x6e 0x75 0x6c 0x6c = little-endian u32: 0x6c6c756e
        //
        // Returns (is_null: i8, error: i32)
        // is_null = 1 if "null", 0 otherwise
        // error = 0 on success

        let pos = builder.use_var(cursor.pos);

        // Result variables
        let result_is_null_var = builder.declare_var(types::I8);
        let result_error_var = builder.declare_var(types::I32);
        let zero_i8 = builder.ins().iconst(types::I8, 0);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        builder.def_var(result_is_null_var, zero_i8);
        builder.def_var(result_error_var, zero_i32);

        // Check if we have at least 4 bytes
        let four = builder.ins().iconst(cursor.ptr_type, 4);
        let pos_plus_4 = builder.ins().iadd(pos, four);
        let have_4_bytes =
            builder
                .ins()
                .icmp(IntCC::UnsignedLessThanOrEqual, pos_plus_4, cursor.len);

        let check_null = builder.create_block();
        let not_enough_bytes = builder.create_block();
        let merge = builder.create_block();

        builder
            .ins()
            .brif(have_4_bytes, check_null, &[], not_enough_bytes, &[]);

        // check_null: load 4 bytes and compare to "null"
        builder.switch_to_block(check_null);
        builder.seal_block(check_null);
        let addr = builder.ins().iadd(cursor.input_ptr, pos);
        let word = builder.ins().load(types::I32, MemFlags::trusted(), addr, 0);
        let null_const = builder.ins().iconst(types::I32, 0x6c6c756ei64); // "null" LE
        let is_null = builder.ins().icmp(IntCC::Equal, word, null_const);
        let one_i8 = builder.ins().iconst(types::I8, 1);
        let is_null_val = builder.ins().select(is_null, one_i8, zero_i8);
        builder.def_var(result_is_null_var, is_null_val);
        builder.ins().jump(merge, &[]);

        // not_enough_bytes: not null (need at least 4 bytes)
        builder.switch_to_block(not_enough_bytes);
        builder.seal_block(not_enough_bytes);
        // result_is_null already 0, result_error already 0
        builder.ins().jump(merge, &[]);

        // merge: return results
        builder.switch_to_block(merge);
        builder.seal_block(merge);
        let result_is_null = builder.use_var(result_is_null_var);
        let result_error = builder.use_var(result_error_var);

        (result_is_null, result_error)
    }

    fn emit_consume_null(&self, builder: &mut FunctionBuilder, cursor: &mut JitCursor) -> Value {
        // Consume "null" (4 bytes) - called after emit_peek_null returned is_null=true
        // Just advance the cursor by 4
        let pos = builder.use_var(cursor.pos);
        let four = builder.ins().iconst(cursor.ptr_type, 4);
        let new_pos = builder.ins().iadd(pos, four);
        builder.def_var(cursor.pos, new_pos);

        // Return success
        builder.ins().iconst(types::I32, 0)
    }

    fn emit_parse_bool(
        &self,
        _module: &mut JITModule,
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
        _module: &mut JITModule,
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
        _module: &mut JITModule,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
    ) -> (Value, Value) {
        // Parse JSON integer: optional '-' followed by one or more digits
        //
        // Control flow:
        //   entry -> check_sign
        //   check_sign -> handle_minus | digit_loop
        //   handle_minus -> digit_loop
        //   digit_loop -> check_digit | eof_check
        //   check_digit -> accumulate | end_number
        //   accumulate -> digit_loop (back edge)
        //   end_number -> success | no_digits_error
        //   eof_check -> success (if has digits) | error
        //   no_digits_error -> merge
        //   success -> merge

        let pos = builder.use_var(cursor.pos);

        // Result variables
        let result_value_var = builder.declare_var(types::I64);
        let result_error_var = builder.declare_var(types::I32);
        let zero_i64 = builder.ins().iconst(types::I64, 0);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        builder.def_var(result_value_var, zero_i64);
        builder.def_var(result_error_var, zero_i32);

        // Accumulator for the parsed value
        let accum_var = builder.declare_var(types::I64);
        builder.def_var(accum_var, zero_i64);

        // Track if we're negative
        let is_neg_var = builder.declare_var(types::I8);
        let zero_i8 = builder.ins().iconst(types::I8, 0);
        builder.def_var(is_neg_var, zero_i8);

        // Track if we've seen at least one digit
        let has_digit_var = builder.declare_var(types::I8);
        builder.def_var(has_digit_var, zero_i8);

        // Position variable for the loop
        let loop_pos_var = builder.declare_var(cursor.ptr_type);
        builder.def_var(loop_pos_var, pos);

        // Constants
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let ten = builder.ins().iconst(types::I64, 10);
        let minus_char = builder.ins().iconst(types::I8, b'-' as i64);
        let zero_char = builder.ins().iconst(types::I8, b'0' as i64);
        let nine_char = builder.ins().iconst(types::I8, b'9' as i64);

        // Create blocks
        let check_sign = builder.create_block();
        let handle_minus = builder.create_block();
        let digit_loop = builder.create_block();
        let check_digit = builder.create_block();
        let accumulate = builder.create_block();
        let end_number = builder.create_block();
        let eof_at_start = builder.create_block();
        let no_digits_error = builder.create_block();
        let success = builder.create_block();
        let merge = builder.create_block();

        // Entry: check if we have any bytes
        let have_bytes = builder.ins().icmp(IntCC::UnsignedLessThan, pos, cursor.len);
        builder
            .ins()
            .brif(have_bytes, check_sign, &[], eof_at_start, &[]);

        // check_sign: look for '-'
        builder.switch_to_block(check_sign);
        builder.seal_block(check_sign);
        let addr = builder.ins().iadd(cursor.input_ptr, pos);
        let byte = builder.ins().load(types::I8, MemFlags::trusted(), addr, 0);
        let is_minus = builder.ins().icmp(IntCC::Equal, byte, minus_char);
        builder
            .ins()
            .brif(is_minus, handle_minus, &[], digit_loop, &[]);

        // handle_minus: set negative flag and advance
        builder.switch_to_block(handle_minus);
        builder.seal_block(handle_minus);
        let one_i8 = builder.ins().iconst(types::I8, 1);
        builder.def_var(is_neg_var, one_i8);
        let pos_after_minus = builder.ins().iadd(pos, one);
        builder.def_var(loop_pos_var, pos_after_minus);
        builder.ins().jump(digit_loop, &[]);

        // digit_loop: main parsing loop
        builder.switch_to_block(digit_loop);
        // Don't seal yet - has back edge from accumulate
        let loop_pos = builder.use_var(loop_pos_var);
        let in_bounds = builder
            .ins()
            .icmp(IntCC::UnsignedLessThan, loop_pos, cursor.len);
        builder
            .ins()
            .brif(in_bounds, check_digit, &[], end_number, &[]);

        // check_digit: is current byte a digit?
        builder.switch_to_block(check_digit);
        builder.seal_block(check_digit);
        let digit_addr = builder.ins().iadd(cursor.input_ptr, loop_pos);
        let digit_byte = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), digit_addr, 0);
        let ge_zero = builder
            .ins()
            .icmp(IntCC::SignedGreaterThanOrEqual, digit_byte, zero_char);
        let le_nine = builder
            .ins()
            .icmp(IntCC::SignedLessThanOrEqual, digit_byte, nine_char);
        let is_digit = builder.ins().band(ge_zero, le_nine);
        builder
            .ins()
            .brif(is_digit, accumulate, &[], end_number, &[]);

        // accumulate: accum = accum * 10 + (byte - '0')
        builder.switch_to_block(accumulate);
        builder.seal_block(accumulate);
        let accum = builder.use_var(accum_var);
        let digit_val = builder.ins().isub(digit_byte, zero_char);
        let digit_i64 = builder.ins().sextend(types::I64, digit_val);
        let accum_times_ten = builder.ins().imul(accum, ten);
        let new_accum = builder.ins().iadd(accum_times_ten, digit_i64);
        builder.def_var(accum_var, new_accum);
        // Mark that we have at least one digit
        builder.def_var(has_digit_var, one_i8);
        // Advance position
        let next_pos = builder.ins().iadd(loop_pos, one);
        builder.def_var(loop_pos_var, next_pos);
        builder.ins().jump(digit_loop, &[]);

        // Seal digit_loop after back edge
        builder.seal_block(digit_loop);

        // end_number: check if we have at least one digit
        builder.switch_to_block(end_number);
        builder.seal_block(end_number);
        let has_digit = builder.use_var(has_digit_var);
        let has_digit_bool = builder.ins().icmp_imm(IntCC::NotEqual, has_digit, 0);
        builder
            .ins()
            .brif(has_digit_bool, success, &[], no_digits_error, &[]);

        // no_digits_error: no digits found
        builder.switch_to_block(no_digits_error);
        builder.seal_block(no_digits_error);
        let err_no_digits = builder
            .ins()
            .iconst(types::I32, error::EXPECTED_NUMBER as i64);
        builder.def_var(result_error_var, err_no_digits);
        builder.ins().jump(merge, &[]);

        // eof_at_start: EOF before any content
        builder.switch_to_block(eof_at_start);
        builder.seal_block(eof_at_start);
        let err_eof = builder
            .ins()
            .iconst(types::I32, error::UNEXPECTED_EOF as i64);
        builder.def_var(result_error_var, err_eof);
        builder.ins().jump(merge, &[]);

        // success: apply sign and set result
        builder.switch_to_block(success);
        builder.seal_block(success);
        let final_accum = builder.use_var(accum_var);
        let is_neg = builder.use_var(is_neg_var);
        let is_neg_bool = builder.ins().icmp_imm(IntCC::NotEqual, is_neg, 0);
        let negated = builder.ins().ineg(final_accum);
        let final_value = builder.ins().select(is_neg_bool, negated, final_accum);
        builder.def_var(result_value_var, final_value);
        // Update cursor position
        let final_pos = builder.use_var(loop_pos_var);
        builder.def_var(cursor.pos, final_pos);
        builder.ins().jump(merge, &[]);

        // merge: return results
        builder.switch_to_block(merge);
        builder.seal_block(merge);
        let result_value = builder.use_var(result_value_var);
        let result_error = builder.use_var(result_error_var);

        (result_value, result_error)
    }

    fn emit_parse_u64(
        &self,
        _module: &mut JITModule,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
    ) -> (Value, Value) {
        // Parse JSON unsigned integer: one or more digits (no negative sign)
        //
        // Control flow:
        //   entry -> digit_loop | eof_at_start
        //   digit_loop -> check_digit | end_number
        //   check_digit -> accumulate | end_number
        //   accumulate -> digit_loop (back edge)
        //   end_number -> success | no_digits_error

        let pos = builder.use_var(cursor.pos);

        // Result variables
        let result_value_var = builder.declare_var(types::I64);
        let result_error_var = builder.declare_var(types::I32);
        let zero_i64 = builder.ins().iconst(types::I64, 0);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        builder.def_var(result_value_var, zero_i64);
        builder.def_var(result_error_var, zero_i32);

        // Accumulator for the parsed value
        let accum_var = builder.declare_var(types::I64);
        builder.def_var(accum_var, zero_i64);

        // Track if we've seen at least one digit
        let has_digit_var = builder.declare_var(types::I8);
        let zero_i8 = builder.ins().iconst(types::I8, 0);
        builder.def_var(has_digit_var, zero_i8);

        // Position variable for the loop
        let loop_pos_var = builder.declare_var(cursor.ptr_type);
        builder.def_var(loop_pos_var, pos);

        // Constants
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let one_i8 = builder.ins().iconst(types::I8, 1);
        let ten = builder.ins().iconst(types::I64, 10);
        let zero_char = builder.ins().iconst(types::I8, b'0' as i64);
        let nine_char = builder.ins().iconst(types::I8, b'9' as i64);

        // Create blocks
        let digit_loop = builder.create_block();
        let check_digit = builder.create_block();
        let accumulate = builder.create_block();
        let end_number = builder.create_block();
        let eof_at_start = builder.create_block();
        let no_digits_error = builder.create_block();
        let success = builder.create_block();
        let merge = builder.create_block();

        // Entry: check if we have any bytes
        let have_bytes = builder.ins().icmp(IntCC::UnsignedLessThan, pos, cursor.len);
        builder
            .ins()
            .brif(have_bytes, digit_loop, &[], eof_at_start, &[]);

        // digit_loop: main parsing loop
        builder.switch_to_block(digit_loop);
        // Don't seal yet - has back edge from accumulate
        let loop_pos = builder.use_var(loop_pos_var);
        let in_bounds = builder
            .ins()
            .icmp(IntCC::UnsignedLessThan, loop_pos, cursor.len);
        builder
            .ins()
            .brif(in_bounds, check_digit, &[], end_number, &[]);

        // check_digit: is current byte a digit?
        builder.switch_to_block(check_digit);
        builder.seal_block(check_digit);
        let digit_addr = builder.ins().iadd(cursor.input_ptr, loop_pos);
        let digit_byte = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), digit_addr, 0);
        let ge_zero = builder
            .ins()
            .icmp(IntCC::SignedGreaterThanOrEqual, digit_byte, zero_char);
        let le_nine = builder
            .ins()
            .icmp(IntCC::SignedLessThanOrEqual, digit_byte, nine_char);
        let is_digit = builder.ins().band(ge_zero, le_nine);
        builder
            .ins()
            .brif(is_digit, accumulate, &[], end_number, &[]);

        // accumulate: accum = accum * 10 + (byte - '0')
        builder.switch_to_block(accumulate);
        builder.seal_block(accumulate);
        let accum = builder.use_var(accum_var);
        let digit_val = builder.ins().isub(digit_byte, zero_char);
        let digit_i64 = builder.ins().uextend(types::I64, digit_val);
        let accum_times_ten = builder.ins().imul(accum, ten);
        let new_accum = builder.ins().iadd(accum_times_ten, digit_i64);
        builder.def_var(accum_var, new_accum);
        // Mark that we have at least one digit
        builder.def_var(has_digit_var, one_i8);
        // Advance position
        let next_pos = builder.ins().iadd(loop_pos, one);
        builder.def_var(loop_pos_var, next_pos);
        builder.ins().jump(digit_loop, &[]);

        // Seal digit_loop after back edge
        builder.seal_block(digit_loop);

        // end_number: check if we have at least one digit
        builder.switch_to_block(end_number);
        builder.seal_block(end_number);
        let has_digit = builder.use_var(has_digit_var);
        let has_digit_bool = builder.ins().icmp_imm(IntCC::NotEqual, has_digit, 0);
        builder
            .ins()
            .brif(has_digit_bool, success, &[], no_digits_error, &[]);

        // no_digits_error: no digits found
        builder.switch_to_block(no_digits_error);
        builder.seal_block(no_digits_error);
        let err_no_digits = builder
            .ins()
            .iconst(types::I32, error::EXPECTED_NUMBER as i64);
        builder.def_var(result_error_var, err_no_digits);
        builder.ins().jump(merge, &[]);

        // eof_at_start: EOF before any content
        builder.switch_to_block(eof_at_start);
        builder.seal_block(eof_at_start);
        let err_eof = builder
            .ins()
            .iconst(types::I32, error::UNEXPECTED_EOF as i64);
        builder.def_var(result_error_var, err_eof);
        builder.ins().jump(merge, &[]);

        // success: set result
        builder.switch_to_block(success);
        builder.seal_block(success);
        let final_accum = builder.use_var(accum_var);
        builder.def_var(result_value_var, final_accum);
        // Update cursor position
        let final_pos = builder.use_var(loop_pos_var);
        builder.def_var(cursor.pos, final_pos);
        builder.ins().jump(merge, &[]);

        // merge: return results
        builder.switch_to_block(merge);
        builder.seal_block(merge);
        let result_value = builder.use_var(result_value_var);
        let result_error = builder.use_var(result_error_var);

        (result_value, result_error)
    }

    fn emit_parse_f64(
        &self,
        module: &mut JITModule,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
    ) -> (Value, Value) {
        // Call the json_jit_parse_f64_out helper function
        // Signature: fn(out: *mut JsonJitF64Result, input: *const u8, len: usize, pos: usize)
        // JsonJitF64Result { new_pos: usize, value: f64, error: i32 }
        //
        // Uses output pointer to avoid ABI issues with f64 return values in Cranelift JIT.

        use facet_format::jit::{StackSlotData, StackSlotKind};

        let pos = builder.use_var(cursor.pos);

        // Allocate stack space for the result struct
        // JsonJitF64Result is: new_pos(8) + value(8) + error(4) + padding(4) = 24 bytes
        let result_slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 24, 8));
        let result_ptr = builder.ins().stack_addr(cursor.ptr_type, result_slot, 0);

        // Create the helper signature
        let helper_sig = {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(cursor.ptr_type)); // out
            sig.params.push(AbiParam::new(cursor.ptr_type)); // input
            sig.params.push(AbiParam::new(cursor.ptr_type)); // len
            sig.params.push(AbiParam::new(cursor.ptr_type)); // pos
            sig
        };

        // Declare the function in the module
        let helper_func_id = module
            .declare_function("json_jit_parse_f64_out", Linkage::Import, &helper_sig)
            .expect("failed to declare json_jit_parse_f64_out");

        // Import it into this function
        let helper_ref = module.declare_func_in_func(helper_func_id, builder.func);

        // Call the helper
        builder
            .ins()
            .call(helper_ref, &[result_ptr, cursor.input_ptr, cursor.len, pos]);

        // Load results from stack slot
        // Struct layout: new_pos at offset 0, value at offset 8, error at offset 16
        let new_pos = builder
            .ins()
            .load(cursor.ptr_type, MemFlags::trusted(), result_ptr, 0);
        let value = builder
            .ins()
            .load(types::F64, MemFlags::trusted(), result_ptr, 8);
        let error = builder
            .ins()
            .load(types::I32, MemFlags::trusted(), result_ptr, 16);

        // Update cursor position on success
        // We need to check error == 0 and only then update pos
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        let is_success = builder.ins().icmp(IntCC::Equal, error, zero_i32);

        let update_pos = builder.create_block();
        let merge = builder.create_block();

        builder.ins().brif(is_success, update_pos, &[], merge, &[]);

        builder.switch_to_block(update_pos);
        builder.seal_block(update_pos);
        builder.def_var(cursor.pos, new_pos);
        builder.ins().jump(merge, &[]);

        builder.switch_to_block(merge);
        builder.seal_block(merge);

        (value, error)
    }

    fn emit_parse_string(
        &self,
        module: &mut JITModule,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
    ) -> (JitStringValue, Value) {
        // Call the json_jit_parse_string helper function
        // Signature: fn(out: *mut JsonJitStringResult, input: *const u8, len: usize, pos: usize)
        // JsonJitStringResult { new_pos: usize, ptr: *const u8, len: usize, cap: usize, owned: u8, error: i32 }
        //
        // The struct is written to the output pointer to avoid ABI issues with large returns.

        use facet_format::jit::{StackSlotData, StackSlotKind};

        let pos = builder.use_var(cursor.pos);

        // Allocate stack space for the result struct
        // JsonJitStringResult is: new_pos(8) + ptr(8) + len(8) + cap(8) + owned(1) + padding(3) + error(4) = 40 bytes
        let result_slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 40, 8));
        let result_ptr = builder.ins().stack_addr(cursor.ptr_type, result_slot, 0);

        // Create the helper signature
        let helper_sig = {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(cursor.ptr_type)); // out
            sig.params.push(AbiParam::new(cursor.ptr_type)); // input
            sig.params.push(AbiParam::new(cursor.ptr_type)); // len
            sig.params.push(AbiParam::new(cursor.ptr_type)); // pos
            sig
        };

        // Declare the function in the module
        let helper_func_id = module
            .declare_function("json_jit_parse_string", Linkage::Import, &helper_sig)
            .expect("failed to declare json_jit_parse_string");

        // Import it into this function
        let helper_ref = module.declare_func_in_func(helper_func_id, builder.func);

        // Call the helper
        builder
            .ins()
            .call(helper_ref, &[result_ptr, cursor.input_ptr, cursor.len, pos]);

        // Load fields from the result struct
        // Offsets: new_pos=0, ptr=8, len=16, cap=24, owned=32, error=36
        let new_pos = builder
            .ins()
            .load(cursor.ptr_type, MemFlags::trusted(), result_ptr, 0);
        let str_ptr = builder
            .ins()
            .load(cursor.ptr_type, MemFlags::trusted(), result_ptr, 8);
        let str_len = builder
            .ins()
            .load(cursor.ptr_type, MemFlags::trusted(), result_ptr, 16);
        let str_cap = builder
            .ins()
            .load(cursor.ptr_type, MemFlags::trusted(), result_ptr, 24);
        let str_owned = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), result_ptr, 32);
        let error = builder
            .ins()
            .load(types::I32, MemFlags::trusted(), result_ptr, 36);

        // Update cursor position on success
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        let is_success = builder.ins().icmp(IntCC::Equal, error, zero_i32);

        let update_pos = builder.create_block();
        let merge = builder.create_block();

        builder.ins().brif(is_success, update_pos, &[], merge, &[]);

        builder.switch_to_block(update_pos);
        builder.seal_block(update_pos);
        builder.def_var(cursor.pos, new_pos);
        builder.ins().jump(merge, &[]);

        builder.switch_to_block(merge);
        builder.seal_block(merge);

        (
            JitStringValue {
                ptr: str_ptr,
                len: str_len,
                cap: str_cap,
                owned: str_owned,
            },
            error,
        )
    }

    fn emit_seq_begin(
        &self,
        _module: &mut JITModule,
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
        _module: &mut JITModule,
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
        _module: &mut JITModule,
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
        _module: &mut JITModule,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
        _state_ptr: Value,
    ) -> Value {
        // Inline map_begin: skip whitespace, expect '{', skip whitespace after
        //
        // Returns error code (I32): 0 on success, negative on error
        //
        // Control flow mirrors emit_seq_begin:
        //   entry -> skip_leading_ws_loop
        //   skip_leading_ws_loop -> check_leading_ws | eof_error
        //   check_leading_ws -> skip_leading_ws_advance | check_brace
        //   skip_leading_ws_advance -> skip_leading_ws_loop (back edge)
        //   check_brace -> skip_trailing_ws_loop | not_brace_error
        //   skip_trailing_ws_loop -> check_trailing_ws | merge (success)
        //   check_trailing_ws -> skip_trailing_ws_advance | merge (success)
        //   skip_trailing_ws_advance -> skip_trailing_ws_loop (back edge)
        //   eof_error -> merge (with error)
        //   not_brace_error -> merge (with error)

        // Result variable (0 = success)
        let result_error_var = builder.declare_var(types::I32);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
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
        let check_brace = builder.create_block();
        let skip_trailing_ws_loop = builder.create_block();
        let check_trailing_ws = builder.create_block();
        let skip_trailing_ws_advance = builder.create_block();
        let not_brace_error = builder.create_block();
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
            .brif(is_ws, skip_leading_ws_advance, &[], check_brace, &[]);

        builder.switch_to_block(skip_leading_ws_advance);
        builder.seal_block(skip_leading_ws_advance);
        let next_pos = builder.ins().iadd(pos, one);
        builder.def_var(cursor.pos, next_pos);
        builder.ins().jump(skip_leading_ws_loop, &[]);

        // Seal loop header after back edge
        builder.seal_block(skip_leading_ws_loop);

        // === Check for '{' ===
        builder.switch_to_block(check_brace);
        builder.seal_block(check_brace);
        let open_brace = builder.ins().iconst(types::I8, b'{' as i64);
        let is_brace = builder.ins().icmp(IntCC::Equal, byte, open_brace);
        builder
            .ins()
            .brif(is_brace, skip_trailing_ws_loop, &[], not_brace_error, &[]);

        // === Advance past '{' and skip trailing whitespace ===
        builder.switch_to_block(skip_trailing_ws_loop);
        builder.seal_block(skip_trailing_ws_loop);
        let pos2 = builder.use_var(cursor.pos);
        let pos_after_brace = builder.ins().iadd(pos2, one);
        builder.def_var(cursor.pos, pos_after_brace);

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
        // If EOF after '{', that's OK - map_is_end will catch the missing '}'
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

        // === Not brace error ===
        builder.switch_to_block(not_brace_error);
        builder.seal_block(not_brace_error);
        let err_not_brace = builder
            .ins()
            .iconst(types::I32, error::EXPECTED_OBJECT_START as i64);
        builder.def_var(result_error_var, err_not_brace);
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
        builder.use_var(result_error_var)
    }

    fn emit_map_is_end(
        &self,
        _module: &mut JITModule,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
        _state_ptr: Value,
    ) -> (Value, Value) {
        // Inline map_is_end: check if current byte is '}'
        //
        // Returns (is_end: I8, error: I32)
        // is_end = 1 if we found '}', 0 otherwise
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

        // check_byte: load byte and compare to '}'
        builder.switch_to_block(check_byte);
        builder.seal_block(check_byte);
        let addr = builder.ins().iadd(cursor.input_ptr, pos);
        let byte = builder.ins().load(types::I8, MemFlags::trusted(), addr, 0);
        let close_brace = builder.ins().iconst(types::I8, b'}' as i64);
        let is_close = builder.ins().icmp(IntCC::Equal, byte, close_brace);
        builder.ins().brif(is_close, found_end, &[], not_end, &[]);

        // found_end: advance past '}' and skip whitespace
        builder.switch_to_block(found_end);
        builder.seal_block(found_end);
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let pos_after_brace = builder.ins().iadd(pos, one);
        builder.def_var(cursor.pos, pos_after_brace);
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

        // Now seal skip_ws_loop since all predecessors are declared
        builder.seal_block(skip_ws_loop);

        // ws_done: finished skipping whitespace, set is_end=true
        builder.switch_to_block(ws_done);
        builder.seal_block(ws_done);
        let one_i8 = builder.ins().iconst(types::I8, 1);
        builder.def_var(result_is_end_var, one_i8);
        builder.def_var(result_error_var, zero_i32);
        builder.ins().jump(merge, &[]);

        // not_end: byte is not '}', return is_end=false
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

    fn emit_map_read_key(
        &self,
        module: &mut JITModule,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
        _state_ptr: Value,
    ) -> (JitStringValue, Value) {
        // In JSON, object keys are always strings.
        // We can directly reuse emit_parse_string.
        self.emit_parse_string(module, builder, cursor)
    }

    fn emit_map_kv_sep(
        &self,
        _module: &mut JITModule,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
        _state_ptr: Value,
    ) -> Value {
        // Inline map_kv_sep: skip whitespace, expect ':', skip whitespace after
        //
        // Returns error code (I32): 0 on success, negative on error
        //
        // Control flow:
        //   entry -> skip_leading_ws_loop
        //   skip_leading_ws_loop -> check_leading_ws | eof_error
        //   check_leading_ws -> skip_leading_ws_advance | check_colon
        //   skip_leading_ws_advance -> skip_leading_ws_loop (back edge)
        //   check_colon -> skip_trailing_ws_loop | not_colon_error
        //   skip_trailing_ws_loop -> check_trailing_ws | merge (success)
        //   check_trailing_ws -> skip_trailing_ws_advance | merge (success)
        //   skip_trailing_ws_advance -> skip_trailing_ws_loop (back edge)
        //   eof_error -> merge (with error)
        //   not_colon_error -> merge (with error)

        // Result variable (0 = success)
        let result_error_var = builder.declare_var(types::I32);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
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
        let check_colon = builder.create_block();
        let skip_trailing_ws_loop = builder.create_block();
        let check_trailing_ws = builder.create_block();
        let skip_trailing_ws_advance = builder.create_block();
        let not_colon_error = builder.create_block();
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
            .brif(is_ws, skip_leading_ws_advance, &[], check_colon, &[]);

        builder.switch_to_block(skip_leading_ws_advance);
        builder.seal_block(skip_leading_ws_advance);
        let next_pos = builder.ins().iadd(pos, one);
        builder.def_var(cursor.pos, next_pos);
        builder.ins().jump(skip_leading_ws_loop, &[]);

        // Seal loop header after back edge
        builder.seal_block(skip_leading_ws_loop);

        // === Check for ':' ===
        builder.switch_to_block(check_colon);
        builder.seal_block(check_colon);
        let colon = builder.ins().iconst(types::I8, b':' as i64);
        let is_colon = builder.ins().icmp(IntCC::Equal, byte, colon);
        builder
            .ins()
            .brif(is_colon, skip_trailing_ws_loop, &[], not_colon_error, &[]);

        // === Advance past ':' and skip trailing whitespace ===
        builder.switch_to_block(skip_trailing_ws_loop);
        builder.seal_block(skip_trailing_ws_loop);
        let pos2 = builder.use_var(cursor.pos);
        let pos_after_colon = builder.ins().iadd(pos2, one);
        builder.def_var(cursor.pos, pos_after_colon);

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
        // If EOF after ':', that's an error (value expected), but we let the value parser catch it
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

        // === Not colon error ===
        builder.switch_to_block(not_colon_error);
        builder.seal_block(not_colon_error);
        let err_not_colon = builder
            .ins()
            .iconst(types::I32, error::EXPECTED_COLON as i64);
        builder.def_var(result_error_var, err_not_colon);
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
        builder.use_var(result_error_var)
    }

    fn emit_map_next(
        &self,
        _module: &mut JITModule,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
        _state_ptr: Value,
    ) -> Value {
        // Inline map_next: skip whitespace, then handle ',' or '}'
        //
        // Returns error code (I32): 0 on success, negative on error
        // - If we find ',', skip it and trailing whitespace, return success
        // - If we find '}', don't consume it (map_is_end handles it), return success
        // - Otherwise return EXPECTED_COMMA_OR_BRACE error
        //
        // Control flow mirrors emit_seq_next:
        //   entry -> skip_leading_ws_loop
        //   skip_leading_ws_loop -> check_leading_ws | eof_error
        //   check_leading_ws -> skip_leading_ws_advance | check_separator
        //   skip_leading_ws_advance -> skip_leading_ws_loop (back edge)
        //   check_separator -> handle_comma | not_comma
        //   not_comma -> handle_close_brace | unexpected_char
        //   handle_comma -> skip_trailing_ws_loop
        //   skip_trailing_ws_loop -> check_trailing_ws | merge
        //   check_trailing_ws -> skip_trailing_ws_advance | merge
        //   skip_trailing_ws_advance -> skip_trailing_ws_loop (back edge)
        //   handle_close_brace -> merge
        //   unexpected_char -> merge (with error)
        //   eof_error -> merge (with error)

        // Result variable (0 = success)
        let result_error_var = builder.declare_var(types::I32);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        builder.def_var(result_error_var, zero_i32);

        let one = builder.ins().iconst(cursor.ptr_type, 1);

        // Whitespace constants
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
        let handle_close_brace = builder.create_block();
        let unexpected_char = builder.create_block();
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
        let comma = builder.ins().iconst(types::I8, b',' as i64);
        let close_brace = builder.ins().iconst(types::I8, b'}' as i64);
        let is_comma = builder.ins().icmp(IntCC::Equal, byte, comma);

        builder
            .ins()
            .brif(is_comma, handle_comma, &[], not_comma, &[]);

        // not_comma: check if it's a close brace
        builder.switch_to_block(not_comma);
        builder.seal_block(not_comma);
        let is_close = builder.ins().icmp(IntCC::Equal, byte, close_brace);
        builder
            .ins()
            .brif(is_close, handle_close_brace, &[], unexpected_char, &[]);

        // === Handle comma: advance past it and skip trailing whitespace ===
        builder.switch_to_block(handle_comma);
        builder.seal_block(handle_comma);
        let pos_after_comma = builder.ins().iadd(pos, one);
        builder.def_var(cursor.pos, pos_after_comma);
        builder.ins().jump(skip_trailing_ws_loop, &[]);

        // === Skip trailing whitespace loop ===
        builder.switch_to_block(skip_trailing_ws_loop);
        // Has back edge from skip_trailing_ws_advance
        let pos2 = builder.use_var(cursor.pos);
        let have_bytes2 = builder
            .ins()
            .icmp(IntCC::UnsignedLessThan, pos2, cursor.len);
        // If EOF after comma, that's OK - next call to map_is_end will catch it
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

        // === Handle close brace: don't consume, return success ===
        builder.switch_to_block(handle_close_brace);
        builder.seal_block(handle_close_brace);
        // result_error already 0
        builder.ins().jump(merge, &[]);

        // === Unexpected character error ===
        builder.switch_to_block(unexpected_char);
        builder.seal_block(unexpected_char);
        let err_unexpected = builder
            .ins()
            .iconst(types::I32, error::EXPECTED_COMMA_OR_BRACE as i64);
        builder.def_var(result_error_var, err_unexpected);
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
        builder.use_var(result_error_var)
    }
}
