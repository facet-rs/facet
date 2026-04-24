// r[impl schema.translation.skip-unknown]
// r[impl schema.translation.reorder]
// r[impl schema.translation.enum]
// r[impl schema.translation.enum.unknown-variant]
// r[impl schema.errors.unknown-variant-runtime]
/// Compact execution IR for postcard decode.
///
/// This IR sits between `TranslationPlan` (semantic) and Cranelift codegen
/// (machine). It is intended to be:
///
/// - fully concrete (no shape reflection at interpret time)
/// - small (flat instruction sequences with explicit operands)
/// - safe to interpret from a pure-Rust fallback path
///
/// The IR does NOT mention `Peek`, `Partial`, or any facet_reflect primitive.
/// All layout knowledge is baked in at lowering time.
use std::collections::HashMap;

use facet_core::{EnumRepr, Facet, ScalarType, Shape, Type, UserType};
use vox_jit_cal::{BorrowMode, CalibrationRegistry, DescriptorHandle};
use vox_schema::{SchemaKind, SchemaRegistry};

use crate::error::DeserializeError;
use crate::plan::{FieldOp, TranslationPlan};

// ---------------------------------------------------------------------------
// Descriptor handles (opaque types — filled in by calibration-engineer)
// ---------------------------------------------------------------------------

/// Handle to a calibrated opaque-type descriptor (e.g. Vec<T>, String).
///
/// The concrete descriptor type is owned by the calibration subsystem.
/// The IR stores only the handle; the interpreter resolves it at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpaqueDescriptorId(pub u32);

impl From<DescriptorHandle> for OpaqueDescriptorId {
    fn from(h: DescriptorHandle) -> Self {
        OpaqueDescriptorId(h.0)
    }
}

impl From<OpaqueDescriptorId> for DescriptorHandle {
    fn from(id: OpaqueDescriptorId) -> Self {
        DescriptorHandle(id.0)
    }
}

// ---------------------------------------------------------------------------
// Scalar primitive tag (no reflection at interpret time)
// ---------------------------------------------------------------------------

/// Wire-level primitive that can be decoded without any shape information.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WirePrimitive {
    Unit,
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    USize,
    I8,
    I16,
    I32,
    I64,
    I128,
    ISize,
    F32,
    F64,
    /// varint-length-prefixed UTF-8 string
    String,
    /// varint-length-prefixed byte buffer
    Bytes,
    /// u32le-length-prefixed opaque payload
    Payload,
    /// char encoded as a length-1 UTF-8 string
    Char,
}

impl WirePrimitive {
    /// Convert from a facet ScalarType where possible.
    pub fn from_scalar(s: ScalarType) -> Option<Self> {
        Some(match s {
            ScalarType::Unit => Self::Unit,
            ScalarType::Bool => Self::Bool,
            ScalarType::U8 => Self::U8,
            ScalarType::U16 => Self::U16,
            ScalarType::U32 => Self::U32,
            ScalarType::U64 => Self::U64,
            ScalarType::U128 => Self::U128,
            ScalarType::USize => Self::USize,
            ScalarType::I8 => Self::I8,
            ScalarType::I16 => Self::I16,
            ScalarType::I32 => Self::I32,
            ScalarType::I64 => Self::I64,
            ScalarType::I128 => Self::I128,
            ScalarType::ISize => Self::ISize,
            ScalarType::F32 => Self::F32,
            ScalarType::F64 => Self::F64,
            ScalarType::String => Self::String,
            ScalarType::Str => Self::String,
            ScalarType::CowStr => Self::String,
            ScalarType::Char => Self::Char,
            _ => return None,
        })
    }
}

// ---------------------------------------------------------------------------
// Tag width
// ---------------------------------------------------------------------------

/// Width (in bytes) of an enum discriminant on the wire / in memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagWidth {
    U8,
    U16,
    U32,
    U64,
}

impl TagWidth {
    pub fn from_enum_repr(repr: EnumRepr) -> Option<Self> {
        match repr {
            EnumRepr::U8 | EnumRepr::I8 => Some(TagWidth::U8),
            EnumRepr::U16 | EnumRepr::I16 => Some(TagWidth::U16),
            EnumRepr::U32 | EnumRepr::I32 => Some(TagWidth::U32),
            EnumRepr::U64 | EnumRepr::I64 | EnumRepr::USize | EnumRepr::ISize => {
                Some(TagWidth::U64)
            }
            EnumRepr::Rust | EnumRepr::RustNPO => None,
        }
    }

    pub fn byte_size(self) -> usize {
        match self {
            TagWidth::U8 => 1,
            TagWidth::U16 => 2,
            TagWidth::U32 => 4,
            TagWidth::U64 => 8,
        }
    }
}

// ---------------------------------------------------------------------------
// IR instruction set
// ---------------------------------------------------------------------------

/// A single IR instruction for the decode path.
///
/// Instructions operate on an implicit cursor (input bytes) and write to
/// an implicit destination pointer (`*mut u8` base + field offsets). The
/// interpreter tracks both.
///
/// Operand convention:
///   - `dst_offset`: byte offset from the base of the struct being written
///   - `block_id`:   index into `DecodeProgram::blocks`
#[derive(Debug, Clone)]
pub enum DecodeOp {
    // -----------------------------------------------------------------------
    // Primitive reads
    // -----------------------------------------------------------------------
    /// Read a scalar primitive from the cursor and write it to `dst_offset`
    /// in the current destination.
    ReadScalar {
        prim: WirePrimitive,
        dst_offset: usize,
    },

    /// Read a varint-length-prefixed byte slice and copy the bytes to
    /// `Vec<u8>` at `dst_offset` (uses opaque descriptor for allocation).
    ReadByteVec {
        dst_offset: usize,
        descriptor: OpaqueDescriptorId,
    },

    /// Read a varint-length-prefixed UTF-8 string into `String` at
    /// `dst_offset` (uses opaque descriptor for allocation).
    ReadString {
        dst_offset: usize,
        descriptor: OpaqueDescriptorId,
    },

    /// Read a varint-length-prefixed UTF-8 string into `Cow<str>` at
    /// `dst_offset`.
    ReadCowStr { dst_offset: usize, borrowed: bool },

    /// Read a varint-length-prefixed UTF-8 string into `&str` at `dst_offset`.
    ReadStrRef { dst_offset: usize },

    /// Read a varint-length-prefixed byte slice and initialize `Cow<[u8]>`
    /// at `dst_offset`.
    ReadCowByteSlice { dst_offset: usize, borrowed: bool },

    /// Read a varint-length-prefixed byte slice and initialize `&[u8]`
    /// at `dst_offset`.
    ReadByteSliceRef { dst_offset: usize },

    /// Read a u32le-length-prefixed opaque payload and initialize the target
    /// via the shape's opaque adapter.
    ReadOpaque {
        shape: &'static Shape,
        dst_offset: usize,
    },

    // -----------------------------------------------------------------------
    // Skip operations (remote fields absent in local type)
    // -----------------------------------------------------------------------

    // r[impl schema.translation.skip-unknown]
    /// Skip one postcard value described by a pre-resolved schema kind.
    /// The kind is stored inline so the interpreter does not need a registry.
    SkipValue { kind: SchemaKind },

    // r[impl schema.translation.fill-defaults]
    /// Initialize a local field via its `Default` implementation. Emitted for
    /// every local struct field that has no corresponding remote field on the
    /// wire (schema evolution: remote dropped a field that has a default on
    /// the local side).
    WriteDefault {
        shape: &'static Shape,
        dst_offset: usize,
    },

    // -----------------------------------------------------------------------
    // Option handling
    // -----------------------------------------------------------------------
    /// Decode an `Option<T>` in-place.
    ///
    /// Reads the tag byte (0 = None, 1 = Some).
    ///
    /// - On None: calls `init_none` via vtable to write the None representation
    ///   into `dst_offset`. Does NOT branch; execution continues inline.
    /// - On Some: calls `init_some_and_get_inner` to write the Some tag and
    ///   obtain the pointer to the inner value slot, then jumps to `some_block`
    ///   with the inner pointer as the new base.
    ///
    /// `none_init_fn` and `some_init_fn` are the vtable function pointers
    /// captured at lowering time so the Cranelift backend can embed them as
    /// immediate constants.
    ///
    /// `inner_offset` is the byte offset from the `Option` base to the inner
    /// value slot (determined by probing the vtable at lowering time). Used by
    /// the interpreter to compute the write address directly.
    DecodeOption {
        dst_offset: usize,
        inner_offset: usize,
        some_block: usize,
        /// Exact bytes of a calibrated `None` value.
        none_bytes: Box<[u8]>,
        /// Exact bytes of a calibrated `Some(_)` value before the payload is
        /// overwritten by the inner decode.
        some_bytes: Box<[u8]>,
    },

    /// Decode a `Result<T, E>` in-place.
    ///
    /// Reads the postcard variant index (`0 = Ok`, `1 = Err`) as a varint,
    /// decodes the selected inner value into a temporary buffer, then moves it
    /// into the destination result via the result vtable.
    DecodeResult {
        dst_offset: usize,
        ok_block: usize,
        err_block: usize,
        ok_offset: usize,
        err_offset: usize,
        /// Exact bytes of a calibrated `Ok(_)` value before the payload is
        /// overwritten by the inner decode.
        ok_bytes: Box<[u8]>,
        /// Exact bytes of a calibrated `Err(_)` value before the payload is
        /// overwritten by the inner decode.
        err_bytes: Box<[u8]>,
    },

    /// Decode a `Result<T, E>` via payload scratch storage, then initialize the
    /// destination with the result vtable. This is used when payload types do
    /// not provide defaults, so calibrated direct-write templates cannot be
    /// built safely.
    DecodeResultInit {
        dst_offset: usize,
        ok_block: usize,
        err_block: usize,
        ok_size: usize,
        ok_align: usize,
        err_size: usize,
        err_align: usize,
        init_ok_fn: facet_core::ResultInitOkFn,
        init_err_fn: facet_core::ResultInitErrFn,
    },

    // -----------------------------------------------------------------------
    // Enum handling
    // -----------------------------------------------------------------------
    /// Read the varint enum discriminant from the wire.
    ReadDiscriminant,

    // r[impl schema.translation.enum]
    // r[impl schema.translation.enum.unknown-variant]
    /// Map remote discriminant (held in the interpreter's scratch register)
    /// to a local variant index.  `variant_table[remote_disc]` is the local
    /// index, or `None` for unknown variants (runtime error).
    ///
    /// On match: writes the local discriminant tag bytes to `tag_offset` and
    /// jumps to the block for that variant.
    BranchOnVariant {
        tag_offset: usize,
        tag_width: TagWidth,
        /// variant_table[remote_index] = Some(local_index) or None
        variant_table: Vec<Option<usize>>,
        /// per-variant: (local_discriminant_value, block_id)
        variant_blocks: Vec<(u64, usize)>,
    },

    // -----------------------------------------------------------------------
    // Struct / tuple field handling
    // -----------------------------------------------------------------------
    /// Push a new stack frame. The new frame's base pointer is
    /// `current_base + field_offset`, size is `frame_size`.
    /// After `PopFrame` the interpreter returns to the parent base.
    PushFrame {
        field_offset: usize,
        frame_size: usize,
    },

    /// Return to the parent frame (mirrors `PushFrame`).
    PopFrame,

    // -----------------------------------------------------------------------
    // List / array handling
    // -----------------------------------------------------------------------
    /// Read the varint element count. Branch to `empty_block` if zero,
    /// otherwise call `alloc_block` to allocate backing storage then execute
    /// `body_block` for each element.
    ReadListLen {
        descriptor: OpaqueDescriptorId,
        dst_offset: usize,
        empty_block: usize,
        body_block: usize,
    },

    /// Commit the current element count to the `len` field of the list
    /// backing at `dst_offset`. Called after each element is successfully
    /// decoded.
    CommitListLen {
        dst_offset: usize,
        descriptor: OpaqueDescriptorId,
    },

    /// Decode a fixed-count array (length known at lowering time).
    /// Repeats `body_block` exactly `count` times, advancing by `elem_size`
    /// each iteration.
    DecodeArray {
        dst_offset: usize,
        count: usize,
        elem_size: usize,
        body_block: usize,
    },

    // -----------------------------------------------------------------------
    // Opaque fast-path
    // -----------------------------------------------------------------------
    /// Copy the calibrated empty-value bytes for an opaque type into the
    /// destination at `dst_offset`. Used for zero-length lists and strings.
    MaterializeEmpty {
        dst_offset: usize,
        descriptor: OpaqueDescriptorId,
    },

