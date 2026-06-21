//! The typed path: lower a [`Descriptor`] (which carries its schema) into a flat
//! [`MemProgram`], then run it to encode or decode a value living in this
//! process's memory.
//!
//! This is the memory counterpart to the dynamic [`Value`]
//! path. The split is phon's schema+descriptor pairing: the **schema** (resolved
//! through the registry) decides the wire bytes and their order; the
//! **descriptor** says where each field lives in memory. Because the wire is
//! schema-driven, a typed value produces byte-identical output to the dynamic
//! codec for the same logical value — that equivalence is the oracle the tests
//! check (in the `phon` front door, over real facet-derived descriptors), and
//! it's what lets a typed peer and a dynamic peer interoperate.
//!
//! Lowering walks the descriptor once and folds every field offset to be
//! relative to the value's base pointer. A nested fixed struct therefore
//! dissolves into a single straight run of scalar copies (`r[ir.inlining]`,
//! `r[ir.memory]`) — no per-decode descriptor walk, no branches. Owned
//! sequences, options, enums (allocation and run-time branching) come next.
//!
//! First cut: fixed-width scalars and in-place records (struct/tuple). Anything
//! else lowers to [`CompactError::Unsupported`].
//!
//! Spec: "The descriptor model", "Compact mode", `r[ir.memory]`.

use std::alloc;
use std::collections::{BTreeMap, HashMap};
use std::convert::Infallible;

use phon_ir::ir::{
    BorrowOp, BytesOp, DefaultOp, EnumOp, EnumVariantOp, Lowered, LoweringError, MapOp, MemOp,
    MemProgram, OpaqueOp, OptionOp, PointerOp, ResultOp, SetOp, SkipOp, fuse, group_record_scalars,
    lower_fixed_array as lower_fixed_array_elements, lower_record_fields, owned_sequence_op,
    set_op,
};
use phon_ir::{
    Access, Construct, Descriptor, EnumAccess, MapStorage, Presence, RecordAccess, ResultAccess,
    SequenceAccess, SequenceStorage, SetAccess, SetStorage, Tag, VariantAccess,
};
use phon_schema::bytes::{Reader, write_u8, write_u32};
use phon_schema::{
    DecodeError, Field, Primitive, SchemaId, SchemaKind, SchemaRef, Value, Variant, VariantPayload,
    read_value, write_value,
};
use weavy::{Control, RunError, RunStats, Step};

use crate::compact::{self, CompactError, Registry, Resolved, alignment, pad_to, skip_pad};
use crate::compat::{self, FieldMatch, VariantMatch, incompatible};

type Result<T> = core::result::Result<T, CompactError>;

impl From<LoweringError> for CompactError {
    fn from(error: LoweringError) -> Self {
        match error {
            LoweringError::ArrayBulkCopySizeOverflow => {
                CompactError::Malformed("array bulk copy size overflow")
            }
            LoweringError::ArrayElementOffsetOverflow => {
                CompactError::Malformed("array element offset overflow")
            }
        }
    }
}

/// The wire (and, for our targets, in-memory) size of a fixed-width scalar, or
/// `None` for the variable-length and uninhabited primitives, which need
/// allocation or are never values and so are not plain copies.
fn fixed_size(p: Primitive) -> Option<usize> {
    Some(match p {
        Primitive::Unit => 0,
        Primitive::Bool | Primitive::U8 | Primitive::I8 => 1,
        Primitive::U16 | Primitive::I16 => 2,
        Primitive::U32 | Primitive::I32 | Primitive::F32 | Primitive::Char => 4,
        Primitive::U64 | Primitive::I64 | Primitive::F64 => 8,
        Primitive::U128 | Primitive::I128 => 16,
        Primitive::String
        | Primitive::Bytes
        | Primitive::Never
        | Primitive::DateTime
        | Primitive::Uuid
        | Primitive::QName => return None,
    })
}

// ============================================================================
// Lowering
// ============================================================================

/// Lower a descriptor into a flat [`MemProgram`]: a list of base-relative memory
/// copies, in wire order. Build it once, run it many times.
///
/// # Errors
/// [`CompactError`] if a referenced schema is missing, the descriptor and schema
/// disagree, or a kind this first cut does not handle is reached.
// r[impl ir.memory]
// r[impl descriptors.fact-driven]
pub fn lower(descriptor: &Descriptor, reg: &Registry) -> Result<MemProgram> {
    let mut out = Vec::new();
    lower_node(descriptor, reg, 0, &mut out)?;
    // Coalesce contiguous scalar runs into single copies (e.g. a flat struct
    // whose wire and memory layouts match becomes one memcpy).
    Ok(fuse(out))
}

/// Lower a descriptor that may be recursive: the root program plus a block program per
/// recursive (cyclic) schema, each lowered once from `descriptor_blocks` (which
/// `phon::derive` collected). A `CallBlock` op resolves into [`Lowered::blocks`] at run
/// time. For a non-recursive type `descriptor_blocks` is empty and the result is the
/// familiar flat program with no blocks (`r[ir.recursion]`).
// r[impl descriptors.separate-implementations]
pub fn lower_typed(
    descriptor: &Descriptor,
    descriptor_blocks: &HashMap<SchemaId, Descriptor>,
    reg: &Registry,
) -> Result<Lowered> {
    let mut root = Vec::new();
    lower_node(descriptor, reg, 0, &mut root)?;
    let mut blocks = BTreeMap::new();
    for (id, body) in descriptor_blocks {
        // A block's ops are relative to the recursive value's own start (base 0); a
        // `CallBlock` supplies the actual pointer at run time.
        let mut ops = Vec::new();
        lower_node(body, reg, 0, &mut ops)?;
        blocks.insert(*id, fuse(ops));
    }
    Ok(Lowered {
        program: fuse(root),
        blocks,
    })
}

// r[impl ir.inlining]
fn lower_node(d: &Descriptor, reg: &Registry, base: usize, out: &mut MemProgram) -> Result<()> {
    // A back-edge to a recursive schema: emit a call into that schema's block, run at
    // `base + offset` (the recursive value's location). The block itself is lowered once
    // by `lower_typed` from `Derived::descriptor_blocks`. (`r[ir.recursion]`)
    if matches!(d.access, Access::Recurse) {
        let schema = match &d.schema {
            SchemaRef::Concrete { id, .. } => *id,
            SchemaRef::Var { .. } => {
                return Err(CompactError::Unsupported(
                    "typed: recursion via type-var ref",
                ));
            }
        };
        out.push(MemOp::CallBlock {
            schema,
            offset: base,
        });
        return Ok(());
    }
    match (&d.access, compact::resolve(reg, &d.schema)?) {
        (Access::Scalar, Resolved::Primitive(p)) => {
            let size = fixed_size(p).ok_or(CompactError::Unsupported(
                "typed: variable-length scalar field",
            ))?;
            if d.layout.size == size {
                out.push(MemOp::Scalar {
                    offset: base,
                    size,
                    align: alignment(p),
                });
            } else if matches!(p, Primitive::U64 | Primitive::I64)
                && matches!(d.layout.size, 1 | 2 | 4 | 8)
            {
                out.push(MemOp::NativeInt {
                    offset: base,
                    mem_size: d.layout.size,
                    signed: matches!(p, Primitive::I64),
                });
            } else {
                return Err(CompactError::Unsupported(
                    "typed: scalar memory width differs from wire width",
                ));
            }
            Ok(())
        }
        (Access::Record(ra), Resolved::Composite(kind)) => {
            let arity = match &kind {
                SchemaKind::Struct { fields, .. } => fields.len(),
                SchemaKind::Tuple { elements } => elements.len(),
                _ => {
                    return Err(CompactError::TypeMismatch {
                        expected: "struct or tuple for a record descriptor",
                    });
                }
            };
            if arity != ra.fields.len() {
                return Err(CompactError::Malformed(
                    "descriptor/schema field count mismatch",
                ));
            }
            match &ra.construct {
                Construct::InPlace => {}
                Construct::Thunk(_) => {
                    return Err(CompactError::Unsupported("typed: thunk construction"));
                }
            }
            let mut record = Vec::new();
            lower_record_fields(&ra.fields, base, &mut record, |field, field_base, out| {
                lower_node(field, reg, field_base, out)
            })?;
            out.extend(group_record_scalars(fuse(record), &ra.byte_ownership, base));
            Ok(())
        }
        // r[impl ir.memory]
        (Access::Sequence(seq), Resolved::Composite(SchemaKind::List { .. })) => {
            let SequenceStorage::Vtable(thunks) = &seq.storage else {
                return Err(CompactError::Unsupported(
                    "typed: only vtable-backed owned sequences so far",
                ));
            };
            // Lower the element once; it runs at each element slot (base 0).
            let stride = seq.element.layout.size;
            let elem_align = seq.element.layout.align;
            let mut element = Vec::new();
            lower_node(&seq.element, reg, 0, &mut element)?;
            // Bulk-copy lowering: an element that is a single scalar covering its
            // whole size, with no inter-element wire padding, decodes/encodes as
            // ONE block copy — `Vec<u32>`, `Vec<f64>`, `Vec<u8>`, flat `repr(C)`
            // structs. Anything with structure stays a per-element sequence.
            out.push(owned_sequence_op(
                base,
                element,
                stride,
                elem_align,
                validate_any,
                *thunks,
            ));
            Ok(())
        }
        (Access::Set(set), Resolved::Composite(SchemaKind::Set { .. })) => {
            lower_set(set, reg, base, out)
        }
        // r[impl ir.memory] — [T; N]: fixed-shape inline elements with no length prefix.
        (
            Access::Array {
                element,
                count,
                stride,
            },
            Resolved::Composite(SchemaKind::Array { dimensions, .. }),
        ) => {
            require_fixed_array_count(*count, &dimensions)?;
            lower_fixed_array(element, *count, *stride, reg, base, out)
        }
        // r[impl ir.memory] — String/Bytes: a bulk contiguous byte run.
        (
            Access::Sequence(seq),
            Resolved::Primitive(p @ (Primitive::String | Primitive::Bytes)),
        ) => {
            match &seq.storage {
                // A BORROWED leaf (`&str`/`&[u8]`): same wire as the owned run, but
                // decode writes a fat pointer into the input (no alloc, no copy).
                SequenceStorage::BorrowedVtable(thunks) => {
                    out.push(MemOp::Borrow(Box::new(BorrowOp {
                        field_offset: base,
                        stride: 1,
                        elem_align: 1,
                        thunks: *thunks,
                    })));
                    Ok(())
                }
                SequenceStorage::Vtable(thunks) => {
                    out.push(MemOp::Bytes(Box::new(BytesOp {
                        field_offset: base,
                        stride: 1,
                        elem_align: 1,
                        validate: if matches!(p, Primitive::String) {
                            validate_utf8
                        } else {
                            validate_any
                        },
                        thunks: *thunks,
                    })));
                    Ok(())
                }
                _ => Err(CompactError::Unsupported(
                    "typed: string/bytes needs vtable thunks",
                )),
            }
        }
        // r[impl ir.memory] — Option<T>: a presence byte then the inner value.
        (Access::Option(opt), Resolved::Composite(SchemaKind::Option { .. })) => {
            let Presence::Vtable(thunks) = &opt.presence else {
                return Err(CompactError::Unsupported(
                    "typed: option needs vtable presence thunks",
                ));
            };
            // The some-payload sub-program runs at the inner value (base 0).
            let mut some = Vec::new();
            lower_node(&opt.some, reg, 0, &mut some)?;
            out.push(MemOp::Option(Box::new(OptionOp {
                field_offset: base,
                some: fuse(some),
                inner_size: opt.some.layout.size,
                inner_align: opt.some.layout.align,
                thunks: *thunks,
            })));
            Ok(())
        }
        // r[impl ir.memory] — #[repr(int)] enum: a u32 wire index then the payload.
        (Access::Enum(ea), Resolved::Composite(SchemaKind::Enum { .. })) => {
            let Tag::Direct { offset, width } = &ea.tag else {
                return Err(CompactError::Unsupported(
                    "typed: only #[repr(int)] enums (direct discriminant) so far",
                ));
            };
            let mut variants = Vec::with_capacity(ea.variants.len());
            for va in &ea.variants {
                // The variant's fields live at base-relative offsets that already
                // account for the discriminant (per facet); lower them as a record.
                let mut payload = Vec::new();
                lower_record_fields(
                    &va.payload.fields,
                    base,
                    &mut payload,
                    |field, field_base, out| lower_node(field, reg, field_base, out),
                )?;
                variants.push(EnumVariantOp {
                    wire_index: va.index,
                    selector: va.selector,
                    payload: group_record_scalars(fuse(payload), &va.payload.byte_ownership, base),
                });
            }
            out.push(MemOp::Enum(Box::new(EnumOp {
                tag_offset: base + *offset,
                tag_width: *width,
                variants,
                writer_only: Vec::new(),
            })));
            Ok(())
        }
        // r[impl ir.memory] — Map<K, V>: a u32 entry count then key+value pairs.
        (Access::Map(ma), Resolved::Composite(SchemaKind::Map { .. })) => {
            let MapStorage::Vtable(thunks) = &ma.storage else {
                return Err(CompactError::Unsupported("typed: map needs vtable thunks"));
            };
            // The key and value sub-programs each run at their own value (base 0).
            let mut key = Vec::new();
            lower_node(&ma.key, reg, 0, &mut key)?;
            let mut value = Vec::new();
            lower_node(&ma.value, reg, 0, &mut value)?;
            out.push(MemOp::Map(Box::new(MapOp {
                field_offset: base,
                key: fuse(key),
                value: fuse(value),
                key_size: ma.key.layout.size,
                key_align: ma.key.layout.align,
                value_size: ma.value.layout.size,
                value_align: ma.value.layout.align,
                thunks: *thunks,
            })));
            Ok(())
        }
        // r[impl ir.memory] — a self-describing dynamic `Value` field: encoded /
        // decoded by the self-describing codec, self-delimiting on the wire.
        (Access::Dynamic, Resolved::Composite(SchemaKind::Dynamic)) => {
            out.push(MemOp::Dynamic { field_offset: base });
            Ok(())
        }
        // r[impl ir.memory] — Result<T, E>: a u32 wire index then the active arm's
        // payload (wire-identical to a two-variant enum). The schema gives the Ok/Err
        // wire indices; the thunks drive the repr(Rust) layout.
        (Access::Result(ra), Resolved::Composite(SchemaKind::Enum { variants, .. })) => {
            out.push(MemOp::Result(Box::new(lower_result(
                ra, &variants, reg, base,
            )?)));
            Ok(())
        }
        // r[impl descriptors.thunk-binding]
        (Access::Pointer(pa), _) => {
            let mut pointee = Vec::new();
            lower_node(&pa.pointee, reg, 0, &mut pointee)?;
            out.push(MemOp::Pointer(Box::new(PointerOp {
                field_offset: base,
                pointee: fuse(pointee),
                pointee_size: pa.pointee.layout.size,
                pointee_align: pa.pointee.layout.align,
                thunks: pa.thunks,
            })));
            Ok(())
        }
        // r[impl ir.memory] — opaque field: a length-prefixed blob (wire-identical
        // to a `Primitive::Bytes` run); the engine frames it and the thunks fill /
        // consume the inner span.
        (Access::Opaque(thunks), Resolved::Primitive(Primitive::Bytes)) => {
            out.push(MemOp::Opaque(Box::new(OpaqueOp {
                field_offset: base,
                thunks: *thunks,
            })));
            Ok(())
        }
        _ => Err(CompactError::Unsupported(
            "typed: only fixed scalars, in-place records, owned sequences, strings, options, #[repr(int)] enums, and opaque fields so far",
        )),
    }
}

