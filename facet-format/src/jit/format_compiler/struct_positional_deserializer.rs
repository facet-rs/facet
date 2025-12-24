use cranelift::codegen::ir::{AbiParam, SigRef};
use cranelift::prelude::*;
use cranelift_jit::JITModule;
use cranelift_module::{FuncId, Linkage, Module};

use facet_core::{Shape, Type, UserType};

use super::super::format::{
    JIT_SCRATCH_ERROR_CODE_OFFSET, JIT_SCRATCH_ERROR_POS_OFFSET, JitCursor, JitFormat, make_c_sig,
};
use super::super::helpers;
use super::{
    PositionalFieldInfo, PositionalFieldKind, ShapeMemo, T2_ERR_UNSUPPORTED,
    classify_positional_field, compile_list_format_deserializer, compile_map_format_deserializer,
    func_addr_value, is_format_jit_field_type_supported, tier2_call_sig,
};

/// Helper to emit scalar field parsing with error handling and storage.
///
/// This function encapsulates the common pattern of:
/// 1. Parsing a scalar value using format.emit_parse_*
/// 2. Checking for parse errors
/// 3. Creating and branching to a success block
/// 4. Storing the parsed value to memory
///
/// Returns the success block that subsequent code should continue from.
#[allow(clippy::too_many_arguments)]
fn emit_parse_and_store_scalar<F: JitFormat>(
    format: &F,
    module: &mut JITModule,
    builder: &mut FunctionBuilder,
    cursor: &mut JitCursor,
    field_kind: &PositionalFieldKind,
    dest_ptr: Value,
    err_var: Variable,
    error_block: Block,
    block_to_seal: Option<Block>,
    write_string_sig_ref: SigRef,
    write_string_ptr: Value,
) -> Option<Block> {
    let pointer_type = cursor.ptr_type;

    match field_kind {
        PositionalFieldKind::Bool => {
            let (val, err) = format.emit_parse_bool(module, builder, cursor);
            builder.def_var(err_var, err);
            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
            let store = builder.create_block();
            builder.ins().brif(ok, store, &[], error_block, &[]);
            if let Some(block) = block_to_seal {
                builder.seal_block(block);
            }
            builder.switch_to_block(store);
            builder.ins().store(MemFlags::trusted(), val, dest_ptr, 0);
            Some(store)
        }
        PositionalFieldKind::U8 => {
            let (val, err) = format.emit_parse_u8(module, builder, cursor);
            builder.def_var(err_var, err);
            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
            let store = builder.create_block();
            builder.ins().brif(ok, store, &[], error_block, &[]);
            if let Some(block) = block_to_seal {
                builder.seal_block(block);
            }
            builder.switch_to_block(store);
            builder.ins().store(MemFlags::trusted(), val, dest_ptr, 0);
            Some(store)
        }
        PositionalFieldKind::I8 => {
            let (val, err) = format.emit_parse_i8(module, builder, cursor);
            builder.def_var(err_var, err);
            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
            let store = builder.create_block();
            builder.ins().brif(ok, store, &[], error_block, &[]);
            if let Some(block) = block_to_seal {
                builder.seal_block(block);
            }
            builder.switch_to_block(store);
            builder.ins().store(MemFlags::trusted(), val, dest_ptr, 0);
            Some(store)
        }
        PositionalFieldKind::I64(scalar_type) => {
            use facet_core::ScalarType;
            let (val_i64, err) = format.emit_parse_i64(module, builder, cursor);
            builder.def_var(err_var, err);
            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
            let store = builder.create_block();
            builder.ins().brif(ok, store, &[], error_block, &[]);
            if let Some(block) = block_to_seal {
                builder.seal_block(block);
            }
            builder.switch_to_block(store);
            let val = match scalar_type {
                ScalarType::I8 => builder.ins().ireduce(types::I8, val_i64),
                ScalarType::I16 => builder.ins().ireduce(types::I16, val_i64),
                ScalarType::I32 => builder.ins().ireduce(types::I32, val_i64),
                _ => val_i64,
            };
            builder.ins().store(MemFlags::trusted(), val, dest_ptr, 0);
            Some(store)
        }
        PositionalFieldKind::U64(scalar_type) => {
            use facet_core::ScalarType;
            let (val_u64, err) = format.emit_parse_u64(module, builder, cursor);
            builder.def_var(err_var, err);
            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
            let store = builder.create_block();
            builder.ins().brif(ok, store, &[], error_block, &[]);
            if let Some(block) = block_to_seal {
                builder.seal_block(block);
            }
            builder.switch_to_block(store);
            let val = match scalar_type {
                ScalarType::U16 => builder.ins().ireduce(types::I16, val_u64),
                ScalarType::U32 => builder.ins().ireduce(types::I32, val_u64),
                _ => val_u64,
            };
            builder.ins().store(MemFlags::trusted(), val, dest_ptr, 0);
            Some(store)
        }
        PositionalFieldKind::F32 => {
            let (val_f32, err) = format.emit_parse_f32(module, builder, cursor);
            builder.def_var(err_var, err);
            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
            let store = builder.create_block();
            builder.ins().brif(ok, store, &[], error_block, &[]);
            if let Some(block) = block_to_seal {
                builder.seal_block(block);
            }
            builder.switch_to_block(store);
            builder
                .ins()
                .store(MemFlags::trusted(), val_f32, dest_ptr, 0);
            Some(store)
        }
        PositionalFieldKind::F64 => {
            let (val_f64, err) = format.emit_parse_f64(module, builder, cursor);
            builder.def_var(err_var, err);
            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
            let store = builder.create_block();
            builder.ins().brif(ok, store, &[], error_block, &[]);
            if let Some(block) = block_to_seal {
                builder.seal_block(block);
            }
            builder.switch_to_block(store);
            builder
                .ins()
                .store(MemFlags::trusted(), val_f64, dest_ptr, 0);
            Some(store)
        }
        PositionalFieldKind::String => {
            let (string_value, err) = format.emit_parse_string(module, builder, cursor);
            builder.def_var(err_var, err);
            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
            let store = builder.create_block();
            builder.ins().brif(ok, store, &[], error_block, &[]);
            if let Some(block) = block_to_seal {
                builder.seal_block(block);
            }
            builder.switch_to_block(store);
            let zero_offset = builder.ins().iconst(pointer_type, 0);
            builder.ins().call_indirect(
                write_string_sig_ref,
                write_string_ptr,
                &[
                    dest_ptr,
                    zero_offset,
                    string_value.ptr,
                    string_value.len,
                    string_value.cap,
                    string_value.owned,
                ],
            );
            Some(store)
        }
        _ => None, // Non-scalar types not supported by this helper
    }
}

