use cranelift::codegen::ir::AbiParam;
use cranelift::prelude::*;
use cranelift_jit::JITModule;
use cranelift_module::{FuncId, Linkage, Module};

use facet_core::{Def, Shape};

use super::super::format::{
    JIT_SCRATCH_ERROR_CODE_OFFSET, JIT_SCRATCH_ERROR_POS_OFFSET,
    JIT_SCRATCH_OUTPUT_INITIALIZED_OFFSET, JitCursor, JitFormat, make_c_sig,
};
use super::super::helpers;
use super::super::jit_debug;
use super::{
    FormatListElementKind, ShapeMemo, compile_list_format_deserializer,
    compile_struct_format_deserializer, compile_struct_positional_deserializer, func_addr_value,
    tier2_call_sig,
};

/// Compile a Tier-2 HashMap deserializer for HashMap<String, V>.
///
/// Generates code that parses a JSON object and populates the HashMap.
/// Signature: fn(input_ptr, len, pos, out, scratch) -> isize
pub(crate) fn compile_map_format_deserializer<F: JitFormat>(
    module: &mut JITModule,
    shape: &'static Shape,
    memo: &mut ShapeMemo,
) -> Option<FuncId> {
    jit_debug!(
        "compile_map_format_deserializer ENTRY for shape {:p}",
        shape
    );

    // Check memo first - return cached FuncId if already compiled
    let shape_ptr = shape as *const Shape;
    if let Some(&func_id) = memo.get(&shape_ptr) {
        jit_debug!(
            "compile_map_format_deserializer: using memoized FuncId for shape {:p}",
            shape
        );
        return Some(func_id);
    }

    let Def::Map(map_def) = &shape.def else {
        jit_debug!("[compile_map] Not a map");
        return None;
    };

    // Only support String keys for now
    if map_def.k.scalar_type() != Some(facet_core::ScalarType::String) {
        jit_debug!("[compile_map] Only String keys supported");
        return None;
    }

    let value_shape = map_def.v;
    let value_kind = match FormatListElementKind::from_shape(value_shape) {
        Some(k) => k,
        None => {
            jit_debug!("[compile_map] Value type not supported");
            return None;
        }
    };

    // Get HashMap vtable functions
    let init_fn = map_def.vtable.init_in_place_with_capacity;
    let insert_fn = map_def.vtable.insert;

    let pointer_type = module.target_config().pointer_type();

    // Function signature: fn(input_ptr, len, pos, out, scratch) -> isize
    let sig = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // input_ptr
        s.params.push(AbiParam::new(pointer_type)); // len
        s.params.push(AbiParam::new(pointer_type)); // pos
        s.params.push(AbiParam::new(pointer_type)); // out (map ptr)
        s.params.push(AbiParam::new(pointer_type)); // scratch
        s.returns.push(AbiParam::new(pointer_type)); // isize
        s
    };

    // Map insert signature: fn(map_ptr: PtrMut, key_ptr: PtrMut, value_ptr: PtrMut) -> ()
    let sig_map_insert = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // map_ptr.ptr
        s.params.push(AbiParam::new(pointer_type)); // map_ptr.metadata
        s.params.push(AbiParam::new(pointer_type)); // key_ptr.ptr
        s.params.push(AbiParam::new(pointer_type)); // key_ptr.metadata
        s.params.push(AbiParam::new(pointer_type)); // value_ptr.ptr
        s.params.push(AbiParam::new(pointer_type)); // value_ptr.metadata
        s
    };

    // Generate unique name for this map deserializer
    let func_name = format!("jit_deserialize_map_{:x}", shape as *const _ as usize);

    let func_id = match module.declare_function(&func_name, Linkage::Local, &sig) {
        Ok(id) => id,
        Err(e) => {
            jit_debug!("[compile_map] declare {} failed: {:?}", func_name, e);
            jit_debug!("declare_function('{}') failed: {:?}", func_name, e);
            return None;
        }
    };

    // Insert into memo immediately after declaration (before IR build) to avoid recursion/cycles
    memo.insert(shape_ptr, func_id);
    jit_debug!(
        "compile_map_format_deserializer: memoized FuncId for shape {:p}",
        shape
    );

    let mut ctx = module.make_context();
    ctx.func.signature = sig;

    let mut builder_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);
    let nested_call_sig_ref = builder.import_signature(tier2_call_sig(module, pointer_type));

    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);

    let input_ptr = builder.block_params(entry)[0];
    let len = builder.block_params(entry)[1];
    let pos_param = builder.block_params(entry)[2];
    let out_ptr = builder.block_params(entry)[3];
    let scratch_ptr = builder.block_params(entry)[4];

    let pos_var = builder.declare_var(pointer_type);
    builder.def_var(pos_var, pos_param);

    let err_var = builder.declare_var(types::I32);
    let zero_i32 = builder.ins().iconst(types::I32, 0);
    builder.def_var(err_var, zero_i32);

    // Map state pointer (format-specific)
    let state_ptr = if F::MAP_STATE_SIZE > 0 {
        let align_shift = F::MAP_STATE_ALIGN.trailing_zeros() as u8;
        let state_slot = builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            F::MAP_STATE_SIZE,
            align_shift,
        ));
        builder.ins().stack_addr(pointer_type, state_slot, 0)
    } else {
        builder.ins().iconst(pointer_type, 0)
    };

    // Track a pending owned key string so we can drop it on early errors (before insertion).
    let key_ptr_var = builder.declare_var(pointer_type);
    let key_len_var = builder.declare_var(pointer_type);
    let key_cap_var = builder.declare_var(pointer_type);
    let key_owned_var = builder.declare_var(types::I8);
    let zero_ptr = builder.ins().iconst(pointer_type, 0);
    let zero_i8 = builder.ins().iconst(types::I8, 0);
    builder.def_var(key_ptr_var, zero_ptr);
    builder.def_var(key_len_var, zero_ptr);
    builder.def_var(key_cap_var, zero_ptr);
    builder.def_var(key_owned_var, zero_i8);

    // Helpers (call_indirect to avoid short-range relocations on AArch64)
    let map_init_sig_ref = {
        // fn(out_ptr: *mut u8, capacity: usize, init_fn: *const u8) -> ()
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // out_ptr
        s.params.push(AbiParam::new(pointer_type)); // capacity
        s.params.push(AbiParam::new(pointer_type)); // init_fn
        builder.import_signature(s)
    };
    let map_init_ptr = builder.ins().iconst(
        pointer_type,
        helpers::jit_map_init_with_capacity as *const u8 as i64,
    );

    let write_string_sig_ref = {
        // jit_write_string(out, offset, ptr, len, cap, owned)
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // out_ptr
        s.params.push(AbiParam::new(pointer_type)); // offset
        s.params.push(AbiParam::new(pointer_type)); // str_ptr
        s.params.push(AbiParam::new(pointer_type)); // str_len
        s.params.push(AbiParam::new(pointer_type)); // str_cap
        s.params.push(AbiParam::new(types::I8)); // owned
        builder.import_signature(s)
    };
    let write_string_ptr = builder
        .ins()
        .iconst(pointer_type, helpers::jit_write_string as *const u8 as i64);

    let drop_owned_string_sig_ref = {
        // jit_drop_owned_string(ptr, len, cap)
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // ptr
        s.params.push(AbiParam::new(pointer_type)); // len
        s.params.push(AbiParam::new(pointer_type)); // cap
        builder.import_signature(s)
    };
    let drop_owned_string_ptr = builder.ins().iconst(
        pointer_type,
        helpers::jit_drop_owned_string as *const u8 as i64,
    );

    // Allocate stack space for the key String (layout: ptr, len, cap).
    let key_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        3 * pointer_type.bytes(),
        pointer_type.bytes().trailing_zeros() as u8,
    ));
    let key_out_ptr = builder.ins().stack_addr(pointer_type, key_slot, 0);

    // Allocate stack space for the value.
    let value_layout = match value_shape.layout.sized_layout() {
        Ok(layout) => layout,
        Err(_) => {
            jit_debug!("[compile_map] Value shape has unsized layout");
            return None;
        }
    };
    let value_size = value_layout.size() as u32;
    let value_align = value_layout.align().trailing_zeros() as u8;
    let value_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        value_size,
        value_align,
    ));
    let value_ptr = builder.ins().stack_addr(pointer_type, value_slot, 0);

    // Initialize map with capacity 0 (will grow as needed).
    let init_fn_ptr = builder.ins().iconst(pointer_type, init_fn as usize as i64);
    let zero_capacity = builder.ins().iconst(pointer_type, 0);
    builder.ins().call_indirect(
        map_init_sig_ref,
        map_init_ptr,
        &[out_ptr, zero_capacity, init_fn_ptr],
    );

    // Mark output as initialized so wrapper can drop on error.
    let one_i8 = builder.ins().iconst(types::I8, 1);
    builder.ins().store(
        MemFlags::trusted(),
        one_i8,
        scratch_ptr,
        JIT_SCRATCH_OUTPUT_INITIALIZED_OFFSET,
    );

    let format = F::default();
    let mut cursor = JitCursor {
        input_ptr,
        len,
        pos: pos_var,
        ptr_type: pointer_type,
    };

    let loop_check_end = builder.create_block();
    let loop_body = builder.create_block();
    let done = builder.create_block();
    let error = builder.create_block();
    let nested_error_passthrough = builder.create_block();

    // map_begin
    let begin_err = format.emit_map_begin(module, &mut builder, &mut cursor, state_ptr);
    builder.def_var(err_var, begin_err);
    let begin_ok = builder.ins().icmp_imm(IntCC::Equal, begin_err, 0);
    builder
        .ins()
        .brif(begin_ok, loop_check_end, &[], error, &[]);
    builder.seal_block(entry);

    // loop_check_end
    // Note: do NOT seal yet; it has a back edge from loop_body.
    builder.switch_to_block(loop_check_end);
    let (is_end, end_err) = format.emit_map_is_end(module, &mut builder, &mut cursor, state_ptr);
    builder.def_var(err_var, end_err);
    let end_ok = builder.ins().icmp_imm(IntCC::Equal, end_err, 0);
    let check_end_value = builder.create_block();
    builder.ins().brif(end_ok, check_end_value, &[], error, &[]);

    builder.switch_to_block(check_end_value);
    builder.seal_block(check_end_value);
    let is_end_bool = builder.ins().icmp_imm(IntCC::NotEqual, is_end, 0);
    builder.ins().brif(is_end_bool, done, &[], loop_body, &[]);

    // loop_body
    builder.switch_to_block(loop_body);

    // Reset pending key raw parts for this iteration.
    builder.def_var(key_ptr_var, zero_ptr);
    builder.def_var(key_len_var, zero_ptr);
    builder.def_var(key_cap_var, zero_ptr);
    builder.def_var(key_owned_var, zero_i8);

    // read_key
    let (key_value, key_err) =
        format.emit_map_read_key(module, &mut builder, &mut cursor, state_ptr);
    builder.def_var(err_var, key_err);
    let key_ok = builder.ins().icmp_imm(IntCC::Equal, key_err, 0);
    let after_key = builder.create_block();
    builder.ins().brif(key_ok, after_key, &[], error, &[]);

    builder.switch_to_block(after_key);
    builder.seal_block(after_key);
    builder.def_var(key_ptr_var, key_value.ptr);
    builder.def_var(key_len_var, key_value.len);
    builder.def_var(key_cap_var, key_value.cap);
    builder.def_var(key_owned_var, key_value.owned);

    // kv_sep
    let sep_err = format.emit_map_kv_sep(module, &mut builder, &mut cursor, state_ptr);
    builder.def_var(err_var, sep_err);
    let sep_ok = builder.ins().icmp_imm(IntCC::Equal, sep_err, 0);
    let after_sep = builder.create_block();
    builder.ins().brif(sep_ok, after_sep, &[], error, &[]);

    builder.switch_to_block(after_sep);
    builder.seal_block(after_sep);

    // value
    match value_kind {
        FormatListElementKind::Bool => {
            let (value_i8, err) = format.emit_parse_bool(module, &mut builder, &mut cursor);
            builder.def_var(err_var, err);
            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
            let store = builder.create_block();
            builder.ins().brif(ok, store, &[], error, &[]);
            builder.switch_to_block(store);
            builder
                .ins()
                .store(MemFlags::trusted(), value_i8, value_ptr, 0);
            builder.seal_block(store);
        }
        FormatListElementKind::U8 => {
            let (value_u8, err) = format.emit_parse_u8(module, &mut builder, &mut cursor);
            builder.def_var(err_var, err);
            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
            let store = builder.create_block();
            builder.ins().brif(ok, store, &[], error, &[]);
            builder.switch_to_block(store);
            builder
                .ins()
                .store(MemFlags::trusted(), value_u8, value_ptr, 0);
            builder.seal_block(store);
        }
        FormatListElementKind::I64 => {
            use facet_core::ScalarType;
            let (value_i64, err) = format.emit_parse_i64(module, &mut builder, &mut cursor);
            builder.def_var(err_var, err);
            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
            let store = builder.create_block();
            builder.ins().brif(ok, store, &[], error, &[]);
            builder.switch_to_block(store);
            let scalar = value_shape.scalar_type().unwrap();
            let value = match scalar {
                ScalarType::I8 => builder.ins().ireduce(types::I8, value_i64),
                ScalarType::I16 => builder.ins().ireduce(types::I16, value_i64),
                ScalarType::I32 => builder.ins().ireduce(types::I32, value_i64),
                ScalarType::I64 => value_i64,
                _ => value_i64,
            };
            builder
                .ins()
                .store(MemFlags::trusted(), value, value_ptr, 0);
            builder.seal_block(store);
        }
        FormatListElementKind::U64 => {
            use facet_core::ScalarType;
            let (value_u64, err) = format.emit_parse_u64(module, &mut builder, &mut cursor);
            builder.def_var(err_var, err);
            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
            let store = builder.create_block();
            builder.ins().brif(ok, store, &[], error, &[]);
            builder.switch_to_block(store);
            let scalar = value_shape.scalar_type().unwrap();
            let value = match scalar {
                ScalarType::U8 => builder.ins().ireduce(types::I8, value_u64),
                ScalarType::U16 => builder.ins().ireduce(types::I16, value_u64),
                ScalarType::U32 => builder.ins().ireduce(types::I32, value_u64),
                ScalarType::U64 => value_u64,
                _ => value_u64,
            };
            builder
                .ins()
                .store(MemFlags::trusted(), value, value_ptr, 0);
            builder.seal_block(store);
        }
        FormatListElementKind::F64 => {
            use facet_core::ScalarType;
            let (value_f64, err) = format.emit_parse_f64(module, &mut builder, &mut cursor);
            builder.def_var(err_var, err);
            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
            let store = builder.create_block();
            builder.ins().brif(ok, store, &[], error, &[]);
            builder.switch_to_block(store);
            let scalar = value_shape.scalar_type().unwrap();
            let value = if matches!(scalar, ScalarType::F32) {
                builder.ins().fdemote(types::F32, value_f64)
            } else {
                value_f64
            };
            builder
                .ins()
                .store(MemFlags::trusted(), value, value_ptr, 0);
            builder.seal_block(store);
        }
        FormatListElementKind::String => {
            let (string_value, err) = format.emit_parse_string(module, &mut builder, &mut cursor);
            builder.def_var(err_var, err);
            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
            let store = builder.create_block();
            builder.ins().brif(ok, store, &[], error, &[]);
            builder.switch_to_block(store);
            let zero_offset = builder.ins().iconst(pointer_type, 0);
            builder.ins().call_indirect(
                write_string_sig_ref,
                write_string_ptr,
                &[
                    value_ptr,
                    zero_offset,
                    string_value.ptr,
                    string_value.len,
                    string_value.cap,
                    string_value.owned,
                ],
            );
            builder.seal_block(store);
        }
        FormatListElementKind::Struct(_) => {
            // Use the appropriate struct compiler based on format encoding
            let struct_func_id = match F::STRUCT_ENCODING {
                crate::jit::StructEncoding::Map => {
                    compile_struct_format_deserializer::<F>(module, value_shape, memo)?
                }
                crate::jit::StructEncoding::Positional => {
                    compile_struct_positional_deserializer::<F>(module, value_shape, memo)?
                }
            };
            let struct_func_ref = module.declare_func_in_func(struct_func_id, builder.func);

            let current_pos = builder.use_var(pos_var);
            let struct_func_ptr = func_addr_value(&mut builder, pointer_type, struct_func_ref);
            let call_result = builder.ins().call_indirect(
                nested_call_sig_ref,
                struct_func_ptr,
                &[input_ptr, len, current_pos, value_ptr, scratch_ptr],
            );
            let new_pos = builder.inst_results(call_result)[0];

            let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
            let nested_ok = builder.create_block();
            builder
                .ins()
                .brif(is_error, nested_error_passthrough, &[], nested_ok, &[]);

            builder.switch_to_block(nested_ok);
            builder.def_var(pos_var, new_pos);
            builder.seal_block(nested_ok);
        }
        FormatListElementKind::List(_) => {
            let list_func_id = compile_list_format_deserializer::<F>(module, value_shape, memo)?;
            let list_func_ref = module.declare_func_in_func(list_func_id, builder.func);

            let current_pos = builder.use_var(pos_var);
            let list_func_ptr = func_addr_value(&mut builder, pointer_type, list_func_ref);
            let call_result = builder.ins().call_indirect(
                nested_call_sig_ref,
                list_func_ptr,
                &[input_ptr, len, current_pos, value_ptr, scratch_ptr],
            );
            let new_pos = builder.inst_results(call_result)[0];

            let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
            let nested_ok = builder.create_block();
            builder
                .ins()
                .brif(is_error, nested_error_passthrough, &[], nested_ok, &[]);

            builder.switch_to_block(nested_ok);
            builder.def_var(pos_var, new_pos);
            builder.seal_block(nested_ok);
        }
        FormatListElementKind::Map(_) => {
            let map_func_id = compile_map_format_deserializer::<F>(module, value_shape, memo)?;
            let map_func_ref = module.declare_func_in_func(map_func_id, builder.func);

            let current_pos = builder.use_var(pos_var);
            let map_func_ptr = func_addr_value(&mut builder, pointer_type, map_func_ref);
            let call_result = builder.ins().call_indirect(
                nested_call_sig_ref,
                map_func_ptr,
                &[input_ptr, len, current_pos, value_ptr, scratch_ptr],
            );
            let new_pos = builder.inst_results(call_result)[0];

            let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
            let nested_ok = builder.create_block();
            builder
                .ins()
                .brif(is_error, nested_error_passthrough, &[], nested_ok, &[]);

            builder.switch_to_block(nested_ok);
            builder.def_var(pos_var, new_pos);
            builder.seal_block(nested_ok);
        }
    }

    // Materialize key into an owned String right before insertion.
    // This avoids constructing a fake String pointing into the input buffer.
    let zero_offset = builder.ins().iconst(pointer_type, 0);
    let key_ptr_raw = builder.use_var(key_ptr_var);
    let key_len_raw = builder.use_var(key_len_var);
    let key_cap_raw = builder.use_var(key_cap_var);
    let key_owned_raw = builder.use_var(key_owned_var);
    builder.ins().call_indirect(
        write_string_sig_ref,
        write_string_ptr,
        &[
            key_out_ptr,
            zero_offset,
            key_ptr_raw,
            key_len_raw,
            key_cap_raw,
            key_owned_raw,
        ],
    );
    // Raw parts consumed when owned=1.
    builder.def_var(key_owned_var, zero_i8);

    // insert
    let insert_fn_addr = builder
        .ins()
        .iconst(pointer_type, insert_fn as usize as i64);
    let sig_ref_map_insert = builder.import_signature(sig_map_insert);
    let zero_meta = builder.ins().iconst(pointer_type, 0);
    builder.ins().call_indirect(
        sig_ref_map_insert,
        insert_fn_addr,
        &[
            out_ptr,
            zero_meta,
            key_out_ptr,
            zero_meta,
            value_ptr,
            zero_meta,
        ],
    );

    // next
    let next_err = format.emit_map_next(module, &mut builder, &mut cursor, state_ptr);
    builder.def_var(err_var, next_err);
    let next_ok = builder.ins().icmp_imm(IntCC::Equal, next_err, 0);
    let after_next = builder.create_block();
    builder.ins().brif(next_ok, after_next, &[], error, &[]);

    builder.switch_to_block(after_next);
    builder.seal_block(after_next);
    builder.ins().jump(loop_check_end, &[]);

    builder.seal_block(loop_body);
    builder.seal_block(loop_check_end);

    // done
    builder.switch_to_block(done);
    let final_pos = builder.use_var(pos_var);
    builder.ins().return_(&[final_pos]);
    builder.seal_block(done);

    // nested_error_passthrough: nested call failed, scratch already written.
    // Still drop any pending owned key raw string.
    builder.switch_to_block(nested_error_passthrough);
    let key_owned = builder.use_var(key_owned_var);
    let need_drop = builder.ins().icmp_imm(IntCC::NotEqual, key_owned, 0);
    let drop_key = builder.create_block();
    let nested_after_drop = builder.create_block();
    builder
        .ins()
        .brif(need_drop, drop_key, &[], nested_after_drop, &[]);

    builder.switch_to_block(drop_key);
    let key_ptr_val = builder.use_var(key_ptr_var);
    let key_len_val = builder.use_var(key_len_var);
    let key_cap_val = builder.use_var(key_cap_var);
    builder.ins().call_indirect(
        drop_owned_string_sig_ref,
        drop_owned_string_ptr,
        &[key_ptr_val, key_len_val, key_cap_val],
    );
    builder.ins().jump(nested_after_drop, &[]);
    builder.seal_block(drop_key);

    builder.switch_to_block(nested_after_drop);
    let minus_one = builder.ins().iconst(pointer_type, -1i64);
    builder.ins().return_(&[minus_one]);
    builder.seal_block(nested_after_drop);
    builder.seal_block(nested_error_passthrough);

    // error: drop pending owned key (if any), write scratch and return -1.
    builder.switch_to_block(error);
    let key_owned = builder.use_var(key_owned_var);
    let need_drop = builder.ins().icmp_imm(IntCC::NotEqual, key_owned, 0);
    let drop_key = builder.create_block();
    let after_drop = builder.create_block();
    builder
        .ins()
        .brif(need_drop, drop_key, &[], after_drop, &[]);

    builder.switch_to_block(drop_key);
    let key_ptr_val = builder.use_var(key_ptr_var);
    let key_len_val = builder.use_var(key_len_var);
    let key_cap_val = builder.use_var(key_cap_var);
    builder.ins().call_indirect(
        drop_owned_string_sig_ref,
        drop_owned_string_ptr,
        &[key_ptr_val, key_len_val, key_cap_val],
    );
    builder.ins().jump(after_drop, &[]);
    builder.seal_block(drop_key);

    builder.switch_to_block(after_drop);
    let err_code = builder.use_var(err_var);
    let err_pos = builder.use_var(pos_var);
    builder.ins().store(
        MemFlags::trusted(),
        err_code,
        scratch_ptr,
        JIT_SCRATCH_ERROR_CODE_OFFSET,
    );
    builder.ins().store(
        MemFlags::trusted(),
        err_pos,
        scratch_ptr,
        JIT_SCRATCH_ERROR_POS_OFFSET,
    );
    let minus_one = builder.ins().iconst(pointer_type, -1i64);
    builder.ins().return_(&[minus_one]);
    builder.seal_block(after_drop);
    builder.seal_block(error);

    builder.finalize();

    if let Err(_e) = module.define_function(func_id, &mut ctx) {
        jit_debug!("[compile_map] define_function failed: {:?}", _e);
        return None;
    }

    jit_debug!("[compile_map] SUCCESS - HashMap<String, V> function compiled");
    Some(func_id)
}