// ============================================================================
// Decode-compat lowering (writer schema ⋈ reader descriptor)
// ============================================================================

/// Lower a *writer* schema translated against a *reader* [`Descriptor`] into a
/// flat [`MemProgram`] of reader-memory ops, in WIRE order. This is the typed
/// (memory-side) analog of `plan::build_plan` + `plan::lower`: it bakes the
/// writer↔reader compatibility decision in once, at lowering, so decode stays as
/// fast as the single-schema path — there is no fast/slow path, only one program.
///
/// The compat rules mirror `plan.rs` exactly (the cross-engine oracle):
/// struct fields match by name (writer-only skipped, reader-only defaulted or, if
/// required, incompatible), enum variants match by name (writer-only → a decode
/// error), and types match without implicit widening (`r[compat.*]`).
///
/// When `writer_root` resolves to the same schema the reader carries, the result
/// is equivalent to [`lower`] (no skips/defaults) — the identity case.
///
/// # Errors
/// [`CompactError::Incompatible`] (or a resolution error) if the writer and reader
/// cannot be translated, or [`CompactError::Unsupported`] for a kind not yet
/// carried by the typed path.
// r[impl compat.plan-first]
pub fn lower_decode(
    writer_root: SchemaId,
    reader: &Descriptor,
    reader_blocks: &HashMap<SchemaId, Descriptor>,
    reg: &Registry,
) -> Result<Lowered> {
    let mut out = Vec::new();
    lower_decode_node(&SchemaRef::concrete(writer_root), reader, reg, 0, &mut out)?;
    // A recursive reader lowers each of its cyclic schemas to a callable block, just
    // as `lower_typed` does — a `Recurse` reader node became a `CallBlock` into one of
    // these. For the same-schema path the writer's schema at every `Recurse`
    // position is that same schema, so a block translates
    // `concrete(R) ⋈ reader_blocks[R]` — the identity case. Compatibility across
    // differing recursive schemas is the tracked follow-up; here the block's writer
    // ref is the reader schema id.
    let mut blocks = BTreeMap::new();
    for (id, body) in reader_blocks {
        let mut ops = Vec::new();
        lower_decode_node(&SchemaRef::concrete(*id), body, reg, 0, &mut ops)?;
        blocks.insert(*id, fuse(ops));
    }
    Ok(Lowered {
        program: fuse(out),
        blocks,
    })
}

/// Append the reader-memory ops for one (writer schema ⋈ reader descriptor) node,
/// folding the reader field offset into `base`.
// r[impl compat.type-match]
fn lower_decode_node(
    writer: &SchemaRef,
    reader: &Descriptor,
    reg: &Registry,
    base: usize,
    out: &mut MemProgram,
) -> Result<()> {
    // A recursive reader back-edge: emit a call into that schema's block, run at
    // `base + offset`. `lower_decode` lowers the block itself from `reader_blocks`.
    // (`r[ir.recursion]`)
    if matches!(reader.access, Access::Recurse) {
        let schema = match &reader.schema {
            SchemaRef::Concrete { id, .. } => *id,
            SchemaRef::Var { .. } => {
                return Err(CompactError::Unsupported(
                    "typed: recursion via type-var ref (decode)",
                ));
            }
        };
        out.push(MemOp::CallBlock {
            schema,
            offset: base,
        });
        return Ok(());
    }
    let w = compact::resolve(reg, writer)?;
    match (&reader.access, w) {
        // Scalar ⋈ scalar: identical primitives copy through; differing ones are
        // incompatible — NO implicit numeric widening (`r[compat.type-match]`).
        (Access::Scalar, Resolved::Primitive(wp)) => {
            let Resolved::Primitive(rp) = compact::resolve(reg, &reader.schema)? else {
                return Err(CompactError::TypeMismatch {
                    expected: "scalar reader schema for a scalar descriptor",
                });
            };
            if wp != rp {
                return Err(incompatible(format!("primitive {wp:?} is not {rp:?}")));
            }
            let size = fixed_size(wp).ok_or(CompactError::Unsupported(
                "typed: variable-length scalar field",
            ))?;
            out.push(MemOp::Scalar {
                offset: base,
                size,
                align: alignment(wp),
            });
            Ok(())
        }
        // Struct ⋈ struct: match fields by name, in WIRE order.
        (Access::Record(ra), Resolved::Composite(SchemaKind::Struct { fields: wf, .. })) => {
            lower_decode_record(&wf, ra, &reader.schema, RecordKind::Struct, reg, base, out)
        }
        // Tuple ⋈ tuple: positional record fields, carried as synthetic index names
        // through the same field matcher.
        (Access::Record(ra), Resolved::Composite(SchemaKind::Tuple { elements })) => {
            let wf = tuple_fields(elements);
            lower_decode_record(&wf, ra, &reader.schema, RecordKind::Tuple, reg, base, out)
        }
        // Enum ⋈ enum: match variants by name.
        (Access::Enum(ea), Resolved::Composite(SchemaKind::Enum { variants: wv, .. })) => {
            lower_decode_enum(&wv, ea, &reader.schema, reg, base, out)
        }
        // Option ⋈ Option: structural shapes match; translate the inner.
        (Access::Option(opt), Resolved::Composite(SchemaKind::Option { element: we })) => {
            require_reader_option(&reader.schema, reg)?;
            let Presence::Vtable(thunks) = &opt.presence else {
                return Err(CompactError::Unsupported(
                    "typed: option needs vtable presence thunks",
                ));
            };
            let mut some = Vec::new();
            lower_decode_node(&we, &opt.some, reg, 0, &mut some)?;
            out.push(MemOp::Option(Box::new(OptionOp {
                field_offset: base,
                some: fuse(some),
                inner_size: opt.some.layout.size,
                inner_align: opt.some.layout.align,
                thunks: *thunks,
            })));
            Ok(())
        }
        // List ⋈ List: translate the element.
        (Access::Sequence(seq), Resolved::Composite(SchemaKind::List { element: we })) => {
            require_reader_list(&reader.schema, reg)?;
            lower_decode_sequence(&we, seq, reg, base, out)
        }
        // Set ⋈ Set: translate the element.
        (Access::Set(set), Resolved::Composite(SchemaKind::Set { element: we })) => {
            require_reader_set(&reader.schema, reg)?;
            lower_decode_set(&we, set, reg, base, out)
        }
        // Fixed array ⋈ fixed array: dimensions match exactly; translate each element.
        (
            Access::Array {
                element,
                count,
                stride,
            },
            Resolved::Composite(SchemaKind::Array {
                element: we,
                dimensions: wd,
            }),
        ) => {
            let Resolved::Composite(SchemaKind::Array { dimensions: rd, .. }) =
                compact::resolve(reg, &reader.schema)?
            else {
                return Err(incompatible("schema kinds differ"));
            };
            if wd != rd {
                return Err(incompatible("array dimensions differ"));
            }
            require_fixed_array_count(*count, &wd)?;
            lower_decode_fixed_array(&we, element, *count, *stride, reg, base, out)
        }
        // String/Bytes ⋈ String/Bytes: a bulk byte run (no element translation).
        (
            Access::Sequence(seq),
            Resolved::Primitive(p @ (Primitive::String | Primitive::Bytes)),
        ) => {
            let Resolved::Primitive(rp) = compact::resolve(reg, &reader.schema)? else {
                return Err(CompactError::TypeMismatch {
                    expected: "string/bytes reader schema",
                });
            };
            if p != rp {
                return Err(incompatible(format!("primitive {p:?} is not {rp:?}")));
            }
            match &seq.storage {
                // A BORROWED leaf (`&str`/`&[u8]`): same wire, zero-copy decode.
                SequenceStorage::BorrowedVtable(thunks) => {
                    out.push(MemOp::Borrow(Box::new(BorrowOp {
                        field_offset: base,
                        stride: 1,
                        elem_align: 1,
                        thunks: *thunks,
                    })));
                    Ok(())
                }
                SequenceStorage::Vtable(thunks) => {
                    out.push(MemOp::Bytes(Box::new(BytesOp {
                        field_offset: base,
                        stride: 1,
                        elem_align: 1,
                        validate: if matches!(p, Primitive::String) {
                            validate_utf8
                        } else {
                            validate_any
                        },
                        thunks: *thunks,
                    })));
                    Ok(())
                }
                _ => Err(CompactError::Unsupported(
                    "typed: string/bytes needs vtable thunks",
                )),
            }
        }
        // Map ⋈ Map: translate key and value.
        (Access::Map(ma), Resolved::Composite(SchemaKind::Map { key: wk, value: wv })) => {
            require_reader_map(&reader.schema, reg)?;
            let MapStorage::Vtable(thunks) = &ma.storage else {
                return Err(CompactError::Unsupported("typed: map needs vtable thunks"));
            };
            let mut key = Vec::new();
            lower_decode_node(&wk, &ma.key, reg, 0, &mut key)?;
            let mut value = Vec::new();
            lower_decode_node(&wv, &ma.value, reg, 0, &mut value)?;
            out.push(MemOp::Map(Box::new(MapOp {
                field_offset: base,
                key: fuse(key),
                value: fuse(value),
                key_size: ma.key.layout.size,
                key_align: ma.key.layout.align,
                value_size: ma.value.layout.size,
                value_align: ma.value.layout.align,
                thunks: *thunks,
            })));
            Ok(())
        }
        // Dynamic ⋈ Dynamic: both sides are self-describing; the value carries its
        // own structure, so there is nothing to translate — passthrough.
        (Access::Dynamic, Resolved::Composite(SchemaKind::Dynamic)) => {
            require_reader_dynamic(&reader.schema, reg)?;
            out.push(MemOp::Dynamic { field_offset: base });
            Ok(())
        }
        // Result ⋈ enum: the writer's Result wire is a two-variant enum; match Ok/Err
        // by name and translate each arm's payload (writer Ok ⋈ reader Ok, etc.).
        (Access::Result(ra), Resolved::Composite(SchemaKind::Enum { variants: wv, .. })) => {
            out.push(MemOp::Result(Box::new(lower_decode_result(
                &wv, ra, reg, base,
            )?)));
            Ok(())
        }
        // r[impl descriptors.thunk-binding]
        (Access::Pointer(pa), _) => {
            let mut pointee = Vec::new();
            lower_decode_node(writer, &pa.pointee, reg, 0, &mut pointee)?;
            out.push(MemOp::Pointer(Box::new(PointerOp {
                field_offset: base,
                pointee: fuse(pointee),
                pointee_size: pa.pointee.layout.size,
                pointee_align: pa.pointee.layout.align,
                thunks: pa.thunks,
            })));
            Ok(())
        }
        // Opaque ⋈ Bytes: the writer wire is a `Primitive::Bytes` run; the reader
        // carries an opaque adapter. The inner bytes are never translated here — the
        // adapter owns the inner type — so this is the single-schema op verbatim.
        (Access::Opaque(thunks), Resolved::Primitive(Primitive::Bytes)) => {
            require_reader_bytes(&reader.schema, reg)?;
            out.push(MemOp::Opaque(Box::new(OpaqueOp {
                field_offset: base,
                thunks: *thunks,
            })));
            Ok(())
        }
        _ => Err(incompatible("writer and reader schema kinds differ")),
    }
}

