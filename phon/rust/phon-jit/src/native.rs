//! Native execution substrate for Apple Silicon: allocate `MAP_JIT` memory, copy
//! compiler-emitted machine code in, flip write-xor-execute, and run it.
//!
//! This is the bottom of the copy-and-patch JIT: the part that actually runs
//! machine code. The bytes it runs are produced by rustc at build time and
//! extracted from the object file (see `build.rs`) — nothing here encodes an
//! instruction. The relocation patching that specializes a stencil sits on top
//! of this and is added next.
//!
//! macOS on Apple Silicon enforces write-xor-execute on JIT pages: a `MAP_JIT`
//! region is either writable or executable per-thread, toggled with
//! `pthread_jit_write_protect_np`, and the instruction cache must be flushed
//! after writing (`sys_icache_invalidate`).
//!
//! Spec: `r[ir.stencils]`, `r[exec.jit-optional]`.

use core::sync::atomic::{AtomicUsize, Ordering};

use phon_ir::ir::{MemOp, MemProgram, SkipOp};
use phon_schema::DecodeError;
use phon_schema::bytes::Reader;

// The backend-agnostic copy-and-patch substrate lives in `copypatch`: executable
// memory (MAP_JIT + W^X + i-cache) and AArch64 relocation patching. This crate
// keeps only the phon-specific parts — the stencils, the per-op state, and the
// IR -> stencil-chain compilation.
use copypatch::{ExecBuf, patch_branch26};

use crate::stencils::{
    BORROW, BORROW_CONT, BYTES, BYTES_CONT, BYTES_ENC, BYTES_ENC_CONT, DEFAULT, DEFAULT_CONT, DONE,
    DONE_ENC, ENUM, ENUM_CONT, ENUM_ENC, ENUM_ENC_CONT, MAP, MAP_CONT, MAP_ENC, MAP_ENC_CONT,
    OPTION, OPTION_CONT, OPTION_ENC, OPTION_ENC_CONT, SCALAR, SCALAR_CONT, SCALAR_ENC,
    SCALAR_ENC_CONT, SEQUENCE, SEQUENCE_CONT, SEQUENCE_ENC, SEQUENCE_ENC_CONT, SKIPWIRE,
    SKIPWIRE_CONT,
};

/// Load the smoke stencil into JIT memory and run it: `x * 3 + 1`, computed by
/// rustc-emitted machine code executing from a `MAP_JIT` page. A self-test that
/// the native execution path works on this machine; the real stencils plug into
/// the same substrate.
#[must_use]
pub fn smoke(x: i64) -> i64 {
    let buf = ExecBuf::new(crate::stencils::SMOKE);
    let f: extern "C" fn(i64) -> i64 = unsafe { core::mem::transmute(buf.as_ptr()) };
    f(x)
}

// ============================================================================
// The scalar decode JIT
// ============================================================================

/// The threaded state, matching `Ctx` in `stencils/stencils.rs` byte for byte.
#[repr(C)]
struct Ctx {
    wire: *const u8,
    wire_start: *const u8,
    wire_end: *const u8,
    base: *mut u8,
    prog: *const u64,
    status: u64,
    aux: u64,
    alloc: unsafe extern "C" fn(usize, usize) -> *mut u8,
    dealloc: unsafe extern "C" fn(*mut u8, usize, usize),
}

/// A sequence op's immediates, matching `SeqInfo` in `stencils/stencils.rs` byte
/// for byte. Reached through a `*const SeqInfo` slot in the prog stream.
#[repr(C)]
struct SeqInfo {
    field_offset: usize,
    stride: usize,
    elem_align: usize,
    min_wire: usize,
    thunks_ctx: *const (),
    from_raw_parts:
        unsafe extern "C" fn(ctx: *const (), list: *mut u8, ptr: *mut u8, len: usize, cap: usize),
    element_entry: unsafe extern "C" fn(cx: *mut Ctx),
    element_prog: *const u64,
}

/// A bulk byte-run op's immediates, matching `BytesInfo` in
/// `stencils/stencils.rs` byte for byte. Reached through a `*const BytesInfo` slot
/// in the prog stream. Unlike [`SeqInfo`] there is no element body — the run is one
/// bulk copy — so it needs no `ExecBuf`-relative binding.
#[repr(C)]
struct BytesInfo {
    field_offset: usize,
    stride: usize,
    elem_align: usize,
    thunks_ctx: *const (),
    from_raw_parts:
        unsafe extern "C" fn(ctx: *const (), list: *mut u8, ptr: *mut u8, len: usize, cap: usize),
    validate: unsafe extern "C" fn(ptr: *const u8, len: usize) -> bool,
}

/// A borrowed, zero-copy byte-run op's immediates, matching `BorrowInfo` in
/// `stencils/stencils.rs` byte for byte. Reached through a `*const BorrowInfo` slot
/// in the prog stream. Like [`BytesInfo`] it carries no `ExecBuf`-relative fields —
/// decode writes a fat pointer into the input via the `set_borrowed` thunk, no
/// sub-chain — so it is built directly.
#[repr(C)]
struct BorrowInfo {
    field_offset: usize,
    stride: usize,
    elem_align: usize,
    thunks_ctx: *const (),
    set_borrowed:
        unsafe extern "C" fn(ctx: *const (), field: *mut u8, ptr: *const u8, len: usize) -> bool,
}

/// An option op's immediates, matching `OptInfo` in `stencils/stencils.rs` byte
/// for byte. Reached through a `*const OptInfo` slot in the prog stream. Like
/// [`SeqInfo`] it carries an `ExecBuf`-relative some-body entry + prog.
#[repr(C)]
struct OptInfo {
    field_offset: usize,
    inner_size: usize,
    inner_align: usize,
    thunks_ctx: *const (),
    init_none: unsafe extern "C" fn(ctx: *const (), option: *mut u8),
    init_some: unsafe extern "C" fn(ctx: *const (), option: *mut u8, value: *mut u8),
    some_entry: unsafe extern "C" fn(cx: *mut Ctx),
    some_prog: *const u64,
}

/// An owned-map op's immediates, matching `MapInfo` in `stencils/stencils.rs` byte
/// for byte. Reached through a `*const MapInfo` slot in the prog stream. Like
/// [`SeqInfo`] it carries `ExecBuf`-relative sub-chain entries + progs — but TWO of
/// them (key + value), the only op with two sub-chains.
#[repr(C)]
struct MapInfo {
    field_offset: usize,
    key_size: usize,
    key_align: usize,
    value_size: usize,
    value_align: usize,
    thunks_ctx: *const (),
    len: unsafe extern "C" fn(ctx: *const (), map: *const u8) -> usize,
    init_with_capacity: unsafe extern "C" fn(ctx: *const (), map: *mut u8, cap: usize),
    insert: unsafe extern "C" fn(ctx: *const (), map: *mut u8, key: *mut u8, value: *mut u8),
    key_entry: unsafe extern "C" fn(cx: *mut Ctx),
    key_prog: *const u64,
    value_entry: unsafe extern "C" fn(cx: *mut Ctx),
    value_prog: *const u64,
}

/// One enum variant's decode immediates, matching `EnumVariantInfo` in
/// `stencils/stencils.rs` byte for byte. The `payload_entry`/`payload_prog` are
/// `ExecBuf`-relative, bound after layout.
#[repr(C)]
struct EnumVariantInfo {
    wire_index: u32,
    selector: u64,
    payload_entry: unsafe extern "C" fn(cx: *mut Ctx),
    payload_prog: *const u64,
}

/// An enum op's immediates, matching `EnumInfo` in `stencils/stencils.rs` byte for
/// byte. Reached through a `*const EnumInfo` slot in the prog stream; `variants`
/// points at a stable `Vec<EnumVariantInfo>` heap buffer.
#[repr(C)]
struct EnumInfo {
    tag_offset: usize,
    tag_width: usize,
    variants: *const EnumVariantInfo,
    variant_count: usize,
    /// Stable `Vec<u32>` of writer-only wire indices (a removed variant arriving
    /// here is `WriterOnlyVariant`, not a garbage `BadVariantIndex`).
    writer_only: *const u32,
    writer_only_count: usize,
}

/// A reader-only-default op's immediates, matching `DefaultInfo` in
/// `stencils/stencils.rs` byte for byte. Reached through a `*const DefaultInfo`
/// slot in the prog stream. Like [`BytesInfo`] it carries no `ExecBuf`-relative
/// fields — the default is written by an indirect thunk call, no sub-chain — so it
/// needs no two-pass code-pointer binding. Decode-only.
#[repr(C)]
struct DefaultInfo {
    offset: usize,
    ctx: *const (),
    thunk: unsafe extern "C" fn(ctx: *const (), slot: *mut u8),
}

/// A writer-only-skip op's immediates, matching `SkipInfo` in
/// `stencils/stencils.rs` byte for byte. Reached through a `*const SkipInfo` slot
/// in the prog stream. Like [`BytesInfo`] it carries no `ExecBuf`-relative fields —
/// the skip is performed by an indirect walk call, no sub-chain. `skip_op` is a raw
/// pointer into the `SkipOp` owned by the `MemProgram`; the program must outlive
/// the `NativeDecode` (see the `progs`-pointer stability note on [`NativeDecode`]).
/// Decode-only.
#[repr(C)]
struct SkipInfo {
    skip_op: *const (),
    walk: unsafe extern "C" fn(skip_op: *const (), wire: *const u8, wire_end: *const u8)
        -> *const u8,
}

/// The `SkipInfo.walk` thunk: advance a cursor over `[wire, wire_end)` past one
/// writer value described by the `SkipOp` at `skip_op`, sharing the one skip walker
/// in `phon-ir` with the interpreter. Returns the advanced cursor on success, or
/// null on a skip failure (truncation, bad presence byte, or unmatched enum index)
/// — which `phon_stencil_skipwire` maps to `status = 1`.
///
/// # Safety
/// `skip_op` must point to a live [`SkipOp`]; `[wire, wire_end)` must be a valid,
/// readable byte range (`wire <= wire_end`).
unsafe extern "C" fn jit_skip_walk(
    skip_op: *const (),
    wire: *const u8,
    wire_end: *const u8,
) -> *const u8 {
    let len = (wire_end as usize) - (wire as usize);
    // Borrow the wire tail as a slice and run the shared walker over a fresh
    // `Reader`. On success the cursor advanced by `position()` bytes; on any error
    // signal failure with a null return.
    let bytes = unsafe { core::slice::from_raw_parts(wire, len) };
    let mut r = Reader::new(bytes);
    let op = unsafe { &*skip_op.cast::<SkipOp>() };
    match phon_ir::ir::skip(&mut r, op) {
        Ok(()) => unsafe { wire.add(r.position()) },
        Err(_) => core::ptr::null(),
    }
}

/// Allocate `size` bytes aligned to `align` with the global Rust allocator, so a
/// `Vec` adopting the buffer via `from_raw_parts` frees it with the same
/// allocator. Returns null on `size == 0`; the stencil substitutes a dangling
/// pointer in that case (and never calls this for an empty sequence).
unsafe extern "C" fn jit_alloc(size: usize, align: usize) -> *mut u8 {
    if size == 0 {
        return core::ptr::null_mut();
    }
    let layout = std::alloc::Layout::from_size_align(size, align).expect("valid element layout");
    unsafe { std::alloc::alloc(layout) }
}

/// Free a buffer from [`jit_alloc`] (same `size`/`align`).
unsafe extern "C" fn jit_dealloc(ptr: *mut u8, size: usize, align: usize) {
    if size == 0 {
        return;
    }
    let layout = std::alloc::Layout::from_size_align(size, align).expect("valid element layout");
    unsafe { std::alloc::dealloc(ptr, layout) };
}

/// A JIT-compiled decoder for a [`MemProgram`]: a `MAP_JIT` page of copied
/// stencils with their continuations patched to chain, ending at `done`. Scalars
/// chain straight through; an owned sequence runs its element body as a
/// separately compiled chain it calls once per element (`r[ir.stencils]`).
pub struct NativeDecode {
    buf: ExecBuf,
    /// Index into `progs` of the top-level chain's immediate stream.
    entry_prog: usize,
    /// Every chain's immediate stream: `[offset, size, align]` triples for
    /// scalars and a `*const SeqInfo` slot per sequence. Boxed so the addresses
    /// the stencils read (and the pointers stored in `seq_infos`) stay stable.
    progs: Vec<Vec<u64>>,
    /// One per sequence op: the immediates the sequence stencil reads through its
    /// prog slot. The `Vec`'s heap buffer is stable — built once with exact
    /// capacity, never re-grown, and a `Vec` move leaves the heap in place — so
    /// the `*const SeqInfo` the prog stream holds stays valid.
    seq_infos: Vec<SeqInfo>,
    /// One per bulk byte-run op: the immediates the bytes stencil reads through its
    /// prog slot. Same stability contract as `seq_infos`.
    bytes_infos: Vec<BytesInfo>,
    /// One per borrowed (zero-copy) byte-run op: the immediates the borrow stencil
    /// reads through its prog slot. Same stability contract as `seq_infos`.
    borrow_infos: Vec<BorrowInfo>,
    /// One per option op: the immediates the option stencil reads through its prog
    /// slot. Same stability contract as `seq_infos`.
    opt_infos: Vec<OptInfo>,
    /// One per map op: the immediates the map stencil reads through its prog slot.
    /// Same stability contract as `seq_infos`.
    map_infos: Vec<MapInfo>,
    /// One per enum op: the immediates the enum stencil reads through its prog
    /// slot. Same stability contract as `seq_infos`.
    enum_infos: Vec<EnumInfo>,
    /// One per enum op: that enum's variant table. Each inner `Vec`'s heap buffer
    /// is stable (exact capacity, never re-grown), so the `*const EnumVariantInfo`
    /// in `enum_infos` stays valid.
    enum_variants: Vec<Vec<EnumVariantInfo>>,
    /// One per enum op: that enum's writer-only wire indices. Same stability
    /// contract as `enum_variants`; the `*const u32` in `enum_infos` aliases here.
    enum_writer_only: Vec<Vec<u32>>,
    /// One per reader-only-default op: the immediates the default stencil reads
    /// through its prog slot. Same stability contract as `seq_infos`.
    default_infos: Vec<DefaultInfo>,
    /// One per writer-only-skip op: the immediates the skipwire stencil reads
    /// through its prog slot. Same stability contract as `seq_infos`. Each
    /// `SkipInfo.skip_op` points into the matching `Box<SkipOp>` in `skip_ops`.
    skip_infos: Vec<SkipInfo>,
    /// One per writer-only-skip op: an owned clone of that op's `SkipOp` tree. Built
    /// once with exact capacity and never re-grown after `compile` takes the
    /// `skip_op` pointers, so each element's address is stable — the same contract
    /// as `seq_infos`/`bytes_infos`. `NativeDecode` only borrows the source program
    /// during `compile`, so it must own these to point a raw `skip_op` at one for
    /// the `jit_skip_walk` thunk to dereference.
    skip_ops: Vec<SkipOp>,
}

/// A compiled element chain: where its first stencil begins in `code`, and which
/// `progs` entry feeds it.
struct Chain {
    entry: usize,
    prog_index: usize,
}

/// A sequence's prog slot to fill once chains are laid out and the `ExecBuf`
/// exists: write `&seq_infos[seqinfo]` into `progs[prog_index][slot]`.
struct SeqFixup {
    prog_index: usize,
    slot: usize,
    seqinfo: usize,
}

/// A bulk byte-run's prog slot to fill once `bytes_infos` is in its final home:
/// write `&bytes_infos[bytesinfo]` into `progs[prog_index][slot]`.
struct BytesFixup {
    prog_index: usize,
    slot: usize,
    bytesinfo: usize,
}

/// A borrowed byte-run's prog slot to fill once `borrow_infos` is in its final
/// home: write `&borrow_infos[borrowinfo]` into `progs[prog_index][slot]`.
struct BorrowFixup {
    prog_index: usize,
    slot: usize,
    borrowinfo: usize,
}

/// An option's prog slot to fill once `opt_infos` is in its final home: write
/// `&opt_infos[optinfo]` into `progs[prog_index][slot]`.
struct OptFixup {
    prog_index: usize,
    slot: usize,
    optinfo: usize,
}

/// A map's prog slot to fill once `map_infos` is in its final home: write
/// `&map_infos[mapinfo]` into `progs[prog_index][slot]`.
struct MapFixup {
    prog_index: usize,
    slot: usize,
    mapinfo: usize,
}

/// An enum's prog slot to fill once `enum_infos` is in its final home: write
/// `&enum_infos[enuminfo]` into `progs[prog_index][slot]`.
struct EnumFixup {
    prog_index: usize,
    slot: usize,
    enuminfo: usize,
}

