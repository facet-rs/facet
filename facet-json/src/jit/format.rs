//! JSON-specific JIT format emitter.
//!
//! Implements `JitFormat` to generate Cranelift IR for direct JSON byte parsing.
//!
//! The emit_* methods generate inline Cranelift IR for parsing operations,
//! eliminating function call overhead in the hot path.

use facet_format::jit::{
    AbiParam, FunctionBuilder, InstBuilder, IntCC, JITBuilder, JITModule, JitCursor, JitFormat,
    JitStringValue, MemFlags, Module, Value, c_call_conv, types,
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
            "json_jit_parse_i64",
            helpers::json_jit_parse_i64 as *const u8,
        );
        builder.symbol(
            "json_jit_parse_u64",
            helpers::json_jit_parse_u64 as *const u8,
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
        // Inline string parser helpers
        builder.symbol(
            "json_jit_memchr2_quote_backslash",
            helpers::json_jit_memchr2_quote_backslash as *const u8,
        );
        builder.symbol(
            "json_jit_scratch_take",
            helpers::json_jit_scratch_take as *const u8,
        );
        builder.symbol(
            "json_jit_scratch_extend",
            helpers::json_jit_scratch_extend as *const u8,
        );
        builder.symbol(
            "json_jit_scratch_push_byte",
            helpers::json_jit_scratch_push_byte as *const u8,
        );
        builder.symbol(
            "json_jit_decode_unicode_escape",
            helpers::json_jit_decode_unicode_escape as *const u8,
        );
        builder.symbol(
            "json_jit_scratch_finalize_string",
            helpers::json_jit_scratch_finalize_string as *const u8,
        );
        builder.symbol("json_jit_is_ascii", helpers::json_jit_is_ascii as *const u8);
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
        // Signature: fn(input: *const u8, len: usize, pos: usize) -> isize
        // Returns: new_pos on success (>= 0), error code on failure (< 0)

        let pos = builder.use_var(cursor.pos);

        // Create the helper signature
        // IMPORTANT: Set calling convention to match `extern "C"` on this platform
        let helper_sig = {
            let mut sig = module.make_signature();
            sig.call_conv = c_call_conv();
            sig.params.push(AbiParam::new(cursor.ptr_type)); // input
            sig.params.push(AbiParam::new(cursor.ptr_type)); // len
            sig.params.push(AbiParam::new(cursor.ptr_type)); // pos
            // Return: isize (new_pos if >= 0, error if < 0)
            sig.returns.push(AbiParam::new(cursor.ptr_type));
            sig
        };
        let helper_sig_ref = builder.import_signature(helper_sig);
        let helper_ptr = builder.ins().iconst(
            cursor.ptr_type,
            helpers::json_jit_skip_value as *const u8 as i64,
        );

        // Call the helper
        let call = builder.ins().call_indirect(
            helper_sig_ref,
            helper_ptr,
            &[cursor.input_ptr, cursor.len, pos],
        );
        let result = builder.inst_results(call)[0];

        // Check if result >= 0 (success) or < 0 (error)
        let zero = builder.ins().iconst(cursor.ptr_type, 0);
        let is_success = builder
            .ins()
            .icmp(IntCC::SignedGreaterThanOrEqual, result, zero);

        let update_pos = builder.create_block();
        let merge = builder.create_block();

        builder.ins().brif(is_success, update_pos, &[], merge, &[]);

        builder.switch_to_block(update_pos);
        builder.seal_block(update_pos);
        builder.def_var(cursor.pos, result); // result IS the new_pos on success
        builder.ins().jump(merge, &[]);

        builder.switch_to_block(merge);
        builder.seal_block(merge);

        // Return error code: 0 on success, negative on failure
        // If success, result >= 0, so we return 0
        // If failure, result < 0, so we return result (the error code)
        let error = builder.ins().select(is_success, zero, result);
        // Truncate to i32 for error code
        builder.ins().ireduce(types::I32, error)
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
        module: &mut JITModule,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
    ) -> (Value, Value) {
        // Call the optimized json_jit_parse_i64 helper
        // Signature: fn(out: *mut JsonJitI64Result, input: *const u8, len: usize, pos: usize)
        //  where JsonJitI64Result = { new_pos: usize, value: i64, error: i32 }

        use facet_format::jit::{StackSlotData, StackSlotKind};

        let sig = {
            let mut s = module.make_signature();
            s.call_conv = c_call_conv();
            s.params.push(AbiParam::new(cursor.ptr_type)); // out
            s.params.push(AbiParam::new(cursor.ptr_type)); // input
            s.params.push(AbiParam::new(cursor.ptr_type)); // len
            s.params.push(AbiParam::new(cursor.ptr_type)); // pos
            s
        };
        let sig_ref = builder.import_signature(sig);
        let callee_ptr = builder.ins().iconst(
            cursor.ptr_type,
            helpers::json_jit_parse_i64 as *const u8 as i64,
        );

        // Allocate stack space for result: new_pos(8) + value(8) + error(4) + padding(4) = 24 bytes
        let result_slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 24, 8));
        let result_ptr = builder.ins().stack_addr(cursor.ptr_type, result_slot, 0);

        let pos = builder.use_var(cursor.pos);
        builder.ins().call_indirect(
            sig_ref,
            callee_ptr,
            &[result_ptr, cursor.input_ptr, cursor.len, pos],
        );

        // Load results from stack slot
        let new_pos = builder
            .ins()
            .load(cursor.ptr_type, MemFlags::trusted(), result_ptr, 0);
        let value = builder
            .ins()
            .load(types::I64, MemFlags::trusted(), result_ptr, 8);
        let error = builder
            .ins()
            .load(types::I32, MemFlags::trusted(), result_ptr, 16);

        // Update cursor position
        builder.def_var(cursor.pos, new_pos);

        (value, error)
    }

    fn emit_parse_u64(
        &self,
        module: &mut JITModule,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
    ) -> (Value, Value) {
        // Call the optimized json_jit_parse_u64 helper
        // Signature: fn(out: *mut JsonJitI64Result, input: *const u8, len: usize, pos: usize)
        //  where JsonJitI64Result = { new_pos: usize, value: i64, error: i32 }

        use facet_format::jit::{StackSlotData, StackSlotKind};

        let sig = {
            let mut s = module.make_signature();
            s.call_conv = c_call_conv();
            s.params.push(AbiParam::new(cursor.ptr_type)); // out
            s.params.push(AbiParam::new(cursor.ptr_type)); // input
            s.params.push(AbiParam::new(cursor.ptr_type)); // len
            s.params.push(AbiParam::new(cursor.ptr_type)); // pos
            s
        };
        let sig_ref = builder.import_signature(sig);
        let callee_ptr = builder.ins().iconst(
            cursor.ptr_type,
            helpers::json_jit_parse_u64 as *const u8 as i64,
        );

        // Allocate stack space for result: new_pos(8) + value(8) + error(4) + padding(4) = 24 bytes
        let result_slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 24, 8));
        let result_ptr = builder.ins().stack_addr(cursor.ptr_type, result_slot, 0);

        let pos = builder.use_var(cursor.pos);
        builder.ins().call_indirect(
            sig_ref,
            callee_ptr,
            &[result_ptr, cursor.input_ptr, cursor.len, pos],
        );

        // Load results from stack slot
        let new_pos = builder
            .ins()
            .load(cursor.ptr_type, MemFlags::trusted(), result_ptr, 0);
        let value = builder
            .ins()
            .load(types::I64, MemFlags::trusted(), result_ptr, 8);
        let error = builder
            .ins()
            .load(types::I32, MemFlags::trusted(), result_ptr, 16);

        // Update cursor position
        builder.def_var(cursor.pos, new_pos);

        (value, error)
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
            sig.call_conv = c_call_conv();
            sig.params.push(AbiParam::new(cursor.ptr_type)); // out
            sig.params.push(AbiParam::new(cursor.ptr_type)); // input
            sig.params.push(AbiParam::new(cursor.ptr_type)); // len
            sig.params.push(AbiParam::new(cursor.ptr_type)); // pos
            sig
        };
        let helper_sig_ref = builder.import_signature(helper_sig);
        let helper_ptr = builder.ins().iconst(
            cursor.ptr_type,
            helpers::json_jit_parse_f64_out as *const u8 as i64,
        );

        // Call the helper
        builder.ins().call_indirect(
            helper_sig_ref,
            helper_ptr,
            &[result_ptr, cursor.input_ptr, cursor.len, pos],
        );

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
        // Just call the optimized json_jit_parse_string helper.
        // It's compiled by LLVM with all optimizations - no point reimplementing in Cranelift.
        //
        // Signature: fn(out: *mut JsonJitStringResult, input: *const u8, len: usize, pos: usize, scratch: *mut JitScratch)
        // JsonJitStringResult layout:
        //   offset 0:  new_pos (usize)
        //   offset 8:  ptr (*const u8)
        //   offset 16: len (usize)
        //   offset 24: cap (usize)
        //   offset 32: owned (u8)
        //   offset 36: error (i32)

        use facet_format::jit::{StackSlotData, StackSlotKind};

        let sig = {
            let mut s = module.make_signature();
            s.call_conv = c_call_conv();
            s.params.push(AbiParam::new(cursor.ptr_type)); // out
            s.params.push(AbiParam::new(cursor.ptr_type)); // input
            s.params.push(AbiParam::new(cursor.ptr_type)); // len
            s.params.push(AbiParam::new(cursor.ptr_type)); // pos
            s.params.push(AbiParam::new(cursor.ptr_type)); // scratch
            s
        };
        let sig_ref = builder.import_signature(sig);
        let callee_ptr = builder.ins().iconst(
            cursor.ptr_type,
            helpers::json_jit_parse_string as *const u8 as i64,
        );

        // Allocate stack space for result (40 bytes, 8-byte aligned)
        let result_slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 40, 8));
        let result_ptr = builder.ins().stack_addr(cursor.ptr_type, result_slot, 0);

        let pos = builder.use_var(cursor.pos);
        builder.ins().call_indirect(
            sig_ref,
            callee_ptr,
            &[
                result_ptr,
                cursor.input_ptr,
                cursor.len,
                pos,
                cursor.scratch_ptr,
            ],
        );

        // Load results from stack slot
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

        // Update cursor position
        builder.def_var(cursor.pos, new_pos);

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

        // Whitespace constants - only need space for fast path, others for slow path
        let space = builder.ins().iconst(types::I8, b' ' as i64);
        let tab = builder.ins().iconst(types::I8, b'\t' as i64);
        let newline = builder.ins().iconst(types::I8, b'\n' as i64);
        let cr = builder.ins().iconst(types::I8, b'\r' as i64);
        let const_32 = builder.ins().iconst(types::I8, 32);

        // Create blocks
        let skip_leading_ws_loop = builder.create_block();
        let check_leading_ws = builder.create_block();
        let maybe_leading_ws = builder.create_block();
        let check_leading_low_ws = builder.create_block();
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

        // Fast path: check if byte > 32 first (most common case - not whitespace)
        builder.switch_to_block(check_leading_ws);
        builder.seal_block(check_leading_ws);
        let addr = builder.ins().iadd(cursor.input_ptr, pos);
        let byte = builder.ins().load(types::I8, MemFlags::trusted(), addr, 0);

        let gt_32 = builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThan, byte, const_32);
        builder
            .ins()
            .brif(gt_32, check_bracket, &[], maybe_leading_ws, &[]);

        // Byte <= 32: check if it's space (most common whitespace)
        builder.switch_to_block(maybe_leading_ws);
        builder.seal_block(maybe_leading_ws);
        let is_space = builder.ins().icmp(IntCC::Equal, byte, space);
        builder.ins().brif(
            is_space,
            skip_leading_ws_advance,
            &[],
            check_leading_low_ws,
            &[],
        );

        // Byte < 32: check tab/lf/cr (rare)
        builder.switch_to_block(check_leading_low_ws);
        builder.seal_block(check_leading_low_ws);
        let is_tab = builder.ins().icmp(IntCC::Equal, byte, tab);
        let is_newline = builder.ins().icmp(IntCC::Equal, byte, newline);
        let is_cr = builder.ins().icmp(IntCC::Equal, byte, cr);
        let is_ws_1 = builder.ins().bor(is_tab, is_newline);
        let is_ws = builder.ins().bor(is_ws_1, is_cr);
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

        // Fast path: check if byte > 32 first
        let maybe_trailing_ws = builder.create_block();
        let check_trailing_low_ws = builder.create_block();

        builder.switch_to_block(check_trailing_ws);
        builder.seal_block(check_trailing_ws);
        let addr3 = builder.ins().iadd(cursor.input_ptr, pos3);
        let byte3 = builder.ins().load(types::I8, MemFlags::trusted(), addr3, 0);

        let gt_32_3 = builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThan, byte3, const_32);
        builder
            .ins()
            .brif(gt_32_3, merge, &[], maybe_trailing_ws, &[]);

        // Byte <= 32: check if it's space
        builder.switch_to_block(maybe_trailing_ws);
        builder.seal_block(maybe_trailing_ws);
        let is_space3 = builder.ins().icmp(IntCC::Equal, byte3, space);
        builder.ins().brif(
            is_space3,
            skip_trailing_ws_advance,
            &[],
            check_trailing_low_ws,
            &[],
        );

        // Byte < 32: check tab/lf/cr
        builder.switch_to_block(check_trailing_low_ws);
        builder.seal_block(check_trailing_low_ws);
        let is_tab3 = builder.ins().icmp(IntCC::Equal, byte3, tab);
        let is_newline3 = builder.ins().icmp(IntCC::Equal, byte3, newline);
        let is_cr3 = builder.ins().icmp(IntCC::Equal, byte3, cr);
        let is_ws3_1 = builder.ins().bor(is_tab3, is_newline3);
        let is_ws3 = builder.ins().bor(is_ws3_1, is_cr3);
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

        // ws_check_char: check if current byte is whitespace (fast path)
        let maybe_ws = builder.create_block();
        let check_low_ws = builder.create_block();

        builder.switch_to_block(ws_check_char);
        builder.seal_block(ws_check_char);
        let ws_addr = builder.ins().iadd(cursor.input_ptr, ws_pos);
        let ws_byte = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), ws_addr, 0);

        // Fast path: check if byte > 32 first
        let const_32 = builder.ins().iconst(types::I8, 32);
        let gt_32 = builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThan, ws_byte, const_32);
        builder.ins().brif(gt_32, ws_done, &[], maybe_ws, &[]);

        // Byte <= 32: check if it's space
        builder.switch_to_block(maybe_ws);
        builder.seal_block(maybe_ws);
        let space = builder.ins().iconst(types::I8, b' ' as i64);
        let is_space = builder.ins().icmp(IntCC::Equal, ws_byte, space);
        builder
            .ins()
            .brif(is_space, skip_ws_check, &[], check_low_ws, &[]);

        // Byte < 32: check tab/lf/cr
        builder.switch_to_block(check_low_ws);
        builder.seal_block(check_low_ws);
        let tab = builder.ins().iconst(types::I8, b'\t' as i64);
        let newline = builder.ins().iconst(types::I8, b'\n' as i64);
        let cr = builder.ins().iconst(types::I8, b'\r' as i64);
        let is_tab = builder.ins().icmp(IntCC::Equal, ws_byte, tab);
        let is_newline = builder.ins().icmp(IntCC::Equal, ws_byte, newline);
        let is_cr = builder.ins().icmp(IntCC::Equal, ws_byte, cr);
        let is_ws_1 = builder.ins().bor(is_tab, is_newline);
        let is_ws = builder.ins().bor(is_ws_1, is_cr);
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

        builder.use_var(result_error_var)
    }

    fn emit_try_empty_seq(
        &self,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
    ) -> Option<(Value, Value)> {
        // Fast path for empty JSON arrays: check for `[]` pattern
        //
        // This avoids the overhead of:
        // 1. Function call to list deserializer
        // 2. Vec initialization
        // 3. seq_begin + seq_is_end flow
        //
        // Returns (is_empty: i8, error: i32):
        // - If we find `[]`: consume it, skip trailing whitespace, return (1, 0)
        // - If not `[]`: leave cursor unchanged, return (0, 0)
        // - On error during whitespace skip: return (0, error)

        // Result variables
        let result_is_empty_var = builder.declare_var(types::I8);
        let result_error_var = builder.declare_var(types::I32);
        let zero_i8 = builder.ins().iconst(types::I8, 0);
        let one_i8 = builder.ins().iconst(types::I8, 1);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        builder.def_var(result_is_empty_var, zero_i8);
        builder.def_var(result_error_var, zero_i32);

        // Save original position (in case we need to restore)
        let orig_pos = builder.use_var(cursor.pos);
        let orig_pos_var = builder.declare_var(cursor.ptr_type);
        builder.def_var(orig_pos_var, orig_pos);

        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let two = builder.ins().iconst(cursor.ptr_type, 2);

        // Whitespace constants
        let space = builder.ins().iconst(types::I8, b' ' as i64);
        let tab = builder.ins().iconst(types::I8, b'\t' as i64);
        let newline = builder.ins().iconst(types::I8, b'\n' as i64);
        let cr = builder.ins().iconst(types::I8, b'\r' as i64);

        // Create blocks
        let skip_ws_loop = builder.create_block();
        let check_ws = builder.create_block();
        let skip_ws_advance = builder.create_block();
        let check_pattern = builder.create_block();
        let found_empty = builder.create_block();
        let skip_trailing_ws_loop = builder.create_block();
        let check_trailing_ws = builder.create_block();
        let skip_trailing_ws_advance = builder.create_block();
        let not_empty = builder.create_block();
        let merge = builder.create_block();

        // Entry: start skipping leading whitespace
        builder.ins().jump(skip_ws_loop, &[]);

        // === Skip leading whitespace ===
        builder.switch_to_block(skip_ws_loop);
        let pos = builder.use_var(cursor.pos);
        let have_bytes = builder.ins().icmp(IntCC::UnsignedLessThan, pos, cursor.len);
        // If EOF before finding `[`, not an empty array - just return false
        builder
            .ins()
            .brif(have_bytes, check_ws, &[], not_empty, &[]);

        builder.switch_to_block(check_ws);
        builder.seal_block(check_ws);
        let addr = builder.ins().iadd(cursor.input_ptr, pos);
        let byte = builder.ins().load(types::I8, MemFlags::trusted(), addr, 0);
        let is_space = builder.ins().icmp(IntCC::Equal, byte, space);
        let is_tab = builder.ins().icmp(IntCC::Equal, byte, tab);
        let is_newline = builder.ins().icmp(IntCC::Equal, byte, newline);
        let is_cr = builder.ins().icmp(IntCC::Equal, byte, cr);
        let is_ws1 = builder.ins().bor(is_space, is_tab);
        let is_ws2 = builder.ins().bor(is_newline, is_cr);
        let is_ws = builder.ins().bor(is_ws1, is_ws2);
        builder
            .ins()
            .brif(is_ws, skip_ws_advance, &[], check_pattern, &[]);

        builder.switch_to_block(skip_ws_advance);
        builder.seal_block(skip_ws_advance);
        let next_pos = builder.ins().iadd(pos, one);
        builder.def_var(cursor.pos, next_pos);
        builder.ins().jump(skip_ws_loop, &[]);
        builder.seal_block(skip_ws_loop);

        // === Check for `[]` pattern ===
        builder.switch_to_block(check_pattern);
        builder.seal_block(check_pattern);
        // Need 2 bytes: check pos + 2 <= len
        let pos2 = builder.use_var(cursor.pos);
        let end_pos = builder.ins().iadd(pos2, two);
        let have_two = builder
            .ins()
            .icmp(IntCC::UnsignedLessThanOrEqual, end_pos, cursor.len);
        let check_bytes = builder.create_block();
        builder
            .ins()
            .brif(have_two, check_bytes, &[], not_empty, &[]);

        builder.switch_to_block(check_bytes);
        builder.seal_block(check_bytes);
        // Load 2 bytes as i16: `[]` = 0x5D5B in little-endian (0x5B = '[', 0x5D = ']')
        let addr2 = builder.ins().iadd(cursor.input_ptr, pos2);
        let two_bytes = builder
            .ins()
            .load(types::I16, MemFlags::trusted(), addr2, 0);
        let empty_pattern = builder.ins().iconst(types::I16, 0x5D5B); // "[]" little-endian
        let is_empty = builder.ins().icmp(IntCC::Equal, two_bytes, empty_pattern);
        builder
            .ins()
            .brif(is_empty, found_empty, &[], not_empty, &[]);

        // === Found empty: advance past `[]` and skip trailing whitespace ===
        builder.switch_to_block(found_empty);
        builder.seal_block(found_empty);
        let pos_after_empty = builder.ins().iadd(pos2, two);
        builder.def_var(cursor.pos, pos_after_empty);
        builder.def_var(result_is_empty_var, one_i8);
        builder.ins().jump(skip_trailing_ws_loop, &[]);

        // === Skip trailing whitespace ===
        builder.switch_to_block(skip_trailing_ws_loop);
        let pos3 = builder.use_var(cursor.pos);
        let have_bytes3 = builder
            .ins()
            .icmp(IntCC::UnsignedLessThan, pos3, cursor.len);
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
        builder.ins().jump(skip_trailing_ws_loop, &[]);
        builder.seal_block(skip_trailing_ws_loop);

        // === Not empty: restore original position ===
        builder.switch_to_block(not_empty);
        builder.seal_block(not_empty);
        let orig = builder.use_var(orig_pos_var);
        builder.def_var(cursor.pos, orig);
        // result_is_empty already 0, result_error already 0
        builder.ins().jump(merge, &[]);

        // === Merge: return results ===
        builder.switch_to_block(merge);
        builder.seal_block(merge);

        let is_empty_result = builder.use_var(result_is_empty_var);
        let error_result = builder.use_var(result_error_var);

        Some((is_empty_result, error_result))
    }

    fn emit_try_empty_map(
        &self,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
    ) -> Option<(Value, Value)> {
        // Fast path for empty JSON objects: check for `{}` pattern
        //
        // Returns (is_empty: i8, error: i32):
        // - If we find `{}`: consume it, skip trailing whitespace, return (1, 0)
        // - If not `{}`: leave cursor unchanged, return (0, 0)

        // Result variables
        let result_is_empty_var = builder.declare_var(types::I8);
        let result_error_var = builder.declare_var(types::I32);
        let zero_i8 = builder.ins().iconst(types::I8, 0);
        let one_i8 = builder.ins().iconst(types::I8, 1);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        builder.def_var(result_is_empty_var, zero_i8);
        builder.def_var(result_error_var, zero_i32);

        // Save original position
        let orig_pos = builder.use_var(cursor.pos);
        let orig_pos_var = builder.declare_var(cursor.ptr_type);
        builder.def_var(orig_pos_var, orig_pos);

        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let two = builder.ins().iconst(cursor.ptr_type, 2);

        // Whitespace constants
        let space = builder.ins().iconst(types::I8, b' ' as i64);
        let tab = builder.ins().iconst(types::I8, b'\t' as i64);
        let newline = builder.ins().iconst(types::I8, b'\n' as i64);
        let cr = builder.ins().iconst(types::I8, b'\r' as i64);

        // Create blocks
        let skip_ws_loop = builder.create_block();
        let check_ws = builder.create_block();
        let skip_ws_advance = builder.create_block();
        let check_pattern = builder.create_block();
        let found_empty = builder.create_block();
        let skip_trailing_ws_loop = builder.create_block();
        let check_trailing_ws = builder.create_block();
        let skip_trailing_ws_advance = builder.create_block();
        let not_empty = builder.create_block();
        let merge = builder.create_block();

        // Entry: start skipping leading whitespace
        builder.ins().jump(skip_ws_loop, &[]);

        // === Skip leading whitespace ===
        builder.switch_to_block(skip_ws_loop);
        let pos = builder.use_var(cursor.pos);
        let have_bytes = builder.ins().icmp(IntCC::UnsignedLessThan, pos, cursor.len);
        builder
            .ins()
            .brif(have_bytes, check_ws, &[], not_empty, &[]);

        builder.switch_to_block(check_ws);
        builder.seal_block(check_ws);
        let addr = builder.ins().iadd(cursor.input_ptr, pos);
        let byte = builder.ins().load(types::I8, MemFlags::trusted(), addr, 0);
        let is_space = builder.ins().icmp(IntCC::Equal, byte, space);
        let is_tab = builder.ins().icmp(IntCC::Equal, byte, tab);
        let is_newline = builder.ins().icmp(IntCC::Equal, byte, newline);
        let is_cr = builder.ins().icmp(IntCC::Equal, byte, cr);
        let is_ws1 = builder.ins().bor(is_space, is_tab);
        let is_ws2 = builder.ins().bor(is_newline, is_cr);
        let is_ws = builder.ins().bor(is_ws1, is_ws2);
        builder
            .ins()
            .brif(is_ws, skip_ws_advance, &[], check_pattern, &[]);

        builder.switch_to_block(skip_ws_advance);
        builder.seal_block(skip_ws_advance);
        let next_pos = builder.ins().iadd(pos, one);
        builder.def_var(cursor.pos, next_pos);
        builder.ins().jump(skip_ws_loop, &[]);
        builder.seal_block(skip_ws_loop);

        // === Check for `{}` pattern ===
        builder.switch_to_block(check_pattern);
        builder.seal_block(check_pattern);
        let pos2 = builder.use_var(cursor.pos);
        let end_pos = builder.ins().iadd(pos2, two);
        let have_two = builder
            .ins()
            .icmp(IntCC::UnsignedLessThanOrEqual, end_pos, cursor.len);
        let check_bytes = builder.create_block();
        builder
            .ins()
            .brif(have_two, check_bytes, &[], not_empty, &[]);

        builder.switch_to_block(check_bytes);
        builder.seal_block(check_bytes);
        // Load 2 bytes as i16: `{}` = 0x7D7B in little-endian (0x7B = '{', 0x7D = '}')
        let addr2 = builder.ins().iadd(cursor.input_ptr, pos2);
        let two_bytes = builder
            .ins()
            .load(types::I16, MemFlags::trusted(), addr2, 0);
        let empty_pattern = builder.ins().iconst(types::I16, 0x7D7B); // "{}" little-endian
        let is_empty = builder.ins().icmp(IntCC::Equal, two_bytes, empty_pattern);
        builder
            .ins()
            .brif(is_empty, found_empty, &[], not_empty, &[]);

        // === Found empty: advance past `{}` and skip trailing whitespace ===
        builder.switch_to_block(found_empty);
        builder.seal_block(found_empty);
        let pos_after_empty = builder.ins().iadd(pos2, two);
        builder.def_var(cursor.pos, pos_after_empty);
        builder.def_var(result_is_empty_var, one_i8);
        builder.ins().jump(skip_trailing_ws_loop, &[]);

        // === Skip trailing whitespace ===
        builder.switch_to_block(skip_trailing_ws_loop);
        let pos3 = builder.use_var(cursor.pos);
        let have_bytes3 = builder
            .ins()
            .icmp(IntCC::UnsignedLessThan, pos3, cursor.len);
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
        builder.ins().jump(skip_trailing_ws_loop, &[]);
        builder.seal_block(skip_trailing_ws_loop);

        // === Not empty: restore original position ===
        builder.switch_to_block(not_empty);
        builder.seal_block(not_empty);
        let orig = builder.use_var(orig_pos_var);
        builder.def_var(cursor.pos, orig);
        builder.ins().jump(merge, &[]);

        // === Merge: return results ===
        builder.switch_to_block(merge);
        builder.seal_block(merge);

        let is_empty_result = builder.use_var(result_is_empty_var);
        let error_result = builder.use_var(result_error_var);

        Some((is_empty_result, error_result))
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

        // Whitespace constants - space for fast path, others for slow path
        let space = builder.ins().iconst(types::I8, b' ' as i64);
        let tab = builder.ins().iconst(types::I8, b'\t' as i64);
        let newline = builder.ins().iconst(types::I8, b'\n' as i64);
        let cr = builder.ins().iconst(types::I8, b'\r' as i64);
        let const_32 = builder.ins().iconst(types::I8, 32);

        // Create blocks
        let skip_leading_ws_loop = builder.create_block();
        let check_leading_ws = builder.create_block();
        let maybe_leading_ws = builder.create_block();
        let check_leading_low_ws = builder.create_block();
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

        // Fast path: check if byte > 32 first (most common case - not whitespace)
        builder.switch_to_block(check_leading_ws);
        builder.seal_block(check_leading_ws);
        let addr = builder.ins().iadd(cursor.input_ptr, pos);
        let byte = builder.ins().load(types::I8, MemFlags::trusted(), addr, 0);

        let gt_32 = builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThan, byte, const_32);
        builder
            .ins()
            .brif(gt_32, check_brace, &[], maybe_leading_ws, &[]);

        // Byte <= 32: check if it's space (most common whitespace)
        builder.switch_to_block(maybe_leading_ws);
        builder.seal_block(maybe_leading_ws);
        let is_space = builder.ins().icmp(IntCC::Equal, byte, space);
        builder.ins().brif(
            is_space,
            skip_leading_ws_advance,
            &[],
            check_leading_low_ws,
            &[],
        );

        // Byte < 32: check tab/lf/cr (rare)
        builder.switch_to_block(check_leading_low_ws);
        builder.seal_block(check_leading_low_ws);
        let is_tab = builder.ins().icmp(IntCC::Equal, byte, tab);
        let is_newline = builder.ins().icmp(IntCC::Equal, byte, newline);
        let is_cr = builder.ins().icmp(IntCC::Equal, byte, cr);
        let is_ws_1 = builder.ins().bor(is_tab, is_newline);
        let is_ws = builder.ins().bor(is_ws_1, is_cr);
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

        // Fast path: check if byte > 32 first
        let maybe_trailing_ws = builder.create_block();
        let check_trailing_low_ws = builder.create_block();

        builder.switch_to_block(check_trailing_ws);
        builder.seal_block(check_trailing_ws);
        let addr3 = builder.ins().iadd(cursor.input_ptr, pos3);
        let byte3 = builder.ins().load(types::I8, MemFlags::trusted(), addr3, 0);

        let gt_32_3 = builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThan, byte3, const_32);
        builder
            .ins()
            .brif(gt_32_3, merge, &[], maybe_trailing_ws, &[]);

        // Byte <= 32: check if it's space
        builder.switch_to_block(maybe_trailing_ws);
        builder.seal_block(maybe_trailing_ws);
        let is_space3 = builder.ins().icmp(IntCC::Equal, byte3, space);
        builder.ins().brif(
            is_space3,
            skip_trailing_ws_advance,
            &[],
            check_trailing_low_ws,
            &[],
        );

        // Byte < 32: check tab/lf/cr
        builder.switch_to_block(check_trailing_low_ws);
        builder.seal_block(check_trailing_low_ws);
        let is_tab3 = builder.ins().icmp(IntCC::Equal, byte3, tab);
        let is_newline3 = builder.ins().icmp(IntCC::Equal, byte3, newline);
        let is_cr3 = builder.ins().icmp(IntCC::Equal, byte3, cr);
        let is_ws3_1 = builder.ins().bor(is_tab3, is_newline3);
        let is_ws3 = builder.ins().bor(is_ws3_1, is_cr3);
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

        // ws_check_char: check if current byte is whitespace (fast path)
        let maybe_ws = builder.create_block();
        let check_low_ws = builder.create_block();

        builder.switch_to_block(ws_check_char);
        builder.seal_block(ws_check_char);
        let ws_addr = builder.ins().iadd(cursor.input_ptr, ws_pos);
        let ws_byte = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), ws_addr, 0);

        // Fast path: check if byte > 32 first
        let const_32 = builder.ins().iconst(types::I8, 32);
        let gt_32 = builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThan, ws_byte, const_32);
        builder.ins().brif(gt_32, ws_done, &[], maybe_ws, &[]);

        // Byte <= 32: check if it's space
        builder.switch_to_block(maybe_ws);
        builder.seal_block(maybe_ws);
        let space = builder.ins().iconst(types::I8, b' ' as i64);
        let is_space = builder.ins().icmp(IntCC::Equal, ws_byte, space);
        builder
            .ins()
            .brif(is_space, skip_ws_check, &[], check_low_ws, &[]);

        // Byte < 32: check tab/lf/cr
        builder.switch_to_block(check_low_ws);
        builder.seal_block(check_low_ws);
        let tab = builder.ins().iconst(types::I8, b'\t' as i64);
        let newline = builder.ins().iconst(types::I8, b'\n' as i64);
        let cr = builder.ins().iconst(types::I8, b'\r' as i64);
        let is_tab = builder.ins().icmp(IntCC::Equal, ws_byte, tab);
        let is_newline = builder.ins().icmp(IntCC::Equal, ws_byte, newline);
        let is_cr = builder.ins().icmp(IntCC::Equal, ws_byte, cr);
        let is_ws_1 = builder.ins().bor(is_tab, is_newline);
        let is_ws = builder.ins().bor(is_ws_1, is_cr);
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

        // Whitespace constants - space for fast path, others for slow path
        let space = builder.ins().iconst(types::I8, b' ' as i64);
        let tab = builder.ins().iconst(types::I8, b'\t' as i64);
        let newline = builder.ins().iconst(types::I8, b'\n' as i64);
        let cr = builder.ins().iconst(types::I8, b'\r' as i64);
        let const_32 = builder.ins().iconst(types::I8, 32);

        // Create blocks
        let skip_leading_ws_loop = builder.create_block();
        let check_leading_ws = builder.create_block();
        let maybe_leading_ws = builder.create_block();
        let check_leading_low_ws = builder.create_block();
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

        // Fast path: check if byte > 32 first (most common case - not whitespace)
        builder.switch_to_block(check_leading_ws);
        builder.seal_block(check_leading_ws);
        let addr = builder.ins().iadd(cursor.input_ptr, pos);
        let byte = builder.ins().load(types::I8, MemFlags::trusted(), addr, 0);

        let gt_32 = builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThan, byte, const_32);
        builder
            .ins()
            .brif(gt_32, check_colon, &[], maybe_leading_ws, &[]);

        // Byte <= 32: check if it's space (most common whitespace)
        builder.switch_to_block(maybe_leading_ws);
        builder.seal_block(maybe_leading_ws);
        let is_space = builder.ins().icmp(IntCC::Equal, byte, space);
        builder.ins().brif(
            is_space,
            skip_leading_ws_advance,
            &[],
            check_leading_low_ws,
            &[],
        );

        // Byte < 32: check tab/lf/cr (rare)
        builder.switch_to_block(check_leading_low_ws);
        builder.seal_block(check_leading_low_ws);
        let is_tab = builder.ins().icmp(IntCC::Equal, byte, tab);
        let is_newline = builder.ins().icmp(IntCC::Equal, byte, newline);
        let is_cr = builder.ins().icmp(IntCC::Equal, byte, cr);
        let is_ws_1 = builder.ins().bor(is_tab, is_newline);
        let is_ws = builder.ins().bor(is_ws_1, is_cr);
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

        // Fast path: check if byte > 32 first
        let maybe_trailing_ws = builder.create_block();
        let check_trailing_low_ws = builder.create_block();

        builder.switch_to_block(check_trailing_ws);
        builder.seal_block(check_trailing_ws);
        let addr3 = builder.ins().iadd(cursor.input_ptr, pos3);
        let byte3 = builder.ins().load(types::I8, MemFlags::trusted(), addr3, 0);

        let gt_32_3 = builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThan, byte3, const_32);
        builder
            .ins()
            .brif(gt_32_3, merge, &[], maybe_trailing_ws, &[]);

        // Byte <= 32: check if it's space
        builder.switch_to_block(maybe_trailing_ws);
        builder.seal_block(maybe_trailing_ws);
        let is_space3 = builder.ins().icmp(IntCC::Equal, byte3, space);
        builder.ins().brif(
            is_space3,
            skip_trailing_ws_advance,
            &[],
            check_trailing_low_ws,
            &[],
        );

        // Byte < 32: check tab/lf/cr
        builder.switch_to_block(check_trailing_low_ws);
        builder.seal_block(check_trailing_low_ws);
        let is_tab3 = builder.ins().icmp(IntCC::Equal, byte3, tab);
        let is_newline3 = builder.ins().icmp(IntCC::Equal, byte3, newline);
        let is_cr3 = builder.ins().icmp(IntCC::Equal, byte3, cr);
        let is_ws3_1 = builder.ins().bor(is_tab3, is_newline3);
        let is_ws3 = builder.ins().bor(is_ws3_1, is_cr3);
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

        // Whitespace constants - space for fast path, others for slow path
        let space = builder.ins().iconst(types::I8, b' ' as i64);
        let tab = builder.ins().iconst(types::I8, b'\t' as i64);
        let newline = builder.ins().iconst(types::I8, b'\n' as i64);
        let cr = builder.ins().iconst(types::I8, b'\r' as i64);
        let const_32 = builder.ins().iconst(types::I8, 32);

        // Create all blocks upfront
        let skip_leading_ws_loop = builder.create_block();
        let check_leading_ws = builder.create_block();
        let maybe_leading_ws = builder.create_block();
        let check_leading_low_ws = builder.create_block();
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

        // Fast path: check if byte > 32 first
        builder.switch_to_block(check_leading_ws);
        builder.seal_block(check_leading_ws);
        let addr = builder.ins().iadd(cursor.input_ptr, pos);
        let byte = builder.ins().load(types::I8, MemFlags::trusted(), addr, 0);

        let gt_32 = builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThan, byte, const_32);
        builder
            .ins()
            .brif(gt_32, check_separator, &[], maybe_leading_ws, &[]);

        // Byte <= 32: check if it's space
        builder.switch_to_block(maybe_leading_ws);
        builder.seal_block(maybe_leading_ws);
        let is_space = builder.ins().icmp(IntCC::Equal, byte, space);
        builder.ins().brif(
            is_space,
            skip_leading_ws_advance,
            &[],
            check_leading_low_ws,
            &[],
        );

        // Byte < 32: check tab/lf/cr
        builder.switch_to_block(check_leading_low_ws);
        builder.seal_block(check_leading_low_ws);
        let is_tab = builder.ins().icmp(IntCC::Equal, byte, tab);
        let is_newline = builder.ins().icmp(IntCC::Equal, byte, newline);
        let is_cr = builder.ins().icmp(IntCC::Equal, byte, cr);
        let is_ws_1 = builder.ins().bor(is_tab, is_newline);
        let is_ws = builder.ins().bor(is_ws_1, is_cr);
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

        // Fast path: check if byte > 32 first
        let maybe_trailing_ws = builder.create_block();
        let check_trailing_low_ws = builder.create_block();

        builder.switch_to_block(check_trailing_ws);
        builder.seal_block(check_trailing_ws);
        let addr2 = builder.ins().iadd(cursor.input_ptr, pos2);
        let byte2 = builder.ins().load(types::I8, MemFlags::trusted(), addr2, 0);

        let gt_32_2 = builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThan, byte2, const_32);
        builder
            .ins()
            .brif(gt_32_2, merge, &[], maybe_trailing_ws, &[]);

        // Byte <= 32: check if it's space
        builder.switch_to_block(maybe_trailing_ws);
        builder.seal_block(maybe_trailing_ws);
        let is_space2 = builder.ins().icmp(IntCC::Equal, byte2, space);
        builder.ins().brif(
            is_space2,
            skip_trailing_ws_advance,
            &[],
            check_trailing_low_ws,
            &[],
        );

        // Byte < 32: check tab/lf/cr
        builder.switch_to_block(check_trailing_low_ws);
        builder.seal_block(check_trailing_low_ws);
        let is_tab2 = builder.ins().icmp(IntCC::Equal, byte2, tab);
        let is_newline2 = builder.ins().icmp(IntCC::Equal, byte2, newline);
        let is_cr2 = builder.ins().icmp(IntCC::Equal, byte2, cr);
        let is_ws2_1 = builder.ins().bor(is_tab2, is_newline2);
        let is_ws2 = builder.ins().bor(is_ws2_1, is_cr2);
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