fn lower_decode_sequence(
    writer_element: &SchemaRef,
    seq: &SequenceAccess,
    reg: &Registry,
    base: usize,
    out: &mut MemProgram,
) -> Result<()> {
    let SequenceStorage::Vtable(thunks) = &seq.storage else {
        return Err(CompactError::Unsupported(
            "typed: only vtable-backed owned sequences so far",
        ));
    };
    let stride = seq.element.layout.size;
    let elem_align = seq.element.layout.align;
    let mut element = Vec::new();
    lower_decode_node(writer_element, &seq.element, reg, 0, &mut element)?;
    out.push(owned_sequence_op(
        base,
        element,
        stride,
        elem_align,
        validate_any,
        *thunks,
    ));
    Ok(())
}

fn lower_fixed_array(
    element: &Descriptor,
    count: usize,
    stride: usize,
    reg: &Registry,
    base: usize,
    out: &mut MemProgram,
) -> Result<()> {
    lower_fixed_array_elements(count, stride, base, out, |element_base, out| {
        lower_node(element, reg, element_base, out)
    })
}

fn lower_decode_fixed_array(
    writer_element: &SchemaRef,
    element: &Descriptor,
    count: usize,
    stride: usize,
    reg: &Registry,
    base: usize,
    out: &mut MemProgram,
) -> Result<()> {
    lower_fixed_array_elements(count, stride, base, out, |element_base, out| {
        lower_decode_node(writer_element, element, reg, element_base, out)
    })
}

fn require_fixed_array_count(count: usize, dimensions: &[u64]) -> Result<()> {
    let schema_count = compact::product(dimensions)?;
    let descriptor_count = u64::try_from(count)
        .map_err(|_| CompactError::Malformed("descriptor array length overflows u64"))?;
    if schema_count == descriptor_count {
        Ok(())
    } else {
        Err(CompactError::Malformed(
            "descriptor/schema array length mismatch",
        ))
    }
}

fn lower_set(set: &SetAccess, reg: &Registry, base: usize, out: &mut MemProgram) -> Result<()> {
    let SetStorage::Vtable(thunks) = &set.storage;
    let mut element = Vec::new();
    lower_node(&set.element, reg, 0, &mut element)?;
    out.push(set_op(
        base,
        element,
        set.element.layout.size,
        set.element.layout.align,
        *thunks,
    ));
    Ok(())
}

fn lower_decode_set(
    writer_element: &SchemaRef,
    set: &SetAccess,
    reg: &Registry,
    base: usize,
    out: &mut MemProgram,
) -> Result<()> {
    let SetStorage::Vtable(thunks) = &set.storage;
    let mut element = Vec::new();
    lower_decode_node(writer_element, &set.element, reg, 0, &mut element)?;
    out.push(set_op(
        base,
        element,
        set.element.layout.size,
        set.element.layout.align,
        *thunks,
    ));
    Ok(())
}

enum RecordKind {
    Struct,
    Tuple,
}

/// Translate a writer struct's wire fields against the reader's record descriptor.
/// Reader field NAMES come from the reader schema (resolved here), aligned by index
/// with the descriptor's fields (the bridge builds them in the same order).
// r[impl compat.field-matching]
fn lower_decode_record(
    w_fields: &[Field],
    ra: &RecordAccess,
    reader_schema: &SchemaRef,
    record_kind: RecordKind,
    reg: &Registry,
    base: usize,
    out: &mut MemProgram,
) -> Result<()> {
    match &ra.construct {
        Construct::InPlace => {}
        Construct::Thunk(_) => {
            return Err(CompactError::Unsupported("typed: thunk construction"));
        }
    }
    // The reader field names, in the same order as `ra.fields`.
    let r_named = reader_record_fields(reader_schema, record_kind, reg)?;
    if r_named.len() != ra.fields.len() {
        return Err(CompactError::Malformed(
            "descriptor/schema field count mismatch",
        ));
    }

    for step in compat::match_fields(
        w_fields,
        &r_named,
        |ri, _| ra.fields[ri].default.is_some(),
        |rf| {
            incompatible(format!(
                "required reader field '{}' is absent from the writer",
                rf.name
            ))
        },
    )? {
        match step {
            FieldMatch::Take {
                writer,
                reader_index: ri,
            } => {
                let fa = &ra.fields[ri];
                lower_decode_node(&writer.schema, &fa.descriptor, reg, base + fa.offset, out)?;
            }
            FieldMatch::Skip { writer } => {
                out.push(MemOp::SkipWire(Box::new(skip_op(&writer.schema, reg)?)));
            }
            FieldMatch::Default { reader_index: ri } => {
                let fa = &ra.fields[ri];
                let Some(d) = fa.default else {
                    return Err(incompatible(format!(
                        "required reader field '{}' is absent from the writer",
                        r_named[ri].name
                    )));
                };
                out.push(MemOp::Default(Box::new(DefaultOp {
                    offset: base + fa.offset,
                    ctx: d.ctx,
                    default: d.thunk,
                })));
            }
        }
    }
    Ok(())
}

/// Translate a writer enum's variants against the reader's enum descriptor, keyed
/// by WRITER variant index → reader variant matched by NAME. Reader variant names
/// come from the reader schema (resolved here), aligned by index with `ea.variants`.
// r[impl compat.enum]
fn lower_decode_enum(
    w_variants: &[Variant],
    ea: &EnumAccess,
    reader_schema: &SchemaRef,
    reg: &Registry,
    base: usize,
    out: &mut MemProgram,
) -> Result<()> {
    let Tag::Direct { offset, width } = &ea.tag else {
        return Err(CompactError::Unsupported(
            "typed: only #[repr(int)] enums (direct discriminant) so far",
        ));
    };
    let r_named = reader_enum_variants(reader_schema, reg)?;
    if r_named.len() != ea.variants.len() {
        return Err(CompactError::Malformed(
            "descriptor/schema variant count mismatch",
        ));
    }
    let mut variants = Vec::new();
    let mut writer_only = Vec::new();
    for step in compat::match_variants(w_variants, &r_named) {
        match step {
            VariantMatch::Take {
                writer,
                reader_index: ri,
            } => {
                let va = &ea.variants[ri];
                let payload =
                    lower_decode_payload(&writer.payload, va, &r_named[ri].payload, reg, base)?;
                variants.push(EnumVariantOp {
                    wire_index: writer.index,
                    selector: va.selector,
                    payload,
                });
            }
            VariantMatch::WriterOnly { writer } => {
                writer_only.push(writer.index);
            }
        }
    }
    out.push(MemOp::Enum(Box::new(EnumOp {
        tag_offset: base + *offset,
        tag_width: *width,
        variants,
        writer_only,
    })));
    Ok(())
}

/// Translate one matched enum variant's payload (writer payload ⋈ reader payload).
/// The reader payload fields live at base-relative offsets carried by the variant
/// access; their names come from the reader schema payload.
fn lower_decode_payload(
    w: &VariantPayload,
    va: &VariantAccess,
    r_schema_payload: &VariantPayload,
    reg: &Registry,
    base: usize,
) -> Result<MemProgram> {
    let mut payload = Vec::new();
    match (w, r_schema_payload) {
        (VariantPayload::Unit, VariantPayload::Unit) => {}
        (VariantPayload::Newtype(wr), VariantPayload::Newtype(_)) => {
            // A single payload field at the variant's first field offset.
            let fa = va.payload.fields.first().ok_or(CompactError::Malformed(
                "newtype variant has no payload field",
            ))?;
            lower_decode_node(wr, &fa.descriptor, reg, base + fa.offset, &mut payload)?;
        }
        (VariantPayload::Tuple(wrs), VariantPayload::Tuple(rrs)) => {
            if wrs.len() != rrs.len() || wrs.len() != va.payload.fields.len() {
                return Err(incompatible("variant tuple arity differs"));
            }
            // Tuple fields are positional (no names): translate element-wise.
            for (wr, fa) in wrs.iter().zip(&va.payload.fields) {
                lower_decode_node(wr, &fa.descriptor, reg, base + fa.offset, &mut payload)?;
            }
        }
        (VariantPayload::Struct(wfs), VariantPayload::Struct(rfs)) => {
            // A struct-shaped payload matches by field name, like a top-level
            // struct, but at the variant's base-relative offsets. Build a synthetic
            // reader-schema ref is unnecessary: translate against the variant's own
            // record access and the reader schema payload field list.
            lower_decode_variant_struct(wfs, &va.payload, rfs, reg, base, &mut payload)?;
        }
        _ => return Err(incompatible("variant payload shapes differ")),
    }
    Ok(fuse(payload))
}

/// Translate a writer struct-variant payload against the reader's variant record
/// access (matching by name, defaulting reader-only fields), at base-relative
/// offsets. Mirrors `lower_decode_struct` but the reader names come straight from
/// the reader schema payload field list (aligned with the variant's fields).
fn lower_decode_variant_struct(
    w_fields: &[Field],
    ra: &RecordAccess,
    r_fields: &[Field],
    reg: &Registry,
    base: usize,
    out: &mut MemProgram,
) -> Result<()> {
    if r_fields.len() != ra.fields.len() {
        return Err(CompactError::Malformed(
            "variant descriptor/schema field count mismatch",
        ));
    }
    for step in compat::match_fields(
        w_fields,
        r_fields,
        |ri, _| ra.fields[ri].default.is_some(),
        |rf| {
            incompatible(format!(
                "required reader variant field '{}' is absent from the writer",
                rf.name
            ))
        },
    )? {
        match step {
            FieldMatch::Take {
                writer,
                reader_index: ri,
            } => {
                let fa = &ra.fields[ri];
                lower_decode_node(&writer.schema, &fa.descriptor, reg, base + fa.offset, out)?;
            }
            FieldMatch::Skip { writer } => {
                out.push(MemOp::SkipWire(Box::new(skip_op(&writer.schema, reg)?)));
            }
            FieldMatch::Default { reader_index: ri } => {
                let fa = &ra.fields[ri];
                let Some(d) = fa.default else {
                    return Err(incompatible(format!(
                        "required reader variant field '{}' is absent from the writer",
                        r_fields[ri].name
                    )));
                };
                out.push(MemOp::Default(Box::new(DefaultOp {
                    offset: base + fa.offset,
                    ctx: d.ctx,
                    default: d.thunk,
                })));
            }
        }
    }
    Ok(())
}

fn tuple_fields(elements: Vec<SchemaRef>) -> Vec<Field> {
    elements
        .into_iter()
        .enumerate()
        .map(|(i, schema)| Field {
            name: i.to_string(),
            schema,
            required: true,
        })
        .collect()
}

/// The reader record's fields (for names), resolved from a reader schema reference.
fn reader_record_fields(
    r: &SchemaRef,
    record_kind: RecordKind,
    reg: &Registry,
) -> Result<Vec<Field>> {
    match (record_kind, compact::resolve(reg, r)?) {
        (RecordKind::Struct, Resolved::Composite(SchemaKind::Struct { fields, .. })) => Ok(fields),
        (RecordKind::Tuple, Resolved::Composite(SchemaKind::Tuple { elements })) => {
            Ok(tuple_fields(elements))
        }
        _ => Err(incompatible("schema kinds differ")),
    }
}

/// The reader enum's variants (for names + payload shapes), resolved from a reader
/// schema reference.
fn reader_enum_variants(r: &SchemaRef, reg: &Registry) -> Result<Vec<Variant>> {
    match compact::resolve(reg, r)? {
        Resolved::Composite(SchemaKind::Enum { variants, .. }) => Ok(variants),
        _ => Err(CompactError::TypeMismatch {
            expected: "enum reader schema for an enum descriptor",
        }),
    }
}

fn require_reader_list(r: &SchemaRef, reg: &Registry) -> Result<()> {
    match compact::resolve(reg, r)? {
        Resolved::Composite(SchemaKind::List { .. }) => Ok(()),
        _ => Err(incompatible("schema kinds differ")),
    }
}

fn require_reader_set(r: &SchemaRef, reg: &Registry) -> Result<()> {
    match compact::resolve(reg, r)? {
        Resolved::Composite(SchemaKind::Set { .. }) => Ok(()),
        _ => Err(incompatible("schema kinds differ")),
    }
}

fn require_reader_option(r: &SchemaRef, reg: &Registry) -> Result<()> {
    match compact::resolve(reg, r)? {
        Resolved::Composite(SchemaKind::Option { .. }) => Ok(()),
        _ => Err(incompatible("schema kinds differ")),
    }
}