/// A reader-only-default's prog slot to fill once `default_infos` is in its final
/// home: write `&default_infos[defaultinfo]` into `progs[prog_index][slot]`.
struct DefaultFixup {
    prog_index: usize,
    slot: usize,
    defaultinfo: usize,
}

/// A writer-only-skip's prog slot to fill once `skip_infos` is in its final home:
/// write `&skip_infos[skipinfo]` into `progs[prog_index][slot]`.
struct SkipFixup {
    prog_index: usize,
    slot: usize,
    skipinfo: usize,
}

/// A sequence's `SeqInfo` minus the two fields only known after the `ExecBuf` is
/// built: the element chain's entry offset and prog index.
struct SeqInfoBuild {
    field_offset: usize,
    stride: usize,
    elem_align: usize,
    min_wire: usize,
    thunks_ctx: *const (),
    from_raw_parts:
        unsafe extern "C" fn(ctx: *const (), list: *mut u8, ptr: *mut u8, len: usize, cap: usize),
    element_entry_offset: usize,
    element_prog_index: usize,
}

/// An option's `OptInfo` minus the two fields only known after the `ExecBuf` is
/// built: the some-body chain's entry offset and prog index.
struct OptInfoBuild {
    field_offset: usize,
    inner_size: usize,
    inner_align: usize,
    thunks_ctx: *const (),
    init_none: unsafe extern "C" fn(ctx: *const (), option: *mut u8),
    init_some: unsafe extern "C" fn(ctx: *const (), option: *mut u8, value: *mut u8),
    some_entry_offset: usize,
    some_prog_index: usize,
}

/// A map's `MapInfo` minus the four fields only known after the `ExecBuf` is
/// built: the key and value sub-chains' entry offsets and prog indices.
struct MapInfoBuild {
    field_offset: usize,
    key_size: usize,
    key_align: usize,
    value_size: usize,
    value_align: usize,
    thunks_ctx: *const (),
    len: unsafe extern "C" fn(ctx: *const (), map: *const u8) -> usize,
    init_with_capacity: unsafe extern "C" fn(ctx: *const (), map: *mut u8, cap: usize),
    insert: unsafe extern "C" fn(ctx: *const (), map: *mut u8, key: *mut u8, value: *mut u8),
    key_entry_offset: usize,
    key_prog_index: usize,
    value_entry_offset: usize,
    value_prog_index: usize,
}

/// One enum variant minus the two `ExecBuf`-relative fields: the payload chain's
/// entry offset and prog index.
struct EnumVariantInfoBuild {
    wire_index: u32,
    selector: u64,
    payload_entry_offset: usize,
    payload_prog_index: usize,
}

/// An enum's `EnumInfo` minus the variant table (built once the chains are laid
/// out and the `ExecBuf` exists).
struct EnumInfoBuild {
    tag_offset: usize,
    tag_width: usize,
    variants: Vec<EnumVariantInfoBuild>,
    /// Writer-only wire indices (copied from `EnumOp.writer_only`); pure data, no
    /// `ExecBuf`-relative binding.
    writer_only: Vec<u32>,
}

/// Accumulates the code bytes, per-chain prog streams, and sequence metadata while
/// the program is walked. Two passes: lay everything out (this struct), then bind
/// the `ExecBuf`-relative pointers ([`NativeDecode::compile`]).
struct Compiler {
    code: Vec<u8>,
    progs: Vec<Vec<u64>>,
    seq_infos: Vec<SeqInfoBuild>,
    fixups: Vec<SeqFixup>,
    /// Built directly (no `ExecBuf`-relative fields): one per bulk byte-run op.
    bytes_infos: Vec<BytesInfo>,
    bytes_fixups: Vec<BytesFixup>,
    /// Built directly (no `ExecBuf`-relative fields): one per borrowed byte-run op.
    borrow_infos: Vec<BorrowInfo>,
    borrow_fixups: Vec<BorrowFixup>,
    opt_infos: Vec<OptInfoBuild>,
    opt_fixups: Vec<OptFixup>,
    map_infos: Vec<MapInfoBuild>,
    map_fixups: Vec<MapFixup>,
    enum_infos: Vec<EnumInfoBuild>,
    enum_fixups: Vec<EnumFixup>,
    /// Built directly (no `ExecBuf`-relative fields): one per reader-only-default op.
    default_infos: Vec<DefaultInfo>,
    default_fixups: Vec<DefaultFixup>,
    /// One owned `SkipOp` clone per writer-only-skip op; the `SkipInfo` for each is
    /// materialized in `compile()` once these are in their final home (`nd.skip_ops`,
    /// never re-grown after) so the `skip_op` pointer is stable.
    skip_ops: Vec<SkipOp>,
    skip_fixups: Vec<SkipFixup>,
}

impl Compiler {
    /// Lay out one chain for `program`: a stencil copy per op, then a `done`, with
    /// continuations patched to chain to the next op (the last to `done`).
    /// Recurses into sequence elements, which become their own chains.
    fn compile_chain(&mut self, program: &MemProgram) -> Chain {
        let entry = self.code.len();
        let prog_index = self.progs.len();
        self.progs.push(Vec::new());

        // First emit each op's stencil copy and its immediates, recording where
        // each begins so continuations can be patched once `done` is placed.
        let mut starts = Vec::with_capacity(program.len());
        for op in program {
            starts.push(self.code.len());
            match op {
                MemOp::Scalar { offset, size, align } => {
                    self.code.extend_from_slice(SCALAR);
                    let p = &mut self.progs[prog_index];
                    p.push(*offset as u64);
                    p.push(*size as u64);
                    p.push(*align as u64);
                }
                MemOp::Sequence(s) => {
                    self.code.extend_from_slice(SEQUENCE);
                    // The sequence reads one prog slot: a `*const SeqInfo` filled
                    // in pass 2. Reserve it and record the fixup now.
                    let slot = self.progs[prog_index].len();
                    self.progs[prog_index].push(0);
                    // Compile the element body as its own chain.
                    let elem = self.compile_chain(&s.element);
                    let seqinfo = self.seq_infos.len();
                    self.seq_infos.push(SeqInfoBuild {
                        field_offset: s.field_offset,
                        stride: s.stride,
                        elem_align: s.elem_align,
                        min_wire: s.min_wire,
                        thunks_ctx: s.thunks.ctx,
                        from_raw_parts: s.thunks.from_raw_parts,
                        element_entry_offset: elem.entry,
                        element_prog_index: elem.prog_index,
                    });
                    self.fixups.push(SeqFixup { prog_index, slot, seqinfo });
                }
                MemOp::Bytes(b) => {
                    self.code.extend_from_slice(BYTES);
                    // The bytes stencil reads one prog slot: a `*const BytesInfo`
                    // filled in pass 2. Reserve it and record the fixup now.
                    let slot = self.progs[prog_index].len();
                    self.progs[prog_index].push(0);
                    let bytesinfo = self.bytes_infos.len();
                    self.bytes_infos.push(BytesInfo {
                        field_offset: b.field_offset,
                        stride: b.stride,
                        elem_align: b.elem_align,
                        thunks_ctx: b.thunks.ctx,
                        from_raw_parts: b.thunks.from_raw_parts,
                        // String runs validate UTF-8; `Vec` runs accept anything.
                        // The stencil calls this indirectly, so no relocation.
                        validate: b.validate,
                    });
                    self.bytes_fixups.push(BytesFixup { prog_index, slot, bytesinfo });
                }
                MemOp::Borrow(b) => {
                    self.code.extend_from_slice(BORROW);
                    // The borrow stencil reads one prog slot: a `*const BorrowInfo`
                    // filled in pass 2. Reserve it and record the fixup now. No
                    // `ExecBuf`-relative fields (the fat pointer is built into the
                    // input by an indirect thunk, no sub-chain), so build it now.
                    let slot = self.progs[prog_index].len();
                    self.progs[prog_index].push(0);
                    let borrowinfo = self.borrow_infos.len();
                    self.borrow_infos.push(BorrowInfo {
                        field_offset: b.field_offset,
                        stride: b.stride,
                        elem_align: b.elem_align,
                        thunks_ctx: b.thunks.ctx,
                        set_borrowed: b.thunks.set_borrowed,
                    });
                    self.borrow_fixups.push(BorrowFixup { prog_index, slot, borrowinfo });
                }
                MemOp::Option(o) => {
                    self.code.extend_from_slice(OPTION);
                    // The option stencil reads one prog slot: a `*const OptInfo`
                    // filled in pass 2. Reserve it and record the fixup now.
                    let slot = self.progs[prog_index].len();
                    self.progs[prog_index].push(0);
                    // Compile the some-body as its own chain.
                    let some = self.compile_chain(&o.some);
                    let optinfo = self.opt_infos.len();
                    self.opt_infos.push(OptInfoBuild {
                        field_offset: o.field_offset,
                        inner_size: o.inner_size,
                        inner_align: o.inner_align,
                        thunks_ctx: o.thunks.ctx,
                        init_none: o.thunks.init_none,
                        init_some: o.thunks.init_some,
                        some_entry_offset: some.entry,
                        some_prog_index: some.prog_index,
                    });
                    self.opt_fixups.push(OptFixup { prog_index, slot, optinfo });
                }
                MemOp::Enum(e) => {
                    self.code.extend_from_slice(ENUM);
                    // The enum stencil reads one prog slot: a `*const EnumInfo`
                    // filled in pass 2. Reserve it and record the fixup now.
                    let slot = self.progs[prog_index].len();
                    self.progs[prog_index].push(0);
                    // Compile each variant's payload as its own chain.
                    let mut variants = Vec::with_capacity(e.variants.len());
                    for v in &e.variants {
                        let payload = self.compile_chain(&v.payload);
                        variants.push(EnumVariantInfoBuild {
                            wire_index: v.wire_index,
                            selector: v.selector,
                            payload_entry_offset: payload.entry,
                            payload_prog_index: payload.prog_index,
                        });
                    }
                    let enuminfo = self.enum_infos.len();
                    self.enum_infos.push(EnumInfoBuild {
                        tag_offset: e.tag_offset,
                        tag_width: e.tag_width,
                        variants,
                        writer_only: e.writer_only.clone(),
                    });
                    self.enum_fixups.push(EnumFixup { prog_index, slot, enuminfo });
                }
                MemOp::Default(d) => {
                    self.code.extend_from_slice(DEFAULT);
                    // The default stencil reads one prog slot: a `*const DefaultInfo`
                    // filled in pass 2. Reserve it and record the fixup now.
                    let slot = self.progs[prog_index].len();
                    self.progs[prog_index].push(0);
                    let defaultinfo = self.default_infos.len();
                    // No `ExecBuf`-relative fields: the default is written by an
                    // indirect thunk call (no wire, no sub-chain), so build it now.
                    self.default_infos.push(DefaultInfo {
                        offset: d.offset,
                        ctx: d.ctx,
                        thunk: d.default,
                    });
                    self.default_fixups.push(DefaultFixup { prog_index, slot, defaultinfo });
                }
                MemOp::SkipWire(s) => {
                    self.code.extend_from_slice(SKIPWIRE);
                    // The skipwire stencil reads one prog slot: a `*const SkipInfo`
                    // filled in pass 2 (its `skip_op` pointer is bound once the
                    // owned `SkipOp` clone is in its final home). Reserve the slot
                    // and clone the tree now.
                    let slot = self.progs[prog_index].len();
                    self.progs[prog_index].push(0);
                    let skipinfo = self.skip_ops.len();
                    self.skip_ops.push((**s).clone());
                    self.skip_fixups.push(SkipFixup { prog_index, slot, skipinfo });
                }
                MemOp::Map(m) => {
                    self.code.extend_from_slice(MAP);
                    // The map stencil reads one prog slot: a `*const MapInfo`
                    // filled in pass 2. Reserve it and record the fixup now.
                    let slot = self.progs[prog_index].len();
                    self.progs[prog_index].push(0);
                    // Compile the key and value sub-bodies as their own chains.
                    let key = self.compile_chain(&m.key);
                    let value = self.compile_chain(&m.value);
                    let mapinfo = self.map_infos.len();
                    self.map_infos.push(MapInfoBuild {
                        field_offset: m.field_offset,
                        key_size: m.key_size,
                        key_align: m.key_align,
                        value_size: m.value_size,
                        value_align: m.value_align,
                        thunks_ctx: m.thunks.ctx,
                        len: m.thunks.len,
                        init_with_capacity: m.thunks.init_with_capacity,
                        insert: m.thunks.insert,
                        key_entry_offset: key.entry,
                        key_prog_index: key.prog_index,
                        value_entry_offset: value.entry,
                        value_prog_index: value.prog_index,
                    });
                    self.map_fixups.push(MapFixup { prog_index, slot, mapinfo });
                }
                MemOp::Result(_) => panic!("phon-jit: Result is interpreter-only for now"),
                MemOp::Dynamic { .. } => panic!("phon-jit: dynamic Value is interpreter-only for now"),
                MemOp::Opaque(_) => panic!("phon-jit: opaque fields are interpreter-only for now"),
            }
        }
        let done_start = self.code.len();
        self.code.extend_from_slice(DONE);

        // Patch every op's continuation branches to the following op (last ->
        // `done`). Scalar and sequence stencils both reach the next op through a
        // `phon_cont` BRANCH26.
        for (i, &op_start) in starts.iter().enumerate() {
            let next = starts.get(i + 1).copied().unwrap_or(done_start);
            let relocs = match &program[i] {
                MemOp::Scalar { .. } => SCALAR_CONT,
                MemOp::Sequence(_) => SEQUENCE_CONT,
                MemOp::Bytes(_) => BYTES_CONT,
                MemOp::Borrow(_) => BORROW_CONT,
                MemOp::Option(_) => OPTION_CONT,
                MemOp::Enum(_) => ENUM_CONT,
                MemOp::Default(_) => DEFAULT_CONT,
                MemOp::SkipWire(_) => SKIPWIRE_CONT,
                MemOp::Map(_) => MAP_CONT,
                MemOp::Result(_) => panic!("phon-jit: Result is interpreter-only for now"),
                MemOp::Dynamic { .. } => panic!("phon-jit: dynamic Value is interpreter-only for now"),
                MemOp::Opaque(_) => panic!("phon-jit: opaque fields are interpreter-only for now"),
            };
            for &rel in relocs {
                patch_branch26(&mut self.code, op_start + rel, next);
            }
        }

        Chain { entry, prog_index }
    }
}