    /// Allocate backing storage for a Vec/String and write the fat-pointer
    /// fields to `dst_offset`. The capacity is in the interpreter's len
    /// register (set by `ReadListLen`).
    ///
    /// `body_block` contains the element decode ops (base = element pointer).
    /// `elem_size` is the stride between elements in the backing allocation.
    /// The Cranelift backend emits the element loop inline using these fields;
    /// the IR interpreter ignores them (it falls back via SlowPath for lists).
    AllocBacking {
        dst_offset: usize,
        descriptor: OpaqueDescriptorId,
        /// IR block containing element decode ops (dst_offset=0, base=elem ptr).
        body_block: usize,
        /// Byte stride between elements in backing storage.
        elem_size: usize,
    },

    /// Allocate a single heap slot for a `Box<T>` and decode the pointee into it.
    ///
    /// Calls `vox_jit_box_alloc(desc, container_ptr)` to allocate and write the
    /// pointer into `dst_offset`. Then decodes the inner type using `body_block`
    /// with the allocated pointer as the new base (`dst_offset=0`).
    AllocBoxed {
        dst_offset: usize,
        descriptor: OpaqueDescriptorId,
        /// IR block containing ops to decode the pointee (dst_offset=0, base=alloc_ptr).
        body_block: usize,
    },

    // -----------------------------------------------------------------------
    // Slow path
    // -----------------------------------------------------------------------

    // r[impl schema.exchange.required]
    /// Fall back to the reflective interpreter for a shape that the IR
    /// cannot lower. The interpreter recognises this instruction and
    /// invokes the reflective path with the embedded plan.
    SlowPath {
        shape: &'static Shape,
        plan: Box<TranslationPlan>,
        dst_offset: usize,
    },

    // -----------------------------------------------------------------------
    // Control flow
    // -----------------------------------------------------------------------
    /// Unconditional jump to `block_id`.
    Jump { block_id: usize },

    /// End of the current block — execution falls through to the next
    /// instruction in the parent context (return from block call).
    Return,
}

// ---------------------------------------------------------------------------
// Basic block
// ---------------------------------------------------------------------------

/// A linear sequence of `DecodeOp` instructions.
#[derive(Debug, Clone, Default)]
pub struct DecodeBlock {
    pub ops: Vec<DecodeOp>,
}

// ---------------------------------------------------------------------------
// Program
// ---------------------------------------------------------------------------

/// A fully-lowered decode program for one root type.
///
/// Block 0 is always the entry point.
#[derive(Debug, Clone)]
pub struct DecodeProgram {
    pub blocks: Vec<DecodeBlock>,
    /// Total size in bytes of the root destination struct.
    pub root_size: usize,
    /// Required alignment of the root destination.
    pub root_align: usize,
}

impl DecodeProgram {
    fn new_block(&mut self) -> usize {
        let id = self.blocks.len();
        self.blocks.push(DecodeBlock::default());
        id
    }

    fn emit(&mut self, block: usize, op: DecodeOp) {
        self.blocks[block].ops.push(op);
    }
}

// ---------------------------------------------------------------------------
// Lowering: TranslationPlan + Shape → DecodeProgram          (Task #2)
// ---------------------------------------------------------------------------

/// Error returned by the lowering pass.
#[derive(Debug)]
pub enum LowerError {
    /// The shape does not have a known sized layout.
    UnsizedShape,
    /// The enum representation is not stable (Rust or NPO repr).
    UnstableEnumRepr,
    /// A required schema lookup failed during skip-op construction.
    SchemaMissing,
}

// r[impl schema.errors.early-detection]
/// Lower a validated `TranslationPlan` and corresponding local `Shape` into a
/// `DecodeProgram`.
///
/// `registry` is the *remote* schema registry, used only to resolve skip-op
/// schema kinds for fields that exist on the remote side but not locally.
///
/// The plan must already be validated (no compatibility errors). This pass
/// does NOT re-check structural compatibility.
pub fn lower(
    plan: &TranslationPlan,
    shape: &'static Shape,
    registry: &SchemaRegistry,
) -> Result<DecodeProgram, LowerError> {
    lower_with_cal(plan, shape, registry, None, BorrowMode::Owned)
}

/// Like `lower` but with an optional calibration registry.
///
/// When `cal` is provided, `Vec<T>` types whose element shape has been
/// pre-registered in the registry (via `register_for_shape`) will be lowered
/// to `ReadListLen` + `AllocBacking` + element loop + `CommitListLen` instead
/// of `SlowPath`.
pub fn lower_with_cal(
    plan: &TranslationPlan,
    shape: &'static Shape,
    registry: &SchemaRegistry,
    cal: Option<&CalibrationRegistry>,
    borrow_mode: BorrowMode,
) -> Result<DecodeProgram, LowerError> {
    let layout = shape
        .layout
        .sized_layout()
        .map_err(|_| LowerError::UnsizedShape)?;
    let mut program = DecodeProgram {
        blocks: vec![DecodeBlock::default()],
        root_size: layout.size(),
        root_align: layout.align(),
    };

    let entry = 0;
    lower_value(
        plan,
        shape,
        registry,
        cal,
        borrow_mode,
        &mut program,
        entry,
        0,
    )?;
    program.emit(entry, DecodeOp::Return);

    Ok(program)
}

fn lower_value(
    plan: &TranslationPlan,
    shape: &'static Shape,
    registry: &SchemaRegistry,
    cal: Option<&CalibrationRegistry>,
    borrow_mode: BorrowMode,
    program: &mut DecodeProgram,
    block: usize,
    dst_offset: usize,
) -> Result<(), LowerError> {
    // `Result<T, E>` is exposed as an opaque/proxy-like user shape by Facet,
    // but its postcard ABI is structural. Route it by Def before generic
    // proxy/opaque handling.
    if let facet_core::Def::Result(_) = shape.def {
        return lower_def(
            plan,
            shape,
            registry,
            cal,
            borrow_mode,
            program,
            block,
            dst_offset,
        );
    }

    if shape.opaque_adapter.is_some() {
        program.emit(block, DecodeOp::ReadOpaque { shape, dst_offset });
        return Ok(());
    }

    if shape.proxy.is_some() {
        program.emit(
            block,
            DecodeOp::SlowPath {
                shape,
                plan: Box::new(clone_plan(plan)),
                dst_offset,
            },
        );
        return Ok(());
    }

    // Transparent wrappers — pass through to inner shape
    if shape.is_transparent() {
        if let Type::User(UserType::Struct(st)) = shape.ty
            && let Some(inner_field) = st.fields.first()
        {
            let inner_shape = inner_field.shape();
            return lower_value(
                plan,
                inner_shape,
                registry,
                cal,
                borrow_mode,
                program,
                block,
                dst_offset,
            );
        }
        // Transparent wrapper with no first field (e.g. dynamically generated
        // transparent struct with no inner slot) — SlowPath by design.
        program.emit(
            block,
            DecodeOp::SlowPath {
                shape,
                plan: Box::new(clone_plan(plan)),
                dst_offset,
            },
        );
        return Ok(());
    }

    // Scalars
    if let Some(scalar) = shape.scalar_type() {
        match scalar {
            facet_core::ScalarType::String => {
                if shape.is_type::<String>()
                    && let Some(cal) = cal
                    && let Some(handle) = cal.string_descriptor_handle()
                {
                    program.emit(
                        block,
                        DecodeOp::ReadString {
                            dst_offset,
                            descriptor: OpaqueDescriptorId(handle.0),
                        },
                    );
                    return Ok(());
                }
            }
            facet_core::ScalarType::CowStr => {
                program.emit(
                    block,
                    DecodeOp::ReadCowStr {
                        dst_offset,
                        borrowed: borrow_mode == BorrowMode::Borrowed,
                    },
                );
                return Ok(());
            }
            facet_core::ScalarType::Str if borrow_mode == BorrowMode::Borrowed => {
                program.emit(block, DecodeOp::ReadStrRef { dst_offset });
                return Ok(());
            }
            _ => {}
        }

        if let Some(prim) = WirePrimitive::from_scalar(scalar) {
            // String scalars: use ReadString (calibrated) when possible so the
            // JIT can emit the fast allocation path instead of calling SlowPath.
            if matches!(prim, WirePrimitive::String)
                && let Some(cal) = cal
                && let Some(handle) = cal.string_descriptor_handle()
            {
                program.emit(
                    block,
                    DecodeOp::ReadString {
                        dst_offset,
                        descriptor: OpaqueDescriptorId(handle.0),
                    },
                );
                return Ok(());
            }
            program.emit(block, DecodeOp::ReadScalar { prim, dst_offset });
            return Ok(());
        }
        // Scalar kind not representable as a postcard primitive
        // (e.g. SocketAddr, IpAddr, ConstTypeId) — SlowPath by design.
        // These types have no canonical postcard encoding; the reflective
        // interpreter handles them via their own vtable deserialize paths.
        program.emit(
            block,
            DecodeOp::SlowPath {
                shape,
                plan: Box::new(clone_plan(plan)),
                dst_offset,
            },
        );
        return Ok(());
    }

    match shape.def {
        facet_core::Def::Option(_)
        | facet_core::Def::Array(_)
        | facet_core::Def::List(_)
        | facet_core::Def::Pointer(_) => {
            return lower_def(
                plan,
                shape,
                registry,
                cal,
                borrow_mode,
                program,
                block,
                dst_offset,
            );
        }
        _ => {}
    }

    // User types
    match shape.ty {
        Type::User(UserType::Struct(st)) => lower_struct(
            plan,
            st,
            registry,
            cal,
            borrow_mode,
            program,
            block,
            dst_offset,
        ),
        Type::User(UserType::Enum(et)) => lower_enum(
            plan,
            shape,
            et,
            registry,
            cal,
            borrow_mode,
            program,
            block,
            dst_offset,
        ),
        _ => lower_def(
            plan,
            shape,
            registry,
            cal,
            borrow_mode,
            program,
            block,
            dst_offset,
        ),
    }
}

fn lower_def(
    plan: &TranslationPlan,
    shape: &'static Shape,
    registry: &SchemaRegistry,
    cal: Option<&CalibrationRegistry>,
    borrow_mode: BorrowMode,
    program: &mut DecodeProgram,
    block: usize,
    dst_offset: usize,
) -> Result<(), LowerError> {
    use facet_core::Def;

    match shape.def {
        Def::Option(opt_def) => lower_option(
            plan,
            shape,
            opt_def,
            registry,
            cal,
            borrow_mode,
            program,
            block,
            dst_offset,
        ),
        Def::Array(arr_def) => lower_array(
            plan,
            arr_def,
            registry,
            cal,
            borrow_mode,
            program,
            block,
            dst_offset,
        ),
        Def::List(list_def) => lower_list(
            plan,
            shape,
            list_def,
            registry,
            cal,
            borrow_mode,
            program,
            block,
            dst_offset,
        ),
        Def::Pointer(ptr_def) => lower_pointer(
            plan,
            shape,
            ptr_def,
            registry,
            cal,
            borrow_mode,
            program,
            block,
            dst_offset,
        ),
        Def::Result(result_def) => lower_result(
            plan,
            shape,
            result_def,
            registry,
            cal,
            borrow_mode,
            program,
            block,
            dst_offset,
        ),
        _ => {
            // Def::Map, Def::Set, Def::Slice, Def::NdArray,
            // Def::DynamicValue, Def::Undefined — SlowPath by design.
            //
            // Set/Map: same postcard wire encoding as List (varint len + elements),
            // but insertion requires vtable calls (SetVTable::insert /
            // MapVTable::insert). The IR has no "insert into container" op;
            // adding one would require per-element vtable dispatch that negates JIT
            // benefits. The spec §Non-Goals does not require JIT for these.
            //
            // Slice: unsized — cannot be placed on the stack by the IR.
            // NdArray/DynamicValue/Undefined: no postcard ABI defined.
            program.emit(
                block,
                DecodeOp::SlowPath {
                    shape,
                    plan: Box::new(clone_plan(plan)),
                    dst_offset,
                },
            );
            Ok(())
        }
    }
}