fn require_reader_map(r: &SchemaRef, reg: &Registry) -> Result<()> {
    match compact::resolve(reg, r)? {
        Resolved::Composite(SchemaKind::Map { .. }) => Ok(()),
        _ => Err(incompatible("schema kinds differ")),
    }
}

fn require_reader_dynamic(r: &SchemaRef, reg: &Registry) -> Result<()> {
    match compact::resolve(reg, r)? {
        Resolved::Composite(SchemaKind::Dynamic) => Ok(()),
        _ => Err(incompatible("schema kinds differ")),
    }
}

fn require_reader_bytes(r: &SchemaRef, reg: &Registry) -> Result<()> {
    match compact::resolve(reg, r)? {
        Resolved::Primitive(Primitive::Bytes) => Ok(()),
        _ => Err(incompatible("primitive Bytes is not reader schema")),
    }
}

/// The wire index of the schema enum variant named `name` (`Ok`/`Err` for a
/// `Result`), for lowering a [`ResultOp`].
fn variant_index_by_name(variants: &[Variant], name: &str) -> Result<u32> {
    variants
        .iter()
        .find(|v| v.name == name)
        .map(|v| v.index)
        .ok_or(CompactError::Malformed(
            "Result schema missing Ok or Err variant",
        ))
}

/// Lower a single-schema [`ResultOp`]: take the Ok/Err wire indices from the schema
/// and the Ok/Err payload sub-programs from the descriptor.
fn lower_result(
    ra: &ResultAccess,
    variants: &[Variant],
    reg: &Registry,
    base: usize,
) -> Result<ResultOp> {
    let ok_wire_index = variant_index_by_name(variants, "Ok")?;
    let err_wire_index = variant_index_by_name(variants, "Err")?;
    let mut ok = Vec::new();
    lower_node(&ra.ok, reg, 0, &mut ok)?;
    let mut err = Vec::new();
    lower_node(&ra.err, reg, 0, &mut err)?;
    Ok(ResultOp {
        field_offset: base,
        ok: fuse(ok),
        ok_size: ra.ok.layout.size,
        ok_align: ra.ok.layout.align,
        ok_wire_index,
        err: fuse(err),
        err_size: ra.err.layout.size,
        err_align: ra.err.layout.align,
        err_wire_index,
        thunks: ra.thunks,
    })
}

/// Lower a decode-compat [`ResultOp`]: match the writer enum's Ok/Err variants by
/// name and translate each arm's payload against the reader's Ok/Err descriptor.
fn lower_decode_result(
    wv: &[Variant],
    ra: &ResultAccess,
    reg: &Registry,
    base: usize,
) -> Result<ResultOp> {
    let ok_wv = wv
        .iter()
        .find(|v| v.name == "Ok")
        .ok_or_else(|| incompatible("writer Result schema missing Ok variant"))?;
    let err_wv = wv
        .iter()
        .find(|v| v.name == "Err")
        .ok_or_else(|| incompatible("writer Result schema missing Err variant"))?;
    Ok(ResultOp {
        field_offset: base,
        ok: lower_decode_result_arm(&ok_wv.payload, &ra.ok, reg)?,
        ok_size: ra.ok.layout.size,
        ok_align: ra.ok.layout.align,
        ok_wire_index: ok_wv.index,
        err: lower_decode_result_arm(&err_wv.payload, &ra.err, reg)?,
        err_size: ra.err.layout.size,
        err_align: ra.err.layout.align,
        err_wire_index: err_wv.index,
        thunks: ra.thunks,
    })
}

/// Translate one `Result` arm: the writer payload is a newtype (`Ok(T)`/`Err(E)`),
/// translated against the reader arm's descriptor at offset 0 (the arm value start).
fn lower_decode_result_arm(
    w: &VariantPayload,
    reader: &Descriptor,
    reg: &Registry,
) -> Result<MemProgram> {
    let VariantPayload::Newtype(wr) = w else {
        return Err(incompatible("Result arm payload must be a newtype"));
    };
    let mut prog = Vec::new();
    lower_decode_node(wr, reader, reg, 0, &mut prog)?;
    Ok(fuse(prog))
}

// ============================================================================
// Wire-skeleton lowering (skip a writer-only value)
// ============================================================================

/// Resolve a writer schema reference into a [`SkipOp`] wire skeleton — a pre-built
/// recipe to advance the cursor past one value of that schema without touching
/// memory. Used for writer-only fields (`r[compat.skip-writer-only]`).
///
/// # Errors
/// [`CompactError::Unsupported`] for a kind the skip walker does not carry, or a
/// resolution error.
fn skip_op(writer: &SchemaRef, reg: &Registry) -> Result<SkipOp> {
    match compact::resolve(reg, writer)? {
        Resolved::Primitive(p) => match p {
            Primitive::String | Primitive::Bytes => Ok(SkipOp::Bytes {
                stride: 1,
                elem_align: 1,
            }),
            other => {
                let size = fixed_size(other).ok_or(CompactError::Unsupported(
                    "skip: variable-length scalar (datetime/uuid/qname)",
                ))?;
                Ok(SkipOp::Scalar {
                    size,
                    align: alignment(other),
                })
            }
        },
        Resolved::Composite(kind) => match kind {
            SchemaKind::Struct { fields, .. } => {
                let mut fs = Vec::with_capacity(fields.len());
                for f in &fields {
                    fs.push(skip_op(&f.schema, reg)?);
                }
                Ok(SkipOp::Struct(fs))
            }
            SchemaKind::Tuple { elements } => {
                let mut fs = Vec::with_capacity(elements.len());
                for e in &elements {
                    fs.push(skip_op(e, reg)?);
                }
                Ok(SkipOp::Struct(fs))
            }
            SchemaKind::Enum { variants, .. } => {
                let mut arms = Vec::with_capacity(variants.len());
                for v in &variants {
                    let fields = match &v.payload {
                        VariantPayload::Unit => Vec::new(),
                        VariantPayload::Newtype(r) => vec![skip_op(r, reg)?],
                        VariantPayload::Tuple(rs) => {
                            let mut fs = Vec::with_capacity(rs.len());
                            for r in rs {
                                fs.push(skip_op(r, reg)?);
                            }
                            fs
                        }
                        VariantPayload::Struct(fields) => {
                            let mut fs = Vec::with_capacity(fields.len());
                            for f in fields {
                                fs.push(skip_op(&f.schema, reg)?);
                            }
                            fs
                        }
                    };
                    arms.push((v.index, fields));
                }
                Ok(SkipOp::Enum(arms))
            }
            SchemaKind::List { element } | SchemaKind::Set { element } => {
                // A bulk byte run when the element is a fixed scalar covering its
                // own size (no inter-element wire padding), else a per-element seq.
                if let Resolved::Primitive(ep) = compact::resolve(reg, &element)?
                    && let Some(size) = fixed_size(ep)
                    && !matches!(ep, Primitive::String | Primitive::Bytes)
                {
                    let align = alignment(ep);
                    if size % align == 0 {
                        return Ok(SkipOp::Bytes {
                            stride: size,
                            elem_align: align,
                        });
                    }
                }
                Ok(SkipOp::Seq(Box::new(skip_op(&element, reg)?)))
            }
            SchemaKind::Option { element } => Ok(SkipOp::Option(Box::new(skip_op(&element, reg)?))),
            SchemaKind::Map { key, value } => Ok(SkipOp::Map(
                Box::new(skip_op(&key, reg)?),
                Box::new(skip_op(&value, reg)?),
            )),
            SchemaKind::Array { .. } => Err(CompactError::Unsupported("skip: fixed array")),
            SchemaKind::Tensor { .. } => Err(CompactError::Unsupported("skip: tensor")),
            SchemaKind::Channel { .. } => Err(CompactError::Unsupported("skip: channel")),
            SchemaKind::External { .. } => Err(CompactError::Unsupported("skip: external")),
            // A self-describing value is self-delimiting: skip it by decoding one
            // value and discarding it.
            SchemaKind::Dynamic => Ok(SkipOp::Dynamic),
            SchemaKind::Primitive(_) => {
                // A composite that resolved to a primitive kind: treat as scalar.
                Err(CompactError::Malformed(
                    "skip: primitive in composite position",
                ))
            }
        },
    }
}

/// Read `width` (1/2/4/8) little-endian bytes at `ptr` as a `u64`.
///
/// # Safety
/// `ptr` must be readable for `width` bytes.
unsafe fn read_uint(ptr: *const u8, width: usize) -> u64 {
    let mut buf = [0u8; 8];
    // Safety: forwarded; `width <= 8`.
    unsafe { core::ptr::copy_nonoverlapping(ptr, buf.as_mut_ptr(), width) };
    u64::from_le_bytes(buf)
}

/// Write the low `width` (1/2/4/8) bytes of `val` little-endian at `ptr`.
///
/// # Safety
/// `ptr` must be writable for `width` bytes.
unsafe fn write_uint(ptr: *mut u8, width: usize, val: u64) {
    let bytes = val.to_le_bytes();
    // Safety: forwarded; `width <= 8`.
    unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, width) };
}

fn sign_extend(raw: u64, width: usize) -> i64 {
    if width >= 8 {
        raw as i64
    } else {
        let shift = 64 - width * 8;
        ((raw << shift) as i64) >> shift
    }
}

fn signed_fits_width(value: i64, width: usize) -> bool {
    if width >= 8 {
        return true;
    }
    let bits = width * 8;
    let min = -(1i64 << (bits - 1));
    let max = (1i64 << (bits - 1)) - 1;
    (min..=max).contains(&value)
}

/// A mask of the low `width` bytes (`width >= 8` → all ones).
fn width_mask(width: usize) -> u64 {
    if width >= 8 {
        u64::MAX
    } else {
        (1u64 << (width * 8)) - 1
    }
}

/// `ByteValidator` for `String` byte runs: the bytes must be valid UTF-8
/// (`r[validate.text]`). Both the interpreter and the JIT call this.
///
/// # Safety
/// `ptr` must point to `len` readable bytes (`len == 0` permits any non-null,
/// aligned `ptr`, which a slice's `as_ptr` always satisfies).
unsafe extern "C" fn validate_utf8(ptr: *const u8, len: usize) -> bool {
    // Safety: forwarded — `ptr`/`len` describe a readable byte run.
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    core::str::from_utf8(bytes).is_ok()
}

/// `ByteValidator` for byte runs with no content constraint — `Vec<u8>` and
/// bulk `Vec<scalar>` runs accept any bytes.
///
/// # Safety
/// Reads nothing; the signature matches `ByteValidator`.
unsafe extern "C" fn validate_any(_ptr: *const u8, _len: usize) -> bool {
    true
}

// ============================================================================
// Encode
// ============================================================================

/// Encode the value at `base` into compact bytes, by a prebuilt program.
///
/// # Safety
/// `base` must point to an initialized value matching the descriptor the program
/// was lowered from, readable for every `offset + size` the program touches.
#[must_use]
pub unsafe fn encode_with(lowered: &Lowered, base: *const u8) -> Vec<u8> {
    let mut out = Vec::new();
    let mut interp = EncodeInterp {
        base,
        out: &mut out,
    };
    match weavy::run_program(&lowered.program, &lowered.blocks, &mut interp) {
        Ok(()) => {}
        Err(RunError::MissingBlock(_)) => panic!("CallBlock references a lowered recursion block"),
        Err(RunError::Step(err)) => match err {},
    }
    out
}

/// Encoded bytes plus the generic Weavy runner counters that produced them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodeReport {
    pub bytes: Vec<u8>,
    pub stats: RunStats,
}

/// Encode the value at `base` and return generic Weavy runner counters.
///
/// # Safety
/// As [`encode_with`].
#[must_use]
pub unsafe fn encode_with_stats(lowered: &Lowered, base: *const u8) -> EncodeReport {
    let mut bytes = Vec::new();
    let mut interp = EncodeInterp {
        base,
        out: &mut bytes,
    };
    let stats = match weavy::run_program_with_stats(&lowered.program, &lowered.blocks, &mut interp)
    {
        Ok(stats) => stats,
        Err(RunError::MissingBlock(_)) => panic!("CallBlock references a lowered recursion block"),
        Err(RunError::Step(err)) => match err {},
    };
    EncodeReport { bytes, stats }
}

struct EncodeInterp<'out> {
    base: *const u8,
    out: &'out mut Vec<u8>,
}

enum EncodeContinuation<'program> {
    RestoreBase {
        previous_base: *const u8,
    },
    Sequence {
        previous_base: *const u8,
        element: &'program MemProgram,
        data: *const u8,
        stride: usize,
        next_index: usize,
        len: usize,
    },
    Set {
        previous_base: *const u8,
        element: &'program MemProgram,
        iter: *mut (),
        thunks: phon_ir::SetThunks,
    },
    MapKey {
        previous_base: *const u8,
        map: &'program MapOp,
        iter: *mut (),
        value: *const u8,
    },
    MapValue {
        previous_base: *const u8,
        map: &'program MapOp,
        iter: *mut (),
    },
}