impl NativeDecode {
    /// Compile a [`MemProgram`] to native machine code.
    // r[impl ir.stencils]
    #[must_use]
    pub fn compile(program: &MemProgram) -> NativeDecode {
        let mut c = Compiler {
            code: Vec::new(),
            progs: Vec::new(),
            seq_infos: Vec::new(),
            fixups: Vec::new(),
            bytes_infos: Vec::new(),
            bytes_fixups: Vec::new(),
            borrow_infos: Vec::new(),
            borrow_fixups: Vec::new(),
            opt_infos: Vec::new(),
            opt_fixups: Vec::new(),
            map_infos: Vec::new(),
            map_fixups: Vec::new(),
            enum_infos: Vec::new(),
            enum_fixups: Vec::new(),
            default_infos: Vec::new(),
            default_fixups: Vec::new(),
            skip_ops: Vec::new(),
            skip_fixups: Vec::new(),
        };
        let top = c.compile_chain(program);

        // The code layout is final; make it executable. Pointers into it are now
        // stable for the lifetime of the `ExecBuf`.
        let buf = ExecBuf::new(&c.code);
        let base = buf.as_ptr();

        // Box each chain's prog stream so its address is stable (the prog
        // pointers in `seq_infos` and the entry pointer alias into these).
        let progs = c.progs;

        // Materialize the `SeqInfo`s now that the code base is known. Reserve the
        // exact capacity so the `Vec` is never re-grown: its heap buffer (and thus
        // each `&SeqInfo` the prog slots point at) then stays put.
        let mut seq_infos: Vec<SeqInfo> = Vec::with_capacity(c.seq_infos.len());
        for b in &c.seq_infos {
            // The element chain entry is the stencil at `base + entry_offset`.
            let element_entry: unsafe extern "C" fn(*mut Ctx) =
                unsafe { core::mem::transmute(base.add(b.element_entry_offset)) };
            seq_infos.push(SeqInfo {
                field_offset: b.field_offset,
                stride: b.stride,
                elem_align: b.elem_align,
                min_wire: b.min_wire,
                thunks_ctx: b.thunks_ctx,
                from_raw_parts: b.from_raw_parts,
                element_entry,
                // Bound below, once `progs` is owned by `NativeDecode`.
                element_prog: core::ptr::null(),
            });
        }

        // Materialize the `OptInfo`s now that the code base is known (exact
        // capacity, never re-grown — its heap stays put for the prog slots).
        let mut opt_infos: Vec<OptInfo> = Vec::with_capacity(c.opt_infos.len());
        for b in &c.opt_infos {
            let some_entry: unsafe extern "C" fn(*mut Ctx) =
                unsafe { core::mem::transmute(base.add(b.some_entry_offset)) };
            opt_infos.push(OptInfo {
                field_offset: b.field_offset,
                inner_size: b.inner_size,
                inner_align: b.inner_align,
                thunks_ctx: b.thunks_ctx,
                init_none: b.init_none,
                init_some: b.init_some,
                some_entry,
                // Bound below, once `progs` is owned by `NativeDecode`.
                some_prog: core::ptr::null(),
            });
        }

        // Materialize the `MapInfo`s now that the code base is known (exact
        // capacity, never re-grown — its heap stays put for the prog slots). The
        // key and value sub-chain entries are `ExecBuf`-relative; the progs are
        // bound below once `progs` is owned by `NativeDecode`.
        let mut map_infos: Vec<MapInfo> = Vec::with_capacity(c.map_infos.len());
        for b in &c.map_infos {
            let key_entry: unsafe extern "C" fn(*mut Ctx) =
                unsafe { core::mem::transmute(base.add(b.key_entry_offset)) };
            let value_entry: unsafe extern "C" fn(*mut Ctx) =
                unsafe { core::mem::transmute(base.add(b.value_entry_offset)) };
            map_infos.push(MapInfo {
                field_offset: b.field_offset,
                key_size: b.key_size,
                key_align: b.key_align,
                value_size: b.value_size,
                value_align: b.value_align,
                thunks_ctx: b.thunks_ctx,
                len: b.len,
                init_with_capacity: b.init_with_capacity,
                insert: b.insert,
                key_entry,
                key_prog: core::ptr::null(),
                value_entry,
                value_prog: core::ptr::null(),
            });
        }

        // Materialize each enum's variant table (the payload entries are
        // `ExecBuf`-relative; the payload progs are bound below). Each inner `Vec`
        // gets exact capacity so its heap stays put for the `EnumInfo` pointer.
        let mut enum_variants: Vec<Vec<EnumVariantInfo>> =
            Vec::with_capacity(c.enum_infos.len());
        for e in &c.enum_infos {
            let mut variants: Vec<EnumVariantInfo> = Vec::with_capacity(e.variants.len());
            for v in &e.variants {
                let payload_entry: unsafe extern "C" fn(*mut Ctx) =
                    unsafe { core::mem::transmute(base.add(v.payload_entry_offset)) };
                variants.push(EnumVariantInfo {
                    wire_index: v.wire_index,
                    selector: v.selector,
                    payload_entry,
                    payload_prog: core::ptr::null(),
                });
            }
            enum_variants.push(variants);
        }
        // Each enum's writer-only index list (pure data; pointers bound below once
        // owned by `NativeDecode`). Exact capacity so the heaps stay put.
        let mut enum_writer_only: Vec<Vec<u32>> = Vec::with_capacity(c.enum_infos.len());
        for e in &c.enum_infos {
            enum_writer_only.push(e.writer_only.clone());
        }
        // The `EnumInfo`s themselves (variant + writer-only pointers bound below,
        // once `enum_variants`/`enum_writer_only` are owned by `NativeDecode`).
        let mut enum_infos: Vec<EnumInfo> = Vec::with_capacity(c.enum_infos.len());
        for e in &c.enum_infos {
            enum_infos.push(EnumInfo {
                tag_offset: e.tag_offset,
                tag_width: e.tag_width,
                variants: core::ptr::null(),
                variant_count: e.variants.len(),
                writer_only: core::ptr::null(),
                writer_only_count: e.writer_only.len(),
            });
        }

        // Materialize the `SkipInfo`s with their `walk` thunk bound; the `skip_op`
        // pointer is bound below, once `skip_ops` is owned by `NativeDecode` so each
        // `Box<SkipOp>`'s heap address is final.
        let skip_infos: Vec<SkipInfo> = (0..c.skip_ops.len())
            .map(|_| SkipInfo { skip_op: core::ptr::null(), walk: jit_skip_walk })
            .collect();

        let mut nd = NativeDecode {
            buf,
            entry_prog: top.prog_index,
            progs,
            seq_infos,
            // Move the byte-run infos into their final home; they carry no
            // `ExecBuf`-relative fields, so no further binding is needed.
            bytes_infos: c.bytes_infos,
            // The borrow infos carry no `ExecBuf`-relative fields either.
            borrow_infos: c.borrow_infos,
            opt_infos,
            map_infos,
            enum_infos,
            enum_variants,
            enum_writer_only,
            // The default infos carry no `ExecBuf`-relative fields either.
            default_infos: c.default_infos,
            skip_infos,
            // Move the owned `SkipOp` clones into their final home; the `skip_op`
            // pointers below alias into these boxes.
            skip_ops: c.skip_ops,
        };

        // Bind each `SkipInfo.skip_op` to its owned `SkipOp` clone, now that
        // `nd.skip_ops` is final (a `Box`'s heap stays put across the `Vec` move).
        let skip_op_ptrs: Vec<*const ()> = nd
            .skip_ops
            .iter()
            .map(|op| core::ptr::from_ref::<SkipOp>(op).cast::<()>())
            .collect();
        for (info, &ptr) in nd.skip_infos.iter_mut().zip(skip_op_ptrs.iter()) {
            info.skip_op = ptr;
        }

        // Now that `nd.progs` is in its final home, bind the prog pointers: each
        // `SeqInfo.element_prog` to its element chain's stream, and each sequence
        // prog slot to its `SeqInfo`.
        for (b, info) in c.seq_infos.iter().zip(nd.seq_infos.iter_mut()) {
            info.element_prog = nd.progs[b.element_prog_index].as_ptr();
        }
        for f in &c.fixups {
            let ptr: *const SeqInfo = &nd.seq_infos[f.seqinfo];
            nd.progs[f.prog_index][f.slot] = ptr as u64;
        }
        // Bind each bulk byte-run's prog slot to its `BytesInfo` in `nd`.
        for f in &c.bytes_fixups {
            let ptr: *const BytesInfo = &nd.bytes_infos[f.bytesinfo];
            nd.progs[f.prog_index][f.slot] = ptr as u64;
        }
        // Bind each borrowed byte-run's prog slot to its `BorrowInfo` in `nd`.
        for f in &c.borrow_fixups {
            let ptr: *const BorrowInfo = &nd.borrow_infos[f.borrowinfo];
            nd.progs[f.prog_index][f.slot] = ptr as u64;
        }
        // Bind each option's some-body prog and its prog slot to the `OptInfo`.
        for (b, info) in c.opt_infos.iter().zip(nd.opt_infos.iter_mut()) {
            info.some_prog = nd.progs[b.some_prog_index].as_ptr();
        }
        for f in &c.opt_fixups {
            let ptr: *const OptInfo = &nd.opt_infos[f.optinfo];
            nd.progs[f.prog_index][f.slot] = ptr as u64;
        }
        // Bind each map's key and value sub-chain progs, then its prog slot to the
        // `MapInfo` (the two sub-chains are bound like the sequence/option ones,
        // but there are two of them).
        for (b, info) in c.map_infos.iter().zip(nd.map_infos.iter_mut()) {
            info.key_prog = nd.progs[b.key_prog_index].as_ptr();
            info.value_prog = nd.progs[b.value_prog_index].as_ptr();
        }
        for f in &c.map_fixups {
            let ptr: *const MapInfo = &nd.map_infos[f.mapinfo];
            nd.progs[f.prog_index][f.slot] = ptr as u64;
        }
        // Bind each enum variant's payload prog, point each `EnumInfo` at its
        // (now stable) variant table, then fill each enum's prog slot.
        for (eb, variants) in c.enum_infos.iter().zip(nd.enum_variants.iter_mut()) {
            for (vb, vi) in eb.variants.iter().zip(variants.iter_mut()) {
                vi.payload_prog = nd.progs[vb.payload_prog_index].as_ptr();
            }
        }
        let variant_ptrs: Vec<*const EnumVariantInfo> =
            nd.enum_variants.iter().map(|v| v.as_ptr()).collect();
        for (info, &ptr) in nd.enum_infos.iter_mut().zip(variant_ptrs.iter()) {
            info.variants = ptr;
        }
        // Point each `EnumInfo` at its (now stable) writer-only index list.
        let writer_only_ptrs: Vec<*const u32> =
            nd.enum_writer_only.iter().map(|w| w.as_ptr()).collect();
        for (info, &ptr) in nd.enum_infos.iter_mut().zip(writer_only_ptrs.iter()) {
            info.writer_only = ptr;
        }
        for f in &c.enum_fixups {
            let ptr: *const EnumInfo = &nd.enum_infos[f.enuminfo];
            nd.progs[f.prog_index][f.slot] = ptr as u64;
        }
        // Bind each reader-only-default's prog slot to its `DefaultInfo` in `nd`.
        for f in &c.default_fixups {
            let ptr: *const DefaultInfo = &nd.default_infos[f.defaultinfo];
            nd.progs[f.prog_index][f.slot] = ptr as u64;
        }
        // Bind each writer-only-skip's prog slot to its `SkipInfo` in `nd`.
        for f in &c.skip_fixups {
            let ptr: *const SkipInfo = &nd.skip_infos[f.skipinfo];
            nd.progs[f.prog_index][f.slot] = ptr as u64;
        }

        nd
    }

    /// Decode `bytes` into the value at `base`, rejecting trailing bytes.
    ///
    /// # Safety
    /// `base` must point to writable storage matching the descriptor this program
    /// was lowered from.
    ///
    /// # Errors
    /// [`DecodeError`] for truncated or trailing input.
    pub unsafe fn run(&self, bytes: &[u8], base: *mut u8) -> Result<(), DecodeError> {
        let start = bytes.as_ptr();
        let mut ctx = Ctx {
            wire: start,
            wire_start: start,
            wire_end: unsafe { start.add(bytes.len()) },
            base,
            prog: self.progs[self.entry_prog].as_ptr(),
            status: 0,
            aux: 0,
            alloc: jit_alloc,
            dealloc: jit_dealloc,
        };
        let entry: extern "C" fn(*mut Ctx) = unsafe { core::mem::transmute(self.buf.as_ptr()) };
        entry(&mut ctx);

        if ctx.status != 0 {
            // Map the stencils' status codes to precise `DecodeError`s.
            match ctx.status {
                // Content validation failed (e.g. a `String` run was not UTF-8).
                2 => return Err(DecodeError::InvalidUtf8),
                // Bad `Option` presence byte (the byte is in `aux`).
                3 => return Err(DecodeError::InvalidBool(ctx.aux as u8)),
                // A garbage enum wire index in neither the reader's variants nor
                // the writer's known set (the index is in `aux`).
                4 => return Err(DecodeError::BadVariantIndex(ctx.aux as u32)),
                // A variant the writer has but the reader removed (the index is in
                // `aux`) — the `DecodeError`-channel counterpart of the interpreter's
                // `CompactError::WriterOnlyVariant`.
                5 => return Err(DecodeError::WriterOnlyVariant(ctx.aux as u32)),
                // A repeated map key collapsed two entries into one (the post-loop
                // `len != count` check) — the interpreter's `DuplicateKey`.
                6 => return Err(DecodeError::DuplicateKey),
                // Anything else is a truncation/bounds failure.
                _ => {}
            }
            let remaining = ctx.wire_end as usize - ctx.wire as usize;
            return Err(DecodeError::UnexpectedEof { needed: 0, remaining });
        }
        let consumed = ctx.wire as usize - start as usize;
        if consumed != bytes.len() {
            return Err(DecodeError::TrailingBytes(bytes.len() - consumed));
        }
        Ok(())
    }
}

// ============================================================================
// The encode JIT
// ============================================================================

/// The encode-side threaded state, matching `EncCtx` in `stencils/stencils.rs`
/// byte for byte. Where decode reads a fixed wire slice into memory, encode reads
/// memory and appends to a *growing* output: that growth is the only structural
/// difference from decode. The engine owns the `Vec<u8>`; `out_handle` points at
/// it so [`jit_grow`] can reserve through it (keeping its allocator), and the
/// driver sets the `Vec`'s length to `out_pos` after the run.
#[repr(C)]
struct EncCtx {
    base: *const u8,
    out_handle: *mut u8,
    out_ptr: *mut u8,
    out_pos: usize,
    out_cap: usize,
    prog: *const u64,
    grow: unsafe extern "C" fn(cx: *mut EncCtx, needed: usize),
}

/// An encode sequence op's immediates, matching `EncSeqInfo` in
/// `stencils/stencils.rs` byte for byte. Reached through a `*const EncSeqInfo`
/// slot in the prog stream.
#[repr(C)]
struct EncSeqInfo {
    field_offset: usize,
    stride: usize,
    thunks_ctx: *const (),
    len: unsafe extern "C" fn(ctx: *const (), list: *const u8) -> usize,
    data: unsafe extern "C" fn(ctx: *const (), list: *const u8) -> *const u8,
    element_entry: unsafe extern "C" fn(cx: *mut EncCtx),
    element_prog: *const u64,
}

/// An encode bulk byte-run op's immediates, matching `EncBytesInfo` in
/// `stencils/stencils.rs` byte for byte. Reached through a `*const EncBytesInfo`
/// slot in the prog stream. Like the decode `BytesInfo`, it carries no
/// `ExecBuf`-relative fields (no element body).
#[repr(C)]
struct EncBytesInfo {
    field_offset: usize,
    stride: usize,
    elem_align: usize,
    thunks_ctx: *const (),
    len: unsafe extern "C" fn(ctx: *const (), list: *const u8) -> usize,
    data: unsafe extern "C" fn(ctx: *const (), list: *const u8) -> *const u8,
}

/// An encode option op's immediates, matching `EncOptInfo` in
/// `stencils/stencils.rs` byte for byte. Reached through a `*const EncOptInfo`
/// slot in the prog stream; carries an `ExecBuf`-relative some-body entry + prog.
#[repr(C)]
struct EncOptInfo {
    field_offset: usize,
    thunks_ctx: *const (),
    is_some: unsafe extern "C" fn(ctx: *const (), option: *const u8) -> bool,
    get_value: unsafe extern "C" fn(ctx: *const (), option: *const u8) -> *const u8,
    some_entry: unsafe extern "C" fn(cx: *mut EncCtx),
    some_prog: *const u64,
}

/// An encode owned-map op's immediates, matching `EncMapInfo` in
/// `stencils/stencils.rs` byte for byte. Reached through a `*const EncMapInfo` slot
/// in the prog stream; carries TWO `ExecBuf`-relative sub-chain (entry, prog) pairs
/// (key + value).
#[repr(C)]
struct EncMapInfo {
    field_offset: usize,
    thunks_ctx: *const (),
    len: unsafe extern "C" fn(ctx: *const (), map: *const u8) -> usize,
    iter_init: unsafe extern "C" fn(ctx: *const (), map: *const u8) -> *mut (),
    iter_next: unsafe extern "C" fn(
        ctx: *const (),
        iter: *mut (),
        key_out: *mut *const u8,
        value_out: *mut *const u8,
    ) -> bool,
    iter_dealloc: unsafe extern "C" fn(ctx: *const (), iter: *mut ()),
    key_entry: unsafe extern "C" fn(cx: *mut EncCtx),
    key_prog: *const u64,
    value_entry: unsafe extern "C" fn(cx: *mut EncCtx),
    value_prog: *const u64,
}

/// One enum variant's encode immediates, matching `EncEnumVariantInfo` in
/// `stencils/stencils.rs` byte for byte. The `payload_entry`/`payload_prog` are
/// `ExecBuf`-relative, bound after layout.
#[repr(C)]
struct EncEnumVariantInfo {
    wire_index: u32,
    selector: u64,
    payload_entry: unsafe extern "C" fn(cx: *mut EncCtx),
    payload_prog: *const u64,
}

/// An encode enum op's immediates, matching `EncEnumInfo` in
/// `stencils/stencils.rs` byte for byte. Reached through a `*const EncEnumInfo`
/// slot in the prog stream; `variants` points at a stable
/// `Vec<EncEnumVariantInfo>` heap buffer.
#[repr(C)]
struct EncEnumInfo {
    tag_offset: usize,
    tag_width: usize,
    variants: *const EncEnumVariantInfo,
    variant_count: usize,
}

