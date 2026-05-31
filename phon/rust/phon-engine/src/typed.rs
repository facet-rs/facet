//! The typed path: lower a [`Descriptor`] (which carries its schema) into a flat
//! [`MemProgram`], then run it to encode or decode a value living in this
//! process's memory.
//!
//! This is the memory counterpart to the dynamic [`Value`](facet_value::Value)
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
use std::collections::HashMap;

use phon_ir::ir::{
    BorrowOp, BytesOp, DefaultOp, EnumOp, EnumVariantOp, MapOp, MemOp, MemProgram, OpaqueOp,
    OptionOp, ResultOp, SeqOp, SkipOp, fuse,
};
use phon_ir::{
    Access, Construct, Descriptor, EnumAccess, MapStorage, Presence, RecordAccess, ResultAccess,
    SequenceStorage, Tag, VariantAccess,
};
use phon_schema::bytes::{Reader, write_u8, write_u32};
use phon_schema::{
    DecodeError, Field, Primitive, SchemaId, SchemaKind, SchemaRef, Value, Variant, VariantPayload,
    read_value, write_value,
};

use crate::compact::{self, CompactError, Registry, Resolved, alignment, pad_to, skip_pad};

type Result<T> = core::result::Result<T, CompactError>;

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
/// The minimum wire bytes one owned-sequence element occupies, for the
/// length-vs-remaining guard (`r[validate.lengths]`). `0` when the element is
/// zero-sized (an empty / all-ZST struct encodes to nothing, so the count is
/// unbounded by the buffer and a fixed cap applies in `read_len` / the JIT
/// stencil); `1` otherwise. An empty program is vacuously zero-sized.
fn elem_min_wire(element: &MemProgram) -> usize {
    let zero_sized = element
        .iter()
        .all(|op| matches!(op, MemOp::Scalar { size: 0, .. }));
    usize::from(!zero_sized)
}

// r[impl ir.memory]
pub fn lower(descriptor: &Descriptor, reg: &Registry) -> Result<MemProgram> {
    let mut out = Vec::new();
    lower_node(descriptor, reg, 0, &mut out)?;
    // Coalesce contiguous scalar runs into single copies (e.g. a flat struct
    // whose wire and memory layouts match becomes one memcpy).
    Ok(fuse(out))
}