impl<'program> Step<'program, SchemaId, MemOp> for EncodeInterp<'_> {
    type Error = Infallible;
    type Continuation = EncodeContinuation<'program>;

    fn step(
        &mut self,
        op: &'program MemOp,
    ) -> core::result::Result<Control<'program, SchemaId, MemOp, Self::Continuation>, Self::Error>
    {
        Ok(match op {
            // A recursive back-edge: run the callee schema's block at `base + offset`.
            MemOp::CallBlock { schema, offset } => {
                // Safety: the recursive value lives at `base + offset`.
                let previous_base = self.base;
                self.base = unsafe { self.base.add(*offset) };
                Control::CallBlockThen(*schema, EncodeContinuation::RestoreBase { previous_base })
            }
            MemOp::Scalar {
                offset,
                size,
                align,
            } => {
                pad_to(self.out, *align);
                // Safety: the value is valid for reads over this field's bytes.
                let src = unsafe { core::slice::from_raw_parts(self.base.add(*offset), *size) };
                self.out.extend_from_slice(src);
                Control::Continue
            }
            MemOp::ScalarRun(run) => {
                for segment in &run.segments {
                    pad_to(self.out, segment.align);
                    // Safety: the value is valid for reads over this field's bytes.
                    let src = unsafe {
                        core::slice::from_raw_parts(self.base.add(segment.offset), segment.size)
                    };
                    self.out.extend_from_slice(src);
                }
                Control::Continue
            }
            MemOp::NativeInt {
                offset,
                mem_size,
                signed,
            } => {
                pad_to(self.out, 8);
                // Safety: the native integer field is readable over `mem_size` bytes.
                let raw = unsafe { read_uint(self.base.add(*offset), *mem_size) };
                if *signed {
                    self.out
                        .extend_from_slice(&sign_extend(raw, *mem_size).to_le_bytes());
                } else {
                    self.out.extend_from_slice(&raw.to_le_bytes());
                }
                Control::Continue
            }
            MemOp::Sequence(s) => {
                // Safety: the sequence handle lives at `field_offset`.
                let list = unsafe { self.base.add(s.field_offset) };
                let n = unsafe { (s.thunks.len)(s.thunks.ctx, list) };
                write_u32(self.out, n as u32);
                let data = unsafe { (s.thunks.data)(s.thunks.ctx, list) };
                if n == 0 {
                    Control::Continue
                } else {
                    let previous_base = self.base;
                    self.base = data;
                    Control::CallProgramThen(
                        &s.element,
                        EncodeContinuation::Sequence {
                            previous_base,
                            element: &s.element,
                            data,
                            stride: s.stride,
                            next_index: 1,
                            len: n,
                        },
                    )
                }
            }
            MemOp::Set(s) => {
                // Safety: the set handle lives at `field_offset`.
                let set = unsafe { self.base.add(s.field_offset) };
                let n = unsafe { (s.thunks.len)(s.thunks.ctx, set) };
                write_u32(self.out, n as u32);
                let it = unsafe { (s.thunks.iter_init)(s.thunks.ctx, set) };
                self.call_next_set(self.base, &s.element, it, s.thunks)
            }
            MemOp::Bytes(b) => {
                // Safety: the handle lives at field_offset; one bulk read of its
                // contiguous `count * stride` bytes.
                let list = unsafe { self.base.add(b.field_offset) };
                let count = unsafe { (b.thunks.len)(b.thunks.ctx, list) };
                write_u32(self.out, count as u32);
                // Alignment pads BEFORE an element's bytes; an empty run has no
                // elements, so it writes no padding (`r[compact.alignment]`).
                if count > 0 {
                    pad_to(self.out, b.elem_align);
                }
                let data = unsafe { (b.thunks.data)(b.thunks.ctx, list) };
                let src = unsafe { core::slice::from_raw_parts(data, count * b.stride) };
                self.out.extend_from_slice(src);
                Control::Continue
            }
            // Encode of a borrowed leaf is byte-identical to the owned bulk run: the
            // `&str`/`&[u8]` reads its length and contiguous bytes through the borrow
            // thunks and writes them as a `u32` count + `count * stride` bytes.
            MemOp::Borrow(b) => {
                // Safety: the borrowed handle (fat pointer) lives at field_offset.
                let field = unsafe { self.base.add(b.field_offset) };
                let count = unsafe { (b.thunks.len)(b.thunks.ctx, field) };
                write_u32(self.out, count as u32);
                // Alignment pads BEFORE an element's bytes; an empty run has no
                // elements, so it writes no padding (`r[compact.alignment]`).
                if count > 0 {
                    pad_to(self.out, b.elem_align);
                }
                let data = unsafe { (b.thunks.data)(b.thunks.ctx, field) };
                let src = unsafe { core::slice::from_raw_parts(data, count * b.stride) };
                self.out.extend_from_slice(src);
                Control::Continue
            }
            MemOp::Option(o) => {
                // Safety: the option handle lives at field_offset.
                let option = unsafe { self.base.add(o.field_offset) };
                if unsafe { (o.thunks.is_some)(o.thunks.ctx, option) } {
                    write_u8(self.out, 1);
                    // Safety: present, so `get_value` returns a valid inner pointer.
                    let inner = unsafe { (o.thunks.get_value)(o.thunks.ctx, option) };
                    let previous_base = self.base;
                    self.base = inner;
                    Control::CallProgramThen(
                        &o.some,
                        EncodeContinuation::RestoreBase { previous_base },
                    )
                } else {
                    write_u8(self.out, 0);
                    Control::Continue
                }
            }
            MemOp::Enum(e) => {
                // Read the in-memory discriminant to pick the active variant.
                // Safety: the discriminant lives at base + tag_offset, tag_width wide.
                let disc = unsafe { read_uint(self.base.add(e.tag_offset), e.tag_width) };
                let mask = width_mask(e.tag_width);
                let variant = e
                    .variants
                    .iter()
                    .find(|v| (v.selector & mask) == (disc & mask))
                    .expect("enum discriminant matches no modelled variant (invalid value)");
                write_u32(self.out, variant.wire_index);
                // The payload fields live at base-relative offsets (same base).
                Control::CallProgram(&variant.payload)
            }
            MemOp::Map(m) => {
                // Safety: the map handle lives at field_offset.
                let map = unsafe { self.base.add(m.field_offset) };
                let n = unsafe { (m.thunks.len)(m.thunks.ctx, map) };
                write_u32(self.out, n as u32);
                // Drive a stateful iterator over the entries, encoding each
                // (key, value) pair in turn.
                let it = unsafe { (m.thunks.iter_init)(m.thunks.ctx, map) };
                self.call_next_map(self.base, m, it)
            }
            // r[impl ir.memory] — a self-describing dynamic `Value`: write it through
            // the self-describing codec (self-delimiting; no length prefix).
            MemOp::Dynamic { field_offset } => {
                // Safety: the field at `field_offset` is an initialized `Value`.
                let v = unsafe { &*self.base.add(*field_offset).cast::<Value>() };
                write_value(self.out, v)
                    .expect("dynamic value is encodable by the self-describing codec");
                Control::Continue
            }
            // r[impl ir.memory] — Result<T, E>: read which arm is active via the
            // vtable, write its wire index, then encode that arm's payload at the
            // pointer the getter returns (the repr(Rust) layout is never assumed).
            MemOp::Result(rs) => {
                // Safety: the result handle lives at field_offset.
                let result = unsafe { self.base.add(rs.field_offset) };
                if unsafe { (rs.thunks.is_ok)(rs.thunks.ctx, result) } {
                    write_u32(self.out, rs.ok_wire_index);
                    // Safety: Ok, so `get_ok` returns a valid inner pointer.
                    let ok = unsafe { (rs.thunks.get_ok)(rs.thunks.ctx, result) };
                    let previous_base = self.base;
                    self.base = ok;
                    Control::CallProgramThen(
                        &rs.ok,
                        EncodeContinuation::RestoreBase { previous_base },
                    )
                } else {
                    write_u32(self.out, rs.err_wire_index);
                    // Safety: Err, so `get_err` returns a valid inner pointer.
                    let err = unsafe { (rs.thunks.get_err)(rs.thunks.ctx, result) };
                    let previous_base = self.base;
                    self.base = err;
                    Control::CallProgramThen(
                        &rs.err,
                        EncodeContinuation::RestoreBase { previous_base },
                    )
                }
            }
            // r[impl descriptors.thunk-binding]
            MemOp::Pointer(p) => {
                // Safety: the owning pointer handle lives at field_offset.
                let pointer = unsafe { self.base.add(p.field_offset) };
                // Safety: `borrow` returns a valid pointee pointer for initialized
                // strong pointers such as Box/Rc/Arc.
                let pointee = unsafe { (p.thunks.borrow)(p.thunks.ctx, pointer) };
                let previous_base = self.base;
                self.base = pointee;
                Control::CallProgramThen(
                    &p.pointee,
                    EncodeContinuation::RestoreBase { previous_base },
                )
            }
            // r[impl ir.memory] — opaque field: reserve a `u32`
            // length (align 1 — wire-identical to a `Primitive::Bytes` run, so no
            // pre-pad), append the inner bytes via the thunk, then backpatch the
            // length. The backpatch is what fixed-width (non-varint) framing buys.
            MemOp::Opaque(o) => {
                // Safety: the opaque field lives at `field_offset`.
                let field = unsafe { self.base.add(o.field_offset) };
                let len_pos = self.out.len();
                write_u32(self.out, 0); // length placeholder, backpatched below
                let start = self.out.len();
                // Safety: `field` points at the opaque field; the thunk appends the
                // inner value's encoded bytes to `out`.
                unsafe { (o.thunks.encode)(o.thunks.ctx, field, core::ptr::from_mut(self.out)) };
                let inner_len = (self.out.len() - start) as u32;
                self.out[len_pos..len_pos + 4].copy_from_slice(&inner_len.to_le_bytes());
                Control::Continue
            }
            // Compat-only decode ops never appear in an encode program (encode is
            // single-schema: `lower`, not `lower_decode`).
            MemOp::SkipWire(_) | MemOp::Default(_) => {
                unreachable!("typed encode never emits compat skip/default ops")
            }
        })
    }

    fn after_return(
        &mut self,
        continuation: Self::Continuation,
    ) -> core::result::Result<Control<'program, SchemaId, MemOp, Self::Continuation>, Self::Error>
    {
        Ok(match continuation {
            EncodeContinuation::RestoreBase { previous_base } => {
                self.base = previous_base;
                Control::Continue
            }
            EncodeContinuation::Sequence {
                previous_base,
                element,
                data,
                stride,
                next_index,
                len,
            } => {
                if next_index == len {
                    self.base = previous_base;
                    Control::Continue
                } else {
                    self.base = unsafe { data.add(next_index * stride) };
                    Control::CallProgramThen(
                        element,
                        EncodeContinuation::Sequence {
                            previous_base,
                            element,
                            data,
                            stride,
                            next_index: next_index + 1,
                            len,
                        },
                    )
                }
            }
            EncodeContinuation::Set {
                previous_base,
                element,
                iter,
                thunks,
            } => self.call_next_set(previous_base, element, iter, thunks),
            EncodeContinuation::MapKey {
                previous_base,
                map,
                iter,
                value,
            } => {
                self.base = value;
                Control::CallProgramThen(
                    &map.value,
                    EncodeContinuation::MapValue {
                        previous_base,
                        map,
                        iter,
                    },
                )
            }
            EncodeContinuation::MapValue {
                previous_base,
                map,
                iter,
            } => self.call_next_map(previous_base, map, iter),
        })
    }
}

impl EncodeInterp<'_> {
    fn call_next_set<'program>(
        &mut self,
        previous_base: *const u8,
        element: &'program MemProgram,
        iter: *mut (),
        thunks: phon_ir::SetThunks,
    ) -> Control<'program, SchemaId, MemOp, EncodeContinuation<'program>> {
        let mut value: *const u8 = core::ptr::null();
        // Safety: `iter` is a live iterator; the out-param is valid.
        if unsafe { (thunks.iter_next)(thunks.ctx, iter, &mut value) } {
            self.base = value;
            Control::CallProgramThen(
                element,
                EncodeContinuation::Set {
                    previous_base,
                    element,
                    iter,
                    thunks,
                },
            )
        } else {
            // Safety: `iter` was built by `iter_init` and is freed exactly once.
            unsafe { (thunks.iter_dealloc)(thunks.ctx, iter) };
            self.base = previous_base;
            Control::Continue
        }
    }

    fn call_next_map<'program>(
        &mut self,
        previous_base: *const u8,
        map: &'program MapOp,
        iter: *mut (),
    ) -> Control<'program, SchemaId, MemOp, EncodeContinuation<'program>> {
        let mut key: *const u8 = core::ptr::null();
        let mut value: *const u8 = core::ptr::null();
        // Safety: `iter` is a live iterator; the out-params are valid.
        if unsafe { (map.thunks.iter_next)(map.thunks.ctx, iter, &mut key, &mut value) } {
            self.base = key;
            Control::CallProgramThen(
                &map.key,
                EncodeContinuation::MapKey {
                    previous_base,
                    map,
                    iter,
                    value,
                },
            )
        } else {
            // Safety: `iter` was built by `iter_init` and is freed exactly once.
            unsafe { (map.thunks.iter_dealloc)(map.thunks.ctx, iter) };
            self.base = previous_base;
            Control::Continue
        }
    }
}