fn lower_result(
    plan: &TranslationPlan,
    shape: &'static Shape,
    result_def: facet_core::ResultDef,
    registry: &SchemaRegistry,
    cal: Option<&CalibrationRegistry>,
    borrow_mode: BorrowMode,
    program: &mut DecodeProgram,
    block: usize,
    dst_offset: usize,
) -> Result<(), LowerError> {
    let (ok_plan, err_plan) = match plan {
        TranslationPlan::Enum { nested, .. } => (
            nested.get(&0).unwrap_or(&TranslationPlan::Identity),
            nested.get(&1).unwrap_or(&TranslationPlan::Identity),
        ),
        TranslationPlan::Identity => (&TranslationPlan::Identity, &TranslationPlan::Identity),
        _ => {
            program.emit(
                block,
                DecodeOp::SlowPath {
                    shape,
                    plan: Box::new(clone_plan(plan)),
                    dst_offset,
                },
            );
            return Ok(());
        }
    };

    let ok_block = program.new_block();
    let err_block = program.new_block();

    if let Some(layout) = calibrate_result_layout(shape, result_def) {
        program.emit(
            block,
            DecodeOp::DecodeResult {
                dst_offset,
                ok_block,
                err_block,
                ok_offset: layout.ok_offset,
                err_offset: layout.err_offset,
                ok_bytes: layout.ok_bytes,
                err_bytes: layout.err_bytes,
            },
        );
    } else {
        let Ok(ok_layout) = result_def.t.layout.sized_layout() else {
            program.emit(
                block,
                DecodeOp::SlowPath {
                    shape,
                    plan: Box::new(clone_plan(plan)),
                    dst_offset,
                },
            );
            return Ok(());
        };
        let Ok(err_layout) = result_def.e.layout.sized_layout() else {
            program.emit(
                block,
                DecodeOp::SlowPath {
                    shape,
                    plan: Box::new(clone_plan(plan)),
                    dst_offset,
                },
            );
            return Ok(());
        };

        program.emit(
            block,
            DecodeOp::DecodeResultInit {
                dst_offset,
                ok_block,
                err_block,
                ok_size: ok_layout.size(),
                ok_align: ok_layout.align(),
                err_size: err_layout.size(),
                err_align: err_layout.align(),
                init_ok_fn: result_def.vtable.init_ok,
                init_err_fn: result_def.vtable.init_err,
            },
        );
    }

    lower_value(
        ok_plan,
        result_def.t,
        registry,
        cal,
        borrow_mode,
        program,
        ok_block,
        0,
    )?;
    program.emit(ok_block, DecodeOp::Return);

    lower_value(
        err_plan,
        result_def.e,
        registry,
        cal,
        borrow_mode,
        program,
        err_block,
        0,
    )?;
    program.emit(err_block, DecodeOp::Return);

    Ok(())
}

// r[impl schema.translation.reorder]
// r[impl schema.translation.skip-unknown]
fn lower_struct(
    plan: &TranslationPlan,
    st: facet_core::StructType,
    registry: &SchemaRegistry,
    cal: Option<&CalibrationRegistry>,
    borrow_mode: BorrowMode,
    program: &mut DecodeProgram,
    block: usize,
    dst_offset: usize,
) -> Result<(), LowerError> {
    let (field_ops, nested) = match plan {
        TranslationPlan::Struct { field_ops, nested }
        | TranslationPlan::Tuple { field_ops, nested } => (field_ops.as_slice(), nested),
        TranslationPlan::Identity => {
            let identity = build_identity_plan_for_struct(st);
            return lower_struct(
                &identity,
                st,
                registry,
                cal,
                borrow_mode,
                program,
                block,
                dst_offset,
            );
        }
        _ => {
            // A validated plan for a struct is always Struct | Tuple | Identity.
            // Any other variant indicates a bug in the caller.
            return Err(LowerError::SchemaMissing);
        }
    };

    let mut matched = vec![false; st.fields.len()];
    for op in field_ops {
        match op {
            FieldOp::Read { local_index } => {
                matched[*local_index] = true;
                let field = &st.fields[*local_index];
                let field_shape = field.shape();
                let field_offset = dst_offset + field.offset;
                let sub_plan = nested
                    .get(local_index)
                    .unwrap_or(&TranslationPlan::Identity);
                lower_value(
                    sub_plan,
                    field_shape,
                    registry,
                    cal,
                    borrow_mode,
                    program,
                    block,
                    field_offset,
                )?;
            }
            FieldOp::Skip { type_ref } => {
                let kind = type_ref
                    .resolve_kind(registry)
                    .ok_or(LowerError::SchemaMissing)?;
                program.emit(block, DecodeOp::SkipValue { kind });
            }
        }
    }

    // r[impl schema.translation.fill-defaults]
    // Local fields with no corresponding remote field need a Default-fill:
    // plan-build has already verified they're not required (i.e. have a
    // `#[facet(default)]` attribute), so `call_default_in_place` will succeed.
    for (i, field) in st.fields.iter().enumerate() {
        if !matched[i] {
            program.emit(
                block,
                DecodeOp::WriteDefault {
                    shape: field.shape(),
                    dst_offset: dst_offset + field.offset,
                },
            );
        }
    }

    Ok(())
}

// r[impl schema.translation.enum]
// r[impl schema.translation.enum.unknown-variant]
// r[impl schema.translation.enum.payload-compat]
fn lower_enum(
    plan: &TranslationPlan,
    shape: &'static Shape,
    et: facet_core::EnumType,
    registry: &SchemaRegistry,
    cal: Option<&CalibrationRegistry>,
    borrow_mode: BorrowMode,
    program: &mut DecodeProgram,
    block: usize,
    dst_offset: usize,
) -> Result<(), LowerError> {
    // EnumRepr::Rust and RustNPO have compiler-chosen discriminant layout; we
    // cannot emit a BranchOnVariant op. Emit SlowPath for the whole enum value
    // (e.g. Option<T>, which has RustNPO/Rust repr) so the field decodes via the
    // reflective interpreter without aborting the entire stub compilation.
    let Some(tag_width) = TagWidth::from_enum_repr(et.enum_repr) else {
        program.emit(
            block,
            DecodeOp::SlowPath {
                shape,
                plan: Box::new(clone_plan(plan)),
                dst_offset,
            },
        );
        return Ok(());
    };

    let (variant_map, variant_plans, nested) = match plan {
        TranslationPlan::Enum {
            variant_map,
            variant_plans,
            nested,
        } => (variant_map, variant_plans, nested),
        TranslationPlan::Identity => {
            let identity = crate::build_identity_plan(shape);
            return lower_enum(
                &identity,
                shape,
                et,
                registry,
                cal,
                borrow_mode,
                program,
                block,
                dst_offset,
            );
        }
        _ => {
            // Non-Enum plan for an enum type — SlowPath by design.
            // Only `TranslationPlan::Enum { .. }` carries variant_map and
            // variant_plans, which are required to emit `BranchOnVariant`.
            // Any other plan variant (e.g. a bare Identity) signals that the
            // plan was built without enum awareness and must use the reflective
            // interpreter.
            program.emit(
                block,
                DecodeOp::SlowPath {
                    shape,
                    plan: Box::new(clone_plan(plan)),
                    dst_offset,
                },
            );
            return Ok(());
        }
    };

    // Emit discriminant read
    program.emit(block, DecodeOp::ReadDiscriminant);

    // Build per-variant blocks
    let mut variant_blocks: Vec<(u64, usize)> = Vec::new();
    for (remote_idx, maybe_local) in variant_map.iter().enumerate() {
        let Some(local_idx) = maybe_local else {
            // Unknown remote variant → push sentinel (interpreter errors at runtime)
            variant_blocks.push((u64::MAX, usize::MAX));
            continue;
        };
        let local_variant = &et.variants[*local_idx];

        // Discriminant value for the local variant
        let local_disc = local_variant
            .discriminant
            .map(|d| d as u64)
            .unwrap_or(*local_idx as u64);

        let variant_block = program.new_block();
        variant_blocks.push((local_disc, variant_block));

        // Lower variant fields into the variant block
        if let Some(variant_plan) = variant_plans.get(&remote_idx) {
            lower_struct(
                variant_plan,
                local_variant.data,
                registry,
                cal,
                borrow_mode,
                program,
                variant_block,
                dst_offset,
            )?;
        } else if let Some(inner_plan) = nested.get(local_idx) {
            // Newtype variant — single field
            if let Some(field) = local_variant.data.fields.first() {
                let field_offset = dst_offset + field.offset;
                lower_value(
                    inner_plan,
                    field.shape(),
                    registry,
                    cal,
                    borrow_mode,
                    program,
                    variant_block,
                    field_offset,
                )?;
            }
        } else {
            // Identity: read fields in order
            let identity = build_identity_plan_for_struct(local_variant.data);
            lower_struct(
                &identity,
                local_variant.data,
                registry,
                cal,
                borrow_mode,
                program,
                variant_block,
                dst_offset,
            )?;
        }

        program.emit(variant_block, DecodeOp::Return);
    }

    program.emit(
        block,
        DecodeOp::BranchOnVariant {
            tag_offset: dst_offset,
            tag_width,
            variant_table: variant_map.clone(),
            variant_blocks,
        },
    );

    Ok(())
}

struct ScratchBuf {
    ptr: *mut u8,
    layout: std::alloc::Layout,
}

impl ScratchBuf {
    #[allow(unsafe_code)]
    fn new(layout: std::alloc::Layout) -> Option<Self> {
        let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
        if ptr.is_null() {
            None
        } else {
            Some(Self { ptr, layout })
        }
    }

    #[allow(unsafe_code)]
    fn to_bytes(&self) -> Box<[u8]> {
        unsafe { std::slice::from_raw_parts(self.ptr as *const u8, self.layout.size()) }
            .to_vec()
            .into_boxed_slice()
    }
}

impl Drop for ScratchBuf {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        unsafe { std::alloc::dealloc(self.ptr, self.layout) };
    }
}

struct CalibratedOptionLayout {
    inner_offset: usize,
    none_bytes: Box<[u8]>,
    some_bytes: Box<[u8]>,
}

struct CalibratedResultLayout {
    ok_offset: usize,
    err_offset: usize,
    ok_bytes: Box<[u8]>,
    err_bytes: Box<[u8]>,
}

#[allow(unsafe_code)]
fn calibrate_option_layout(
    shape: &'static Shape,
    opt_def: facet_core::OptionDef,
) -> Option<CalibratedOptionLayout> {
    let opt_layout = shape.layout.sized_layout().ok()?;
    let inner_layout = opt_def.t.layout.sized_layout().ok()?;

    let none_buf = ScratchBuf::new(opt_layout)?;
    unsafe { (opt_def.vtable.init_none)(facet_core::PtrUninit::new(none_buf.ptr as *mut ())) };

    let some_buf = ScratchBuf::new(opt_layout)?;
    let inner_buf = ScratchBuf::new(inner_layout)?;
    unsafe {
        opt_def
            .t
            .call_default_in_place(facet_core::PtrUninit::new(inner_buf.ptr as *mut ()))?
    };

    unsafe {
        (opt_def.vtable.init_some)(
            facet_core::PtrUninit::new(some_buf.ptr as *mut ()),
            facet_core::PtrMut::new(inner_buf.ptr as *mut ()),
        )
    };

    let ret_ptr =
        unsafe { (opt_def.vtable.get_value)(facet_core::PtrConst::new(some_buf.ptr as *const ())) };
    let base_ptr = some_buf.ptr as *const u8;
    let end_ptr = unsafe { base_ptr.add(opt_layout.size()) };
    if ret_ptr < base_ptr || ret_ptr > end_ptr {
        return None;
    }

    let inner_offset = ret_ptr as usize - base_ptr as usize;
    if inner_offset.checked_add(inner_layout.size())? > opt_layout.size() {
        return None;
    }

    let none_bytes = none_buf.to_bytes();
    let some_bytes = some_buf.to_bytes();
    let _ = unsafe { shape.call_drop_in_place(facet_core::PtrMut::new(some_buf.ptr as *mut ())) };

    Some(CalibratedOptionLayout {
        inner_offset,
        none_bytes,
        some_bytes,
    })
}

#[allow(unsafe_code)]
fn calibrate_result_layout(
    shape: &'static Shape,
    result_def: facet_core::ResultDef,
) -> Option<CalibratedResultLayout> {
    let result_layout = shape.layout.sized_layout().ok()?;
    let ok_layout = result_def.t.layout.sized_layout().ok()?;
    let err_layout = result_def.e.layout.sized_layout().ok()?;

    let ok_buf = ScratchBuf::new(result_layout)?;
    let ok_inner = ScratchBuf::new(ok_layout)?;
    unsafe {
        result_def
            .t
            .call_default_in_place(facet_core::PtrUninit::new(ok_inner.ptr as *mut ()))?
    };
    unsafe {
        (result_def.vtable.init_ok)(
            facet_core::PtrUninit::new(ok_buf.ptr as *mut ()),
            facet_core::PtrMut::new(ok_inner.ptr as *mut ()),
        )
    };

    let err_buf = ScratchBuf::new(result_layout)?;
    let err_inner = ScratchBuf::new(err_layout)?;
    unsafe {
        result_def
            .e
            .call_default_in_place(facet_core::PtrUninit::new(err_inner.ptr as *mut ()))?
    };
    unsafe {
        (result_def.vtable.init_err)(
            facet_core::PtrUninit::new(err_buf.ptr as *mut ()),
            facet_core::PtrMut::new(err_inner.ptr as *mut ()),
        )
    };

    let base_ptr = ok_buf.ptr as *const u8;
    let end_ptr = unsafe { base_ptr.add(result_layout.size()) };
    let ok_ptr =
        unsafe { (result_def.vtable.get_ok)(facet_core::PtrConst::new(ok_buf.ptr as *const ())) };
    if ok_ptr < base_ptr || ok_ptr > end_ptr {
        return None;
    }
    let ok_offset = ok_ptr as usize - base_ptr as usize;
    if ok_offset.checked_add(ok_layout.size())? > result_layout.size() {
        return None;
    }

    let err_base_ptr = err_buf.ptr as *const u8;
    let err_end_ptr = unsafe { err_base_ptr.add(result_layout.size()) };
    let err_ptr =
        unsafe { (result_def.vtable.get_err)(facet_core::PtrConst::new(err_buf.ptr as *const ())) };
    if err_ptr < err_base_ptr || err_ptr > err_end_ptr {
        return None;
    }
    let err_offset = err_ptr as usize - err_base_ptr as usize;
    if err_offset.checked_add(err_layout.size())? > result_layout.size() {
        return None;
    }

    let ok_bytes = ok_buf.to_bytes();
    let err_bytes = err_buf.to_bytes();
    let _ = unsafe { shape.call_drop_in_place(facet_core::PtrMut::new(ok_buf.ptr as *mut ())) };
    let _ = unsafe { shape.call_drop_in_place(facet_core::PtrMut::new(err_buf.ptr as *mut ())) };

    Some(CalibratedResultLayout {
        ok_offset,
        err_offset,
        ok_bytes,
        err_bytes,
    })
}