// r[impl ir.inlining]
fn lower_node(d: &Descriptor, reg: &Registry, base: usize, out: &mut MemProgram) -> Result<()> {
    match (&d.access, compact::resolve(reg, &d.schema)?) {
        (Access::Scalar, Resolved::Primitive(p)) => {
            let size = fixed_size(p)
                .ok_or(CompactError::Unsupported("typed: variable-length scalar field"))?;
            out.push(MemOp::Scalar {
                offset: base,
                size,
                align: alignment(p),
            });
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
                return Err(CompactError::Malformed("descriptor/schema field count mismatch"));
            }
            match &ra.construct {
                Construct::InPlace => {}
                Construct::Thunk(_) => {
                    return Err(CompactError::Unsupported("typed: thunk construction"));
                }
            }
            // Splice each field in wire order, folding its memory offset into the
            // base. A field's own descriptor carries its schema and layout.
            for fa in &ra.fields {
                lower_node(&fa.descriptor, reg, base + fa.offset, out)?;
            }
            Ok(())
        }
        // r[impl ir.memory]
        (
            Access::Sequence(seq),
            Resolved::Composite(SchemaKind::List { .. } | SchemaKind::Set { .. }),
        ) => {
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
            let element = fuse(element);
            // Bulk-copy lowering: an element that is a single scalar covering its
            // whole size, with no inter-element wire padding, decodes/encodes as
            // ONE block copy — `Vec<u32>`, `Vec<f64>`, `Vec<u8>`, flat `repr(C)`
            // structs. Anything with structure stays a per-element sequence.
            let bulk = matches!(
                element.as_slice(),
                [MemOp::Scalar { offset: 0, size, align }]
                    if *size == stride && stride % *align == 0
            );
            if bulk {
                out.push(MemOp::Bytes(Box::new(BytesOp {
                    field_offset: base,
                    stride,
                    elem_align,
                    validate: validate_any,
                    thunks: *thunks,
                })));
            } else {
                let min_wire = elem_min_wire(&element);
                out.push(MemOp::Sequence(Box::new(SeqOp {
                    field_offset: base,
                    element,
                    stride,
                    elem_align,
                    min_wire,
                    thunks: *thunks,
                })));
            }
            Ok(())
        }
        // r[impl ir.memory] — String/Bytes: a bulk contiguous byte run.
        (Access::Sequence(seq), Resolved::Primitive(p @ (Primitive::String | Primitive::Bytes))) => {
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
                for f in &va.payload.fields {
                    lower_node(&f.descriptor, reg, base + f.offset, &mut payload)?;
                }
                variants.push(EnumVariantOp {
                    wire_index: va.index,
                    selector: va.selector,
                    payload: fuse(payload),
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
                return Err(CompactError::Unsupported(
                    "typed: map needs vtable thunks",
                ));
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
            out.push(MemOp::Result(Box::new(lower_result(ra, &variants, reg, base)?)));
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

/// Lower a *writer* schema reconciled against a *reader* [`Descriptor`] into a
/// flat [`MemProgram`] of reader-memory ops, in WIRE order. This is the typed
/// (memory-side) analog of `plan::build_plan` + `plan::lower`: it bakes the
/// writer↔reader compatibility decision in once, at lowering, so decode stays as
/// fast as the single-schema path — there is no fast/slow path, only one program.
///
/// The reconciliation rules mirror `plan.rs` exactly (the cross-engine oracle):
/// struct fields match by name (writer-only skipped, reader-only defaulted or, if
/// required, incompatible), enum variants match by name (writer-only → a decode
/// error), and types match without implicit widening (`r[compat.*]`).
///
/// When `writer_root` resolves to the same schema the reader carries, the result
/// is equivalent to [`lower`] (no skips/defaults) — the drift-free identity.
///
/// # Errors
/// [`CompactError::Incompatible`] (or a resolution error) if the writer and reader
/// cannot be reconciled, or [`CompactError::Unsupported`] for a kind not yet
/// carried by the typed path.
// r[impl compat.plan-first]
pub fn lower_decode(
    writer_root: SchemaId,
    reader: &Descriptor,
    reg: &Registry,
) -> Result<MemProgram> {
    let mut out = Vec::new();
    lower_decode_node(&SchemaRef::concrete(writer_root), reader, reg, 0, &mut out)?;
    Ok(fuse(out))
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
            let size = fixed_size(wp)
                .ok_or(CompactError::Unsupported("typed: variable-length scalar field"))?;
            out.push(MemOp::Scalar {
                offset: base,
                size,
                align: alignment(wp),
            });
            Ok(())
        }
        // Struct ⋈ struct: reconcile fields by name, in WIRE order.
        (Access::Record(ra), Resolved::Composite(SchemaKind::Struct { fields: wf, .. })) => {
            lower_decode_struct(&wf, ra, &reader.schema, reg, base, out)
        }
        // Enum ⋈ enum: reconcile variants by name.
        (Access::Enum(ea), Resolved::Composite(SchemaKind::Enum { variants: wv, .. })) => {
            lower_decode_enum(&wv, ea, &reader.schema, reg, base, out)
        }
        // Option ⋈ Option: structural shapes match; reconcile the inner.
        (Access::Option(opt), Resolved::Composite(SchemaKind::Option { element: we })) => {
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
        // List/Set ⋈ List/Set: reconcile the element.
        (
            Access::Sequence(seq),
            Resolved::Composite(SchemaKind::List { element: we } | SchemaKind::Set { element: we }),
        ) => {
            let SequenceStorage::Vtable(thunks) = &seq.storage else {
                return Err(CompactError::Unsupported(
                    "typed: only vtable-backed owned sequences so far",
                ));
            };
            let stride = seq.element.layout.size;
            let elem_align = seq.element.layout.align;
            let mut element = Vec::new();
            lower_decode_node(&we, &seq.element, reg, 0, &mut element)?;
            let element = fuse(element);
            let bulk = matches!(
                element.as_slice(),
                [MemOp::Scalar { offset: 0, size, align }]
                    if *size == stride && stride % *align == 0
            );
            if bulk {
                out.push(MemOp::Bytes(Box::new(BytesOp {
                    field_offset: base,
                    stride,
                    elem_align,
                    validate: validate_any,
                    thunks: *thunks,
                })));
            } else {
                let min_wire = elem_min_wire(&element);
                out.push(MemOp::Sequence(Box::new(SeqOp {
                    field_offset: base,
                    element,
                    stride,
                    elem_align,
                    min_wire,
                    thunks: *thunks,
                })));
            }
            Ok(())
        }
        // String/Bytes ⋈ String/Bytes: a bulk byte run (no inner drift possible).
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
        // Map ⋈ Map: reconcile key and value.
        (Access::Map(ma), Resolved::Composite(SchemaKind::Map { key: wk, value: wv })) => {
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
        // own structure, so there is nothing to reconcile — passthrough.
        (Access::Dynamic, Resolved::Composite(SchemaKind::Dynamic)) => {
            out.push(MemOp::Dynamic { field_offset: base });
            Ok(())
        }
        // Result ⋈ enum: the writer's Result wire is a two-variant enum; match Ok/Err
        // by name and reconcile each arm's payload (writer Ok ⋈ reader Ok, etc.).
        (Access::Result(ra), Resolved::Composite(SchemaKind::Enum { variants: wv, .. })) => {
            out.push(MemOp::Result(Box::new(lower_decode_result(&wv, ra, reg, base)?)));
            Ok(())
        }
        // Opaque ⋈ Bytes: the writer wire is a `Primitive::Bytes` run; the reader
        // carries an opaque adapter. The inner bytes are never reconciled here — the
        // adapter owns the inner type — so this is the single-schema op verbatim.
        (Access::Opaque(thunks), Resolved::Primitive(Primitive::Bytes)) => {
            out.push(MemOp::Opaque(Box::new(OpaqueOp {
                field_offset: base,
                thunks: *thunks,
            })));
            Ok(())
        }
        _ => Err(incompatible("writer and reader schema kinds differ")),
    }
}

/// Reconcile a writer struct's wire fields against the reader's record descriptor.
/// Reader field NAMES come from the reader schema (resolved here), aligned by index
/// with the descriptor's fields (the bridge builds them in the same order).
// r[impl compat.field-matching]
fn lower_decode_struct(
    w_fields: &[Field],
    ra: &RecordAccess,
    reader_schema: &SchemaRef,
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
    let r_named = reader_struct_fields(reader_schema, reg)?;
    if r_named.len() != ra.fields.len() {
        return Err(CompactError::Malformed("descriptor/schema field count mismatch"));
    }
    let reader_by_name: HashMap<&str, usize> = r_named
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.as_str(), i))
        .collect();

    // One step per WRITER field, in wire order: take the matched reader field, or
    // skip the writer-only one.
    let mut matched = vec![false; ra.fields.len()];
    for wf in w_fields {
        if let Some(&ri) = reader_by_name.get(wf.name.as_str()) {
            let fa = &ra.fields[ri];
            lower_decode_node(&wf.schema, &fa.descriptor, reg, base + fa.offset, out)?;
            matched[ri] = true;
        } else {
            out.push(MemOp::SkipWire(Box::new(skip_op(&wf.schema, reg)?)));
        }
    }
    // Reader-only fields: default in place, or — if required — incompatible.
    for (ri, fa) in ra.fields.iter().enumerate() {
        if matched[ri] {
            continue;
        }
        match fa.default {
            Some(d) => out.push(MemOp::Default(Box::new(DefaultOp {
                offset: base + fa.offset,
                ctx: d.ctx,
                default: d.thunk,
            }))),
            None => {
                return Err(incompatible(format!(
                    "required reader field '{}' is absent from the writer",
                    r_named[ri].name
                )));
            }
        }
    }
    Ok(())
}

/// Reconcile a writer enum's variants against the reader's enum descriptor, keyed
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
    // Reader variant by name -> (descriptor variant access, reader schema variant).
    let reader_by_name: HashMap<&str, usize> = r_named
        .iter()
        .enumerate()
        .map(|(i, v)| (v.name.as_str(), i))
        .collect();

    let mut variants = Vec::new();
    let mut writer_only = Vec::new();
    for wv in w_variants {
        let Some(&ri) = reader_by_name.get(wv.name.as_str()) else {
            // A writer variant the reader lacks: receiving it is a decode error.
            writer_only.push(wv.index);
            continue;
        };
        let va = &ea.variants[ri];
        let payload = lower_decode_payload(&wv.payload, va, &r_named[ri].payload, reg, base)?;
        variants.push(EnumVariantOp {
            wire_index: wv.index,
            selector: va.selector,
            payload,
        });
    }
    out.push(MemOp::Enum(Box::new(EnumOp {
        tag_offset: base + *offset,
        tag_width: *width,
        variants,
        writer_only,
    })));
    Ok(())
}

/// Reconcile one matched enum variant's payload (writer payload ⋈ reader payload).
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
            let fa = va
                .payload
                .fields
                .first()
                .ok_or(CompactError::Malformed("newtype variant has no payload field"))?;
            lower_decode_node(wr, &fa.descriptor, reg, base + fa.offset, &mut payload)?;
        }
        (VariantPayload::Tuple(wrs), VariantPayload::Tuple(rrs)) => {
            if wrs.len() != rrs.len() || wrs.len() != va.payload.fields.len() {
                return Err(incompatible("variant tuple arity differs"));
            }
            // Tuple fields are positional (no names): reconcile element-wise.
            for (wr, fa) in wrs.iter().zip(&va.payload.fields) {
                lower_decode_node(wr, &fa.descriptor, reg, base + fa.offset, &mut payload)?;
            }
        }
        (VariantPayload::Struct(wfs), VariantPayload::Struct(rfs)) => {
            // A struct-shaped payload reconciles by field name, like a top-level
            // struct, but at the variant's base-relative offsets. Build a synthetic
            // reader-schema ref is unnecessary: reconcile against the variant's own
            // record access and the reader schema payload field list.
            lower_decode_variant_struct(wfs, &va.payload, rfs, reg, base, &mut payload)?;
        }
        _ => return Err(incompatible("variant payload shapes differ")),
    }
    Ok(fuse(payload))
}