/// Lower `descriptor` and encode the value at `base` in one step.
///
/// # Safety
/// As [`encode_with`].
///
/// # Errors
/// As [`lower`].
pub unsafe fn encode(
    base: *const u8,
    descriptor: &Descriptor,
    descriptor_blocks: &HashMap<SchemaId, Descriptor>,
    reg: &Registry,
) -> Result<Vec<u8>> {
    let lowered = lower_typed(descriptor, descriptor_blocks, reg)?;
    // Safety: forwarded from this function's contract.
    Ok(unsafe { encode_with(&lowered, base) })
}

// ============================================================================
// Decode
// ============================================================================

/// Decode compact `bytes` into the value at `base`, by a prebuilt program,
/// rejecting trailing bytes.
///
/// # Safety
/// `base` must point to writable, suitably sized and aligned uninitialized
/// storage for the descriptor the program was lowered from. On `Ok` the bytes it
/// covers are initialized.
///
/// # Errors
/// [`CompactError`] for malformed or trailing input.
pub unsafe fn decode_with(lowered: &Lowered, bytes: &[u8], base: *mut u8) -> Result<()> {
    let mut interp = DecodeInterp {
        reader: Reader::new(bytes),
        base,
    };
    match weavy::run_program(&lowered.program, &lowered.blocks, &mut interp) {
        Ok(()) => {}
        Err(RunError::Step(err)) => return Err(err),
        Err(RunError::MissingBlock(_)) => {
            panic!("CallBlock references a lowered recursion block")
        }
    }
    if interp.reader.remaining() != 0 {
        return Err(CompactError::Decode(DecodeError::TrailingBytes(
            interp.reader.remaining(),
        )));
    }
    Ok(())
}

/// Decode compact `bytes` into `base` and return generic Weavy runner counters.
///
/// # Safety
/// As [`decode_with`].
///
/// # Errors
/// [`CompactError`] for malformed or trailing input.
pub unsafe fn decode_with_stats(
    lowered: &Lowered,
    bytes: &[u8],
    base: *mut u8,
) -> Result<RunStats> {
    let mut interp = DecodeInterp {
        reader: Reader::new(bytes),
        base,
    };
    let stats = match weavy::run_program_with_stats(&lowered.program, &lowered.blocks, &mut interp)
    {
        Ok(stats) => stats,
        Err(RunError::Step(err)) => return Err(err),
        Err(RunError::MissingBlock(_)) => {
            panic!("CallBlock references a lowered recursion block")
        }
    };
    if interp.reader.remaining() != 0 {
        return Err(CompactError::Decode(DecodeError::TrailingBytes(
            interp.reader.remaining(),
        )));
    }
    Ok(stats)
}

struct DecodeInterp<'bytes> {
    reader: Reader<'bytes>,
    base: *mut u8,
}

struct Scratch {
    ptr: *mut u8,
    layout: Option<alloc::Layout>,
}

impl Scratch {
    fn new(size: usize, align: usize) -> Result<Self> {
        let (ptr, layout) = alloc_scratch(size, align)?;
        Ok(Self { ptr, layout })
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        free_scratch(self.ptr, self.layout);
        self.layout = None;
    }
}

struct SequenceBuffer {
    ptr: *mut u8,
    cap: usize,
    stride: usize,
    layout: Option<alloc::Layout>,
}

impl SequenceBuffer {
    fn new(count: usize, stride: usize, elem_align: usize) -> Result<Self> {
        if count == 0 || stride == 0 {
            Ok(Self {
                ptr: elem_align as *mut u8,
                cap: count,
                stride,
                layout: None,
            })
        } else {
            let layout =
                alloc::Layout::from_size_align(count * stride, elem_align).map_err(|_| {
                    CompactError::Decode(DecodeError::Malformed("sequence layout overflow"))
                })?;
            // Safety: layout has non-zero size (count > 0 and stride > 0).
            let ptr = unsafe { alloc::alloc(layout) };
            if ptr.is_null() {
                alloc::handle_alloc_error(layout);
            }
            Ok(Self {
                ptr,
                cap: count,
                stride,
                layout: Some(layout),
            })
        }
    }

    fn adopt(&mut self) {
        self.layout = None;
    }
}

impl Drop for SequenceBuffer {
    fn drop(&mut self) {
        if let Some(layout) = self.layout.take() {
            // Safety: `ptr` was allocated with this layout and has not been adopted.
            unsafe { alloc::dealloc(self.ptr, layout) };
        }
    }
}

enum DecodeContinuation<'program> {
    RestoreBase {
        previous_base: *mut u8,
    },
    Sequence {
        previous_base: *mut u8,
        element: &'program MemProgram,
        buffer: SequenceBuffer,
        next_index: usize,
        count: usize,
        list: *mut u8,
        thunks: phon_ir::SeqThunks,
    },
    SetElement {
        previous_base: *mut u8,
        set: &'program SetOp,
        set_ptr: *mut u8,
        remaining_after_current: usize,
        scratch: Scratch,
    },
    MapKey {
        previous_base: *mut u8,
        map: &'program MapOp,
        map_ptr: *mut u8,
        total: usize,
        remaining: usize,
        key_scratch: Scratch,
        value_scratch: Scratch,
    },
    MapValue {
        previous_base: *mut u8,
        map: &'program MapOp,
        map_ptr: *mut u8,
        total: usize,
        remaining: usize,
        key_scratch: Scratch,
        value_scratch: Scratch,
    },
    Init {
        previous_base: *mut u8,
        target: InitTarget,
        scratch: Scratch,
    },
}