/// Grow the engine-owned output `Vec<u8>` to hold at least `needed` bytes, then
/// write the new data pointer and capacity back into the `EncCtx`. Called
/// indirectly through `EncCtx.grow`, so the only relocation a copied encode
/// stencil carries is still its `phon_econt` `BRANCH26`.
///
/// The `Vec`'s live length is tracked in `cx.out_pos`; here we keep the `Vec`'s
/// own length at 0 and use [`Vec::reserve`] from a 0-length buffer to enlarge it,
/// so `reserve` never copies bytes it doesn't need to — but the bytes already
/// written below `out_pos` must survive, so we instead set the `Vec`'s length to
/// `out_pos` before reserving (preserving them), then drop it back to 0.
unsafe extern "C" fn jit_grow(cx: *mut EncCtx, needed: usize) {
    let c = unsafe { &mut *cx };
    let v = unsafe { &mut *c.out_handle.cast::<Vec<u8>>() };
    // Make the live bytes part of the Vec's length so `reserve` preserves them on
    // reallocation, then ask for enough headroom to reach `needed`.
    unsafe { v.set_len(c.out_pos) };
    if needed > v.len() {
        v.reserve(needed - v.len());
    }
    // Hand the length back to the driver/stencils via `out_pos`; the Vec carries
    // no length of its own between calls.
    unsafe { v.set_len(0) };
    c.out_ptr = v.as_mut_ptr();
    c.out_cap = v.capacity();
}

/// A JIT-compiled encoder for a [`MemProgram`]: a `MAP_JIT` page of copied encode
/// stencils with their continuations patched to chain, ending at `done`. The
/// mirror of [`NativeDecode`] — scalars chain straight through; an owned sequence
/// runs its element body as a separately compiled chain it calls once per element.
pub struct NativeEncode {
    buf: ExecBuf,
    /// Index into `progs` of the top-level chain's immediate stream.
    entry_prog: usize,
    /// Every chain's immediate stream: `[offset, size, align]` triples for
    /// scalars and a `*const EncSeqInfo` slot per sequence.
    progs: Vec<Vec<u64>>,
    /// One per sequence op: the immediates the sequence stencil reads through its
    /// prog slot. Built once with exact capacity, never re-grown, so the
    /// `*const EncSeqInfo` the prog stream holds stays valid.
    seq_infos: Vec<EncSeqInfo>,
    /// One per bulk byte-run op: the immediates the bytes stencil reads through its
    /// prog slot. Same stability contract as `seq_infos`.
    bytes_infos: Vec<EncBytesInfo>,
    /// One per option op. Same stability contract as `seq_infos`.
    opt_infos: Vec<EncOptInfo>,
    /// One per map op. Same stability contract as `seq_infos`.
    map_infos: Vec<EncMapInfo>,
    /// One per enum op. Same stability contract as `seq_infos`.
    enum_infos: Vec<EncEnumInfo>,
    /// One per enum op: that enum's variant table (stable heap per inner `Vec`).
    enum_variants: Vec<Vec<EncEnumVariantInfo>>,
    /// Byte length the previous `run` produced. The next `run` pre-reserves this,
    /// so a steady stream of similar-sized values pays no buffer-grow cost after
    /// warmup (the cap-0 cold path costs ~log2(size) reallocations + copies).
    /// `Relaxed`: a sizing hint, never a correctness input.
    last_size: AtomicUsize,
}

/// An encode sequence's `EncSeqInfo` minus the two fields only known after the
/// `ExecBuf` is built: the element chain's entry offset and prog index.
struct EncSeqInfoBuild {
    field_offset: usize,
    stride: usize,
    thunks_ctx: *const (),
    len: unsafe extern "C" fn(ctx: *const (), list: *const u8) -> usize,
    data: unsafe extern "C" fn(ctx: *const (), list: *const u8) -> *const u8,
    element_entry_offset: usize,
    element_prog_index: usize,
}

/// An encode option's `EncOptInfo` minus the two `ExecBuf`-relative fields.
struct EncOptInfoBuild {
    field_offset: usize,
    thunks_ctx: *const (),
    is_some: unsafe extern "C" fn(ctx: *const (), option: *const u8) -> bool,
    get_value: unsafe extern "C" fn(ctx: *const (), option: *const u8) -> *const u8,
    some_entry_offset: usize,
    some_prog_index: usize,
}

/// An encode map's `EncMapInfo` minus the four `ExecBuf`-relative fields: the key
/// and value sub-chains' entry offsets and prog indices.
struct EncMapInfoBuild {
    field_offset: usize,
    thunks_ctx: *const (),
    len: unsafe extern "C" fn(ctx: *const (), map: *const u8) -> usize,
    iter_init: unsafe extern "C" fn(ctx: *const (), map: *const u8) -> *mut (),
    iter_next: unsafe extern "C" fn(
        ctx: *const (),
        iter: *mut (),
        key_out: *mut *const u8,
        value_out: *mut *const u8,
    ) -> bool,
    iter_dealloc: unsafe extern "C" fn(ctx: *const (), iter: *mut ()),
    key_entry_offset: usize,
    key_prog_index: usize,
    value_entry_offset: usize,
    value_prog_index: usize,
}

/// One encode enum variant minus the two `ExecBuf`-relative fields.
struct EncEnumVariantInfoBuild {
    wire_index: u32,
    selector: u64,
    payload_entry_offset: usize,
    payload_prog_index: usize,
}

/// An encode enum's `EncEnumInfo` minus the variant table.
struct EncEnumInfoBuild {
    tag_offset: usize,
    tag_width: usize,
    variants: Vec<EncEnumVariantInfoBuild>,
}

/// Accumulates the encode code bytes, per-chain prog streams, and sequence
/// metadata while the program is walked. Two passes, like the decode [`Compiler`]:
/// lay everything out, then bind the `ExecBuf`-relative pointers.
struct EncCompiler {
    code: Vec<u8>,
    progs: Vec<Vec<u64>>,
    seq_infos: Vec<EncSeqInfoBuild>,
    fixups: Vec<SeqFixup>,
    /// Built directly (no `ExecBuf`-relative fields): one per bulk byte-run op.
    bytes_infos: Vec<EncBytesInfo>,
    bytes_fixups: Vec<BytesFixup>,
    opt_infos: Vec<EncOptInfoBuild>,
    opt_fixups: Vec<OptFixup>,
    map_infos: Vec<EncMapInfoBuild>,
    map_fixups: Vec<MapFixup>,
    enum_infos: Vec<EncEnumInfoBuild>,
    enum_fixups: Vec<EnumFixup>,
}

impl EncCompiler {
    /// Lay out one encode chain for `program`: a stencil copy per op, then a
    /// `done`, with continuations patched to chain to the next op (the last to
    /// `done`). Recurses into sequence elements, which become their own chains.
    fn compile_chain(&mut self, program: &MemProgram) -> Chain {
        let entry = self.code.len();
        let prog_index = self.progs.len();
        self.progs.push(Vec::new());

        let mut starts = Vec::with_capacity(program.len());
        for op in program {
            starts.push(self.code.len());
            match op {
                MemOp::Scalar { offset, size, align } => {
                    self.code.extend_from_slice(SCALAR_ENC);
                    let p = &mut self.progs[prog_index];
                    p.push(*offset as u64);
                    p.push(*size as u64);
                    p.push(*align as u64);
                }
                MemOp::Sequence(s) => {
                    self.code.extend_from_slice(SEQUENCE_ENC);
                    let slot = self.progs[prog_index].len();
                    self.progs[prog_index].push(0);
                    let elem = self.compile_chain(&s.element);
                    let seqinfo = self.seq_infos.len();
                    self.seq_infos.push(EncSeqInfoBuild {
                        field_offset: s.field_offset,
                        stride: s.stride,
                        thunks_ctx: s.thunks.ctx,
                        len: s.thunks.len,
                        data: s.thunks.data,
                        element_entry_offset: elem.entry,
                        element_prog_index: elem.prog_index,
                    });
                    self.fixups.push(SeqFixup { prog_index, slot, seqinfo });
                }
                MemOp::Bytes(b) => {
                    // Encode never validates — the in-memory `String`/`Vec` is
                    // already well-formed; we just copy its bytes out.
                    self.code.extend_from_slice(BYTES_ENC);
                    let slot = self.progs[prog_index].len();
                    self.progs[prog_index].push(0);
                    let bytesinfo = self.bytes_infos.len();
                    self.bytes_infos.push(EncBytesInfo {
                        field_offset: b.field_offset,
                        stride: b.stride,
                        elem_align: b.elem_align,
                        thunks_ctx: b.thunks.ctx,
                        len: b.thunks.len,
                        data: b.thunks.data,
                    });
                    self.bytes_fixups.push(BytesFixup { prog_index, slot, bytesinfo });
                }
                MemOp::Borrow(b) => {
                    // Encode of a borrowed leaf is byte-identical to the owned bulk
                    // run: the same `BYTES_ENC` stencil reads the `&str`/`&[u8]`
                    // length + bytes through the borrow thunks (whose `len`/`data`
                    // share `SeqThunks`' signatures) and writes the wire run.
                    self.code.extend_from_slice(BYTES_ENC);
                    let slot = self.progs[prog_index].len();
                    self.progs[prog_index].push(0);
                    let bytesinfo = self.bytes_infos.len();
                    self.bytes_infos.push(EncBytesInfo {
                        field_offset: b.field_offset,
                        stride: b.stride,
                        elem_align: b.elem_align,
                        thunks_ctx: b.thunks.ctx,
                        len: b.thunks.len,
                        data: b.thunks.data,
                    });
                    self.bytes_fixups.push(BytesFixup { prog_index, slot, bytesinfo });
                }
                MemOp::Option(o) => {
                    self.code.extend_from_slice(OPTION_ENC);
                    let slot = self.progs[prog_index].len();
                    self.progs[prog_index].push(0);
                    let some = self.compile_chain(&o.some);
                    let optinfo = self.opt_infos.len();
                    self.opt_infos.push(EncOptInfoBuild {
                        field_offset: o.field_offset,
                        thunks_ctx: o.thunks.ctx,
                        is_some: o.thunks.is_some,
                        get_value: o.thunks.get_value,
                        some_entry_offset: some.entry,
                        some_prog_index: some.prog_index,
                    });
                    self.opt_fixups.push(OptFixup { prog_index, slot, optinfo });
                }
                MemOp::Enum(e) => {
                    self.code.extend_from_slice(ENUM_ENC);
                    let slot = self.progs[prog_index].len();
                    self.progs[prog_index].push(0);
                    let mut variants = Vec::with_capacity(e.variants.len());
                    for v in &e.variants {
                        let payload = self.compile_chain(&v.payload);
                        variants.push(EncEnumVariantInfoBuild {
                            wire_index: v.wire_index,
                            selector: v.selector,
                            payload_entry_offset: payload.entry,
                            payload_prog_index: payload.prog_index,
                        });
                    }
                    let enuminfo = self.enum_infos.len();
                    self.enum_infos.push(EncEnumInfoBuild {
                        tag_offset: e.tag_offset,
                        tag_width: e.tag_width,
                        variants,
                    });
                    self.enum_fixups.push(EnumFixup { prog_index, slot, enuminfo });
                }
                MemOp::Map(m) => {
                    self.code.extend_from_slice(MAP_ENC);
                    let slot = self.progs[prog_index].len();
                    self.progs[prog_index].push(0);
                    // Compile the key and value sub-bodies as their own chains.
                    let key = self.compile_chain(&m.key);
                    let value = self.compile_chain(&m.value);
                    let mapinfo = self.map_infos.len();
                    self.map_infos.push(EncMapInfoBuild {
                        field_offset: m.field_offset,
                        thunks_ctx: m.thunks.ctx,
                        len: m.thunks.len,
                        iter_init: m.thunks.iter_init,
                        iter_next: m.thunks.iter_next,
                        iter_dealloc: m.thunks.iter_dealloc,
                        key_entry_offset: key.entry,
                        key_prog_index: key.prog_index,
                        value_entry_offset: value.entry,
                        value_prog_index: value.prog_index,
                    });
                    self.map_fixups.push(MapFixup { prog_index, slot, mapinfo });
                }
                MemOp::SkipWire(_) | MemOp::Default(_) => {
                    panic!("phon-jit: compat skip/default are interpreter-only for now")
                }
                MemOp::Result(_) => panic!("phon-jit: Result is interpreter-only for now"),
                MemOp::Dynamic { .. } => panic!("phon-jit: dynamic Value is interpreter-only for now"),
                MemOp::Opaque(_) => panic!("phon-jit: opaque fields are interpreter-only for now"),
            }
        }
        let done_start = self.code.len();
        self.code.extend_from_slice(DONE_ENC);

        for (i, &op_start) in starts.iter().enumerate() {
            let next = starts.get(i + 1).copied().unwrap_or(done_start);
            let relocs = match &program[i] {
                MemOp::Scalar { .. } => SCALAR_ENC_CONT,
                MemOp::Sequence(_) => SEQUENCE_ENC_CONT,
                MemOp::Bytes(_) | MemOp::Borrow(_) => BYTES_ENC_CONT,
                MemOp::Option(_) => OPTION_ENC_CONT,
                MemOp::Enum(_) => ENUM_ENC_CONT,
                MemOp::Map(_) => MAP_ENC_CONT,
                MemOp::SkipWire(_) | MemOp::Default(_) => {
                    unreachable!("phon-jit: compat skip/default are interpreter-only for now")
                }
                MemOp::Result(_) => panic!("phon-jit: Result is interpreter-only for now"),
                MemOp::Dynamic { .. } => panic!("phon-jit: dynamic Value is interpreter-only for now"),
                MemOp::Opaque(_) => panic!("phon-jit: opaque fields are interpreter-only for now"),
            };
            for &rel in relocs {
                patch_branch26(&mut self.code, op_start + rel, next);
            }
        }

        Chain { entry, prog_index }
    }
}

