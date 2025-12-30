use std::collections::HashMap;

use cranelift::codegen::ir::{AbiParam, BlockArg};
use cranelift::prelude::*;
use cranelift_jit::JITModule;
use cranelift_module::{FuncId, Linkage, Module};

use facet_core::{Def, Shape, Type, UserType};

use super::super::format::{
    JIT_SCRATCH_ERROR_CODE_OFFSET, JIT_SCRATCH_ERROR_POS_OFFSET, JitCursor, JitFormat, make_c_sig,
};
use super::super::helpers;
use super::super::jit_debug;
use super::{
    DispatchTarget, FieldCodegenInfo, FlattenedMapInfo, FlattenedVariantInfo,
    FormatListElementKind, KeyDispatchStrategy, ShapeMemo, compile_list_format_deserializer,
    compile_map_format_deserializer, compute_field_prefix, compute_key_colon_pattern_extended,
    ensure_format_jit_field_type_supported, func_addr_value, tier2_call_sig,
};

/// Compile a Tier-2 struct deserializer.
///
/// Generates IR that uses the map protocol to deserialize struct fields:
/// - map_begin() -> is_end() loop -> read_key() -> match field -> deserialize value -> kv_sep() -> next()
/// - Unknown fields are skipped via emit_skip_value()
/// - Missing optional fields (`Option<T>`) are pre-initialized to None
/// - Missing required fields cause an error
pub(crate) fn compile_struct_format_deserializer<F: JitFormat>(
    module: &mut JITModule,
    shape: &'static Shape,
    memo: &mut ShapeMemo,
) -> Option<FuncId> {
    jit_debug!("compile_struct_format_deserializer ENTRY");
    jit_debug!("[compile_struct] ═══ ENTRY ═══");
    jit_debug!("[compile_struct] Shape type: {:?}", shape.ty);

    // Check memo first - return cached FuncId if already compiled
    let shape_ptr = shape as *const Shape;
    if let Some(&func_id) = memo.get(&shape_ptr) {
        jit_debug!(
            "compile_struct_format_deserializer: using memoized FuncId for shape {:p}",
            shape
        );
        return Some(func_id);
    }

    let Type::User(UserType::Struct(struct_def)) = &shape.ty else {
        jit_debug!("[compile_struct] ✗ FAIL: Not a struct");
        jit_debug!("Shape is not a struct");
        return None;
    };

    jit_debug!(
        "[compile_struct] Compiling struct with {} fields",
        struct_def.fields.len()
    );

    // Build field metadata - separate normal fields from flattened enum variants
    //
    // Phase 1: Identify flattened enum fields and assign "seen" bit indices
    let mut enum_field_to_seen_bit: HashMap<usize, u8> = HashMap::new();
    let mut enum_seen_bit_count = 0u8;

    for (field_idx, field) in struct_def.fields.iter().enumerate() {
        if field.is_flattened() {
            let field_shape = field.shape.get();
            if let facet_core::Type::User(facet_core::UserType::Enum(_)) = &field_shape.ty {
                // Assign a unique "seen" bit for this enum field
                enum_field_to_seen_bit.insert(field_idx, enum_seen_bit_count);
                enum_seen_bit_count += 1;
            }
        }
    }

    jit_debug!(
        "Identified {} flattened enum fields requiring 'seen' tracking",
        enum_seen_bit_count
    );

    // Phase 2: Build field_infos and flatten_variants with assigned bit indices
    // Note: Flattened struct fields are added directly to field_infos with combined offsets
    let mut field_infos = Vec::new();
    let mut flatten_variants = Vec::new();
    let mut flatten_map: Option<FlattenedMapInfo> = None;
    let mut required_count = 0u8;

    for (field_idx, field) in struct_def.fields.iter().enumerate() {
        // Get serialized name (prefer rename, fall back to name)
        let name = field.rename.unwrap_or(field.name);

        // Get field shape
        let field_shape = field.shape.get();

        jit_debug!(
            "[compile_struct]   Field '{}': shape.def = {:?}",
            name,
            field_shape.def
        );

        // Check if this is a flattened field
        if field.is_flattened() {
            // Handle flattened enums
            if let facet_core::Type::User(facet_core::UserType::Enum(enum_type)) = &field_shape.ty {
                let enum_seen_bit = *enum_field_to_seen_bit.get(&field_idx).unwrap();

                jit_debug!(
                    "Processing flattened enum field '{}' with {} variants (seen bit={})",
                    name,
                    enum_type.variants.len(),
                    enum_seen_bit
                );

                // Extract all variants and add as dispatch targets
                let mut has_supported_variants = false;
                for variant in enum_type.variants {
                    let variant_name = variant.name;

                    // Get discriminant value (required for #[repr(C)] enums)
                    let discriminant = variant.discriminant.unwrap_or(0) as usize;

                    // Handle both unit variants and variants with data
                    // Unit variants (e.g., Active, Inactive) have no fields
                    // Data variants have at least one field containing the payload
                    if variant.data.fields.is_empty() {
                        // Unit variant - no payload, just the discriminant
                        jit_debug!(
                            "  Skipping unit variant '{}' (discriminant {}): flattened unit variants not yet supported",
                            variant_name,
                            discriminant
                        );
                        // TODO: Support unit variants by checking for the key in JSON and writing discriminant
                        continue;
                    }

                    // Get payload shape and offset (first field of tuple/struct variant)
                    // The offset already accounts for discriminant size/alignment per Variant docs
                    let payload_shape = variant.data.fields[0].shape();
                    let payload_offset_in_enum = variant.data.fields[0].offset;

                    jit_debug!(
                        "  Adding variant '{}' with discriminant {}, payload offset {}",
                        variant_name,
                        discriminant,
                        payload_offset_in_enum
                    );

                    flatten_variants.push(FlattenedVariantInfo {
                        variant_name,
                        enum_field_offset: field.offset,
                        discriminant,
                        payload_shape,
                        payload_offset_in_enum,
                        enum_seen_bit_index: enum_seen_bit,
                    });
                    has_supported_variants = true;
                }

                // If no variants are supported, fall back to Tier 1
                if !has_supported_variants {
                    jit_debug!(
                        "Flattened enum field '{}' has no supported variants (all are unit variants)",
                        name
                    );
                    return None;
                }

                // Don't add flattened enum to field_infos - it's handled via variants
                continue;
            }
            // Handle flattened structs
            else if let facet_core::Type::User(facet_core::UserType::Struct(inner_struct_def)) =
                &field_shape.ty
            {
                jit_debug!(
                    "Processing flattened struct field '{}' with {} inner fields",
                    name,
                    inner_struct_def.fields.len()
                );

                // Add inner fields directly to field_infos with combined offsets
                // This allows us to reuse all the existing field parsing logic
                for inner_field in inner_struct_def.fields {
                    let inner_field_name = inner_field.rename.unwrap_or(inner_field.name);
                    let inner_field_shape = inner_field.shape.get();

                    // Check if inner field type is supported
                    if ensure_format_jit_field_type_supported(
                        inner_field_shape,
                        "(flattened)",
                        inner_field_name,
                    )
                    .is_err()
                    {
                        jit_debug!(
                            "  Flattened struct '{}' contains unsupported field '{}': {:?}",
                            name,
                            inner_field_name,
                            inner_field_shape.def
                        );
                        return None;
                    }

                    // Check if this inner field is Option<T>
                    let is_inner_option = matches!(inner_field_shape.def, Def::Option(_));

                    // Assign required bit index if not Option and no default
                    let inner_required_bit_index = if !is_inner_option && !inner_field.has_default()
                    {
                        let bit = required_count;
                        required_count += 1;
                        Some(bit)
                    } else {
                        None
                    };

                    // Compute combined offset: parent struct offset + inner field offset
                    let combined_offset = field.offset + inner_field.offset;

                    jit_debug!(
                        "  Adding flattened field '{}' at combined offset {} (parent {} + inner {})",
                        inner_field_name,
                        combined_offset,
                        field.offset,
                        inner_field.offset
                    );

                    // Add to field_infos as a normal field with adjusted offset
                    field_infos.push(FieldCodegenInfo {
                        name: inner_field_name,
                        offset: combined_offset,
                        shape: inner_field_shape,
                        is_option: is_inner_option,
                        required_bit_index: inner_required_bit_index,
                    });
                }

                // Don't add the flattened struct itself to field_infos - it's replaced by its fields
                continue;
            }
            // Handle flattened maps (for unknown key capture)
            else if let Def::Map(map_def) = &field_shape.def {
                jit_debug!(
                    "Processing flattened map field '{}' for unknown key capture",
                    name
                );

                // Validate: only one flattened map allowed
                if flatten_map.is_some() {
                    jit_debug!(
                        "Multiple flattened maps are not allowed - field '{}' conflicts with previous flattened map",
                        name
                    );
                    return None;
                }

                // Validate: key must be String
                if map_def.k.scalar_type() != Some(facet_core::ScalarType::String) {
                    jit_debug!(
                        "Flattened map field '{}' must have String keys, found {:?}",
                        name,
                        map_def.k.scalar_type()
                    );
                    return None;
                }

                // Validate: value type must be Tier-2 compatible
                let value_shape = map_def.v;
                let value_kind = match FormatListElementKind::from_shape(value_shape) {
                    Some(kind) => kind,
                    None => {
                        jit_debug!(
                            "Flattened map field '{}' has unsupported value type: {:?}",
                            name,
                            value_shape.def
                        );
                        return None;
                    }
                };

                jit_debug!(
                    "  Flattened map '{}' will capture unknown keys with value type {:?}",
                    name,
                    value_shape.def
                );

                flatten_map = Some(FlattenedMapInfo {
                    map_field_offset: field.offset,
                    value_shape,
                    value_kind,
                });

                // Don't add the flattened map to field_infos - it's handled via unknown_key logic
                continue;
            } else {
                // Unsupported flattened type
                jit_debug!(
                    "Flattened field '{}' has unsupported type: {:?}",
                    name,
                    field_shape.ty
                );
                return None;
            }
        }

        jit_debug!(
            "[compile_struct]   Field '{}': scalar_type = {:?}",
            name,
            field_shape.scalar_type()
        );

        // Check if this is Option<T>
        let is_option = matches!(field_shape.def, Def::Option(_));

        // Assign required bit index if not Option and no default
        let required_bit_index = if !is_option && !field.has_default() {
            let bit = required_count;
            required_count += 1;
            Some(bit)
        } else {
            None
        };

        field_infos.push(FieldCodegenInfo {
            name,
            offset: field.offset,
            shape: field_shape,
            is_option,
            required_bit_index,
        });
    }

    jit_debug!("[compile_struct] Required fields: {}", required_count);
    jit_debug!(
        "Built field metadata: {} fields (including flattened), {} flattened enum variants, {} flattened map",
        field_infos.len(),
        flatten_variants.len(),
        if flatten_map.is_some() { 1 } else { 0 }
    );

    // Check field count limit: we use u64 bitsets for tracking required fields and enum seen bits
    // Valid bit indices are 0-63, so we can track at most 64 bits total
    // (required_count uses bits 0..required_count-1, enum_seen_bit_count uses the remaining bits)
    let total_tracking_bits = required_count as usize + enum_seen_bit_count as usize;
    if total_tracking_bits >= 64 {
        jit_debug!(
            "Struct has too many tracking bits ({} required fields + {} flattened enums = {} total bits) - maximum is 63",
            required_count,
            enum_seen_bit_count,
            total_tracking_bits
        );
        return None;
    }

    // Phase 3: Detect dispatch key collisions (normal fields vs flattened enum variants)
    let mut seen_keys: HashMap<&'static str, &str> = HashMap::new();

    // Check normal field names
    for field_info in &field_infos {
        if let Some(conflicting_source) = seen_keys.insert(field_info.name, "field") {
            jit_debug!(
                "Dispatch collision: field '{}' conflicts with {} key",
                field_info.name,
                conflicting_source
            );
            return None;
        }
    }

    // Check variant names against field names
    for variant_info in &flatten_variants {
        if let Some(conflicting_source) = seen_keys.insert(variant_info.variant_name, "variant") {
            jit_debug!(
                "Dispatch collision: variant '{}' conflicts with {} key",
                variant_info.variant_name,
                conflicting_source
            );
            return None;
        }
    }

    jit_debug!(
        "Dispatch collision check passed: {} unique keys",
        seen_keys.len()
    );

    // Build unified dispatch table: normal fields + flattened enum variants
    let mut dispatch_entries: Vec<(&'static str, DispatchTarget)> = Vec::new();

    for (idx, field_info) in field_infos.iter().enumerate() {
        dispatch_entries.push((field_info.name, DispatchTarget::Field(idx)));
    }

    for (idx, variant_info) in flatten_variants.iter().enumerate() {
        dispatch_entries.push((
            variant_info.variant_name,
            DispatchTarget::FlattenEnumVariant(idx),
        ));
    }

    jit_debug!(
        "Built dispatch table with {} total entries",
        dispatch_entries.len()
    );

    // Analyze and determine key dispatch strategy (using combined dispatch table)
    // Check if all keys are short enough for inline matching (≤13 chars for "key": pattern with two u64 loads)
    let max_key_len = dispatch_entries
        .iter()
        .map(|(name, _)| name.len())
        .max()
        .unwrap_or(0);

    let dispatch_strategy = if dispatch_entries.len() < 10 && max_key_len <= 13 {
        // All keys short enough for inline "key": matching (up to 13 chars = 16 bytes with two u64s)
        jit_debug!(
            "Using Inline dispatch (max_key_len={}, {} entries)",
            max_key_len,
            dispatch_entries.len()
        );
        KeyDispatchStrategy::Inline
    } else if dispatch_entries.len() < 10 {
        KeyDispatchStrategy::Linear
    } else {
        // Prefix dispatch requires that all dispatch keys are at least prefix_len bytes.
        // Otherwise, short keys (e.g. "id") would never match and we'd treat them as unknown.
        let min_key_len = dispatch_entries
            .iter()
            .map(|(name, _)| name.len())
            .min()
            .unwrap_or(0);

        // Try to choose an optimal prefix length that balances uniqueness and code size
        let chosen_prefix_len = if min_key_len >= 8 {
            // Try 8-byte prefix for maximum dispersion
            let unique_prefixes_8 = dispatch_entries
                .iter()
                .map(|(name, _)| compute_field_prefix(name, 8).0)
                .collect::<std::collections::HashSet<_>>()
                .len();

            // If 8-byte prefix gives good dispersion (>75% unique), use it
            if unique_prefixes_8 * 4 > dispatch_entries.len() * 3 {
                8
            } else {
                4
            }
        } else if min_key_len >= 4 {
            4
        } else {
            0 // Will fall back to Linear
        };

        if chosen_prefix_len == 0 {
            KeyDispatchStrategy::Linear
        } else {
            // Check collision rate - if too many collisions, linear search might be better
            let unique_prefixes = dispatch_entries
                .iter()
                .map(|(name, _)| compute_field_prefix(name, chosen_prefix_len).0)
                .collect::<std::collections::HashSet<_>>()
                .len();

            // If we have very few unique prefixes (high collision rate), use linear
            // This avoids pathological cases like all fields starting with "field_"
            if unique_prefixes * 2 < dispatch_entries.len() {
                jit_debug!(
                    "Prefix dispatch has poor dispersion ({} unique prefixes for {} fields), using linear",
                    unique_prefixes,
                    dispatch_entries.len()
                );
                KeyDispatchStrategy::Linear
            } else {
                KeyDispatchStrategy::PrefixSwitch {
                    prefix_len: chosen_prefix_len,
                }
            }
        }
    };

    let pointer_type = module.target_config().pointer_type();

    // Function signature: fn(input_ptr, len, pos, out, scratch) -> isize
    // IMPORTANT: Use C ABI calling convention to match extern "C" callers
    let mut sig = make_c_sig(module);
    sig.params.push(AbiParam::new(pointer_type)); // input_ptr
    sig.params.push(AbiParam::new(pointer_type)); // len
    sig.params.push(AbiParam::new(pointer_type)); // pos
    sig.params.push(AbiParam::new(pointer_type)); // out
    sig.params.push(AbiParam::new(pointer_type)); // scratch
    sig.returns.push(AbiParam::new(pointer_type)); // new_pos or error

    // Create unique function name using shape pointer address
    let func_name = format!("jit_deserialize_struct_{:x}", shape as *const _ as usize);

    let func_id = match module.declare_function(&func_name, Linkage::Export, &sig) {
        Ok(id) => id,
        Err(e) => {
            jit_debug!("[compile_struct] ✗ FAIL: declare_function failed: {:?}", e);
            jit_debug!("declare_function('{}') failed: {:?}", func_name, e);
            return None;
        }
    };
    jit_debug!(
        "[compile_struct] ✓ Function '{}' declared successfully",
        func_name
    );

    // Insert into memo immediately after declaration (before IR build) to avoid recursion/cycles
    memo.insert(shape_ptr, func_id);
    jit_debug!(
        "compile_struct_format_deserializer: memoized FuncId for shape {:p}",
        shape
    );
    jit_debug!("Function declared, starting IR generation");

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

        // Variable for required fields bitset (u64)
        let required_bits_var = builder.declare_var(types::I64);
        let zero_i64 = builder.ins().iconst(types::I64, 0);
        builder.def_var(required_bits_var, zero_i64);

        // Variable for enum "seen" tracking bitset (one bit per flattened enum field)
        let enum_seen_bits_var = builder.declare_var(types::I64);
        builder.def_var(enum_seen_bits_var, zero_i64);

        // Variable for tracking whether flattened map has been initialized (only if flatten_map exists)
        let map_initialized_var = if flatten_map.is_some() {
            let var = builder.declare_var(types::I8);
            let zero_i8 = builder.ins().iconst(types::I8, 0);
            builder.def_var(var, zero_i8);
            Some(var)
        } else {
            None
        };

        // Create basic blocks
        let map_begin = builder.create_block();
        let check_map_begin_err = builder.create_block();
        let init_options = builder.create_block();
        let loop_check_end = builder.create_block();
        let check_is_end_err = builder.create_block();
        let check_is_end_value = builder.create_block();
        let read_key = builder.create_block();
        let check_read_key_err = builder.create_block();
        let key_dispatch = builder.create_block();
        let unknown_key = builder.create_block();
        let after_value = builder.create_block();
        let check_map_next_err = builder.create_block();
        let validate_required = builder.create_block();
        let success = builder.create_block();
        let error = builder.create_block();

        // Allocate stack slot for map state if needed
        let state_ptr = if F::MAP_STATE_SIZE > 0 {
            let align_shift = F::MAP_STATE_ALIGN.trailing_zeros() as u8;
            let slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                F::MAP_STATE_SIZE,
                align_shift,
            ));
            builder.ins().stack_addr(pointer_type, slot, 0)
        } else {
            builder.ins().iconst(pointer_type, 0)
        };

        // Jump to map_begin
        builder.ins().jump(map_begin, &[]);
        builder.seal_block(entry);

        // map_begin: consume map start delimiter
        builder.switch_to_block(map_begin);
        let mut cursor = JitCursor {
            input_ptr,
            len,
            pos: pos_var,
            ptr_type: pointer_type,
            scratch_ptr,
        };
        let format = F::default();
        let err_code = format.emit_map_begin(module, &mut builder, &mut cursor, state_ptr);
        builder.def_var(err_var, err_code);
        builder.ins().jump(check_map_begin_err, &[]);
        builder.seal_block(map_begin);

        // check_map_begin_err
        builder.switch_to_block(check_map_begin_err);
        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
        builder.ins().brif(is_ok, init_options, &[], error, &[]);
        builder.seal_block(check_map_begin_err);

        // init_options: pre-initialize Option fields to None
        builder.switch_to_block(init_options);

        // Declare jit_option_init_none helper signature
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

        // Pre-initialize all Option<T> fields to None (normal fields)
        for field_info in &field_infos {
            if field_info.is_option {
                // Get the OptionDef from the field shape
                if let Def::Option(opt_def) = &field_info.shape.def {
                    let field_ptr = builder.ins().iadd_imm(out_ptr, field_info.offset as i64);
                    let init_none_fn_ptr = builder
                        .ins()
                        .iconst(pointer_type, opt_def.vtable.init_none as *const () as i64);
                    builder.ins().call_indirect(
                        option_init_none_sig_ref,
                        option_init_none_ptr,
                        &[field_ptr, init_none_fn_ptr],
                    );
                }
            }
        }

        builder.ins().jump(loop_check_end, &[]);
        builder.seal_block(init_options);

        // loop_check_end: check if we're at map end
        builder.switch_to_block(loop_check_end);

        let mut cursor = JitCursor {
            input_ptr,
            len,
            pos: pos_var,
            ptr_type: pointer_type,
            scratch_ptr,
        };

        // Call emit_map_is_end to check if we're done
        let format = F::default();
        let (is_end_i8, err_code) =
            format.emit_map_is_end(module, &mut builder, &mut cursor, state_ptr);
        builder.def_var(err_var, err_code);

        builder.ins().jump(check_is_end_err, &[]);
        // Note: loop_check_end will be sealed after check_map_next_err

        // check_is_end_err
        builder.switch_to_block(check_is_end_err);
        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
        builder
            .ins()
            .brif(is_ok, check_is_end_value, &[], error, &[]);
        builder.seal_block(check_is_end_err);

        // check_is_end_value: branch based on is_end
        builder.switch_to_block(check_is_end_value);
        let is_end = builder.ins().uextend(pointer_type, is_end_i8);
        let is_end_bool = builder.ins().icmp_imm(IntCC::NotEqual, is_end, 0);
        builder
            .ins()
            .brif(is_end_bool, validate_required, &[], read_key, &[]);
        builder.seal_block(check_is_end_value);

        // validate_required: check all required fields were set
        builder.switch_to_block(validate_required);

        if required_count > 0 {
            // Compute required_mask: all bits for required fields
            let required_mask = (1u64 << required_count) - 1;
            let mask_val = builder.ins().iconst(types::I64, required_mask as i64);

            let bits = builder.use_var(required_bits_var);
            let bits_masked = builder.ins().band(bits, mask_val);

            // Check if (bits_masked == mask_val)
            let all_set = builder.ins().icmp(IntCC::Equal, bits_masked, mask_val);

            // If not all set, set error and jump to error block
            let required_ok = builder.create_block();
            let required_fail = builder.create_block();
            builder
                .ins()
                .brif(all_set, required_ok, &[], required_fail, &[]);

            // required_fail: set ERR_MISSING_REQUIRED_FIELD and error
            builder.switch_to_block(required_fail);
            let err = builder
                .ins()
                .iconst(types::I32, helpers::ERR_MISSING_REQUIRED_FIELD as i64);
            builder.def_var(err_var, err);
            builder.ins().jump(error, &[]);
            builder.seal_block(required_fail);

            // required_ok: continue to success
            builder.switch_to_block(required_ok);
            builder.ins().jump(success, &[]);
            builder.seal_block(required_ok);
        } else {
            // No required fields, go straight to success
            builder.ins().jump(success, &[]);
        }

        builder.seal_block(validate_required);

        // success: return new position
        builder.switch_to_block(success);

        // Initialize flattened map to empty if it exists but was never initialized (no unknown keys)
        if let Some(flatten_map_info) = &flatten_map {
            let map_initialized_var = map_initialized_var.unwrap();
            let map_initialized = builder.use_var(map_initialized_var);
            let already_initialized = builder.ins().icmp_imm(IntCC::NotEqual, map_initialized, 0);
            let init_empty_map = builder.create_block();
            let after_empty_init = builder.create_block();
            builder.ins().brif(
                already_initialized,
                after_empty_init,
                &[],
                init_empty_map,
                &[],
            );

            // init_empty_map: initialize to empty HashMap
            builder.switch_to_block(init_empty_map);
            jit_debug!("Initializing flattened map to empty (no unknown keys encountered)");

            let map_ptr = builder
                .ins()
                .iadd_imm(out_ptr, flatten_map_info.map_field_offset as i64);

            // Get map init function (already computed during field metadata building)
            let map_shape = {
                let mut found_shape = None;
                for field in struct_def.fields {
                    if field.is_flattened() {
                        let field_shape = field.shape.get();
                        if let Def::Map(_) = &field_shape.def
                            && field.offset == flatten_map_info.map_field_offset
                        {
                            found_shape = Some(field_shape);
                            break;
                        }
                    }
                }
                found_shape.expect("flattened map shape must exist")
            };

            let map_def = match &map_shape.def {
                Def::Map(m) => m,
                _ => unreachable!("flatten_map_info must be from a Map"),
            };

            let init_fn = map_def.vtable.init_in_place_with_capacity;

            let map_init_sig_ref = {
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

            // Call jit_map_init_with_capacity(map_ptr, 0, init_fn)
            let zero_capacity = builder.ins().iconst(pointer_type, 0);
            let init_fn_ptr = builder.ins().iconst(pointer_type, init_fn as usize as i64);
            builder.ins().call_indirect(
                map_init_sig_ref,
                map_init_ptr,
                &[map_ptr, zero_capacity, init_fn_ptr],
            );

            builder.ins().jump(after_empty_init, &[]);
            builder.seal_block(init_empty_map);

            // after_empty_init: continue to return
            builder.switch_to_block(after_empty_init);
            builder.seal_block(after_empty_init);
        }

        let final_pos = builder.use_var(pos_var);
        builder.ins().return_(&[final_pos]);
        builder.seal_block(success);

        // error: write scratch and return -1
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
        // Note: error block will be sealed later, after all branches to it

        // Declare key value variables (needed for fallback path)
        let key_ptr_var = builder.declare_var(pointer_type);
        let key_len_var = builder.declare_var(pointer_type);
        let key_cap_var = builder.declare_var(pointer_type);
        let key_owned_var = builder.declare_var(types::I8);

        // Create inline value blocks for Inline strategy (skip kv_sep, just parse value)
        let inline_value_blocks: Vec<Block> =
            if matches!(dispatch_strategy, KeyDispatchStrategy::Inline) {
                dispatch_entries
                    .iter()
                    .map(|_| builder.create_block())
                    .collect()
            } else {
                Vec::new()
            };

        // read_key: read the map key (with optional inline matching fast path)
        builder.switch_to_block(read_key);

        // For Inline strategy, try matching "key": patterns directly first
        if matches!(dispatch_strategy, KeyDispatchStrategy::Inline) {
            let pos = builder.use_var(pos_var);
            let fallback_parse = builder.create_block();

            // Create shared whitespace skip blocks (used by all inline matches)
            // This reduces code bloat by having one ws skip loop instead of N
            let shared_ws_entry = builder.create_block();
            builder.append_block_param(shared_ws_entry, pointer_type); // new_pos
            builder.append_block_param(shared_ws_entry, pointer_type); // continuation_idx

            let shared_ws_loop = builder.create_block();
            builder.append_block_param(shared_ws_loop, pointer_type); // continuation_idx

            let shared_ws_check = builder.create_block();
            builder.append_block_param(shared_ws_check, pointer_type); // continuation_idx

            let shared_ws_dispatch = builder.create_block();
            builder.append_block_param(shared_ws_dispatch, pointer_type); // continuation_idx

            // Try inline matching for each known key
            let mut current_block = read_key;
            for (i, (key_name, _)) in dispatch_entries.iter().enumerate() {
                if i > 0 {
                    builder.switch_to_block(current_block);
                }

                // Compute "key": pattern (extended version supports keys up to 13 chars)
                let pattern = compute_key_colon_pattern_extended(key_name).unwrap();

                // Create next check block (or fallback if last)
                let next_check = if i + 1 < dispatch_entries.len() {
                    builder.create_block()
                } else {
                    fallback_parse
                };

                // Check bounds: pos + total_len <= len
                let pattern_len_val = builder.ins().iconst(pointer_type, pattern.total_len as i64);
                let end_pos = builder.ins().iadd(pos, pattern_len_val);
                let in_bounds = builder
                    .ins()
                    .icmp(IntCC::UnsignedLessThanOrEqual, end_pos, len);

                let check_pattern = builder.create_block();
                builder
                    .ins()
                    .brif(in_bounds, check_pattern, &[], next_check, &[]);
                if i > 0 {
                    builder.seal_block(current_block);
                }

                // check_pattern: load and compare first 8 bytes
                builder.switch_to_block(check_pattern);
                builder.seal_block(check_pattern);

                // Load first 8 bytes from input[pos]
                let addr = builder.ins().iadd(input_ptr, pos);
                let loaded1 = builder.ins().load(types::I64, MemFlags::trusted(), addr, 0);

                // Mask to pattern1_len bytes if needed (skip no-op mask when pattern fills 8 bytes)
                let value_to_compare = if pattern.pattern1_len >= 8 {
                    // No masking needed - pattern fills all 8 bytes
                    loaded1
                } else {
                    let mask1 = (1u64 << (pattern.pattern1_len * 8)) - 1;
                    let mask1_val = builder.ins().iconst(types::I64, mask1 as i64);
                    builder.ins().band(loaded1, mask1_val)
                };

                // Compare with expected pattern1
                let expected1 = builder.ins().iconst(types::I64, pattern.pattern1 as i64);
                let matches1 = builder
                    .ins()
                    .icmp(IntCC::Equal, value_to_compare, expected1);

                let match_success = builder.create_block();

                if pattern.pattern2_len > 0 {
                    // Need to check second pattern too
                    let check_pattern2 = builder.create_block();
                    builder
                        .ins()
                        .brif(matches1, check_pattern2, &[], next_check, &[]);

                    // check_pattern2: load and compare second 8 bytes
                    builder.switch_to_block(check_pattern2);
                    builder.seal_block(check_pattern2);

                    // Load next 8 bytes from input[pos + 8]
                    let eight = builder.ins().iconst(pointer_type, 8);
                    let addr2 = builder.ins().iadd(addr, eight);
                    let loaded2 = builder
                        .ins()
                        .load(types::I64, MemFlags::trusted(), addr2, 0);

                    // Mask to pattern2_len bytes if needed (skip no-op mask when pattern fills 8 bytes)
                    let value2_to_compare = if pattern.pattern2_len >= 8 {
                        // No masking needed - pattern fills all 8 bytes
                        loaded2
                    } else {
                        let mask2 = (1u64 << (pattern.pattern2_len * 8)) - 1;
                        let mask2_val = builder.ins().iconst(types::I64, mask2 as i64);
                        builder.ins().band(loaded2, mask2_val)
                    };

                    // Compare with expected pattern2
                    let expected2 = builder.ins().iconst(types::I64, pattern.pattern2 as i64);
                    let matches2 = builder
                        .ins()
                        .icmp(IntCC::Equal, value2_to_compare, expected2);

                    builder
                        .ins()
                        .brif(matches2, match_success, &[], next_check, &[]);
                } else {
                    // Single pattern match only
                    builder
                        .ins()
                        .brif(matches1, match_success, &[], next_check, &[]);
                }

                // match_success: advance pos and jump to shared whitespace skip
                builder.switch_to_block(match_success);
                builder.seal_block(match_success);

                // Compute new_pos and jump to shared ws skip with continuation index
                let new_pos = builder.ins().iadd(pos, pattern_len_val);
                let cont_idx = builder.ins().iconst(pointer_type, i as i64);
                builder.ins().jump(
                    shared_ws_entry,
                    &[BlockArg::from(new_pos), BlockArg::from(cont_idx)],
                );

                current_block = next_check;
            }

            // === Shared whitespace skip blocks ===

            // shared_ws_entry: store new_pos to pos_var and start skip loop
            builder.switch_to_block(shared_ws_entry);
            let entry_new_pos = builder.block_params(shared_ws_entry)[0];
            let entry_cont_idx = builder.block_params(shared_ws_entry)[1];
            builder.def_var(pos_var, entry_new_pos);
            builder
                .ins()
                .jump(shared_ws_loop, &[BlockArg::from(entry_cont_idx)]);

            // shared_ws_loop: check bounds
            builder.switch_to_block(shared_ws_loop);
            let loop_cont_idx = builder.block_params(shared_ws_loop)[0];
            let ws_pos = builder.use_var(pos_var);
            let ws_in_bounds = builder.ins().icmp(IntCC::UnsignedLessThan, ws_pos, len);
            builder.ins().brif(
                ws_in_bounds,
                shared_ws_check,
                &[BlockArg::from(loop_cont_idx)],
                shared_ws_dispatch,
                &[BlockArg::from(loop_cont_idx)],
            );

            // shared_ws_check: load byte and check if whitespace
            builder.switch_to_block(shared_ws_check);
            let check_cont_idx = builder.block_params(shared_ws_check)[0];
            let ws_pos_check = builder.use_var(pos_var); // Reload from var, can't use value from different block
            let ws_addr = builder.ins().iadd(input_ptr, ws_pos_check);
            let ws_byte = builder
                .ins()
                .load(types::I8, MemFlags::trusted(), ws_addr, 0);

            // Check if whitespace (space, tab, newline, cr)
            let space = builder.ins().iconst(types::I8, b' ' as i64);
            let tab = builder.ins().iconst(types::I8, b'\t' as i64);
            let newline = builder.ins().iconst(types::I8, b'\n' as i64);
            let cr = builder.ins().iconst(types::I8, b'\r' as i64);
            let is_space = builder.ins().icmp(IntCC::Equal, ws_byte, space);
            let is_tab = builder.ins().icmp(IntCC::Equal, ws_byte, tab);
            let is_newline = builder.ins().icmp(IntCC::Equal, ws_byte, newline);
            let is_cr = builder.ins().icmp(IntCC::Equal, ws_byte, cr);
            let is_ws1 = builder.ins().bor(is_space, is_tab);
            let is_ws2 = builder.ins().bor(is_newline, is_cr);
            let is_ws = builder.ins().bor(is_ws1, is_ws2);

            let shared_ws_advance = builder.create_block();
            builder.ins().brif(
                is_ws,
                shared_ws_advance,
                &[],
                shared_ws_dispatch,
                &[BlockArg::from(check_cont_idx)],
            );

            // shared_ws_advance: increment pos and loop
            builder.switch_to_block(shared_ws_advance);
            builder.seal_block(shared_ws_advance);
            let ws_pos_adv = builder.use_var(pos_var); // Reload from var
            let one = builder.ins().iconst(pointer_type, 1);
            let next_ws_pos = builder.ins().iadd(ws_pos_adv, one);
            builder.def_var(pos_var, next_ws_pos);
            builder
                .ins()
                .jump(shared_ws_loop, &[BlockArg::from(check_cont_idx)]);

            // Seal ws blocks now that all predecessors are known
            builder.seal_block(shared_ws_entry);
            builder.seal_block(shared_ws_loop);
            builder.seal_block(shared_ws_check);

            // shared_ws_dispatch: use comparison chain to jump to correct inline_value_block
            builder.switch_to_block(shared_ws_dispatch);
            let dispatch_cont_idx = builder.block_params(shared_ws_dispatch)[0];

            // Dispatch using comparison chain (simpler than br_table for typical struct sizes)
            // Handle empty case (should never happen, but need terminator for valid IR)
            if inline_value_blocks.is_empty() {
                builder.ins().trap(TrapCode::user(1).unwrap());
            }
            for (i, block) in inline_value_blocks.iter().enumerate() {
                let idx_val = builder.ins().iconst(pointer_type, i as i64);
                let is_match = builder.ins().icmp(IntCC::Equal, dispatch_cont_idx, idx_val);
                // Create a continuation block for the next comparison (or fallback)
                let next_block = if i + 1 < inline_value_blocks.len() {
                    let b = builder.create_block();
                    builder.append_block_param(b, pointer_type);
                    b
                } else {
                    // Last block - should never reach here, but trap if we do
                    builder.create_block()
                };
                let is_last = i + 1 >= inline_value_blocks.len();
                let next_args: &[BlockArg] = if is_last {
                    &[] // Trap block takes no args
                } else {
                    &[BlockArg::from(dispatch_cont_idx)]
                };
                builder
                    .ins()
                    .brif(is_match, *block, &[], next_block, next_args);
                if !is_last {
                    builder.seal_block(next_block);
                    builder.switch_to_block(next_block);
                    // Get the carried-through dispatch index
                    let _ = builder.block_params(next_block)[0];
                } else {
                    builder.seal_block(next_block);
                    builder.switch_to_block(next_block);
                    builder.ins().trap(TrapCode::user(1).unwrap());
                }
            }
            builder.seal_block(shared_ws_dispatch);

            // fallback_parse: no inline match, use regular parsing
            builder.switch_to_block(fallback_parse);
            builder.seal_block(fallback_parse);
        }

        // Regular key parsing (always needed for fallback/non-inline paths)
        let mut cursor = JitCursor {
            input_ptr,
            len,
            pos: pos_var,
            ptr_type: pointer_type,
            scratch_ptr,
        };

        let format = F::default();
        let (key_value, err_code) =
            format.emit_map_read_key(module, &mut builder, &mut cursor, state_ptr);
        builder.def_var(err_var, err_code);

        // Store key value in variables for use in dispatch
        builder.def_var(key_ptr_var, key_value.ptr);
        builder.def_var(key_len_var, key_value.len);
        builder.def_var(key_cap_var, key_value.cap);
        builder.def_var(key_owned_var, key_value.owned);

        builder.ins().jump(check_read_key_err, &[]);
        builder.seal_block(read_key);

        // check_read_key_err
        builder.switch_to_block(check_read_key_err);
        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
        builder.ins().brif(is_ok, key_dispatch, &[], error, &[]);
        builder.seal_block(check_read_key_err);

        // key_dispatch: match the key against field names
        builder.switch_to_block(key_dispatch);

        // For each dispatch entry (field or variant), create a match block and parse_value block
        // match_blocks: does kv_sep then jumps to parse_value_blocks
        // parse_value_blocks: does actual value parsing (shared by match_blocks and inline_value_blocks)
        let mut match_blocks = Vec::new();
        let mut parse_value_blocks = Vec::new();
        for _ in &dispatch_entries {
            match_blocks.push(builder.create_block());
            parse_value_blocks.push(builder.create_block());
        }

        // Handle empty dispatch table (only flattened map, no normal fields/variants)
        if dispatch_entries.is_empty() {
            builder.ins().jump(unknown_key, &[]);
            builder.seal_block(key_dispatch);
        } else {
            // Get key pointer and length
            let key_ptr = builder.use_var(key_ptr_var);
            let key_len = builder.use_var(key_len_var);

            // Dispatch based on strategy
            match dispatch_strategy {
                KeyDispatchStrategy::Inline | KeyDispatchStrategy::Linear => {
                    // Linear scan for small structs
                    // (Inline fallback also uses linear scan when inline matching fails)
                    let mut current_block = key_dispatch;
                    for (i, (key_name, _target)) in dispatch_entries.iter().enumerate() {
                        if i > 0 {
                            builder.switch_to_block(current_block);
                        }

                        let key_name_len = key_name.len();

                        // First check length
                        let len_matches =
                            builder
                                .ins()
                                .icmp_imm(IntCC::Equal, key_len, key_name_len as i64);

                        let check_content = builder.create_block();
                        let next_check = if i + 1 < dispatch_entries.len() {
                            builder.create_block()
                        } else {
                            unknown_key
                        };

                        builder
                            .ins()
                            .brif(len_matches, check_content, &[], next_check, &[]);
                        if i > 0 {
                            builder.seal_block(current_block);
                        }

                        // check_content: word-sized comparison for efficiency
                        // We already know key_len == key_name_len from the length check above
                        builder.switch_to_block(check_content);

                        let content_matches = if key_name_len <= 8 {
                            // For keys up to 8 bytes, use word-sized loads
                            let (expected_val, _) = compute_field_prefix(key_name, key_name_len);

                            // Choose the appropriate load type based on key length
                            let (load_type, mask_needed) = match key_name_len {
                                1 => (types::I8, false),
                                2 => (types::I16, false),
                                3 => (types::I32, true), // mask to 3 bytes
                                4 => (types::I32, false),
                                5..=7 => (types::I64, true), // mask to key_name_len bytes
                                8 => (types::I64, false),
                                _ => unreachable!(),
                            };

                            // Load the key data
                            let loaded_val =
                                builder
                                    .ins()
                                    .load(load_type, MemFlags::trusted(), key_ptr, 0);

                            // Compare with expected value (masking if needed)
                            if mask_needed {
                                // Mask off unused high bytes
                                let mask = (1u64 << (key_name_len * 8)) - 1;
                                let mask_val = builder.ins().iconst(load_type, mask as i64);
                                let masked = builder.ins().band(loaded_val, mask_val);
                                let expected = builder.ins().iconst(load_type, expected_val as i64);
                                builder.ins().icmp(IntCC::Equal, masked, expected)
                            } else {
                                let expected = builder.ins().iconst(load_type, expected_val as i64);
                                builder.ins().icmp(IntCC::Equal, loaded_val, expected)
                            }
                        } else {
                            // For longer keys, fall back to byte-by-byte comparison
                            let mut all_match = builder.ins().iconst(types::I8, 1);

                            for (j, &byte) in key_name.as_bytes().iter().enumerate() {
                                let offset = builder.ins().iconst(pointer_type, j as i64);
                                let char_ptr = builder.ins().iadd(key_ptr, offset);
                                let char_val =
                                    builder
                                        .ins()
                                        .load(types::I8, MemFlags::trusted(), char_ptr, 0);
                                let expected = builder.ins().iconst(types::I8, byte as i64);
                                let byte_matches =
                                    builder.ins().icmp(IntCC::Equal, char_val, expected);
                                let one = builder.ins().iconst(types::I8, 1);
                                let zero = builder.ins().iconst(types::I8, 0);
                                let byte_match_i8 = builder.ins().select(byte_matches, one, zero);
                                all_match = builder.ins().band(all_match, byte_match_i8);
                            }

                            builder.ins().icmp_imm(IntCC::NotEqual, all_match, 0)
                        };
                        builder
                            .ins()
                            .brif(content_matches, match_blocks[i], &[], next_check, &[]);
                        builder.seal_block(check_content);

                        current_block = next_check;
                    }

                    builder.seal_block(key_dispatch);
                    if dispatch_entries.len() > 1 {
                        builder.seal_block(current_block);
                    }
                }
                KeyDispatchStrategy::PrefixSwitch { prefix_len } => {
                    // Prefix-based dispatch for larger structs
                    // Group fields by prefix
                    use std::collections::HashMap;
                    let mut prefix_map: HashMap<u64, Vec<usize>> = HashMap::new();

                    for (i, field_info) in field_infos.iter().enumerate() {
                        let (prefix, _) = compute_field_prefix(field_info.name, prefix_len);
                        prefix_map.entry(prefix).or_default().push(i);
                    }

                    // Load prefix from key (handle short keys gracefully)
                    // Use a variable to hold the prefix value
                    let prefix_var = builder.declare_var(types::I64);

                    // First check if key is long enough for the full prefix
                    let prefix_len_i64 = prefix_len as i64;
                    let has_full_prefix = builder.ins().icmp_imm(
                        IntCC::UnsignedGreaterThanOrEqual,
                        key_len,
                        prefix_len_i64,
                    );

                    let load_full_prefix_block = builder.create_block();
                    let load_partial_prefix_block = builder.create_block();
                    let prefix_loaded_block = builder.create_block();

                    builder.ins().brif(
                        has_full_prefix,
                        load_full_prefix_block,
                        &[],
                        load_partial_prefix_block,
                        &[],
                    );

                    // Load full prefix
                    builder.switch_to_block(load_full_prefix_block);
                    // Note: key_ptr is a *const u8 into input slice, NOT guaranteed aligned
                    // Use unaligned load to avoid UB on some targets
                    let prefix_u64 =
                        builder
                            .ins()
                            .load(types::I64, MemFlags::trusted(), key_ptr, 0);
                    builder.def_var(prefix_var, prefix_u64);
                    builder.ins().jump(prefix_loaded_block, &[]);
                    builder.seal_block(load_full_prefix_block);

                    // Load partial prefix (byte by byte for short keys)
                    builder.switch_to_block(load_partial_prefix_block);
                    let partial_prefix = builder.ins().iconst(types::I64, 0);
                    // For simplicity, just set to 0 for short keys (they'll fall through to linear check)
                    builder.def_var(prefix_var, partial_prefix);
                    builder.ins().jump(prefix_loaded_block, &[]);
                    builder.seal_block(load_partial_prefix_block);

                    // prefix_loaded_block uses the variable
                    builder.switch_to_block(prefix_loaded_block);
                    let loaded_prefix_raw = builder.use_var(prefix_var);

                    // Mask the loaded prefix to only use prefix_len bytes
                    // compute_field_prefix only packs prefix_len bytes, but we load 8 bytes
                    // so we need to mask out the high bytes
                    let loaded_prefix = if prefix_len < 8 {
                        // mask = (1 << (prefix_len * 8)) - 1
                        let mask = (1u64 << (prefix_len * 8)) - 1;
                        let mask_val = builder.ins().iconst(types::I64, mask as i64);
                        builder.ins().band(loaded_prefix_raw, mask_val)
                    } else {
                        // prefix_len == 8, no masking needed
                        loaded_prefix_raw
                    };

                    // Build a switch table (cranelift expects u128 for EntryIndex)
                    // First, create disambiguation blocks for collisions and store them
                    let mut disambig_blocks: HashMap<u64, Block> = HashMap::new();

                    for (prefix_val, field_indices) in &prefix_map {
                        if field_indices.len() > 1 {
                            // Collision - create disambiguation block
                            let disambig_block = builder.create_block();
                            disambig_blocks.insert(*prefix_val, disambig_block);
                        }
                    }

                    // Build the switch table
                    let mut switch_data = cranelift::frontend::Switch::new();
                    let fallback_block = unknown_key;

                    for (prefix_val, field_indices) in &prefix_map {
                        if field_indices.len() == 1 {
                            // Unique prefix - direct match
                            let field_idx = field_indices[0];
                            switch_data.set_entry(*prefix_val as u128, match_blocks[field_idx]);
                        } else {
                            // Collision - use pre-created disambiguation block
                            let disambig_block = disambig_blocks[prefix_val];
                            switch_data.set_entry(*prefix_val as u128, disambig_block);
                        }
                    }

                    switch_data.emit(&mut builder, loaded_prefix, fallback_block);
                    builder.seal_block(prefix_loaded_block);

                    // Generate code for disambiguation blocks
                    for (prefix_val, field_indices) in &prefix_map {
                        if field_indices.len() > 1 {
                            // Collision case - need to check full string
                            let disambig_block = disambig_blocks[prefix_val];
                            builder.switch_to_block(disambig_block);

                            // Seal disambig_block immediately as it only has one predecessor (the switch)
                            builder.seal_block(disambig_block);

                            let mut current_check_block = disambig_block;
                            for (j, &field_idx) in field_indices.iter().enumerate() {
                                if j > 0 {
                                    builder.switch_to_block(current_check_block);
                                }

                                let field_name = field_infos[field_idx].name;
                                let field_name_len = field_name.len();

                                // Check length first
                                let len_matches = builder.ins().icmp_imm(
                                    IntCC::Equal,
                                    key_len,
                                    field_name_len as i64,
                                );

                                let check_full_match = builder.create_block();
                                let next_in_collision = if j + 1 < field_indices.len() {
                                    builder.create_block()
                                } else {
                                    fallback_block
                                };

                                builder.ins().brif(
                                    len_matches,
                                    check_full_match,
                                    &[],
                                    next_in_collision,
                                    &[],
                                );

                                // check_full_match: full string comparison
                                builder.switch_to_block(check_full_match);
                                let mut all_match = builder.ins().iconst(types::I8, 1);

                                for (k, &byte) in field_name.as_bytes().iter().enumerate() {
                                    let offset = builder.ins().iconst(pointer_type, k as i64);
                                    let char_ptr = builder.ins().iadd(key_ptr, offset);
                                    let char_val = builder.ins().load(
                                        types::I8,
                                        MemFlags::trusted(),
                                        char_ptr,
                                        0,
                                    );
                                    let expected = builder.ins().iconst(types::I8, byte as i64);
                                    let byte_matches =
                                        builder.ins().icmp(IntCC::Equal, char_val, expected);
                                    let one = builder.ins().iconst(types::I8, 1);
                                    let zero = builder.ins().iconst(types::I8, 0);
                                    let byte_match_i8 =
                                        builder.ins().select(byte_matches, one, zero);
                                    all_match = builder.ins().band(all_match, byte_match_i8);
                                }

                                let all_match_bool =
                                    builder.ins().icmp_imm(IntCC::NotEqual, all_match, 0);
                                builder.ins().brif(
                                    all_match_bool,
                                    match_blocks[field_idx],
                                    &[],
                                    next_in_collision,
                                    &[],
                                );
                                builder.seal_block(check_full_match);

                                // Now seal next_in_collision - both its predecessors are filled:
                                // 1. The brif from current_check_block's length check
                                // 2. The brif from check_full_match's full match check
                                if next_in_collision != fallback_block {
                                    builder.seal_block(next_in_collision);
                                }

                                // current_check_block was already sealed:
                                // - j=0: it's disambig_block (sealed before loop)
                                // - j>0: it's previous iteration's next_in_collision (sealed above)

                                current_check_block = next_in_collision;
                            }
                        }
                    }

                    // Seal unknown_key now that all predecessors are known:
                    // 1. Switch fallback (line 2196)
                    // 2. All disambiguation chain fallbacks (line 2229)
                    builder.seal_block(unknown_key);

                    builder.seal_block(key_dispatch);
                }
            } // end else (non-empty dispatch table)
        }

        // unknown_key: either insert into flattened map or skip the value
        builder.switch_to_block(unknown_key);

        let mut cursor = JitCursor {
            input_ptr,
            len,
            pos: pos_var,
            ptr_type: pointer_type,
            scratch_ptr,
        };

        // First consume the kv separator
        let format = F::default();
        let err_code = format.emit_map_kv_sep(module, &mut builder, &mut cursor, state_ptr);
        builder.def_var(err_var, err_code);

        // Check for error
        let kv_sep_ok = builder.create_block();
        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
        builder.ins().brif(is_ok, kv_sep_ok, &[], error, &[]);

        builder.switch_to_block(kv_sep_ok);

        // Branch on whether we have a flattened map for unknown key capture
        if let Some(flatten_map_info) = &flatten_map {
            jit_debug!(
                "Unknown key handler: capturing into flattened map at offset {}",
                flatten_map_info.map_field_offset
            );

            // Lazy-init the map if not already initialized
            let map_initialized_var = map_initialized_var.unwrap();
            let map_initialized = builder.use_var(map_initialized_var);
            let already_initialized = builder.ins().icmp_imm(IntCC::NotEqual, map_initialized, 0);
            let init_map = builder.create_block();
            let after_init = builder.create_block();
            builder
                .ins()
                .brif(already_initialized, after_init, &[], init_map, &[]);

            // init_map: initialize the HashMap with capacity 0
            builder.switch_to_block(init_map);
            jit_debug!("  Initializing flattened map on first unknown key");

            // Get map field pointer
            let map_ptr = builder
                .ins()
                .iadd_imm(out_ptr, flatten_map_info.map_field_offset as i64);

            // Get map init function from shape
            let map_shape = {
                // Reconstruct the map shape from the struct field
                // We need to find the corresponding field in struct_def
                let mut found_shape = None;
                for field in struct_def.fields {
                    if field.is_flattened() {
                        let field_shape = field.shape.get();
                        if let Def::Map(_) = &field_shape.def
                            && field.offset == flatten_map_info.map_field_offset
                        {
                            found_shape = Some(field_shape);
                            break;
                        }
                    }
                }
                found_shape.expect("flattened map shape must exist")
            };

            let map_def = match &map_shape.def {
                Def::Map(m) => m,
                _ => unreachable!("flatten_map_info must be from a Map"),
            };

            let init_fn = map_def.vtable.init_in_place_with_capacity;

            let map_init_sig_ref = {
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

            // Call jit_map_init_with_capacity(map_ptr, 0, init_fn)
            let zero_capacity = builder.ins().iconst(pointer_type, 0);
            let init_fn_ptr = builder.ins().iconst(pointer_type, init_fn as usize as i64);
            builder.ins().call_indirect(
                map_init_sig_ref,
                map_init_ptr,
                &[map_ptr, zero_capacity, init_fn_ptr],
            );

            // Mark map as initialized
            let one_i8 = builder.ins().iconst(types::I8, 1);
            builder.def_var(map_initialized_var, one_i8);

            builder.ins().jump(after_init, &[]);
            builder.seal_block(init_map);

            // after_init: parse the value and insert into map
            builder.switch_to_block(after_init);

            // Get map field pointer for insertion
            let map_ptr = builder
                .ins()
                .iadd_imm(out_ptr, flatten_map_info.map_field_offset as i64);

            // Get map insert function
            let map_def = match &map_shape.def {
                Def::Map(m) => m,
                _ => unreachable!("flatten_map must be a Map"),
            };
            let insert_fn = map_def.vtable.insert;

            let write_string_sig_ref = {
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

            // Create stack slots for key and value
            let key_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                3 * pointer_type.bytes(),
                pointer_type.bytes().trailing_zeros() as u8,
            ));
            let key_out_ptr = builder.ins().stack_addr(pointer_type, key_slot, 0);

            let value_layout = match flatten_map_info.value_shape.layout.sized_layout() {
                Ok(layout) => layout,
                Err(_) => {
                    jit_debug!("[compile_struct] Flattened map value has unsized layout");
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

            // Parse the value based on value_kind
            let value_shape = flatten_map_info.value_shape;
            let mut cursor = JitCursor {
                input_ptr,
                len,
                pos: pos_var,
                ptr_type: pointer_type,
                scratch_ptr,
            };

            // Create continuation block for after value is parsed and stored
            let value_stored = builder.create_block();

            match flatten_map_info.value_kind {
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
                    builder.ins().jump(value_stored, &[]);
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
                    builder.ins().jump(value_stored, &[]);
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
                    builder.ins().jump(value_stored, &[]);
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
                    builder.ins().jump(value_stored, &[]);
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
                    builder.ins().jump(value_stored, &[]);
                    builder.seal_block(store);
                }
                FormatListElementKind::String => {
                    let (string_value, err) =
                        format.emit_parse_string(module, &mut builder, &mut cursor);
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
                    builder.ins().jump(value_stored, &[]);
                    builder.seal_block(store);
                }
                FormatListElementKind::Struct(_) => {
                    let struct_func_id =
                        compile_struct_format_deserializer::<F>(module, value_shape, memo)?;
                    let struct_func_ref = module.declare_func_in_func(struct_func_id, builder.func);
                    let struct_func_ptr =
                        func_addr_value(&mut builder, pointer_type, struct_func_ref);
                    let current_pos = builder.use_var(pos_var);
                    let call_result = builder.ins().call_indirect(
                        nested_call_sig_ref,
                        struct_func_ptr,
                        &[input_ptr, len, current_pos, value_ptr, scratch_ptr],
                    );
                    let new_pos = builder.inst_results(call_result)[0];
                    let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
                    let nested_ok = builder.create_block();
                    builder.ins().brif(is_error, error, &[], nested_ok, &[]);
                    builder.switch_to_block(nested_ok);
                    builder.def_var(pos_var, new_pos);
                    builder.ins().jump(value_stored, &[]);
                    builder.seal_block(nested_ok);
                }
                FormatListElementKind::List(_) => {
                    let list_func_id =
                        compile_list_format_deserializer::<F>(module, value_shape, memo)?;
                    let list_func_ref = module.declare_func_in_func(list_func_id, builder.func);
                    let list_func_ptr = func_addr_value(&mut builder, pointer_type, list_func_ref);
                    let current_pos = builder.use_var(pos_var);
                    let call_result = builder.ins().call_indirect(
                        nested_call_sig_ref,
                        list_func_ptr,
                        &[input_ptr, len, current_pos, value_ptr, scratch_ptr],
                    );
                    let new_pos = builder.inst_results(call_result)[0];
                    let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
                    let nested_ok = builder.create_block();
                    builder.ins().brif(is_error, error, &[], nested_ok, &[]);
                    builder.switch_to_block(nested_ok);
                    builder.def_var(pos_var, new_pos);
                    builder.ins().jump(value_stored, &[]);
                    builder.seal_block(nested_ok);
                }
                FormatListElementKind::Map(_) => {
                    let map_func_id =
                        compile_map_format_deserializer::<F>(module, value_shape, memo)?;
                    let map_func_ref = module.declare_func_in_func(map_func_id, builder.func);
                    let map_func_ptr = func_addr_value(&mut builder, pointer_type, map_func_ref);
                    let current_pos = builder.use_var(pos_var);
                    let call_result = builder.ins().call_indirect(
                        nested_call_sig_ref,
                        map_func_ptr,
                        &[input_ptr, len, current_pos, value_ptr, scratch_ptr],
                    );
                    let new_pos = builder.inst_results(call_result)[0];
                    let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
                    let nested_ok = builder.create_block();
                    builder.ins().brif(is_error, error, &[], nested_ok, &[]);
                    builder.switch_to_block(nested_ok);
                    builder.def_var(pos_var, new_pos);
                    builder.ins().jump(value_stored, &[]);
                    builder.seal_block(nested_ok);
                }
            }

            // Switch to the continuation block after value is stored
            builder.switch_to_block(value_stored);

            // Materialize key into the stack slot using write_string
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
            // Key raw parts consumed by write_string when owned=1
            let zero_i8 = builder.ins().iconst(types::I8, 0);
            builder.def_var(key_owned_var, zero_i8);

            // Insert (key, value) into the map
            let insert_fn_addr = builder
                .ins()
                .iconst(pointer_type, insert_fn as usize as i64);
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
            let sig_ref_map_insert = builder.import_signature(sig_map_insert);
            let zero_meta = builder.ins().iconst(pointer_type, 0);
            builder.ins().call_indirect(
                sig_ref_map_insert,
                insert_fn_addr,
                &[
                    map_ptr,
                    zero_meta,
                    key_out_ptr,
                    zero_meta,
                    value_ptr,
                    zero_meta,
                ],
            );

            // Continue to after_value (no error checking for insert, no key cleanup needed)
            builder.ins().jump(after_value, &[]);
            builder.seal_block(value_stored);
            builder.seal_block(after_init);
            builder.seal_block(kv_sep_ok);
        } else {
            // No flattened map - skip the value (original behavior)
            let err_code = format.emit_skip_value(module, &mut builder, &mut cursor);
            builder.def_var(err_var, err_code);

            // Check if owned key needs cleanup
            let key_owned = builder.use_var(key_owned_var);
            let needs_drop = builder.ins().icmp_imm(IntCC::NotEqual, key_owned, 0);
            let drop_key = builder.create_block();
            let after_drop = builder.create_block();
            builder
                .ins()
                .brif(needs_drop, drop_key, &[], after_drop, &[]);

            // drop_key: call jit_drop_owned_string
            builder.switch_to_block(drop_key);
            let key_ptr = builder.use_var(key_ptr_var);
            let key_len = builder.use_var(key_len_var);
            let key_cap = builder.use_var(key_cap_var);

            let sig_drop = {
                let mut s = make_c_sig(module);
                s.params.push(AbiParam::new(pointer_type)); // ptr
                s.params.push(AbiParam::new(pointer_type)); // len
                s.params.push(AbiParam::new(pointer_type)); // cap
                s
            };
            let drop_sig_ref = builder.import_signature(sig_drop);
            let drop_ptr = builder.ins().iconst(
                pointer_type,
                helpers::jit_drop_owned_string as *const u8 as i64,
            );
            builder
                .ins()
                .call_indirect(drop_sig_ref, drop_ptr, &[key_ptr, key_len, key_cap]);
            builder.ins().jump(after_drop, &[]);
            builder.seal_block(drop_key);

            // after_drop: check skip_value error and continue
            builder.switch_to_block(after_drop);
            let skip_err = builder.use_var(err_var);
            let is_ok = builder.ins().icmp_imm(IntCC::Equal, skip_err, 0);
            builder.ins().brif(is_ok, after_value, &[], error, &[]);
            builder.seal_block(kv_sep_ok);
            builder.seal_block(after_drop);
        }
        // Note: unknown_key is already sealed by both dispatch strategies:
        // - Linear: sealed as current_block on the last field iteration
        // - PrefixSwitch: sealed after all disambiguation blocks are generated
        // Only seal if we have a single dispatch entry (special case)
        if dispatch_entries.len() == 1 {
            builder.seal_block(unknown_key);
        }

        // Implement match blocks for each dispatch entry (field or variant)
        // match_blocks only do kv_sep and jump to parse_value_blocks
        for (i, (_key_name, target)) in dispatch_entries.iter().enumerate() {
            builder.switch_to_block(match_blocks[i]);

            match target {
                DispatchTarget::Field(_field_idx) => {
                    // Consume the kv separator (':' in JSON), then jump to value parsing
                    let mut cursor = JitCursor {
                        input_ptr,
                        len,
                        pos: pos_var,
                        ptr_type: pointer_type,
                        scratch_ptr,
                    };

                    let format = F::default();
                    let err_code =
                        format.emit_map_kv_sep(module, &mut builder, &mut cursor, state_ptr);
                    builder.def_var(err_var, err_code);

                    // Check for error - on success jump to parse_value_blocks
                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                    builder
                        .ins()
                        .brif(is_ok, parse_value_blocks[i], &[], error, &[]);
                }
                DispatchTarget::FlattenEnumVariant(_variant_idx) => {
                    // Consume the kv separator (':' in JSON), then jump to value parsing
                    let mut cursor = JitCursor {
                        input_ptr,
                        len,
                        pos: pos_var,
                        ptr_type: pointer_type,
                        scratch_ptr,
                    };

                    let format = F::default();
                    let err_code =
                        format.emit_map_kv_sep(module, &mut builder, &mut cursor, state_ptr);
                    builder.def_var(err_var, err_code);

                    // Check for error - on success jump to parse_value_blocks
                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                    builder
                        .ins()
                        .brif(is_ok, parse_value_blocks[i], &[], error, &[]);
                }
            }

            builder.seal_block(match_blocks[i]);
        }

        // Populate inline_value_blocks (for Inline strategy) - they skip kv_sep
        if matches!(dispatch_strategy, KeyDispatchStrategy::Inline) {
            for i in 0..dispatch_entries.len() {
                builder.switch_to_block(inline_value_blocks[i]);
                // No key was parsed in inline path, set owned=0 to skip cleanup
                let zero_i8 = builder.ins().iconst(types::I8, 0);
                builder.def_var(key_owned_var, zero_i8);
                // Jump directly to value parsing (kv_sep already consumed in inline match)
                builder.ins().jump(parse_value_blocks[i], &[]);
                builder.seal_block(inline_value_blocks[i]);
            }
        }

        // Implement parse_value_blocks for each dispatch entry (field or variant)
        // This contains the actual value parsing logic, shared by match_blocks and inline_value_blocks
        for (i, (_key_name, target)) in dispatch_entries.iter().enumerate() {
            builder.switch_to_block(parse_value_blocks[i]);

            match target {
                DispatchTarget::Field(field_idx) => {
                    // Normal field parsing (existing logic)
                    let field_info = &field_infos[*field_idx];

                    jit_debug!(
                        "Processing field {}: '{}' type {:?}",
                        i,
                        field_info.name,
                        field_info.shape.def
                    );

                    // Parse the field value based on its type
                    let field_shape = field_info.shape;
                    let field_ptr = builder.ins().iadd_imm(out_ptr, field_info.offset as i64);

                    // Duplicate cleanup: drop old value if this required field was already seen
                    // This prevents memory leaks when duplicate keys appear in JSON for owned types
                    // (String, Vec, HashMap, enum payloads)
                    if let Some(bit_index) = field_info.required_bit_index {
                        let bits = builder.use_var(required_bits_var);
                        let mask = builder.ins().iconst(types::I64, 1i64 << bit_index);
                        let already_set_bits = builder.ins().band(bits, mask);
                        let already_set =
                            builder.ins().icmp_imm(IntCC::NotEqual, already_set_bits, 0);

                        let drop_old = builder.create_block();
                        let after_drop = builder.create_block();
                        builder
                            .ins()
                            .brif(already_set, drop_old, &[], after_drop, &[]);

                        // drop_old: call jit_drop_in_place to drop the previous value
                        builder.switch_to_block(drop_old);
                        let field_shape_ptr = builder
                            .ins()
                            .iconst(pointer_type, field_shape as *const Shape as usize as i64);

                        let sig_drop = {
                            let mut s = make_c_sig(module);
                            s.params.push(AbiParam::new(pointer_type)); // shape_ptr
                            s.params.push(AbiParam::new(pointer_type)); // ptr
                            s
                        };
                        let drop_sig_ref = builder.import_signature(sig_drop);
                        let drop_ptr = builder
                            .ins()
                            .iconst(pointer_type, helpers::jit_drop_in_place as *const u8 as i64);
                        builder.ins().call_indirect(
                            drop_sig_ref,
                            drop_ptr,
                            &[field_shape_ptr, field_ptr],
                        );
                        builder.ins().jump(after_drop, &[]);
                        builder.seal_block(drop_old);

                        // after_drop: proceed with parsing the new value
                        builder.switch_to_block(after_drop);
                        builder.seal_block(after_drop);
                    }

                    // For MVP: only support scalar types
                    // Vec and nested structs will be added later
                    use facet_core::ScalarType;
                    jit_debug!(
                        "[compile_struct]   Parsing field '{}', scalar_type = {:?}",
                        field_info.name,
                        field_shape.scalar_type()
                    );
                    if let Some(scalar_type) = field_shape.scalar_type() {
                        // Parse scalar value
                        let mut cursor = JitCursor {
                            input_ptr,
                            len,
                            pos: pos_var,
                            ptr_type: pointer_type,
                            scratch_ptr,
                        };

                        let format = F::default();

                        // Create a shared continuation block for all scalar parsing paths
                        let parse_and_store_done = builder.create_block();

                        match scalar_type {
                            ScalarType::Bool => {
                                let (value, err) =
                                    format.emit_parse_bool(module, &mut builder, &mut cursor);
                                builder.def_var(err_var, err);
                                let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);

                                // Create dedicated block for storing this type
                                let bool_store = builder.create_block();
                                builder.ins().brif(is_ok, bool_store, &[], error, &[]);

                                builder.switch_to_block(bool_store);
                                builder
                                    .ins()
                                    .store(MemFlags::trusted(), value, field_ptr, 0);
                                builder.ins().jump(parse_and_store_done, &[]);
                                builder.seal_block(bool_store);
                            }
                            ScalarType::I8
                            | ScalarType::I16
                            | ScalarType::I32
                            | ScalarType::I64 => {
                                let (value_i64, err) =
                                    format.emit_parse_i64(module, &mut builder, &mut cursor);
                                builder.def_var(err_var, err);
                                let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);

                                let int_store = builder.create_block();
                                builder.ins().brif(is_ok, int_store, &[], error, &[]);

                                builder.switch_to_block(int_store);
                                let value = match scalar_type {
                                    ScalarType::I8 => builder.ins().ireduce(types::I8, value_i64),
                                    ScalarType::I16 => builder.ins().ireduce(types::I16, value_i64),
                                    ScalarType::I32 => builder.ins().ireduce(types::I32, value_i64),
                                    ScalarType::I64 => value_i64,
                                    _ => unreachable!(),
                                };
                                builder
                                    .ins()
                                    .store(MemFlags::trusted(), value, field_ptr, 0);
                                builder.ins().jump(parse_and_store_done, &[]);
                                builder.seal_block(int_store);
                            }
                            ScalarType::U8
                            | ScalarType::U16
                            | ScalarType::U32
                            | ScalarType::U64 => {
                                let (value_u64, err) =
                                    format.emit_parse_u64(module, &mut builder, &mut cursor);
                                builder.def_var(err_var, err);
                                let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);

                                let uint_store = builder.create_block();
                                builder.ins().brif(is_ok, uint_store, &[], error, &[]);

                                builder.switch_to_block(uint_store);
                                let value = match scalar_type {
                                    ScalarType::U8 => builder.ins().ireduce(types::I8, value_u64),
                                    ScalarType::U16 => builder.ins().ireduce(types::I16, value_u64),
                                    ScalarType::U32 => builder.ins().ireduce(types::I32, value_u64),
                                    ScalarType::U64 => value_u64,
                                    _ => unreachable!(),
                                };
                                builder
                                    .ins()
                                    .store(MemFlags::trusted(), value, field_ptr, 0);
                                builder.ins().jump(parse_and_store_done, &[]);
                                builder.seal_block(uint_store);
                            }
                            ScalarType::F32 | ScalarType::F64 => {
                                let (value_f64, err) =
                                    format.emit_parse_f64(module, &mut builder, &mut cursor);
                                builder.def_var(err_var, err);
                                let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);

                                let float_store = builder.create_block();
                                builder.ins().brif(is_ok, float_store, &[], error, &[]);

                                builder.switch_to_block(float_store);
                                let value = if matches!(scalar_type, ScalarType::F32) {
                                    builder.ins().fdemote(types::F32, value_f64)
                                } else {
                                    value_f64
                                };
                                builder
                                    .ins()
                                    .store(MemFlags::trusted(), value, field_ptr, 0);
                                builder.ins().jump(parse_and_store_done, &[]);
                                builder.seal_block(float_store);
                            }
                            ScalarType::String => {
                                let (string_val, err) =
                                    format.emit_parse_string(module, &mut builder, &mut cursor);
                                builder.def_var(err_var, err);
                                let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);

                                let string_store = builder.create_block();
                                builder.ins().brif(is_ok, string_store, &[], error, &[]);

                                builder.switch_to_block(string_store);

                                // Write String to field using jit_write_string helper
                                let sig_write_string = {
                                    let mut s = make_c_sig(module);
                                    s.params.push(AbiParam::new(pointer_type)); // out_ptr
                                    s.params.push(AbiParam::new(pointer_type)); // offset
                                    s.params.push(AbiParam::new(pointer_type)); // str_ptr
                                    s.params.push(AbiParam::new(pointer_type)); // str_len
                                    s.params.push(AbiParam::new(pointer_type)); // str_cap
                                    s.params.push(AbiParam::new(types::I8)); // owned
                                    s
                                };
                                let write_string_sig_ref =
                                    builder.import_signature(sig_write_string);
                                let write_string_ptr = builder.ins().iconst(
                                    pointer_type,
                                    helpers::jit_write_string as *const u8 as i64,
                                );
                                let field_offset =
                                    builder.ins().iconst(pointer_type, field_info.offset as i64);
                                builder.ins().call_indirect(
                                    write_string_sig_ref,
                                    write_string_ptr,
                                    &[
                                        out_ptr,
                                        field_offset,
                                        string_val.ptr,
                                        string_val.len,
                                        string_val.cap,
                                        string_val.owned,
                                    ],
                                );
                                builder.ins().jump(parse_and_store_done, &[]);
                                builder.seal_block(string_store);
                            }
                            _ => {
                                // Unsupported scalar type - fall back to Tier-1
                                jit_debug!(
                                    "[compile_struct] Unsupported scalar type: {:?}",
                                    scalar_type
                                );
                                return None;
                            }
                        }

                        // Now switch to parse_and_store_done for the shared code
                        builder.switch_to_block(parse_and_store_done);

                        // Set required bit if this is a required field
                        if let Some(bit_index) = field_info.required_bit_index {
                            let bits = builder.use_var(required_bits_var);
                            let bit_mask = builder.ins().iconst(types::I64, 1i64 << bit_index);
                            let new_bits = builder.ins().bor(bits, bit_mask);
                            builder.def_var(required_bits_var, new_bits);
                        }

                        // Drop owned key if needed
                        let key_owned = builder.use_var(key_owned_var);
                        let needs_drop = builder.ins().icmp_imm(IntCC::NotEqual, key_owned, 0);
                        let drop_key2 = builder.create_block();
                        let after_drop2 = builder.create_block();
                        builder
                            .ins()
                            .brif(needs_drop, drop_key2, &[], after_drop2, &[]);

                        // Seal parse_and_store_done now that it has a terminator (the brif above)
                        builder.seal_block(parse_and_store_done);

                        builder.switch_to_block(drop_key2);
                        let key_ptr = builder.use_var(key_ptr_var);
                        let key_len = builder.use_var(key_len_var);
                        let key_cap = builder.use_var(key_cap_var);
                        // Reuse drop helper signature from earlier
                        let sig_drop = {
                            let mut s = make_c_sig(module);
                            s.params.push(AbiParam::new(pointer_type));
                            s.params.push(AbiParam::new(pointer_type));
                            s.params.push(AbiParam::new(pointer_type));
                            s
                        };
                        let drop_sig_ref = builder.import_signature(sig_drop);
                        let drop_ptr = builder.ins().iconst(
                            pointer_type,
                            helpers::jit_drop_owned_string as *const u8 as i64,
                        );
                        builder.ins().call_indirect(
                            drop_sig_ref,
                            drop_ptr,
                            &[key_ptr, key_len, key_cap],
                        );
                        builder.ins().jump(after_drop2, &[]);
                        builder.seal_block(drop_key2);

                        builder.switch_to_block(after_drop2);
                        builder.ins().jump(after_value, &[]);
                        builder.seal_block(after_drop2);
                    } else if matches!(field_shape.def, Def::Option(_)) {
                        // Handle Option<T> fields
                        // Strategy: peek to check if null, then either consume null (None) or parse value (Some)
                        jit_debug!(
                            "[compile_struct]   Parsing Option field '{}'",
                            field_info.name
                        );

                        let mut cursor = JitCursor {
                            input_ptr,
                            len,
                            pos: pos_var,
                            ptr_type: pointer_type,
                            scratch_ptr,
                        };

                        let format = F::default();

                        // Peek to check if the value is null
                        let (is_null_u8, peek_err) =
                            format.emit_peek_null(&mut builder, &mut cursor);
                        builder.def_var(err_var, peek_err);
                        let peek_ok = builder.ins().icmp_imm(IntCC::Equal, peek_err, 0);

                        let check_null_block = builder.create_block();
                        builder
                            .ins()
                            .brif(peek_ok, check_null_block, &[], error, &[]);

                        builder.switch_to_block(check_null_block);
                        builder.seal_block(check_null_block);
                        let is_null = builder.ins().icmp_imm(IntCC::NotEqual, is_null_u8, 0);

                        let handle_none_block = builder.create_block();
                        let handle_some_block = builder.create_block();
                        builder
                            .ins()
                            .brif(is_null, handle_none_block, &[], handle_some_block, &[]);

                        // Handle None case: consume null, drop old value, and re-init to None
                        // This handles duplicate keys like {"opt":"x","opt":null} -> None (no leak)
                        builder.switch_to_block(handle_none_block);
                        let consume_err = format.emit_consume_null(&mut builder, &mut cursor);
                        builder.def_var(err_var, consume_err);
                        let consume_ok = builder.ins().icmp_imm(IntCC::Equal, consume_err, 0);
                        let none_done = builder.create_block();
                        builder.ins().brif(consume_ok, none_done, &[], error, &[]);

                        builder.switch_to_block(none_done);

                        // Drop the previous value (safe even if it's None)
                        let field_shape_ptr = builder
                            .ins()
                            .iconst(pointer_type, field_shape as *const Shape as usize as i64);
                        let sig_drop = {
                            let mut s = make_c_sig(module);
                            s.params.push(AbiParam::new(pointer_type)); // shape_ptr
                            s.params.push(AbiParam::new(pointer_type)); // ptr
                            s
                        };
                        let drop_sig_ref = builder.import_signature(sig_drop);
                        let drop_ptr = builder
                            .ins()
                            .iconst(pointer_type, helpers::jit_drop_in_place as *const u8 as i64);
                        builder.ins().call_indirect(
                            drop_sig_ref,
                            drop_ptr,
                            &[field_shape_ptr, field_ptr],
                        );

                        // Re-initialize to None (ensures valid None state regardless of previous value)
                        let Def::Option(option_def) = &field_shape.def else {
                            unreachable!();
                        };
                        let init_none_fn_ptr = builder.ins().iconst(
                            pointer_type,
                            option_def.vtable.init_none as *const () as i64,
                        );
                        let sig_option_init_none = {
                            let mut s = make_c_sig(module);
                            s.params.push(AbiParam::new(pointer_type)); // out_ptr
                            s.params.push(AbiParam::new(pointer_type)); // init_none_fn
                            s
                        };
                        let option_init_none_sig_ref =
                            builder.import_signature(sig_option_init_none);
                        let option_init_none_ptr = builder.ins().iconst(
                            pointer_type,
                            helpers::jit_option_init_none as *const u8 as i64,
                        );
                        builder.ins().call_indirect(
                            option_init_none_sig_ref,
                            option_init_none_ptr,
                            &[field_ptr, init_none_fn_ptr],
                        );

                        builder.ins().jump(after_value, &[]);
                        builder.seal_block(handle_none_block);
                        builder.seal_block(none_done);

                        // Handle Some case: parse inner value and init to Some
                        builder.switch_to_block(handle_some_block);
                        builder.seal_block(handle_some_block);

                        // Get the inner type of the Option
                        let Def::Option(option_def) = &field_shape.def else {
                            unreachable!();
                        };
                        let inner_shape = option_def.t;

                        // For now, only support Option<scalar> (not Option<Vec> or Option<struct>)
                        if let Some(inner_scalar_type) = inner_shape.scalar_type() {
                            // Allocate stack slot for inner value (256 bytes is enough for any scalar)
                            let value_slot = builder.create_sized_stack_slot(StackSlotData::new(
                                StackSlotKind::ExplicitSlot,
                                256,
                                8,
                            ));
                            let value_ptr = builder.ins().stack_addr(pointer_type, value_slot, 0);

                            // Create block for calling the init_some helper after parsing
                            let call_init_some = builder.create_block();

                            // Parse inner scalar value based on type
                            match inner_scalar_type {
                                ScalarType::Bool => {
                                    let (value, err) =
                                        format.emit_parse_bool(module, &mut builder, &mut cursor);
                                    builder.def_var(err_var, err);
                                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);

                                    let bool_store = builder.create_block();
                                    builder.ins().brif(is_ok, bool_store, &[], error, &[]);

                                    builder.switch_to_block(bool_store);
                                    builder
                                        .ins()
                                        .store(MemFlags::trusted(), value, value_ptr, 0);
                                    builder.ins().jump(call_init_some, &[]);
                                    builder.seal_block(bool_store);
                                }
                                ScalarType::I8
                                | ScalarType::I16
                                | ScalarType::I32
                                | ScalarType::I64 => {
                                    let (value_i64, err) =
                                        format.emit_parse_i64(module, &mut builder, &mut cursor);
                                    builder.def_var(err_var, err);
                                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);

                                    let int_store = builder.create_block();
                                    builder.ins().brif(is_ok, int_store, &[], error, &[]);

                                    builder.switch_to_block(int_store);
                                    let value = match inner_scalar_type {
                                        ScalarType::I8 => {
                                            builder.ins().ireduce(types::I8, value_i64)
                                        }
                                        ScalarType::I16 => {
                                            builder.ins().ireduce(types::I16, value_i64)
                                        }
                                        ScalarType::I32 => {
                                            builder.ins().ireduce(types::I32, value_i64)
                                        }
                                        ScalarType::I64 => value_i64,
                                        _ => unreachable!(),
                                    };
                                    builder
                                        .ins()
                                        .store(MemFlags::trusted(), value, value_ptr, 0);
                                    builder.ins().jump(call_init_some, &[]);
                                    builder.seal_block(int_store);
                                }
                                ScalarType::U8
                                | ScalarType::U16
                                | ScalarType::U32
                                | ScalarType::U64 => {
                                    let (value_u64, err) =
                                        format.emit_parse_u64(module, &mut builder, &mut cursor);
                                    builder.def_var(err_var, err);
                                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);

                                    let uint_store = builder.create_block();
                                    builder.ins().brif(is_ok, uint_store, &[], error, &[]);

                                    builder.switch_to_block(uint_store);
                                    let value = match inner_scalar_type {
                                        ScalarType::U8 => {
                                            builder.ins().ireduce(types::I8, value_u64)
                                        }
                                        ScalarType::U16 => {
                                            builder.ins().ireduce(types::I16, value_u64)
                                        }
                                        ScalarType::U32 => {
                                            builder.ins().ireduce(types::I32, value_u64)
                                        }
                                        ScalarType::U64 => value_u64,
                                        _ => unreachable!(),
                                    };
                                    builder
                                        .ins()
                                        .store(MemFlags::trusted(), value, value_ptr, 0);
                                    builder.ins().jump(call_init_some, &[]);
                                    builder.seal_block(uint_store);
                                }
                                ScalarType::F32 | ScalarType::F64 => {
                                    let (value_f64, err) =
                                        format.emit_parse_f64(module, &mut builder, &mut cursor);
                                    builder.def_var(err_var, err);
                                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);

                                    let float_store = builder.create_block();
                                    builder.ins().brif(is_ok, float_store, &[], error, &[]);

                                    builder.switch_to_block(float_store);
                                    let value = if matches!(inner_scalar_type, ScalarType::F32) {
                                        builder.ins().fdemote(types::F32, value_f64)
                                    } else {
                                        value_f64
                                    };
                                    builder
                                        .ins()
                                        .store(MemFlags::trusted(), value, value_ptr, 0);
                                    builder.ins().jump(call_init_some, &[]);
                                    builder.seal_block(float_store);
                                }
                                ScalarType::String => {
                                    // Parse String then materialize it into a temporary stack slot, then
                                    // call init_some which will move it into the Option.
                                    let (string_val, err) =
                                        format.emit_parse_string(module, &mut builder, &mut cursor);
                                    builder.def_var(err_var, err);
                                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);

                                    let string_store = builder.create_block();
                                    builder.ins().brif(is_ok, string_store, &[], error, &[]);

                                    builder.switch_to_block(string_store);

                                    // Declare jit_write_string helper
                                    let sig_write_string = {
                                        let mut s = make_c_sig(module);
                                        s.params.push(AbiParam::new(pointer_type)); // out_ptr
                                        s.params.push(AbiParam::new(pointer_type)); // offset
                                        s.params.push(AbiParam::new(pointer_type)); // str_ptr
                                        s.params.push(AbiParam::new(pointer_type)); // str_len
                                        s.params.push(AbiParam::new(pointer_type)); // str_cap
                                        s.params.push(AbiParam::new(types::I8)); // owned
                                        s
                                    };
                                    let write_string_sig_ref =
                                        builder.import_signature(sig_write_string);
                                    let write_string_ptr = builder.ins().iconst(
                                        pointer_type,
                                        helpers::jit_write_string as *const u8 as i64,
                                    );
                                    let zero_offset = builder.ins().iconst(pointer_type, 0);
                                    builder.ins().call_indirect(
                                        write_string_sig_ref,
                                        write_string_ptr,
                                        &[
                                            value_ptr,
                                            zero_offset,
                                            string_val.ptr,
                                            string_val.len,
                                            string_val.cap,
                                            string_val.owned,
                                        ],
                                    );

                                    builder.ins().jump(call_init_some, &[]);
                                    builder.seal_block(string_store);
                                }
                                _ => {
                                    jit_debug!(
                                        "[compile_struct] Unsupported Option<scalar> type: {:?}",
                                        inner_scalar_type
                                    );
                                    return None;
                                }
                            }

                            // After storing the value, call jit_option_init_some_from_value
                            // This helper takes (field_ptr, value_ptr, init_some_fn)
                            builder.switch_to_block(call_init_some);
                            builder.seal_block(call_init_some);

                            // Drop the previous value before overwriting with new Some
                            // This handles duplicate keys like {"opt":"x","opt":"y"} -> Some("y") (no leak)
                            // Use the Option shape pointer (field_shape), not the inner shape
                            let field_shape_ptr = builder
                                .ins()
                                .iconst(pointer_type, field_shape as *const Shape as usize as i64);

                            let sig_drop = {
                                let mut s = make_c_sig(module);
                                s.params.push(AbiParam::new(pointer_type)); // shape_ptr
                                s.params.push(AbiParam::new(pointer_type)); // ptr
                                s
                            };
                            let drop_sig_ref = builder.import_signature(sig_drop);
                            let drop_ptr = builder.ins().iconst(
                                pointer_type,
                                helpers::jit_drop_in_place as *const u8 as i64,
                            );
                            builder.ins().call_indirect(
                                drop_sig_ref,
                                drop_ptr,
                                &[field_shape_ptr, field_ptr],
                            );

                            let init_some_fn_ptr = option_def.vtable.init_some as *const u8;
                            let init_some_fn_val =
                                builder.ins().iconst(pointer_type, init_some_fn_ptr as i64);

                            let sig_option_init = {
                                let mut s = make_c_sig(module);
                                s.params.push(AbiParam::new(pointer_type)); // field_ptr
                                s.params.push(AbiParam::new(pointer_type)); // value_ptr
                                s.params.push(AbiParam::new(pointer_type)); // init_some_fn
                                s
                            };
                            let option_init_sig_ref = builder.import_signature(sig_option_init);
                            let option_init_ptr = builder.ins().iconst(
                                pointer_type,
                                helpers::jit_option_init_some_from_value as *const u8 as i64,
                            );
                            builder.ins().call_indirect(
                                option_init_sig_ref,
                                option_init_ptr,
                                &[field_ptr, value_ptr, init_some_fn_val],
                            );
                            builder.ins().jump(after_value, &[]);
                        } else {
                            jit_debug!(
                                "[compile_struct] Option<non-scalar> not supported for field '{}'",
                                field_info.name
                            );
                            return None;
                        }
                    } else if matches!(field_shape.ty, Type::User(UserType::Struct(_))) {
                        // Handle nested struct fields
                        jit_debug!(
                            "[compile_struct]   Parsing nested struct field '{}'",
                            field_info.name
                        );

                        // Recursively compile the nested struct deserializer
                        let nested_func_id =
                            compile_struct_format_deserializer::<F>(module, field_shape, memo)?;
                        let nested_func_ref =
                            module.declare_func_in_func(nested_func_id, builder.func);
                        let nested_func_ptr =
                            func_addr_value(&mut builder, pointer_type, nested_func_ref);

                        // Get field pointer (out_ptr + field offset)
                        let field_ptr = builder.ins().iadd_imm(out_ptr, field_info.offset as i64);

                        // Read current pos
                        let current_pos = builder.use_var(pos_var);

                        // Call nested struct deserializer: (input_ptr, len, pos, field_ptr, scratch_ptr)
                        let call_result = builder.ins().call_indirect(
                            nested_call_sig_ref,
                            nested_func_ptr,
                            &[input_ptr, len, current_pos, field_ptr, scratch_ptr],
                        );
                        let new_pos = builder.inst_results(call_result)[0];

                        // Check for error (new_pos < 0 means error)
                        let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);

                        let nested_ok = builder.create_block();
                        builder.ins().brif(is_error, error, &[], nested_ok, &[]);

                        // On success: update pos_var and continue
                        builder.switch_to_block(nested_ok);
                        builder.def_var(pos_var, new_pos);

                        // Set required bit if this is a required field
                        if let Some(bit_index) = field_info.required_bit_index {
                            let bits = builder.use_var(required_bits_var);
                            let bit_mask = builder.ins().iconst(types::I64, 1i64 << bit_index);
                            let new_bits = builder.ins().bor(bits, bit_mask);
                            builder.def_var(required_bits_var, new_bits);
                        }

                        builder.ins().jump(after_value, &[]);
                        builder.seal_block(nested_ok);
                    } else if let Def::List(list_def) = &field_shape.def {
                        // Handle Vec<T> fields
                        jit_debug!("[compile_struct]   Parsing Vec field '{}'", field_info.name);

                        // Get field pointer (out_ptr + field offset)
                        let field_ptr = builder.ins().iadd_imm(out_ptr, field_info.offset as i64);

                        // Fast path: try to match empty array `[]` inline
                        let format = F::default();
                        let mut cursor = JitCursor {
                            input_ptr,
                            len,
                            pos: pos_var,
                            ptr_type: pointer_type,
                            scratch_ptr,
                        };

                        // Blocks for the fast path
                        let list_fast_path_success = builder.create_block();
                        let list_slow_path = builder.create_block();

                        if let Some((is_empty, empty_err)) =
                            format.emit_try_empty_seq(&mut builder, &mut cursor)
                        {
                            // Check for error first
                            let no_err = builder.ins().icmp_imm(IntCC::Equal, empty_err, 0);
                            let check_empty = builder.create_block();
                            builder.def_var(err_var, empty_err);
                            builder.ins().brif(no_err, check_empty, &[], error, &[]);

                            // Check if it was empty
                            builder.switch_to_block(check_empty);
                            builder.seal_block(check_empty);
                            let was_empty = builder.ins().icmp_imm(IntCC::NotEqual, is_empty, 0);
                            builder.ins().brif(
                                was_empty,
                                list_fast_path_success,
                                &[],
                                list_slow_path,
                                &[],
                            );

                            // Fast path: initialize empty Vec inline
                            builder.switch_to_block(list_fast_path_success);
                            builder.seal_block(list_fast_path_success);

                            // Get init_in_place_with_capacity function
                            if let Some(init_fn) = list_def.init_in_place_with_capacity() {
                                let init_fn_ptr = builder
                                    .ins()
                                    .iconst(pointer_type, init_fn as *const () as i64);
                                let zero_capacity = builder.ins().iconst(pointer_type, 0);

                                // Call jit_vec_init_with_capacity(out, capacity=0, init_fn)
                                let sig_vec_init = {
                                    let mut s = make_c_sig(module);
                                    s.params.push(AbiParam::new(pointer_type)); // out
                                    s.params.push(AbiParam::new(pointer_type)); // capacity
                                    s.params.push(AbiParam::new(pointer_type)); // init_fn
                                    s
                                };
                                let sig_vec_init_ref = builder.import_signature(sig_vec_init);
                                let vec_init_ptr = builder.ins().iconst(
                                    pointer_type,
                                    helpers::jit_vec_init_with_capacity as *const u8 as i64,
                                );
                                builder.ins().call_indirect(
                                    sig_vec_init_ref,
                                    vec_init_ptr,
                                    &[field_ptr, zero_capacity, init_fn_ptr],
                                );

                                // Set required bit if needed
                                if let Some(bit_index) = field_info.required_bit_index {
                                    let bits = builder.use_var(required_bits_var);
                                    let bit_mask =
                                        builder.ins().iconst(types::I64, 1i64 << bit_index);
                                    let new_bits = builder.ins().bor(bits, bit_mask);
                                    builder.def_var(required_bits_var, new_bits);
                                }

                                builder.ins().jump(after_value, &[]);
                            } else {
                                // No init function available, fall through to slow path
                                builder.ins().jump(list_slow_path, &[]);
                            }

                            // Slow path: call full list deserializer
                            builder.switch_to_block(list_slow_path);
                            builder.seal_block(list_slow_path);
                        } else {
                            // Format doesn't support empty seq fast path, seal the blocks
                            builder.seal_block(list_fast_path_success);
                            builder.switch_to_block(list_fast_path_success);
                            builder.ins().trap(TrapCode::user(1).unwrap());
                            builder.switch_to_block(list_slow_path);
                            builder.seal_block(list_slow_path);
                        }

                        // Recursively compile the list deserializer for this Vec shape
                        let list_func_id =
                            compile_list_format_deserializer::<F>(module, field_shape, memo)?;
                        let list_func_ref = module.declare_func_in_func(list_func_id, builder.func);
                        let list_func_ptr =
                            func_addr_value(&mut builder, pointer_type, list_func_ref);

                        // Read current pos
                        let current_pos = builder.use_var(pos_var);

                        // Call list deserializer: (input_ptr, len, pos, field_ptr, scratch_ptr)
                        let call_result = builder.ins().call_indirect(
                            nested_call_sig_ref,
                            list_func_ptr,
                            &[input_ptr, len, current_pos, field_ptr, scratch_ptr],
                        );
                        let new_pos = builder.inst_results(call_result)[0];

                        // Check for error (new_pos < 0 means error)
                        // IMPORTANT: Don't jump to `error` block - that would overwrite scratch!
                        // The nested list deserializer already wrote error details to scratch.
                        // We need an "error passthrough" that just returns -1.
                        let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);

                        let list_ok = builder.create_block();
                        let list_error_passthrough = builder.create_block();
                        builder
                            .ins()
                            .brif(is_error, list_error_passthrough, &[], list_ok, &[]);

                        // Error passthrough: nested call failed, scratch already written, just return -1
                        builder.switch_to_block(list_error_passthrough);
                        let minus_one = builder.ins().iconst(pointer_type, -1);
                        builder.ins().return_(&[minus_one]);
                        builder.seal_block(list_error_passthrough);

                        // On success: update pos_var and continue
                        builder.switch_to_block(list_ok);
                        builder.def_var(pos_var, new_pos);

                        // Set required bit if this is a required field
                        if let Some(bit_index) = field_info.required_bit_index {
                            let bits = builder.use_var(required_bits_var);
                            let bit_mask = builder.ins().iconst(types::I64, 1i64 << bit_index);
                            let new_bits = builder.ins().bor(bits, bit_mask);
                            builder.def_var(required_bits_var, new_bits);
                        }

                        builder.ins().jump(after_value, &[]);
                        builder.seal_block(list_ok);
                    } else if let Def::Map(map_def) = &field_shape.def {
                        // Handle HashMap<String, V> fields
                        jit_debug!(
                            "[compile_struct]   Parsing HashMap field '{}'",
                            field_info.name
                        );

                        // Get field pointer (out_ptr + field offset)
                        let field_ptr = builder.ins().iadd_imm(out_ptr, field_info.offset as i64);

                        // Fast path: try to match empty object `{}` inline
                        let format = F::default();
                        let mut cursor = JitCursor {
                            input_ptr,
                            len,
                            pos: pos_var,
                            ptr_type: pointer_type,
                            scratch_ptr,
                        };

                        // Blocks for the fast path
                        let map_fast_path_success = builder.create_block();
                        let map_slow_path = builder.create_block();

                        if let Some((is_empty, empty_err)) =
                            format.emit_try_empty_map(&mut builder, &mut cursor)
                        {
                            // Check for error first
                            let no_err = builder.ins().icmp_imm(IntCC::Equal, empty_err, 0);
                            let check_empty = builder.create_block();
                            builder.def_var(err_var, empty_err);
                            builder.ins().brif(no_err, check_empty, &[], error, &[]);

                            // Check if it was empty
                            builder.switch_to_block(check_empty);
                            builder.seal_block(check_empty);
                            let was_empty = builder.ins().icmp_imm(IntCC::NotEqual, is_empty, 0);
                            builder.ins().brif(
                                was_empty,
                                map_fast_path_success,
                                &[],
                                map_slow_path,
                                &[],
                            );

                            // Fast path: initialize empty HashMap inline
                            builder.switch_to_block(map_fast_path_success);
                            builder.seal_block(map_fast_path_success);

                            // Get init_in_place_with_capacity function from vtable
                            let init_fn = map_def.vtable.init_in_place_with_capacity;
                            let init_fn_ptr = builder
                                .ins()
                                .iconst(pointer_type, init_fn as *const () as i64);
                            let zero_capacity = builder.ins().iconst(pointer_type, 0);

                            // Call jit_map_init_with_capacity(out, capacity=0, init_fn)
                            let sig_map_init = {
                                let mut s = make_c_sig(module);
                                s.params.push(AbiParam::new(pointer_type)); // out
                                s.params.push(AbiParam::new(pointer_type)); // capacity
                                s.params.push(AbiParam::new(pointer_type)); // init_fn
                                s
                            };
                            let sig_map_init_ref = builder.import_signature(sig_map_init);
                            let map_init_ptr = builder.ins().iconst(
                                pointer_type,
                                helpers::jit_map_init_with_capacity as *const u8 as i64,
                            );
                            builder.ins().call_indirect(
                                sig_map_init_ref,
                                map_init_ptr,
                                &[field_ptr, zero_capacity, init_fn_ptr],
                            );

                            // Set required bit if needed
                            if let Some(bit_index) = field_info.required_bit_index {
                                let bits = builder.use_var(required_bits_var);
                                let bit_mask = builder.ins().iconst(types::I64, 1i64 << bit_index);
                                let new_bits = builder.ins().bor(bits, bit_mask);
                                builder.def_var(required_bits_var, new_bits);
                            }

                            builder.ins().jump(after_value, &[]);

                            // Slow path: call full map deserializer
                            builder.switch_to_block(map_slow_path);
                            builder.seal_block(map_slow_path);
                        } else {
                            // Format doesn't support empty map fast path
                            builder.seal_block(map_fast_path_success);
                            builder.switch_to_block(map_fast_path_success);
                            builder.ins().trap(TrapCode::user(1).unwrap());
                            builder.switch_to_block(map_slow_path);
                            builder.seal_block(map_slow_path);
                        }

                        // Recursively compile the map deserializer for this HashMap shape
                        jit_debug!("Compiling map deserializer for field '{}'", field_info.name);
                        let map_func_id =
                            match compile_map_format_deserializer::<F>(module, field_shape, memo) {
                                Some(id) => id,
                                None => {
                                    jit_debug!(
                                        "compile_map_format_deserializer failed for field '{}'",
                                        field_info.name
                                    );
                                    return None;
                                }
                            };
                        let map_func_ref = module.declare_func_in_func(map_func_id, builder.func);
                        let map_func_ptr =
                            func_addr_value(&mut builder, pointer_type, map_func_ref);

                        // Read current pos
                        let current_pos = builder.use_var(pos_var);

                        // Call map deserializer: (input_ptr, len, pos, field_ptr, scratch_ptr)
                        let call_result = builder.ins().call_indirect(
                            nested_call_sig_ref,
                            map_func_ptr,
                            &[input_ptr, len, current_pos, field_ptr, scratch_ptr],
                        );
                        let new_pos = builder.inst_results(call_result)[0];

                        // Check for error (new_pos < 0 means error)
                        // Use error passthrough pattern like Vec fields
                        let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);

                        let map_ok = builder.create_block();
                        let map_error_passthrough = builder.create_block();
                        builder
                            .ins()
                            .brif(is_error, map_error_passthrough, &[], map_ok, &[]);

                        // Error passthrough: nested call failed, scratch already written, just return -1
                        builder.switch_to_block(map_error_passthrough);
                        let minus_one = builder.ins().iconst(pointer_type, -1);
                        builder.ins().return_(&[minus_one]);
                        builder.seal_block(map_error_passthrough);

                        // On success: update pos_var and continue
                        builder.switch_to_block(map_ok);
                        builder.def_var(pos_var, new_pos);

                        // Set required bit if this is a required field
                        if let Some(bit_index) = field_info.required_bit_index {
                            let bits = builder.use_var(required_bits_var);
                            let bit_mask = builder.ins().iconst(types::I64, 1i64 << bit_index);
                            let new_bits = builder.ins().bor(bits, bit_mask);
                            builder.def_var(required_bits_var, new_bits);
                        }

                        builder.ins().jump(after_value, &[]);
                        builder.seal_block(map_ok);
                    } else if let Type::User(UserType::Enum(enum_def)) = &field_shape.ty {
                        // Handle standalone (non-flattened) enum fields
                        // JSON shape: {"VariantName": {...payload...}}
                        jit_debug!(
                            "[compile_struct]   Parsing enum field '{}' ({} variants)",
                            field_info.name,
                            enum_def.variants.len()
                        );

                        let mut cursor = JitCursor {
                            input_ptr,
                            len,
                            pos: pos_var,
                            ptr_type: pointer_type,
                            scratch_ptr,
                        };

                        let format = F::default();

                        // Allocate stack slot for map state if needed (for the enum wrapper object)
                        let enum_state_ptr = if F::MAP_STATE_SIZE > 0 {
                            let align_shift = F::MAP_STATE_ALIGN.trailing_zeros() as u8;
                            let slot = builder.create_sized_stack_slot(StackSlotData::new(
                                StackSlotKind::ExplicitSlot,
                                F::MAP_STATE_SIZE,
                                align_shift,
                            ));
                            builder.ins().stack_addr(pointer_type, slot, 0)
                        } else {
                            builder.ins().iconst(pointer_type, 0)
                        };

                        // 1. emit_map_begin for the enum wrapper object
                        let err_code = format.emit_map_begin(
                            module,
                            &mut builder,
                            &mut cursor,
                            enum_state_ptr,
                        );
                        builder.def_var(err_var, err_code);

                        let map_begin_ok = builder.create_block();
                        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                        builder.ins().brif(is_ok, map_begin_ok, &[], error, &[]);

                        builder.switch_to_block(map_begin_ok);

                        // 2. emit_map_is_end to reject empty enum objects
                        let (is_end, err_code) = format.emit_map_is_end(
                            module,
                            &mut builder,
                            &mut cursor,
                            enum_state_ptr,
                        );
                        builder.def_var(err_var, err_code);

                        let check_is_end_err = builder.create_block();
                        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                        builder.ins().brif(is_ok, check_is_end_err, &[], error, &[]);

                        builder.switch_to_block(check_is_end_err);
                        let is_empty = builder.ins().icmp_imm(IntCC::NotEqual, is_end, 0);

                        let enum_not_empty = builder.create_block();
                        let empty_enum_error = builder.create_block();
                        builder
                            .ins()
                            .brif(is_empty, empty_enum_error, &[], enum_not_empty, &[]);

                        // Empty enum object error
                        builder.switch_to_block(empty_enum_error);
                        // Use static error message to avoid memory leak
                        const EMPTY_ENUM_ERROR: &str =
                            "empty enum object - expected exactly one variant key";
                        let error_msg_ptr = EMPTY_ENUM_ERROR.as_ptr();
                        let error_msg_len = EMPTY_ENUM_ERROR.len();

                        let msg_ptr_const =
                            builder.ins().iconst(pointer_type, error_msg_ptr as i64);
                        let msg_len_const =
                            builder.ins().iconst(pointer_type, error_msg_len as i64);

                        let sig_write_error = {
                            let mut s = make_c_sig(module);
                            s.params.push(AbiParam::new(pointer_type));
                            s.params.push(AbiParam::new(pointer_type));
                            s.params.push(AbiParam::new(pointer_type));
                            s
                        };
                        let write_error_sig_ref = builder.import_signature(sig_write_error.clone());
                        let write_error_ptr = builder.ins().iconst(
                            pointer_type,
                            helpers::jit_write_error_string as *const u8 as i64,
                        );
                        builder.ins().call_indirect(
                            write_error_sig_ref,
                            write_error_ptr,
                            &[scratch_ptr, msg_ptr_const, msg_len_const],
                        );

                        let minus_one = builder.ins().iconst(pointer_type, -1);
                        builder.ins().return_(&[minus_one]);
                        builder.seal_block(empty_enum_error);

                        // Continue parsing enum
                        builder.switch_to_block(enum_not_empty);

                        // 3. emit_map_read_key to get variant name
                        let (variant_key, err_code) = format.emit_map_read_key(
                            module,
                            &mut builder,
                            &mut cursor,
                            enum_state_ptr,
                        );
                        builder.def_var(err_var, err_code);

                        // Store variant key components in variables for later cleanup
                        let variant_key_ptr_var = builder.declare_var(pointer_type);
                        let variant_key_len_var = builder.declare_var(pointer_type);
                        let variant_key_cap_var = builder.declare_var(pointer_type);
                        let variant_key_owned_var = builder.declare_var(types::I8);

                        builder.def_var(variant_key_ptr_var, variant_key.ptr);
                        builder.def_var(variant_key_len_var, variant_key.len);
                        builder.def_var(variant_key_cap_var, variant_key.cap);
                        builder.def_var(variant_key_owned_var, variant_key.owned);

                        let read_key_ok = builder.create_block();
                        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                        builder.ins().brif(is_ok, read_key_ok, &[], error, &[]);

                        builder.switch_to_block(read_key_ok);

                        // 4. Dispatch variant name
                        // Create match blocks for each variant, plus unknown variant block
                        let mut variant_match_blocks = Vec::new();
                        for _ in enum_def.variants {
                            variant_match_blocks.push(builder.create_block());
                        }
                        let unknown_variant_block = builder.create_block();

                        // Linear dispatch for variants (enum variants typically < 10)
                        let variant_dispatch = builder.create_block();
                        builder.ins().jump(variant_dispatch, &[]);

                        builder.switch_to_block(variant_dispatch);
                        let mut current_block = variant_dispatch;

                        for (variant_idx, variant) in enum_def.variants.iter().enumerate() {
                            if variant_idx > 0 {
                                builder.switch_to_block(current_block);
                            }

                            let variant_name = variant.name;
                            let variant_name_len = variant_name.len();

                            // Check length first
                            let len_matches = builder.ins().icmp_imm(
                                IntCC::Equal,
                                variant_key.len,
                                variant_name_len as i64,
                            );

                            let check_content = builder.create_block();
                            let next_check = if variant_idx + 1 < enum_def.variants.len() {
                                builder.create_block()
                            } else {
                                unknown_variant_block
                            };

                            builder
                                .ins()
                                .brif(len_matches, check_content, &[], next_check, &[]);
                            if variant_idx > 0 {
                                builder.seal_block(current_block);
                            }

                            // Byte-by-byte comparison
                            builder.switch_to_block(check_content);
                            let mut all_match = builder.ins().iconst(types::I8, 1);

                            for (byte_idx, &byte) in variant_name.as_bytes().iter().enumerate() {
                                let offset = builder.ins().iconst(pointer_type, byte_idx as i64);
                                let char_ptr = builder.ins().iadd(variant_key.ptr, offset);
                                let char_val =
                                    builder
                                        .ins()
                                        .load(types::I8, MemFlags::trusted(), char_ptr, 0);
                                let expected = builder.ins().iconst(types::I8, byte as i64);
                                let byte_matches =
                                    builder.ins().icmp(IntCC::Equal, char_val, expected);
                                let one = builder.ins().iconst(types::I8, 1);
                                let zero = builder.ins().iconst(types::I8, 0);
                                let byte_match_i8 = builder.ins().select(byte_matches, one, zero);
                                all_match = builder.ins().band(all_match, byte_match_i8);
                            }

                            let all_match_bool =
                                builder.ins().icmp_imm(IntCC::NotEqual, all_match, 0);
                            builder.ins().brif(
                                all_match_bool,
                                variant_match_blocks[variant_idx],
                                &[],
                                next_check,
                                &[],
                            );
                            builder.seal_block(check_content);

                            current_block = next_check;
                        }

                        builder.seal_block(variant_dispatch);
                        if enum_def.variants.len() > 1 {
                            builder.seal_block(current_block);
                        }

                        // Handle unknown variant
                        builder.switch_to_block(unknown_variant_block);
                        // Use static error message to avoid memory leak
                        const UNKNOWN_VARIANT_ERROR: &str = "unknown variant for enum field";
                        let error_msg_ptr = UNKNOWN_VARIANT_ERROR.as_ptr();
                        let error_msg_len = UNKNOWN_VARIANT_ERROR.len();

                        let msg_ptr_const =
                            builder.ins().iconst(pointer_type, error_msg_ptr as i64);
                        let msg_len_const =
                            builder.ins().iconst(pointer_type, error_msg_len as i64);

                        let write_error_sig_ref = builder.import_signature(sig_write_error.clone());
                        let write_error_ptr = builder.ins().iconst(
                            pointer_type,
                            helpers::jit_write_error_string as *const u8 as i64,
                        );
                        builder.ins().call_indirect(
                            write_error_sig_ref,
                            write_error_ptr,
                            &[scratch_ptr, msg_ptr_const, msg_len_const],
                        );

                        let minus_one = builder.ins().iconst(pointer_type, -1);
                        builder.ins().return_(&[minus_one]);
                        // Seal unknown_variant_block (for single variant case, multi-variant sealed above)
                        if enum_def.variants.len() == 1 {
                            builder.seal_block(unknown_variant_block);
                        }

                        // 5. Implement variant match blocks
                        // Block to jump to after variant parsing
                        let enum_parsed = builder.create_block();

                        for (variant_idx, variant) in enum_def.variants.iter().enumerate() {
                            builder.switch_to_block(variant_match_blocks[variant_idx]);

                            // Consume kv_sep before payload
                            let mut cursor = JitCursor {
                                input_ptr,
                                len,
                                pos: pos_var,
                                ptr_type: pointer_type,
                                scratch_ptr,
                            };
                            let err_code = format.emit_map_kv_sep(
                                module,
                                &mut builder,
                                &mut cursor,
                                enum_state_ptr,
                            );
                            builder.def_var(err_var, err_code);

                            let kv_sep_ok_variant = builder.create_block();
                            let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                            builder
                                .ins()
                                .brif(is_ok, kv_sep_ok_variant, &[], error, &[]);

                            builder.switch_to_block(kv_sep_ok_variant);

                            // Parse payload struct
                            let payload_shape = variant.data.fields[0].shape();
                            let payload_func_id = compile_struct_format_deserializer::<F>(
                                module,
                                payload_shape,
                                memo,
                            )?;
                            let payload_func_ref =
                                module.declare_func_in_func(payload_func_id, builder.func);
                            let payload_func_ptr =
                                func_addr_value(&mut builder, pointer_type, payload_func_ref);

                            // Allocate stack slot for payload
                            let payload_layout = payload_shape.layout.sized_layout().ok()?;
                            let payload_slot = builder.create_sized_stack_slot(StackSlotData::new(
                                StackSlotKind::ExplicitSlot,
                                payload_layout.size() as u32,
                                payload_layout.align() as u8,
                            ));
                            let payload_ptr =
                                builder.ins().stack_addr(pointer_type, payload_slot, 0);

                            // Call payload deserializer
                            let current_pos = builder.use_var(pos_var);
                            let call_result = builder.ins().call_indirect(
                                nested_call_sig_ref,
                                payload_func_ptr,
                                &[input_ptr, len, current_pos, payload_ptr, scratch_ptr],
                            );
                            let new_pos = builder.inst_results(call_result)[0];

                            // Check for error
                            let payload_ok = builder.create_block();
                            let is_error =
                                builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);

                            let error_passthrough = builder.create_block();
                            builder
                                .ins()
                                .brif(is_error, error_passthrough, &[], payload_ok, &[]);

                            builder.switch_to_block(error_passthrough);
                            let minus_one = builder.ins().iconst(pointer_type, -1);
                            builder.ins().return_(&[minus_one]);
                            builder.seal_block(error_passthrough);

                            builder.switch_to_block(payload_ok);
                            builder.def_var(pos_var, new_pos);

                            // Get enum field pointer
                            let enum_ptr =
                                builder.ins().iadd_imm(out_ptr, field_info.offset as i64);

                            // Write discriminant
                            let discriminant = variant.discriminant.unwrap_or(0);
                            let discrim_val = builder.ins().iconst(types::I64, discriminant);
                            builder
                                .ins()
                                .store(MemFlags::trusted(), discrim_val, enum_ptr, 0);

                            // Copy payload
                            let payload_offset_in_enum = variant.data.fields[0].offset;
                            let enum_payload_ptr = builder
                                .ins()
                                .iadd_imm(enum_ptr, payload_offset_in_enum as i64);

                            let sig_memcpy = {
                                let mut s = make_c_sig(module);
                                s.params.push(AbiParam::new(pointer_type));
                                s.params.push(AbiParam::new(pointer_type));
                                s.params.push(AbiParam::new(pointer_type));
                                s
                            };
                            let memcpy_sig_ref = builder.import_signature(sig_memcpy);
                            let memcpy_ptr = builder
                                .ins()
                                .iconst(pointer_type, helpers::jit_memcpy as *const u8 as i64);
                            let payload_size = builder
                                .ins()
                                .iconst(pointer_type, payload_layout.size() as i64);
                            builder.ins().call_indirect(
                                memcpy_sig_ref,
                                memcpy_ptr,
                                &[enum_payload_ptr, payload_ptr, payload_size],
                            );

                            // Jump to enum_parsed to check for end-of-enum-object
                            builder.ins().jump(enum_parsed, &[]);
                            builder.seal_block(kv_sep_ok_variant);
                            builder.seal_block(payload_ok);
                            builder.seal_block(variant_match_blocks[variant_idx]);
                        }

                        // 6. After parsing variant payload, verify end of enum object
                        builder.switch_to_block(enum_parsed);

                        // emit_map_next to check for closing } or extra keys
                        let mut cursor = JitCursor {
                            input_ptr,
                            len,
                            pos: pos_var,
                            ptr_type: pointer_type,
                            scratch_ptr,
                        };
                        let err_code =
                            format.emit_map_next(module, &mut builder, &mut cursor, enum_state_ptr);
                        builder.def_var(err_var, err_code);

                        let map_next_ok = builder.create_block();
                        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                        builder.ins().brif(is_ok, map_next_ok, &[], error, &[]);

                        builder.switch_to_block(map_next_ok);

                        // emit_map_is_end to verify we're at the closing }
                        let (is_end, err_code) = format.emit_map_is_end(
                            module,
                            &mut builder,
                            &mut cursor,
                            enum_state_ptr,
                        );
                        builder.def_var(err_var, err_code);

                        let check_end_ok = builder.create_block();
                        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                        builder.ins().brif(is_ok, check_end_ok, &[], error, &[]);

                        builder.switch_to_block(check_end_ok);
                        let at_end = builder.ins().icmp_imm(IntCC::NotEqual, is_end, 0);

                        let enum_complete = builder.create_block();
                        let extra_keys_error = builder.create_block();
                        builder
                            .ins()
                            .brif(at_end, enum_complete, &[], extra_keys_error, &[]);

                        // Extra keys in enum object error
                        builder.switch_to_block(extra_keys_error);
                        // Use static error message to avoid memory leak
                        const EXTRA_KEYS_ERROR: &str =
                            "enum field has extra keys - expected exactly one variant";
                        let error_msg_ptr = EXTRA_KEYS_ERROR.as_ptr();
                        let error_msg_len = EXTRA_KEYS_ERROR.len();

                        let msg_ptr_const =
                            builder.ins().iconst(pointer_type, error_msg_ptr as i64);
                        let msg_len_const =
                            builder.ins().iconst(pointer_type, error_msg_len as i64);

                        let write_error_sig_ref = builder.import_signature(sig_write_error.clone());
                        let write_error_ptr = builder.ins().iconst(
                            pointer_type,
                            helpers::jit_write_error_string as *const u8 as i64,
                        );
                        builder.ins().call_indirect(
                            write_error_sig_ref,
                            write_error_ptr,
                            &[scratch_ptr, msg_ptr_const, msg_len_const],
                        );

                        let minus_one = builder.ins().iconst(pointer_type, -1);
                        builder.ins().return_(&[minus_one]);
                        builder.seal_block(extra_keys_error);

                        // Enum successfully parsed
                        builder.switch_to_block(enum_complete);

                        // Clean up owned variant key if needed
                        let key_owned = builder.use_var(variant_key_owned_var);
                        let needs_drop = builder.ins().icmp_imm(IntCC::NotEqual, key_owned, 0);
                        let drop_variant_key = builder.create_block();
                        let after_drop_variant = builder.create_block();
                        builder.ins().brif(
                            needs_drop,
                            drop_variant_key,
                            &[],
                            after_drop_variant,
                            &[],
                        );

                        builder.switch_to_block(drop_variant_key);
                        let key_ptr = builder.use_var(variant_key_ptr_var);
                        let key_len = builder.use_var(variant_key_len_var);
                        let key_cap = builder.use_var(variant_key_cap_var);

                        let sig_drop = {
                            let mut s = make_c_sig(module);
                            s.params.push(AbiParam::new(pointer_type));
                            s.params.push(AbiParam::new(pointer_type));
                            s.params.push(AbiParam::new(pointer_type));
                            s
                        };
                        let drop_sig_ref = builder.import_signature(sig_drop);
                        let drop_ptr = builder.ins().iconst(
                            pointer_type,
                            helpers::jit_drop_owned_string as *const u8 as i64,
                        );
                        builder.ins().call_indirect(
                            drop_sig_ref,
                            drop_ptr,
                            &[key_ptr, key_len, key_cap],
                        );
                        builder.ins().jump(after_drop_variant, &[]);
                        builder.seal_block(drop_variant_key);

                        builder.switch_to_block(after_drop_variant);

                        // Set required bit if this is a required field
                        if let Some(bit_index) = field_info.required_bit_index {
                            let bits = builder.use_var(required_bits_var);
                            let bit_mask = builder.ins().iconst(types::I64, 1i64 << bit_index);
                            let new_bits = builder.ins().bor(bits, bit_mask);
                            builder.def_var(required_bits_var, new_bits);
                        }

                        builder.ins().jump(after_value, &[]);
                        builder.seal_block(map_begin_ok);
                        builder.seal_block(check_is_end_err);
                        builder.seal_block(enum_not_empty);
                        builder.seal_block(read_key_ok);
                        builder.seal_block(enum_parsed);
                        builder.seal_block(map_next_ok);
                        builder.seal_block(check_end_ok);
                        builder.seal_block(enum_complete);
                        builder.seal_block(after_drop_variant);
                    } else {
                        // Unsupported field type (Set, etc.)
                        jit_debug!(
                            "[compile_struct] Field {} has unsupported type (not scalar/Option/struct/Vec/Map/Enum)",
                            field_info.name
                        );
                        jit_debug!(
                            "Field '{}' has unsupported type: {:?}",
                            field_info.name,
                            field_info.shape.def
                        );
                        return None;
                    }
                }
                DispatchTarget::FlattenEnumVariant(variant_idx) => {
                    // Flattened enum variant parsing
                    let variant_info = &flatten_variants[*variant_idx];

                    jit_debug!(
                        "Processing flattened variant '{}' for enum at offset {} (seen_bit={})",
                        variant_info.variant_name,
                        variant_info.enum_field_offset,
                        variant_info.enum_seen_bit_index
                    );

                    // 0. Check if this enum has already been set (duplicate variant key error)
                    let enum_bit_mask = builder
                        .ins()
                        .iconst(types::I64, 1i64 << variant_info.enum_seen_bit_index);
                    let current_seen_bits = builder.use_var(enum_seen_bits_var);
                    let already_seen = builder.ins().band(current_seen_bits, enum_bit_mask);
                    let is_duplicate = builder.ins().icmp_imm(IntCC::NotEqual, already_seen, 0);

                    let enum_not_seen = builder.create_block();
                    let duplicate_variant_error = builder.create_block();

                    builder.ins().brif(
                        is_duplicate,
                        duplicate_variant_error,
                        &[],
                        enum_not_seen,
                        &[],
                    );

                    // Duplicate variant key: write error to scratch and return -1
                    builder.switch_to_block(duplicate_variant_error);
                    // Use static error message to avoid memory leak
                    const DUPLICATE_VARIANT_ERROR: &str = "duplicate variant key for enum field";
                    let error_msg_ptr = DUPLICATE_VARIANT_ERROR.as_ptr();
                    let error_msg_len = DUPLICATE_VARIANT_ERROR.len();

                    let msg_ptr_const = builder.ins().iconst(pointer_type, error_msg_ptr as i64);
                    let msg_len_const = builder.ins().iconst(pointer_type, error_msg_len as i64);

                    // Call jit_write_error_string to write error to scratch buffer
                    let sig_write_error = {
                        let mut s = make_c_sig(module);
                        s.params.push(AbiParam::new(pointer_type)); // scratch_ptr
                        s.params.push(AbiParam::new(pointer_type)); // msg_ptr
                        s.params.push(AbiParam::new(pointer_type)); // msg_len
                        s
                    };
                    let write_error_sig_ref = builder.import_signature(sig_write_error);
                    let write_error_ptr = builder.ins().iconst(
                        pointer_type,
                        helpers::jit_write_error_string as *const u8 as i64,
                    );
                    builder.ins().call_indirect(
                        write_error_sig_ref,
                        write_error_ptr,
                        &[scratch_ptr, msg_ptr_const, msg_len_const],
                    );

                    let minus_one = builder.ins().iconst(pointer_type, -1);
                    builder.ins().return_(&[minus_one]);
                    builder.seal_block(duplicate_variant_error);

                    // Continue normal parsing
                    builder.switch_to_block(enum_not_seen);
                    builder.seal_block(enum_not_seen);

                    // kv_sep already consumed in match_blocks, proceed to payload parsing

                    // Compile nested struct deserializer for payload
                    let payload_func_id = compile_struct_format_deserializer::<F>(
                        module,
                        variant_info.payload_shape,
                        memo,
                    )?;
                    let payload_func_ref =
                        module.declare_func_in_func(payload_func_id, builder.func);
                    let payload_func_ptr =
                        func_addr_value(&mut builder, pointer_type, payload_func_ref);

                    // 3. Allocate stack slot for payload struct
                    let payload_layout = variant_info.payload_shape.layout.sized_layout().ok()?;
                    let payload_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        payload_layout.size() as u32,
                        payload_layout.align() as u8,
                    ));
                    let payload_ptr = builder.ins().stack_addr(pointer_type, payload_slot, 0);

                    // 4. Call payload deserializer
                    let current_pos = builder.use_var(pos_var);
                    let call_result = builder.ins().call_indirect(
                        nested_call_sig_ref,
                        payload_func_ptr,
                        &[input_ptr, len, current_pos, payload_ptr, scratch_ptr],
                    );
                    let new_pos = builder.inst_results(call_result)[0];

                    // 5. Check for error (passthrough pattern)
                    let payload_ok = builder.create_block();
                    let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);

                    let error_passthrough = builder.create_block();
                    builder
                        .ins()
                        .brif(is_error, error_passthrough, &[], payload_ok, &[]);

                    // Error passthrough: nested call failed, scratch already written, just return -1
                    builder.switch_to_block(error_passthrough);
                    let minus_one = builder.ins().iconst(pointer_type, -1);
                    builder.ins().return_(&[minus_one]);
                    builder.seal_block(error_passthrough);

                    builder.switch_to_block(payload_ok);
                    builder.def_var(pos_var, new_pos);

                    // 6. Initialize enum at field offset
                    // For #[repr(C)] enums: discriminant at offset 0, payload after discriminant
                    let enum_ptr = builder
                        .ins()
                        .iadd_imm(out_ptr, variant_info.enum_field_offset as i64);

                    // Write discriminant (use i64 for #[repr(C)] which uses isize by default)
                    let discrim_val = builder
                        .ins()
                        .iconst(types::I64, variant_info.discriminant as i64);
                    builder
                        .ins()
                        .store(MemFlags::trusted(), discrim_val, enum_ptr, 0);

                    // Copy payload from stack to enum using actual payload offset
                    // The offset accounts for discriminant size/alignment per the shape metadata
                    let enum_payload_ptr = builder
                        .ins()
                        .iadd_imm(enum_ptr, variant_info.payload_offset_in_enum as i64);

                    // Use memcpy to copy payload
                    let sig_memcpy = {
                        let mut s = make_c_sig(module);
                        s.params.push(AbiParam::new(pointer_type)); // dest
                        s.params.push(AbiParam::new(pointer_type)); // src
                        s.params.push(AbiParam::new(pointer_type)); // len
                        s
                    };
                    let memcpy_sig_ref = builder.import_signature(sig_memcpy);
                    let memcpy_ptr = builder
                        .ins()
                        .iconst(pointer_type, helpers::jit_memcpy as *const u8 as i64);
                    let payload_size = builder
                        .ins()
                        .iconst(pointer_type, payload_layout.size() as i64);
                    builder.ins().call_indirect(
                        memcpy_sig_ref,
                        memcpy_ptr,
                        &[enum_payload_ptr, payload_ptr, payload_size],
                    );

                    // 7. Mark this enum as seen (prevent duplicate variant keys)
                    let current_seen = builder.use_var(enum_seen_bits_var);
                    let new_seen = builder.ins().bor(current_seen, enum_bit_mask);
                    builder.def_var(enum_seen_bits_var, new_seen);

                    // 8. Jump to after_value
                    builder.ins().jump(after_value, &[]);
                    builder.seal_block(payload_ok);
                }
            }

            builder.seal_block(parse_value_blocks[i]);
        }

        // after_value: advance to next entry
        builder.switch_to_block(after_value);

        let mut cursor = JitCursor {
            input_ptr,
            len,
            pos: pos_var,
            ptr_type: pointer_type,
            scratch_ptr,
        };

        let format = F::default();
        let err_code = format.emit_map_next(module, &mut builder, &mut cursor, state_ptr);
        builder.def_var(err_var, err_code);

        builder.ins().jump(check_map_next_err, &[]);
        builder.seal_block(after_value);

        // check_map_next_err
        builder.switch_to_block(check_map_next_err);
        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
        builder.ins().brif(is_ok, loop_check_end, &[], error, &[]);
        builder.seal_block(check_map_next_err);

        // Now seal loop_check_end and error (all predecessors known)
        builder.seal_block(loop_check_end);
        builder.seal_block(error);

        builder.finalize();
    }

    // Debug: print the generated IR
    if std::env::var("FACET_JIT_TRACE").is_ok() {
        eprintln!("[compile_struct] Generated Cranelift IR:");
        eprintln!("{}", ctx.func.display());
    }

    if let Err(e) = module.define_function(func_id, &mut ctx) {
        jit_debug!("[compile_struct] define_function failed: {:?}", e);
        jit_debug!("define_function failed: {:?}", e);
        return None;
    }

    jit_debug!("[compile_struct] SUCCESS - function compiled");
    jit_debug!("compile_struct_format_deserializer SUCCESS");
    Some(func_id)
}