impl<'program> Step<'program, SchemaId, MemOp> for DecodeInterp<'_> {
    type Error = CompactError;
    type Continuation = DecodeContinuation<'program>;

    fn step(
        &mut self,
        op: &'program MemOp,
    ) -> Result<Control<'program, SchemaId, MemOp, Self::Continuation>> {
        Ok(match op {
            // A recursive back-edge: run the callee schema's block at `base + offset`.
            MemOp::CallBlock { schema, offset } => {
                // Safety: the recursive value lives at `base + offset`.
                let previous_base = self.base;
                self.base = unsafe { self.base.add(*offset) };
                Control::CallBlockThen(*schema, DecodeContinuation::RestoreBase { previous_base })
            }
            MemOp::Scalar {
                offset,
                size,
                align,
            } => {
                skip_pad(&mut self.reader, *align)?;
                let src = self.reader.read_slice(*size)?;
                // Safety: `base` is valid for writes over this field's bytes, and
                // the wire bytes equal the in-memory bytes for a fixed scalar.
                unsafe {
                    core::ptr::copy_nonoverlapping(src.as_ptr(), self.base.add(*offset), *size)
                };
                Control::Continue
            }
            MemOp::ScalarRun(run) => {
                for segment in &run.segments {
                    skip_pad(&mut self.reader, segment.align)?;
                    let src = self.reader.read_slice(segment.size)?;
                    // Safety: forwarded from the grouped scalar op: each segment
                    // writes within a fixed scalar field, and gaps are untouched.
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            src.as_ptr(),
                            self.base.add(segment.offset),
                            segment.size,
                        )
                    };
                }
                Control::Continue
            }
            MemOp::NativeInt {
                offset,
                mem_size,
                signed,
            } => {
                skip_pad(&mut self.reader, 8)?;
                if *signed {
                    let value = self.reader.read_i64()?;
                    if !signed_fits_width(value, *mem_size) {
                        return Err(DecodeError::Malformed(
                            "native-sized signed integer out of range",
                        )
                        .into());
                    }
                    // Safety: `base + offset` is writable for the native integer field.
                    unsafe { write_uint(self.base.add(*offset), *mem_size, value as u64) };
                } else {
                    let value = self.reader.read_u64()?;
                    if *mem_size < 8 && value > width_mask(*mem_size) {
                        return Err(DecodeError::Malformed(
                            "native-sized unsigned integer out of range",
                        )
                        .into());
                    }
                    // Safety: `base + offset` is writable for the native integer field.
                    unsafe { write_uint(self.base.add(*offset), *mem_size, value) };
                }
                Control::Continue
            }
            MemOp::Sequence(s) => {
                let count = self.reader.read_len(s.min_wire)?;
                let mut buffer = SequenceBuffer::new(count, s.stride, s.elem_align)?;
                // Safety: the handle lives at `field_offset`.
                let list = unsafe { self.base.add(s.field_offset) };
                if count == 0 {
                    unsafe {
                        (s.thunks.from_raw_parts)(s.thunks.ctx, list, buffer.ptr, count, buffer.cap)
                    };
                    buffer.adopt();
                    Control::Continue
                } else {
                    let previous_base = self.base;
                    self.base = buffer.ptr;
                    Control::CallProgramThen(
                        &s.element,
                        DecodeContinuation::Sequence {
                            previous_base,
                            element: &s.element,
                            buffer,
                            next_index: 1,
                            count,
                            list,
                            thunks: s.thunks,
                        },
                    )
                }
            }
            MemOp::Set(s) => {
                let count = self.reader.read_len(s.min_wire)?;
                // Safety: the set handle lives at field_offset.
                let set = unsafe { self.base.add(s.field_offset) };
                // Initialize the (uninitialized) set with room for `count` entries.
                // NOTE: a decode error after this point leaks the partial set — the
                // same trivially-droppable limitation as sequences/options/maps.
                unsafe { (s.thunks.init_with_capacity)(s.thunks.ctx, set, count) };
                self.decode_next_set_element(self.base, s, set, count)?
            }
            MemOp::Bytes(b) => {
                let count = self.reader.read_len(b.stride.max(1))?;
                // Symmetric with encode: only an empty run skips no padding.
                if count > 0 {
                    skip_pad(&mut self.reader, b.elem_align)?;
                }
                let total = count * b.stride;
                let src = self.reader.read_slice(total)?;
                // r[impl validate.text] — validate the run before adopting it
                // (UTF-8 for `String`, a no-op for `Vec`). The JIT calls the very
                // same thunk, so both engines share one validation path.
                // Safety: `src` is `total` readable bytes.
                if !unsafe { (b.validate)(src.as_ptr(), total) } {
                    return Err(CompactError::Decode(DecodeError::InvalidUtf8));
                }
                // Allocate, bulk-copy the run in, adopt it via `from_raw_parts`.
                let (buffer, cap) = if total == 0 {
                    (b.elem_align as *mut u8, 0usize)
                } else {
                    let layout =
                        alloc::Layout::from_size_align(total, b.elem_align).map_err(|_| {
                            CompactError::Decode(DecodeError::Malformed("bytes layout overflow"))
                        })?;
                    // Safety: total > 0.
                    let buf = unsafe { alloc::alloc(layout) };
                    if buf.is_null() {
                        alloc::handle_alloc_error(layout);
                    }
                    // Safety: src and buf are both `total` bytes, non-overlapping.
                    unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), buf, total) };
                    (buf, count)
                };
                // Safety: the handle lives at field_offset; `from_raw_parts` adopts
                // the `count`-element buffer.
                let list = unsafe { self.base.add(b.field_offset) };
                unsafe { (b.thunks.from_raw_parts)(b.thunks.ctx, list, buffer, count, cap) };
                Control::Continue
            }
            // r[impl ir.memory] — BORROWED, zero-copy `&str`/`&[u8]`: same wire as
            // `Bytes`, but write a fat pointer INTO the input `bytes` — NO alloc, NO
            // copy. The written `&str`/`&[u8]` borrows the reader's input buffer, so
            // the caller must keep `bytes` alive as long as the decoded value (the
            // standard zero-copy contract, documented on `decode_with`'s `Safety`).
            MemOp::Borrow(b) => {
                let count = self.reader.read_len(b.stride.max(1))?;
                // Symmetric with encode: only an empty run skips no padding.
                if count > 0 {
                    skip_pad(&mut self.reader, b.elem_align)?;
                }
                let total = count * b.stride;
                // `src` is a slice INTO the input `bytes` (no copy): the borrowed
                // value will point at exactly these bytes.
                let src = self.reader.read_slice(total)?;
                // Safety: the borrowed handle lives at field_offset; the construct
                // thunk builds the `&str`/`&[u8]` fat pointer there, pointing into the
                // input. Returns false on invalid content (e.g. non-UTF-8 `&str`).
                let field = unsafe { self.base.add(b.field_offset) };
                if !unsafe { (b.thunks.set_borrowed)(b.thunks.ctx, field, src.as_ptr(), count) } {
                    return Err(CompactError::Decode(DecodeError::InvalidUtf8));
                }
                Control::Continue
            }
            MemOp::Option(o) => {
                // Safety: the option handle lives at field_offset.
                let option = unsafe { self.base.add(o.field_offset) };
                match self.reader.read_u8()? {
                    0 => {
                        unsafe { (o.thunks.init_none)(o.thunks.ctx, option) };
                        Control::Continue
                    }
                    1 => {
                        // Decode the inner into an engine-owned scratch buffer, then
                        // move it into the Option (init_some does a ptr::read) and
                        // free the scratch WITHOUT dropping (ownership transferred).
                        self.decode_into_via_init(
                            &o.some,
                            o.inner_size,
                            o.inner_align,
                            InitTarget {
                                ctx: o.thunks.ctx,
                                handle: option,
                                init: o.thunks.init_some,
                            },
                        )?
                    }
                    b => return Err(CompactError::Decode(DecodeError::InvalidBool(b))),
                }
            }
            MemOp::Enum(e) => {
                let wire_index = self.reader.read_u32()?;
                let variant = match e.variants.iter().find(|v| v.wire_index == wire_index) {
                    Some(v) => v,
                    None if e.writer_only.contains(&wire_index) => {
                        // A variant the writer has but the reader lacks
                        // (`r[compat.enum]`) — the same error plan.rs reports.
                        return Err(CompactError::WriterOnlyVariant(wire_index));
                    }
                    None => return Err(CompactError::BadVariantIndex(wire_index)),
                };
                // Write the in-memory discriminant, then decode the payload fields
                // (disjoint memory: the discriminant precedes every field).
                // Safety: the discriminant lives at base + tag_offset, tag_width wide.
                unsafe { write_uint(self.base.add(e.tag_offset), e.tag_width, variant.selector) };
                // Safety: payload fields write within the enum's storage at base.
                Control::CallProgram(&variant.payload)
            }
            MemOp::Map(m) => {
                let n = self.reader.read_len(1)?;
                // Safety: the map handle lives at field_offset.
                let map = unsafe { self.base.add(m.field_offset) };
                // Initialize the (uninitialized) map with room for `n` entries.
                // NOTE: a decode error after this point leaks the partial map — the
                // same trivially-droppable limitation as sequences/options.
                unsafe { (m.thunks.init_with_capacity)(m.thunks.ctx, map, n) };
                self.decode_next_map_entry(self.base, m, map, n, n)?
            }
            // r[impl ir.memory] — a self-describing dynamic `Value`: decode one value
            // (self-delimiting) and write it into the field.
            MemOp::Dynamic { field_offset } => {
                let v = read_value(&mut self.reader)?;
                // Safety: `base + field_offset` is uninitialized `Value` storage; we
                // move the decoded value in.
                unsafe { core::ptr::write(self.base.add(*field_offset).cast::<Value>(), v) };
                Control::Continue
            }
            // r[impl ir.memory] — Result<T, E>: read the u32 wire index, decode the
            // matching arm's payload into an engine scratch buffer, then move it into
            // the Result via `init_ok`/`init_err` (the repr(Rust) layout is built by
            // the vtable, mirroring the Option some-arm). An index matching neither
            // arm is a decode error.
            MemOp::Result(rs) => {
                let idx = self.reader.read_u32()?;
                // Safety: the result handle lives at field_offset.
                let result = unsafe { self.base.add(rs.field_offset) };
                if idx == rs.ok_wire_index {
                    self.decode_into_via_init(
                        &rs.ok,
                        rs.ok_size,
                        rs.ok_align,
                        InitTarget {
                            ctx: rs.thunks.ctx,
                            handle: result,
                            init: rs.thunks.init_ok,
                        },
                    )?
                } else if idx == rs.err_wire_index {
                    self.decode_into_via_init(
                        &rs.err,
                        rs.err_size,
                        rs.err_align,
                        InitTarget {
                            ctx: rs.thunks.ctx,
                            handle: result,
                            init: rs.thunks.init_err,
                        },
                    )?
                } else {
                    return Err(CompactError::BadVariantIndex(idx));
                }
            }
            // r[impl descriptors.thunk-binding]
            MemOp::Pointer(p) => self.decode_into_via_init(
                &p.pointee,
                p.pointee_size,
                p.pointee_align,
                InitTarget {
                    ctx: p.thunks.ctx,
                    handle: unsafe { self.base.add(p.field_offset) },
                    init: p.thunks.init,
                },
            )?,
            // r[impl compat.skip-writer-only] — consume a writer-only value's wire
            // bytes; write nothing to memory. The walker lives in `phon-ir` next to
            // `SkipOp`, shared with the JIT so both decode engines skip identically.
            MemOp::SkipWire(s) => {
                phon_ir::ir::skip(&mut self.reader, s)?;
                Control::Continue
            }
            // r[impl compat.reader-only-fields]
            // r[impl compat.defaults-are-reader-side]
            // Write a reader-only field's default in place; read no wire.
            MemOp::Default(d) => {
                // Safety: `base + offset` is uninitialized storage of the reader
                // field's type; the bound thunk initializes it.
                unsafe { (d.default)(d.ctx, self.base.add(d.offset)) };
                Control::Continue
            }
            // r[impl ir.memory] — opaque field: read the `u32`
            // length (bounds-checked), borrow the inner span from the input, and hand
            // it to the adapter. The decoded value may borrow that span (zero-copy),
            // so the caller must keep the input alive as long as it (the contract on
            // `decode_with`). The inner schema is never known here.
            MemOp::Opaque(o) => {
                let len = self.reader.read_len(1)?;
                let span = self.reader.read_slice(len)?;
                // Safety: the opaque field lives at `field_offset`; the decode thunk
                // builds it from the borrowed span. `false` ⇒ the adapter rejected it,
                // leaving `slot` uninitialized.
                let slot = unsafe { self.base.add(o.field_offset) };
                if !unsafe { (o.thunks.decode)(o.thunks.ctx, span.as_ptr(), len, slot) } {
                    return Err(CompactError::Decode(DecodeError::Malformed(
                        "opaque adapter rejected input",
                    )));
                }
                Control::Continue
            }
        })
    }

    fn after_return(
        &mut self,
        continuation: Self::Continuation,
    ) -> Result<Control<'program, SchemaId, MemOp, Self::Continuation>> {
        match continuation {
            DecodeContinuation::RestoreBase { previous_base } => {
                self.base = previous_base;
                Ok(Control::Continue)
            }
            DecodeContinuation::Sequence {
                previous_base,
                element,
                mut buffer,
                next_index,
                count,
                list,
                thunks,
            } => {
                if next_index == count {
                    unsafe {
                        (thunks.from_raw_parts)(thunks.ctx, list, buffer.ptr, count, buffer.cap)
                    };
                    buffer.adopt();
                    self.base = previous_base;
                    Ok(Control::Continue)
                } else {
                    self.base = unsafe { buffer.ptr.add(next_index * buffer.stride) };
                    Ok(Control::CallProgramThen(
                        element,
                        DecodeContinuation::Sequence {
                            previous_base,
                            element,
                            buffer,
                            next_index: next_index + 1,
                            count,
                            list,
                            thunks,
                        },
                    ))
                }
            }
            DecodeContinuation::SetElement {
                previous_base,
                set,
                set_ptr,
                remaining_after_current,
                scratch,
            } => {
                // Safety: scratch holds an initialized element; `insert` moves it
                // into the set and tells us whether it was unique.
                let inserted = unsafe { (set.thunks.insert)(set.thunks.ctx, set_ptr, scratch.ptr) };
                drop(scratch);
                if !inserted {
                    return Err(CompactError::Decode(DecodeError::DuplicateElement));
                }
                self.decode_next_set_element(previous_base, set, set_ptr, remaining_after_current)
            }
            DecodeContinuation::MapKey {
                previous_base,
                map,
                map_ptr,
                total,
                remaining,
                key_scratch,
                value_scratch,
            } => {
                self.base = value_scratch.ptr;
                Ok(Control::CallProgramThen(
                    &map.value,
                    DecodeContinuation::MapValue {
                        previous_base,
                        map,
                        map_ptr,
                        total,
                        remaining,
                        key_scratch,
                        value_scratch,
                    },
                ))
            }
            DecodeContinuation::MapValue {
                previous_base,
                map,
                map_ptr,
                total,
                remaining,
                key_scratch,
                value_scratch,
            } => {
                unsafe {
                    (map.thunks.insert)(
                        map.thunks.ctx,
                        map_ptr,
                        key_scratch.ptr,
                        value_scratch.ptr,
                    );
                }
                drop(key_scratch);
                drop(value_scratch);
                let remaining = remaining - 1;
                if remaining == 0 {
                    if unsafe { (map.thunks.len)(map.thunks.ctx, map_ptr) } != total {
                        return Err(CompactError::Decode(DecodeError::DuplicateKey));
                    }
                    self.base = previous_base;
                    Ok(Control::Continue)
                } else {
                    self.decode_next_map_entry(previous_base, map, map_ptr, total, remaining)
                }
            }
            DecodeContinuation::Init {
                previous_base,
                target,
                scratch,
            } => {
                unsafe { (target.init)(target.ctx, target.handle, scratch.ptr) };
                drop(scratch);
                self.base = previous_base;
                Ok(Control::Continue)
            }
        }
    }
}

impl DecodeInterp<'_> {
    fn decode_next_set_element<'program>(
        &mut self,
        previous_base: *mut u8,
        set: &'program SetOp,
        set_ptr: *mut u8,
        remaining: usize,
    ) -> Result<Control<'program, SchemaId, MemOp, DecodeContinuation<'program>>> {
        if remaining == 0 {
            self.base = previous_base;
            return Ok(Control::Continue);
        }

        let scratch = Scratch::new(set.elem_size, set.elem_align)?;
        self.base = scratch.ptr;
        Ok(Control::CallProgramThen(
            &set.element,
            DecodeContinuation::SetElement {
                previous_base,
                set,
                set_ptr,
                remaining_after_current: remaining - 1,
                scratch,
            },
        ))
    }

    fn decode_next_map_entry<'program>(
        &mut self,
        previous_base: *mut u8,
        map: &'program MapOp,
        map_ptr: *mut u8,
        total: usize,
        remaining: usize,
    ) -> Result<Control<'program, SchemaId, MemOp, DecodeContinuation<'program>>> {
        if remaining == 0 {
            self.base = previous_base;
            return Ok(Control::Continue);
        }

        let key_scratch = Scratch::new(map.key_size, map.key_align)?;
        let value_scratch = Scratch::new(map.value_size, map.value_align)?;
        self.base = key_scratch.ptr;
        Ok(Control::CallProgramThen(
            &map.key,
            DecodeContinuation::MapKey {
                previous_base,
                map,
                map_ptr,
                total,
                remaining,
                key_scratch,
                value_scratch,
            },
        ))
    }

    fn decode_into_via_init<'program>(
        &mut self,
        program: &'program MemProgram,
        size: usize,
        align: usize,
        target: InitTarget,
    ) -> Result<Control<'program, SchemaId, MemOp, DecodeContinuation<'program>>> {
        let scratch = Scratch::new(size, align)?;
        let previous_base = self.base;
        self.base = scratch.ptr;
        Ok(Control::CallProgramThen(
            program,
            DecodeContinuation::Init {
                previous_base,
                target,
                scratch,
            },
        ))
    }
}

/// Allocate an engine-owned scratch buffer of `size`/`align` for a decoded
/// key/value before it is moved into a map. A zero-size element needs no
/// allocation: a dangling-but-aligned pointer suffices (and `free_scratch` then
/// does nothing for it).
fn alloc_scratch(size: usize, align: usize) -> Result<(*mut u8, Option<alloc::Layout>)> {
    if size == 0 {
        Ok((align as *mut u8, None))
    } else {
        let layout = alloc::Layout::from_size_align(size, align).map_err(|_| {
            CompactError::Decode(DecodeError::Malformed("map scratch layout overflow"))
        })?;
        // Safety: size > 0.
        let buf = unsafe { alloc::alloc(layout) };
        if buf.is_null() {
            alloc::handle_alloc_error(layout);
        }
        Ok((buf, Some(layout)))
    }
}

/// Free a scratch buffer from [`alloc_scratch`] WITHOUT dropping its contents
/// (ownership was moved into the map by `insert`). A `None` layout is a zero-size
/// dangling pointer that was never allocated.
fn free_scratch(buf: *mut u8, layout: Option<alloc::Layout>) {
    if let Some(layout) = layout {
        // Safety: `buf` was allocated by `alloc_scratch` with this exact layout.
        unsafe { alloc::dealloc(buf, layout) };
    }
}

struct InitTarget {
    ctx: *const (),
    handle: *mut u8,
    init: unsafe extern "C" fn(ctx: *const (), handle: *mut u8, value: *mut u8),
}

