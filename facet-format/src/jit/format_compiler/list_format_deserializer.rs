use cranelift::codegen::ir::AbiParam;
use cranelift::prelude::*;
use cranelift_jit::JITModule;
use cranelift_module::{FuncId, Linkage, Module};

use facet_core::{Def, ScalarType, Shape};

use super::super::format::{
    JIT_SCRATCH_ERROR_CODE_OFFSET, JIT_SCRATCH_ERROR_POS_OFFSET,
    JIT_SCRATCH_OUTPUT_INITIALIZED_OFFSET, JitCursor, JitFormat, make_c_sig,
};
use super::super::helpers;
use super::super::jit_debug;
use super::{
    FormatListElementKind, ShapeMemo, compile_map_format_deserializer,
    compile_struct_format_deserializer, compile_struct_positional_deserializer, func_addr_value,
    tier2_call_sig,
};

/// Compile a Tier-2 list deserializer.
///
/// Generates code that:
/// 1. Calls format helper for seq_begin
/// 2. Loops: check seq_is_end, parse element, push, seq_next
/// 3. Returns new position on success
///
/// This implementation directly calls format-specific helper functions
/// via symbol names provided by `JitFormat::helper_*()` methods.
/// The helper functions are registered via `JitFormat::register_helpers`.
pub(crate) fn compile_list_format_deserializer<F: JitFormat>(
    module: &mut JITModule,
    shape: &'static Shape,
    memo: &mut ShapeMemo,
) -> Option<FuncId> {
    // Check memo first - return cached FuncId if already compiled
    let shape_ptr = shape as *const Shape;
    if let Some(&func_id) = memo.get(&shape_ptr) {
        jit_debug!(
            "compile_list_format_deserializer: using memoized FuncId for shape {:p}",
            shape
        );
        return Some(func_id);
    }

    let Def::List(list_def) = &shape.def else {
        jit_debug!("[compile_list] Not a list");
        return None;
    };

    let elem_shape = list_def.t;
    let elem_kind = match FormatListElementKind::from_shape(elem_shape) {
        Some(k) => k,
        None => {
            jit_debug!("[compile_list] Element type not supported");
            return None;
        }
    };

    // Get Vec vtable functions
    let init_fn = match list_def.init_in_place_with_capacity() {
        Some(f) => f,
        None => {
            jit_debug!("[compile_list] No init_in_place_with_capacity");
            return None;
        }
    };
    let push_fn = match list_def.push() {
        Some(f) => f,
        None => {
            jit_debug!("[compile_list] No push fn");
            return None;
        }
    };

    let pointer_type = module.target_config().pointer_type();

    // Function signature: fn(input_ptr, len, pos, out, scratch) -> isize
    // IMPORTANT: Use C ABI calling convention to match extern "C" callers
    let sig = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // input_ptr: *const u8
        s.params.push(AbiParam::new(pointer_type)); // len: usize
        s.params.push(AbiParam::new(pointer_type)); // pos: usize
        s.params.push(AbiParam::new(pointer_type)); // out: *mut u8
        s.params.push(AbiParam::new(pointer_type)); // scratch: *mut JitScratch
        s.returns.push(AbiParam::new(pointer_type)); // isize (new pos or error)
        s
    };

    // Vec helper signatures
    // IMPORTANT: Use C ABI calling convention to match extern "C" helpers
    let sig_vec_init = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // out
        s.params.push(AbiParam::new(pointer_type)); // capacity
        s.params.push(AbiParam::new(pointer_type)); // init_fn
        s
    };

    // Direct push signature: fn(vec_ptr: PtrMut, elem_ptr: PtrMut) -> ()
    // This is the actual ListPushFn signature - we call it directly via call_indirect
    // NOTE: PtrMut is a 16-byte struct (TaggedPtr + metadata), so each PtrMut becomes
    // TWO pointer-sized arguments in the C ABI. For thin pointers, metadata is 0.
    let sig_direct_push = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // vec_ptr.ptr (TaggedPtr)
        s.params.push(AbiParam::new(pointer_type)); // vec_ptr.metadata
        s.params.push(AbiParam::new(pointer_type)); // elem_ptr.ptr (TaggedPtr)
        s.params.push(AbiParam::new(pointer_type)); // elem_ptr.metadata
        s
    };

    // Direct-fill helper signatures
    // jit_vec_set_len(vec_ptr, len, set_len_fn)
    let sig_vec_set_len = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // vec_ptr
        s.params.push(AbiParam::new(pointer_type)); // len
        s.params.push(AbiParam::new(pointer_type)); // set_len_fn
        s
    };
    // jit_vec_as_mut_ptr_typed(vec_ptr, as_mut_ptr_typed_fn) -> *mut u8
    let sig_vec_as_mut_ptr_typed = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // vec_ptr
        s.params.push(AbiParam::new(pointer_type)); // as_mut_ptr_typed_fn
        s.returns.push(AbiParam::new(pointer_type)); // *mut u8
        s
    };
    // jit_vec_reserve(vec_ptr, additional, reserve_fn)
    let sig_vec_reserve = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // vec_ptr
        s.params.push(AbiParam::new(pointer_type)); // additional
        s.params.push(AbiParam::new(pointer_type)); // reserve_fn
        s
    };
    // jit_vec_capacity(vec_ptr, capacity_fn) -> usize
    let sig_vec_capacity = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // vec_ptr
        s.params.push(AbiParam::new(pointer_type)); // capacity_fn
        s.returns.push(AbiParam::new(pointer_type)); // usize
        s
    };

    // Element size and alignment from actual element type, not from elem_kind
    // (elem_kind groups types: I64 includes i8/i16/i32/i64, U64 includes u16/u32/u64)
    let elem_layout = elem_shape.layout.sized_layout().ok()?;
    let elem_size = elem_layout.size() as u32;
    let elem_align_shift = elem_layout.align().trailing_zeros() as u8;

    // Get direct-fill functions from list_def (optional - may be None)
    let set_len_fn = list_def.set_len();
    let as_mut_ptr_typed_fn = list_def.as_mut_ptr_typed();
    let reserve_fn = list_def.reserve();
    let capacity_fn = list_def.capacity();

    // Check if element is a simple scalar type (can be written directly to buffer)
    let is_direct_fill_scalar = matches!(
        elem_kind,
        FormatListElementKind::Bool
            | FormatListElementKind::U8
            | FormatListElementKind::I64
            | FormatListElementKind::U64
    );

    // Check if element is a struct type (can also be written directly to buffer)
    let is_direct_fill_struct = matches!(elem_kind, FormatListElementKind::Struct(_));

    // Direct-fill requires:
    // 1. Vec operations (set_len, as_mut_ptr_typed)
    // 2. Scalar element type
    // 3. Format provides accurate count (not delimiter-based like JSON)
    let use_direct_fill = set_len_fn.is_some()
        && as_mut_ptr_typed_fn.is_some()
        && F::PROVIDES_SEQ_COUNT
        && is_direct_fill_scalar;

    // Buffered direct-fill: for delimiter-based formats (JSON) where count is unknown.
    // We start with small capacity, grow as needed, and set_len at the end.
    // Requires: set_len, as_mut_ptr_typed, reserve, capacity + scalar elements
    let use_buffered_direct_fill = !use_direct_fill
        && set_len_fn.is_some()
        && as_mut_ptr_typed_fn.is_some()
        && reserve_fn.is_some()
        && capacity_fn.is_some()
        && is_direct_fill_scalar;

    // Buffered direct-fill for structs: same as above but for struct element types.
    // Instead of deserializing into a stack slot and then pushing, we deserialize
    // directly into the Vec's buffer, avoiding a copy per element.
    let use_buffered_direct_fill_struct = !use_direct_fill
        && !use_buffered_direct_fill
        && set_len_fn.is_some()
        && as_mut_ptr_typed_fn.is_some()
        && reserve_fn.is_some()
        && capacity_fn.is_some()
        && is_direct_fill_struct;

    // No need to declare push helper - we call push_fn directly via call_indirect
    // No format helper functions need to be declared

    // Declare our function with unique name based on shape address (avoids collisions)
    let func_name = format!("jit_deserialize_list_{:x}", shape as *const _ as usize);
    let func_id = match module.declare_function(&func_name, Linkage::Local, &sig) {
        Ok(id) => id,
        Err(_e) => {
            jit_debug!("[compile_list] declare {} failed: {:?}", func_name, _e);
            return None;
        }
    };

    // Insert into memo immediately after declaration (before IR build) to avoid recursion/cycles
    memo.insert(shape_ptr, func_id);
    jit_debug!(
        "compile_list_format_deserializer: memoized FuncId for shape {:p}",
        shape
    );

    let mut ctx = module.make_context();
    ctx.func.signature = sig;

    let mut builder_ctx = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);
        let nested_call_sig_ref = builder.import_signature(tier2_call_sig(module, pointer_type));

        let sig_vec_init_ref = builder.import_signature(sig_vec_init);
        let sig_vec_set_len_ref = builder.import_signature(sig_vec_set_len);
        let sig_vec_as_mut_ptr_typed_ref = builder.import_signature(sig_vec_as_mut_ptr_typed);
        let sig_vec_reserve_ref = builder.import_signature(sig_vec_reserve);
        let sig_vec_capacity_ref = builder.import_signature(sig_vec_capacity);
        // Import signature for direct push call (call_indirect)
        let sig_direct_push_ref = builder.import_signature(sig_direct_push);

        // Create blocks
        let entry = builder.create_block();
        let seq_begin = builder.create_block();
        let check_seq_begin_err = builder.create_block();
        let init_vec = builder.create_block();
        // Push-based path (for delimiter formats like JSON, or when count==0)
        let loop_check_end = builder.create_block();
        let check_is_end_err = builder.create_block();
        let check_is_end_value = builder.create_block();
        let parse_element = builder.create_block();
        let check_parse_err = builder.create_block();
        let push_element = builder.create_block();
        let seq_next = builder.create_block();
        let check_seq_next_err = builder.create_block();
        // Direct-fill path (for counted formats like postcard when count>0)
        let df_setup = builder.create_block();
        // Bulk copy path (for Vec<u8> when format supports it)
        let df_bulk_copy = builder.create_block();
        let df_bulk_copy_check_err = builder.create_block();
        // Element-by-element loop path
        let df_loop_check = builder.create_block();
        let df_parse = builder.create_block();
        let df_check_parse_err = builder.create_block();
        let df_store = builder.create_block();
        let df_finalize = builder.create_block();
        // Buffered direct-fill path (for delimiter formats with reserve/capacity)
        let bf_setup = builder.create_block();
        let bf_loop_check = builder.create_block();
        let bf_check_is_end_err = builder.create_block();
        let bf_check_is_end_value = builder.create_block();
        let bf_check_capacity = builder.create_block();
        let bf_grow = builder.create_block();
        let bf_parse = builder.create_block();
        let bf_check_parse_err = builder.create_block();
        let bf_store = builder.create_block();
        let bf_seq_next = builder.create_block();
        let bf_check_seq_next_err = builder.create_block();
        let bf_finalize = builder.create_block();
        // Buffered direct-fill path for structs (similar to bf_* but calls struct deserializer)
        let bfs_setup = builder.create_block();
        let bfs_loop_check = builder.create_block();
        let bfs_check_is_end_err = builder.create_block();
        let bfs_check_is_end_value = builder.create_block();
        let bfs_check_capacity = builder.create_block();
        let bfs_grow = builder.create_block();
        let bfs_parse = builder.create_block();
        let bfs_check_parse_err = builder.create_block();
        let bfs_seq_next = builder.create_block();
        let bfs_check_seq_next_err = builder.create_block();
        let bfs_finalize = builder.create_block();
        let success = builder.create_block();
        let error = builder.create_block();
        let nested_error_passthrough = builder.create_block();

        // Entry block: setup parameters
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);

        let input_ptr = builder.block_params(entry)[0];
        let len = builder.block_params(entry)[1];
        let pos_param = builder.block_params(entry)[2];
        let out_ptr = builder.block_params(entry)[3];
        let scratch_ptr = builder.block_params(entry)[4];

        // Create position variable (mutable)
        let pos_var = builder.declare_var(pointer_type);
        builder.def_var(pos_var, pos_param);

        // Variable to hold parsed value (type depends on element kind)
        let parsed_value_type = match elem_kind {
            FormatListElementKind::Bool | FormatListElementKind::U8 => types::I8,
            FormatListElementKind::I64 | FormatListElementKind::U64 => types::I64,
            FormatListElementKind::F64 => types::F64,
            FormatListElementKind::String => types::I64, // placeholder, not used for String
            FormatListElementKind::Struct(_) => types::I64, // placeholder, not used for Struct
            FormatListElementKind::List(_) => types::I64, // placeholder, not used for List
            FormatListElementKind::Map(_) => types::I64, // placeholder, not used for Map
        };
        let parsed_value_var = builder.declare_var(parsed_value_type);
        let zero_val = match elem_kind {
            FormatListElementKind::Bool | FormatListElementKind::U8 => {
                builder.ins().iconst(types::I8, 0)
            }
            FormatListElementKind::I64 | FormatListElementKind::U64 => {
                builder.ins().iconst(types::I64, 0)
            }
            FormatListElementKind::F64 => builder.ins().f64const(0.0),
            FormatListElementKind::String => builder.ins().iconst(types::I64, 0),
            FormatListElementKind::Struct(_) => builder.ins().iconst(types::I64, 0),
            FormatListElementKind::List(_) => builder.ins().iconst(types::I64, 0),
            FormatListElementKind::Map(_) => builder.ins().iconst(types::I64, 0),
        };
        builder.def_var(parsed_value_var, zero_val);

        // Variable for error code (used across blocks)
        let err_var = builder.declare_var(types::I32);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        builder.def_var(err_var, zero_i32);

        // Variable for is_end flag (used across blocks)
        let is_end_var = builder.declare_var(pointer_type);
        let zero_ptr = builder.ins().iconst(pointer_type, 0);
        builder.def_var(is_end_var, zero_ptr);

        // Store push_fn_ptr in a Variable since it's used in the loop body
        // (Cranelift SSA requires Variable for values used across loop boundaries)
        let push_fn_var = builder.declare_var(pointer_type);
        let push_fn_val = builder
            .ins()
            .iconst(pointer_type, push_fn as *const () as i64);
        builder.def_var(push_fn_var, push_fn_val);

        // Constants (used in entry or blocks directly reachable from entry)
        let init_fn_ptr = builder
            .ins()
            .iconst(pointer_type, init_fn as *const () as i64);
        let zero_cap = builder.ins().iconst(pointer_type, 0);

        // Allocate stack slot for sequence state if the format needs it
        let state_ptr = if F::SEQ_STATE_SIZE > 0 {
            // align_shift is log2(alignment), e.g. for 8-byte alignment: log2(8) = 3
            let align_shift = F::SEQ_STATE_ALIGN.trailing_zeros() as u8;
            let slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                F::SEQ_STATE_SIZE,
                align_shift,
            ));
            builder.ins().stack_addr(pointer_type, slot, 0)
        } else {
            builder.ins().iconst(pointer_type, 0)
        };

        // Allocate stack slot for element storage (used for inline push)
        let elem_slot = builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            elem_size,
            elem_align_shift,
        ));

        // Variable to hold element count from seq_begin (used for preallocation)
        let seq_count_var = builder.declare_var(pointer_type);
        builder.def_var(seq_count_var, zero_cap);

        builder.ins().jump(seq_begin, &[]);
        builder.seal_block(entry);

        // seq_begin: use inline IR for array start (no helper call!)
        builder.switch_to_block(seq_begin);

        // Create cursor for emit_seq_begin
        let mut cursor = JitCursor {
            input_ptr,
            len,
            pos: pos_var,
            ptr_type: pointer_type,
            scratch_ptr,
        };

        // Use inline IR for seq_begin
        // Returns (count, error) - count is used for Vec preallocation
        let format = F::default();
        let (seq_count, err_code) =
            format.emit_seq_begin(module, &mut builder, &mut cursor, state_ptr);

        // emit_seq_begin leaves us at its merge block and updates cursor.pos internally
        builder.def_var(err_var, err_code);
        builder.def_var(seq_count_var, seq_count);
        builder.ins().jump(check_seq_begin_err, &[]);
        builder.seal_block(seq_begin);

        // check_seq_begin_err
        builder.switch_to_block(check_seq_begin_err);
        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
        builder.ins().brif(is_ok, init_vec, &[], error, &[]);
        builder.seal_block(check_seq_begin_err);

        // init_vec: initialize Vec with capacity from seq_begin count
        // This preallocates for length-prefixed formats (postcard) and is 0 for
        // delimiter formats (JSON) where the count isn't known upfront
        builder.switch_to_block(init_vec);
        let capacity = builder.use_var(seq_count_var);
        let vec_init_ptr = builder.ins().iconst(
            pointer_type,
            helpers::jit_vec_init_with_capacity as *const u8 as i64,
        );
        builder.ins().call_indirect(
            sig_vec_init_ref,
            vec_init_ptr,
            &[out_ptr, capacity, init_fn_ptr],
        );

        // Mark output as initialized so wrapper can drop on error
        let one_i8 = builder.ins().iconst(types::I8, 1);
        builder.ins().store(
            MemFlags::trusted(),
            one_i8,
            scratch_ptr,
            JIT_SCRATCH_OUTPUT_INITIALIZED_OFFSET,
        );

        // Branch to direct-fill, buffered direct-fill, or push-based path
        if use_direct_fill {
            // For counted formats: if count > 0, use direct-fill; else success (empty array)
            let count_gt_zero = builder.ins().icmp_imm(IntCC::NotEqual, capacity, 0);
            builder
                .ins()
                .brif(count_gt_zero, df_setup, &[], success, &[]);
        } else if use_buffered_direct_fill {
            // For delimiter formats with scalars: use buffered direct-fill
            builder.ins().jump(bf_setup, &[]);
        } else if use_buffered_direct_fill_struct {
            // For delimiter formats with structs: use buffered direct-fill for structs
            builder.ins().jump(bfs_setup, &[]);
        } else {
            // For delimiter formats with complex types: use push-based loop
            builder.ins().jump(loop_check_end, &[]);
        }
        builder.seal_block(init_vec);

        // Only build the push-based loop if we're not using direct-fill or buffered paths.
        // The direct-fill, buffered scalar, and buffered struct paths have their own loops.
        let use_push_based_loop =
            !use_direct_fill && !use_buffered_direct_fill && !use_buffered_direct_fill_struct;

        if use_push_based_loop {
            // loop_check_end: use inline IR for seq_is_end
            //
            // VALUE BOUNDARY INVARIANT (format-neutral):
            // At loop entry, cursor.pos is at a valid "value boundary" for the format.
            // This is maintained by format-specific emit_* methods:
            //   - emit_seq_begin leaves cursor ready for first element or end check
            //   - emit_seq_next advances past any element separator, leaving cursor
            //     ready for the next element or end check
            //   - emit_parse_* methods consume exactly one value
            //   - emit_seq_is_end checks (and consumes end marker if present)
            //
            // For delimiter formats (JSON): value boundary = after trivia
            // For counted formats (postcard): value boundary = at next byte (no trivia)
            //
            // Note: loop_check_end is NOT sealed here - it has a back edge from check_seq_next_err
            builder.switch_to_block(loop_check_end);

            // Create cursor for emit methods (reused for seq_is_end and seq_next)
            let mut cursor = JitCursor {
                input_ptr,
                len,
                pos: pos_var,
                ptr_type: pointer_type,
                scratch_ptr,
            };

            // Use inline IR for seq_is_end (no helper call!)
            let format = F::default();
            // state_ptr was allocated in entry block - reuse it
            let (is_end_i8, err_code) =
                format.emit_seq_is_end(module, &mut builder, &mut cursor, state_ptr);

            // emit_seq_is_end leaves us at its merge block
            // Store error for error block and check results
            builder.def_var(err_var, err_code);

            // Convert is_end from I8 to check
            let is_end = builder.ins().uextend(pointer_type, is_end_i8);

            builder.ins().jump(check_is_end_err, &[]);
            // Note: loop_check_end will be sealed later, after check_seq_next_err is declared

            // check_is_end_err
            builder.switch_to_block(check_is_end_err);
            let err_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
            builder
                .ins()
                .brif(err_ok, check_is_end_value, &[], error, &[]);
            builder.seal_block(check_is_end_err);

            // check_is_end_value
            builder.switch_to_block(check_is_end_value);
            let is_end_bool = builder.ins().icmp_imm(IntCC::NotEqual, is_end, 0);
            builder
                .ins()
                .brif(is_end_bool, success, &[], parse_element, &[]);
            builder.seal_block(check_is_end_value);

            // parse_element: use inline IR for parsing
            builder.switch_to_block(parse_element);
            match elem_kind {
                FormatListElementKind::Bool => {
                    // Create cursor for emit methods
                    let mut cursor = JitCursor {
                        input_ptr,
                        len,
                        pos: pos_var,
                        ptr_type: pointer_type,
                        scratch_ptr,
                    };

                    // Use inline IR for bool parsing (no helper call!)
                    let format = F::default();
                    let (value_i8, err_code) =
                        format.emit_parse_bool(module, &mut builder, &mut cursor);

                    // Store parsed value and error
                    builder.def_var(parsed_value_var, value_i8);
                    builder.def_var(err_var, err_code);

                    // emit_parse_bool leaves us in its merge block, jump to check_parse_err
                    builder.ins().jump(check_parse_err, &[]);

                    // Seal parse_element (its only predecessor check_is_end_value already branched to it)
                    builder.seal_block(parse_element);

                    // check_parse_err: check error and branch
                    builder.switch_to_block(check_parse_err);
                    let parse_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                    builder.ins().brif(parse_ok, push_element, &[], error, &[]);
                    builder.seal_block(check_parse_err);
                }
                FormatListElementKind::U8 => {
                    let mut cursor = JitCursor {
                        input_ptr,
                        len,
                        pos: pos_var,
                        ptr_type: pointer_type,
                        scratch_ptr,
                    };

                    let format = F::default();
                    let (value_u8, err_code) =
                        format.emit_parse_u8(module, &mut builder, &mut cursor);

                    builder.def_var(parsed_value_var, value_u8);
                    builder.def_var(err_var, err_code);

                    builder.ins().jump(check_parse_err, &[]);
                    builder.seal_block(parse_element);

                    builder.switch_to_block(check_parse_err);
                    let parse_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                    builder.ins().brif(parse_ok, push_element, &[], error, &[]);
                    builder.seal_block(check_parse_err);
                }
                FormatListElementKind::I64 => {
                    let mut cursor = JitCursor {
                        input_ptr,
                        len,
                        pos: pos_var,
                        ptr_type: pointer_type,
                        scratch_ptr,
                    };

                    let format = F::default();
                    let (value_i64, err_code) =
                        format.emit_parse_i64(module, &mut builder, &mut cursor);

                    builder.def_var(parsed_value_var, value_i64);
                    builder.def_var(err_var, err_code);

                    builder.ins().jump(check_parse_err, &[]);
                    builder.seal_block(parse_element);

                    builder.switch_to_block(check_parse_err);
                    let parse_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                    builder.ins().brif(parse_ok, push_element, &[], error, &[]);
                    builder.seal_block(check_parse_err);
                }
                FormatListElementKind::U64 => {
                    let mut cursor = JitCursor {
                        input_ptr,
                        len,
                        pos: pos_var,
                        ptr_type: pointer_type,
                        scratch_ptr,
                    };

                    let format = F::default();
                    let (value_u64, err_code) =
                        format.emit_parse_u64(module, &mut builder, &mut cursor);

                    builder.def_var(parsed_value_var, value_u64);
                    builder.def_var(err_var, err_code);

                    builder.ins().jump(check_parse_err, &[]);
                    builder.seal_block(parse_element);

                    builder.switch_to_block(check_parse_err);
                    let parse_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                    builder.ins().brif(parse_ok, push_element, &[], error, &[]);
                    builder.seal_block(check_parse_err);
                }
                FormatListElementKind::F64 => {
                    let mut cursor = JitCursor {
                        input_ptr,
                        len,
                        pos: pos_var,
                        ptr_type: pointer_type,
                        scratch_ptr,
                    };

                    let format = F::default();
                    let (value_f64, err_code) =
                        format.emit_parse_f64(module, &mut builder, &mut cursor);

                    builder.def_var(parsed_value_var, value_f64);
                    builder.def_var(err_var, err_code);

                    builder.ins().jump(check_parse_err, &[]);
                    builder.seal_block(parse_element);

                    builder.switch_to_block(check_parse_err);
                    let parse_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                    builder.ins().brif(parse_ok, push_element, &[], error, &[]);
                    builder.seal_block(check_parse_err);
                }
                FormatListElementKind::String => {
                    // String parsing returns JitStringValue (ptr, len, cap, owned)
                    let mut cursor = JitCursor {
                        input_ptr,
                        len,
                        pos: pos_var,
                        ptr_type: pointer_type,
                        scratch_ptr,
                    };

                    let format = F::default();
                    let (string_val, err_code) =
                        format.emit_parse_string(module, &mut builder, &mut cursor);

                    builder.def_var(err_var, err_code);

                    builder.ins().jump(check_parse_err, &[]);
                    builder.seal_block(parse_element);

                    // check_parse_err: check error and handle String push differently
                    builder.switch_to_block(check_parse_err);
                    let parse_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);

                    // For String, we need to call a helper instead of using push_element block
                    // Create a push_string block
                    let push_string = builder.create_block();
                    builder.ins().brif(parse_ok, push_string, &[], error, &[]);
                    builder.seal_block(check_parse_err);

                    // push_string: call jit_vec_push_string helper
                    builder.switch_to_block(push_string);
                    let vec_out_ptr = out_ptr;
                    let push_fn_ptr = builder.use_var(push_fn_var);

                    // Declare jit_vec_push_string helper
                    let helper_sig = {
                        let mut sig = make_c_sig(module);
                        sig.params.push(AbiParam::new(pointer_type)); // vec_ptr
                        sig.params.push(AbiParam::new(pointer_type)); // push_fn
                        sig.params.push(AbiParam::new(pointer_type)); // str_ptr
                        sig.params.push(AbiParam::new(pointer_type)); // str_len
                        sig.params.push(AbiParam::new(pointer_type)); // str_cap
                        sig.params.push(AbiParam::new(types::I8)); // owned (bool)
                        sig
                    };

                    let helper_sig_ref = builder.import_signature(helper_sig);
                    let helper_ptr = builder.ins().iconst(
                        pointer_type,
                        helpers::jit_vec_push_string as *const u8 as i64,
                    );

                    // owned is already i8 (1 for owned, 0 for borrowed), use it directly
                    // No need to extend since it matches the helper signature

                    // Call helper
                    builder.ins().call_indirect(
                        helper_sig_ref,
                        helper_ptr,
                        &[
                            vec_out_ptr,
                            push_fn_ptr,
                            string_val.ptr,
                            string_val.len,
                            string_val.cap,
                            string_val.owned,
                        ],
                    );

                    // Jump to seq_next
                    builder.ins().jump(seq_next, &[]);
                    builder.seal_block(push_string);
                }
                FormatListElementKind::Struct(struct_shape) => {
                    // Struct parsing: recursively call struct deserializer
                    jit_debug!("[compile_list] Parsing struct element");

                    // Compile the nested struct deserializer using the appropriate encoding
                    let struct_func_id = match F::STRUCT_ENCODING {
                        crate::jit::StructEncoding::Map => {
                            compile_struct_format_deserializer::<F>(module, struct_shape, memo)?
                        }
                        crate::jit::StructEncoding::Positional => {
                            compile_struct_positional_deserializer::<F>(module, struct_shape, memo)?
                        }
                    };
                    let struct_func_ref = module.declare_func_in_func(struct_func_id, builder.func);

                    // Allocate stack slot for struct element
                    let struct_layout = struct_shape.layout.sized_layout().ok()?;
                    let struct_size = struct_layout.size() as u32;
                    let struct_align = struct_layout.align().trailing_zeros() as u8;
                    let struct_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        struct_size,
                        struct_align,
                    ));
                    let struct_elem_ptr = builder.ins().stack_addr(pointer_type, struct_slot, 0);

                    // Call struct deserializer: (input_ptr, len, pos, struct_elem_ptr, scratch_ptr)
                    let current_pos = builder.use_var(pos_var);
                    let struct_func_ptr =
                        func_addr_value(&mut builder, pointer_type, struct_func_ref);
                    let call_result = builder.ins().call_indirect(
                        nested_call_sig_ref,
                        struct_func_ptr,
                        &[input_ptr, len, current_pos, struct_elem_ptr, scratch_ptr],
                    );
                    let new_pos = builder.inst_results(call_result)[0];

                    // Check for error (new_pos < 0 means error, scratch already written)
                    let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
                    let struct_parse_ok = builder.create_block();
                    builder.ins().brif(
                        is_error,
                        nested_error_passthrough,
                        &[],
                        struct_parse_ok,
                        &[],
                    );
                    builder.seal_block(parse_element);

                    // On success: update pos_var and push struct element
                    builder.switch_to_block(struct_parse_ok);
                    builder.def_var(pos_var, new_pos);

                    // Push struct element to Vec using push_fn via call_indirect
                    let vec_out_ptr = out_ptr;
                    let push_fn_ptr = builder.use_var(push_fn_var);

                    // Signature for push_fn: PtrMut arguments become two pointer-sized values (ptr + metadata)
                    // push_fn(vec_ptr, vec_metadata, elem_ptr, elem_metadata)
                    let push_sig = {
                        let mut sig = make_c_sig(module);
                        sig.params.push(AbiParam::new(pointer_type)); // vec_ptr
                        sig.params.push(AbiParam::new(pointer_type)); // vec_metadata (0 for thin pointers)
                        sig.params.push(AbiParam::new(pointer_type)); // elem_ptr
                        sig.params.push(AbiParam::new(pointer_type)); // elem_metadata (0 for thin pointers)
                        sig
                    };
                    let push_sig_ref = builder.import_signature(push_sig);

                    // Call push_fn indirectly with metadata (0 for thin pointers)
                    let null_metadata = builder.ins().iconst(pointer_type, 0);
                    builder.ins().call_indirect(
                        push_sig_ref,
                        push_fn_ptr,
                        &[vec_out_ptr, null_metadata, struct_elem_ptr, null_metadata],
                    );

                    // Jump to seq_next
                    builder.ins().jump(seq_next, &[]);
                    builder.seal_block(struct_parse_ok);
                }
                FormatListElementKind::List(inner_shape) => {
                    // Nested Vec<T> parsing: recursively call list deserializer
                    jit_debug!("[compile_list] Parsing nested list element");

                    // Compile the nested list deserializer
                    let list_func_id =
                        compile_list_format_deserializer::<F>(module, inner_shape, memo)?;
                    let list_func_ref = module.declare_func_in_func(list_func_id, builder.func);

                    // Allocate stack slot for Vec element (ptr + len + cap)
                    let vec_layout = inner_shape.layout.sized_layout().ok()?;
                    let vec_size = vec_layout.size() as u32;
                    let vec_align = vec_layout.align().trailing_zeros() as u8;
                    let vec_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        vec_size,
                        vec_align,
                    ));
                    let vec_elem_ptr = builder.ins().stack_addr(pointer_type, vec_slot, 0);

                    // Call list deserializer: (input_ptr, len, pos, vec_elem_ptr, scratch_ptr)
                    let current_pos = builder.use_var(pos_var);
                    let list_func_ptr = func_addr_value(&mut builder, pointer_type, list_func_ref);
                    let call_result = builder.ins().call_indirect(
                        nested_call_sig_ref,
                        list_func_ptr,
                        &[input_ptr, len, current_pos, vec_elem_ptr, scratch_ptr],
                    );
                    let new_pos = builder.inst_results(call_result)[0];

                    // Check for error (new_pos < 0 means error, scratch already written)
                    let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
                    let list_parse_ok = builder.create_block();
                    let list_drop_and_passthrough = builder.create_block();
                    builder.ins().brif(
                        is_error,
                        list_drop_and_passthrough,
                        &[],
                        list_parse_ok,
                        &[],
                    );
                    builder.seal_block(parse_element);

                    // list_drop_and_passthrough: nested list initialized its output; drop it to avoid leaks,
                    // then passthrough error without overwriting scratch.
                    builder.switch_to_block(list_drop_and_passthrough);
                    let drop_in_place_sig_ref = {
                        let mut s = make_c_sig(module);
                        s.params.push(AbiParam::new(pointer_type)); // shape_ptr
                        s.params.push(AbiParam::new(pointer_type)); // ptr
                        builder.import_signature(s)
                    };
                    let drop_in_place_ptr = builder
                        .ins()
                        .iconst(pointer_type, helpers::jit_drop_in_place as *const u8 as i64);
                    let shape_ptr = builder
                        .ins()
                        .iconst(pointer_type, inner_shape as *const _ as usize as i64);
                    builder.ins().call_indirect(
                        drop_in_place_sig_ref,
                        drop_in_place_ptr,
                        &[shape_ptr, vec_elem_ptr],
                    );
                    builder.ins().jump(nested_error_passthrough, &[]);
                    builder.seal_block(list_drop_and_passthrough);

                    // On success: update pos_var and push Vec element
                    builder.switch_to_block(list_parse_ok);
                    builder.def_var(pos_var, new_pos);

                    // Push Vec element to outer Vec using push_fn via call_indirect
                    let vec_out_ptr = out_ptr;
                    let push_fn_ptr = builder.use_var(push_fn_var);

                    // Signature for push_fn: PtrMut arguments become two pointer-sized values (ptr + metadata)
                    // push_fn(vec_ptr, vec_metadata, elem_ptr, elem_metadata)
                    let push_sig = {
                        let mut sig = make_c_sig(module);
                        sig.params.push(AbiParam::new(pointer_type)); // vec_ptr
                        sig.params.push(AbiParam::new(pointer_type)); // vec_metadata (0 for thin pointers)
                        sig.params.push(AbiParam::new(pointer_type)); // elem_ptr
                        sig.params.push(AbiParam::new(pointer_type)); // elem_metadata (0 for thin pointers)
                        sig
                    };
                    let push_sig_ref = builder.import_signature(push_sig);

                    // Call push_fn indirectly with metadata (0 for thin pointers)
                    let null_metadata = builder.ins().iconst(pointer_type, 0);
                    builder.ins().call_indirect(
                        push_sig_ref,
                        push_fn_ptr,
                        &[vec_out_ptr, null_metadata, vec_elem_ptr, null_metadata],
                    );

                    // Jump to seq_next
                    builder.ins().jump(seq_next, &[]);
                    builder.seal_block(list_parse_ok);
                }
                FormatListElementKind::Map(inner_shape) => {
                    // Nested HashMap<K, V> parsing: recursively call map deserializer
                    jit_debug!("[compile_list] Parsing nested map element");

                    // Compile the nested map deserializer
                    let map_func_id =
                        compile_map_format_deserializer::<F>(module, inner_shape, memo)?;
                    let map_func_ref = module.declare_func_in_func(map_func_id, builder.func);

                    // Allocate stack slot for HashMap element
                    let map_layout = inner_shape.layout.sized_layout().ok()?;
                    let map_size = map_layout.size() as u32;
                    let map_align = map_layout.align().trailing_zeros() as u8;
                    let map_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        map_size,
                        map_align,
                    ));
                    let map_elem_ptr = builder.ins().stack_addr(pointer_type, map_slot, 0);

                    // Call map deserializer: (input_ptr, len, pos, map_elem_ptr, scratch_ptr)
                    let current_pos = builder.use_var(pos_var);
                    let map_func_ptr = func_addr_value(&mut builder, pointer_type, map_func_ref);
                    let call_result = builder.ins().call_indirect(
                        nested_call_sig_ref,
                        map_func_ptr,
                        &[input_ptr, len, current_pos, map_elem_ptr, scratch_ptr],
                    );
                    let new_pos = builder.inst_results(call_result)[0];

                    // Check for error (new_pos < 0 means error, scratch already written)
                    let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
                    let map_parse_ok = builder.create_block();
                    let map_drop_and_passthrough = builder.create_block();
                    builder
                        .ins()
                        .brif(is_error, map_drop_and_passthrough, &[], map_parse_ok, &[]);
                    builder.seal_block(parse_element);

                    // map_drop_and_passthrough: nested map initialized its output; drop it to avoid leaks,
                    // then passthrough error without overwriting scratch.
                    builder.switch_to_block(map_drop_and_passthrough);
                    let drop_in_place_sig_ref = {
                        let mut s = make_c_sig(module);
                        s.params.push(AbiParam::new(pointer_type)); // shape_ptr
                        s.params.push(AbiParam::new(pointer_type)); // ptr
                        builder.import_signature(s)
                    };
                    let drop_in_place_ptr = builder
                        .ins()
                        .iconst(pointer_type, helpers::jit_drop_in_place as *const u8 as i64);
                    let shape_ptr = builder
                        .ins()
                        .iconst(pointer_type, inner_shape as *const _ as usize as i64);
                    builder.ins().call_indirect(
                        drop_in_place_sig_ref,
                        drop_in_place_ptr,
                        &[shape_ptr, map_elem_ptr],
                    );
                    builder.ins().jump(nested_error_passthrough, &[]);
                    builder.seal_block(map_drop_and_passthrough);

                    // On success: update pos_var and push HashMap element
                    builder.switch_to_block(map_parse_ok);
                    builder.def_var(pos_var, new_pos);

                    // Push HashMap element to Vec using push_fn via call_indirect
                    let vec_out_ptr = out_ptr;
                    let push_fn_ptr = builder.use_var(push_fn_var);

                    // Signature for push_fn: PtrMut arguments become two pointer-sized values (ptr + metadata)
                    // push_fn(vec_ptr, vec_metadata, elem_ptr, elem_metadata)
                    let push_sig = {
                        let mut sig = make_c_sig(module);
                        sig.params.push(AbiParam::new(pointer_type)); // vec_ptr
                        sig.params.push(AbiParam::new(pointer_type)); // vec_metadata (0 for thin pointers)
                        sig.params.push(AbiParam::new(pointer_type)); // elem_ptr
                        sig.params.push(AbiParam::new(pointer_type)); // elem_metadata (0 for thin pointers)
                        sig
                    };
                    let push_sig_ref = builder.import_signature(push_sig);

                    // Call push_fn indirectly with metadata (0 for thin pointers)
                    let null_metadata = builder.ins().iconst(pointer_type, 0);
                    builder.ins().call_indirect(
                        push_sig_ref,
                        push_fn_ptr,
                        &[vec_out_ptr, null_metadata, map_elem_ptr, null_metadata],
                    );

                    // Jump to seq_next
                    builder.ins().jump(seq_next, &[]);
                    builder.seal_block(map_parse_ok);
                }
            }

            // push_element: store value to stack slot and call push_fn directly
            builder.switch_to_block(push_element);
            let parsed_value = builder.use_var(parsed_value_var);
            let push_fn_ptr = builder.use_var(push_fn_var);

            // Get address of element stack slot
            let elem_ptr = builder.ins().stack_addr(pointer_type, elem_slot, 0);

            // Store the parsed value into the element stack slot
            builder
                .ins()
                .store(MemFlags::trusted(), parsed_value, elem_ptr, 0);

            // Call push_fn directly via call_indirect: push_fn(vec_ptr, elem_ptr)
            // PtrMut is a 16-byte struct (TaggedPtr + metadata), so each PtrMut argument
            // becomes two pointer-sized values. For thin pointers, metadata is 0.
            let null_metadata = builder.ins().iconst(pointer_type, 0);
            builder.ins().call_indirect(
                sig_direct_push_ref,
                push_fn_ptr,
                &[out_ptr, null_metadata, elem_ptr, null_metadata],
            );

            builder.ins().jump(seq_next, &[]);
            builder.seal_block(push_element);

            // seq_next: use inline IR for comma handling
            builder.switch_to_block(seq_next);

            // Reuse cursor (need to recreate since emit_parse_bool may have been called)
            let mut cursor = JitCursor {
                input_ptr,
                len,
                pos: pos_var,
                ptr_type: pointer_type,
                scratch_ptr,
            };

            // Use inline IR for seq_next (no helper call!)
            let format = F::default();
            // state_ptr was allocated in entry block - reuse it
            let err_code = format.emit_seq_next(module, &mut builder, &mut cursor, state_ptr);

            // emit_seq_next leaves us at its merge block and updates cursor.pos internally
            builder.def_var(err_var, err_code);
            builder.ins().jump(check_seq_next_err, &[]);
            builder.seal_block(seq_next);

            // check_seq_next_err
            builder.switch_to_block(check_seq_next_err);
            let next_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
            builder.ins().brif(next_ok, loop_check_end, &[], error, &[]);
            builder.seal_block(check_seq_next_err);

            // Seal loop_check_end - it has a back edge from check_seq_next_err
            builder.seal_block(loop_check_end);
        } // end of push-based loop construction

        // nested_error_passthrough: nested call failed and already wrote scratch,
        // return -1 without overwriting scratch.
        // This block is used by both push-based loop (for Struct/List/Map elements)
        // and buffered struct path. We define it here so both paths can branch to it.
        // NOTE: We don't seal this block yet - it's sealed at the end.
        builder.switch_to_block(nested_error_passthrough);
        let neg_one = builder.ins().iconst(pointer_type, -1i64);
        builder.ins().return_(&[neg_one]);

        // =================================================================
        // Direct-fill path (only used when use_direct_fill is true)
        // =================================================================
        if use_direct_fill {
            // df_setup: get base pointer and initialize counter
            builder.switch_to_block(df_setup);
            let set_len_fn_ptr = builder
                .ins()
                .iconst(pointer_type, set_len_fn.unwrap() as *const () as i64);
            let as_mut_ptr_fn_ptr = builder.ins().iconst(
                pointer_type,
                as_mut_ptr_typed_fn.unwrap() as *const () as i64,
            );

            // Get base pointer to vec's buffer
            let vec_as_mut_ptr_typed_ptr = builder.ins().iconst(
                pointer_type,
                helpers::jit_vec_as_mut_ptr_typed as *const u8 as i64,
            );
            let call_inst = builder.ins().call_indirect(
                sig_vec_as_mut_ptr_typed_ref,
                vec_as_mut_ptr_typed_ptr,
                &[out_ptr, as_mut_ptr_fn_ptr],
            );
            let base_ptr = builder.inst_results(call_inst)[0];

            // Store base_ptr and set_len_fn_ptr in variables for use in loop/finalize
            let base_ptr_var = builder.declare_var(pointer_type);
            builder.def_var(base_ptr_var, base_ptr);
            let set_len_fn_var = builder.declare_var(pointer_type);
            builder.def_var(set_len_fn_var, set_len_fn_ptr);

            // Initialize loop counter to 0
            let counter_var = builder.declare_var(pointer_type);
            let zero = builder.ins().iconst(pointer_type, 0);
            builder.def_var(counter_var, zero);

            // For U8: try bulk copy path first
            if elem_kind == FormatListElementKind::U8 {
                builder.ins().jump(df_bulk_copy, &[]);
            } else {
                builder.ins().jump(df_loop_check, &[]);
            }
            builder.seal_block(df_setup);

            // df_bulk_copy: try bulk copy for Vec<u8>
            builder.switch_to_block(df_bulk_copy);
            let format = F::default();
            let count = builder.use_var(seq_count_var);
            let base_ptr = builder.use_var(base_ptr_var);
            let mut cursor = JitCursor {
                input_ptr,
                len,
                pos: pos_var,
                ptr_type: pointer_type,
                scratch_ptr,
            };
            if let Some(bulk_err) =
                format.emit_seq_bulk_copy_u8(&mut builder, &mut cursor, count, base_ptr)
            {
                // Format supports bulk copy - check error
                builder.def_var(err_var, bulk_err);
                builder.ins().jump(df_bulk_copy_check_err, &[]);
            } else {
                // Format doesn't support bulk copy, fall back to element-by-element loop
                builder.ins().jump(df_loop_check, &[]);
            }
            builder.seal_block(df_bulk_copy);

            // df_bulk_copy_check_err: check if bulk copy succeeded
            builder.switch_to_block(df_bulk_copy_check_err);
            let bulk_err = builder.use_var(err_var);
            let bulk_ok = builder.ins().icmp_imm(IntCC::Equal, bulk_err, 0);
            // On success: set counter_var = count so df_finalize sets correct length
            let set_counter_block = builder.create_block();
            builder
                .ins()
                .brif(bulk_ok, set_counter_block, &[], error, &[]);
            builder.seal_block(df_bulk_copy_check_err);

            builder.switch_to_block(set_counter_block);
            let count = builder.use_var(seq_count_var);
            builder.def_var(counter_var, count);
            builder.ins().jump(df_finalize, &[]);
            builder.seal_block(set_counter_block);

            // df_loop_check: check if counter < count
            builder.switch_to_block(df_loop_check);
            let counter = builder.use_var(counter_var);
            let count = builder.use_var(seq_count_var);
            let done = builder
                .ins()
                .icmp(IntCC::UnsignedGreaterThanOrEqual, counter, count);
            builder.ins().brif(done, df_finalize, &[], df_parse, &[]);
            // Note: df_loop_check will be sealed after df_store (back edge)

            // df_parse: parse the next element
            builder.switch_to_block(df_parse);
            let mut cursor = JitCursor {
                input_ptr,
                len,
                pos: pos_var,
                ptr_type: pointer_type,
                scratch_ptr,
            };

            // Parse based on element type
            let format = F::default();
            let (parsed_val, parse_err) = match elem_kind {
                FormatListElementKind::Bool => {
                    format.emit_parse_bool(module, &mut builder, &mut cursor)
                }
                FormatListElementKind::U8 => {
                    format.emit_parse_u8(module, &mut builder, &mut cursor)
                }
                FormatListElementKind::I64 => {
                    format.emit_parse_i64(module, &mut builder, &mut cursor)
                }
                FormatListElementKind::U64 => {
                    format.emit_parse_u64(module, &mut builder, &mut cursor)
                }
                _ => unreachable!("direct-fill only for scalars"),
            };
            builder.def_var(parsed_value_var, parsed_val);
            builder.def_var(err_var, parse_err);
            builder.ins().jump(df_check_parse_err, &[]);
            builder.seal_block(df_parse);

            // df_check_parse_err
            builder.switch_to_block(df_check_parse_err);
            let parse_ok = builder.ins().icmp_imm(IntCC::Equal, parse_err, 0);
            builder.ins().brif(parse_ok, df_store, &[], error, &[]);
            builder.seal_block(df_check_parse_err);

            // df_store: store parsed value directly into vec buffer
            builder.switch_to_block(df_store);
            let parsed_val = builder.use_var(parsed_value_var);
            let base_ptr = builder.use_var(base_ptr_var);
            let counter = builder.use_var(counter_var);

            // Calculate offset: base_ptr + counter * elem_size
            let elem_size_val = builder.ins().iconst(pointer_type, elem_size as i64);
            let offset = builder.ins().imul(counter, elem_size_val);
            let dest_ptr = builder.ins().iadd(base_ptr, offset);

            // Truncate value if needed and store with the correct width.
            // Note: emit_parse_bool/emit_parse_u8 already return i8, so no truncation needed.
            // Only emit_parse_i64/emit_parse_u64 (which return i64) need truncation for smaller types.
            use facet_core::ScalarType;
            let scalar_type = elem_shape.scalar_type().unwrap();
            let store_val = match scalar_type {
                // Bool/U8/I8: parser returns i8 directly, no truncation needed
                ScalarType::Bool | ScalarType::U8 | ScalarType::I8 => parsed_val,
                // I16/U16/I32/U32: parser returns i64, truncate to correct width
                ScalarType::I16 | ScalarType::U16 => builder.ins().ireduce(types::I16, parsed_val),
                ScalarType::I32 | ScalarType::U32 => builder.ins().ireduce(types::I32, parsed_val),
                // I64/U64: parser returns i64 directly
                ScalarType::I64 | ScalarType::U64 => parsed_val,
                _ => unreachable!("direct-fill only for integers"),
            };
            builder
                .ins()
                .store(MemFlags::trusted(), store_val, dest_ptr, 0);

            // Increment counter
            let one = builder.ins().iconst(pointer_type, 1);
            let new_counter = builder.ins().iadd(counter, one);
            builder.def_var(counter_var, new_counter);

            // Loop back
            builder.ins().jump(df_loop_check, &[]);
            builder.seal_block(df_store);
            builder.seal_block(df_loop_check); // Now we can seal it (back edge from df_store)

            // df_finalize: set the vec's length and go to success
            builder.switch_to_block(df_finalize);
            let final_count = builder.use_var(counter_var);
            let set_len_fn_ptr = builder.use_var(set_len_fn_var);
            let vec_set_len_ptr = builder
                .ins()
                .iconst(pointer_type, helpers::jit_vec_set_len as *const u8 as i64);
            builder.ins().call_indirect(
                sig_vec_set_len_ref,
                vec_set_len_ptr,
                &[out_ptr, final_count, set_len_fn_ptr],
            );
            builder.ins().jump(success, &[]);
            builder.seal_block(df_finalize);

            // Seal unused push-based and buffered blocks
            builder.seal_block(loop_check_end);
            builder.seal_block(bf_setup);
            builder.seal_block(bf_loop_check);
            builder.seal_block(bf_check_is_end_err);
            builder.seal_block(bf_check_is_end_value);
            builder.seal_block(bf_check_capacity);
            builder.seal_block(bf_grow);
            builder.seal_block(bf_parse);
            builder.seal_block(bf_check_parse_err);
            builder.seal_block(bf_store);
            builder.seal_block(bf_seq_next);
            builder.seal_block(bf_check_seq_next_err);
            builder.seal_block(bf_finalize);
            builder.seal_block(bfs_setup);
            builder.seal_block(bfs_loop_check);
            builder.seal_block(bfs_check_is_end_err);
            builder.seal_block(bfs_check_is_end_value);
            builder.seal_block(bfs_check_capacity);
            builder.seal_block(bfs_grow);
            builder.seal_block(bfs_parse);
            builder.seal_block(bfs_check_parse_err);
            builder.seal_block(bfs_seq_next);
            builder.seal_block(bfs_check_seq_next_err);
            builder.seal_block(bfs_finalize);
        } else if use_buffered_direct_fill {
            // =================================================================
            // Buffered direct-fill path (for delimiter formats with scalars)
            // =================================================================

            // bf_setup: get capacity, base_ptr, initialize counter
            builder.switch_to_block(bf_setup);

            // Get vtable function pointers
            let set_len_fn_ptr = builder
                .ins()
                .iconst(pointer_type, set_len_fn.unwrap() as *const () as i64);
            let as_mut_ptr_fn_ptr = builder.ins().iconst(
                pointer_type,
                as_mut_ptr_typed_fn.unwrap() as *const () as i64,
            );
            let reserve_fn_ptr = builder
                .ins()
                .iconst(pointer_type, reserve_fn.unwrap() as *const () as i64);
            let capacity_fn_ptr = builder
                .ins()
                .iconst(pointer_type, capacity_fn.unwrap() as *const () as i64);

            // Store vtable pointers in variables
            let set_len_fn_var = builder.declare_var(pointer_type);
            builder.def_var(set_len_fn_var, set_len_fn_ptr);
            let as_mut_ptr_fn_var = builder.declare_var(pointer_type);
            builder.def_var(as_mut_ptr_fn_var, as_mut_ptr_fn_ptr);
            let reserve_fn_var = builder.declare_var(pointer_type);
            builder.def_var(reserve_fn_var, reserve_fn_ptr);
            let capacity_fn_var = builder.declare_var(pointer_type);
            builder.def_var(capacity_fn_var, capacity_fn_ptr);

            // Get initial capacity
            let vec_capacity_ptr = builder
                .ins()
                .iconst(pointer_type, helpers::jit_vec_capacity as *const u8 as i64);
            let cap_call = builder.ins().call_indirect(
                sig_vec_capacity_ref,
                vec_capacity_ptr,
                &[out_ptr, capacity_fn_ptr],
            );
            let initial_capacity = builder.inst_results(cap_call)[0];

            // Get initial base pointer
            let vec_as_mut_ptr_ptr = builder.ins().iconst(
                pointer_type,
                helpers::jit_vec_as_mut_ptr_typed as *const u8 as i64,
            );
            let base_call = builder.ins().call_indirect(
                sig_vec_as_mut_ptr_typed_ref,
                vec_as_mut_ptr_ptr,
                &[out_ptr, as_mut_ptr_fn_ptr],
            );
            let initial_base_ptr = builder.inst_results(base_call)[0];

            // Variables for loop: base_ptr, capacity, count
            let bf_base_ptr_var = builder.declare_var(pointer_type);
            builder.def_var(bf_base_ptr_var, initial_base_ptr);
            let bf_capacity_var = builder.declare_var(pointer_type);
            builder.def_var(bf_capacity_var, initial_capacity);
            let bf_counter_var = builder.declare_var(pointer_type);
            let zero = builder.ins().iconst(pointer_type, 0);
            builder.def_var(bf_counter_var, zero);

            builder.ins().jump(bf_loop_check, &[]);
            builder.seal_block(bf_setup);

            // bf_loop_check: check if we've reached end of sequence
            builder.switch_to_block(bf_loop_check);
            let mut cursor = JitCursor {
                input_ptr,
                len,
                pos: pos_var,
                ptr_type: pointer_type,
                scratch_ptr,
            };
            let format = F::default();
            let (is_end_i8, is_end_err) =
                format.emit_seq_is_end(module, &mut builder, &mut cursor, state_ptr);
            // Extend I8 to pointer_type for storage/comparison
            let is_end = builder.ins().uextend(pointer_type, is_end_i8);
            builder.def_var(is_end_var, is_end);
            builder.def_var(err_var, is_end_err);
            builder.ins().jump(bf_check_is_end_err, &[]);
            // Note: bf_loop_check sealed after bf_check_seq_next_err (back edge)

            // bf_check_is_end_err: check for error from seq_is_end
            builder.switch_to_block(bf_check_is_end_err);
            let is_end_ok = builder.ins().icmp_imm(IntCC::Equal, is_end_err, 0);
            builder
                .ins()
                .brif(is_end_ok, bf_check_is_end_value, &[], error, &[]);
            builder.seal_block(bf_check_is_end_err);

            // bf_check_is_end_value: check if end marker found
            builder.switch_to_block(bf_check_is_end_value);
            let is_end_val = builder.use_var(is_end_var);
            let is_done = builder.ins().icmp_imm(IntCC::NotEqual, is_end_val, 0);
            builder
                .ins()
                .brif(is_done, bf_finalize, &[], bf_check_capacity, &[]);
            builder.seal_block(bf_check_is_end_value);

            // bf_check_capacity: check if we need to grow
            builder.switch_to_block(bf_check_capacity);
            let count = builder.use_var(bf_counter_var);
            let capacity = builder.use_var(bf_capacity_var);
            let needs_grow = builder
                .ins()
                .icmp(IntCC::UnsignedGreaterThanOrEqual, count, capacity);
            builder.ins().brif(needs_grow, bf_grow, &[], bf_parse, &[]);
            builder.seal_block(bf_check_capacity);

            // bf_grow: reserve more capacity
            builder.switch_to_block(bf_grow);
            // Double capacity (or start with 16 if 0)
            let capacity = builder.use_var(bf_capacity_var);
            let sixteen = builder.ins().iconst(pointer_type, 16);
            let is_zero = builder.ins().icmp_imm(IntCC::Equal, capacity, 0);
            let doubled = builder.ins().ishl_imm(capacity, 1); // capacity * 2
            let new_capacity = builder.ins().select(is_zero, sixteen, doubled);

            // Call reserve
            let vec_reserve_ptr = builder
                .ins()
                .iconst(pointer_type, helpers::jit_vec_reserve as *const u8 as i64);
            let reserve_fn_ptr = builder.use_var(reserve_fn_var);
            builder.ins().call_indirect(
                sig_vec_reserve_ref,
                vec_reserve_ptr,
                &[out_ptr, new_capacity, reserve_fn_ptr],
            );

            // Get new base pointer (may have moved)
            let as_mut_ptr_fn_ptr = builder.use_var(as_mut_ptr_fn_var);
            let new_base_call = builder.ins().call_indirect(
                sig_vec_as_mut_ptr_typed_ref,
                vec_as_mut_ptr_ptr,
                &[out_ptr, as_mut_ptr_fn_ptr],
            );
            let new_base_ptr = builder.inst_results(new_base_call)[0];
            builder.def_var(bf_base_ptr_var, new_base_ptr);

            // Get new capacity
            let capacity_fn_ptr = builder.use_var(capacity_fn_var);
            let new_cap_call = builder.ins().call_indirect(
                sig_vec_capacity_ref,
                vec_capacity_ptr,
                &[out_ptr, capacity_fn_ptr],
            );
            let actual_new_cap = builder.inst_results(new_cap_call)[0];
            builder.def_var(bf_capacity_var, actual_new_cap);

            builder.ins().jump(bf_parse, &[]);
            builder.seal_block(bf_grow);

            // bf_parse: parse the next element
            builder.switch_to_block(bf_parse);
            let mut cursor = JitCursor {
                input_ptr,
                len,
                pos: pos_var,
                ptr_type: pointer_type,
                scratch_ptr,
            };
            let format = F::default();
            let (parsed_val, parse_err) = match elem_kind {
                FormatListElementKind::Bool => {
                    format.emit_parse_bool(module, &mut builder, &mut cursor)
                }
                FormatListElementKind::U8 => {
                    format.emit_parse_u8(module, &mut builder, &mut cursor)
                }
                FormatListElementKind::I64 => {
                    format.emit_parse_i64(module, &mut builder, &mut cursor)
                }
                FormatListElementKind::U64 => {
                    format.emit_parse_u64(module, &mut builder, &mut cursor)
                }
                _ => unreachable!("buffered direct-fill only for scalars"),
            };
            builder.def_var(parsed_value_var, parsed_val);
            builder.def_var(err_var, parse_err);
            builder.ins().jump(bf_check_parse_err, &[]);
            builder.seal_block(bf_parse);

            // bf_check_parse_err: check for parse error
            builder.switch_to_block(bf_check_parse_err);
            let parse_ok = builder.ins().icmp_imm(IntCC::Equal, parse_err, 0);
            builder.ins().brif(parse_ok, bf_store, &[], error, &[]);
            builder.seal_block(bf_check_parse_err);

            // bf_store: store parsed value directly into vec buffer
            builder.switch_to_block(bf_store);
            let parsed_val = builder.use_var(parsed_value_var);
            let base_ptr = builder.use_var(bf_base_ptr_var);
            let count = builder.use_var(bf_counter_var);

            // Calculate offset: base_ptr + count * elem_size
            let elem_size_val = builder.ins().iconst(pointer_type, elem_size as i64);
            let offset = builder.ins().imul(count, elem_size_val);
            let dest_ptr = builder.ins().iadd(base_ptr, offset);

            // Store value (extend to correct size if needed)
            let scalar_type = elem_shape.scalar_type().unwrap();
            let store_val = match scalar_type {
                ScalarType::Bool | ScalarType::U8 | ScalarType::I8 => parsed_val,
                ScalarType::I16 | ScalarType::U16 => builder.ins().ireduce(types::I16, parsed_val),
                ScalarType::I32 | ScalarType::U32 => builder.ins().ireduce(types::I32, parsed_val),
                ScalarType::I64 | ScalarType::U64 => parsed_val,
                _ => unreachable!("buffered direct-fill only for integers"),
            };
            builder
                .ins()
                .store(MemFlags::trusted(), store_val, dest_ptr, 0);

            // Increment counter
            let one = builder.ins().iconst(pointer_type, 1);
            let new_count = builder.ins().iadd(count, one);
            builder.def_var(bf_counter_var, new_count);

            builder.ins().jump(bf_seq_next, &[]);
            builder.seal_block(bf_store);

            // bf_seq_next: handle separator and loop
            builder.switch_to_block(bf_seq_next);
            let mut cursor = JitCursor {
                input_ptr,
                len,
                pos: pos_var,
                ptr_type: pointer_type,
                scratch_ptr,
            };
            let format = F::default();
            let seq_next_err = format.emit_seq_next(module, &mut builder, &mut cursor, state_ptr);
            builder.def_var(err_var, seq_next_err);
            builder.ins().jump(bf_check_seq_next_err, &[]);
            builder.seal_block(bf_seq_next);

            // bf_check_seq_next_err: check for seq_next error
            builder.switch_to_block(bf_check_seq_next_err);
            let seq_next_ok = builder.ins().icmp_imm(IntCC::Equal, seq_next_err, 0);
            builder
                .ins()
                .brif(seq_next_ok, bf_loop_check, &[], error, &[]);
            builder.seal_block(bf_check_seq_next_err);
            builder.seal_block(bf_loop_check); // Now seal - has back edge from bf_check_seq_next_err

            // bf_finalize: set the vec's length and go to success
            builder.switch_to_block(bf_finalize);
            let final_count = builder.use_var(bf_counter_var);
            let set_len_fn_ptr = builder.use_var(set_len_fn_var);
            let vec_set_len_ptr = builder
                .ins()
                .iconst(pointer_type, helpers::jit_vec_set_len as *const u8 as i64);
            builder.ins().call_indirect(
                sig_vec_set_len_ref,
                vec_set_len_ptr,
                &[out_ptr, final_count, set_len_fn_ptr],
            );
            builder.ins().jump(success, &[]);
            builder.seal_block(bf_finalize);

            // Seal unused push-based, counted direct-fill, and struct buffered blocks
            builder.seal_block(loop_check_end);
            builder.seal_block(df_setup);
            builder.seal_block(df_bulk_copy);
            builder.seal_block(df_bulk_copy_check_err);
            builder.seal_block(df_loop_check);
            builder.seal_block(df_parse);
            builder.seal_block(df_check_parse_err);
            builder.seal_block(df_store);
            builder.seal_block(df_finalize);
            builder.seal_block(bfs_setup);
            builder.seal_block(bfs_loop_check);
            builder.seal_block(bfs_check_is_end_err);
            builder.seal_block(bfs_check_is_end_value);
            builder.seal_block(bfs_check_capacity);
            builder.seal_block(bfs_grow);
            builder.seal_block(bfs_parse);
            builder.seal_block(bfs_check_parse_err);
            builder.seal_block(bfs_seq_next);
            builder.seal_block(bfs_check_seq_next_err);
            builder.seal_block(bfs_finalize);
        } else if use_buffered_direct_fill_struct {
            // =================================================================
            // Buffered direct-fill path for STRUCTS
            // Instead of deserializing to a stack slot and then pushing,
            // we deserialize directly into the Vec's buffer.
            // =================================================================

            let FormatListElementKind::Struct(struct_shape) = elem_kind else {
                unreachable!("use_buffered_direct_fill_struct requires Struct element");
            };

            // Compile the nested struct deserializer
            let struct_func_id = match F::STRUCT_ENCODING {
                crate::jit::StructEncoding::Map => {
                    compile_struct_format_deserializer::<F>(module, struct_shape, memo)?
                }
                crate::jit::StructEncoding::Positional => {
                    compile_struct_positional_deserializer::<F>(module, struct_shape, memo)?
                }
            };
            let struct_func_ref = module.declare_func_in_func(struct_func_id, builder.func);

            // Get struct layout info
            let struct_layout = struct_shape.layout.sized_layout().ok()?;
            let struct_size = struct_layout.size();

            // bfs_setup: get capacity, base_ptr, initialize counter
            builder.switch_to_block(bfs_setup);

            // Get vtable function pointers
            let set_len_fn_ptr = builder
                .ins()
                .iconst(pointer_type, set_len_fn.unwrap() as *const () as i64);
            let as_mut_ptr_fn_ptr = builder.ins().iconst(
                pointer_type,
                as_mut_ptr_typed_fn.unwrap() as *const () as i64,
            );
            let reserve_fn_ptr = builder
                .ins()
                .iconst(pointer_type, reserve_fn.unwrap() as *const () as i64);
            let capacity_fn_ptr = builder
                .ins()
                .iconst(pointer_type, capacity_fn.unwrap() as *const () as i64);

            // Store vtable pointers in variables (for use across blocks)
            let bfs_set_len_fn_var = builder.declare_var(pointer_type);
            builder.def_var(bfs_set_len_fn_var, set_len_fn_ptr);
            let bfs_as_mut_ptr_fn_var = builder.declare_var(pointer_type);
            builder.def_var(bfs_as_mut_ptr_fn_var, as_mut_ptr_fn_ptr);
            let bfs_reserve_fn_var = builder.declare_var(pointer_type);
            builder.def_var(bfs_reserve_fn_var, reserve_fn_ptr);
            let bfs_capacity_fn_var = builder.declare_var(pointer_type);
            builder.def_var(bfs_capacity_fn_var, capacity_fn_ptr);

            // Get initial capacity
            let vec_capacity_ptr = builder
                .ins()
                .iconst(pointer_type, helpers::jit_vec_capacity as *const u8 as i64);
            let cap_call = builder.ins().call_indirect(
                sig_vec_capacity_ref,
                vec_capacity_ptr,
                &[out_ptr, capacity_fn_ptr],
            );
            let initial_capacity = builder.inst_results(cap_call)[0];

            // Get initial base pointer
            let vec_as_mut_ptr_ptr = builder.ins().iconst(
                pointer_type,
                helpers::jit_vec_as_mut_ptr_typed as *const u8 as i64,
            );
            let base_call = builder.ins().call_indirect(
                sig_vec_as_mut_ptr_typed_ref,
                vec_as_mut_ptr_ptr,
                &[out_ptr, as_mut_ptr_fn_ptr],
            );
            let initial_base_ptr = builder.inst_results(base_call)[0];

            // Variables for loop: base_ptr, capacity, count
            let bfs_base_ptr_var = builder.declare_var(pointer_type);
            builder.def_var(bfs_base_ptr_var, initial_base_ptr);
            let bfs_capacity_var = builder.declare_var(pointer_type);
            builder.def_var(bfs_capacity_var, initial_capacity);
            let bfs_counter_var = builder.declare_var(pointer_type);
            let zero = builder.ins().iconst(pointer_type, 0);
            builder.def_var(bfs_counter_var, zero);

            builder.ins().jump(bfs_loop_check, &[]);
            builder.seal_block(bfs_setup);

            // bfs_loop_check: check if we've reached end of sequence
            builder.switch_to_block(bfs_loop_check);
            let mut cursor = JitCursor {
                input_ptr,
                len,
                pos: pos_var,
                ptr_type: pointer_type,
                scratch_ptr,
            };
            let format = F::default();
            let (is_end_i8, is_end_err) =
                format.emit_seq_is_end(module, &mut builder, &mut cursor, state_ptr);
            let is_end = builder.ins().uextend(pointer_type, is_end_i8);
            builder.def_var(is_end_var, is_end);
            builder.def_var(err_var, is_end_err);
            builder.ins().jump(bfs_check_is_end_err, &[]);
            // Note: bfs_loop_check sealed after bfs_check_seq_next_err (back edge)

            // bfs_check_is_end_err: check for error from seq_is_end
            builder.switch_to_block(bfs_check_is_end_err);
            let is_end_ok = builder.ins().icmp_imm(IntCC::Equal, is_end_err, 0);
            builder
                .ins()
                .brif(is_end_ok, bfs_check_is_end_value, &[], error, &[]);
            builder.seal_block(bfs_check_is_end_err);

            // bfs_check_is_end_value: check if end marker found
            builder.switch_to_block(bfs_check_is_end_value);
            let is_end_val = builder.use_var(is_end_var);
            let is_done = builder.ins().icmp_imm(IntCC::NotEqual, is_end_val, 0);
            builder
                .ins()
                .brif(is_done, bfs_finalize, &[], bfs_check_capacity, &[]);
            builder.seal_block(bfs_check_is_end_value);

            // bfs_check_capacity: check if we need to grow
            builder.switch_to_block(bfs_check_capacity);
            let count = builder.use_var(bfs_counter_var);
            let capacity = builder.use_var(bfs_capacity_var);
            let needs_grow = builder
                .ins()
                .icmp(IntCC::UnsignedGreaterThanOrEqual, count, capacity);
            builder
                .ins()
                .brif(needs_grow, bfs_grow, &[], bfs_parse, &[]);
            builder.seal_block(bfs_check_capacity);

            // bfs_grow: reserve more capacity
            builder.switch_to_block(bfs_grow);
            let capacity = builder.use_var(bfs_capacity_var);
            let sixteen = builder.ins().iconst(pointer_type, 16);
            let is_zero = builder.ins().icmp_imm(IntCC::Equal, capacity, 0);
            let doubled = builder.ins().ishl_imm(capacity, 1); // capacity * 2
            let new_capacity = builder.ins().select(is_zero, sixteen, doubled);

            // Call reserve
            let vec_reserve_ptr = builder
                .ins()
                .iconst(pointer_type, helpers::jit_vec_reserve as *const u8 as i64);
            let reserve_fn_ptr = builder.use_var(bfs_reserve_fn_var);
            builder.ins().call_indirect(
                sig_vec_reserve_ref,
                vec_reserve_ptr,
                &[out_ptr, new_capacity, reserve_fn_ptr],
            );

            // Get new base pointer (may have moved after realloc)
            let as_mut_ptr_fn_ptr = builder.use_var(bfs_as_mut_ptr_fn_var);
            let new_base_call = builder.ins().call_indirect(
                sig_vec_as_mut_ptr_typed_ref,
                vec_as_mut_ptr_ptr,
                &[out_ptr, as_mut_ptr_fn_ptr],
            );
            let new_base_ptr = builder.inst_results(new_base_call)[0];
            builder.def_var(bfs_base_ptr_var, new_base_ptr);

            // Get new capacity
            let capacity_fn_ptr = builder.use_var(bfs_capacity_fn_var);
            let new_cap_call = builder.ins().call_indirect(
                sig_vec_capacity_ref,
                vec_capacity_ptr,
                &[out_ptr, capacity_fn_ptr],
            );
            let actual_new_cap = builder.inst_results(new_cap_call)[0];
            builder.def_var(bfs_capacity_var, actual_new_cap);

            builder.ins().jump(bfs_parse, &[]);
            builder.seal_block(bfs_grow);

            // bfs_parse: parse the next struct element directly into the Vec buffer
            builder.switch_to_block(bfs_parse);
            let base_ptr = builder.use_var(bfs_base_ptr_var);
            let count = builder.use_var(bfs_counter_var);

            // Calculate destination: base_ptr + count * struct_size
            let struct_size_val = builder.ins().iconst(pointer_type, struct_size as i64);
            let offset = builder.ins().imul(count, struct_size_val);
            let dest_ptr = builder.ins().iadd(base_ptr, offset);

            // Call struct deserializer directly into dest_ptr
            let current_pos = builder.use_var(pos_var);
            let struct_func_ptr = func_addr_value(&mut builder, pointer_type, struct_func_ref);
            let call_result = builder.ins().call_indirect(
                nested_call_sig_ref,
                struct_func_ptr,
                &[input_ptr, len, current_pos, dest_ptr, scratch_ptr],
            );
            let new_pos = builder.inst_results(call_result)[0];

            // Check for error (new_pos < 0 means error, scratch already written)
            let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
            builder.ins().brif(
                is_error,
                nested_error_passthrough,
                &[],
                bfs_check_parse_err,
                &[],
            );
            builder.seal_block(bfs_parse);

            // bfs_check_parse_err: update position and increment counter
            builder.switch_to_block(bfs_check_parse_err);
            builder.def_var(pos_var, new_pos);

            // Increment counter
            let count = builder.use_var(bfs_counter_var);
            let one = builder.ins().iconst(pointer_type, 1);
            let new_count = builder.ins().iadd(count, one);
            builder.def_var(bfs_counter_var, new_count);

            builder.ins().jump(bfs_seq_next, &[]);
            builder.seal_block(bfs_check_parse_err);

            // bfs_seq_next: handle separator and loop
            builder.switch_to_block(bfs_seq_next);
            let mut cursor = JitCursor {
                input_ptr,
                len,
                pos: pos_var,
                ptr_type: pointer_type,
                scratch_ptr,
            };
            let format = F::default();
            let seq_next_err = format.emit_seq_next(module, &mut builder, &mut cursor, state_ptr);
            builder.def_var(err_var, seq_next_err);
            builder.ins().jump(bfs_check_seq_next_err, &[]);
            builder.seal_block(bfs_seq_next);

            // bfs_check_seq_next_err: check for seq_next error
            builder.switch_to_block(bfs_check_seq_next_err);
            let seq_next_ok = builder.ins().icmp_imm(IntCC::Equal, seq_next_err, 0);
            builder
                .ins()
                .brif(seq_next_ok, bfs_loop_check, &[], error, &[]);
            builder.seal_block(bfs_check_seq_next_err);
            builder.seal_block(bfs_loop_check); // Now seal - has back edge

            // bfs_finalize: set the vec's length and go to success
            builder.switch_to_block(bfs_finalize);
            let final_count = builder.use_var(bfs_counter_var);
            let set_len_fn_ptr = builder.use_var(bfs_set_len_fn_var);
            let vec_set_len_ptr = builder
                .ins()
                .iconst(pointer_type, helpers::jit_vec_set_len as *const u8 as i64);
            builder.ins().call_indirect(
                sig_vec_set_len_ref,
                vec_set_len_ptr,
                &[out_ptr, final_count, set_len_fn_ptr],
            );
            builder.ins().jump(success, &[]);
            builder.seal_block(bfs_finalize);

            // Seal unused push-based, counted direct-fill, and scalar buffered blocks
            builder.seal_block(loop_check_end);
            builder.seal_block(df_setup);
            builder.seal_block(df_bulk_copy);
            builder.seal_block(df_bulk_copy_check_err);
            builder.seal_block(df_loop_check);
            builder.seal_block(df_parse);
            builder.seal_block(df_check_parse_err);
            builder.seal_block(df_store);
            builder.seal_block(df_finalize);
            builder.seal_block(bf_setup);
            builder.seal_block(bf_loop_check);
            builder.seal_block(bf_check_is_end_err);
            builder.seal_block(bf_check_is_end_value);
            builder.seal_block(bf_check_capacity);
            builder.seal_block(bf_grow);
            builder.seal_block(bf_parse);
            builder.seal_block(bf_check_parse_err);
            builder.seal_block(bf_store);
            builder.seal_block(bf_seq_next);
            builder.seal_block(bf_check_seq_next_err);
            builder.seal_block(bf_finalize);
        } else {
            // Seal unused direct-fill and buffered blocks (push-based mode)
            builder.seal_block(df_setup);
            builder.seal_block(df_bulk_copy);
            builder.seal_block(df_bulk_copy_check_err);
            builder.seal_block(df_loop_check);
            builder.seal_block(df_parse);
            builder.seal_block(df_check_parse_err);
            builder.seal_block(df_store);
            builder.seal_block(df_finalize);
            builder.seal_block(bf_setup);
            builder.seal_block(bf_loop_check);
            builder.seal_block(bf_check_is_end_err);
            builder.seal_block(bf_check_is_end_value);
            builder.seal_block(bf_check_capacity);
            builder.seal_block(bf_grow);
            builder.seal_block(bf_parse);
            builder.seal_block(bf_check_parse_err);
            builder.seal_block(bf_store);
            builder.seal_block(bf_seq_next);
            builder.seal_block(bf_check_seq_next_err);
            builder.seal_block(bf_finalize);
            builder.seal_block(bfs_setup);
            builder.seal_block(bfs_loop_check);
            builder.seal_block(bfs_check_is_end_err);
            builder.seal_block(bfs_check_is_end_value);
            builder.seal_block(bfs_check_capacity);
            builder.seal_block(bfs_grow);
            builder.seal_block(bfs_parse);
            builder.seal_block(bfs_check_parse_err);
            builder.seal_block(bfs_seq_next);
            builder.seal_block(bfs_check_seq_next_err);
            builder.seal_block(bfs_finalize);
        }

        // success: return new position
        builder.switch_to_block(success);
        let final_pos = builder.use_var(pos_var);
        builder.ins().return_(&[final_pos]);
        builder.seal_block(success);

        // error: write scratch and return negative
        builder.switch_to_block(error);
        let err_code = builder.use_var(err_var); // Use actual error code from helper
        let err_pos = builder.use_var(pos_var);
        // Write error_code to scratch
        builder.ins().store(
            MemFlags::trusted(),
            err_code,
            scratch_ptr,
            JIT_SCRATCH_ERROR_CODE_OFFSET,
        );
        // Write error_pos to scratch
        builder.ins().store(
            MemFlags::trusted(),
            err_pos,
            scratch_ptr,
            JIT_SCRATCH_ERROR_POS_OFFSET,
        );
        let neg_one = builder.ins().iconst(pointer_type, -1i64);
        builder.ins().return_(&[neg_one]);
        builder.seal_block(error);

        // Seal nested_error_passthrough at the very end, after all paths that may branch to it
        builder.seal_block(nested_error_passthrough);

        builder.finalize();
    }

    if let Err(_e) = module.define_function(func_id, &mut ctx) {
        jit_debug!("[compile_list] define_function failed: {:?}", _e);
        return None;
    }

    jit_debug!("[compile_list] SUCCESS - function compiled");
    Some(func_id)
}