/// Reconcile a writer struct-variant payload against the reader's variant record
/// access (matching by name, defaulting reader-only fields), at base-relative
/// offsets. Mirrors [`lower_decode_struct`] but the reader names come straight from
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
    let reader_by_name: HashMap<&str, usize> = r_fields
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.as_str(), i))
        .collect();
    let mut matched = vec![false; ra.fields.len()];
    for wf in w_fields {
        if let Some(&ri) = reader_by_name.get(wf.name.as_str()) {
            let fa = &ra.fields[ri];
            lower_decode_node(&wf.schema, &fa.descriptor, reg, base + fa.offset, out)?;
            matched[ri] = true;
        } else {
            out.push(MemOp::SkipWire(Box::new(skip_op(&wf.schema, reg)?)));
        }
    }
    for (ri, fa) in ra.fields.iter().enumerate() {
        if matched[ri] {
            continue;
        }
        match fa.default {
            Some(d) => out.push(MemOp::Default(Box::new(DefaultOp {
                offset: base + fa.offset,
                ctx: d.ctx,
                default: d.thunk,
            }))),
            None => {
                return Err(incompatible(format!(
                    "required reader variant field '{}' is absent from the writer",
                    r_fields[ri].name
                )));
            }
        }
    }
    Ok(())
}