/// Lower `descriptor` and decode `bytes` into the value at `base` in one step.
///
/// # Safety
/// As [`decode_with`].
///
/// # Errors
/// As [`lower`] and [`decode_with`].
pub unsafe fn decode(
    bytes: &[u8],
    descriptor: &Descriptor,
    descriptor_blocks: &HashMap<SchemaId, Descriptor>,
    reg: &Registry,
    base: *mut u8,
) -> Result<()> {
    let lowered = lower_typed(descriptor, descriptor_blocks, reg)?;
    // Safety: forwarded from this function's contract.
    unsafe { decode_with(&lowered, bytes, base) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{MaybeUninit, align_of, offset_of, size_of};
    use facet_value::{VArray, Value};
    use phon_ir::{FieldAccess, Layout, RecordByteOwnership, SeqThunks, SequenceAccess};
    use phon_schema::bytes::{write_i64, write_u64};
    use phon_schema::{Schema, SchemaId, SchemaRef, primitive_id};

    // Hand-written list thunks for `Vec<u32>`. The facet bridge will generate
    // equivalents from the list vtable; here we wire them by hand to exercise the
    // engine's sequence machinery on a real `Vec`. The engine allocates the
    // buffer; `from_raw_parts` adopts it.
    unsafe extern "C" fn vu32_from_raw_parts(
        _ctx: *const (),
        list: *mut u8,
        ptr: *mut u8,
        len: usize,
        cap: usize,
    ) {
        let v = unsafe { Vec::<u32>::from_raw_parts(ptr.cast::<u32>(), len, cap) };
        unsafe { core::ptr::write(list.cast::<Vec<u32>>(), v) };
    }
    unsafe extern "C" fn vu32_len(_ctx: *const (), list: *const u8) -> usize {
        unsafe { (*list.cast::<Vec<u32>>()).len() }
    }
    unsafe extern "C" fn vu32_data(_ctx: *const (), list: *const u8) -> *const u8 {
        unsafe { (*list.cast::<Vec<u32>>()).as_ptr().cast::<u8>() }
    }

    fn vu32_thunks() -> SeqThunks {
        SeqThunks {
            ctx: core::ptr::null(),
            from_raw_parts: vu32_from_raw_parts,
            len: vu32_len,
            data: vu32_data,
        }
    }

    fn vec_u32_descriptor(schema: SchemaId) -> Descriptor {
        Descriptor {
            schema: SchemaRef::concrete(schema),
            layout: Layout {
                size: size_of::<Vec<u32>>(),
                align: align_of::<Vec<u32>>(),
            },
            access: Access::Sequence(SequenceAccess {
                element: Box::new(Descriptor {
                    schema: SchemaRef::concrete(primitive_id(Primitive::U32)),
                    layout: Layout { size: 4, align: 4 },
                    access: Access::Scalar,
                }),
                storage: SequenceStorage::Vtable(vu32_thunks()),
            }),
        }
    }

    #[repr(C)]
    #[derive(Debug, PartialEq)]
    struct NarrowNativeInts {
        count: u32,
        delta: i32,
    }

    #[repr(C)]
    #[derive(Debug, PartialEq)]
    struct PaddedScalars {
        a: u32,
        b: u64,
    }

    fn narrow_native_int_schema(schema: SchemaId) -> Schema {
        Schema {
            id: schema,
            type_params: Vec::new(),
            kind: SchemaKind::Struct {
                name: "NarrowNativeInts".to_string(),
                fields: vec![
                    Field {
                        name: "count".to_string(),
                        schema: SchemaRef::concrete(primitive_id(Primitive::U64)),
                        required: true,
                    },
                    Field {
                        name: "delta".to_string(),
                        schema: SchemaRef::concrete(primitive_id(Primitive::I64)),
                        required: true,
                    },
                ],
            },
        }
    }

    fn narrow_native_int_descriptor(schema: SchemaId) -> Descriptor {
        let layout = Layout {
            size: size_of::<NarrowNativeInts>(),
            align: align_of::<NarrowNativeInts>(),
        };
        let fields = vec![
            FieldAccess {
                offset: offset_of!(NarrowNativeInts, count),
                descriptor: Descriptor {
                    schema: SchemaRef::concrete(primitive_id(Primitive::U64)),
                    layout: Layout {
                        size: size_of::<u32>(),
                        align: align_of::<u32>(),
                    },
                    access: Access::Scalar,
                },
                default: None,
            },
            FieldAccess {
                offset: offset_of!(NarrowNativeInts, delta),
                descriptor: Descriptor {
                    schema: SchemaRef::concrete(primitive_id(Primitive::I64)),
                    layout: Layout {
                        size: size_of::<i32>(),
                        align: align_of::<i32>(),
                    },
                    access: Access::Scalar,
                },
                default: None,
            },
        ];
        let byte_ownership = RecordByteOwnership::from_record_layout(layout, &fields);
        Descriptor {
            schema: SchemaRef::concrete(schema),
            layout,
            access: Access::Record(RecordAccess {
                fields,
                byte_ownership,
                construct: Construct::InPlace,
            }),
        }
    }

    fn padded_scalars_schema(schema: SchemaId) -> Schema {
        Schema {
            id: schema,
            type_params: Vec::new(),
            kind: SchemaKind::Struct {
                name: "PaddedScalars".to_string(),
                fields: vec![
                    Field {
                        name: "a".to_string(),
                        schema: SchemaRef::concrete(primitive_id(Primitive::U32)),
                        required: true,
                    },
                    Field {
                        name: "b".to_string(),
                        schema: SchemaRef::concrete(primitive_id(Primitive::U64)),
                        required: true,
                    },
                ],
            },
        }
    }

    fn padded_scalars_descriptor(schema: SchemaId) -> Descriptor {
        let layout = Layout {
            size: size_of::<PaddedScalars>(),
            align: align_of::<PaddedScalars>(),
        };
        let fields = vec![
            FieldAccess {
                offset: offset_of!(PaddedScalars, a),
                descriptor: Descriptor {
                    schema: SchemaRef::concrete(primitive_id(Primitive::U32)),
                    layout: Layout {
                        size: size_of::<u32>(),
                        align: align_of::<u32>(),
                    },
                    access: Access::Scalar,
                },
                default: None,
            },
            FieldAccess {
                offset: offset_of!(PaddedScalars, b),
                descriptor: Descriptor {
                    schema: SchemaRef::concrete(primitive_id(Primitive::U64)),
                    layout: Layout {
                        size: size_of::<u64>(),
                        align: align_of::<u64>(),
                    },
                    access: Access::Scalar,
                },
                default: None,
            },
        ];
        let byte_ownership = RecordByteOwnership::from_record_layout(layout, &fields);
        Descriptor {
            schema: SchemaRef::concrete(schema),
            layout,
            access: Access::Record(RecordAccess {
                fields,
                byte_ownership,
                construct: Construct::InPlace,
            }),
        }
    }

    #[test]
    fn owned_vec_u32_roundtrips_and_matches_dynamic() {
        // Root type: List<u32> / Vec<u32>.
        let list = Schema {
            id: SchemaId(1),
            type_params: Vec::new(),
            kind: SchemaKind::List {
                element: SchemaRef::concrete(primitive_id(Primitive::U32)),
            },
        };
        let reg = Registry::new([list]);

        let desc = vec_u32_descriptor(SchemaId(1));

        let values = [1u32, 2, 999, 0xDEAD_BEEF];

        // Oracle: the dynamic List<u32> codec over the equivalent array.
        let mut arr = VArray::new();
        for &v in &values {
            arr.push(Value::from(v));
        }
        let dyn_bytes = compact::to_bytes(&Value::from(arr), SchemaId(1), &reg).unwrap();

        // Typed encode of a real Vec<u32> must produce identical bytes.
        let v: Vec<u32> = values.to_vec();
        let no_blocks = HashMap::new();
        let typed_bytes = unsafe {
            encode(
                core::ptr::from_ref(&v).cast::<u8>(),
                &desc,
                &no_blocks,
                &reg,
            )
        }
        .unwrap();
        assert_eq!(typed_bytes, dyn_bytes);

        // Typed decode reconstructs the Vec.
        let mut slot = MaybeUninit::<Vec<u32>>::uninit();
        unsafe {
            decode(
                &typed_bytes,
                &desc,
                &no_blocks,
                &reg,
                slot.as_mut_ptr().cast::<u8>(),
            )
        }
        .unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back, values.to_vec());
    }

    #[test]
    fn padded_record_lowers_to_scalar_run_and_roundtrips() {
        let schema = SchemaId(1);
        let reg = Registry::new([padded_scalars_schema(schema)]);
        let desc = padded_scalars_descriptor(schema);
        let no_blocks = HashMap::new();
        let lowered = lower_typed(&desc, &no_blocks, &reg).unwrap();

        match lowered.program.as_slice() {
            [MemOp::ScalarRun(run)] => {
                assert_eq!(run.segments.len(), 2);
                assert_eq!(
                    (
                        run.segments[0].offset,
                        run.segments[0].size,
                        run.segments[0].align
                    ),
                    (
                        offset_of!(PaddedScalars, a),
                        size_of::<u32>(),
                        align_of::<u32>()
                    )
                );
                assert_eq!(
                    (
                        run.segments[1].offset,
                        run.segments[1].size,
                        run.segments[1].align
                    ),
                    (
                        offset_of!(PaddedScalars, b),
                        size_of::<u64>(),
                        align_of::<u64>()
                    )
                );
            }
            other => panic!("expected one grouped scalar run, got {other:?}"),
        }

        let value = PaddedScalars {
            a: 0x1122_3344,
            b: 0xAABB_CCDD_EEFF_0011,
        };
        let encode_report =
            unsafe { encode_with_stats(&lowered, core::ptr::from_ref(&value).cast::<u8>()) };
        let bytes = encode_report.bytes;
        assert_eq!(encode_report.stats.step_count, 1);
        assert_eq!(encode_report.stats.max_frame_depth, 1);
        let mut expected = value.a.to_le_bytes().to_vec();
        expected.extend_from_slice(&[0; 4]);
        expected.extend_from_slice(&value.b.to_le_bytes());
        assert_eq!(bytes, expected);

        let mut slot = MaybeUninit::<PaddedScalars>::uninit();
        let decode_stats =
            unsafe { decode_with_stats(&lowered, &bytes, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        assert_eq!(decode_stats.step_count, 1);
        assert_eq!(decode_stats.max_frame_depth, 1);
        assert_eq!(unsafe { slot.assume_init() }, value);
    }

    #[test]
    // r[verify type-system.rust-subset]
    // r[verify exec.jit-optional]
    fn native_int_memops_roundtrip_and_reject_out_of_range_values() {
        let schema = SchemaId(1);
        let reg = Registry::new([narrow_native_int_schema(schema)]);
        let desc = narrow_native_int_descriptor(schema);
        let no_blocks = HashMap::new();
        let lowered = lower_typed(&desc, &no_blocks, &reg).unwrap();

        assert_eq!(lowered.program.len(), 2);
        assert!(matches!(
            lowered.program[0],
            MemOp::NativeInt {
                mem_size: 4,
                signed: false,
                ..
            }
        ));
        assert!(matches!(
            lowered.program[1],
            MemOp::NativeInt {
                mem_size: 4,
                signed: true,
                ..
            }
        ));

        let value = NarrowNativeInts {
            count: 0xCAFE_F00D,
            delta: -42,
        };
        let bytes = unsafe { encode_with(&lowered, core::ptr::from_ref(&value).cast::<u8>()) };

        let mut expected = Vec::new();
        write_u64(&mut expected, u64::from(value.count));
        write_i64(&mut expected, i64::from(value.delta));
        assert_eq!(bytes, expected);

        let mut slot = MaybeUninit::<NarrowNativeInts>::uninit();
        unsafe { decode_with(&lowered, &bytes, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        assert_eq!(unsafe { slot.assume_init() }, value);

        let mut unsigned_out_of_range = Vec::new();
        write_u64(&mut unsigned_out_of_range, u64::from(u32::MAX) + 1);
        write_i64(&mut unsigned_out_of_range, 0);
        let mut slot = MaybeUninit::<NarrowNativeInts>::uninit();
        let err = unsafe {
            decode_with(
                &lowered,
                &unsigned_out_of_range,
                slot.as_mut_ptr().cast::<u8>(),
            )
        }
        .unwrap_err();
        assert!(matches!(
            err,
            CompactError::Decode(DecodeError::Malformed(
                "native-sized unsigned integer out of range"
            ))
        ));

        let mut signed_out_of_range = Vec::new();
        write_u64(&mut signed_out_of_range, 0);
        write_i64(&mut signed_out_of_range, i64::from(i32::MIN) - 1);
        let mut slot = MaybeUninit::<NarrowNativeInts>::uninit();
        let err = unsafe {
            decode_with(
                &lowered,
                &signed_out_of_range,
                slot.as_mut_ptr().cast::<u8>(),
            )
        }
        .unwrap_err();
        assert!(matches!(
            err,
            CompactError::Decode(DecodeError::Malformed(
                "native-sized signed integer out of range"
            ))
        ));
    }

    #[test]
    fn decode_compat_rejects_list_set_kind_mismatch() {
        let element = SchemaRef::concrete(primitive_id(Primitive::U32));
        let writer = Schema {
            id: SchemaId(1),
            type_params: Vec::new(),
            kind: SchemaKind::Set {
                element: element.clone(),
            },
        };
        let reader = Schema {
            id: SchemaId(2),
            type_params: Vec::new(),
            kind: SchemaKind::List { element },
        };
        let reg = Registry::new([writer, reader]);
        let desc = vec_u32_descriptor(SchemaId(2));
        let no_blocks = HashMap::new();

        let typed = lower_decode(SchemaId(1), &desc, &no_blocks, &reg);
        assert!(
            matches!(typed, Err(CompactError::Incompatible(_))),
            "typed compat accepted Set writer for List reader: {typed:?}"
        );

        let dynamic = crate::plan::build_plan(SchemaId(1), SchemaId(2), &reg);
        assert!(
            matches!(dynamic, Err(CompactError::Incompatible(_))),
            "dynamic compat unexpectedly accepted Set writer for List reader"
        );
    }
}