#[allow(unsafe_code)]
fn lower_option(
    plan: &TranslationPlan,
    shape: &'static Shape,
    opt_def: facet_core::OptionDef,
    registry: &SchemaRegistry,
    cal: Option<&CalibrationRegistry>,
    borrow_mode: BorrowMode,
    program: &mut DecodeProgram,
    block: usize,
    dst_offset: usize,
) -> Result<(), LowerError> {
    let inner_plan = match plan {
        TranslationPlan::Option { inner } => inner.as_ref(),
        TranslationPlan::Identity => &TranslationPlan::Identity,
        _ => {
            program.emit(
                block,
                DecodeOp::SlowPath {
                    shape,
                    plan: Box::new(clone_plan(plan)),
                    dst_offset,
                },
            );
            return Ok(());
        }
    };

    let Some(layout) = calibrate_option_layout(shape, opt_def) else {
        program.emit(
            block,
            DecodeOp::SlowPath {
                shape,
                plan: Box::new(clone_plan(plan)),
                dst_offset,
            },
        );
        return Ok(());
    };

    let some_block = program.new_block();

    program.emit(
        block,
        DecodeOp::DecodeOption {
            dst_offset,
            inner_offset: layout.inner_offset,
            some_block,
            none_bytes: layout.none_bytes,
            some_bytes: layout.some_bytes,
        },
    );

    // Lower the inner value decode into some_block.
    // The interpreter will call run_block(some_block, inner_ptr) where inner_ptr
    // already points to the inner slot, so dst_offset for inner ops is 0.
    lower_value(
        inner_plan,
        opt_def.t,
        registry,
        cal,
        borrow_mode,
        program,
        some_block,
        0,
    )?;
    program.emit(some_block, DecodeOp::Return);

    Ok(())
}

fn lower_array(
    plan: &TranslationPlan,
    arr_def: facet_core::ArrayDef,
    registry: &SchemaRegistry,
    cal: Option<&CalibrationRegistry>,
    borrow_mode: BorrowMode,
    program: &mut DecodeProgram,
    block: usize,
    dst_offset: usize,
) -> Result<(), LowerError> {
    let element_plan = match plan {
        TranslationPlan::Array { element } => element.as_ref(),
        TranslationPlan::Identity => &TranslationPlan::Identity,
        _ => &TranslationPlan::Identity,
    };

    let elem_shape = arr_def.t;
    let elem_layout = elem_shape
        .layout
        .sized_layout()
        .map_err(|_| LowerError::UnsizedShape)?;
    let elem_size = elem_layout.size();

    let body_block = program.new_block();

    program.emit(
        block,
        DecodeOp::DecodeArray {
            dst_offset,
            count: arr_def.n,
            elem_size,
            body_block,
        },
    );

    // Lower one element's decode into body_block (base = element pointer, offset = 0).
    lower_value(
        element_plan,
        elem_shape,
        registry,
        cal,
        borrow_mode,
        program,
        body_block,
        0,
    )?;
    program.emit(body_block, DecodeOp::Return);

    Ok(())
}

fn lower_list(
    plan: &TranslationPlan,
    shape: &'static Shape,
    list_def: facet_core::ListDef,
    registry: &SchemaRegistry,
    cal: Option<&CalibrationRegistry>,
    borrow_mode: BorrowMode,
    program: &mut DecodeProgram,
    block: usize,
    dst_offset: usize,
) -> Result<(), LowerError> {
    let elem_shape = list_def.t;
    let elem_layout = match elem_shape.layout.sized_layout() {
        Ok(l) => l,
        Err(_) => {
            program.emit(
                block,
                DecodeOp::SlowPath {
                    shape,
                    plan: Box::new(clone_plan(plan)),
                    dst_offset,
                },
            );
            return Ok(());
        }
    };
    let elem_size = elem_layout.size();

    // Vec<u8> scalar fast path: ReadByteVec copies bytes directly.
    if list_def.t.is_type::<u8>() {
        if let Some(cal) = cal
            && let Some(descriptor) = cal.lookup_by_shape(shape)
        {
            let descriptor = OpaqueDescriptorId(descriptor.0);
            let empty_block = program.new_block();
            let body_block = program.new_block();
            program.emit(
                block,
                DecodeOp::ReadListLen {
                    descriptor,
                    dst_offset,
                    empty_block,
                    body_block,
                },
            );
            program.emit(
                empty_block,
                DecodeOp::MaterializeEmpty {
                    dst_offset,
                    descriptor,
                },
            );
            program.emit(empty_block, DecodeOp::Return);
            // Vec<u8> body: AllocBacking then a single bulk copy (elem decode is ReadScalar<U8>).
            let inner_block = program.new_block();
            program.emit(
                body_block,
                DecodeOp::AllocBacking {
                    dst_offset,
                    descriptor,
                    body_block: inner_block,
                    elem_size,
                },
            );
            program.emit(body_block, DecodeOp::Return);
            // Element body: read one byte into out_ptr[0].
            let element_plan = match plan {
                TranslationPlan::List { element } => element.as_ref(),
                _ => &TranslationPlan::Identity,
            };
            lower_value(
                element_plan,
                elem_shape,
                registry,
                Some(cal),
                borrow_mode,
                program,
                inner_block,
                0,
            )?;
            program.emit(
                inner_block,
                DecodeOp::CommitListLen {
                    dst_offset,
                    descriptor,
                },
            );
            program.emit(inner_block, DecodeOp::Return);
            return Ok(());
        }
        program.emit(
            block,
            DecodeOp::SlowPath {
                shape,
                plan: Box::new(clone_plan(plan)),
                dst_offset,
            },
        );
        return Ok(());
    }

    // Generic Vec<T> — emit calibrated ops when available.
    // lookup_by_shape keys by Shape value (via blanket impl<T: Hash> Hash for &T).
    if let Some(cal) = cal
        && let Some(descriptor_handle) = cal.lookup_by_shape(shape)
    {
        let descriptor = OpaqueDescriptorId(descriptor_handle.0);

        let empty_block = program.new_block();
        let body_block = program.new_block();
        let inner_block = program.new_block();

        program.emit(
            block,
            DecodeOp::ReadListLen {
                descriptor,
                dst_offset,
                empty_block,
                body_block,
            },
        );

        // Empty path: copy calibrated empty bytes.
        program.emit(
            empty_block,
            DecodeOp::MaterializeEmpty {
                dst_offset,
                descriptor,
            },
        );
        program.emit(empty_block, DecodeOp::Return);

        // Non-empty path: allocate backing, then loop body.
        program.emit(
            body_block,
            DecodeOp::AllocBacking {
                dst_offset,
                descriptor,
                body_block: inner_block,
                elem_size,
            },
        );
        program.emit(body_block, DecodeOp::Return);

        // Element body block: decode one element at out_ptr[0], commit len.
        let element_plan = match plan {
            TranslationPlan::List { element } => element.as_ref(),
            _ => &TranslationPlan::Identity,
        };
        lower_value(
            element_plan,
            elem_shape,
            registry,
            Some(cal),
            borrow_mode,
            program,
            inner_block,
            0,
        )?;
        program.emit(
            inner_block,
            DecodeOp::CommitListLen {
                dst_offset,
                descriptor,
            },
        );
        program.emit(inner_block, DecodeOp::Return);

        return Ok(());
    }

    // No calibration — fall back to reflective interpreter.
    program.emit(
        block,
        DecodeOp::SlowPath {
            shape,
            plan: Box::new(clone_plan(plan)),
            dst_offset,
        },
    );
    Ok(())
}

fn lower_pointer(
    plan: &TranslationPlan,
    shape: &'static Shape,
    ptr_def: facet_core::PointerDef,
    registry: &SchemaRegistry,
    cal: Option<&CalibrationRegistry>,
    borrow_mode: BorrowMode,
    program: &mut DecodeProgram,
    block: usize,
    dst_offset: usize,
) -> Result<(), LowerError> {
    let pointee_plan = match plan {
        TranslationPlan::Pointer { pointee } => pointee.as_ref(),
        TranslationPlan::Identity => &TranslationPlan::Identity,
        _ => {
            program.emit(
                block,
                DecodeOp::SlowPath {
                    shape,
                    plan: Box::new(clone_plan(plan)),
                    dst_offset,
                },
            );
            return Ok(());
        }
    };

    let Some(pointee_shape) = ptr_def.pointee() else {
        // Opaque pointer — slow path.
        program.emit(
            block,
            DecodeOp::SlowPath {
                shape,
                plan: Box::new(clone_plan(plan)),
                dst_offset,
            },
        );
        return Ok(());
    };

    if let facet_core::Def::Slice(slice_def) = pointee_shape.def
        && slice_def.t().is_type::<u8>()
    {
        match ptr_def.known {
            Some(facet_core::KnownPointer::Cow) => {
                program.emit(
                    block,
                    DecodeOp::ReadCowByteSlice {
                        dst_offset,
                        borrowed: borrow_mode == BorrowMode::Borrowed,
                    },
                );
                return Ok(());
            }
            Some(facet_core::KnownPointer::SharedReference)
                if borrow_mode == BorrowMode::Borrowed =>
            {
                program.emit(block, DecodeOp::ReadByteSliceRef { dst_offset });
                return Ok(());
            }
            _ => {}
        }
    }

    // Box<[T]> — fat pointer. Look up by structural shape identity.
    if let facet_core::Def::Slice(_) = pointee_shape.def {
        if let Some(cal) = cal
            && let Some(descriptor_handle) = cal.lookup_by_shape(shape)
        {
            let descriptor = OpaqueDescriptorId(descriptor_handle.0);
            let body_block = program.new_block();
            program.emit(
                block,
                DecodeOp::AllocBoxed {
                    dst_offset,
                    descriptor,
                    body_block,
                },
            );
            // Slice element decode is not yet implemented inline — slow path for now.
            program.emit(
                body_block,
                DecodeOp::SlowPath {
                    shape,
                    plan: Box::new(clone_plan(plan)),
                    dst_offset: 0,
                },
            );
            program.emit(body_block, DecodeOp::Return);
            return Ok(());
        }
        program.emit(
            block,
            DecodeOp::SlowPath {
                shape,
                plan: Box::new(clone_plan(plan)),
                dst_offset,
            },
        );
        return Ok(());
    }

    // Box<T> (non-slice). Look up by structural shape identity.
    if let Some(cal) = cal
        && let Some(descriptor_handle) = cal.lookup_by_shape(shape)
    {
        let descriptor = OpaqueDescriptorId(descriptor_handle.0);
        let body_block = program.new_block();
        program.emit(
            block,
            DecodeOp::AllocBoxed {
                dst_offset,
                descriptor,
                body_block,
            },
        );
        // Decode the pointee into the newly allocated slot (base = alloc ptr, offset = 0).
        lower_value(
            pointee_plan,
            pointee_shape,
            registry,
            Some(cal),
            borrow_mode,
            program,
            body_block,
            0,
        )?;
        program.emit(body_block, DecodeOp::Return);
        return Ok(());
    }

    // No calibration or no descriptor registered for this pointer shape — fall back.
    program.emit(
        block,
        DecodeOp::SlowPath {
            shape,
            plan: Box::new(clone_plan(plan)),
            dst_offset,
        },
    );
    Ok(())
}

