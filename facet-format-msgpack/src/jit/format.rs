//! MsgPack-specific JIT format emitter.
//!
//! Implements `JitFormat` to generate Cranelift IR for direct MsgPack byte parsing.
//!
//! MsgPack is a tagged binary format with NO trivia (whitespace/comments), which means
//! `emit_skip_ws` and similar operations are no-ops. Sequences use count-prefix
//! encoding rather than delimiters, so "end" detection is state-based.

use facet_format::jit::{
    BlockArg, FunctionBuilder, InstBuilder, IntCC, JITBuilder, JitCursor, JitFormat,
    JitStringValue, MemFlags, Value, types,
};

use super::helpers;

/// MsgPack format JIT emitter.
///
/// A zero-sized type that implements `JitFormat` for MsgPack binary syntax.
/// Helper functions are defined in this crate's `jit::helpers` module.
#[derive(Debug, Clone, Copy, Default)]
pub struct MsgPackJitFormat;

/// Error codes for MsgPack JIT parsing.
pub mod error {
    pub use super::helpers::error::*;
}

/// MsgPack wire format tags.
pub mod tags {
    pub use super::helpers::tags::*;
}

impl MsgPackJitFormat {
    /// Emit inline IR to parse a MsgPack unsigned integer as u64.
    ///
    /// Handles: positive fixint, u8, u16, u32, u64 tags.
    /// Returns (value: i64, error: i32) - value contains u64 bits.
    /// Updates cursor.pos to point past the value.
    fn emit_parse_uint(builder: &mut FunctionBuilder, cursor: &mut JitCursor) -> (Value, Value) {
        let pos = builder.use_var(cursor.pos);

        // Variables for results
        let result_var = builder.declare_var(types::I64);
        let error_var = builder.declare_var(types::I32);
        let zero_i64 = builder.ins().iconst(types::I64, 0);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        builder.def_var(result_var, zero_i64);
        builder.def_var(error_var, zero_i32);

        // Create all blocks upfront (including intermediate ok blocks)
        let eof_error = builder.create_block();
        let check_tag = builder.create_block();
        let is_fixint = builder.create_block();
        let check_u8 = builder.create_block();
        let check_u16 = builder.create_block();
        let check_u32 = builder.create_block();
        let check_u64 = builder.create_block();
        let read_u8 = builder.create_block();
        let read_u8_ok = builder.create_block();
        let read_u16 = builder.create_block();
        let read_u16_ok = builder.create_block();
        let read_u32 = builder.create_block();
        let read_u32_ok = builder.create_block();
        let read_u64 = builder.create_block();
        let read_u64_ok = builder.create_block();
        let invalid_tag = builder.create_block();
        let merge = builder.create_block();

        // Check bounds - first branch to eof_error
        let have_byte = builder.ins().icmp(IntCC::UnsignedLessThan, pos, cursor.len);
        builder
            .ins()
            .brif(have_byte, check_tag, &[], eof_error, &[]);

        // check_tag: load tag and classify
        builder.switch_to_block(check_tag);
        builder.seal_block(check_tag);
        let addr = builder.ins().iadd(cursor.input_ptr, pos);
        let tag = builder.ins().load(types::I8, MemFlags::trusted(), addr, 0);

        // Check if positive fixint (tag <= 0x7F)
        let is_positive_fixint = builder
            .ins()
            .icmp_imm(IntCC::UnsignedLessThanOrEqual, tag, 0x7F);
        builder
            .ins()
            .brif(is_positive_fixint, is_fixint, &[], check_u8, &[]);

        // is_fixint: value = tag (zero extended), advance by 1
        builder.switch_to_block(is_fixint);
        builder.seal_block(is_fixint);
        let tag_i64 = builder.ins().uextend(types::I64, tag);
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let new_pos = builder.ins().iadd(pos, one);
        builder.def_var(cursor.pos, new_pos);
        builder.def_var(result_var, tag_i64);
        builder.def_var(error_var, zero_i32);
        builder.ins().jump(merge, &[]);

        // check_u8: tag == 0xCC?
        builder.switch_to_block(check_u8);
        builder.seal_block(check_u8);
        let is_u8 = builder.ins().icmp_imm(IntCC::Equal, tag, tags::U8 as i64);
        builder.ins().brif(is_u8, read_u8, &[], check_u16, &[]);

        // read_u8: need 1 more byte - second branch to eof_error
        builder.switch_to_block(read_u8);
        builder.seal_block(read_u8);
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let pos_plus_1_u8 = builder.ins().iadd(pos, one);
        let two = builder.ins().iconst(cursor.ptr_type, 2);
        let end_pos_u8 = builder.ins().iadd(pos, two);
        let have_value_u8 =
            builder
                .ins()
                .icmp(IntCC::UnsignedLessThanOrEqual, end_pos_u8, cursor.len);
        builder
            .ins()
            .brif(have_value_u8, read_u8_ok, &[], eof_error, &[]);

        builder.switch_to_block(read_u8_ok);
        builder.seal_block(read_u8_ok);
        let value_addr = builder.ins().iadd(cursor.input_ptr, pos_plus_1_u8);
        let value_u8 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), value_addr, 0);
        let value_i64 = builder.ins().uextend(types::I64, value_u8);
        builder.def_var(cursor.pos, end_pos_u8);
        builder.def_var(result_var, value_i64);
        builder.def_var(error_var, zero_i32);
        builder.ins().jump(merge, &[]);

        // check_u16: tag == 0xCD?
        builder.switch_to_block(check_u16);
        builder.seal_block(check_u16);
        let is_u16 = builder.ins().icmp_imm(IntCC::Equal, tag, tags::U16 as i64);
        builder.ins().brif(is_u16, read_u16, &[], check_u32, &[]);

        // read_u16: need 2 more bytes (big endian) - third branch to eof_error
        builder.switch_to_block(read_u16);
        builder.seal_block(read_u16);
        let three = builder.ins().iconst(cursor.ptr_type, 3);
        let end_pos_u16 = builder.ins().iadd(pos, three);
        let have_value_u16 =
            builder
                .ins()
                .icmp(IntCC::UnsignedLessThanOrEqual, end_pos_u16, cursor.len);
        builder
            .ins()
            .brif(have_value_u16, read_u16_ok, &[], eof_error, &[]);

        builder.switch_to_block(read_u16_ok);
        builder.seal_block(read_u16_ok);
        // Read two bytes and compose big-endian
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let pos_plus_1 = builder.ins().iadd(pos, one);
        let two = builder.ins().iconst(cursor.ptr_type, 2);
        let pos_plus_2 = builder.ins().iadd(pos, two);
        let addr1 = builder.ins().iadd(cursor.input_ptr, pos_plus_1);
        let addr2 = builder.ins().iadd(cursor.input_ptr, pos_plus_2);
        let b0 = builder.ins().load(types::I8, MemFlags::trusted(), addr1, 0);
        let b1 = builder.ins().load(types::I8, MemFlags::trusted(), addr2, 0);
        // value = (b0 << 8) | b1
        let b0_i64 = builder.ins().uextend(types::I64, b0);
        let b1_i64 = builder.ins().uextend(types::I64, b1);
        let eight = builder.ins().iconst(types::I64, 8);
        let b0_shifted = builder.ins().ishl(b0_i64, eight);
        let value_i64 = builder.ins().bor(b0_shifted, b1_i64);
        builder.def_var(cursor.pos, end_pos_u16);
        builder.def_var(result_var, value_i64);
        builder.def_var(error_var, zero_i32);
        builder.ins().jump(merge, &[]);

        // check_u32: tag == 0xCE?
        builder.switch_to_block(check_u32);
        builder.seal_block(check_u32);
        let is_u32 = builder.ins().icmp_imm(IntCC::Equal, tag, tags::U32 as i64);
        builder.ins().brif(is_u32, read_u32, &[], check_u64, &[]);

        // read_u32: need 4 more bytes - fourth branch to eof_error
        builder.switch_to_block(read_u32);
        builder.seal_block(read_u32);
        let five = builder.ins().iconst(cursor.ptr_type, 5);
        let end_pos_u32 = builder.ins().iadd(pos, five);
        let have_value_u32 =
            builder
                .ins()
                .icmp(IntCC::UnsignedLessThanOrEqual, end_pos_u32, cursor.len);
        builder
            .ins()
            .brif(have_value_u32, read_u32_ok, &[], eof_error, &[]);

        builder.switch_to_block(read_u32_ok);
        builder.seal_block(read_u32_ok);
        // Read 4 bytes big-endian
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let pos_plus_1 = builder.ins().iadd(pos, one);
        let addr_base = builder.ins().iadd(cursor.input_ptr, pos_plus_1);
        let b0 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 0);
        let b1 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 1);
        let b2 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 2);
        let b3 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 3);
        let b0_i64 = builder.ins().uextend(types::I64, b0);
        let b1_i64 = builder.ins().uextend(types::I64, b1);
        let b2_i64 = builder.ins().uextend(types::I64, b2);
        let b3_i64 = builder.ins().uextend(types::I64, b3);
        let c24 = builder.ins().iconst(types::I64, 24);
        let c16 = builder.ins().iconst(types::I64, 16);
        let c8 = builder.ins().iconst(types::I64, 8);
        let v0 = builder.ins().ishl(b0_i64, c24);
        let v1 = builder.ins().ishl(b1_i64, c16);
        let v2 = builder.ins().ishl(b2_i64, c8);
        let v01 = builder.ins().bor(v0, v1);
        let v012 = builder.ins().bor(v01, v2);
        let value_i64 = builder.ins().bor(v012, b3_i64);
        builder.def_var(cursor.pos, end_pos_u32);
        builder.def_var(result_var, value_i64);
        builder.def_var(error_var, zero_i32);
        builder.ins().jump(merge, &[]);

        // check_u64: tag == 0xCF?
        builder.switch_to_block(check_u64);
        builder.seal_block(check_u64);
        let is_u64 = builder.ins().icmp_imm(IntCC::Equal, tag, tags::U64 as i64);
        builder.ins().brif(is_u64, read_u64, &[], invalid_tag, &[]);

        // read_u64: need 8 more bytes - fifth (final) branch to eof_error
        builder.switch_to_block(read_u64);
        builder.seal_block(read_u64);
        let nine = builder.ins().iconst(cursor.ptr_type, 9);
        let end_pos_u64 = builder.ins().iadd(pos, nine);
        let have_value_u64 =
            builder
                .ins()
                .icmp(IntCC::UnsignedLessThanOrEqual, end_pos_u64, cursor.len);
        builder
            .ins()
            .brif(have_value_u64, read_u64_ok, &[], eof_error, &[]);

        // NOW we can seal eof_error - all 5 predecessors are declared
        builder.seal_block(eof_error);

        builder.switch_to_block(read_u64_ok);
        builder.seal_block(read_u64_ok);
        // Read 8 bytes big-endian
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let pos_plus_1 = builder.ins().iadd(pos, one);
        let addr_base = builder.ins().iadd(cursor.input_ptr, pos_plus_1);
        // Load all 8 bytes
        let b0 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 0);
        let b1 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 1);
        let b2 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 2);
        let b3 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 3);
        let b4 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 4);
        let b5 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 5);
        let b6 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 6);
        let b7 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 7);
        let b0_i64 = builder.ins().uextend(types::I64, b0);
        let b1_i64 = builder.ins().uextend(types::I64, b1);
        let b2_i64 = builder.ins().uextend(types::I64, b2);
        let b3_i64 = builder.ins().uextend(types::I64, b3);
        let b4_i64 = builder.ins().uextend(types::I64, b4);
        let b5_i64 = builder.ins().uextend(types::I64, b5);
        let b6_i64 = builder.ins().uextend(types::I64, b6);
        let b7_i64 = builder.ins().uextend(types::I64, b7);
        let c56 = builder.ins().iconst(types::I64, 56);
        let c48 = builder.ins().iconst(types::I64, 48);
        let c40 = builder.ins().iconst(types::I64, 40);
        let c32 = builder.ins().iconst(types::I64, 32);
        let c24 = builder.ins().iconst(types::I64, 24);
        let c16 = builder.ins().iconst(types::I64, 16);
        let c8 = builder.ins().iconst(types::I64, 8);
        let v0 = builder.ins().ishl(b0_i64, c56);
        let v1 = builder.ins().ishl(b1_i64, c48);
        let v2 = builder.ins().ishl(b2_i64, c40);
        let v3 = builder.ins().ishl(b3_i64, c32);
        let v4 = builder.ins().ishl(b4_i64, c24);
        let v5 = builder.ins().ishl(b5_i64, c16);
        let v6 = builder.ins().ishl(b6_i64, c8);
        let v01 = builder.ins().bor(v0, v1);
        let v23 = builder.ins().bor(v2, v3);
        let v45 = builder.ins().bor(v4, v5);
        let v67 = builder.ins().bor(v6, b7_i64);
        let v0123 = builder.ins().bor(v01, v23);
        let v4567 = builder.ins().bor(v45, v67);
        let value_i64 = builder.ins().bor(v0123, v4567);
        builder.def_var(cursor.pos, end_pos_u64);
        builder.def_var(result_var, value_i64);
        builder.def_var(error_var, zero_i32);
        builder.ins().jump(merge, &[]);

        // invalid_tag
        builder.switch_to_block(invalid_tag);
        builder.seal_block(invalid_tag);
        let invalid_err = builder.ins().iconst(types::I32, error::EXPECTED_INT as i64);
        builder.def_var(error_var, invalid_err);
        builder.ins().jump(merge, &[]);

        // eof_error - define contents (already sealed above)
        builder.switch_to_block(eof_error);
        let eof_err = builder
            .ins()
            .iconst(types::I32, error::UNEXPECTED_EOF as i64);
        builder.def_var(error_var, eof_err);
        builder.ins().jump(merge, &[]);

        // merge
        builder.switch_to_block(merge);
        builder.seal_block(merge);

        let value = builder.use_var(result_var);
        let error = builder.use_var(error_var);
        (value, error)
    }

    /// Emit inline IR to parse a MsgPack signed integer as i64.
    ///
    /// Handles: positive fixint, negative fixint, u8-u64, i8-i64 tags.
    /// Returns (value: i64, error: i32).
    fn emit_parse_int(builder: &mut FunctionBuilder, cursor: &mut JitCursor) -> (Value, Value) {
        let pos = builder.use_var(cursor.pos);

        // Variables for results
        let result_var = builder.declare_var(types::I64);
        let error_var = builder.declare_var(types::I32);
        let zero_i64 = builder.ins().iconst(types::I64, 0);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        builder.def_var(result_var, zero_i64);
        builder.def_var(error_var, zero_i32);

        // Create ALL blocks upfront (including ok blocks created later)
        let eof_error = builder.create_block();
        let check_tag = builder.create_block();
        let is_pos_fixint = builder.create_block();
        let check_neg_fixint = builder.create_block();
        let is_neg_fixint = builder.create_block();
        let check_unsigned = builder.create_block();
        let check_i16 = builder.create_block();
        let check_i32 = builder.create_block();
        let check_i64 = builder.create_block();
        let check_u_tags = builder.create_block();
        let check_u16_tag = builder.create_block();
        let check_u32_tag = builder.create_block();
        let check_u64_tag = builder.create_block();
        let read_i8 = builder.create_block();
        let read_i8_ok = builder.create_block();
        let read_i16 = builder.create_block();
        let read_i16_ok = builder.create_block();
        let read_i32 = builder.create_block();
        let read_i32_ok = builder.create_block();
        let read_i64 = builder.create_block();
        let read_i64_ok = builder.create_block();
        let read_u8_as_signed = builder.create_block();
        let read_u8_as_signed_ok = builder.create_block();
        let read_u16_as_signed = builder.create_block();
        let read_u16_as_signed_ok = builder.create_block();
        let read_u32_as_signed = builder.create_block();
        let read_u32_as_signed_ok = builder.create_block();
        let read_u64_as_signed = builder.create_block();
        let read_u64_as_signed_ok = builder.create_block();
        let invalid_tag = builder.create_block();
        let merge = builder.create_block();

        // Check bounds - branch 1 to eof_error
        let have_byte = builder.ins().icmp(IntCC::UnsignedLessThan, pos, cursor.len);
        builder
            .ins()
            .brif(have_byte, check_tag, &[], eof_error, &[]);

        // check_tag: load tag and classify
        builder.switch_to_block(check_tag);
        builder.seal_block(check_tag);
        let addr = builder.ins().iadd(cursor.input_ptr, pos);
        let tag = builder.ins().load(types::I8, MemFlags::trusted(), addr, 0);

        // Check if positive fixint (tag <= 0x7F, i.e. high bit clear)
        let is_positive = builder
            .ins()
            .icmp_imm(IntCC::UnsignedLessThanOrEqual, tag, 0x7F);
        builder
            .ins()
            .brif(is_positive, is_pos_fixint, &[], check_neg_fixint, &[]);

        // is_pos_fixint: value = tag (zero extended)
        builder.switch_to_block(is_pos_fixint);
        builder.seal_block(is_pos_fixint);
        let tag_i64 = builder.ins().uextend(types::I64, tag);
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let new_pos = builder.ins().iadd(pos, one);
        builder.def_var(cursor.pos, new_pos);
        builder.def_var(result_var, tag_i64);
        builder.def_var(error_var, zero_i32);
        builder.ins().jump(merge, &[]);

        // check_neg_fixint: tag >= 0xE0?
        builder.switch_to_block(check_neg_fixint);
        builder.seal_block(check_neg_fixint);
        let is_neg = builder
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThanOrEqual, tag, 0xE0);
        builder
            .ins()
            .brif(is_neg, is_neg_fixint, &[], check_unsigned, &[]);

        // is_neg_fixint: value = sign-extend tag as i8
        builder.switch_to_block(is_neg_fixint);
        builder.seal_block(is_neg_fixint);
        let tag_i64 = builder.ins().sextend(types::I64, tag);
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let new_pos = builder.ins().iadd(pos, one);
        builder.def_var(cursor.pos, new_pos);
        builder.def_var(result_var, tag_i64);
        builder.def_var(error_var, zero_i32);
        builder.ins().jump(merge, &[]);

        // check_unsigned: check for u8/u16/u32/u64 or i8/i16/i32/i64 tags
        builder.switch_to_block(check_unsigned);
        builder.seal_block(check_unsigned);

        // Build a chain of tag checks for signed types
        let is_i8_tag = builder.ins().icmp_imm(IntCC::Equal, tag, tags::I8 as i64);
        builder.ins().brif(is_i8_tag, read_i8, &[], check_i16, &[]);

        builder.switch_to_block(check_i16);
        builder.seal_block(check_i16);
        let is_i16_tag = builder.ins().icmp_imm(IntCC::Equal, tag, tags::I16 as i64);
        builder
            .ins()
            .brif(is_i16_tag, read_i16, &[], check_i32, &[]);

        builder.switch_to_block(check_i32);
        builder.seal_block(check_i32);
        let is_i32_tag = builder.ins().icmp_imm(IntCC::Equal, tag, tags::I32 as i64);
        builder
            .ins()
            .brif(is_i32_tag, read_i32, &[], check_i64, &[]);

        builder.switch_to_block(check_i64);
        builder.seal_block(check_i64);
        let is_i64_tag = builder.ins().icmp_imm(IntCC::Equal, tag, tags::I64 as i64);
        builder
            .ins()
            .brif(is_i64_tag, read_i64, &[], check_u_tags, &[]);

        // Also accept unsigned tags (permissive mode)
        builder.switch_to_block(check_u_tags);
        builder.seal_block(check_u_tags);
        // For u8/u16/u32/u64 tags, reuse the unsigned parsing logic
        // We need to handle these separately since they return unsigned values
        let is_u8 = builder.ins().icmp_imm(IntCC::Equal, tag, tags::U8 as i64);
        builder
            .ins()
            .brif(is_u8, read_u8_as_signed, &[], check_u16_tag, &[]);

        builder.switch_to_block(check_u16_tag);
        builder.seal_block(check_u16_tag);
        let is_u16 = builder.ins().icmp_imm(IntCC::Equal, tag, tags::U16 as i64);
        builder
            .ins()
            .brif(is_u16, read_u16_as_signed, &[], check_u32_tag, &[]);

        builder.switch_to_block(check_u32_tag);
        builder.seal_block(check_u32_tag);
        let is_u32 = builder.ins().icmp_imm(IntCC::Equal, tag, tags::U32 as i64);
        builder
            .ins()
            .brif(is_u32, read_u32_as_signed, &[], check_u64_tag, &[]);

        builder.switch_to_block(check_u64_tag);
        builder.seal_block(check_u64_tag);
        let is_u64 = builder.ins().icmp_imm(IntCC::Equal, tag, tags::U64 as i64);
        builder
            .ins()
            .brif(is_u64, read_u64_as_signed, &[], invalid_tag, &[]);

        // read_i8: 1 byte signed - branch 2 to eof_error
        builder.switch_to_block(read_i8);
        builder.seal_block(read_i8);
        let two = builder.ins().iconst(cursor.ptr_type, 2);
        let end_pos_i8 = builder.ins().iadd(pos, two);
        let have_value_i8 =
            builder
                .ins()
                .icmp(IntCC::UnsignedLessThanOrEqual, end_pos_i8, cursor.len);
        builder
            .ins()
            .brif(have_value_i8, read_i8_ok, &[], eof_error, &[]);

        builder.switch_to_block(read_i8_ok);
        builder.seal_block(read_i8_ok);
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let pos_plus_1 = builder.ins().iadd(pos, one);
        let value_addr = builder.ins().iadd(cursor.input_ptr, pos_plus_1);
        let value_i8 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), value_addr, 0);
        let value_i64 = builder.ins().sextend(types::I64, value_i8);
        builder.def_var(cursor.pos, end_pos_i8);
        builder.def_var(result_var, value_i64);
        builder.def_var(error_var, zero_i32);
        builder.ins().jump(merge, &[]);

        // read_i16: 2 bytes signed big-endian - branch 3 to eof_error
        builder.switch_to_block(read_i16);
        builder.seal_block(read_i16);
        let three = builder.ins().iconst(cursor.ptr_type, 3);
        let end_pos_i16 = builder.ins().iadd(pos, three);
        let have_value_i16 =
            builder
                .ins()
                .icmp(IntCC::UnsignedLessThanOrEqual, end_pos_i16, cursor.len);
        builder
            .ins()
            .brif(have_value_i16, read_i16_ok, &[], eof_error, &[]);

        builder.switch_to_block(read_i16_ok);
        builder.seal_block(read_i16_ok);
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let pos_plus_1 = builder.ins().iadd(pos, one);
        let addr_base = builder.ins().iadd(cursor.input_ptr, pos_plus_1);
        let b0 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 0);
        let b1 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 1);
        // Compose as i16 then sign-extend
        let b0_u16 = builder.ins().uextend(types::I16, b0);
        let b1_u16 = builder.ins().uextend(types::I16, b1);
        let eight = builder.ins().iconst(types::I16, 8);
        let b0_shifted = builder.ins().ishl(b0_u16, eight);
        let value_i16 = builder.ins().bor(b0_shifted, b1_u16);
        let value_i64 = builder.ins().sextend(types::I64, value_i16);
        builder.def_var(cursor.pos, end_pos_i16);
        builder.def_var(result_var, value_i64);
        builder.def_var(error_var, zero_i32);
        builder.ins().jump(merge, &[]);

        // read_i32: 4 bytes signed big-endian - branch 4 to eof_error
        builder.switch_to_block(read_i32);
        builder.seal_block(read_i32);
        let five = builder.ins().iconst(cursor.ptr_type, 5);
        let end_pos_i32 = builder.ins().iadd(pos, five);
        let have_value_i32 =
            builder
                .ins()
                .icmp(IntCC::UnsignedLessThanOrEqual, end_pos_i32, cursor.len);
        builder
            .ins()
            .brif(have_value_i32, read_i32_ok, &[], eof_error, &[]);

        builder.switch_to_block(read_i32_ok);
        builder.seal_block(read_i32_ok);
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let pos_plus_1 = builder.ins().iadd(pos, one);
        let addr_base = builder.ins().iadd(cursor.input_ptr, pos_plus_1);
        let b0 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 0);
        let b1 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 1);
        let b2 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 2);
        let b3 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 3);
        let b0_u32 = builder.ins().uextend(types::I32, b0);
        let b1_u32 = builder.ins().uextend(types::I32, b1);
        let b2_u32 = builder.ins().uextend(types::I32, b2);
        let b3_u32 = builder.ins().uextend(types::I32, b3);
        let c24 = builder.ins().iconst(types::I32, 24);
        let c16 = builder.ins().iconst(types::I32, 16);
        let c8 = builder.ins().iconst(types::I32, 8);
        let v0 = builder.ins().ishl(b0_u32, c24);
        let v1 = builder.ins().ishl(b1_u32, c16);
        let v2 = builder.ins().ishl(b2_u32, c8);
        let v01 = builder.ins().bor(v0, v1);
        let v012 = builder.ins().bor(v01, v2);
        let value_i32 = builder.ins().bor(v012, b3_u32);
        let value_i64 = builder.ins().sextend(types::I64, value_i32);
        builder.def_var(cursor.pos, end_pos_i32);
        builder.def_var(result_var, value_i64);
        builder.def_var(error_var, zero_i32);
        builder.ins().jump(merge, &[]);

        // read_i64: 8 bytes signed big-endian - branch 5 to eof_error
        builder.switch_to_block(read_i64);
        builder.seal_block(read_i64);
        let nine = builder.ins().iconst(cursor.ptr_type, 9);
        let end_pos_i64 = builder.ins().iadd(pos, nine);
        let have_value_i64 =
            builder
                .ins()
                .icmp(IntCC::UnsignedLessThanOrEqual, end_pos_i64, cursor.len);
        builder
            .ins()
            .brif(have_value_i64, read_i64_ok, &[], eof_error, &[]);

        builder.switch_to_block(read_i64_ok);
        builder.seal_block(read_i64_ok);
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let pos_plus_1 = builder.ins().iadd(pos, one);
        let addr_base = builder.ins().iadd(cursor.input_ptr, pos_plus_1);
        let b0 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 0);
        let b1 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 1);
        let b2 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 2);
        let b3 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 3);
        let b4 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 4);
        let b5 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 5);
        let b6 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 6);
        let b7 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 7);
        let b0_i64 = builder.ins().uextend(types::I64, b0);
        let b1_i64 = builder.ins().uextend(types::I64, b1);
        let b2_i64 = builder.ins().uextend(types::I64, b2);
        let b3_i64 = builder.ins().uextend(types::I64, b3);
        let b4_i64 = builder.ins().uextend(types::I64, b4);
        let b5_i64 = builder.ins().uextend(types::I64, b5);
        let b6_i64 = builder.ins().uextend(types::I64, b6);
        let b7_i64 = builder.ins().uextend(types::I64, b7);
        let c56 = builder.ins().iconst(types::I64, 56);
        let c48 = builder.ins().iconst(types::I64, 48);
        let c40 = builder.ins().iconst(types::I64, 40);
        let c32 = builder.ins().iconst(types::I64, 32);
        let c24 = builder.ins().iconst(types::I64, 24);
        let c16 = builder.ins().iconst(types::I64, 16);
        let c8 = builder.ins().iconst(types::I64, 8);
        let v0 = builder.ins().ishl(b0_i64, c56);
        let v1 = builder.ins().ishl(b1_i64, c48);
        let v2 = builder.ins().ishl(b2_i64, c40);
        let v3 = builder.ins().ishl(b3_i64, c32);
        let v4 = builder.ins().ishl(b4_i64, c24);
        let v5 = builder.ins().ishl(b5_i64, c16);
        let v6 = builder.ins().ishl(b6_i64, c8);
        let v01 = builder.ins().bor(v0, v1);
        let v23 = builder.ins().bor(v2, v3);
        let v45 = builder.ins().bor(v4, v5);
        let v67 = builder.ins().bor(v6, b7_i64);
        let v0123 = builder.ins().bor(v01, v23);
        let v4567 = builder.ins().bor(v45, v67);
        let value_i64 = builder.ins().bor(v0123, v4567);
        builder.def_var(cursor.pos, end_pos_i64);
        builder.def_var(result_var, value_i64);
        builder.def_var(error_var, zero_i32);
        builder.ins().jump(merge, &[]);

        // Handle unsigned tags as signed (permissive mode)
        // read_u8_as_signed - branch 6 to eof_error
        builder.switch_to_block(read_u8_as_signed);
        builder.seal_block(read_u8_as_signed);
        let two = builder.ins().iconst(cursor.ptr_type, 2);
        let end_pos_u8s = builder.ins().iadd(pos, two);
        let have_value_u8s =
            builder
                .ins()
                .icmp(IntCC::UnsignedLessThanOrEqual, end_pos_u8s, cursor.len);
        builder
            .ins()
            .brif(have_value_u8s, read_u8_as_signed_ok, &[], eof_error, &[]);

        builder.switch_to_block(read_u8_as_signed_ok);
        builder.seal_block(read_u8_as_signed_ok);
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let pos_plus_1 = builder.ins().iadd(pos, one);
        let value_addr = builder.ins().iadd(cursor.input_ptr, pos_plus_1);
        let value_u8 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), value_addr, 0);
        let value_i64 = builder.ins().uextend(types::I64, value_u8);
        builder.def_var(cursor.pos, end_pos_u8s);
        builder.def_var(result_var, value_i64);
        builder.def_var(error_var, zero_i32);
        builder.ins().jump(merge, &[]);

        // read_u16_as_signed - branch 7 to eof_error
        builder.switch_to_block(read_u16_as_signed);
        builder.seal_block(read_u16_as_signed);
        let three = builder.ins().iconst(cursor.ptr_type, 3);
        let end_pos_u16s = builder.ins().iadd(pos, three);
        let have_value_u16s =
            builder
                .ins()
                .icmp(IntCC::UnsignedLessThanOrEqual, end_pos_u16s, cursor.len);
        builder
            .ins()
            .brif(have_value_u16s, read_u16_as_signed_ok, &[], eof_error, &[]);

        builder.switch_to_block(read_u16_as_signed_ok);
        builder.seal_block(read_u16_as_signed_ok);
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let pos_plus_1 = builder.ins().iadd(pos, one);
        let addr_base = builder.ins().iadd(cursor.input_ptr, pos_plus_1);
        let b0 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 0);
        let b1 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 1);
        let b0_i64 = builder.ins().uextend(types::I64, b0);
        let b1_i64 = builder.ins().uextend(types::I64, b1);
        let eight = builder.ins().iconst(types::I64, 8);
        let b0_shifted = builder.ins().ishl(b0_i64, eight);
        let value_i64 = builder.ins().bor(b0_shifted, b1_i64);
        builder.def_var(cursor.pos, end_pos_u16s);
        builder.def_var(result_var, value_i64);
        builder.def_var(error_var, zero_i32);
        builder.ins().jump(merge, &[]);

        // read_u32_as_signed - branch 8 to eof_error
        builder.switch_to_block(read_u32_as_signed);
        builder.seal_block(read_u32_as_signed);
        let five = builder.ins().iconst(cursor.ptr_type, 5);
        let end_pos_u32s = builder.ins().iadd(pos, five);
        let have_value_u32s =
            builder
                .ins()
                .icmp(IntCC::UnsignedLessThanOrEqual, end_pos_u32s, cursor.len);
        builder
            .ins()
            .brif(have_value_u32s, read_u32_as_signed_ok, &[], eof_error, &[]);

        builder.switch_to_block(read_u32_as_signed_ok);
        builder.seal_block(read_u32_as_signed_ok);
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let pos_plus_1 = builder.ins().iadd(pos, one);
        let addr_base = builder.ins().iadd(cursor.input_ptr, pos_plus_1);
        let b0 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 0);
        let b1 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 1);
        let b2 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 2);
        let b3 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 3);
        let b0_i64 = builder.ins().uextend(types::I64, b0);
        let b1_i64 = builder.ins().uextend(types::I64, b1);
        let b2_i64 = builder.ins().uextend(types::I64, b2);
        let b3_i64 = builder.ins().uextend(types::I64, b3);
        let c24 = builder.ins().iconst(types::I64, 24);
        let c16 = builder.ins().iconst(types::I64, 16);
        let c8 = builder.ins().iconst(types::I64, 8);
        let v0 = builder.ins().ishl(b0_i64, c24);
        let v1 = builder.ins().ishl(b1_i64, c16);
        let v2 = builder.ins().ishl(b2_i64, c8);
        let v01 = builder.ins().bor(v0, v1);
        let v012 = builder.ins().bor(v01, v2);
        let value_i64 = builder.ins().bor(v012, b3_i64);
        builder.def_var(cursor.pos, end_pos_u32s);
        builder.def_var(result_var, value_i64);
        builder.def_var(error_var, zero_i32);
        builder.ins().jump(merge, &[]);

        // read_u64_as_signed - branch 9 (final) to eof_error
        builder.switch_to_block(read_u64_as_signed);
        builder.seal_block(read_u64_as_signed);
        let nine = builder.ins().iconst(cursor.ptr_type, 9);
        let end_pos_u64s = builder.ins().iadd(pos, nine);
        let have_value_u64s =
            builder
                .ins()
                .icmp(IntCC::UnsignedLessThanOrEqual, end_pos_u64s, cursor.len);
        builder
            .ins()
            .brif(have_value_u64s, read_u64_as_signed_ok, &[], eof_error, &[]);

        // NOW we can seal eof_error - all 9 predecessors are declared
        builder.seal_block(eof_error);

        builder.switch_to_block(read_u64_as_signed_ok);
        builder.seal_block(read_u64_as_signed_ok);
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let pos_plus_1 = builder.ins().iadd(pos, one);
        let addr_base = builder.ins().iadd(cursor.input_ptr, pos_plus_1);
        let b0 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 0);
        let b1 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 1);
        let b2 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 2);
        let b3 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 3);
        let b4 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 4);
        let b5 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 5);
        let b6 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 6);
        let b7 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 7);
        let b0_i64 = builder.ins().uextend(types::I64, b0);
        let b1_i64 = builder.ins().uextend(types::I64, b1);
        let b2_i64 = builder.ins().uextend(types::I64, b2);
        let b3_i64 = builder.ins().uextend(types::I64, b3);
        let b4_i64 = builder.ins().uextend(types::I64, b4);
        let b5_i64 = builder.ins().uextend(types::I64, b5);
        let b6_i64 = builder.ins().uextend(types::I64, b6);
        let b7_i64 = builder.ins().uextend(types::I64, b7);
        let c56 = builder.ins().iconst(types::I64, 56);
        let c48 = builder.ins().iconst(types::I64, 48);
        let c40 = builder.ins().iconst(types::I64, 40);
        let c32 = builder.ins().iconst(types::I64, 32);
        let c24 = builder.ins().iconst(types::I64, 24);
        let c16 = builder.ins().iconst(types::I64, 16);
        let c8 = builder.ins().iconst(types::I64, 8);
        let v0 = builder.ins().ishl(b0_i64, c56);
        let v1 = builder.ins().ishl(b1_i64, c48);
        let v2 = builder.ins().ishl(b2_i64, c40);
        let v3 = builder.ins().ishl(b3_i64, c32);
        let v4 = builder.ins().ishl(b4_i64, c24);
        let v5 = builder.ins().ishl(b5_i64, c16);
        let v6 = builder.ins().ishl(b6_i64, c8);
        let v01 = builder.ins().bor(v0, v1);
        let v23 = builder.ins().bor(v2, v3);
        let v45 = builder.ins().bor(v4, v5);
        let v67 = builder.ins().bor(v6, b7_i64);
        let v0123 = builder.ins().bor(v01, v23);
        let v4567 = builder.ins().bor(v45, v67);
        let value_i64 = builder.ins().bor(v0123, v4567);
        builder.def_var(cursor.pos, end_pos_u64s);
        builder.def_var(result_var, value_i64);
        builder.def_var(error_var, zero_i32);
        builder.ins().jump(merge, &[]);

        // invalid_tag
        builder.switch_to_block(invalid_tag);
        builder.seal_block(invalid_tag);
        let invalid_err = builder.ins().iconst(types::I32, error::EXPECTED_INT as i64);
        builder.def_var(error_var, invalid_err);
        builder.ins().jump(merge, &[]);

        // eof_error - define contents (already sealed above)
        builder.switch_to_block(eof_error);
        let eof_err = builder
            .ins()
            .iconst(types::I32, error::UNEXPECTED_EOF as i64);
        builder.def_var(error_var, eof_err);
        builder.ins().jump(merge, &[]);

        // merge
        builder.switch_to_block(merge);
        builder.seal_block(merge);

        let value = builder.use_var(result_var);
        let error = builder.use_var(error_var);
        (value, error)
    }

    /// Emit inline IR to parse a MsgPack array header.
    ///
    /// Handles: fixarray, array16, array32.
    /// Returns (count: usize, error: i32).
    /// Stores count in state_ptr for is_end/next operations.
    fn emit_array_header(
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
        state_ptr: Value,
    ) -> (Value, Value) {
        let pos = builder.use_var(cursor.pos);

        // Variables
        let count_var = builder.declare_var(cursor.ptr_type);
        let error_var = builder.declare_var(types::I32);
        let zero_count = builder.ins().iconst(cursor.ptr_type, 0);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        builder.def_var(count_var, zero_count);
        builder.def_var(error_var, zero_i32);

        // Create all blocks upfront
        let eof_error = builder.create_block();
        let check_tag = builder.create_block();
        let is_fixarray = builder.create_block();
        let check_array16 = builder.create_block();
        let check_array32 = builder.create_block();
        let read_array16 = builder.create_block();
        let read_array16_ok = builder.create_block();
        let read_array32 = builder.create_block();
        let read_array32_ok = builder.create_block();
        let invalid_tag = builder.create_block();
        let store_and_done = builder.create_block();
        let merge = builder.create_block();

        // Check bounds - first branch to eof_error
        let have_byte = builder.ins().icmp(IntCC::UnsignedLessThan, pos, cursor.len);
        builder
            .ins()
            .brif(have_byte, check_tag, &[], eof_error, &[]);

        // check_tag
        builder.switch_to_block(check_tag);
        builder.seal_block(check_tag);
        let addr = builder.ins().iadd(cursor.input_ptr, pos);
        let tag = builder.ins().load(types::I8, MemFlags::trusted(), addr, 0);

        // Check if fixarray (0x90 <= tag <= 0x9F)
        let is_ge_90 = builder
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThanOrEqual, tag, 0x90);
        let is_le_9f = builder
            .ins()
            .icmp_imm(IntCC::UnsignedLessThanOrEqual, tag, 0x9F);
        let is_fix = builder.ins().band(is_ge_90, is_le_9f);
        builder
            .ins()
            .brif(is_fix, is_fixarray, &[], check_array16, &[]);

        // is_fixarray: count = tag & 0x0F
        builder.switch_to_block(is_fixarray);
        builder.seal_block(is_fixarray);
        let mask = builder.ins().iconst(types::I8, 0x0F);
        let count_i8 = builder.ins().band(tag, mask);
        let count = builder.ins().uextend(cursor.ptr_type, count_i8);
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let new_pos = builder.ins().iadd(pos, one);
        builder.def_var(cursor.pos, new_pos);
        builder.def_var(count_var, count);
        builder.def_var(error_var, zero_i32);
        builder.ins().jump(store_and_done, &[]);

        // check_array16: tag == 0xDC?
        builder.switch_to_block(check_array16);
        builder.seal_block(check_array16);
        let is_arr16 = builder
            .ins()
            .icmp_imm(IntCC::Equal, tag, tags::ARRAY16 as i64);
        builder
            .ins()
            .brif(is_arr16, read_array16, &[], check_array32, &[]);

        // read_array16: 2 bytes for count - second branch to eof_error
        builder.switch_to_block(read_array16);
        builder.seal_block(read_array16);
        let three = builder.ins().iconst(cursor.ptr_type, 3);
        let end_pos_16 = builder.ins().iadd(pos, three);
        let have_count_16 =
            builder
                .ins()
                .icmp(IntCC::UnsignedLessThanOrEqual, end_pos_16, cursor.len);
        builder
            .ins()
            .brif(have_count_16, read_array16_ok, &[], eof_error, &[]);

        builder.switch_to_block(read_array16_ok);
        builder.seal_block(read_array16_ok);
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let pos_plus_1 = builder.ins().iadd(pos, one);
        let addr_base = builder.ins().iadd(cursor.input_ptr, pos_plus_1);
        let b0 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 0);
        let b1 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 1);
        let b0_ext = builder.ins().uextend(cursor.ptr_type, b0);
        let b1_ext = builder.ins().uextend(cursor.ptr_type, b1);
        let eight = builder.ins().iconst(cursor.ptr_type, 8);
        let b0_shifted = builder.ins().ishl(b0_ext, eight);
        let count = builder.ins().bor(b0_shifted, b1_ext);
        builder.def_var(cursor.pos, end_pos_16);
        builder.def_var(count_var, count);
        builder.def_var(error_var, zero_i32);
        builder.ins().jump(store_and_done, &[]);

        // check_array32: tag == 0xDD?
        builder.switch_to_block(check_array32);
        builder.seal_block(check_array32);
        let is_arr32 = builder
            .ins()
            .icmp_imm(IntCC::Equal, tag, tags::ARRAY32 as i64);
        builder
            .ins()
            .brif(is_arr32, read_array32, &[], invalid_tag, &[]);

        // read_array32: 4 bytes for count - third (final) branch to eof_error
        builder.switch_to_block(read_array32);
        builder.seal_block(read_array32);
        let five = builder.ins().iconst(cursor.ptr_type, 5);
        let end_pos_32 = builder.ins().iadd(pos, five);
        let have_count_32 =
            builder
                .ins()
                .icmp(IntCC::UnsignedLessThanOrEqual, end_pos_32, cursor.len);
        builder
            .ins()
            .brif(have_count_32, read_array32_ok, &[], eof_error, &[]);

        // NOW we can seal eof_error - all 3 predecessors are declared
        builder.seal_block(eof_error);

        builder.switch_to_block(read_array32_ok);
        builder.seal_block(read_array32_ok);
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let pos_plus_1 = builder.ins().iadd(pos, one);
        let addr_base = builder.ins().iadd(cursor.input_ptr, pos_plus_1);
        let b0 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 0);
        let b1 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 1);
        let b2 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 2);
        let b3 = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), addr_base, 3);
        let b0_ext = builder.ins().uextend(cursor.ptr_type, b0);
        let b1_ext = builder.ins().uextend(cursor.ptr_type, b1);
        let b2_ext = builder.ins().uextend(cursor.ptr_type, b2);
        let b3_ext = builder.ins().uextend(cursor.ptr_type, b3);
        let c24 = builder.ins().iconst(cursor.ptr_type, 24);
        let c16 = builder.ins().iconst(cursor.ptr_type, 16);
        let c8 = builder.ins().iconst(cursor.ptr_type, 8);
        let v0 = builder.ins().ishl(b0_ext, c24);
        let v1 = builder.ins().ishl(b1_ext, c16);
        let v2 = builder.ins().ishl(b2_ext, c8);
        let v01 = builder.ins().bor(v0, v1);
        let v012 = builder.ins().bor(v01, v2);
        let count = builder.ins().bor(v012, b3_ext);
        builder.def_var(cursor.pos, end_pos_32);
        builder.def_var(count_var, count);
        builder.def_var(error_var, zero_i32);
        builder.ins().jump(store_and_done, &[]);

        // invalid_tag
        builder.switch_to_block(invalid_tag);
        builder.seal_block(invalid_tag);
        let invalid_err = builder
            .ins()
            .iconst(types::I32, error::EXPECTED_ARRAY as i64);
        builder.def_var(error_var, invalid_err);
        builder.ins().jump(merge, &[]);

        // eof_error - define contents (already sealed above)
        builder.switch_to_block(eof_error);
        let eof_err = builder
            .ins()
            .iconst(types::I32, error::UNEXPECTED_EOF as i64);
        builder.def_var(error_var, eof_err);
        builder.ins().jump(merge, &[]);

        // store_and_done: store count in state for is_end/next
        builder.switch_to_block(store_and_done);
        builder.seal_block(store_and_done);
        let count = builder.use_var(count_var);
        // Store as u64
        let count_i64 = if cursor.ptr_type == types::I64 {
            count
        } else {
            builder.ins().uextend(types::I64, count)
        };
        builder
            .ins()
            .store(MemFlags::trusted(), count_i64, state_ptr, 0);
        builder.ins().jump(merge, &[]);

        // merge
        builder.switch_to_block(merge);
        builder.seal_block(merge);

        let final_count = builder.use_var(count_var);
        let final_error = builder.use_var(error_var);
        (final_count, final_error)
    }
}

