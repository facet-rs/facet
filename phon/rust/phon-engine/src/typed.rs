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

use phon_ir::ir::{BytesOp, MemOp, MemProgram, SeqOp, fuse};
use phon_ir::{Access, Construct, Descriptor, SequenceStorage};
use phon_schema::bytes::{Reader, write_u32};
use phon_schema::{DecodeError, Primitive, SchemaKind};

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
                    utf8: false,
                    thunks: *thunks,
                })));
            } else {
                out.push(MemOp::Sequence(Box::new(SeqOp {
                    field_offset: base,
                    element,
                    stride,
                    elem_align,
                    min_wire: 1,
                    thunks: *thunks,
                })));
            }
            Ok(())
        }
        // r[impl ir.memory] — String/Bytes: a bulk contiguous byte run.
        (Access::Sequence(seq), Resolved::Primitive(p @ (Primitive::String | Primitive::Bytes))) => {
            let SequenceStorage::Vtable(thunks) = &seq.storage else {
                return Err(CompactError::Unsupported(
                    "typed: string/bytes needs vtable thunks",
                ));
            };
            out.push(MemOp::Bytes(Box::new(BytesOp {
                field_offset: base,
                stride: 1,
                elem_align: 1,
                utf8: matches!(p, Primitive::String),
                thunks: *thunks,
            })));
            Ok(())
        }
        _ => Err(CompactError::Unsupported(
            "typed: only fixed scalars, in-place records, owned sequences, and strings so far",
        )),
    }
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
                pad_to(out, b.elem_align);
                let data = unsafe { (b.thunks.data)(b.thunks.ctx, list) };
                let src = unsafe { core::slice::from_raw_parts(data, count * b.stride) };
                out.extend_from_slice(src);
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
                let (buffer, cap) = if count == 0 {
                    // Dangling but aligned; `from_raw_parts` with cap 0 won't touch it.
                    (s.elem_align as *mut u8, 0usize)
                } else {
                    let layout = alloc::Layout::from_size_align(count * s.stride, s.elem_align)
                        .map_err(|_| {
                            CompactError::Decode(DecodeError::Malformed("sequence layout overflow"))
                        })?;
                    // Safety: layout has non-zero size (count > 0).
                    let buf = unsafe { alloc::alloc(layout) };
                    if buf.is_null() {
                        alloc::handle_alloc_error(layout);
                    }
                    (buf, count)
                };
                for i in 0..count {
                    // Safety: element `i` occupies `buffer + i*stride`.
                    if let Err(e) = unsafe { decode_program(&s.element, r, buffer.add(i * s.stride)) }
                    {
                        // Free the buffer on a mid-fill failure (elements are
                        // assumed trivially droppable for now).
                        if cap != 0 {
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
                skip_pad(r, b.elem_align)?;
                let total = count * b.stride;
                let src = r.read_slice(total)?;
                // r[impl validate.text]
                if b.utf8 && core::str::from_utf8(src).is_err() {
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
        }
    }
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