fn build_identity_plan_for_struct(st: facet_core::StructType) -> TranslationPlan {
    let field_ops = (0..st.fields.len())
        .map(|i| FieldOp::Read { local_index: i })
        .collect();
    TranslationPlan::Struct {
        field_ops,
        nested: HashMap::new(),
    }
}

/// Shallow clone of a `TranslationPlan` for embedding in `SlowPath` ops.
fn clone_plan(plan: &TranslationPlan) -> TranslationPlan {
    match plan {
        TranslationPlan::Identity => TranslationPlan::Identity,
        TranslationPlan::Struct { field_ops, nested } => TranslationPlan::Struct {
            field_ops: field_ops.clone(),
            nested: nested.iter().map(|(&k, v)| (k, clone_plan(v))).collect(),
        },
        TranslationPlan::Enum {
            variant_map,
            variant_plans,
            nested,
        } => TranslationPlan::Enum {
            variant_map: variant_map.clone(),
            variant_plans: variant_plans
                .iter()
                .map(|(&k, v)| (k, clone_plan(v)))
                .collect(),
            nested: nested.iter().map(|(&k, v)| (k, clone_plan(v))).collect(),
        },
        TranslationPlan::Tuple { field_ops, nested } => TranslationPlan::Tuple {
            field_ops: field_ops.clone(),
            nested: nested.iter().map(|(&k, v)| (k, clone_plan(v))).collect(),
        },
        TranslationPlan::List { element } => TranslationPlan::List {
            element: Box::new(clone_plan(element)),
        },
        TranslationPlan::Option { inner } => TranslationPlan::Option {
            inner: Box::new(clone_plan(inner)),
        },
        TranslationPlan::Map { key, value } => TranslationPlan::Map {
            key: Box::new(clone_plan(key)),
            value: Box::new(clone_plan(value)),
        },
        TranslationPlan::Array { element } => TranslationPlan::Array {
            element: Box::new(clone_plan(element)),
        },
        TranslationPlan::Pointer { pointee } => TranslationPlan::Pointer {
            pointee: Box::new(clone_plan(pointee)),
        },
    }
}

// ---------------------------------------------------------------------------
// Pure IR interpreter                                         (Task #3)
// ---------------------------------------------------------------------------

/// Interpreter state — wraps the input cursor.
struct InterpState<'a> {
    input: &'a [u8],
    pos: usize,
    /// Scratch register for the last-decoded discriminant (enum dispatch).
    discriminant: u64,
    /// Scratch register for list length (used during list alloc/commit).
    list_len: usize,
}

impl<'a> InterpState<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            pos: 0,
            discriminant: 0,
            list_len: 0,
        }
    }

    fn read_byte(&mut self) -> Result<u8, DeserializeError> {
        if self.pos >= self.input.len() {
            return Err(DeserializeError::UnexpectedEof { pos: self.pos });
        }
        let b = self.input[self.pos];
        self.pos += 1;
        Ok(b)
    }

    fn read_bytes(&mut self, n: usize) -> Result<&'a [u8], DeserializeError> {
        if self.pos + n > self.input.len() {
            return Err(DeserializeError::UnexpectedEof { pos: self.pos });
        }
        let s = &self.input[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn read_varint(&mut self) -> Result<u64, DeserializeError> {
        let start = self.pos;
        let mut result: u64 = 0;
        let mut shift: u32 = 0;
        loop {
            let byte = self.read_byte()?;
            result |= ((byte & 0x7F) as u64) << shift;
            if byte & 0x80 == 0 {
                return Ok(result);
            }
            shift += 7;
            if shift >= 64 {
                return Err(DeserializeError::VarintOverflow { pos: start });
            }
        }
    }

    fn read_signed_varint(&mut self) -> Result<i64, DeserializeError> {
        let z = self.read_varint()?;
        Ok(((z >> 1) as i64) ^ (-((z & 1) as i64)))
    }

    fn read_varint_u128(&mut self) -> Result<u128, DeserializeError> {
        let start = self.pos;
        let mut result: u128 = 0;
        let mut shift: u32 = 0;
        loop {
            let byte = self.read_byte()?;
            result |= ((byte & 0x7F) as u128) << shift;
            if byte & 0x80 == 0 {
                return Ok(result);
            }
            shift += 7;
            if shift >= 128 {
                return Err(DeserializeError::VarintOverflow { pos: start });
            }
        }
    }

    fn read_signed_varint_i128(&mut self) -> Result<i128, DeserializeError> {
        let z = self.read_varint_u128()?;
        Ok(((z >> 1) as i128) ^ (-((z & 1) as i128)))
    }

    fn read_str(&mut self) -> Result<&'a str, DeserializeError> {
        let len = self.read_varint()? as usize;
        let bytes = self.read_bytes(len)?;
        std::str::from_utf8(bytes).map_err(|_| DeserializeError::InvalidUtf8 {
            pos: self.pos - len,
        })
    }

    fn read_byte_slice(&mut self) -> Result<&'a [u8], DeserializeError> {
        let len = self.read_varint()? as usize;
        self.read_bytes(len)
    }

    fn read_opaque_bytes(&mut self) -> Result<&'a [u8], DeserializeError> {
        let len_bytes = self.read_bytes(4)?;
        let len =
            u32::from_le_bytes([len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]]) as usize;
        self.read_bytes(len)
    }
}

/// Interpret a `DecodeProgram` against `input`, writing the decoded value into
/// `dst` (which must point to at least `program.root_size` bytes of
/// writeable, properly-aligned memory, initialised to zero before calling).
///
/// `cal` may be `None` when calibration is unavailable; opaque fast-path ops
/// will fall back to conservative stubs in that case.
///
/// # Safety
///
/// - `dst` must be valid for writes of `program.root_size` bytes.
/// - `dst` must satisfy `program.root_align` alignment.
/// - Fields written by the program must not alias.
/// - The caller is responsible for dropping `dst` contents if this returns an
///   error after partial writes.
#[allow(unsafe_code)]
pub unsafe fn interpret(
    program: &DecodeProgram,
    input: &[u8],
    dst: *mut u8,
    registry: &SchemaRegistry,
    cal: Option<&CalibrationRegistry>,
) -> Result<usize, DeserializeError> {
    let mut state = InterpState::new(input);
    run_block(program, 0, &mut state, dst, registry, cal)?;
    Ok(state.pos)
}