impl NativeEncode {
    /// Compile a [`MemProgram`] to native encode machine code.
    // r[impl ir.stencils]
    #[must_use]
    pub fn compile(program: &MemProgram) -> NativeEncode {
        let mut c = EncCompiler {
            code: Vec::new(),
            progs: Vec::new(),
            seq_infos: Vec::new(),
            fixups: Vec::new(),
            bytes_infos: Vec::new(),
            bytes_fixups: Vec::new(),
            opt_infos: Vec::new(),
            opt_fixups: Vec::new(),
            map_infos: Vec::new(),
            map_fixups: Vec::new(),
            enum_infos: Vec::new(),
            enum_fixups: Vec::new(),
        };
        let top = c.compile_chain(program);

        let buf = ExecBuf::new(&c.code);
        let base = buf.as_ptr();
        let progs = c.progs;

        let mut seq_infos: Vec<EncSeqInfo> = Vec::with_capacity(c.seq_infos.len());
        for b in &c.seq_infos {
            let element_entry: unsafe extern "C" fn(*mut EncCtx) =
                unsafe { core::mem::transmute(base.add(b.element_entry_offset)) };
            seq_infos.push(EncSeqInfo {
                field_offset: b.field_offset,
                stride: b.stride,
                thunks_ctx: b.thunks_ctx,
                len: b.len,
                data: b.data,
                element_entry,
                // Bound below, once `progs` is owned by `NativeEncode`.
                element_prog: core::ptr::null(),
            });
        }

        // Materialize the `EncOptInfo`s (some-body entry is `ExecBuf`-relative;
        // some-body prog bound below).
        let mut opt_infos: Vec<EncOptInfo> = Vec::with_capacity(c.opt_infos.len());
        for b in &c.opt_infos {
            let some_entry: unsafe extern "C" fn(*mut EncCtx) =
                unsafe { core::mem::transmute(base.add(b.some_entry_offset)) };
            opt_infos.push(EncOptInfo {
                field_offset: b.field_offset,
                thunks_ctx: b.thunks_ctx,
                is_some: b.is_some,
                get_value: b.get_value,
                some_entry,
                some_prog: core::ptr::null(),
            });
        }

        // Materialize the `EncMapInfo`s (key/value sub-chain entries are
        // `ExecBuf`-relative; the progs are bound below).
        let mut map_infos: Vec<EncMapInfo> = Vec::with_capacity(c.map_infos.len());
        for b in &c.map_infos {
            let key_entry: unsafe extern "C" fn(*mut EncCtx) =
                unsafe { core::mem::transmute(base.add(b.key_entry_offset)) };
            let value_entry: unsafe extern "C" fn(*mut EncCtx) =
                unsafe { core::mem::transmute(base.add(b.value_entry_offset)) };
            map_infos.push(EncMapInfo {
                field_offset: b.field_offset,
                thunks_ctx: b.thunks_ctx,
                len: b.len,
                iter_init: b.iter_init,
                iter_next: b.iter_next,
                iter_dealloc: b.iter_dealloc,
                key_entry,
                key_prog: core::ptr::null(),
                value_entry,
                value_prog: core::ptr::null(),
            });
        }

        // Materialize each enum's variant table (payload entries `ExecBuf`-relative;
        // payload progs bound below).
        let mut enum_variants: Vec<Vec<EncEnumVariantInfo>> =
            Vec::with_capacity(c.enum_infos.len());
        for e in &c.enum_infos {
            let mut variants: Vec<EncEnumVariantInfo> = Vec::with_capacity(e.variants.len());
            for v in &e.variants {
                let payload_entry: unsafe extern "C" fn(*mut EncCtx) =
                    unsafe { core::mem::transmute(base.add(v.payload_entry_offset)) };
                variants.push(EncEnumVariantInfo {
                    wire_index: v.wire_index,
                    selector: v.selector,
                    payload_entry,
                    payload_prog: core::ptr::null(),
                });
            }
            enum_variants.push(variants);
        }
        let mut enum_infos: Vec<EncEnumInfo> = Vec::with_capacity(c.enum_infos.len());
        for e in &c.enum_infos {
            enum_infos.push(EncEnumInfo {
                tag_offset: e.tag_offset,
                tag_width: e.tag_width,
                variants: core::ptr::null(),
                variant_count: e.variants.len(),
            });
        }

        let mut ne = NativeEncode {
            buf,
            entry_prog: top.prog_index,
            progs,
            seq_infos,
            // Move the byte-run infos into their final home; no further binding.
            bytes_infos: c.bytes_infos,
            opt_infos,
            map_infos,
            enum_infos,
            enum_variants,
            last_size: AtomicUsize::new(0),
        };

        for (b, info) in c.seq_infos.iter().zip(ne.seq_infos.iter_mut()) {
            info.element_prog = ne.progs[b.element_prog_index].as_ptr();
        }
        for f in &c.fixups {
            let ptr: *const EncSeqInfo = &ne.seq_infos[f.seqinfo];
            ne.progs[f.prog_index][f.slot] = ptr as u64;
        }
        // Bind each bulk byte-run's prog slot to its `EncBytesInfo` in `ne`.
        for f in &c.bytes_fixups {
            let ptr: *const EncBytesInfo = &ne.bytes_infos[f.bytesinfo];
            ne.progs[f.prog_index][f.slot] = ptr as u64;
        }
        // Bind each option's some-body prog and its prog slot to the `EncOptInfo`.
        for (b, info) in c.opt_infos.iter().zip(ne.opt_infos.iter_mut()) {
            info.some_prog = ne.progs[b.some_prog_index].as_ptr();
        }
        for f in &c.opt_fixups {
            let ptr: *const EncOptInfo = &ne.opt_infos[f.optinfo];
            ne.progs[f.prog_index][f.slot] = ptr as u64;
        }
        // Bind each map's key and value sub-chain progs, then its prog slot to the
        // `EncMapInfo` (two sub-chains, like the decode side).
        for (b, info) in c.map_infos.iter().zip(ne.map_infos.iter_mut()) {
            info.key_prog = ne.progs[b.key_prog_index].as_ptr();
            info.value_prog = ne.progs[b.value_prog_index].as_ptr();
        }
        for f in &c.map_fixups {
            let ptr: *const EncMapInfo = &ne.map_infos[f.mapinfo];
            ne.progs[f.prog_index][f.slot] = ptr as u64;
        }
        // Bind each enum variant's payload prog, point each `EncEnumInfo` at its
        // (now stable) variant table, then fill each enum's prog slot.
        for (eb, variants) in c.enum_infos.iter().zip(ne.enum_variants.iter_mut()) {
            for (vb, vi) in eb.variants.iter().zip(variants.iter_mut()) {
                vi.payload_prog = ne.progs[vb.payload_prog_index].as_ptr();
            }
        }
        let variant_ptrs: Vec<*const EncEnumVariantInfo> =
            ne.enum_variants.iter().map(|v| v.as_ptr()).collect();
        for (info, &ptr) in ne.enum_infos.iter_mut().zip(variant_ptrs.iter()) {
            info.variants = ptr;
        }
        for f in &c.enum_fixups {
            let ptr: *const EncEnumInfo = &ne.enum_infos[f.enuminfo];
            ne.progs[f.prog_index][f.slot] = ptr as u64;
        }

        ne
    }