impl JitFormat for MsgPackJitFormat {
    fn register_helpers(builder: &mut JITBuilder) {
        // Register MsgPack-specific helper functions (for potential future use)
        builder.symbol(
            "msgpack_jit_bulk_copy_u8",
            helpers::msgpack_jit_bulk_copy_u8 as *const u8,
        );
    }

    // MsgPack sequences need state for the remaining element count
    const SEQ_STATE_SIZE: u32 = 8; // u64 for remaining count
    const SEQ_STATE_ALIGN: u32 = 8;

    const MAP_STATE_SIZE: u32 = 8;
    const MAP_STATE_ALIGN: u32 = 8;

    fn emit_skip_ws(&self, builder: &mut FunctionBuilder, _cursor: &mut JitCursor) -> Value {
        // MsgPack has NO trivia - this is a no-op
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
        let pos = builder.use_var(cursor.pos);

        let result_value_var = builder.declare_var(types::I8);
        let result_error_var = builder.declare_var(types::I32);
        let zero_i8 = builder.ins().iconst(types::I8, 0);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        builder.def_var(result_value_var, zero_i8);
        builder.def_var(result_error_var, zero_i32);

        let have_byte = builder.ins().icmp(IntCC::UnsignedLessThan, pos, cursor.len);

        let check_tag = builder.create_block();
        let valid_false = builder.create_block();
        let check_true = builder.create_block();
        let valid_true = builder.create_block();
        let invalid_tag = builder.create_block();
        let eof_error = builder.create_block();
        let merge = builder.create_block();

        builder
            .ins()
            .brif(have_byte, check_tag, &[], eof_error, &[]);

        builder.switch_to_block(eof_error);
        builder.seal_block(eof_error);
        let eof_err = builder
            .ins()
            .iconst(types::I32, error::UNEXPECTED_EOF as i64);
        builder.def_var(result_error_var, eof_err);
        builder.ins().jump(merge, &[]);

        builder.switch_to_block(check_tag);
        builder.seal_block(check_tag);
        let addr = builder.ins().iadd(cursor.input_ptr, pos);
        let tag = builder.ins().load(types::I8, MemFlags::trusted(), addr, 0);

        let is_false = builder
            .ins()
            .icmp_imm(IntCC::Equal, tag, tags::FALSE as i64);
        builder
            .ins()
            .brif(is_false, valid_false, &[], check_true, &[]);

        builder.switch_to_block(valid_false);
        builder.seal_block(valid_false);
        let one = builder.ins().iconst(cursor.ptr_type, 1);
        let new_pos = builder.ins().iadd(pos, one);
        builder.def_var(cursor.pos, new_pos);
        builder.def_var(result_value_var, zero_i8);
        builder.def_var(result_error_var, zero_i32);
        builder.ins().jump(merge, &[]);

        builder.switch_to_block(check_true);
        builder.seal_block(check_true);
        let is_true = builder.ins().icmp_imm(IntCC::Equal, tag, tags::TRUE as i64);
        builder
            .ins()
            .brif(is_true, valid_true, &[], invalid_tag, &[]);

        builder.switch_to_block(valid_true);
        builder.seal_block(valid_true);
        let one_val = builder.ins().iconst(types::I8, 1);
        let one_ptr = builder.ins().iconst(cursor.ptr_type, 1);
        let new_pos = builder.ins().iadd(pos, one_ptr);
        builder.def_var(cursor.pos, new_pos);
        builder.def_var(result_value_var, one_val);
        builder.def_var(result_error_var, zero_i32);
        builder.ins().jump(merge, &[]);

        builder.switch_to_block(invalid_tag);
        builder.seal_block(invalid_tag);
        let invalid_err = builder
            .ins()
            .iconst(types::I32, error::EXPECTED_BOOL as i64);
        builder.def_var(result_error_var, invalid_err);
        builder.ins().jump(merge, &[]);

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
        // Parse as u64 and truncate to u8
        let (value_i64, error) = Self::emit_parse_uint(builder, cursor);
        let value_i8 = builder.ins().ireduce(types::I8, value_i64);
        (value_i8, error)
    }