#[allow(unsafe_code)]
fn run_block(
    program: &DecodeProgram,
    block_id: usize,
    state: &mut InterpState<'_>,
    base: *mut u8,
    registry: &SchemaRegistry,
    cal: Option<&CalibrationRegistry>,
) -> Result<(), DeserializeError> {
    let block = &program.blocks[block_id];
    let mut i = 0;
    while i < block.ops.len() {
        let op = &block.ops[i];
        match op {
            DecodeOp::ReadScalar { prim, dst_offset } => {
                let dst = unsafe { base.add(*dst_offset) };
                exec_read_scalar(state, *prim, dst)?;
            }

            DecodeOp::ReadByteVec {
                dst_offset,
                descriptor: _,
            } => {
                // Without calibrated descriptors, fall back to Vec<u8> via
                // standard allocation.
                let bytes = state.read_byte_slice()?;
                let vec: Vec<u8> = bytes.to_vec();
                unsafe {
                    std::ptr::write(base.add(*dst_offset) as *mut Vec<u8>, vec);
                }
            }

            DecodeOp::ReadString {
                dst_offset,
                descriptor: _,
            } => {
                let s = state.read_str()?;
                let owned = s.to_owned();
                unsafe {
                    std::ptr::write(base.add(*dst_offset) as *mut String, owned);
                }
            }

            DecodeOp::ReadCowStr {
                dst_offset,
                borrowed,
            } => {
                let s = state.read_str()?;
                let dst = unsafe { base.add(*dst_offset) as *mut std::borrow::Cow<'static, str> };
                let value = if *borrowed {
                    let borrowed: &'static str = unsafe { std::mem::transmute(s) };
                    std::borrow::Cow::Borrowed(borrowed)
                } else {
                    std::borrow::Cow::Owned(s.to_owned())
                };
                unsafe {
                    std::ptr::write(dst, value);
                }
            }

            DecodeOp::ReadStrRef { dst_offset } => {
                let s = state.read_str()?;
                let s: &'static str = unsafe { std::mem::transmute(s) };
                unsafe {
                    std::ptr::write(base.add(*dst_offset) as *mut &'static str, s);
                }
            }

            DecodeOp::ReadCowByteSlice {
                dst_offset,
                borrowed,
            } => {
                let bytes = state.read_byte_slice()?;
                let dst = unsafe { base.add(*dst_offset) as *mut std::borrow::Cow<'static, [u8]> };
                let value = if *borrowed {
                    let borrowed: &'static [u8] = unsafe { std::mem::transmute(bytes) };
                    std::borrow::Cow::Borrowed(borrowed)
                } else {
                    std::borrow::Cow::Owned(bytes.to_vec())
                };
                unsafe {
                    std::ptr::write(dst, value);
                }
            }

            DecodeOp::ReadByteSliceRef { dst_offset } => {
                let bytes = state.read_byte_slice()?;
                let bytes: &'static [u8] = unsafe { std::mem::transmute(bytes) };
                unsafe {
                    std::ptr::write(base.add(*dst_offset) as *mut &'static [u8], bytes);
                }
            }

            DecodeOp::ReadOpaque { shape, dst_offset } => {
                let bytes = state.read_opaque_bytes()?;
                let adapter = shape.opaque_adapter.ok_or_else(|| {
                    DeserializeError::ReflectError(format!("missing opaque adapter for {shape}"))
                })?;
                let input = facet::OpaqueDeserialize::Borrowed(bytes);
                unsafe {
                    (adapter.deserialize)(input, facet_core::PtrUninit::new(base.add(*dst_offset)))
                }
                .map_err(|e| {
                    DeserializeError::ReflectError(format!(
                        "opaque adapter deserialize failed for {shape}: {e}"
                    ))
                })?;
            }

            DecodeOp::SkipValue { kind } => {
                skip_in_state(state, kind, registry)?;
            }

            DecodeOp::WriteDefault { shape, dst_offset } => {
                let dst = unsafe { base.add(*dst_offset) };
                unsafe {
                    shape
                        .call_default_in_place(facet_core::PtrUninit::new(dst as *mut ()))
                        .ok_or_else(|| {
                            DeserializeError::ReflectError(format!(
                                "no Default available for fill-defaults field {shape}"
                            ))
                        })?;
                }
            }

            DecodeOp::DecodeOption {
                dst_offset,
                inner_offset,
                some_block,
                none_bytes,
                some_bytes,
            } => {
                let tag = state.read_byte()?;
                let option_ptr = unsafe { base.add(*dst_offset) };
                match tag {
                    0x00 => unsafe {
                        std::ptr::copy_nonoverlapping(
                            none_bytes.as_ptr(),
                            option_ptr,
                            none_bytes.len(),
                        );
                    },
                    0x01 => {
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                some_bytes.as_ptr(),
                                option_ptr,
                                some_bytes.len(),
                            );
                        }
                        let inner_ptr = unsafe { option_ptr.add(*inner_offset) };
                        run_block(program, *some_block, state, inner_ptr, registry, cal)?;
                    }
                    other => {
                        return Err(DeserializeError::InvalidOptionTag {
                            pos: state.pos - 1,
                            got: other,
                        });
                    }
                }
            }

            DecodeOp::DecodeResult {
                dst_offset,
                ok_block,
                err_block,
                ok_offset,
                err_offset,
                ok_bytes,
                err_bytes,
            } => {
                let variant_index = state.read_varint()? as usize;
                let result_ptr = unsafe { base.add(*dst_offset) };
                match variant_index {
                    0 => {
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                ok_bytes.as_ptr(),
                                result_ptr,
                                ok_bytes.len(),
                            );
                        }
                        let payload_ptr = unsafe { result_ptr.add(*ok_offset) };
                        run_block(program, *ok_block, state, payload_ptr, registry, cal)?;
                    }
                    1 => {
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                err_bytes.as_ptr(),
                                result_ptr,
                                err_bytes.len(),
                            );
                        }
                        let payload_ptr = unsafe { result_ptr.add(*err_offset) };
                        run_block(program, *err_block, state, payload_ptr, registry, cal)?;
                    }
                    other => {
                        return Err(DeserializeError::UnknownVariant {
                            remote_index: other,
                        });
                    }
                }
            }

            DecodeOp::DecodeResultInit {
                dst_offset,
                ok_block,
                err_block,
                ok_size,
                ok_align,
                err_size,
                err_align,
                init_ok_fn,
                init_err_fn,
            } => {
                let variant_index = state.read_varint()? as usize;
                let result_ptr = unsafe { base.add(*dst_offset) };
                match variant_index {
                    0 => {
                        let layout = std::alloc::Layout::from_size_align(*ok_size, *ok_align)
                            .map_err(|_| {
                                DeserializeError::Custom("bad Result::Ok layout".into())
                            })?;
                        let tmp = facet_core::alloc_for_layout(layout);
                        let tmp_ptr = unsafe { tmp.assume_init() };
                        if let Err(err) = run_block(
                            program,
                            *ok_block,
                            state,
                            tmp_ptr.as_mut_byte_ptr(),
                            registry,
                            cal,
                        ) {
                            unsafe { facet_core::dealloc_for_layout(tmp_ptr, layout) };
                            return Err(err);
                        }
                        unsafe {
                            init_ok_fn(facet_core::PtrUninit::new(result_ptr), tmp_ptr);
                            facet_core::dealloc_for_layout(tmp_ptr, layout);
                        }
                    }
                    1 => {
                        let layout = std::alloc::Layout::from_size_align(*err_size, *err_align)
                            .map_err(|_| {
                                DeserializeError::Custom("bad Result::Err layout".into())
                            })?;
                        let tmp = facet_core::alloc_for_layout(layout);
                        let tmp_ptr = unsafe { tmp.assume_init() };
                        if let Err(err) = run_block(
                            program,
                            *err_block,
                            state,
                            tmp_ptr.as_mut_byte_ptr(),
                            registry,
                            cal,
                        ) {
                            unsafe { facet_core::dealloc_for_layout(tmp_ptr, layout) };
                            return Err(err);
                        }
                        unsafe {
                            init_err_fn(facet_core::PtrUninit::new(result_ptr), tmp_ptr);
                            facet_core::dealloc_for_layout(tmp_ptr, layout);
                        }
                    }
                    other => {
                        return Err(DeserializeError::UnknownVariant {
                            remote_index: other,
                        });
                    }
                }
            }

            DecodeOp::ReadDiscriminant => {
                state.discriminant = state.read_varint()?;
            }

            // r[impl schema.errors.unknown-variant-runtime]
            DecodeOp::BranchOnVariant {
                tag_offset,
                tag_width,
                variant_table,
                variant_blocks,
            } => {
                let remote_disc = state.discriminant as usize;
                let local_idx = variant_table.get(remote_disc).copied().flatten().ok_or(
                    DeserializeError::UnknownVariant {
                        remote_index: remote_disc,
                    },
                )?;

                // There may be no corresponding entry in variant_blocks if the
                // remote had more variants than we mapped.
                let (local_disc, variant_block) = variant_blocks
                    .get(remote_disc)
                    .copied()
                    .filter(|&(_, b)| b != usize::MAX)
                    .ok_or(DeserializeError::UnknownVariant {
                        remote_index: remote_disc,
                    })?;

                // Write the local discriminant tag
                write_tag(unsafe { base.add(*tag_offset) }, *tag_width, local_disc);

                run_block(program, variant_block, state, base, registry, cal)?;

                let _ = local_idx; // used implicitly via variant_block selection
            }

            DecodeOp::PushFrame {
                field_offset,
                frame_size: _,
            } => {
                let new_base = unsafe { base.add(*field_offset) };
                // Execute the rest of the current block list from the new base?
                // PushFrame/PopFrame would normally be paired; for now, since the
                // interpreter uses `run_block` recursion for nested types, we
                // don't need an explicit stack here.  This op is reserved for the
                // Cranelift backend where explicit frame management is needed.
                let _ = new_base;
            }

            DecodeOp::PopFrame => { /* see PushFrame — no-op in interpreter */ }

            DecodeOp::ReadListLen {
                descriptor: _,
                dst_offset: _,
                empty_block,
                body_block,
            } => {
                let len = state.read_varint()? as usize;
                state.list_len = len;
                if len == 0 {
                    run_block(program, *empty_block, state, base, registry, cal)?;
                } else {
                    run_block(program, *body_block, state, base, registry, cal)?;
                }
            }

            DecodeOp::CommitListLen {
                dst_offset: _,
                descriptor: _,
            } => {
                // Without calibrated descriptors the interpreter defers list
                // handling to SlowPath.  This op is a no-op here.
            }

            DecodeOp::DecodeArray {
                dst_offset,
                count,
                elem_size,
                body_block,
            } => {
                let mut elem_ptr = unsafe { base.add(*dst_offset) };
                for _ in 0..*count {
                    run_block(program, *body_block, state, elem_ptr, registry, cal)?;
                    elem_ptr = unsafe { elem_ptr.add(*elem_size) };
                }
            }

            DecodeOp::MaterializeEmpty {
                dst_offset,
                descriptor,
            } => {
                // Use the calibrated empty bytes when available; fall back to
                // zeroing the slot (conservative but correct on all current
                // Rust targets where Vec/String empty repr is all-zeros).
                if let Some(cal) = cal
                    && let Some(desc) = cal.get(DescriptorHandle::from(*descriptor))
                {
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            desc.empty_bytes.as_ptr(),
                            base.add(*dst_offset),
                            desc.empty_bytes.len(),
                        );
                    }
                } else {
                    unsafe {
                        std::ptr::write_bytes(
                            base.add(*dst_offset),
                            0,
                            std::mem::size_of::<usize>() * 3,
                        );
                    }
                }
            }

            DecodeOp::AllocBacking {
                dst_offset: _,
                descriptor: _,
                body_block: _,
                elem_size: _,
            } => {
                // Reserve/commit is deferred to the Cranelift backend (task #9).
                // The IR interpreter falls back to SlowPath for non-empty lists.
            }

            DecodeOp::AllocBoxed {
                dst_offset,
                descriptor,
                body_block,
            } => {
                let Some(cal) = cal else {
                    return Err(DeserializeError::Custom(
                        "AllocBoxed requires a calibration registry".into(),
                    ));
                };
                let handle = DescriptorHandle(descriptor.0);
                let desc = cal.get(handle).ok_or_else(|| {
                    DeserializeError::Custom("AllocBoxed: descriptor handle not found".into())
                })?;
                let alloc_ptr = if desc.elem_size == 0 {
                    desc.elem_align as *mut u8
                } else {
                    let layout =
                        std::alloc::Layout::from_size_align(desc.elem_size, desc.elem_align)
                            .map_err(|_| {
                                DeserializeError::Custom("AllocBoxed: invalid layout".into())
                            })?;
                    let p = unsafe { std::alloc::alloc(layout) };
                    if p.is_null() {
                        return Err(DeserializeError::Custom(
                            "AllocBoxed: allocation failed (OOM)".into(),
                        ));
                    }
                    p
                };
                let container_ptr = unsafe { base.add(*dst_offset) };
                unsafe {
                    let ptr_slot = container_ptr.add(desc.ptr_offset as usize) as *mut *mut u8;
                    std::ptr::write(ptr_slot, alloc_ptr);
                }
                run_block(program, *body_block, state, alloc_ptr, registry, Some(cal))?;
            }

            DecodeOp::SlowPath {
                shape,
                plan,
                dst_offset,
            } => {
                exec_slow_path(state, shape, plan, *dst_offset, base, registry)?;
            }

            DecodeOp::Jump { block_id } => {
                return run_block(program, *block_id, state, base, registry, cal);
            }

            DecodeOp::Return => {
                return Ok(());
            }
        }

        i += 1;
    }

    Ok(())
}

/// Scalar write helper — writes a decoded value at `dst` (correctly typed).
#[allow(unsafe_code)]
fn exec_read_scalar(
    state: &mut InterpState<'_>,
    prim: WirePrimitive,
    dst: *mut u8,
) -> Result<(), DeserializeError> {
    match prim {
        WirePrimitive::Unit => {}
        WirePrimitive::Bool => {
            let b = state.read_byte()?;
            match b {
                0x00 => unsafe { std::ptr::write(dst as *mut bool, false) },
                0x01 => unsafe { std::ptr::write(dst as *mut bool, true) },
                other => {
                    return Err(DeserializeError::InvalidBool {
                        pos: state.pos - 1,
                        got: other,
                    });
                }
            }
        }
        WirePrimitive::U8 => {
            let v = state.read_byte()?;
            unsafe { std::ptr::write(dst as *mut u8, v) };
        }
        WirePrimitive::U16 => {
            let v = state.read_varint()? as u16;
            unsafe { std::ptr::write(dst as *mut u16, v) };
        }
        WirePrimitive::U32 => {
            let v = state.read_varint()? as u32;
            unsafe { std::ptr::write(dst as *mut u32, v) };
        }
        WirePrimitive::U64 => {
            let v = state.read_varint()?;
            unsafe { std::ptr::write(dst as *mut u64, v) };
        }
        WirePrimitive::U128 => {
            let v = state.read_varint_u128()?;
            unsafe { std::ptr::write(dst as *mut u128, v) };
        }
        WirePrimitive::USize => {
            let v = state.read_varint()? as usize;
            unsafe { std::ptr::write(dst as *mut usize, v) };
        }
        WirePrimitive::I8 => {
            let v = state.read_byte()? as i8;
            unsafe { std::ptr::write(dst as *mut i8, v) };
        }
        WirePrimitive::I16 => {
            let v = state.read_signed_varint()? as i16;
            unsafe { std::ptr::write(dst as *mut i16, v) };
        }
        WirePrimitive::I32 => {
            let v = state.read_signed_varint()? as i32;
            unsafe { std::ptr::write(dst as *mut i32, v) };
        }
        WirePrimitive::I64 => {
            let v = state.read_signed_varint()?;
            unsafe { std::ptr::write(dst as *mut i64, v) };
        }
        WirePrimitive::I128 => {
            let v = state.read_signed_varint_i128()?;
            unsafe { std::ptr::write(dst as *mut i128, v) };
        }
        WirePrimitive::ISize => {
            let v = state.read_signed_varint()? as isize;
            unsafe { std::ptr::write(dst as *mut isize, v) };
        }
        WirePrimitive::F32 => {
            let bytes = state.read_bytes(4)?;
            let v = f32::from_le_bytes(bytes.try_into().unwrap());
            unsafe { std::ptr::write(dst as *mut f32, v) };
        }
        WirePrimitive::F64 => {
            let bytes = state.read_bytes(8)?;
            let v = f64::from_le_bytes(bytes.try_into().unwrap());
            unsafe { std::ptr::write(dst as *mut f64, v) };
        }
        WirePrimitive::String => {
            let s = state.read_str()?;
            let owned = s.to_owned();
            unsafe { std::ptr::write(dst as *mut String, owned) };
        }
        WirePrimitive::Bytes => {
            let bytes = state.read_byte_slice()?;
            let vec: Vec<u8> = bytes.to_vec();
            unsafe { std::ptr::write(dst as *mut Vec<u8>, vec) };
        }
        WirePrimitive::Payload => {
            let len_bytes = state.read_bytes(4)?;
            let len = u32::from_le_bytes(len_bytes.try_into().unwrap()) as usize;
            let payload = state.read_bytes(len)?.to_vec();
            unsafe { std::ptr::write(dst as *mut Vec<u8>, payload) };
        }
        WirePrimitive::Char => {
            let s = state.read_str()?;
            let c = s
                .chars()
                .next()
                .ok_or_else(|| DeserializeError::Custom("empty string for char".into()))?;
            unsafe { std::ptr::write(dst as *mut char, c) };
        }
    }
    Ok(())
}

/// Write `disc` into `tag_ptr` according to `width`.
#[allow(unsafe_code)]
fn write_tag(tag_ptr: *mut u8, width: TagWidth, disc: u64) {
    match width {
        TagWidth::U8 => unsafe { std::ptr::write(tag_ptr, disc as u8) },
        TagWidth::U16 => unsafe { std::ptr::write(tag_ptr as *mut u16, disc as u16) },
        TagWidth::U32 => unsafe { std::ptr::write(tag_ptr as *mut u32, disc as u32) },
        TagWidth::U64 => unsafe { std::ptr::write(tag_ptr as *mut u64, disc) },
    }
}

/// Skip a value using the existing `decode::skip_value` path.
/// We rebuild a temporary cursor from the current state position.
fn skip_in_state(
    state: &mut InterpState<'_>,
    kind: &SchemaKind,
    registry: &SchemaRegistry,
) -> Result<(), DeserializeError> {
    let mut tmp = crate::decode::Cursor::new(state.input);
    tmp.advance_to(state.pos);
    crate::decode::skip_value(&mut tmp, kind, registry)?;
    state.pos = tmp.pos();
    Ok(())
}