/// The reader struct's fields (for names), resolved from a reader schema reference.
fn reader_struct_fields(r: &SchemaRef, reg: &Registry) -> Result<Vec<Field>> {
    match compact::resolve(reg, r)? {
        Resolved::Composite(SchemaKind::Struct { fields, .. }) => Ok(fields),
        Resolved::Composite(SchemaKind::Tuple { elements }) => {
            // A tuple reader: positional, synthesize index names so the matcher
            // lines up with the descriptor's positional fields.
            Ok(elements
                .into_iter()
                .enumerate()
                .map(|(i, schema)| Field {
                    name: i.to_string(),
                    schema,
                    required: true,
                })
                .collect())
        }
        _ => Err(CompactError::TypeMismatch {
            expected: "struct or tuple reader schema for a record descriptor",
        }),
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

fn incompatible(why: impl Into<String>) -> CompactError {
    CompactError::Incompatible(why.into())
}

/// The wire index of the schema enum variant named `name` (`Ok`/`Err` for a
/// `Result`), for lowering a [`ResultOp`].
fn variant_index_by_name(variants: &[Variant], name: &str) -> Result<u32> {
    variants
        .iter()
        .find(|v| v.name == name)
        .map(|v| v.index)
        .ok_or(CompactError::Malformed("Result schema missing Ok or Err variant"))
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
/// name and reconcile each arm's payload against the reader's Ok/Err descriptor.
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

/// Reconcile one `Result` arm: the writer payload is a newtype (`Ok(T)`/`Err(E)`),
/// reconciled against the reader arm's descriptor at offset 0 (the arm value start).
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
            SchemaKind::Option { element } => {
                Ok(SkipOp::Option(Box::new(skip_op(&element, reg)?)))
            }
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
                Err(CompactError::Malformed("skip: primitive in composite position"))
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

/// A mask of the low `width` bytes (`width >= 8` → all ones).
fn width_mask(width: usize) -> u64 {
    if width >= 8 {
        u64::MAX
    } else {
        (1u64 << (width * 8)) - 1
    }
}

/// [`ByteValidator`] for `String` byte runs: the bytes must be valid UTF-8
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

/// [`ByteValidator`] for byte runs with no content constraint — `Vec<u8>` and
/// bulk `Vec<scalar>` runs accept any bytes.
///
/// # Safety
/// Reads nothing; the signature matches [`ByteValidator`].
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
pub unsafe fn encode_with(program: &MemProgram, base: *const u8) -> Vec<u8> {
    let mut out = Vec::new();
    // Safety: forwarded from this function's contract.
    unsafe { encode_program(program, base, &mut out) };
    out
}

unsafe fn encode_program(program: &MemProgram, base: *const u8, out: &mut Vec<u8>) {
    for op in program {
        match op {
            MemOp::Scalar { offset, size, align } => {
                pad_to(out, *align);
                // Safety: the value is valid for reads over this field's bytes.
                let src = unsafe { core::slice::from_raw_parts(base.add(*offset), *size) };
                out.extend_from_slice(src);
            }
            MemOp::Sequence(s) => {
                // Safety: the sequence handle lives at `field_offset`.
                let list = unsafe { base.add(s.field_offset) };
                let n = unsafe { (s.thunks.len)(s.thunks.ctx, list) };
                write_u32(out, n as u32);
                let data = unsafe { (s.thunks.data)(s.thunks.ctx, list) };
                for i in 0..n {
                    // Safety: element `i` lives at `data + i*stride`.
                    unsafe { encode_program(&s.element, data.add(i * s.stride), out) };
                }
            }
            MemOp::Bytes(b) => {
                // Safety: the handle lives at field_offset; one bulk read of its
                // contiguous `count * stride` bytes.
                let list = unsafe { base.add(b.field_offset) };
                let count = unsafe { (b.thunks.len)(b.thunks.ctx, list) };
                write_u32(out, count as u32);
                // Alignment pads BEFORE an element's bytes; an empty run has no
                // elements, so it writes no padding (`r[compact.alignment]`).
                if count > 0 {
                    pad_to(out, b.elem_align);
                }
                let data = unsafe { (b.thunks.data)(b.thunks.ctx, list) };
                let src = unsafe { core::slice::from_raw_parts(data, count * b.stride) };
                out.extend_from_slice(src);
            }
            // Encode of a borrowed leaf is byte-identical to the owned bulk run: the
            // `&str`/`&[u8]` reads its length and contiguous bytes through the borrow
            // thunks and writes them as a `u32` count + `count * stride` bytes.
            MemOp::Borrow(b) => {
                // Safety: the borrowed handle (fat pointer) lives at field_offset.
                let field = unsafe { base.add(b.field_offset) };
                let count = unsafe { (b.thunks.len)(b.thunks.ctx, field) };
                write_u32(out, count as u32);
                // Alignment pads BEFORE an element's bytes; an empty run has no
                // elements, so it writes no padding (`r[compact.alignment]`).
                if count > 0 {
                    pad_to(out, b.elem_align);
                }
                let data = unsafe { (b.thunks.data)(b.thunks.ctx, field) };
                let src = unsafe { core::slice::from_raw_parts(data, count * b.stride) };
                out.extend_from_slice(src);
            }
            MemOp::Option(o) => {
                // Safety: the option handle lives at field_offset.
                let option = unsafe { base.add(o.field_offset) };
                if unsafe { (o.thunks.is_some)(o.thunks.ctx, option) } {
                    write_u8(out, 1);
                    // Safety: present, so `get_value` returns a valid inner pointer.
                    let inner = unsafe { (o.thunks.get_value)(o.thunks.ctx, option) };
                    unsafe { encode_program(&o.some, inner, out) };
                } else {
                    write_u8(out, 0);
                }
            }
            MemOp::Enum(e) => {
                // Read the in-memory discriminant to pick the active variant.
                // Safety: the discriminant lives at base + tag_offset, tag_width wide.
                let disc = unsafe { read_uint(base.add(e.tag_offset), e.tag_width) };
                let mask = width_mask(e.tag_width);
                let variant = e
                    .variants
                    .iter()
                    .find(|v| (v.selector & mask) == (disc & mask))
                    .expect("enum discriminant matches no modelled variant (invalid value)");
                write_u32(out, variant.wire_index);
                // The payload fields live at base-relative offsets (same base).
                unsafe { encode_program(&variant.payload, base, out) };
            }
            MemOp::Map(m) => {
                // Safety: the map handle lives at field_offset.
                let map = unsafe { base.add(m.field_offset) };
                let n = unsafe { (m.thunks.len)(m.thunks.ctx, map) };
                write_u32(out, n as u32);
                // Drive a stateful iterator over the entries, encoding each
                // (key, value) pair in turn.
                let it = unsafe { (m.thunks.iter_init)(m.thunks.ctx, map) };
                loop {
                    let mut k: *const u8 = core::ptr::null();
                    let mut v: *const u8 = core::ptr::null();
                    // Safety: `it` is a live iterator; the out-params are valid.
                    if !unsafe { (m.thunks.iter_next)(m.thunks.ctx, it, &mut k, &mut v) } {
                        break;
                    }
                    // Safety: `k`/`v` borrow the current entry's key/value.
                    unsafe { encode_program(&m.key, k, out) };
                    unsafe { encode_program(&m.value, v, out) };
                }
                // Safety: `it` was built by `iter_init` and is freed exactly once.
                unsafe { (m.thunks.iter_dealloc)(m.thunks.ctx, it) };
            }
            // r[impl ir.memory] — a self-describing dynamic `Value`: write it through
            // the self-describing codec (self-delimiting; no length prefix).
            MemOp::Dynamic { field_offset } => {
                // Safety: the field at `field_offset` is an initialized `Value`.
                let v = unsafe { &*base.add(*field_offset).cast::<Value>() };
                write_value(out, v).expect("dynamic value is encodable by the self-describing codec");
            }
            // r[impl ir.memory] — Result<T, E>: read which arm is active via the
            // vtable, write its wire index, then encode that arm's payload at the
            // pointer the getter returns (the repr(Rust) layout is never assumed).
            MemOp::Result(rs) => {
                // Safety: the result handle lives at field_offset.
                let result = unsafe { base.add(rs.field_offset) };
                if unsafe { (rs.thunks.is_ok)(rs.thunks.ctx, result) } {
                    write_u32(out, rs.ok_wire_index);
                    // Safety: Ok, so `get_ok` returns a valid inner pointer.
                    let ok = unsafe { (rs.thunks.get_ok)(rs.thunks.ctx, result) };
                    unsafe { encode_program(&rs.ok, ok, out) };
                } else {
                    write_u32(out, rs.err_wire_index);
                    // Safety: Err, so `get_err` returns a valid inner pointer.
                    let err = unsafe { (rs.thunks.get_err)(rs.thunks.ctx, result) };
                    unsafe { encode_program(&rs.err, err, out) };
                }
            }
            // r[impl zerocopy.framing.value.opaque] — opaque field: reserve a `u32`
            // length (align 1 — wire-identical to a `Primitive::Bytes` run, so no
            // pre-pad), append the inner bytes via the thunk, then backpatch the
            // length. The backpatch is what fixed-width (non-varint) framing buys.
            MemOp::Opaque(o) => {
                // Safety: the opaque field lives at `field_offset`.
                let field = unsafe { base.add(o.field_offset) };
                let len_pos = out.len();
                write_u32(out, 0); // length placeholder, backpatched below
                let start = out.len();
                // Safety: `field` points at the opaque field; the thunk appends the
                // inner value's encoded bytes to `out`.
                unsafe { (o.thunks.encode)(o.thunks.ctx, field, core::ptr::from_mut(out)) };
                let inner_len = (out.len() - start) as u32;
                out[len_pos..len_pos + 4].copy_from_slice(&inner_len.to_le_bytes());
            }
            // Compat-only decode ops never appear in an encode program (encode is
            // single-schema: `lower`, not `lower_decode`).
            MemOp::SkipWire(_) | MemOp::Default(_) => {
                unreachable!("typed encode never emits compat skip/default ops")
            }
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
pub unsafe fn encode(base: *const u8, descriptor: &Descriptor, reg: &Registry) -> Result<Vec<u8>> {
    let program = lower(descriptor, reg)?;
    // Safety: forwarded from this function's contract.
    Ok(unsafe { encode_with(&program, base) })
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
pub unsafe fn decode_with(program: &MemProgram, bytes: &[u8], base: *mut u8) -> Result<()> {
    let mut r = Reader::new(bytes);
    // Safety: forwarded from this function's contract.
    unsafe { decode_program(program, &mut r, base)? };
    if r.remaining() != 0 {
        return Err(CompactError::Decode(DecodeError::TrailingBytes(r.remaining())));
    }
    Ok(())
}

unsafe fn decode_program(program: &MemProgram, r: &mut Reader, base: *mut u8) -> Result<()> {
    for op in program {
        match op {
            MemOp::Scalar { offset, size, align } => {
                skip_pad(r, *align)?;
                let src = r.read_slice(*size)?;
                // Safety: `base` is valid for writes over this field's bytes, and
                // the wire bytes equal the in-memory bytes for a fixed scalar.
                unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), base.add(*offset), *size) };
            }
            MemOp::Sequence(s) => {
                let count = r.read_len(s.min_wire)?;
                // Engine owns the element buffer: allocate it, fill it directly,
                // then hand it to the sequence with `from_raw_parts`.
                // A zero total byte size — an empty sequence, OR any number of
                // zero-sized elements (`stride == 0`) — must not reach the
                // allocator: a zero-size `Layout` is UB to allocate. Use a
                // dangling-but-aligned pointer, exactly as `Vec` does for ZSTs;
                // `from_raw_parts` then adopts `count` elements over no bytes
                // (`size_of::<T>() * cap == 0` matches the empty allocation).
                let (buffer, cap) = if count == 0 || s.stride == 0 {
                    (s.elem_align as *mut u8, count)
                } else {
                    let layout = alloc::Layout::from_size_align(count * s.stride, s.elem_align)
                        .map_err(|_| {
                            CompactError::Decode(DecodeError::Malformed("sequence layout overflow"))
                        })?;
                    // Safety: layout has non-zero size (count > 0 and stride > 0).
                    let buf = unsafe { alloc::alloc(layout) };
                    if buf.is_null() {
                        alloc::handle_alloc_error(layout);
                    }
                    (buf, count)
                };
                for i in 0..count {
                    // Safety: element `i` occupies `buffer + i*stride`. For a ZST
                    // (`stride == 0`) every element shares the dangling pointer and
                    // `.add(0)` is sound (provenance is only required for non-zero
                    // offsets); the element program touches no buffer bytes.
                    if let Err(e) = unsafe { decode_program(&s.element, r, buffer.add(i * s.stride)) }
                    {
                        // Free the buffer on a mid-fill failure (elements are
                        // assumed trivially droppable for now). Only a real,
                        // non-zero-size allocation needs freeing.
                        if cap != 0 && s.stride != 0 {
                            let layout =
                                alloc::Layout::from_size_align(cap * s.stride, s.elem_align).unwrap();
                            unsafe { alloc::dealloc(buffer, layout) };
                        }
                        return Err(e);
                    }
                }
                // Safety: the handle lives at `field_offset`; the buffer holds
                // `count` initialized elements allocated with the element layout.
                let list = unsafe { base.add(s.field_offset) };
                unsafe { (s.thunks.from_raw_parts)(s.thunks.ctx, list, buffer, count, cap) };
            }
            MemOp::Bytes(b) => {
                let count = r.read_len(b.stride.max(1))?;
                // Symmetric with encode: only an empty run skips no padding.
                if count > 0 {
                    skip_pad(r, b.elem_align)?;
                }
                let total = count * b.stride;
                let src = r.read_slice(total)?;
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
                    let layout = alloc::Layout::from_size_align(total, b.elem_align)
                        .map_err(|_| {
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
                let list = unsafe { base.add(b.field_offset) };
                unsafe { (b.thunks.from_raw_parts)(b.thunks.ctx, list, buffer, count, cap) };
            }
            // r[impl ir.memory] — BORROWED, zero-copy `&str`/`&[u8]`: same wire as
            // `Bytes`, but write a fat pointer INTO the input `bytes` — NO alloc, NO
            // copy. The written `&str`/`&[u8]` borrows the reader's input buffer, so
            // the caller must keep `bytes` alive as long as the decoded value (the
            // standard zero-copy contract, documented on `decode_with`'s `Safety`).
            MemOp::Borrow(b) => {
                let count = r.read_len(b.stride.max(1))?;
                // Symmetric with encode: only an empty run skips no padding.
                if count > 0 {
                    skip_pad(r, b.elem_align)?;
                }
                let total = count * b.stride;
                // `src` is a slice INTO the input `bytes` (no copy): the borrowed
                // value will point at exactly these bytes.
                let src = r.read_slice(total)?;
                // Safety: the borrowed handle lives at field_offset; the construct
                // thunk builds the `&str`/`&[u8]` fat pointer there, pointing into the
                // input. Returns false on invalid content (e.g. non-UTF-8 `&str`).
                let field = unsafe { base.add(b.field_offset) };
                if !unsafe { (b.thunks.set_borrowed)(b.thunks.ctx, field, src.as_ptr(), count) } {
                    return Err(CompactError::Decode(DecodeError::InvalidUtf8));
                }
            }
            MemOp::Option(o) => {
                // Safety: the option handle lives at field_offset.
                let option = unsafe { base.add(o.field_offset) };
                match r.read_u8()? {
                    0 => unsafe { (o.thunks.init_none)(o.thunks.ctx, option) },
                    1 => {
                        // Decode the inner into an engine-owned scratch buffer, then
                        // move it into the Option (init_some does a ptr::read) and
                        // free the scratch WITHOUT dropping (ownership transferred).
                        let (scratch, layout) = if o.inner_size == 0 {
                            (o.inner_align as *mut u8, None)
                        } else {
                            let layout =
                                alloc::Layout::from_size_align(o.inner_size, o.inner_align)
                                    .map_err(|_| {
                                        CompactError::Decode(DecodeError::Malformed(
                                            "option inner layout overflow",
                                        ))
                                    })?;
                            // Safety: inner_size > 0.
                            let buf = unsafe { alloc::alloc(layout) };
                            if buf.is_null() {
                                alloc::handle_alloc_error(layout);
                            }
                            (buf, Some(layout))
                        };
                        // Safety: scratch is inner_size bytes at inner_align.
                        if let Err(e) = unsafe { decode_program(&o.some, r, scratch) } {
                            if let Some(layout) = layout {
                                unsafe { alloc::dealloc(scratch, layout) };
                            }
                            return Err(e);
                        }
                        // Safety: scratch holds the initialized inner; init_some moves
                        // it into the option.
                        unsafe { (o.thunks.init_some)(o.thunks.ctx, option, scratch) };
                        if let Some(layout) = layout {
                            unsafe { alloc::dealloc(scratch, layout) };
                        }
                    }
                    b => return Err(CompactError::Decode(DecodeError::InvalidBool(b))),
                }
            }
            MemOp::Enum(e) => {
                let wire_index = r.read_u32()?;
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
                unsafe { write_uint(base.add(e.tag_offset), e.tag_width, variant.selector) };
                // Safety: payload fields write within the enum's storage at base.
                unsafe { decode_program(&variant.payload, r, base)? };
            }
            MemOp::Map(m) => {
                let n = r.read_len(1)?;
                // Safety: the map handle lives at field_offset.
                let map = unsafe { base.add(m.field_offset) };
                // Initialize the (uninitialized) map with room for `n` entries.
                // NOTE: a decode error after this point leaks the partial map — the
                // same trivially-droppable limitation as sequences/options.
                unsafe { (m.thunks.init_with_capacity)(m.thunks.ctx, map, n) };
                for _ in 0..n {
                    // Engine-owned scratch for the key and value: decode each in
                    // place, then `insert` moves both out (ptr::read), so we free the
                    // scratch WITHOUT dropping. A zero-size element needs no alloc — a
                    // dangling-but-aligned pointer suffices.
                    let (key_scratch, key_layout) = alloc_scratch(m.key_size, m.key_align)?;
                    let (value_scratch, value_layout) =
                        match alloc_scratch(m.value_size, m.value_align) {
                            Ok(s) => s,
                            Err(e) => {
                                free_scratch(key_scratch, key_layout);
                                return Err(e);
                            }
                        };
                    // Safety: key_scratch is key_size bytes at key_align.
                    if let Err(e) = unsafe { decode_program(&m.key, r, key_scratch) } {
                        free_scratch(key_scratch, key_layout);
                        free_scratch(value_scratch, value_layout);
                        return Err(e);
                    }
                    // Safety: value_scratch is value_size bytes at value_align.
                    if let Err(e) = unsafe { decode_program(&m.value, r, value_scratch) } {
                        free_scratch(key_scratch, key_layout);
                        free_scratch(value_scratch, value_layout);
                        return Err(e);
                    }
                    // Safety: both scratch buffers hold initialized values; `insert`
                    // moves them into the map.
                    unsafe {
                        (m.thunks.insert)(m.thunks.ctx, map, key_scratch, value_scratch);
                    }
                    // The key and value were moved into the map; free without dropping.
                    free_scratch(key_scratch, key_layout);
                    free_scratch(value_scratch, value_layout);
                }
                // A repeated key collapses two entries into one — reject it, matching
                // the dynamic codec's duplicate-key rejection (the oracle).
                if unsafe { (m.thunks.len)(m.thunks.ctx, map) } != n {
                    return Err(CompactError::Decode(DecodeError::DuplicateKey));
                }
            }
            // r[impl ir.memory] — a self-describing dynamic `Value`: decode one value
            // (self-delimiting) and write it into the field.
            MemOp::Dynamic { field_offset } => {
                let v = read_value(r)?;
                // Safety: `base + field_offset` is uninitialized `Value` storage; we
                // move the decoded value in.
                unsafe { core::ptr::write(base.add(*field_offset).cast::<Value>(), v) };
            }
            // r[impl ir.memory] — Result<T, E>: read the u32 wire index, decode the
            // matching arm's payload into an engine scratch buffer, then move it into
            // the Result via `init_ok`/`init_err` (the repr(Rust) layout is built by
            // the vtable, mirroring the Option some-arm). An index matching neither
            // arm is a decode error.
            MemOp::Result(rs) => {
                let idx = r.read_u32()?;
                // Safety: the result handle lives at field_offset.
                let result = unsafe { base.add(rs.field_offset) };
                if idx == rs.ok_wire_index {
                    // Safety: `result` is uninitialized Result storage; `init_ok`
                    // moves the decoded Ok payload in.
                    unsafe {
                        decode_into_via_init(
                            &rs.ok,
                            rs.ok_size,
                            rs.ok_align,
                            r,
                            rs.thunks.ctx,
                            result,
                            rs.thunks.init_ok,
                        )?
                    };
                } else if idx == rs.err_wire_index {
                    // Safety: as above, `init_err` moves the decoded Err payload in.
                    unsafe {
                        decode_into_via_init(
                            &rs.err,
                            rs.err_size,
                            rs.err_align,
                            r,
                            rs.thunks.ctx,
                            result,
                            rs.thunks.init_err,
                        )?
                    };
                } else {
                    return Err(CompactError::BadVariantIndex(idx));
                }
            }
            // r[impl compat.skip-writer-only] — consume a writer-only value's wire
            // bytes; write nothing to memory. The walker lives in `phon-ir` next to
            // `SkipOp`, shared with the JIT so both decode engines skip identically.
            MemOp::SkipWire(s) => phon_ir::ir::skip(r, s)?,
            // r[impl compat.reader-only-fields] — write a reader-only field's
            // default in place; read no wire.
            MemOp::Default(d) => {
                // Safety: `base + offset` is uninitialized storage of the reader
                // field's type; the bound thunk initializes it.
                unsafe { (d.default)(d.ctx, base.add(d.offset)) };
            }
            // r[impl zerocopy.framing.value.opaque] — opaque field: read the `u32`
            // length (bounds-checked), borrow the inner span from the input, and hand
            // it to the adapter. The decoded value may borrow that span (zero-copy),
            // so the caller must keep the input alive as long as it (the contract on
            // `decode_with`). The inner schema is never known here.
            MemOp::Opaque(o) => {
                let len = r.read_len(1)?;
                let span = r.read_slice(len)?;
                // Safety: the opaque field lives at `field_offset`; the decode thunk
                // builds it from the borrowed span. `false` ⇒ the adapter rejected it,
                // leaving `slot` uninitialized.
                let slot = unsafe { base.add(o.field_offset) };
                if !unsafe { (o.thunks.decode)(o.thunks.ctx, span.as_ptr(), len, slot) } {
                    return Err(CompactError::Decode(DecodeError::Malformed(
                        "opaque adapter rejected input",
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Allocate an engine-owned scratch buffer of `size`/`align` for a decoded
/// key/value before it is moved into a map. A zero-size element needs no
/// allocation: a dangling-but-aligned pointer suffices (and `free_scratch` then
/// does nothing for it).
fn alloc_scratch(size: usize, align: usize) -> Result<(*mut u8, Option<alloc::Layout>)> {
    if size == 0 {
        Ok((align as *mut u8, None))
    } else {
        let layout = alloc::Layout::from_size_align(size, align)
            .map_err(|_| CompactError::Decode(DecodeError::Malformed("map scratch layout overflow")))?;
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

/// Decode one `program`'s value of `size`/`align` into an engine-owned scratch
/// buffer, then move it into `handle` via the `init` thunk (which `ptr::read`s the
/// scratch), freeing the scratch WITHOUT dropping. The construction half of a
/// [`MemOp::Result`] arm (and the same shape as the Option some-arm); `init` is
/// `init_ok` or `init_err`.
///
/// # Safety
/// `handle` must be uninitialized storage for the containing type; `program` must
/// match the arm payload of `size`/`align`; `ctx`/`init` are the bound result thunks.
unsafe fn decode_into_via_init(
    program: &MemProgram,
    size: usize,
    align: usize,
    r: &mut Reader,
    ctx: *const (),
    handle: *mut u8,
    init: unsafe extern "C" fn(ctx: *const (), handle: *mut u8, value: *mut u8),
) -> Result<()> {
    let (scratch, layout) = alloc_scratch(size, align)?;
    // Safety: scratch is `size` bytes at `align`.
    if let Err(e) = unsafe { decode_program(program, r, scratch) } {
        free_scratch(scratch, layout);
        return Err(e);
    }
    // Safety: scratch holds the initialized arm payload; `init` moves it into `handle`.
    unsafe { init(ctx, handle, scratch) };
    free_scratch(scratch, layout);
    Ok(())
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
    reg: &Registry,
    base: *mut u8,
) -> Result<()> {
    let program = lower(descriptor, reg)?;
    // Safety: forwarded from this function's contract.
    unsafe { decode_with(&program, bytes, base) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{MaybeUninit, align_of, size_of};
    use facet_value::{VArray, Value};
    use phon_ir::{Layout, SeqThunks, SequenceAccess};
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

        let desc = Descriptor {
            schema: SchemaRef::concrete(SchemaId(1)),
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
        };

        let values = [1u32, 2, 999, 0xDEAD_BEEF];

        // Oracle: the dynamic List<u32> codec over the equivalent array.
        let mut arr = VArray::new();
        for &v in &values {
            arr.push(Value::from(v));
        }
        let dyn_bytes = compact::to_bytes(&Value::from(arr), SchemaId(1), &reg).unwrap();

        // Typed encode of a real Vec<u32> must produce identical bytes.
        let v: Vec<u32> = values.to_vec();
        let typed_bytes =
            unsafe { encode(core::ptr::from_ref(&v).cast::<u8>(), &desc, &reg) }.unwrap();
        assert_eq!(typed_bytes, dyn_bytes);

        // Typed decode reconstructs the Vec.
        let mut slot = MaybeUninit::<Vec<u32>>::uninit();
        unsafe { decode(&typed_bytes, &desc, &reg, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back, values.to_vec());
    }
}