    fn emit_parse_i64(
        &self,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
    ) -> (Value, Value) {
        Self::emit_parse_int(builder, cursor)
    }

    fn emit_parse_u64(
        &self,
        builder: &mut FunctionBuilder,
        cursor: &mut JitCursor,
    ) -> (Value, Value) {
        Self::emit_parse_uint(builder, cursor)
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
        Self::emit_array_header(builder, cursor, state_ptr)
    }

    fn emit_seq_is_end(
        &self,
        builder: &mut FunctionBuilder,
        _cursor: &mut JitCursor,
        state_ptr: Value,
    ) -> (Value, Value) {
        let remaining = builder
            .ins()
            .load(types::I64, MemFlags::trusted(), state_ptr, 0);
        let is_zero = builder.ins().icmp_imm(IntCC::Equal, remaining, 0);
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
        let remaining = builder
            .ins()
            .load(types::I64, MemFlags::trusted(), state_ptr, 0);
        let is_zero = builder.ins().icmp_imm(IntCC::Equal, remaining, 0);

        let underflow_block = builder.create_block();
        let decrement_block = builder.create_block();
        let merge = builder.create_block();
        builder.append_block_param(merge, types::I32);

        builder
            .ins()
            .brif(is_zero, underflow_block, &[], decrement_block, &[]);

        builder.switch_to_block(underflow_block);
        builder.seal_block(underflow_block);
        let underflow_err = builder
            .ins()
            .iconst(types::I32, error::SEQ_UNDERFLOW as i64);
        builder.ins().jump(merge, &[BlockArg::from(underflow_err)]);

        builder.switch_to_block(decrement_block);
        builder.seal_block(decrement_block);
        let one = builder.ins().iconst(types::I64, 1);
        let new_remaining = builder.ins().isub(remaining, one);
        builder
            .ins()
            .store(MemFlags::trusted(), new_remaining, state_ptr, 0);
        let success = builder.ins().iconst(types::I32, 0);
        builder.ins().jump(merge, &[BlockArg::from(success)]);

        builder.switch_to_block(merge);
        builder.seal_block(merge);

        builder.block_params(merge)[0]
    }

    fn emit_seq_bulk_copy_u8(
        &self,
        _builder: &mut FunctionBuilder,
        _cursor: &mut JitCursor,
        _count: Value,
        _dest_ptr: Value,
    ) -> Option<Value> {
        // MsgPack arrays of integers are NOT contiguous bytes (each element has a tag)
        None
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