// r[impl schema.exchange.required]
/// Slow-path: fall back to the reflective `deserialize_value` path for
/// shapes the IR could not lower.
///
/// Creates a `Partial` over the caller-provided destination slot (no extra
/// allocation), deserializes into it in-place, and calls `finish_in_place`
/// so that optional-field defaults are applied.
#[allow(unsafe_code)]
fn exec_slow_path(
    state: &mut InterpState<'_>,
    shape: &'static Shape,
    plan: &TranslationPlan,
    dst_offset: usize,
    base: *mut u8,
    registry: &SchemaRegistry,
) -> Result<(), DeserializeError> {
    use facet_core::PtrUninit;
    use facet_reflect::Partial;

    let remaining = &state.input[state.pos..];
    let mut cursor = crate::decode::Cursor::new(remaining);

    let dst_ptr = unsafe { base.add(dst_offset) };
    let uninit = PtrUninit::new(dst_ptr as *mut ());
    let partial = unsafe { Partial::from_raw_with_shape(uninit, shape) }
        .map_err(|e| DeserializeError::ReflectError(e.to_string()))?;

    let partial =
        crate::deserialize::deserialize_value_pub::<false>(partial, &mut cursor, plan, registry)?;

    partial
        .finish_in_place()
        .map_err(|e| DeserializeError::ReflectError(e.to_string()))?;

    state.pos += cursor.pos();
    Ok(())
}

/// Public raw bridge for the JIT SlowPath helper.
///
/// Decodes a single value of `shape` using the reflective interpreter, reading
/// from `input[consumed..]`. On success writes the initialized value into
/// `dst_base.add(dst_offset)` and returns the new consumed position.
/// On failure returns `None`.
///
/// A fresh `SchemaRegistry` is used — SlowPath types use identity plans that
/// contain no `FieldOp::Skip` entries, so no cross-schema lookups are needed.
pub fn slow_path_decode_raw(
    input_ptr: *const u8,
    input_len: usize,
    consumed: usize,
    shape: &'static Shape,
    plan: *const TranslationPlan,
    dst_base: *mut u8,
    dst_offset: usize,
) -> Option<usize> {
    use facet_core::PtrUninit;
    use facet_reflect::Partial;

    let input = unsafe { core::slice::from_raw_parts(input_ptr, input_len) };
    let remaining = &input[consumed..];
    let mut cursor = crate::decode::Cursor::new(remaining);

    let dst_ptr = unsafe { dst_base.add(dst_offset) };
    let uninit = PtrUninit::new(dst_ptr as *mut ());
    let partial = unsafe { Partial::from_raw_with_shape(uninit, shape) }.ok()?;

    let plan_ref = unsafe { &*plan };
    let registry = vox_schema::SchemaRegistry::new();

    let partial = crate::deserialize::deserialize_value_pub::<false>(
        partial,
        &mut cursor,
        plan_ref,
        &registry,
    )
    .ok()?;

    partial.finish_in_place().ok()?;

    Some(consumed + cursor.pos())
}

// ---------------------------------------------------------------------------
// Public entry point for IR-based deserialization
// ---------------------------------------------------------------------------

// r[impl schema.translation.serialization-unchanged]
/// Deserialize `input` into a value of type `T` using the IR interpreter.
///
/// Falls back to `SlowPath` ops for shapes not yet handled by the IR lowering.
/// This is the correctness oracle path — it must agree with the reflective
/// interpreter for all valid inputs.
///
/// Pass `cal` to enable the calibrated fast path for opaque types (Vec<T>,
/// String). Pass `None` to use zero-filled fallbacks for those ops.
pub fn from_slice_ir<T>(
    input: &[u8],
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
    cal: Option<&CalibrationRegistry>,
) -> Result<T, DeserializeError>
where
    T: facet::Facet<'static>,
{
    unsafe { from_slice_ir_impl::<T>(input, plan, registry, cal, BorrowMode::Owned) }
}

/// Borrowed-mode sibling of [`from_slice_ir`]. Emits `ReadStrRef` /
/// `ReadByteSliceRef` / borrowed `Cow` ops so the decoded value may hold
/// references into `input`.
///
/// The `'input: 'facet` bound mirrors the JIT `try_decode_borrowed` entry
/// point — the input must outlive the borrowed value.
pub fn from_slice_ir_borrowed<'input, 'facet, T>(
    input: &'input [u8],
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
    cal: Option<&CalibrationRegistry>,
) -> Result<T, DeserializeError>
where
    T: facet::Facet<'facet>,
    'input: 'facet,
{
    unsafe { from_slice_ir_impl::<T>(input, plan, registry, cal, BorrowMode::Borrowed) }
}

/// SAFETY: caller must uphold the lifetime contract between `input` and `T`
/// when `borrow_mode` is `Borrowed` — the `BorrowMode::Owned` public wrapper
/// enforces `T: Facet<'static>`; the `Borrowed` wrapper enforces
/// `'input: 'facet`.
#[allow(unsafe_code)]
unsafe fn from_slice_ir_impl<'facet, T>(
    input: &[u8],
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
    cal: Option<&CalibrationRegistry>,
    borrow_mode: BorrowMode,
) -> Result<T, DeserializeError>
where
    T: facet::Facet<'facet>,
{
    let shape = T::SHAPE;
    let program = lower_with_cal(plan, shape, registry, cal, borrow_mode).map_err(|e| match e {
        LowerError::UnsizedShape => DeserializeError::UnsupportedType("unsized shape".into()),
        LowerError::UnstableEnumRepr => {
            DeserializeError::UnsupportedType("unstable enum repr".into())
        }
        LowerError::SchemaMissing => DeserializeError::Custom("schema missing during lower".into()),
    })?;

    let layout = shape
        .layout
        .sized_layout()
        .map_err(|_| DeserializeError::UnsupportedType(format!("{shape}")))?;

    let ptr = {
        let p = unsafe {
            std::alloc::alloc_zeroed(
                std::alloc::Layout::from_size_align(layout.size(), layout.align())
                    .map_err(|_| DeserializeError::Custom("bad layout".into()))?,
            )
        };
        if p.is_null() {
            std::alloc::handle_alloc_error(
                std::alloc::Layout::from_size_align(layout.size(), layout.align()).unwrap(),
            );
        }
        p
    };

    let result = unsafe { interpret(&program, input, ptr, registry, cal) };

    match result {
        Ok(_bytes_consumed) => {
            let value: T = unsafe { std::ptr::read(ptr as *const T) };
            unsafe {
                std::alloc::dealloc(
                    ptr,
                    std::alloc::Layout::from_size_align(layout.size(), layout.align()).unwrap(),
                );
            }
            Ok(value)
        }
        Err(e) => {
            unsafe {
                std::ptr::write_bytes(ptr, 0, layout.size());
                std::alloc::dealloc(
                    ptr,
                    std::alloc::Layout::from_size_align(layout.size(), layout.align()).unwrap(),
                );
            }
            Err(e)
        }
    }
}

// ---------------------------------------------------------------------------
// Encode IR: EncodeOp + EncodeProgram + lower_encode           (Task #17)
// ---------------------------------------------------------------------------
//
// Encode does NOT use translation plans. It walks the sender's local type
// layout directly and writes postcard bytes into an EncodeCtx buffer.
//
// The IR is symmetric to DecodeProgram but simpler: no skip operations,
// no discriminant mapping, no partial-init concerns.

/// A single IR instruction for the encode path.
///
/// Instructions read from an implicit source pointer (`*const u8` base +
/// field offsets) and write bytes to an implicit output buffer (EncodeCtx).
///
/// Operand convention:
///   `src_offset`: byte offset from the base of the struct being read.
#[derive(Debug, Clone)]
pub enum EncodeOp {
    // -----------------------------------------------------------------------
    // Primitive writes
    // -----------------------------------------------------------------------
    /// Read a scalar primitive from `src_offset` and write its postcard
    /// encoding to the output buffer.
    WriteScalar {
        prim: WirePrimitive,
        src_offset: usize,
    },

    /// Encode a string-like field (`String`, `&str`, `Cow<str>`) without the
    /// reflective walker.
    WriteStringLike {
        shape: &'static Shape,
        src_offset: usize,
    },

    /// Encode a bytes-like field (`Cow<[u8]>`, `&[u8]`) without the
    /// reflective walker.
    WriteBytesLike {
        shape: &'static Shape,
        src_offset: usize,
    },

    /// Encode a field by delegating to a nested encoder for the exact shape.
    WriteShape {
        shape: &'static Shape,
        src_offset: usize,
    },

    /// Encode an opaque-adapter field (`#[facet(opaque = ...)]`) via a dedicated
    /// helper that preserves postcard's length-prefixed opaque semantics while
    /// delegating nested value encoding back through the JIT runtime.
    WriteOpaque {
        shape: &'static Shape,
        src_offset: usize,
    },

    /// Encode a proxy field (`#[facet(proxy = ...)]`) by converting to the
    /// proxy value and then delegating nested encoding back through the JIT
    /// runtime.
    WriteProxy {
        shape: &'static Shape,
        src_offset: usize,
    },

    /// Encode one field reflectively from `src_offset`.
    SlowPath {
        shape: &'static Shape,
        src_offset: usize,
    },

    /// Borrow a pointee from a pointer-like value and continue encoding from
    /// the borrowed pointee pointer.
    BorrowPointer {
        src_offset: usize,
        body_block: usize,
        borrow_fn: facet_core::BorrowFn,
    },

    /// Write a varint-length-prefixed byte slice from a slice-like value.
    WriteByteSlice {
        src_offset: usize,
        len_fn: facet_core::SliceLenFn,
        as_ptr_fn: facet_core::SliceAsPtrFn,
    },

    // -----------------------------------------------------------------------
    // Option handling
    // -----------------------------------------------------------------------
    /// Encode an `Option<T>` at `src_offset`.
    ///
    /// Reads the option state via `is_some_fn` (0 = None, 1 = Some).
    /// - None: writes 0x00.
    /// - Some: writes 0x01, then jumps to `some_block` with `inner_ptr`.
    ///
    /// `get_value_fn` returns a pointer to the inner value when the option
    /// is Some; the encode block uses that pointer as its source base.
    EncodeOption {
        src_offset: usize,
        some_block: usize,
        /// vtable fn: `unsafe extern "C" fn(PtrConst) -> bool`
        is_some_fn: facet_core::OptionIsSomeFn,
        /// vtable fn: `unsafe extern "C" fn(PtrConst) -> PtrConst`
        get_value_fn: facet_core::OptionGetValueFn,
    },

    /// Encode a `Result<T, E>` at `src_offset`.
    ///
    /// Writes postcard discriminant `0` for `Ok`, `1` for `Err`, then encodes
    /// the corresponding inner value from the pointer returned by the vtable.
    EncodeResult {
        shape: &'static Shape,
        src_offset: usize,
        ok_block: usize,
        err_block: usize,
        ok_shape: &'static Shape,
        err_shape: &'static Shape,
        is_ok_fn: facet_core::ResultIsOkFn,
        get_ok_fn: facet_core::ResultGetOkFn,
        get_err_fn: facet_core::ResultGetErrFn,
    },

    // -----------------------------------------------------------------------
    // Enum handling
    // -----------------------------------------------------------------------
    /// Write the enum variant's postcard index (its position in the variant
    /// list) as a varint. Emitted as the first op of each variant body so
    /// the index is correct regardless of any explicit Rust discriminant.
    WriteVariantIndex { index: u64 },

    /// Branch to the encode block for the active variant.
    ///
    /// Reads the tag at `src_offset` and dispatches to `variant_blocks[disc]`.
    /// `variant_blocks[i] = (discriminant_value, block_id)`.
    BranchOnEncode {
        src_offset: usize,
        tag_width: TagWidth,
        /// Parallel to variant_blocks: (disc_value, block_id).
        variant_blocks: Vec<(u64, usize)>,
    },

    // -----------------------------------------------------------------------
    // List / array handling
    // -----------------------------------------------------------------------
    /// Write the element count of a Vec-like container at `src_offset` as a
    /// varint, then iterate over elements using `body_block`.
    ///
    /// The body block is called once per element with base = element pointer.
    EncodeList {
        src_offset: usize,
        descriptor: OpaqueDescriptorId,
        body_block: usize,
        /// Byte stride between elements in the backing allocation.
        elem_size: usize,
    },

    /// Encode a fixed-size array at `src_offset`.
    ///
    /// Calls `body_block` exactly `count` times, advancing by `elem_size`.
    EncodeArray {
        src_offset: usize,
        count: usize,
        elem_size: usize,
        body_block: usize,
    },

    // -----------------------------------------------------------------------
    // Control flow (mirrors decode IR)
    // -----------------------------------------------------------------------
    /// Unconditional jump to `block_id`.
    Jump { block_id: usize },

    /// End of the current block.
    Return,
}

/// A linear sequence of `EncodeOp` instructions.
#[derive(Debug, Clone, Default)]
pub struct EncodeBlock {
    pub ops: Vec<EncodeOp>,
}