/// Compile a Tier-2 positional struct deserializer.
///
/// For formats like postcard where struct fields are encoded in declaration order
/// without field names or delimiters. This generates straight-line code that:
/// 1. Parses each field in order (no key dispatch)
/// 2. Stores values directly to output at field offsets
/// 3. Returns new position on success
///
/// Unlike the map-based deserializer, this does NOT support:
/// - `#[facet(flatten)]` attributes (fields must be in order)
/// - Missing fields (all fields must be present)
/// - Field reordering (schema must match exactly)
pub(crate) fn compile_struct_positional_deserializer<F: JitFormat>(
    module: &mut JITModule,
    shape: &'static Shape,
    memo: &mut ShapeMemo,
) -> Option<FuncId> {
    jit_diag!("compile_struct_positional_deserializer ENTRY");

    // Check memo first - return cached FuncId if already compiled
    let shape_ptr = shape as *const Shape;
    if let Some(&func_id) = memo.get(&shape_ptr) {
        jit_diag!(
            "compile_struct_positional_deserializer: using memoized FuncId for shape {:p}",
            shape
        );
        return Some(func_id);
    }

    let Type::User(UserType::Struct(struct_def)) = &shape.ty else {
        jit_diag!("Shape is not a struct");
        return None;
    };

    jit_diag!(
        "Compiling positional struct with {} fields",
        struct_def.fields.len()
    );

    // Build field metadata - reject flattened fields (not supported for positional)
    let mut field_infos: Vec<PositionalFieldInfo> = Vec::new();

    for field in struct_def.fields {
        // Reject flatten - positional formats require fixed field order
        if field.is_flattened() {
            jit_diag!(
                "Flattened field '{}' not supported for positional struct encoding",
                field.name
            );
            return None;
        }

        let field_shape = field.shape.get();

        // Check if field type is supported
        if !is_format_jit_field_type_supported(field_shape) {
            jit_diag!(
                "Field '{}' has unsupported type: {:?}",
                field.name,
                field_shape.def
            );
            return None;
        }

        // Classify field type
        let field_kind = classify_positional_field(field_shape)?;

        field_infos.push(PositionalFieldInfo {
            name: field.name,
            offset: field.offset,
            shape: field_shape,
            kind: field_kind,
        });
    }

    jit_diag!("Built {} positional field infos", field_infos.len());

    let pointer_type = module.target_config().pointer_type();

    // Function signature: fn(input_ptr, len, pos, out, scratch) -> isize
    let mut sig = make_c_sig(module);
    sig.params.push(AbiParam::new(pointer_type)); // input_ptr
    sig.params.push(AbiParam::new(pointer_type)); // len
    sig.params.push(AbiParam::new(pointer_type)); // pos
    sig.params.push(AbiParam::new(pointer_type)); // out
    sig.params.push(AbiParam::new(pointer_type)); // scratch
    sig.returns.push(AbiParam::new(pointer_type)); // new_pos or error

    // Create unique function name
    let func_name = format!(
        "jit_deserialize_positional_struct_{:x}",
        shape as *const _ as usize
    );

    let func_id = match module.declare_function(&func_name, Linkage::Export, &sig) {
        Ok(id) => id,
        Err(e) => {
            jit_diag!("declare_function('{}') failed: {:?}", func_name, e);
            return None;
        }
    };

    // Insert into memo immediately to handle recursive types
    memo.insert(shape_ptr, func_id);
    jit_diag!("Function declared, starting IR generation");

    let mut ctx = module.make_context();
    ctx.func.signature = sig;

    let mut builder_ctx = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);
        let nested_call_sig_ref = builder.import_signature(tier2_call_sig(module, pointer_type));

        let entry = builder.create_block();
        builder.switch_to_block(entry);
        builder.append_block_params_for_function_params(entry);

        // Get function parameters
        let input_ptr = builder.block_params(entry)[0];
        let len = builder.block_params(entry)[1];
        let pos_param = builder.block_params(entry)[2];
        let out_ptr = builder.block_params(entry)[3];
        let scratch_ptr = builder.block_params(entry)[4];

        // Create position variable (mutable)
        let pos_var = builder.declare_var(pointer_type);
        builder.def_var(pos_var, pos_param);

        // Variable for error code
        let err_var = builder.declare_var(types::I32);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        builder.def_var(err_var, zero_i32);

        // Create basic blocks
        let success = builder.create_block();
        let error = builder.create_block();

        // Import helper signatures
        let sig_write_string = {
            let mut s = make_c_sig(module);
            s.params.push(AbiParam::new(pointer_type)); // out_ptr
            s.params.push(AbiParam::new(pointer_type)); // offset
            s.params.push(AbiParam::new(pointer_type)); // str_ptr
            s.params.push(AbiParam::new(pointer_type)); // str_len
            s.params.push(AbiParam::new(pointer_type)); // str_cap
            s.params.push(AbiParam::new(types::I8)); // str_owned
            s
        };
        let write_string_sig_ref = builder.import_signature(sig_write_string);
        let write_string_ptr = builder
            .ins()
            .iconst(pointer_type, helpers::jit_write_string as *const u8 as i64);

        // Import option init helpers
        let sig_option_init_none = {
            let mut s = make_c_sig(module);
            s.params.push(AbiParam::new(pointer_type)); // out_ptr
            s.params.push(AbiParam::new(pointer_type)); // init_none_fn
            s
        };
        let option_init_none_sig_ref = builder.import_signature(sig_option_init_none);
        let option_init_none_ptr = builder.ins().iconst(
            pointer_type,
            helpers::jit_option_init_none as *const u8 as i64,
        );

        let sig_option_init_some = {
            let mut s = make_c_sig(module);
            s.params.push(AbiParam::new(pointer_type)); // out_ptr
            s.params.push(AbiParam::new(pointer_type)); // value_ptr
            s.params.push(AbiParam::new(pointer_type)); // init_some_fn
            s
        };
        let option_init_some_sig_ref = builder.import_signature(sig_option_init_some);
        let option_init_some_ptr = builder.ins().iconst(
            pointer_type,
            helpers::jit_option_init_some_from_value as *const u8 as i64,
        );

        let format = F::default();

        // Current block for chaining field parsing
        let mut current_block = entry;

        // Handle empty struct (unit struct) - just jump to success
        if field_infos.is_empty() {
            builder.ins().jump(success, &[]);
            builder.seal_block(entry);
        }

        // Parse each field in order
        for (field_idx, field_info) in field_infos.iter().enumerate() {
            // Switch to current_block at start of each field
            // (skip first iteration since we're already in entry block)
            if field_idx > 0 {
                builder.switch_to_block(current_block);
            }

            let field_ptr = builder.ins().iadd_imm(out_ptr, field_info.offset as i64);

            let mut cursor = JitCursor {
                input_ptr,
                len,
                pos: pos_var,
                ptr_type: pointer_type,
            };

            // Create next block for after this field
            let next_block = if field_idx < field_infos.len() - 1 {
                builder.create_block()
            } else {
                success
            };

            match &field_info.kind {
                PositionalFieldKind::Bool => {
                    let (value_i8, err) = format.emit_parse_bool(module, &mut builder, &mut cursor);
                    builder.def_var(err_var, err);
                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                    let store_block = builder.create_block();
                    builder.ins().brif(is_ok, store_block, &[], error, &[]);
                    builder.seal_block(current_block);

                    builder.switch_to_block(store_block);
                    builder
                        .ins()
                        .store(MemFlags::trusted(), value_i8, field_ptr, 0);
                    builder.ins().jump(next_block, &[]);
                    builder.seal_block(store_block);
                    current_block = next_block;
                }

                PositionalFieldKind::U8 => {
                    let (value_u8, err) = format.emit_parse_u8(module, &mut builder, &mut cursor);
                    builder.def_var(err_var, err);
                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                    let store_block = builder.create_block();
                    builder.ins().brif(is_ok, store_block, &[], error, &[]);
                    builder.seal_block(current_block);

                    builder.switch_to_block(store_block);
                    builder
                        .ins()
                        .store(MemFlags::trusted(), value_u8, field_ptr, 0);
                    builder.ins().jump(next_block, &[]);
                    builder.seal_block(store_block);
                    current_block = next_block;
                }

                PositionalFieldKind::I8 => {
                    let (value_i8, err) = format.emit_parse_i8(module, &mut builder, &mut cursor);
                    builder.def_var(err_var, err);
                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                    let store_block = builder.create_block();
                    builder.ins().brif(is_ok, store_block, &[], error, &[]);
                    builder.seal_block(current_block);

                    builder.switch_to_block(store_block);
                    builder
                        .ins()
                        .store(MemFlags::trusted(), value_i8, field_ptr, 0);
                    builder.ins().jump(next_block, &[]);
                    builder.seal_block(store_block);
                    current_block = next_block;
                }

                PositionalFieldKind::I64(scalar_type) => {
                    use facet_core::ScalarType;
                    let (value_i64, err) = format.emit_parse_i64(module, &mut builder, &mut cursor);
                    builder.def_var(err_var, err);
                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                    let store_block = builder.create_block();
                    builder.ins().brif(is_ok, store_block, &[], error, &[]);
                    builder.seal_block(current_block);

                    builder.switch_to_block(store_block);
                    let value = match scalar_type {
                        ScalarType::I8 => builder.ins().ireduce(types::I8, value_i64),
                        ScalarType::I16 => builder.ins().ireduce(types::I16, value_i64),
                        ScalarType::I32 => builder.ins().ireduce(types::I32, value_i64),
                        _ => value_i64,
                    };
                    builder
                        .ins()
                        .store(MemFlags::trusted(), value, field_ptr, 0);
                    builder.ins().jump(next_block, &[]);
                    builder.seal_block(store_block);
                    current_block = next_block;
                }

                PositionalFieldKind::U64(scalar_type) => {
                    use facet_core::ScalarType;
                    let (value_u64, err) = format.emit_parse_u64(module, &mut builder, &mut cursor);
                    builder.def_var(err_var, err);
                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                    let store_block = builder.create_block();
                    builder.ins().brif(is_ok, store_block, &[], error, &[]);
                    builder.seal_block(current_block);

                    builder.switch_to_block(store_block);
                    let value = match scalar_type {
                        ScalarType::U16 => builder.ins().ireduce(types::I16, value_u64),
                        ScalarType::U32 => builder.ins().ireduce(types::I32, value_u64),
                        _ => value_u64,
                    };
                    builder
                        .ins()
                        .store(MemFlags::trusted(), value, field_ptr, 0);
                    builder.ins().jump(next_block, &[]);
                    builder.seal_block(store_block);
                    current_block = next_block;
                }

                PositionalFieldKind::F32 => {
                    let (value_f32, err) = format.emit_parse_f32(module, &mut builder, &mut cursor);
                    builder.def_var(err_var, err);
                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                    let store_block = builder.create_block();
                    builder.ins().brif(is_ok, store_block, &[], error, &[]);
                    builder.seal_block(current_block);

                    builder.switch_to_block(store_block);
                    builder
                        .ins()
                        .store(MemFlags::trusted(), value_f32, field_ptr, 0);
                    builder.ins().jump(next_block, &[]);
                    builder.seal_block(store_block);
                    current_block = next_block;
                }

                PositionalFieldKind::F64 => {
                    let (value_f64, err) = format.emit_parse_f64(module, &mut builder, &mut cursor);
                    builder.def_var(err_var, err);
                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                    let store_block = builder.create_block();
                    builder.ins().brif(is_ok, store_block, &[], error, &[]);
                    builder.seal_block(current_block);

                    builder.switch_to_block(store_block);
                    builder
                        .ins()
                        .store(MemFlags::trusted(), value_f64, field_ptr, 0);
                    builder.ins().jump(next_block, &[]);
                    builder.seal_block(store_block);
                    current_block = next_block;
                }

                PositionalFieldKind::String => {
                    let (string_value, err) =
                        format.emit_parse_string(module, &mut builder, &mut cursor);
                    builder.def_var(err_var, err);
                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                    let store_block = builder.create_block();
                    builder.ins().brif(is_ok, store_block, &[], error, &[]);
                    builder.seal_block(current_block);

                    builder.switch_to_block(store_block);
                    let zero_offset = builder.ins().iconst(pointer_type, 0);
                    builder.ins().call_indirect(
                        write_string_sig_ref,
                        write_string_ptr,
                        &[
                            field_ptr,
                            zero_offset,
                            string_value.ptr,
                            string_value.len,
                            string_value.cap,
                            string_value.owned,
                        ],
                    );
                    builder.ins().jump(next_block, &[]);
                    builder.seal_block(store_block);
                    current_block = next_block;
                }

                PositionalFieldKind::Option(opt_def) => {
                    // For positional formats, Option<T> uses discriminant encoding:
                    // 0x00 = None, 0x01 = Some(value)

                    // Read discriminant byte (using emit_parse_u8)
                    let (disc_byte, err) = format.emit_parse_u8(module, &mut builder, &mut cursor);
                    builder.def_var(err_var, err);
                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                    let check_disc = builder.create_block();
                    builder.ins().brif(is_ok, check_disc, &[], error, &[]);
                    builder.seal_block(current_block);

                    // Check discriminant value
                    builder.switch_to_block(check_disc);
                    let is_none = builder.ins().icmp_imm(IntCC::Equal, disc_byte, 0);
                    let none_block = builder.create_block();
                    let check_some = builder.create_block();
                    builder
                        .ins()
                        .brif(is_none, none_block, &[], check_some, &[]);
                    builder.seal_block(check_disc);

                    // None case: call init_none
                    builder.switch_to_block(none_block);
                    let init_none_fn_ptr = builder
                        .ins()
                        .iconst(pointer_type, opt_def.vtable.init_none as *const () as i64);
                    builder.ins().call_indirect(
                        option_init_none_sig_ref,
                        option_init_none_ptr,
                        &[field_ptr, init_none_fn_ptr],
                    );
                    builder.ins().jump(next_block, &[]);
                    builder.seal_block(none_block);

                    // Check if Some (disc == 1)
                    builder.switch_to_block(check_some);
                    let is_some = builder.ins().icmp_imm(IntCC::Equal, disc_byte, 1);
                    let some_block = builder.create_block();
                    let invalid_disc = builder.create_block();
                    builder
                        .ins()
                        .brif(is_some, some_block, &[], invalid_disc, &[]);
                    builder.seal_block(check_some);

                    // Invalid discriminant error
                    builder.switch_to_block(invalid_disc);
                    let err_code = builder
                        .ins()
                        .iconst(types::I32, helpers::ERR_INVALID_OPTION_DISCRIMINANT as i64);
                    builder.def_var(err_var, err_code);
                    builder.ins().jump(error, &[]);
                    builder.seal_block(invalid_disc);

                    // Some case: parse inner value, then call init_some
                    builder.switch_to_block(some_block);

                    // Get inner type shape
                    let inner_shape = opt_def.t;

                    // Allocate stack slot for inner value
                    let inner_layout = match inner_shape.layout.sized_layout() {
                        Ok(l) => l,
                        Err(_) => {
                            jit_diag!(
                                "Field '{}' Option inner type has unsized layout",
                                field_info.name
                            );
                            return None;
                        }
                    };
                    let inner_size = inner_layout.size() as u32;
                    let inner_align = inner_layout.align().trailing_zeros() as u8;
                    let inner_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        inner_size,
                        inner_align,
                    ));
                    let inner_ptr = builder.ins().stack_addr(pointer_type, inner_slot, 0);

                    // Parse inner value - dispatch based on inner type
                    let inner_kind = classify_positional_field(inner_shape);
                    let inner_kind = match inner_kind {
                        Some(k) => k,
                        None => {
                            jit_diag!(
                                "Field '{}' Option inner type not supported",
                                field_info.name
                            );
                            return None;
                        }
                    };

                    // We need to emit parsing code for the inner value
                    // For simplicity, handle scalar types inline; for complex types, call nested deserializer
                    let inner_parsed = if let Some(store_block) = emit_parse_and_store_scalar(
                        &format,
                        module,
                        &mut builder,
                        &mut JitCursor {
                            input_ptr,
                            len,
                            pos: pos_var,
                            ptr_type: pointer_type,
                        },
                        &inner_kind,
                        inner_ptr,
                        err_var,
                        error,
                        Some(some_block),
                        write_string_sig_ref,
                        write_string_ptr,
                    ) {
                        store_block
                    } else {
                        match inner_kind {
                            PositionalFieldKind::Struct(nested_shape) => {
                                // Call nested struct deserializer
                                let nested_func_id = compile_struct_positional_deserializer::<F>(
                                    module,
                                    nested_shape,
                                    memo,
                                )?;
                                let nested_func_ref =
                                    module.declare_func_in_func(nested_func_id, builder.func);
                                let nested_func_ptr =
                                    func_addr_value(&mut builder, pointer_type, nested_func_ref);
                                let current_pos = builder.use_var(pos_var);
                                let call_result = builder.ins().call_indirect(
                                    nested_call_sig_ref,
                                    nested_func_ptr,
                                    &[input_ptr, len, current_pos, inner_ptr, scratch_ptr],
                                );
                                let new_pos = builder.inst_results(call_result)[0];
                                let is_err =
                                    builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
                                let nested_ok = builder.create_block();
                                builder.ins().brif(is_err, error, &[], nested_ok, &[]);
                                builder.seal_block(some_block);
                                builder.switch_to_block(nested_ok);
                                builder.def_var(pos_var, new_pos);
                                nested_ok
                            }
                            PositionalFieldKind::List(list_shape) => {
                                let list_func_id = compile_list_format_deserializer::<F>(
                                    module, list_shape, memo,
                                )?;
                                let list_func_ref =
                                    module.declare_func_in_func(list_func_id, builder.func);
                                let list_func_ptr =
                                    func_addr_value(&mut builder, pointer_type, list_func_ref);
                                let current_pos = builder.use_var(pos_var);
                                let call_result = builder.ins().call_indirect(
                                    nested_call_sig_ref,
                                    list_func_ptr,
                                    &[input_ptr, len, current_pos, inner_ptr, scratch_ptr],
                                );
                                let new_pos = builder.inst_results(call_result)[0];
                                let is_err =
                                    builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
                                let nested_ok = builder.create_block();
                                builder.ins().brif(is_err, error, &[], nested_ok, &[]);
                                builder.seal_block(some_block);
                                builder.switch_to_block(nested_ok);
                                builder.def_var(pos_var, new_pos);
                                nested_ok
                            }
                            PositionalFieldKind::Map(map_shape) => {
                                let map_func_id =
                                    compile_map_format_deserializer::<F>(module, map_shape, memo)?;
                                let map_func_ref =
                                    module.declare_func_in_func(map_func_id, builder.func);
                                let map_func_ptr =
                                    func_addr_value(&mut builder, pointer_type, map_func_ref);
                                let current_pos = builder.use_var(pos_var);
                                let call_result = builder.ins().call_indirect(
                                    nested_call_sig_ref,
                                    map_func_ptr,
                                    &[input_ptr, len, current_pos, inner_ptr, scratch_ptr],
                                );
                                let new_pos = builder.inst_results(call_result)[0];
                                let is_err =
                                    builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
                                let nested_ok = builder.create_block();
                                builder.ins().brif(is_err, error, &[], nested_ok, &[]);
                                builder.seal_block(some_block);
                                builder.switch_to_block(nested_ok);
                                builder.def_var(pos_var, new_pos);
                                nested_ok
                            }
                            PositionalFieldKind::Option(_) => {
                                // Nested Option - not commonly used but supported
                                jit_diag!(
                                    "Field '{}' nested Option<Option<T>> not yet supported",
                                    field_info.name
                                );
                                return None;
                            }
                            PositionalFieldKind::Result(_) => {
                                // Nested Result - Option<Result<T, E>>
                                jit_diag!(
                                    "Field '{}' nested Option<Result<T, E>> not yet supported",
                                    field_info.name
                                );
                                return None;
                            }
                            PositionalFieldKind::Enum(enum_shape) => {
                                // Option<Enum> - parse the inner enum into inner_ptr
                                use facet_core::Type;
                                use facet_core::UserType;

                                // Extract enum definition
                                let Type::User(UserType::Enum(enum_def)) = &enum_shape.ty else {
                                    jit_diag!(
                                        "Field '{}' Option<Enum> inner shape is not an enum",
                                        field_info.name
                                    );
                                    return None;
                                };

                                // Parse discriminant as varint
                                let mut inner_cursor = JitCursor {
                                    input_ptr,
                                    len,
                                    pos: pos_var,
                                    ptr_type: pointer_type,
                                };
                                let (discriminant, err) =
                                    format.emit_parse_u64(module, &mut builder, &mut inner_cursor);
                                builder.def_var(err_var, err);
                                let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                                let disc_ok_block = builder.create_block();
                                builder.ins().brif(is_ok, disc_ok_block, &[], error, &[]);
                                builder.seal_block(some_block);

                                builder.switch_to_block(disc_ok_block);

                                // Create blocks for variant dispatch
                                let mut variant_blocks: Vec<_> = (0..enum_def.variants.len())
                                    .map(|_| builder.create_block())
                                    .collect();
                                let invalid_discriminant_block = builder.create_block();
                                let after_enum_parse = builder.create_block();

                                // Dispatch on discriminant
                                let mut current_check_block = disc_ok_block;
                                for (i, variant) in enum_def.variants.iter().enumerate() {
                                    let disc_val = match variant.discriminant {
                                        Some(v) => v as u64,
                                        None => {
                                            jit_diag!(
                                                "Field '{}' Option<Enum> variant '{}' has no discriminant",
                                                field_info.name,
                                                variant.name
                                            );
                                            return None;
                                        }
                                    };
                                    let matches = builder.ins().icmp_imm(
                                        IntCC::Equal,
                                        discriminant,
                                        disc_val as i64,
                                    );

                                    let next_check_block = if i < enum_def.variants.len() - 1 {
                                        builder.create_block()
                                    } else {
                                        invalid_discriminant_block
                                    };

                                    builder.ins().brif(
                                        matches,
                                        variant_blocks[i],
                                        &[],
                                        next_check_block,
                                        &[],
                                    );
                                    builder.seal_block(current_check_block);

                                    if i < enum_def.variants.len() - 1 {
                                        builder.switch_to_block(next_check_block);
                                        current_check_block = next_check_block;
                                    }
                                }

                                // Generate code for each variant
                                for (i, variant) in enum_def.variants.iter().enumerate() {
                                    builder.switch_to_block(variant_blocks[i]);

                                    // Store discriminant to inner_ptr
                                    let disc_val = variant.discriminant.unwrap();
                                    match enum_def.enum_repr {
                                        facet_core::EnumRepr::U8 | facet_core::EnumRepr::I8 => {
                                            let disc_i8 = builder.ins().iconst(types::I8, disc_val);
                                            builder.ins().store(
                                                MemFlags::trusted(),
                                                disc_i8,
                                                inner_ptr,
                                                0,
                                            );
                                        }
                                        facet_core::EnumRepr::U16 | facet_core::EnumRepr::I16 => {
                                            let disc_i16 =
                                                builder.ins().iconst(types::I16, disc_val);
                                            builder.ins().store(
                                                MemFlags::trusted(),
                                                disc_i16,
                                                inner_ptr,
                                                0,
                                            );
                                        }
                                        facet_core::EnumRepr::U32 | facet_core::EnumRepr::I32 => {
                                            let disc_i32 =
                                                builder.ins().iconst(types::I32, disc_val);
                                            builder.ins().store(
                                                MemFlags::trusted(),
                                                disc_i32,
                                                inner_ptr,
                                                0,
                                            );
                                        }
                                        facet_core::EnumRepr::U64
                                        | facet_core::EnumRepr::I64
                                        | facet_core::EnumRepr::USize
                                        | facet_core::EnumRepr::ISize => {
                                            let disc_i64 =
                                                builder.ins().iconst(types::I64, disc_val);
                                            builder.ins().store(
                                                MemFlags::trusted(),
                                                disc_i64,
                                                inner_ptr,
                                                0,
                                            );
                                        }
                                        facet_core::EnumRepr::RustNPO => {
                                            jit_diag!(
                                                "Field '{}' Option<Enum> uses RustNPO repr (not yet supported)",
                                                field_info.name
                                            );
                                            return None;
                                        }
                                    }

                                    // Parse variant data
                                    use facet_core::StructKind;
                                    match variant.data.kind {
                                        StructKind::Unit => {
                                            // No data to parse
                                            builder.ins().jump(after_enum_parse, &[]);
                                        }
                                        StructKind::TupleStruct
                                        | StructKind::Struct
                                        | StructKind::Tuple => {
                                            // Parse each field
                                            for field in variant.data.fields {
                                                let field_shape = field.shape.get();
                                                let field_kind =
                                                    classify_positional_field(field_shape)?;

                                                let field_offset = builder
                                                    .ins()
                                                    .iconst(pointer_type, field.offset as i64);
                                                let variant_field_ptr =
                                                    builder.ins().iadd(inner_ptr, field_offset);

                                                match field_kind {
                                                    PositionalFieldKind::U8 => {
                                                        let (val, err) = format.emit_parse_u8(
                                                            module,
                                                            &mut builder,
                                                            &mut inner_cursor,
                                                        );
                                                        builder.def_var(err_var, err);
                                                        let ok = builder.ins().icmp_imm(
                                                            IntCC::Equal,
                                                            err,
                                                            0,
                                                        );
                                                        let store_block = builder.create_block();
                                                        builder.ins().brif(
                                                            ok,
                                                            store_block,
                                                            &[],
                                                            error,
                                                            &[],
                                                        );
                                                        builder.seal_block(variant_blocks[i]);

                                                        builder.switch_to_block(store_block);
                                                        builder.ins().store(
                                                            MemFlags::trusted(),
                                                            val,
                                                            variant_field_ptr,
                                                            0,
                                                        );
                                                        variant_blocks[i] = store_block;
                                                    }
                                                    PositionalFieldKind::I64(scalar_type) => {
                                                        use facet_core::ScalarType;
                                                        let (val_i64, err) = format.emit_parse_i64(
                                                            module,
                                                            &mut builder,
                                                            &mut inner_cursor,
                                                        );
                                                        builder.def_var(err_var, err);
                                                        let ok = builder.ins().icmp_imm(
                                                            IntCC::Equal,
                                                            err,
                                                            0,
                                                        );
                                                        let store_block = builder.create_block();
                                                        builder.ins().brif(
                                                            ok,
                                                            store_block,
                                                            &[],
                                                            error,
                                                            &[],
                                                        );
                                                        builder.seal_block(variant_blocks[i]);

                                                        builder.switch_to_block(store_block);
                                                        let value = match scalar_type {
                                                            ScalarType::I8 => builder
                                                                .ins()
                                                                .ireduce(types::I8, val_i64),
                                                            ScalarType::I16 => builder
                                                                .ins()
                                                                .ireduce(types::I16, val_i64),
                                                            ScalarType::I32 => builder
                                                                .ins()
                                                                .ireduce(types::I32, val_i64),
                                                            _ => val_i64,
                                                        };
                                                        builder.ins().store(
                                                            MemFlags::trusted(),
                                                            value,
                                                            variant_field_ptr,
                                                            0,
                                                        );
                                                        variant_blocks[i] = store_block;
                                                    }
                                                    PositionalFieldKind::U64(scalar_type) => {
                                                        use facet_core::ScalarType;
                                                        let (val_u64, err) = format.emit_parse_u64(
                                                            module,
                                                            &mut builder,
                                                            &mut inner_cursor,
                                                        );
                                                        builder.def_var(err_var, err);
                                                        let ok = builder.ins().icmp_imm(
                                                            IntCC::Equal,
                                                            err,
                                                            0,
                                                        );
                                                        let store_block = builder.create_block();
                                                        builder.ins().brif(
                                                            ok,
                                                            store_block,
                                                            &[],
                                                            error,
                                                            &[],
                                                        );
                                                        builder.seal_block(variant_blocks[i]);

                                                        builder.switch_to_block(store_block);
                                                        let value = match scalar_type {
                                                            ScalarType::U16 => builder
                                                                .ins()
                                                                .ireduce(types::I16, val_u64),
                                                            ScalarType::U32 => builder
                                                                .ins()
                                                                .ireduce(types::I32, val_u64),
                                                            _ => val_u64,
                                                        };
                                                        builder.ins().store(
                                                            MemFlags::trusted(),
                                                            value,
                                                            variant_field_ptr,
                                                            0,
                                                        );
                                                        variant_blocks[i] = store_block;
                                                    }
                                                    PositionalFieldKind::String => {
                                                        let (string_value, err) = format
                                                            .emit_parse_string(
                                                                module,
                                                                &mut builder,
                                                                &mut inner_cursor,
                                                            );
                                                        builder.def_var(err_var, err);
                                                        let ok = builder.ins().icmp_imm(
                                                            IntCC::Equal,
                                                            err,
                                                            0,
                                                        );
                                                        let store_block = builder.create_block();
                                                        builder.ins().brif(
                                                            ok,
                                                            store_block,
                                                            &[],
                                                            error,
                                                            &[],
                                                        );
                                                        builder.seal_block(variant_blocks[i]);

                                                        builder.switch_to_block(store_block);
                                                        let zero_offset =
                                                            builder.ins().iconst(pointer_type, 0);
                                                        builder.ins().call_indirect(
                                                            write_string_sig_ref,
                                                            write_string_ptr,
                                                            &[
                                                                variant_field_ptr,
                                                                zero_offset,
                                                                string_value.ptr,
                                                                string_value.len,
                                                                string_value.cap,
                                                                string_value.owned,
                                                            ],
                                                        );
                                                        variant_blocks[i] = store_block;
                                                    }
                                                    _ => {
                                                        jit_diag!(
                                                            "Field '{}' Option<Enum> variant '{}' field '{}' type not yet supported",
                                                            field_info.name,
                                                            variant.name,
                                                            field.name
                                                        );
                                                        return None;
                                                    }
                                                }
                                            }

                                            builder.ins().jump(after_enum_parse, &[]);
                                        }
                                    }
                                    builder.seal_block(variant_blocks[i]);
                                }

                                // Invalid discriminant - error
                                builder.switch_to_block(invalid_discriminant_block);
                                builder.seal_block(invalid_discriminant_block);
                                let invalid_err =
                                    builder.ins().iconst(types::I32, T2_ERR_UNSUPPORTED as i64);
                                builder.def_var(err_var, invalid_err);
                                builder.ins().jump(error, &[]);

                                // After enum parse
                                builder.switch_to_block(after_enum_parse);
                                builder.seal_block(after_enum_parse);
                                after_enum_parse
                            }
                            _ => {
                                // Scalar types should have been handled by emit_parse_and_store_scalar
                                jit_diag!(
                                    "Field '{}' Option inner type {:?} not supported",
                                    field_info.name,
                                    inner_kind
                                );
                                return None;
                            }
                        }
                    };

                    // Call init_some to move value into Option
                    let init_some_fn_ptr = builder
                        .ins()
                        .iconst(pointer_type, opt_def.vtable.init_some as *const () as i64);
                    builder.ins().call_indirect(
                        option_init_some_sig_ref,
                        option_init_some_ptr,
                        &[field_ptr, inner_ptr, init_some_fn_ptr],
                    );
                    builder.ins().jump(next_block, &[]);
                    builder.seal_block(inner_parsed);
                    current_block = next_block;
                }

                PositionalFieldKind::Result(result_def) => {
                    // For positional formats, Result<T, E> uses discriminant encoding:
                    // 0x00 = Ok, 0x01 = Err

                    // Read discriminant byte
                    let (disc_byte, err) = format.emit_parse_u8(module, &mut builder, &mut cursor);
                    builder.def_var(err_var, err);
                    let is_ok_parse = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                    let check_disc = builder.create_block();
                    builder.ins().brif(is_ok_parse, check_disc, &[], error, &[]);
                    builder.seal_block(current_block);

                    // Check discriminant value
                    builder.switch_to_block(check_disc);
                    let is_ok_variant = builder.ins().icmp_imm(IntCC::Equal, disc_byte, 0);
                    let ok_block = builder.create_block();
                    let check_err = builder.create_block();
                    builder
                        .ins()
                        .brif(is_ok_variant, ok_block, &[], check_err, &[]);
                    builder.seal_block(check_disc);

                    // Ok case: parse T value and call jit_result_init_ok_from_value
                    builder.switch_to_block(ok_block);
                    let ok_shape = result_def.t;

                    // Allocate stack slot for Ok value
                    let ok_layout = match ok_shape.layout.sized_layout() {
                        Ok(l) => l,
                        Err(_) => {
                            jit_diag!(
                                "Field '{}' Result::Ok type has unsized layout",
                                field_info.name
                            );
                            return None;
                        }
                    };
                    let ok_size = ok_layout.size() as u32;
                    let ok_align = ok_layout.align().trailing_zeros() as u8;
                    let ok_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        ok_size,
                        ok_align,
                    ));
                    let ok_ptr = builder.ins().stack_addr(pointer_type, ok_slot, 0);

                    // Parse Ok value based on its type (similar to Option::Some handling)
                    let ok_kind = match classify_positional_field(ok_shape) {
                        Some(k) => k,
                        None => {
                            jit_diag!(
                                "Field '{}' Result::Ok type not supported for JIT",
                                field_info.name
                            );
                            return None;
                        }
                    };

                    // Parse Ok value using helper (supports all scalar types)
                    let check_ok = match emit_parse_and_store_scalar(
                        &format,
                        module,
                        &mut builder,
                        &mut cursor,
                        &ok_kind,
                        ok_ptr,
                        err_var,
                        error,
                        Some(ok_block), // Seal ok_block as we're done with it
                        write_string_sig_ref,
                        write_string_ptr,
                    ) {
                        Some(block) => block,
                        None => {
                            jit_diag!(
                                "Field '{}' Result::Ok type {:?} not supported (only scalar types supported)",
                                field_info.name,
                                ok_kind
                            );
                            return None;
                        }
                    };

                    // Call jit_result_init_ok_from_value
                    let init_ok_fn_ptr = builder
                        .ins()
                        .iconst(pointer_type, result_def.vtable.init_ok as *const () as i64);
                    let result_init_ok_ptr = builder.ins().iconst(
                        pointer_type,
                        helpers::jit_result_init_ok_from_value as *const u8 as i64,
                    );
                    let result_init_ok_sig_ref = builder.import_signature({
                        let mut s = make_c_sig(module);
                        s.params.push(AbiParam::new(pointer_type)); // out
                        s.params.push(AbiParam::new(pointer_type)); // value_ptr
                        s.params.push(AbiParam::new(pointer_type)); // init_ok_fn
                        s
                    });
                    builder.ins().call_indirect(
                        result_init_ok_sig_ref,
                        result_init_ok_ptr,
                        &[field_ptr, ok_ptr, init_ok_fn_ptr],
                    );
                    builder.ins().jump(next_block, &[]);

                    // Err case: check disc == 1, then parse E value and call jit_result_init_err_from_value
                    builder.switch_to_block(check_err);
                    let is_err_variant = builder.ins().icmp_imm(IntCC::Equal, disc_byte, 1);
                    let err_block = builder.create_block();
                    let invalid_disc = builder.create_block();
                    builder
                        .ins()
                        .brif(is_err_variant, err_block, &[], invalid_disc, &[]);
                    builder.seal_block(check_err);

                    // Invalid discriminant error
                    builder.switch_to_block(invalid_disc);
                    let err_code = builder.ins().iconst(types::I32, T2_ERR_UNSUPPORTED as i64);
                    builder.def_var(err_var, err_code);
                    builder.ins().jump(error, &[]);
                    builder.seal_block(invalid_disc);

                    // Err case: parse E value
                    builder.switch_to_block(err_block);
                    let err_shape = result_def.e;

                    // Allocate stack slot for Err value
                    let err_layout = match err_shape.layout.sized_layout() {
                        Ok(l) => l,
                        Err(_) => {
                            jit_diag!(
                                "Field '{}' Result::Err type has unsized layout",
                                field_info.name
                            );
                            return None;
                        }
                    };
                    let err_size = err_layout.size() as u32;
                    let err_align = err_layout.align().trailing_zeros() as u8;
                    let err_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        err_size,
                        err_align,
                    ));
                    let err_ptr = builder.ins().stack_addr(pointer_type, err_slot, 0);

                    // Parse Err value
                    let err_kind = match classify_positional_field(err_shape) {
                        Some(k) => k,
                        None => {
                            jit_diag!(
                                "Field '{}' Result::Err type not supported for JIT",
                                field_info.name
                            );
                            return None;
                        }
                    };

                    // Parse Err value using helper (supports all scalar types)
                    let check_err = match emit_parse_and_store_scalar(
                        &format,
                        module,
                        &mut builder,
                        &mut cursor,
                        &err_kind,
                        err_ptr,
                        err_var,
                        error,
                        Some(err_block), // Seal err_block as we're done with it
                        write_string_sig_ref,
                        write_string_ptr,
                    ) {
                        Some(block) => block,
                        None => {
                            jit_diag!(
                                "Field '{}' Result::Err type {:?} not supported (only scalar types supported)",
                                field_info.name,
                                err_kind
                            );
                            return None;
                        }
                    };

                    // Call jit_result_init_err_from_value
                    let init_err_fn_ptr = builder
                        .ins()
                        .iconst(pointer_type, result_def.vtable.init_err as *const () as i64);
                    let result_init_err_ptr = builder.ins().iconst(
                        pointer_type,
                        helpers::jit_result_init_err_from_value as *const u8 as i64,
                    );
                    let result_init_err_sig_ref = builder.import_signature({
                        let mut s = make_c_sig(module);
                        s.params.push(AbiParam::new(pointer_type)); // out
                        s.params.push(AbiParam::new(pointer_type)); // value_ptr
                        s.params.push(AbiParam::new(pointer_type)); // init_err_fn
                        s
                    });
                    builder.ins().call_indirect(
                        result_init_err_sig_ref,
                        result_init_err_ptr,
                        &[field_ptr, err_ptr, init_err_fn_ptr],
                    );
                    builder.ins().jump(next_block, &[]);
                    // Note: ok_block and err_block are sealed by emit_parse_and_store_scalar
                    // The check_ok and check_err blocks (returned by the helper) are sealed below
                    builder.seal_block(check_ok);
                    builder.seal_block(check_err);
                    current_block = next_block;
                }

                PositionalFieldKind::Struct(nested_shape) => {
                    // Call nested struct deserializer
                    let nested_func_id =
                        compile_struct_positional_deserializer::<F>(module, nested_shape, memo)?;
                    let nested_func_ref = module.declare_func_in_func(nested_func_id, builder.func);
                    let nested_func_ptr =
                        func_addr_value(&mut builder, pointer_type, nested_func_ref);
                    let current_pos = builder.use_var(pos_var);
                    let call_result = builder.ins().call_indirect(
                        nested_call_sig_ref,
                        nested_func_ptr,
                        &[input_ptr, len, current_pos, field_ptr, scratch_ptr],
                    );
                    let new_pos = builder.inst_results(call_result)[0];
                    let is_err = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
                    let nested_ok = builder.create_block();
                    builder.ins().brif(is_err, error, &[], nested_ok, &[]);
                    builder.seal_block(current_block);

                    builder.switch_to_block(nested_ok);
                    builder.def_var(pos_var, new_pos);
                    builder.ins().jump(next_block, &[]);
                    builder.seal_block(nested_ok);
                    current_block = next_block;
                }

                PositionalFieldKind::List(list_shape) => {
                    // Call list deserializer
                    let list_func_id =
                        compile_list_format_deserializer::<F>(module, list_shape, memo)?;
                    let list_func_ref = module.declare_func_in_func(list_func_id, builder.func);
                    let list_func_ptr = func_addr_value(&mut builder, pointer_type, list_func_ref);
                    let current_pos = builder.use_var(pos_var);
                    let call_result = builder.ins().call_indirect(
                        nested_call_sig_ref,
                        list_func_ptr,
                        &[input_ptr, len, current_pos, field_ptr, scratch_ptr],
                    );
                    let new_pos = builder.inst_results(call_result)[0];
                    let is_err = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
                    let nested_ok = builder.create_block();
                    builder.ins().brif(is_err, error, &[], nested_ok, &[]);
                    builder.seal_block(current_block);

                    builder.switch_to_block(nested_ok);
                    builder.def_var(pos_var, new_pos);
                    builder.ins().jump(next_block, &[]);
                    builder.seal_block(nested_ok);
                    current_block = next_block;
                }

                PositionalFieldKind::Map(map_shape) => {
                    // Call map deserializer
                    let map_func_id =
                        compile_map_format_deserializer::<F>(module, map_shape, memo)?;
                    let map_func_ref = module.declare_func_in_func(map_func_id, builder.func);
                    let map_func_ptr = func_addr_value(&mut builder, pointer_type, map_func_ref);
                    let current_pos = builder.use_var(pos_var);
                    let call_result = builder.ins().call_indirect(
                        nested_call_sig_ref,
                        map_func_ptr,
                        &[input_ptr, len, current_pos, field_ptr, scratch_ptr],
                    );
                    let new_pos = builder.inst_results(call_result)[0];
                    let is_err = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
                    let nested_ok = builder.create_block();
                    builder.ins().brif(is_err, error, &[], nested_ok, &[]);
                    builder.seal_block(current_block);

                    builder.switch_to_block(nested_ok);
                    builder.def_var(pos_var, new_pos);
                    builder.ins().jump(next_block, &[]);
                    builder.seal_block(nested_ok);
                    current_block = next_block;
                }

                PositionalFieldKind::Enum(enum_shape) => {
                    use facet_core::Type;
                    use facet_core::UserType;

                    // Extract enum definition
                    let Type::User(UserType::Enum(enum_def)) = &enum_shape.ty else {
                        jit_diag!("Field '{}' enum shape is not an enum", field_info.name);
                        return None;
                    };

                    // Parse discriminant as varint
                    let (discriminant, err) =
                        format.emit_parse_u64(module, &mut builder, &mut cursor);
                    builder.def_var(err_var, err);
                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                    let disc_ok_block = builder.create_block();
                    builder.ins().brif(is_ok, disc_ok_block, &[], error, &[]);
                    builder.seal_block(current_block);

                    builder.switch_to_block(disc_ok_block);

                    // Find the matching variant - create a switch on discriminant
                    // We'll create blocks for each variant and a default error block
                    let mut variant_blocks: Vec<_> = (0..enum_def.variants.len())
                        .map(|_| builder.create_block())
                        .collect();
                    let invalid_discriminant_block = builder.create_block();
                    let after_variant_block = builder.create_block();

                    // Emit a chain of if-then-else for discriminant dispatch
                    // This is less efficient than a jump table but simpler to implement correctly
                    let mut current_check_block = disc_ok_block;
                    for (i, variant) in enum_def.variants.iter().enumerate() {
                        let disc_val = match variant.discriminant {
                            Some(v) => v as u64,
                            None => {
                                jit_diag!(
                                    "Field '{}' variant '{}' has no discriminant value",
                                    field_info.name,
                                    variant.name
                                );
                                return None;
                            }
                        };
                        let matches =
                            builder
                                .ins()
                                .icmp_imm(IntCC::Equal, discriminant, disc_val as i64);

                        let next_check_block = if i < enum_def.variants.len() - 1 {
                            builder.create_block()
                        } else {
                            invalid_discriminant_block
                        };

                        builder
                            .ins()
                            .brif(matches, variant_blocks[i], &[], next_check_block, &[]);
                        builder.seal_block(current_check_block);

                        if i < enum_def.variants.len() - 1 {
                            builder.switch_to_block(next_check_block);
                            current_check_block = next_check_block;
                        }
                    }

                    // Generate code for each variant block
                    for (i, variant) in enum_def.variants.iter().enumerate() {
                        builder.switch_to_block(variant_blocks[i]);

                        // Store discriminant to memory at field_ptr (base of enum)
                        let disc_val = variant.discriminant.unwrap();
                        match enum_def.enum_repr {
                            facet_core::EnumRepr::U8 | facet_core::EnumRepr::I8 => {
                                let disc_i8 = builder.ins().iconst(types::I8, disc_val);
                                builder
                                    .ins()
                                    .store(MemFlags::trusted(), disc_i8, field_ptr, 0);
                            }
                            facet_core::EnumRepr::U16 | facet_core::EnumRepr::I16 => {
                                let disc_i16 = builder.ins().iconst(types::I16, disc_val);
                                builder
                                    .ins()
                                    .store(MemFlags::trusted(), disc_i16, field_ptr, 0);
                            }
                            facet_core::EnumRepr::U32 | facet_core::EnumRepr::I32 => {
                                let disc_i32 = builder.ins().iconst(types::I32, disc_val);
                                builder
                                    .ins()
                                    .store(MemFlags::trusted(), disc_i32, field_ptr, 0);
                            }
                            facet_core::EnumRepr::U64
                            | facet_core::EnumRepr::I64
                            | facet_core::EnumRepr::USize
                            | facet_core::EnumRepr::ISize => {
                                let disc_i64 = builder.ins().iconst(types::I64, disc_val);
                                builder
                                    .ins()
                                    .store(MemFlags::trusted(), disc_i64, field_ptr, 0);
                            }
                            facet_core::EnumRepr::RustNPO => {
                                jit_diag!(
                                    "Field '{}' enum uses RustNPO repr (not yet supported)",
                                    field_info.name
                                );
                                return None;
                            }
                        }

                        // Parse variant data (StructType with fields)
                        // If Unit variant, nothing to parse
                        // If Newtype/Tuple/Struct variant, recursively compile the struct
                        use facet_core::StructKind;
                        match variant.data.kind {
                            StructKind::Unit => {
                                // No data to parse
                                builder.ins().jump(after_variant_block, &[]);
                            }
                            StructKind::TupleStruct | StructKind::Struct | StructKind::Tuple => {
                                // Create a temporary shape for the variant data
                                // We need to parse fields and store them at the right offsets

                                // For each field in variant.data.fields, parse and store
                                for field in variant.data.fields {
                                    let field_shape = field.shape.get();
                                    let field_kind = classify_positional_field(field_shape)?;

                                    // Calculate absolute pointer to this field
                                    let field_offset =
                                        builder.ins().iconst(pointer_type, field.offset as i64);
                                    let variant_field_ptr =
                                        builder.ins().iadd(field_ptr, field_offset);

                                    // Parse based on field kind - this is similar to the main loop
                                    // For simplicity, handle a few common types inline
                                    match field_kind {
                                        PositionalFieldKind::U8 => {
                                            let (val, err) = format.emit_parse_u8(
                                                module,
                                                &mut builder,
                                                &mut cursor,
                                            );
                                            builder.def_var(err_var, err);
                                            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                                            let store_block = builder.create_block();
                                            builder.ins().brif(ok, store_block, &[], error, &[]);
                                            builder.seal_block(variant_blocks[i]);

                                            builder.switch_to_block(store_block);
                                            builder.ins().store(
                                                MemFlags::trusted(),
                                                val,
                                                variant_field_ptr,
                                                0,
                                            );
                                            variant_blocks[i] = store_block;
                                        }
                                        PositionalFieldKind::I64(scalar_type) => {
                                            use facet_core::ScalarType;
                                            let (val_i64, err) = format.emit_parse_i64(
                                                module,
                                                &mut builder,
                                                &mut cursor,
                                            );
                                            builder.def_var(err_var, err);
                                            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                                            let store_block = builder.create_block();
                                            builder.ins().brif(ok, store_block, &[], error, &[]);
                                            builder.seal_block(variant_blocks[i]);

                                            builder.switch_to_block(store_block);
                                            let value = match scalar_type {
                                                ScalarType::I8 => {
                                                    builder.ins().ireduce(types::I8, val_i64)
                                                }
                                                ScalarType::I16 => {
                                                    builder.ins().ireduce(types::I16, val_i64)
                                                }
                                                ScalarType::I32 => {
                                                    builder.ins().ireduce(types::I32, val_i64)
                                                }
                                                _ => val_i64,
                                            };
                                            builder.ins().store(
                                                MemFlags::trusted(),
                                                value,
                                                variant_field_ptr,
                                                0,
                                            );
                                            variant_blocks[i] = store_block;
                                        }
                                        PositionalFieldKind::U64(scalar_type) => {
                                            use facet_core::ScalarType;
                                            let (val_u64, err) = format.emit_parse_u64(
                                                module,
                                                &mut builder,
                                                &mut cursor,
                                            );
                                            builder.def_var(err_var, err);
                                            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                                            let store_block = builder.create_block();
                                            builder.ins().brif(ok, store_block, &[], error, &[]);
                                            builder.seal_block(variant_blocks[i]);

                                            builder.switch_to_block(store_block);
                                            let value = match scalar_type {
                                                ScalarType::U16 => {
                                                    builder.ins().ireduce(types::I16, val_u64)
                                                }
                                                ScalarType::U32 => {
                                                    builder.ins().ireduce(types::I32, val_u64)
                                                }
                                                _ => val_u64,
                                            };
                                            builder.ins().store(
                                                MemFlags::trusted(),
                                                value,
                                                variant_field_ptr,
                                                0,
                                            );
                                            variant_blocks[i] = store_block;
                                        }
                                        PositionalFieldKind::String => {
                                            let (string_value, err) = format.emit_parse_string(
                                                module,
                                                &mut builder,
                                                &mut cursor,
                                            );
                                            builder.def_var(err_var, err);
                                            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                                            let store_block = builder.create_block();
                                            builder.ins().brif(ok, store_block, &[], error, &[]);
                                            builder.seal_block(variant_blocks[i]);

                                            builder.switch_to_block(store_block);
                                            let zero_offset = builder.ins().iconst(pointer_type, 0);
                                            builder.ins().call_indirect(
                                                write_string_sig_ref,
                                                write_string_ptr,
                                                &[
                                                    variant_field_ptr,
                                                    zero_offset,
                                                    string_value.ptr,
                                                    string_value.len,
                                                    string_value.cap,
                                                    string_value.owned,
                                                ],
                                            );
                                            variant_blocks[i] = store_block;
                                        }
                                        _ => {
                                            jit_diag!(
                                                "Field '{}' variant '{}' field '{}' type not yet supported in enum",
                                                field_info.name,
                                                variant.name,
                                                field.name
                                            );
                                            return None;
                                        }
                                    }
                                }

                                builder.ins().jump(after_variant_block, &[]);
                            }
                        }
                        builder.seal_block(variant_blocks[i]);
                    }

                    // Invalid discriminant block - jump to error
                    builder.switch_to_block(invalid_discriminant_block);
                    builder.seal_block(invalid_discriminant_block);
                    let invalid_err = builder.ins().iconst(types::I32, T2_ERR_UNSUPPORTED as i64);
                    builder.def_var(err_var, invalid_err);
                    builder.ins().jump(error, &[]);

                    // After variant block - continue to next field
                    builder.switch_to_block(after_variant_block);
                    builder.seal_block(after_variant_block);
                    builder.ins().jump(next_block, &[]);
                    current_block = next_block;
                }
            }
        }

        // success block
        builder.switch_to_block(success);
        let final_pos = builder.use_var(pos_var);
        builder.ins().return_(&[final_pos]);
        builder.seal_block(success);

        // error block
        builder.switch_to_block(error);
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
        let neg_one = builder.ins().iconst(pointer_type, -1i64);
        builder.ins().return_(&[neg_one]);
        builder.seal_block(error);

        builder.finalize();
    }

    // Debug: print the generated IR
    if std::env::var("FACET_JIT_TRACE").is_ok() {
        eprintln!("[compile_positional_struct] Generated Cranelift IR:");
        eprintln!("{}", ctx.func.display());
    }

    if let Err(e) = module.define_function(func_id, &mut ctx) {
        jit_diag!("define_function failed: {:?}", e);
        return None;
    }

    jit_diag!("compile_struct_positional_deserializer SUCCESS");
    Some(func_id)
}