    /// Encode the value at `base` into compact bytes.
    ///
    /// # Safety
    /// `base` must point to an initialized value matching the descriptor this
    /// program was lowered from, readable for every `offset + size` the program
    /// touches.
    #[must_use]
    pub unsafe fn run(&self, base: *const u8) -> Vec<u8> {
        // The engine owns the output `Vec`; the stencils write into its buffer and
        // grow it through `jit_grow`. The `Vec` carries length 0 throughout (the
        // live length is `ctx.out_pos`); we set its final length at the end.
        // Pre-reserve to the previous output size so a steady stream of
        // similar-sized values writes with zero buffer grows after the first call.
        let mut out: Vec<u8> = Vec::with_capacity(self.last_size.load(Ordering::Relaxed));
        let mut ctx = EncCtx {
            base,
            out_handle: core::ptr::from_mut(&mut out).cast::<u8>(),
            out_ptr: out.as_mut_ptr(),
            out_pos: 0,
            out_cap: out.capacity(),
            prog: self.progs[self.entry_prog].as_ptr(),
            grow: jit_grow,
        };
        let entry: extern "C" fn(*mut EncCtx) = unsafe { core::mem::transmute(self.buf.as_ptr()) };
        entry(&mut ctx);

        // Adopt the written bytes: the buffer holds `out_pos` initialized bytes
        // (the stencils only ever wrote within the capacity `jit_grow` ensured).
        unsafe { out.set_len(ctx.out_pos) };
        self.last_size.fetch_max(ctx.out_pos, Ordering::Relaxed);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lower::compile_decode;

    #[test]
    fn runs_compiler_emitted_machine_code() {
        assert_eq!(smoke(14), 43);
        assert_eq!(smoke(0), 1);
        assert_eq!(smoke(100), 301);
        assert_eq!(smoke(-1), -2);
    }

    /// JIT-compile a scalar program to native code and check it against the
    /// threaded executor (the oracle) and the known layout.
    #[test]
    fn jit_decode_matches_threaded() {
        // { u32 @ 0, u64 @ 8 }. Wire: u32, pad 4, u64.
        let program: MemProgram = vec![
            MemOp::Scalar { offset: 0, size: 4, align: 4 },
            MemOp::Scalar { offset: 8, size: 8, align: 8 },
        ];
        let mut wire = Vec::new();
        wire.extend_from_slice(&0x1122_3344u32.to_le_bytes());
        wire.extend_from_slice(&[0, 0, 0, 0]);
        wire.extend_from_slice(&0xAABB_CCDD_EEFF_0011u64.to_le_bytes());

        // Oracle: the portable threaded executor.
        let mut expected = [0u8; 16];
        unsafe { compile_decode(&program).run(&wire, expected.as_mut_ptr()) }.unwrap();

        // JIT: copied stencils with patched continuations, run from MAP_JIT.
        let jit = NativeDecode::compile(&program);
        let mut got = [0u8; 16];
        unsafe { jit.run(&wire, got.as_mut_ptr()) }.unwrap();

        assert_eq!(got, expected, "JIT disagreed with the threaded executor");
        assert_eq!(&got[0..4], &0x1122_3344u32.to_le_bytes());
        assert_eq!(&got[8..16], &0xAABB_CCDD_EEFF_0011u64.to_le_bytes());
    }

    /// A wider program exercising every fixed scalar width and reordered offsets.
    #[test]
    fn jit_decode_many_widths() {
        let program: MemProgram = vec![
            MemOp::Scalar { offset: 16, size: 1, align: 1 },
            MemOp::Scalar { offset: 0, size: 16, align: 16 },
            MemOp::Scalar { offset: 18, size: 2, align: 2 },
            MemOp::Scalar { offset: 20, size: 4, align: 4 },
        ];
        // Wire order is program order: u8 @0, pad to 16, u128, u16, pad 2, u32.
        let mut wire = vec![0xEE];
        wire.resize(16, 0); // u8 then pad to the u128's 16-byte alignment
        wire.extend_from_slice(&0x0011_2233_4455_6677_8899_AABB_CCDD_EEFFu128.to_le_bytes());
        wire.extend_from_slice(&0x1234u16.to_le_bytes()); // u16 at 32
        wire.extend_from_slice(&[0, 0]); // pad 34 -> 36 for the u32's alignment
        wire.extend_from_slice(&0xCAFE_F00Du32.to_le_bytes()); // u32 at 36

        let mut expected = [0u8; 24];
        unsafe { compile_decode(&program).run(&wire, expected.as_mut_ptr()) }.unwrap();
        let jit = NativeDecode::compile(&program);
        let mut got = [0u8; 24];
        unsafe { jit.run(&wire, got.as_mut_ptr()) }.unwrap();
        assert_eq!(got, expected);
    }

    #[test]
    fn jit_decode_rejects_trailing() {
        let program: MemProgram = vec![MemOp::Scalar { offset: 0, size: 4, align: 4 }];
        let jit = NativeDecode::compile(&program);
        let mut out = [0u8; 4];
        let err = unsafe { jit.run(&[1, 2, 3, 4, 5], out.as_mut_ptr()) }.unwrap_err();
        assert!(matches!(err, DecodeError::TrailingBytes(1)));
    }

    // ====================================================================
    // Owned-sequence decode
    // ====================================================================

    use core::mem::MaybeUninit;
    use phon_ir::ir::{BytesOp, SeqOp};
    use phon_ir::SeqThunks;

    // Hand-written list thunks for `Vec<u32>`, copied from
    // `phon-engine::typed`'s test: the engine allocates the buffer, then
    // `from_raw_parts` adopts it. The JIT calls these exactly as the
    // interpreter does.
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

    // Thunks for the outer `Vec<Vec<u32>>` in the nested test.
    unsafe extern "C" fn vvu32_from_raw_parts(
        _ctx: *const (),
        list: *mut u8,
        ptr: *mut u8,
        len: usize,
        cap: usize,
    ) {
        let v = unsafe { Vec::<Vec<u32>>::from_raw_parts(ptr.cast::<Vec<u32>>(), len, cap) };
        unsafe { core::ptr::write(list.cast::<Vec<Vec<u32>>>(), v) };
    }
    unsafe extern "C" fn vvu32_len(_ctx: *const (), list: *const u8) -> usize {
        unsafe { (*list.cast::<Vec<Vec<u32>>>()).len() }
    }
    unsafe extern "C" fn vvu32_data(_ctx: *const (), list: *const u8) -> *const u8 {
        unsafe { (*list.cast::<Vec<Vec<u32>>>()).as_ptr().cast::<u8>() }
    }
    fn vvu32_thunks() -> SeqThunks {
        SeqThunks {
            ctx: core::ptr::null(),
            from_raw_parts: vvu32_from_raw_parts,
            len: vvu32_len,
            data: vvu32_data,
        }
    }

    /// A root program of a single owned `Vec<u32>` sequence.
    fn vu32_program() -> MemProgram {
        vec![MemOp::Sequence(Box::new(SeqOp {
            field_offset: 0,
            element: vec![MemOp::Scalar { offset: 0, size: 4, align: 4 }],
            stride: 4,
            elem_align: 4,
            min_wire: 1,
            thunks: vu32_thunks(),
        }))]
    }

    /// Build the wire bytes for a `Vec<u32>`: a `u32` count then each element.
    fn vu32_wire(values: &[u32]) -> Vec<u8> {
        let mut wire = Vec::new();
        wire.extend_from_slice(&(values.len() as u32).to_le_bytes());
        for &v in values {
            wire.extend_from_slice(&v.to_le_bytes());
        }
        wire
    }

    /// JIT-decode a `Vec<u32>` sequence (the first stencil with control flow) and
    /// confirm the reconstructed `Vec` equals the expected values.
    #[test]
    fn jit_decode_sequence_vec_u32() {
        let program = vu32_program();
        let values = [1u32, 2, 999, 0xDEAD_BEEF];
        let wire = vu32_wire(&values);

        let jit = NativeDecode::compile(&program);
        // The handle is the engine-owned `Vec<u32>`; decode fills it in place.
        let mut slot = MaybeUninit::<Vec<u32>>::uninit();
        unsafe { jit.run(&wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back, values.to_vec());
    }

    /// An empty sequence: count 0, no allocation, the `Vec` is empty.
    #[test]
    fn jit_decode_sequence_empty() {
        let program = vu32_program();
        let wire = vu32_wire(&[]);
        let jit = NativeDecode::compile(&program);
        let mut slot = MaybeUninit::<Vec<u32>>::uninit();
        unsafe { jit.run(&wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert!(back.is_empty());
    }

    /// A hostile count larger than the remaining bytes can supply must be
    /// rejected (the `read_len` bounds check), not allocated.
    #[test]
    fn jit_decode_sequence_rejects_huge_count() {
        let program = vu32_program();
        // Claim 1_000_000 elements but supply only one element's bytes.
        let mut wire = 1_000_000u32.to_le_bytes().to_vec();
        wire.extend_from_slice(&7u32.to_le_bytes());
        let jit = NativeDecode::compile(&program);
        let mut slot = MaybeUninit::<Vec<u32>>::uninit();
        let err = unsafe { jit.run(&wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap_err();
        assert!(matches!(err, DecodeError::UnexpectedEof { .. }));
    }

    /// A sequence with a non-trivial element body (a two-scalar record per
    /// element): `Vec<u32>` where each "element" is the low then high half of a
    /// value reassembled by two adjacent scalar copies.
    #[test]
    fn jit_decode_sequence_multi_scalar_element() {
        // Element program: two u32 scalars at offsets 0 and 4 (stride 8).
        let program: MemProgram = vec![MemOp::Sequence(Box::new(SeqOp {
            field_offset: 0,
            element: vec![
                MemOp::Scalar { offset: 0, size: 4, align: 4 },
                MemOp::Scalar { offset: 4, size: 4, align: 4 },
            ],
            stride: 8,
            elem_align: 4,
            min_wire: 1,
            thunks: vu64_thunks(),
        }))];
        let pairs: [(u32, u32); 3] = [(1, 2), (0xAAAA, 0xBBBB), (7, 0)];
        let mut wire = (pairs.len() as u32).to_le_bytes().to_vec();
        for (a, b) in pairs {
            wire.extend_from_slice(&a.to_le_bytes());
            wire.extend_from_slice(&b.to_le_bytes());
        }
        let jit = NativeDecode::compile(&program);
        let mut slot = MaybeUninit::<Vec<u64>>::uninit();
        unsafe { jit.run(&wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        // Each element is two LE u32 halves contiguous = one LE u64.
        let expected: Vec<u64> = pairs.iter().map(|&(a, b)| (a as u64) | ((b as u64) << 32)).collect();
        assert_eq!(back, expected);
    }

    // Thunks for a `Vec<u64>` (used by the multi-scalar element test, where two
    // u32 copies fill an 8-byte slot).
    unsafe extern "C" fn vu64_from_raw_parts(
        _ctx: *const (),
        list: *mut u8,
        ptr: *mut u8,
        len: usize,
        cap: usize,
    ) {
        let v = unsafe { Vec::<u64>::from_raw_parts(ptr.cast::<u64>(), len, cap) };
        unsafe { core::ptr::write(list.cast::<Vec<u64>>(), v) };
    }
    unsafe extern "C" fn vu64_len(_ctx: *const (), list: *const u8) -> usize {
        unsafe { (*list.cast::<Vec<u64>>()).len() }
    }
    unsafe extern "C" fn vu64_data(_ctx: *const (), list: *const u8) -> *const u8 {
        unsafe { (*list.cast::<Vec<u64>>()).as_ptr().cast::<u8>() }
    }
    fn vu64_thunks() -> SeqThunks {
        SeqThunks {
            ctx: core::ptr::null(),
            from_raw_parts: vu64_from_raw_parts,
            len: vu64_len,
            data: vu64_data,
        }
    }

    /// A nested sequence `Vec<Vec<u32>>`: the outer sequence's element body is
    /// itself a sequence stencil, exercising the recursive call-program layout.
    #[test]
    fn jit_decode_nested_sequence() {
        let inner = SeqOp {
            field_offset: 0,
            element: vec![MemOp::Scalar { offset: 0, size: 4, align: 4 }],
            stride: 4,
            elem_align: 4,
            min_wire: 1,
            thunks: vu32_thunks(),
        };
        let program: MemProgram = vec![MemOp::Sequence(Box::new(SeqOp {
            field_offset: 0,
            element: vec![MemOp::Sequence(Box::new(inner))],
            stride: core::mem::size_of::<Vec<u32>>(),
            elem_align: core::mem::align_of::<Vec<u32>>(),
            min_wire: 1,
            thunks: vvu32_thunks(),
        }))];

        let rows: [&[u32]; 3] = [&[1, 2, 3], &[], &[42]];
        let mut wire = (rows.len() as u32).to_le_bytes().to_vec();
        for row in rows {
            wire.extend_from_slice(&(row.len() as u32).to_le_bytes());
            for &v in row {
                wire.extend_from_slice(&v.to_le_bytes());
            }
        }

        let jit = NativeDecode::compile(&program);
        let mut slot = MaybeUninit::<Vec<Vec<u32>>>::uninit();
        unsafe { jit.run(&wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        let expected: Vec<Vec<u32>> = rows.iter().map(|r| r.to_vec()).collect();
        assert_eq!(back, expected);
    }

    // ====================================================================
    // Encode
    // ====================================================================

    use crate::lower::compile_encode;

    /// JIT-encode a fixed-scalar program and check it against the threaded encoder
    /// (the oracle) and the known wire layout: `{ u32 @ 0, u64 @ 8 }` -> u32, pad
    /// 4, u64.
    #[test]
    fn jit_encode_matches_threaded() {
        let program: MemProgram = vec![
            MemOp::Scalar { offset: 0, size: 4, align: 4 },
            MemOp::Scalar { offset: 8, size: 8, align: 8 },
        ];
        #[repr(C, align(8))]
        struct Mem([u8; 16]);
        let mut mem = Mem([0; 16]);
        mem.0[0..4].copy_from_slice(&0x1122_3344u32.to_le_bytes());
        mem.0[8..16].copy_from_slice(&0xAABB_CCDD_EEFF_0011u64.to_le_bytes());

        // Oracle: the portable threaded encoder.
        let expected = unsafe { compile_encode(&program).run(mem.0.as_ptr()) };

        // JIT: copied encode stencils, run from MAP_JIT.
        let jit = NativeEncode::compile(&program);
        let got = unsafe { jit.run(mem.0.as_ptr()) };

        assert_eq!(got, expected, "JIT disagreed with the threaded encoder");
        // u32 (4) + pad (4) + u64 (8) = 16 wire bytes, byte-for-byte.
        assert_eq!(got.len(), 16);
        assert_eq!(&got[0..4], &0x1122_3344u32.to_le_bytes());
        assert_eq!(&got[4..8], &[0, 0, 0, 0]);
        assert_eq!(&got[8..16], &0xAABB_CCDD_EEFF_0011u64.to_le_bytes());
    }

    /// A wider scalar program (every fixed width, reordered offsets), encoded and
    /// round-tripped back through [`NativeDecode`].
    #[test]
    fn jit_encode_many_widths_roundtrips() {
        let program: MemProgram = vec![
            MemOp::Scalar { offset: 16, size: 1, align: 1 },
            MemOp::Scalar { offset: 0, size: 16, align: 16 },
            MemOp::Scalar { offset: 18, size: 2, align: 2 },
            MemOp::Scalar { offset: 20, size: 4, align: 4 },
        ];
        #[repr(C, align(16))]
        struct Mem([u8; 24]);
        let mut mem = Mem([0; 24]);
        mem.0[16] = 0xEE;
        mem.0[0..16]
            .copy_from_slice(&0x0011_2233_4455_6677_8899_AABB_CCDD_EEFFu128.to_le_bytes());
        mem.0[18..20].copy_from_slice(&0x1234u16.to_le_bytes());
        mem.0[20..24].copy_from_slice(&0xCAFE_F00Du32.to_le_bytes());

        let expected = unsafe { compile_encode(&program).run(mem.0.as_ptr()) };
        let jit = NativeEncode::compile(&program);
        let got = unsafe { jit.run(mem.0.as_ptr()) };
        assert_eq!(got, expected);

        // Round-trip: decode the JIT-encoded bytes back and compare memory.
        let dec = NativeDecode::compile(&program);
        let mut back = Mem([0; 24]);
        unsafe { dec.run(&got, back.0.as_mut_ptr()) }.unwrap();
        assert_eq!(back.0, mem.0);
    }

    /// JIT-encode a single owned `Vec<u32>` sequence and check the wire bytes (a
    /// `u32` count then each element) and a `NativeDecode` round-trip.
    #[test]
    fn jit_encode_sequence_vec_u32() {
        let program = vu32_program();
        let values = vec![1u32, 2, 999, 0xDEAD_BEEF];

        // The handle is the engine-owned `Vec<u32>`; encode reads it in place.
        let got = unsafe { jit_encode_vec(&program, &values) };
        assert_eq!(got, vu32_wire(&values));

        // Oracle agreement with the threaded encoder.
        let expected =
            unsafe { compile_encode(&program).run(core::ptr::from_ref(&values).cast::<u8>()) };
        assert_eq!(got, expected);

        // Round-trip back into a fresh `Vec<u32>`.
        let dec = NativeDecode::compile(&program);
        let mut slot = MaybeUninit::<Vec<u32>>::uninit();
        unsafe { dec.run(&got, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back, values);
    }

    /// An empty sequence: count 0, no elements.
    #[test]
    fn jit_encode_sequence_empty() {
        let program = vu32_program();
        let values: Vec<u32> = Vec::new();
        let got = unsafe { jit_encode_vec(&program, &values) };
        assert_eq!(got, vu32_wire(&values));
        assert_eq!(got, 0u32.to_le_bytes().to_vec());
    }

    /// A struct with multiple scalars and a trailing `Vec<u32>`:
    /// `{ u64 @ 0, u32 @ 8, Vec<u32> @ 16 }`. Exercises scalars + a sequence in
    /// one program, plus a full `NativeDecode` round-trip.
    #[test]
    fn jit_encode_struct_with_sequence() {
        // The in-memory struct: a u64, a u32 (padded to 16 by the Vec's align),
        // then a `Vec<u32>` handle.
        #[repr(C)]
        struct S {
            a: u64,
            b: u32,
            v: Vec<u32>,
        }
        let v_off = core::mem::offset_of!(S, v);
        let program: MemProgram = vec![
            MemOp::Scalar { offset: core::mem::offset_of!(S, a), size: 8, align: 8 },
            MemOp::Scalar { offset: core::mem::offset_of!(S, b), size: 4, align: 4 },
            MemOp::Sequence(Box::new(SeqOp {
                field_offset: v_off,
                element: vec![MemOp::Scalar { offset: 0, size: 4, align: 4 }],
                stride: 4,
                elem_align: 4,
                min_wire: 1,
                thunks: vu32_thunks(),
            })),
        ];

        let s = S {
            a: 0xAABB_CCDD_EEFF_0011,
            b: 0x1234_5678,
            v: vec![7u32, 8, 9],
        };
        let base = core::ptr::from_ref(&s).cast::<u8>();

        let jit = NativeEncode::compile(&program);
        let got = unsafe { jit.run(base) };

        // Oracle: the threaded encoder over the same memory.
        let expected = unsafe { compile_encode(&program).run(base) };
        assert_eq!(got, expected, "JIT disagreed with the threaded encoder");

        // Known wire layout: u64, u32 (no pad — already 8-aligned), then the
        // sequence (u32 count + elements).
        let mut want = Vec::new();
        want.extend_from_slice(&s.a.to_le_bytes());
        want.extend_from_slice(&s.b.to_le_bytes());
        want.extend_from_slice(&(s.v.len() as u32).to_le_bytes());
        for &x in &s.v {
            want.extend_from_slice(&x.to_le_bytes());
        }
        assert_eq!(got, want);

        // Round-trip: decode back into a fresh struct image and compare fields.
        let dec = NativeDecode::compile(&program);
        let mut slot = MaybeUninit::<S>::uninit();
        unsafe { dec.run(&got, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back.a, s.a);
        assert_eq!(back.b, s.b);
        assert_eq!(back.v, s.v);
    }

    /// A large sequence to force the output buffer to grow several times across
    /// element encodes (exercising `jit_grow` and the ptr/cap re-read).
    #[test]
    fn jit_encode_sequence_grows() {
        let program = vu32_program();
        let values: Vec<u32> = (0..5000u32).collect();
        let got = unsafe { jit_encode_vec(&program, &values) };
        assert_eq!(got, vu32_wire(&values));

        let dec = NativeDecode::compile(&program);
        let mut slot = MaybeUninit::<Vec<u32>>::uninit();
        unsafe { dec.run(&got, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back, values);
    }

    /// A nested sequence `Vec<Vec<u32>>` encoded and round-tripped, exercising the
    /// recursive element call-program on the encode side.
    #[test]
    fn jit_encode_nested_sequence_roundtrips() {
        let inner = SeqOp {
            field_offset: 0,
            element: vec![MemOp::Scalar { offset: 0, size: 4, align: 4 }],
            stride: 4,
            elem_align: 4,
            min_wire: 1,
            thunks: vu32_thunks(),
        };
        let program: MemProgram = vec![MemOp::Sequence(Box::new(SeqOp {
            field_offset: 0,
            element: vec![MemOp::Sequence(Box::new(inner))],
            stride: core::mem::size_of::<Vec<u32>>(),
            elem_align: core::mem::align_of::<Vec<u32>>(),
            min_wire: 1,
            thunks: vvu32_thunks(),
        }))];

        let value: Vec<Vec<u32>> = vec![vec![1, 2, 3], vec![], vec![42]];
        let base = core::ptr::from_ref(&value).cast::<u8>();

        let jit = NativeEncode::compile(&program);
        let got = unsafe { jit.run(base) };
        let expected = unsafe { compile_encode(&program).run(base) };
        assert_eq!(got, expected);

        let dec = NativeDecode::compile(&program);
        let mut slot = MaybeUninit::<Vec<Vec<u32>>>::uninit();
        unsafe { dec.run(&got, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back, value);
    }

    /// JIT-encode a program whose root is a single `Vec<u32>` sequence, reading
    /// the `Vec` handle directly as the base.
    unsafe fn jit_encode_vec(program: &MemProgram, values: &Vec<u32>) -> Vec<u8> {
        let jit = NativeEncode::compile(program);
        unsafe { jit.run(core::ptr::from_ref(values).cast::<u8>()) }
    }

    // ====================================================================
    // Bulk byte-run (MemOp::Bytes, non-UTF-8) decode + encode
    // ====================================================================

    /// A root program of a single bulk byte-run representing a `Vec<u32>`: stride
    /// 4, elem_align 4, non-UTF-8. The same `Vec<u32>` thunks the sequence tests
    /// use — `from_raw_parts` adopts a buffer, `len`/`data` read it.
    fn vu32_bytes_program() -> MemProgram {
        vec![MemOp::Bytes(Box::new(BytesOp {
            field_offset: 0,
            stride: 4,
            elem_align: 4,
            validate: validate_any,
            thunks: vu32_thunks(),
        }))]
    }

    /// The wire bytes for a bulk `Vec<u32>` run: a `u32` count then `count * 4`
    /// contiguous bytes (no padding here — count ends 4-aligned, elem_align is 4).
    fn vu32_bytes_wire(values: &[u32]) -> Vec<u8> {
        let mut wire = (values.len() as u32).to_le_bytes().to_vec();
        for &v in values {
            wire.extend_from_slice(&v.to_le_bytes());
        }
        wire
    }

    /// JIT-decode a bulk byte run into a `Vec<u32>` and confirm the reconstructed
    /// `Vec` equals the expected values (one block copy, no per-element loop).
    #[test]
    fn jit_decode_bytes_vec_u32() {
        let program = vu32_bytes_program();
        let values = [1u32, 2, 999, 0xDEAD_BEEF, 0x0102_0304];
        let wire = vu32_bytes_wire(&values);

        let jit = NativeDecode::compile(&program);
        let mut slot = MaybeUninit::<Vec<u32>>::uninit();
        unsafe { jit.run(&wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back, values.to_vec());
    }

    /// The empty case: count 0, no allocation, an empty `Vec`.
    #[test]
    fn jit_decode_bytes_empty() {
        let program = vu32_bytes_program();
        let wire = vu32_bytes_wire(&[]);
        let jit = NativeDecode::compile(&program);
        let mut slot = MaybeUninit::<Vec<u32>>::uninit();
        unsafe { jit.run(&wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert!(back.is_empty());
    }

    /// A multi-KB run, exercising the word-wise bulk copy across many words and a
    /// tail. Decode and check, then re-encode byte-identically.
    #[test]
    fn jit_decode_bytes_large() {
        let program = vu32_bytes_program();
        let values: Vec<u32> = (0..4096u32).map(|i| i.wrapping_mul(0x9E37_79B9)).collect();
        let wire = vu32_bytes_wire(&values);

        let jit = NativeDecode::compile(&program);
        let mut slot = MaybeUninit::<Vec<u32>>::uninit();
        unsafe { jit.run(&wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back, values);
    }

    /// A hostile count larger than the remaining bytes can supply must be rejected
    /// (the count bounds check), not allocated.
    #[test]
    fn jit_decode_bytes_rejects_huge_count() {
        let program = vu32_bytes_program();
        // Claim 1_000_000 elements but supply only one element's bytes.
        let mut wire = 1_000_000u32.to_le_bytes().to_vec();
        wire.extend_from_slice(&7u32.to_le_bytes());
        let jit = NativeDecode::compile(&program);
        let mut slot = MaybeUninit::<Vec<u32>>::uninit();
        let err = unsafe { jit.run(&wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap_err();
        assert!(matches!(err, DecodeError::UnexpectedEof { .. }));
    }

    /// JIT-encode a `Vec<u32>` bulk byte run and confirm byte-identical wire, plus
    /// a `NativeDecode` round-trip.
    #[test]
    fn jit_encode_bytes_vec_u32() {
        let program = vu32_bytes_program();
        let values = vec![1u32, 2, 999, 0xDEAD_BEEF, 0x0102_0304];

        let jit = NativeEncode::compile(&program);
        let got = unsafe { jit.run(core::ptr::from_ref(&values).cast::<u8>()) };
        assert_eq!(got, vu32_bytes_wire(&values));

        // Round-trip back into a fresh `Vec<u32>`.
        let dec = NativeDecode::compile(&program);
        let mut slot = MaybeUninit::<Vec<u32>>::uninit();
        unsafe { dec.run(&got, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back, values);
    }

    /// The empty encode case: just a zero `u32` count.
    #[test]
    fn jit_encode_bytes_empty() {
        let program = vu32_bytes_program();
        let values: Vec<u32> = Vec::new();
        let jit = NativeEncode::compile(&program);
        let got = unsafe { jit.run(core::ptr::from_ref(&values).cast::<u8>()) };
        assert_eq!(got, 0u32.to_le_bytes().to_vec());
    }

    /// A large (multi-KB) round-trip: encode then decode, byte-identical wire and
    /// equal values, forcing the output buffer to grow under the bulk copy.
    #[test]
    fn jit_encode_bytes_large_roundtrips() {
        let program = vu32_bytes_program();
        let values: Vec<u32> = (0..4096u32).map(|i| i.wrapping_mul(0x85EB_CA77)).collect();

        let jit = NativeEncode::compile(&program);
        let got = unsafe { jit.run(core::ptr::from_ref(&values).cast::<u8>()) };
        assert_eq!(got, vu32_bytes_wire(&values));

        let dec = NativeDecode::compile(&program);
        let mut slot = MaybeUninit::<Vec<u32>>::uninit();
        unsafe { dec.run(&got, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back, values);
    }

    /// A bulk byte run embedded after scalars, exercising wire alignment padding
    /// before the run (`{ u8 @ 0, Vec<u32> @ 8 }` → u8, pad to the count, count,
    /// then the run padded to elem_align 4).
    #[test]
    fn jit_bytes_after_scalar_roundtrips() {
        #[repr(C)]
        struct S {
            tag: u8,
            v: Vec<u32>,
        }
        let v_off = core::mem::offset_of!(S, v);
        let program: MemProgram = vec![
            MemOp::Scalar { offset: core::mem::offset_of!(S, tag), size: 1, align: 1 },
            MemOp::Bytes(Box::new(BytesOp {
                field_offset: v_off,
                stride: 4,
                elem_align: 4,
                validate: validate_any,
                thunks: vu32_thunks(),
            })),
        ];

        let s = S { tag: 0xAB, v: vec![10u32, 20, 30] };
        let base = core::ptr::from_ref(&s).cast::<u8>();

        let enc = NativeEncode::compile(&program);
        let got = unsafe { enc.run(base) };

        // Known wire: u8 tag, then the count u32 (no pad — count starts at offset 1
        // and `write_u32` is unaligned), then the run padded to elem_align 4.
        let mut want = vec![0xABu8];
        want.extend_from_slice(&(s.v.len() as u32).to_le_bytes());
        // After 1 + 4 = 5 bytes, pad to a multiple of 4 -> 3 pad bytes to reach 8.
        want.extend_from_slice(&[0, 0, 0]);
        for &x in &s.v {
            want.extend_from_slice(&x.to_le_bytes());
        }
        assert_eq!(got, want);

        // Round-trip back into a fresh struct image and compare fields.
        let dec = NativeDecode::compile(&program);
        let mut slot = MaybeUninit::<S>::uninit();
        unsafe { dec.run(&got, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back.tag, s.tag);
        assert_eq!(back.v, s.v);
    }

    // ====================================================================
    // String: a UTF-8-validated bulk byte run (stride 1) in the JIT
    // ====================================================================

    /// Validator for `Vec` runs: any bytes are valid.
    unsafe extern "C" fn validate_any(_ptr: *const u8, _len: usize) -> bool {
        true
    }

    /// Validator for `String` runs: the bytes must be UTF-8.
    unsafe extern "C" fn validate_utf8(ptr: *const u8, len: usize) -> bool {
        let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
        core::str::from_utf8(bytes).is_ok()
    }

    /// Adopt an engine-allocated, already-validated buffer into the `String`.
    unsafe extern "C" fn str_from_raw_parts(
        _ctx: *const (),
        list: *mut u8,
        ptr: *mut u8,
        len: usize,
        cap: usize,
    ) {
        let s = unsafe { String::from_raw_parts(ptr, len, cap) };
        unsafe { core::ptr::write(list.cast::<String>(), s) };
    }
    unsafe extern "C" fn str_len(_ctx: *const (), list: *const u8) -> usize {
        let s: &String = unsafe { &*list.cast::<String>() };
        s.len()
    }
    unsafe extern "C" fn str_data(_ctx: *const (), list: *const u8) -> *const u8 {
        let s: &String = unsafe { &*list.cast::<String>() };
        s.as_ptr()
    }
    fn str_thunks() -> SeqThunks {
        SeqThunks {
            ctx: core::ptr::null(),
            from_raw_parts: str_from_raw_parts,
            len: str_len,
            data: str_data,
        }
    }

    /// A root program of a single UTF-8-validated `String` run (stride 1).
    fn string_program() -> MemProgram {
        vec![MemOp::Bytes(Box::new(BytesOp {
            field_offset: 0,
            stride: 1,
            elem_align: 1,
            validate: validate_utf8,
            thunks: str_thunks(),
        }))]
    }

    fn string_wire(s: &str) -> Vec<u8> {
        let mut wire = (s.len() as u32).to_le_bytes().to_vec();
        wire.extend_from_slice(s.as_bytes());
        wire
    }

    /// JIT-decode valid UTF-8 `String` runs, including the empty case — the
    /// validator runs in-stencil (indirect call) and accepts them.
    #[test]
    fn jit_decode_bytes_string() {
        let program = string_program();
        let jit = NativeDecode::compile(&program);
        for text in ["héllo wörld 🐝", "", "ascii"] {
            let wire = string_wire(text);
            let mut slot = MaybeUninit::<String>::uninit();
            unsafe { jit.run(&wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
            let back = unsafe { slot.assume_init() };
            assert_eq!(back, text);
        }
    }

    /// JIT-decode invalid UTF-8 → `InvalidUtf8` (the in-stencil validator sets
    /// status 2, distinct from EOF), and nothing is adopted.
    #[test]
    fn jit_decode_bytes_string_rejects_invalid_utf8() {
        let program = string_program();
        let jit = NativeDecode::compile(&program);
        // count 1, one byte 0xFF (not valid UTF-8).
        let mut wire = 1u32.to_le_bytes().to_vec();
        wire.push(0xFF);
        let mut slot = MaybeUninit::<String>::uninit();
        let err = unsafe { jit.run(&wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap_err();
        assert!(matches!(err, DecodeError::InvalidUtf8));
    }

    /// JIT-encode a `String` (encode never validates) and round-trip it.
    #[test]
    fn jit_encode_bytes_string_roundtrips() {
        let program = string_program();
        let s = String::from("héllo 🐝");
        let enc = NativeEncode::compile(&program);
        let got = unsafe { enc.run(core::ptr::from_ref(&s).cast::<u8>()) };
        assert_eq!(got, string_wire(&s));

        let dec = NativeDecode::compile(&program);
        let mut slot = MaybeUninit::<String>::uninit();
        unsafe { dec.run(&got, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back, s);
    }

    // ====================================================================
    // Option: the data-directed presence branch (decode + encode)
    // ====================================================================

    use phon_ir::ir::{EnumOp, EnumVariantOp, OptionOp};
    use phon_ir::OptionThunks;

    // Hand-written `Option<u32>` thunks: a scalar inner (no heap), so `init_some`
    // just copies the four bytes out of scratch.
    unsafe extern "C" fn ou32_is_some(_ctx: *const (), option: *const u8) -> bool {
        unsafe { (*option.cast::<Option<u32>>()).is_some() }
    }
    unsafe extern "C" fn ou32_get_value(_ctx: *const (), option: *const u8) -> *const u8 {
        match unsafe { &*option.cast::<Option<u32>>() } {
            Some(v) => core::ptr::from_ref(v).cast::<u8>(),
            None => core::ptr::null(),
        }
    }
    unsafe extern "C" fn ou32_init_some(_ctx: *const (), option: *mut u8, value: *mut u8) {
        let v = unsafe { core::ptr::read(value.cast::<u32>()) };
        unsafe { core::ptr::write(option.cast::<Option<u32>>(), Some(v)) };
    }
    unsafe extern "C" fn ou32_init_none(_ctx: *const (), option: *mut u8) {
        unsafe { core::ptr::write(option.cast::<Option<u32>>(), None) };
    }
    fn ou32_thunks() -> OptionThunks {
        OptionThunks {
            ctx: core::ptr::null(),
            is_some: ou32_is_some,
            get_value: ou32_get_value,
            init_some: ou32_init_some,
            init_none: ou32_init_none,
        }
    }

    /// A root program of a single `Option<u32>`.
    fn ou32_program() -> MemProgram {
        vec![MemOp::Option(Box::new(OptionOp {
            field_offset: 0,
            some: vec![MemOp::Scalar { offset: 0, size: 4, align: 4 }],
            inner_size: 4,
            inner_align: 4,
            thunks: ou32_thunks(),
        }))]
    }

    /// `Option<u32>`: none encodes/decodes a lone `0` byte; some encodes `1` then
    /// the u32. Both directions, both arms, round-tripped.
    #[test]
    fn jit_option_u32_none_and_some_roundtrip() {
        let program = ou32_program();
        let enc = NativeEncode::compile(&program);
        let dec = NativeDecode::compile(&program);

        for val in [None, Some(0xDEAD_BEEFu32), Some(0u32)] {
            let got = unsafe { enc.run(core::ptr::from_ref(&val).cast::<u8>()) };
            // Known wire: presence byte, then (if some) pad-to-4 + the u32.
            let mut want = vec![if val.is_some() { 1u8 } else { 0u8 }];
            if let Some(x) = val {
                want.extend_from_slice(&[0, 0, 0]); // pad after the 1-byte presence
                want.extend_from_slice(&x.to_le_bytes());
            }
            assert_eq!(got, want, "encode mismatch for {val:?}");

            let mut slot = MaybeUninit::<Option<u32>>::uninit();
            unsafe { dec.run(&got, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
            let back = unsafe { slot.assume_init() };
            assert_eq!(back, val, "roundtrip mismatch for {val:?}");
        }
    }

    // `Option<String>` thunks: a heap inner — exercises the decode scratch buffer
    // and the `init_some` move (the `String` is built into scratch, then moved
    // into the `Option`, then the scratch freed without dropping).
    unsafe extern "C" fn ostr_is_some(_ctx: *const (), option: *const u8) -> bool {
        unsafe { (*option.cast::<Option<String>>()).is_some() }
    }
    unsafe extern "C" fn ostr_get_value(_ctx: *const (), option: *const u8) -> *const u8 {
        match unsafe { &*option.cast::<Option<String>>() } {
            Some(v) => core::ptr::from_ref(v).cast::<u8>(),
            None => core::ptr::null(),
        }
    }
    unsafe extern "C" fn ostr_init_some(_ctx: *const (), option: *mut u8, value: *mut u8) {
        let v = unsafe { core::ptr::read(value.cast::<String>()) };
        unsafe { core::ptr::write(option.cast::<Option<String>>(), Some(v)) };
    }
    unsafe extern "C" fn ostr_init_none(_ctx: *const (), option: *mut u8) {
        unsafe { core::ptr::write(option.cast::<Option<String>>(), None) };
    }
    fn ostr_thunks() -> OptionThunks {
        OptionThunks {
            ctx: core::ptr::null(),
            is_some: ostr_is_some,
            get_value: ostr_get_value,
            init_some: ostr_init_some,
            init_none: ostr_init_none,
        }
    }

    /// A root program of a single `Option<String>` (the inner is a UTF-8 byte run).
    fn ostr_program() -> MemProgram {
        vec![MemOp::Option(Box::new(OptionOp {
            field_offset: 0,
            some: vec![MemOp::Bytes(Box::new(BytesOp {
                field_offset: 0,
                stride: 1,
                elem_align: 1,
                validate: validate_utf8,
                thunks: str_thunks(),
            }))],
            inner_size: core::mem::size_of::<String>(),
            inner_align: core::mem::align_of::<String>(),
            thunks: ostr_thunks(),
        }))]
    }

    /// `Option<String>`: the some-arm builds a heap `String` into scratch and moves
    /// it into the option. Both arms, round-tripped, byte-identical wire.
    #[test]
    fn jit_option_string_scratch_move_roundtrip() {
        let program = ostr_program();
        let enc = NativeEncode::compile(&program);
        let dec = NativeDecode::compile(&program);

        for val in [None, Some(String::new()), Some("héllo 🐝 wörld".to_string())] {
            let got = unsafe { enc.run(core::ptr::from_ref(&val).cast::<u8>()) };
            // Known wire: presence byte, then (if some) the String run (u32 len +
            // bytes; no pad needed — len starts at offset 1, elem_align 1).
            let mut want = vec![if val.is_some() { 1u8 } else { 0u8 }];
            if let Some(s) = &val {
                want.extend_from_slice(&(s.len() as u32).to_le_bytes());
                want.extend_from_slice(s.as_bytes());
            }
            assert_eq!(got, want, "encode mismatch for {val:?}");

            let mut slot = MaybeUninit::<Option<String>>::uninit();
            unsafe { dec.run(&got, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
            let back = unsafe { slot.assume_init() };
            assert_eq!(back, val, "roundtrip mismatch for {val:?}");
        }
    }

    /// A presence byte other than 0/1 must reject (`InvalidBool`), never produce a
    /// value.
    #[test]
    fn jit_option_rejects_bad_presence() {
        let program = ou32_program();
        let dec = NativeDecode::compile(&program);
        let mut slot = MaybeUninit::<Option<u32>>::uninit();
        let err = unsafe { dec.run(&[2u8], slot.as_mut_ptr().cast::<u8>()) }.unwrap_err();
        assert!(matches!(err, DecodeError::InvalidBool(2)), "got {err:?}");

        // An empty wire (no presence byte at all) is a truncation, not InvalidBool.
        let mut slot2 = MaybeUninit::<Option<u32>>::uninit();
        let err2 = unsafe { dec.run(&[], slot2.as_mut_ptr().cast::<u8>()) }.unwrap_err();
        assert!(matches!(err2, DecodeError::UnexpectedEof { .. }), "got {err2:?}");
    }

    // ====================================================================
    // Enum: the data-directed variant branch (decode + encode)
    // ====================================================================

    // A `#[repr(u8)]`-style enum with three payload shapes mirroring phon's `Msg`:
    //   Ping        -> unit (no payload)
    //   Echo(u32)   -> single scalar payload
    //   Move{x,y}   -> a two-scalar (struct) payload
    // The discriminant is a u8 at offset 0; the payload fields follow (facet keeps
    // the discriminant first). We model it over a fixed 12-byte image so x/y have
    // room at offsets 4 and 8.
    #[repr(C, align(4))]
    struct MsgImage([u8; 12]);

    fn msg_program() -> MemProgram {
        vec![MemOp::Enum(Box::new(EnumOp {
            tag_offset: 0,
            tag_width: 1,
            variants: vec![
                // Ping: wire index 0, selector 0, no payload.
                EnumVariantOp { wire_index: 0, selector: 0, payload: vec![] },
                // Echo(u32): wire index 1, selector 1, one scalar at offset 4.
                EnumVariantOp {
                    wire_index: 1,
                    selector: 1,
                    payload: vec![MemOp::Scalar { offset: 4, size: 4, align: 4 }],
                },
                // Move{x,y}: wire index 2, selector 2, two scalars at 4 and 8.
                EnumVariantOp {
                    wire_index: 2,
                    selector: 2,
                    payload: vec![
                        MemOp::Scalar { offset: 4, size: 4, align: 4 },
                        MemOp::Scalar { offset: 8, size: 4, align: 4 },
                    ],
                },
            ],
            writer_only: Vec::new(),
        }))]
    }

    /// Each enum variant shape (unit, scalar payload, struct payload) encodes to
    /// the expected wire and round-trips byte-identically.
    #[test]
    fn jit_enum_all_variant_shapes_roundtrip() {
        let program = msg_program();
        let enc = NativeEncode::compile(&program);
        let dec = NativeDecode::compile(&program);

        // (image, expected wire) per variant.
        let mut ping = MsgImage([0; 12]);
        ping.0[0] = 0; // selector 0
        let ping_wire = 0u32.to_le_bytes().to_vec();

        let mut echo = MsgImage([0; 12]);
        echo.0[0] = 1; // selector 1
        echo.0[4..8].copy_from_slice(&0xCAFE_F00Du32.to_le_bytes());
        let mut echo_wire = 1u32.to_le_bytes().to_vec();
        echo_wire.extend_from_slice(&0xCAFE_F00Du32.to_le_bytes());

        let mut mv = MsgImage([0; 12]);
        mv.0[0] = 2; // selector 2
        mv.0[4..8].copy_from_slice(&3i32.to_le_bytes());
        mv.0[8..12].copy_from_slice(&(-4i32).to_le_bytes());
        let mut mv_wire = 2u32.to_le_bytes().to_vec();
        mv_wire.extend_from_slice(&3i32.to_le_bytes());
        mv_wire.extend_from_slice(&(-4i32).to_le_bytes());

        for (img, want) in [(&ping, &ping_wire), (&echo, &echo_wire), (&mv, &mv_wire)] {
            let got = unsafe { enc.run(img.0.as_ptr()) };
            assert_eq!(&got, want, "encode mismatch (selector {})", img.0[0]);

            let mut slot = MsgImage([0xFF; 12]);
            unsafe { dec.run(&got, slot.0.as_mut_ptr()) }.unwrap();
            // The discriminant is written; payload fields restored. (For Ping the
            // payload region is untouched, which is fine — only the selector is
            // semantically live.)
            assert_eq!(slot.0[0], img.0[0], "selector mismatch");
            if img.0[0] != 0 {
                assert_eq!(&slot.0[4..8], &img.0[4..8], "x/scalar mismatch");
            }
            if img.0[0] == 2 {
                assert_eq!(&slot.0[8..12], &img.0[8..12], "y mismatch");
            }
        }
    }

    /// An unmatched wire index must reject, never produce a value.
    #[test]
    fn jit_enum_rejects_unmatched_index() {
        let program = msg_program();
        let dec = NativeDecode::compile(&program);
        // Wire variant index 99 — no such variant, and (single-schema) no
        // writer-only set, so it's a garbage index → BadVariantIndex.
        let wire = 99u32.to_le_bytes().to_vec();
        let mut slot = MsgImage([0; 12]);
        let err = unsafe { dec.run(&wire, slot.0.as_mut_ptr()) }.unwrap_err();
        assert!(matches!(err, DecodeError::BadVariantIndex(99)), "got {err:?}");

        // A truncated index (3 bytes) is an EOF, not a bad-index rejection.
        let mut slot2 = MsgImage([0; 12]);
        let err2 = unsafe { dec.run(&[0u8, 0, 0], slot2.0.as_mut_ptr()) }.unwrap_err();
        assert!(matches!(err2, DecodeError::UnexpectedEof { .. }), "got {err2:?}");
    }

    /// A wider discriminant (`tag_width = 4`) exercises the multi-byte selector
    /// read/write path, distinct from the 1-byte `Msg` case.
    #[test]
    fn jit_enum_wide_tag_roundtrip() {
        #[repr(C, align(4))]
        struct Img([u8; 8]);
        // Discriminant u32 at offset 0; one u32 payload at offset 4.
        let program: MemProgram = vec![MemOp::Enum(Box::new(EnumOp {
            tag_offset: 0,
            tag_width: 4,
            variants: vec![
                EnumVariantOp { wire_index: 0, selector: 0x1111_1111, payload: vec![] },
                EnumVariantOp {
                    wire_index: 1,
                    selector: 0x2222_2222,
                    payload: vec![MemOp::Scalar { offset: 4, size: 4, align: 4 }],
                },
            ],
            writer_only: Vec::new(),
        }))];
        let enc = NativeEncode::compile(&program);
        let dec = NativeDecode::compile(&program);

        let mut a = Img([0; 8]);
        a.0[0..4].copy_from_slice(&0x2222_2222u32.to_le_bytes());
        a.0[4..8].copy_from_slice(&0x0BAD_F00Du32.to_le_bytes());
        let got = unsafe { enc.run(a.0.as_ptr()) };
        let mut want = 1u32.to_le_bytes().to_vec();
        want.extend_from_slice(&0x0BAD_F00Du32.to_le_bytes());
        assert_eq!(got, want);

        let mut slot = Img([0; 8]);
        unsafe { dec.run(&got, slot.0.as_mut_ptr()) }.unwrap();
        assert_eq!(slot.0[0..4], 0x2222_2222u32.to_le_bytes());
        assert_eq!(slot.0[4..8], 0x0BAD_F00Du32.to_le_bytes());
    }

    /// An enum nested inside a struct: a leading scalar, then the enum (whose
    /// payload writes at base-relative offsets past the discriminant). Exercises
    /// the enum continuing the outer chain after its payload sub-chain.
    #[test]
    fn jit_enum_after_scalar_roundtrip() {
        #[repr(C, align(4))]
        struct Img([u8; 16]);
        // u32 @ 0, then an enum with discriminant u8 @ 4 and a u32 payload @ 8.
        let program: MemProgram = vec![
            MemOp::Scalar { offset: 0, size: 4, align: 4 },
            MemOp::Enum(Box::new(EnumOp {
                tag_offset: 4,
                tag_width: 1,
                variants: vec![
                    EnumVariantOp { wire_index: 0, selector: 0, payload: vec![] },
                    EnumVariantOp {
                        wire_index: 1,
                        selector: 1,
                        payload: vec![MemOp::Scalar { offset: 8, size: 4, align: 4 }],
                    },
                ],
                writer_only: Vec::new(),
            })),
        ];
        let enc = NativeEncode::compile(&program);
        let dec = NativeDecode::compile(&program);

        let mut img = Img([0; 16]);
        img.0[0..4].copy_from_slice(&0xABCD_1234u32.to_le_bytes());
        img.0[4] = 1; // selector 1
        img.0[8..12].copy_from_slice(&0x5678_9ABCu32.to_le_bytes());
        let got = unsafe { enc.run(img.0.as_ptr()) };

        // Known wire: the leading u32, then the enum's u32 wire index, then the
        // payload u32 (no padding — the enum's u32 index starts 4-aligned and the
        // payload u32 follows it contiguously).
        let mut want = 0xABCD_1234u32.to_le_bytes().to_vec();
        want.extend_from_slice(&1u32.to_le_bytes());
        want.extend_from_slice(&0x5678_9ABCu32.to_le_bytes());
        assert_eq!(got, want, "encode wire mismatch");

        let mut slot = Img([0; 16]);
        unsafe { dec.run(&got, slot.0.as_mut_ptr()) }.unwrap();
        assert_eq!(slot.0[0..4], 0xABCD_1234u32.to_le_bytes());
        assert_eq!(slot.0[4], 1);
        assert_eq!(slot.0[8..12], 0x5678_9ABCu32.to_le_bytes());
    }

    // ====================================================================
    // Map: a LOOP with TWO sub-chains (key program + value program), plus
    // per-pair allocation + insert on decode and a stateful iterator on encode
    // ====================================================================

    use phon_ir::ir::MapOp;
    use phon_ir::MapThunks;
    use std::collections::BTreeMap;

    // Hand-written `BTreeMap<String, u32>` thunks, mirroring the interpreter's map
    // test thunks: the engine decodes a key+value into scratch and `insert` moves
    // both in; encode reads length and iterates entries through a stateful iterator.
    type SU32 = BTreeMap<String, u32>;

    unsafe extern "C" fn su32_len(_ctx: *const (), map: *const u8) -> usize {
        unsafe { (*map.cast::<SU32>()).len() }
    }
    unsafe extern "C" fn su32_init_with_capacity(_ctx: *const (), map: *mut u8, _cap: usize) {
        // `BTreeMap` has no with_capacity; an empty map is the right starting point.
        unsafe { core::ptr::write(map.cast::<SU32>(), BTreeMap::new()) };
    }
    unsafe extern "C" fn su32_insert(_ctx: *const (), map: *mut u8, key: *mut u8, value: *mut u8) {
        // Move the key and value out of the engine scratch (the engine then frees
        // both WITHOUT dropping).
        let k = unsafe { core::ptr::read(key.cast::<String>()) };
        let v = unsafe { core::ptr::read(value.cast::<u32>()) };
        unsafe { (*map.cast::<SU32>()).insert(k, v) };
    }
    // The boxed iterator state: the borrowed (key, value) pointers collected up
    // front (BTreeMap iteration is sorted/stable), plus a cursor. The pointers
    // borrow the map's own storage, so encode reads the real bytes.
    struct SU32Iter {
        pairs: Vec<(*const u8, *const u8)>,
        pos: usize,
    }
    unsafe extern "C" fn su32_iter_init(_ctx: *const (), map: *const u8) -> *mut () {
        let m = unsafe { &*map.cast::<SU32>() };
        let pairs: Vec<(*const u8, *const u8)> = m
            .iter()
            .map(|(k, v)| {
                (core::ptr::from_ref(k).cast::<u8>(), core::ptr::from_ref(v).cast::<u8>())
            })
            .collect();
        Box::into_raw(Box::new(SU32Iter { pairs, pos: 0 })).cast::<()>()
    }
    unsafe extern "C" fn su32_iter_next(
        _ctx: *const (),
        iter: *mut (),
        key_out: *mut *const u8,
        value_out: *mut *const u8,
    ) -> bool {
        let it = unsafe { &mut *iter.cast::<SU32Iter>() };
        if it.pos >= it.pairs.len() {
            return false;
        }
        let (k, v) = it.pairs[it.pos];
        it.pos += 1;
        unsafe {
            *key_out = k;
            *value_out = v;
        }
        true
    }
    unsafe extern "C" fn su32_iter_dealloc(_ctx: *const (), iter: *mut ()) {
        drop(unsafe { Box::from_raw(iter.cast::<SU32Iter>()) });
    }
    fn su32_thunks() -> MapThunks {
        MapThunks {
            ctx: core::ptr::null(),
            len: su32_len,
            init_with_capacity: su32_init_with_capacity,
            insert: su32_insert,
            iter_init: su32_iter_init,
            iter_next: su32_iter_next,
            iter_dealloc: su32_iter_dealloc,
        }
    }

    /// A root program of a single owned `BTreeMap<String, u32>`. Key sub-chain: a
    /// UTF-8-validated `String` run; value sub-chain: a u32 scalar.
    fn su32_program() -> MemProgram {
        vec![MemOp::Map(Box::new(MapOp {
            field_offset: 0,
            key: vec![MemOp::Bytes(Box::new(BytesOp {
                field_offset: 0,
                stride: 1,
                elem_align: 1,
                validate: validate_utf8,
                thunks: str_thunks(),
            }))],
            value: vec![MemOp::Scalar { offset: 0, size: 4, align: 4 }],
            key_size: core::mem::size_of::<String>(),
            key_align: core::mem::align_of::<String>(),
            value_size: 4,
            value_align: 4,
            thunks: su32_thunks(),
        }))]
    }

    /// Build the wire bytes for a `BTreeMap<String, u32>`: a `u32` entry count then,
    /// per entry (in the map's sorted iteration order), the key `String` (u32 len +
    /// bytes) then the value u32 padded to its 4-byte alignment.
    fn su32_wire(m: &SU32) -> Vec<u8> {
        let mut wire = (m.len() as u32).to_le_bytes().to_vec();
        for (k, v) in m {
            wire.extend_from_slice(&(k.len() as u32).to_le_bytes());
            wire.extend_from_slice(k.as_bytes());
            // Pad to the value u32's 4-byte alignment, measured from the start.
            while !wire.len().is_multiple_of(4) {
                wire.push(0);
            }
            wire.extend_from_slice(&v.to_le_bytes());
        }
        wire
    }

    /// JIT-decode a `BTreeMap<String, u32>` (a loop with two sub-chains, per-pair
    /// scratch alloc + insert) and confirm the reconstructed map equals the input.
    #[test]
    fn jit_decode_map_string_u32() {
        let program = su32_program();
        let mut m = BTreeMap::new();
        m.insert("alpha".to_string(), 1u32);
        m.insert("beta".to_string(), 0xCAFEu32);
        m.insert("gamma".to_string(), 0xDEAD_BEEFu32);
        let wire = su32_wire(&m);

        let jit = NativeDecode::compile(&program);
        let mut slot = MaybeUninit::<SU32>::uninit();
        unsafe { jit.run(&wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back, m);
    }

    /// JIT-encode a `BTreeMap<String, u32>` and confirm byte-identical wire (the
    /// stateful iterator path) plus a `NativeDecode` round-trip.
    #[test]
    fn jit_encode_map_string_u32_roundtrips() {
        let program = su32_program();
        let mut m = BTreeMap::new();
        m.insert("alpha".to_string(), 1u32);
        m.insert("beta".to_string(), 0xCAFEu32);
        m.insert("gamma".to_string(), 0xDEAD_BEEFu32);

        let enc = NativeEncode::compile(&program);
        let got = unsafe { enc.run(core::ptr::from_ref(&m).cast::<u8>()) };
        assert_eq!(got, su32_wire(&m));

        let dec = NativeDecode::compile(&program);
        let mut slot = MaybeUninit::<SU32>::uninit();
        unsafe { dec.run(&got, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back, m);
    }

    /// An empty map: count 0, no entries, no allocation, the map is empty.
    #[test]
    fn jit_map_empty_roundtrips() {
        let program = su32_program();
        let m: SU32 = BTreeMap::new();

        let enc = NativeEncode::compile(&program);
        let got = unsafe { enc.run(core::ptr::from_ref(&m).cast::<u8>()) };
        assert_eq!(got, 0u32.to_le_bytes().to_vec());

        let dec = NativeDecode::compile(&program);
        let mut slot = MaybeUninit::<SU32>::uninit();
        unsafe { dec.run(&got, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert!(back.is_empty());
    }

    /// A wire with a repeated key collapses two entries into one in the `BTreeMap`,
    /// so `len(1) != count(2)`: the JIT must reject with `DuplicateKey`.
    #[test]
    fn jit_map_rejects_duplicate_key() {
        let program = su32_program();
        // count 2, then the SAME key "k" twice (each with its own u32 value padded
        // to 4-byte alignment).
        let mut wire = 2u32.to_le_bytes().to_vec();
        for val in [10u32, 20u32] {
            wire.extend_from_slice(&1u32.to_le_bytes()); // key length 1
            wire.push(b'k');
            while !wire.len().is_multiple_of(4) {
                wire.push(0);
            }
            wire.extend_from_slice(&val.to_le_bytes());
        }

        let dec = NativeDecode::compile(&program);
        let mut slot = MaybeUninit::<SU32>::uninit();
        let err = unsafe { dec.run(&wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap_err();
        assert!(matches!(err, DecodeError::DuplicateKey), "got {err:?}");

        // The duplicate collapsed into one entry before the rejection; the engine
        // documents the partial-map leak (matching the interpreter). Reclaim the
        // initialized map so the leak does not trip Miri (this path never runs under
        // Miri — JIT code can't — but keeps the test self-consistent).
        let partial = unsafe { core::ptr::read(slot.as_ptr().cast::<SU32>()) };
        assert_eq!(partial.len(), 1);
        drop(partial);
    }

    /// A `BTreeMap<String, String>`: a heap value, exercising BOTH sub-chains'
    /// scratch-move (the key AND value `String`s are decoded into engine scratch,
    /// then `insert` moves them into the map and the scratch is freed without
    /// dropping).
    #[test]
    fn jit_map_string_string_roundtrips() {
        type SS = BTreeMap<String, String>;

        unsafe extern "C" fn ss_len(_ctx: *const (), map: *const u8) -> usize {
            unsafe { (*map.cast::<SS>()).len() }
        }
        unsafe extern "C" fn ss_init(_ctx: *const (), map: *mut u8, _cap: usize) {
            unsafe { core::ptr::write(map.cast::<SS>(), BTreeMap::new()) };
        }
        unsafe extern "C" fn ss_insert(_ctx: *const (), map: *mut u8, key: *mut u8, value: *mut u8) {
            let k = unsafe { core::ptr::read(key.cast::<String>()) };
            let v = unsafe { core::ptr::read(value.cast::<String>()) };
            unsafe { (*map.cast::<SS>()).insert(k, v) };
        }
        struct SSIter {
            pairs: Vec<(*const u8, *const u8)>,
            pos: usize,
        }
        unsafe extern "C" fn ss_iter_init(_ctx: *const (), map: *const u8) -> *mut () {
            let m = unsafe { &*map.cast::<SS>() };
            let pairs = m
                .iter()
                .map(|(k, v)| {
                    (core::ptr::from_ref(k).cast::<u8>(), core::ptr::from_ref(v).cast::<u8>())
                })
                .collect();
            Box::into_raw(Box::new(SSIter { pairs, pos: 0 })).cast::<()>()
        }
        unsafe extern "C" fn ss_iter_next(
            _ctx: *const (),
            iter: *mut (),
            key_out: *mut *const u8,
            value_out: *mut *const u8,
        ) -> bool {
            let it = unsafe { &mut *iter.cast::<SSIter>() };
            if it.pos >= it.pairs.len() {
                return false;
            }
            let (k, v) = it.pairs[it.pos];
            it.pos += 1;
            unsafe {
                *key_out = k;
                *value_out = v;
            }
            true
        }
        unsafe extern "C" fn ss_iter_dealloc(_ctx: *const (), iter: *mut ()) {
            drop(unsafe { Box::from_raw(iter.cast::<SSIter>()) });
        }

        let str_run = || {
            MemOp::Bytes(Box::new(BytesOp {
                field_offset: 0,
                stride: 1,
                elem_align: 1,
                validate: validate_utf8,
                thunks: str_thunks(),
            }))
        };
        let program: MemProgram = vec![MemOp::Map(Box::new(MapOp {
            field_offset: 0,
            key: vec![str_run()],
            value: vec![str_run()],
            key_size: core::mem::size_of::<String>(),
            key_align: core::mem::align_of::<String>(),
            value_size: core::mem::size_of::<String>(),
            value_align: core::mem::align_of::<String>(),
            thunks: MapThunks {
                ctx: core::ptr::null(),
                len: ss_len,
                init_with_capacity: ss_init,
                insert: ss_insert,
                iter_init: ss_iter_init,
                iter_next: ss_iter_next,
                iter_dealloc: ss_iter_dealloc,
            },
        }))];

        let mut m: SS = BTreeMap::new();
        m.insert("name".to_string(), "héllo 🐝".to_string());
        m.insert("other".to_string(), "wörld".to_string());
        m.insert("zed".to_string(), String::new());

        // Known wire: count, then per sorted entry the key String run (no pad — len
        // and bytes are byte-aligned) then the value String run.
        let mut want = (m.len() as u32).to_le_bytes().to_vec();
        for (k, v) in &m {
            want.extend_from_slice(&(k.len() as u32).to_le_bytes());
            want.extend_from_slice(k.as_bytes());
            want.extend_from_slice(&(v.len() as u32).to_le_bytes());
            want.extend_from_slice(v.as_bytes());
        }

        let enc = NativeEncode::compile(&program);
        let got = unsafe { enc.run(core::ptr::from_ref(&m).cast::<u8>()) };
        assert_eq!(got, want, "encode wire mismatch");

        let dec = NativeDecode::compile(&program);
        let mut slot = MaybeUninit::<SS>::uninit();
        unsafe { dec.run(&got, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back, m);
    }
}