/// A fully-lowered encode program for one root type.
///
/// Block 0 is always the entry point.
#[derive(Debug, Clone)]
pub struct EncodeProgram {
    pub blocks: Vec<EncodeBlock>,
    /// Size in bytes of the root value.
    pub root_size: usize,
    /// Alignment of the root value.
    pub root_align: usize,
}

impl EncodeProgram {
    fn new_block(&mut self) -> usize {
        let id = self.blocks.len();
        self.blocks.push(EncodeBlock::default());
        id
    }

    fn emit(&mut self, block: usize, op: EncodeOp) {
        self.blocks[block].ops.push(op);
    }
}

/// Error returned by the encode lowering pass.
#[derive(Debug)]
pub enum EncodeLowerError {
    /// The shape does not have a known sized layout.
    UnsizedShape,
    /// The enum representation is not stable (Rust or NPO repr).
    UnstableEnumRepr,
    /// The type is not supported by the JIT encode path.
    Unsupported(String),
}

impl std::fmt::Display for EncodeLowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsizedShape => write!(f, "unsized shape"),
            Self::UnstableEnumRepr => write!(f, "unstable enum repr"),
            Self::Unsupported(s) => write!(f, "unsupported: {s}"),
        }
    }
}

/// Lower a `Shape` into an `EncodeProgram`.
///
/// No translation plan needed — encode always uses the local type definition.
/// `cal` is used to resolve Vec-family opaque descriptor handles.
pub fn lower_encode(
    shape: &'static Shape,
    cal: Option<&CalibrationRegistry>,
) -> Result<EncodeProgram, EncodeLowerError> {
    let layout = shape
        .layout
        .sized_layout()
        .map_err(|_| EncodeLowerError::UnsizedShape)?;
    let mut program = EncodeProgram {
        blocks: vec![EncodeBlock::default()],
        root_size: layout.size(),
        root_align: layout.align(),
    };
    lower_encode_value(shape, cal, &mut program, 0, 0)?;
    program.emit(0, EncodeOp::Return);
    Ok(program)
}

fn lower_encode_value(
    shape: &'static Shape,
    cal: Option<&CalibrationRegistry>,
    program: &mut EncodeProgram,
    block: usize,
    src_offset: usize,
) -> Result<(), EncodeLowerError> {
    use facet_core::{Def, Type, UserType};

    // Transparent wrappers — pass through to inner shape
    if shape.is_transparent() {
        if let Type::User(UserType::Struct(st)) = shape.ty
            && let Some(inner_field) = st.fields.first()
        {
            return lower_encode_value(inner_field.shape(), cal, program, block, src_offset);
        }
        return Err(EncodeLowerError::Unsupported(format!(
            "transparent non-struct: {shape}"
        )));
    }

    if let Def::Result(result_def) = shape.def {
        return lower_encode_result(shape, result_def, cal, program, block, src_offset);
    }

    if shape.opaque_adapter.is_some() {
        program.emit(block, EncodeOp::WriteOpaque { shape, src_offset });
        return Ok(());
    }

    if shape.proxy.is_some() {
        program.emit(block, EncodeOp::WriteProxy { shape, src_offset });
        return Ok(());
    }

    // Scalars
    if let Some(scalar) = shape.scalar_type() {
        match scalar {
            facet_core::ScalarType::String
            | facet_core::ScalarType::Str
            | facet_core::ScalarType::CowStr => {
                program.emit(block, EncodeOp::WriteStringLike { shape, src_offset });
                return Ok(());
            }
            _ => {}
        }

        if let Some(prim) = WirePrimitive::from_scalar(scalar) {
            program.emit(block, EncodeOp::WriteScalar { prim, src_offset });
            return Ok(());
        }
        return Err(EncodeLowerError::Unsupported(format!(
            "unknown scalar: {scalar:?}"
        )));
    }

    match shape.def {
        Def::Option(opt_def) => {
            return lower_encode_option(opt_def, cal, program, block, src_offset);
        }
        Def::Array(arr_def) => {
            return lower_encode_array(arr_def, cal, program, block, src_offset);
        }
        Def::List(list_def) => {
            return lower_encode_list(shape, list_def, cal, program, block, src_offset);
        }
        Def::Pointer(ptr_def) => {
            return lower_encode_pointer(shape, ptr_def, cal, program, block, src_offset);
        }
        Def::Slice(slice_def) => {
            return lower_encode_slice(shape, slice_def, program, block, src_offset);
        }
        Def::Map(_) | Def::Set(_) => {
            return Err(EncodeLowerError::Unsupported(format!(
                "unsupported def: {shape}"
            )));
        }
        _ => {}
    }

    // User types: struct / enum
    match shape.ty {
        Type::User(UserType::Struct(st)) => {
            for field in st.fields {
                let field_offset = src_offset + field.offset;
                lower_encode_value(field.shape(), cal, program, block, field_offset)?;
            }
            Ok(())
        }
        Type::User(UserType::Enum(et)) => {
            lower_encode_enum(shape, et, cal, program, block, src_offset)
        }
        _ => Err(EncodeLowerError::Unsupported(format!(
            "unsupported type: {shape}"
        ))),
    }
}

fn lower_encode_pointer(
    shape: &'static Shape,
    ptr_def: facet_core::PointerDef,
    _cal: Option<&CalibrationRegistry>,
    program: &mut EncodeProgram,
    block: usize,
    src_offset: usize,
) -> Result<(), EncodeLowerError> {
    let Some(pointee_shape) = ptr_def.pointee() else {
        return Err(EncodeLowerError::Unsupported(
            "opaque pointer without pointee".into(),
        ));
    };

    if pointee_shape == <str as Facet<'static>>::SHAPE {
        program.emit(block, EncodeOp::WriteStringLike { shape, src_offset });
        return Ok(());
    }

    if let facet_core::Def::Slice(slice_def) = pointee_shape.def
        && slice_def.t().is_type::<u8>()
    {
        program.emit(block, EncodeOp::WriteBytesLike { shape, src_offset });
        return Ok(());
    }

    Err(EncodeLowerError::Unsupported(format!(
        "unsupported pointer: {pointee_shape}"
    )))
}

fn should_inline_loop_body(shape: &'static Shape) -> bool {
    if let Some(scalar) = shape.scalar_type() {
        let _ = scalar;
        return true;
    }

    match shape.def {
        facet_core::Def::Pointer(ptr_def) => {
            if let Some(pointee) = ptr_def.pointee() {
                return pointee.scalar_type() == Some(facet_core::ScalarType::Str)
                    || matches!(pointee.def, facet_core::Def::Slice(slice_def) if slice_def.t().is_type::<u8>());
            }
            false
        }
        facet_core::Def::Slice(slice_def) => slice_def.t().is_type::<u8>(),
        _ => false,
    }
}

fn lower_encode_slice(
    shape: &'static Shape,
    slice_def: facet_core::SliceDef,
    program: &mut EncodeProgram,
    block: usize,
    src_offset: usize,
) -> Result<(), EncodeLowerError> {
    if !slice_def.t().is_type::<u8>() {
        return Err(EncodeLowerError::Unsupported(format!(
            "unsupported slice: {shape}"
        )));
    }

    program.emit(
        block,
        EncodeOp::WriteByteSlice {
            src_offset,
            len_fn: slice_def.vtable.len,
            as_ptr_fn: slice_def.vtable.as_ptr,
        },
    );
    Ok(())
}

fn lower_encode_option(
    opt_def: facet_core::OptionDef,
    cal: Option<&CalibrationRegistry>,
    program: &mut EncodeProgram,
    block: usize,
    src_offset: usize,
) -> Result<(), EncodeLowerError> {
    let some_block = program.new_block();

    program.emit(
        block,
        EncodeOp::EncodeOption {
            src_offset,
            some_block,
            is_some_fn: opt_def.vtable.is_some,
            get_value_fn: opt_def.vtable.get_value,
        },
    );

    // Lower the inner value encode into some_block (base = inner_ptr, offset = 0).
    lower_encode_value(opt_def.t, cal, program, some_block, 0)?;
    program.emit(some_block, EncodeOp::Return);

    Ok(())
}

fn lower_encode_result(
    shape: &'static Shape,
    result_def: facet_core::ResultDef,
    cal: Option<&CalibrationRegistry>,
    program: &mut EncodeProgram,
    block: usize,
    src_offset: usize,
) -> Result<(), EncodeLowerError> {
    let ok_block = program.new_block();
    let err_block = program.new_block();

    program.emit(
        block,
        EncodeOp::EncodeResult {
            shape,
            src_offset,
            ok_block,
            err_block,
            ok_shape: result_def.t,
            err_shape: result_def.e,
            is_ok_fn: result_def.vtable.is_ok,
            get_ok_fn: result_def.vtable.get_ok,
            get_err_fn: result_def.vtable.get_err,
        },
    );

    lower_encode_value(result_def.t, cal, program, ok_block, 0)?;
    program.emit(ok_block, EncodeOp::Return);

    lower_encode_value(result_def.e, cal, program, err_block, 0)?;
    program.emit(err_block, EncodeOp::Return);

    Ok(())
}

fn lower_encode_array(
    arr_def: facet_core::ArrayDef,
    cal: Option<&CalibrationRegistry>,
    program: &mut EncodeProgram,
    block: usize,
    src_offset: usize,
) -> Result<(), EncodeLowerError> {
    let elem_shape = arr_def.t;
    let elem_layout = elem_shape
        .layout
        .sized_layout()
        .map_err(|_| EncodeLowerError::UnsizedShape)?;
    let elem_size = elem_layout.size();
    let body_block = program.new_block();

    program.emit(
        block,
        EncodeOp::EncodeArray {
            src_offset,
            count: arr_def.n,
            elem_size,
            body_block,
        },
    );

    if should_inline_loop_body(elem_shape) {
        lower_encode_value(elem_shape, cal, program, body_block, 0)?;
    } else {
        program.emit(
            body_block,
            EncodeOp::WriteShape {
                shape: elem_shape,
                src_offset: 0,
            },
        );
    }
    program.emit(body_block, EncodeOp::Return);

    Ok(())
}

fn lower_encode_list(
    shape: &'static Shape,
    list_def: facet_core::ListDef,
    cal: Option<&CalibrationRegistry>,
    program: &mut EncodeProgram,
    block: usize,
    src_offset: usize,
) -> Result<(), EncodeLowerError> {
    let elem_shape = list_def.t;
    let elem_layout = elem_shape
        .layout
        .sized_layout()
        .map_err(|_| EncodeLowerError::UnsizedShape)?;
    let elem_size = elem_layout.size();

    // Look up calibration by structural shape identity (not pointer address).
    let descriptor = if let Some(cal) = cal
        && let Some(h) = cal.lookup_by_shape(shape)
    {
        OpaqueDescriptorId(h.0)
    } else {
        return Err(EncodeLowerError::Unsupported(format!(
            "Vec<T> without calibration: {shape}"
        )));
    };

    let body_block = program.new_block();

    program.emit(
        block,
        EncodeOp::EncodeList {
            src_offset,
            descriptor,
            body_block,
            elem_size,
        },
    );

    if should_inline_loop_body(elem_shape) {
        lower_encode_value(elem_shape, cal, program, body_block, 0)?;
    } else {
        program.emit(
            body_block,
            EncodeOp::WriteShape {
                shape: elem_shape,
                src_offset: 0,
            },
        );
    }
    program.emit(body_block, EncodeOp::Return);

    Ok(())
}

fn lower_encode_enum(
    _shape: &'static Shape,
    et: facet_core::EnumType,
    cal: Option<&CalibrationRegistry>,
    program: &mut EncodeProgram,
    block: usize,
    src_offset: usize,
) -> Result<(), EncodeLowerError> {
    let tag_width =
        TagWidth::from_enum_repr(et.enum_repr).ok_or(EncodeLowerError::UnstableEnumRepr)?;

    // Each variant body first writes the variant's postcard index, then
    // encodes its fields. The dispatch block only branches — the index write
    // is deferred to the body so that explicit Rust discriminants (e.g.
    // `Inline = 1`) don't leak onto the wire. Postcard wants the variant's
    // position in the enum, not its in-memory tag byte.
    let mut variant_blocks: Vec<(u64, usize)> = Vec::new();
    for (i, variant) in et.variants.iter().enumerate() {
        let disc = variant.discriminant.map(|d| d as u64).unwrap_or(i as u64);
        let vblock = program.new_block();
        variant_blocks.push((disc, vblock));

        program.emit(vblock, EncodeOp::WriteVariantIndex { index: i as u64 });

        for field in variant.data.fields {
            let field_offset = src_offset + field.offset;
            lower_encode_value(field.shape(), cal, program, vblock, field_offset)?;
        }
        program.emit(vblock, EncodeOp::Return);
    }

    program.emit(
        block,
        EncodeOp::BranchOnEncode {
            src_offset,
            tag_width,
            variant_blocks,
        },
    );

    Ok(())
}
