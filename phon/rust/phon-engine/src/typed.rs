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

use phon_ir::ir::{BytesOp, EnumOp, EnumVariantOp, MapOp, MemOp, MemProgram, OptionOp, SeqOp, fuse};
use phon_ir::{Access, Construct, Descriptor, MapStorage, Presence, SequenceStorage, Tag};
use phon_schema::bytes::{Reader, write_u8, write_u32};
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
                    validate: validate_any,
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
                validate: if matches!(p, Primitive::String) {
                    validate_utf8
                } else {
                    validate_any
                },
                thunks: *thunks,
            })));
            Ok(())
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
        _ => Err(CompactError::Unsupported(
            "typed: only fixed scalars, in-place records, owned sequences, strings, options, and #[repr(int)] enums so far",
        )),
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
                pad_to(out, b.elem_align);
                let data = unsafe { (b.thunks.data)(b.thunks.ctx, list) };
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
                let variant = e
                    .variants
                    .iter()
                    .find(|v| v.wire_index == wire_index)
                    .ok_or(CompactError::BadVariantIndex(wire_index))?;
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
