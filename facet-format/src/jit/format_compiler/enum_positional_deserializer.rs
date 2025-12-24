/// Compile a top-level enum deserializer for positional formats (e.g., postcard).
///
/// Positional enums:
/// - Discriminant encoded as varint (u64)
/// - Followed immediately by variant data fields
/// - No map/object wrapper (unlike JSON format)
///
/// This function generates code to:
/// 1. Parse discriminant from input stream
/// 2. Dispatch to correct variant handler
/// 3. Store discriminant to output memory
/// 4. Parse variant payload fields
///
/// Returns `Some(FuncId)` if compilation succeeds, `None` if the enum is incompatible.
pub(crate) fn compile_enum_positional_deserializer<F: JitFormat>(
    module: &mut JITModule,
    shape: &'static Shape,
    memo: &mut ShapeMemo,
) -> Option<FuncId> {
    jit_diag!("compile_enum_positional_deserializer ENTRY");

    // Check memo first - return cached FuncId if already compiled
    let shape_ptr = shape as *const Shape;
    if let Some(&func_id) = memo.get(&shape_ptr) {
        jit_diag!(
            "compile_enum_positional_deserializer: using memoized FuncId for shape {:p}",
            shape
        );
        return Some(func_id);
    }

    // Extract enum definition from shape
    let Type::User(UserType::Enum(enum_def)) = &shape.ty else {
        jit_diag!("Shape is not an enum");
        return None;
    };

    jit_diag!(
        "Compiling positional enum with {} variants",
        enum_def.variants.len()
    );

    let pointer_type = module.target_config().pointer_type();

    // Function signature: fn(input_ptr, len, pos, out, scratch) -> isize
    // Same as struct deserializer - returns new position or -1 on error
    let mut sig = make_c_sig(module);
    sig.params.push(AbiParam::new(pointer_type)); // input_ptr
    sig.params.push(AbiParam::new(pointer_type)); // len
    sig.params.push(AbiParam::new(pointer_type)); // pos
    sig.params.push(AbiParam::new(pointer_type)); // out (where to write enum)
    sig.params.push(AbiParam::new(pointer_type)); // scratch (for error messages)
    sig.returns.push(AbiParam::new(pointer_type)); // new_pos or -1 on error

    // Create unique function name based on shape address
    let func_name = format!(
        "jit_deserialize_positional_enum_{:x}",
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

        // Create entry block
        let entry_block = builder.create_block();
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        // Extract function parameters
        let params = builder.block_params(entry_block);
        let input_ptr = params[0];
        let len = params[1];
        let initial_pos = params[2];
        let out_ptr = params[3];
        let _scratch_ptr = params[4]; // Reserved for error messages

        // Create variables for position tracking and error handling
        let pos_var = builder.declare_var(pointer_type);
        let err_var = builder.declare_var(types::I32);
        builder.def_var(pos_var, initial_pos);
        let zero_err = builder.ins().iconst(types::I32, 0);
        builder.def_var(err_var, zero_err);

        // Create error block (returns -1 on error)
        let error = builder.create_block();

        // Create format instance for parsing
        let format = F::default();

        // Step 1: Parse discriminant as varint (u64)
        let mut cursor = JitCursor {
            input_ptr,
            len,
            pos: pos_var,
            ptr_type: pointer_type,
        };

        let (discriminant, err) = format.emit_parse_u64(module, &mut builder, &mut cursor);
        builder.def_var(err_var, err);
        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
        let disc_ok_block = builder.create_block();
        builder.ins().brif(is_ok, disc_ok_block, &[], error, &[]);

        builder.switch_to_block(disc_ok_block);

        // Step 2: Create blocks for variant dispatch
        let mut variant_blocks: Vec<_> = (0..enum_def.variants.len())
            .map(|_| builder.create_block())
            .collect();
        let invalid_discriminant_block = builder.create_block();
        let after_variant_block = builder.create_block();

        // Step 3: Dispatch on discriminant using if-then-else chain
        let mut current_check_block = disc_ok_block;
        for (i, variant) in enum_def.variants.iter().enumerate() {
            let disc_val = match variant.discriminant {
                Some(v) => v as u64,
                None => {
                    jit_diag!("Variant '{}' has no discriminant value", variant.name);
                    return None;
                }
            };

            let matches = builder
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

        // Step 4: Generate code for each variant
        for (i, variant) in enum_def.variants.iter().enumerate() {
            builder.switch_to_block(variant_blocks[i]);

            // Store discriminant to output memory (at base of enum)
            let disc_val = variant.discriminant.unwrap();
            match enum_def.enum_repr {
                facet_core::EnumRepr::U8 | facet_core::EnumRepr::I8 => {
                    let disc_i8 = builder.ins().iconst(types::I8, disc_val);
                    builder
                        .ins()
                        .store(MemFlags::trusted(), disc_i8, out_ptr, 0);
                }
                facet_core::EnumRepr::U16 | facet_core::EnumRepr::I16 => {
                    let disc_i16 = builder.ins().iconst(types::I16, disc_val);
                    builder
                        .ins()
                        .store(MemFlags::trusted(), disc_i16, out_ptr, 0);
                }
                facet_core::EnumRepr::U32 | facet_core::EnumRepr::I32 => {
                    let disc_i32 = builder.ins().iconst(types::I32, disc_val);
                    builder
                        .ins()
                        .store(MemFlags::trusted(), disc_i32, out_ptr, 0);
                }
                facet_core::EnumRepr::U64
                | facet_core::EnumRepr::I64
                | facet_core::EnumRepr::USize
                | facet_core::EnumRepr::ISize => {
                    let disc_i64 = builder.ins().iconst(types::I64, disc_val);
                    builder
                        .ins()
                        .store(MemFlags::trusted(), disc_i64, out_ptr, 0);
                }
                facet_core::EnumRepr::RustNPO => {
                    jit_diag!(
                        "Variant '{}' uses RustNPO repr (not yet supported)",
                        variant.name
                    );
                    return None;
                }
            }

            // Parse variant data based on variant kind
            use facet_core::StructKind;
            match variant.data.kind {
                StructKind::Unit => {
                    // No data to parse for unit variants
                    builder.ins().jump(after_variant_block, &[]);
                    builder.seal_block(variant_blocks[i]);
                }
                StructKind::TupleStruct | StructKind::Struct | StructKind::Tuple => {
                    // Parse each field in the variant's data
                    let mut sealed_initial = false;
                    for field in variant.data.fields {
                        let field_shape = field.shape.get();
                        let field_kind = classify_positional_field(field_shape)?;

                        // Calculate absolute pointer to this field
                        let field_offset = builder.ins().iconst(pointer_type, field.offset as i64);
                        let variant_field_ptr = builder.ins().iadd(out_ptr, field_offset);

                        // Parse based on field kind
                        match field_kind {
                            PositionalFieldKind::Bool => {
                                let (val, err) =
                                    format.emit_parse_bool(module, &mut builder, &mut cursor);
                                builder.def_var(err_var, err);
                                let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                                let store_block = builder.create_block();
                                builder.ins().brif(ok, store_block, &[], error, &[]);
                                if !sealed_initial {
                                    builder.seal_block(variant_blocks[i]);
                                    sealed_initial = true;
                                }

                                builder.switch_to_block(store_block);
                                builder.seal_block(store_block);
                                builder
                                    .ins()
                                    .store(MemFlags::trusted(), val, variant_field_ptr, 0);
                                variant_blocks[i] = store_block;
                            }
                            PositionalFieldKind::U8 => {
                                let (val, err) =
                                    format.emit_parse_u8(module, &mut builder, &mut cursor);
                                builder.def_var(err_var, err);
                                let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                                let store_block = builder.create_block();
                                builder.ins().brif(ok, store_block, &[], error, &[]);
                                if !sealed_initial {
                                    builder.seal_block(variant_blocks[i]);
                                    sealed_initial = true;
                                }

                                builder.switch_to_block(store_block);
                                builder.seal_block(store_block);
                                builder
                                    .ins()
                                    .store(MemFlags::trusted(), val, variant_field_ptr, 0);
                                variant_blocks[i] = store_block;
                            }
                            PositionalFieldKind::I8 => {
                                let (val_i64, err) =
                                    format.emit_parse_i64(module, &mut builder, &mut cursor);
                                builder.def_var(err_var, err);
                                let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                                let store_block = builder.create_block();
                                builder.ins().brif(ok, store_block, &[], error, &[]);
                                if !sealed_initial {
                                    builder.seal_block(variant_blocks[i]);
                                    sealed_initial = true;
                                }

                                builder.switch_to_block(store_block);
                                builder.seal_block(store_block);
                                let val = builder.ins().ireduce(types::I8, val_i64);
                                builder
                                    .ins()
                                    .store(MemFlags::trusted(), val, variant_field_ptr, 0);
                                variant_blocks[i] = store_block;
                            }
                            PositionalFieldKind::I64(scalar_type) => {
                                use facet_core::ScalarType;
                                let (val_i64, err) =
                                    format.emit_parse_i64(module, &mut builder, &mut cursor);
                                builder.def_var(err_var, err);
                                let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                                let store_block = builder.create_block();
                                builder.ins().brif(ok, store_block, &[], error, &[]);
                                if !sealed_initial {
                                    builder.seal_block(variant_blocks[i]);
                                    sealed_initial = true;
                                }

                                builder.switch_to_block(store_block);
                                builder.seal_block(store_block);
                                let value = match scalar_type {
                                    ScalarType::I8 => builder.ins().ireduce(types::I8, val_i64),
                                    ScalarType::I16 => builder.ins().ireduce(types::I16, val_i64),
                                    ScalarType::I32 => builder.ins().ireduce(types::I32, val_i64),
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
                                let (val_u64, err) =
                                    format.emit_parse_u64(module, &mut builder, &mut cursor);
                                builder.def_var(err_var, err);
                                let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                                let store_block = builder.create_block();
                                builder.ins().brif(ok, store_block, &[], error, &[]);
                                if !sealed_initial {
                                    builder.seal_block(variant_blocks[i]);
                                    sealed_initial = true;
                                }

                                builder.switch_to_block(store_block);
                                builder.seal_block(store_block);
                                let value = match scalar_type {
                                    ScalarType::U8 => builder.ins().ireduce(types::I8, val_u64),
                                    ScalarType::U16 => builder.ins().ireduce(types::I16, val_u64),
                                    ScalarType::U32 => builder.ins().ireduce(types::I32, val_u64),
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
                            PositionalFieldKind::F32 => {
                                let (val, err) =
                                    format.emit_parse_f32(module, &mut builder, &mut cursor);
                                builder.def_var(err_var, err);
                                let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                                let store_block = builder.create_block();
                                builder.ins().brif(ok, store_block, &[], error, &[]);
                                if !sealed_initial {
                                    builder.seal_block(variant_blocks[i]);
                                    sealed_initial = true;
                                }

                                builder.switch_to_block(store_block);
                                builder.seal_block(store_block);
                                builder
                                    .ins()
                                    .store(MemFlags::trusted(), val, variant_field_ptr, 0);
                                variant_blocks[i] = store_block;
                            }
                            PositionalFieldKind::F64 => {
                                let (val, err) =
                                    format.emit_parse_f64(module, &mut builder, &mut cursor);
                                builder.def_var(err_var, err);
                                let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                                let store_block = builder.create_block();
                                builder.ins().brif(ok, store_block, &[], error, &[]);
                                if !sealed_initial {
                                    builder.seal_block(variant_blocks[i]);
                                    sealed_initial = true;
                                }

                                builder.switch_to_block(store_block);
                                builder.seal_block(store_block);
                                builder
                                    .ins()
                                    .store(MemFlags::trusted(), val, variant_field_ptr, 0);
                                variant_blocks[i] = store_block;
                            }
                            PositionalFieldKind::String => {
                                // Import write_string helper signature
                                let write_string_sig = {
                                    let mut s = make_c_sig(module);
                                    s.params.push(AbiParam::new(pointer_type)); // dest
                                    s.params.push(AbiParam::new(pointer_type)); // offset
                                    s.params.push(AbiParam::new(pointer_type)); // ptr
                                    s.params.push(AbiParam::new(pointer_type)); // len
                                    s.params.push(AbiParam::new(pointer_type)); // cap
                                    s.params.push(AbiParam::new(types::I8)); // owned
                                    s
                                };
                                let write_string_sig_ref =
                                    builder.import_signature(write_string_sig);
                                let write_string_ptr = builder.ins().iconst(
                                    pointer_type,
                                    helpers::jit_write_string as *const u8 as i64,
                                );

                                let (string_value, err) =
                                    format.emit_parse_string(module, &mut builder, &mut cursor);
                                builder.def_var(err_var, err);
                                let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                                let store_block = builder.create_block();
                                builder.ins().brif(ok, store_block, &[], error, &[]);
                                if !sealed_initial {
                                    builder.seal_block(variant_blocks[i]);
                                    sealed_initial = true;
                                }

                                builder.switch_to_block(store_block);
                                builder.seal_block(store_block);
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
                            PositionalFieldKind::Option(_)
                            | PositionalFieldKind::Struct(_)
                            | PositionalFieldKind::List(_)
                            | PositionalFieldKind::Map(_)
                            | PositionalFieldKind::Enum(_) => {
                                jit_diag!(
                                    "Variant '{}' field '{}' has complex type (not yet supported for top-level enum variants)",
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
        }

        // Invalid discriminant error block
        builder.switch_to_block(invalid_discriminant_block);
        builder.seal_block(invalid_discriminant_block);
        let invalid_err = builder.ins().iconst(types::I32, T2_ERR_UNSUPPORTED as i64);
        builder.def_var(err_var, invalid_err);
        builder.ins().jump(error, &[]);

        // After variant block - success path
        builder.switch_to_block(after_variant_block);
        builder.seal_block(after_variant_block);
        let final_pos = builder.use_var(pos_var);
        builder.ins().return_(&[final_pos]);

        // Error block - return -1
        builder.switch_to_block(error);
        builder.seal_block(error);
        let minus_one = builder.ins().iconst(pointer_type, -1);
        builder.ins().return_(&[minus_one]);

        builder.finalize();
    }

    // Define the function in the module
    if let Err(e) = module.define_function(func_id, &mut ctx) {
        jit_diag!("define_function failed: {:?}", e);
        return None;
    }

    jit_diag!("compile_enum_positional_deserializer SUCCESS");
    Some(func_id)
}
