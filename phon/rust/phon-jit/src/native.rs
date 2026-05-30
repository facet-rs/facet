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

use core::ffi::c_void;

use phon_ir::ir::{MemOp, MemProgram};
use phon_schema::DecodeError;

use crate::stencils::{DONE, SCALAR, SCALAR_CONT, SEQUENCE, SEQUENCE_CONT};

unsafe extern "C" {
    /// Toggle the calling thread's JIT pages between writable (`0`) and
    /// executable (`1`).
    fn pthread_jit_write_protect_np(enabled: i32);
    /// Flush the instruction cache for a region after writing code into it.
    fn sys_icache_invalidate(start: *mut c_void, len: usize);
}

/// A page of executable memory holding copied machine code.
pub struct ExecBuf {
    ptr: *mut u8,
    len: usize,
}

impl ExecBuf {
    /// Allocate JIT memory, copy `code` into it, and make it executable.
    ///
    /// # Panics
    /// If `code` is empty or the `MAP_JIT` allocation fails.
    #[must_use]
    pub fn new(code: &[u8]) -> ExecBuf {
        assert!(!code.is_empty(), "cannot execute empty code");
        let len = code.len();
        unsafe {
            let ptr = libc::mmap(
                core::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
                libc::MAP_PRIVATE | libc::MAP_ANON | libc::MAP_JIT,
                -1,
                0,
            );
            assert!(ptr != libc::MAP_FAILED, "mmap(MAP_JIT) failed");
            let ptr = ptr.cast::<u8>();

            // Writable -> copy -> executable -> flush i-cache.
            pthread_jit_write_protect_np(0);
            core::ptr::copy_nonoverlapping(code.as_ptr(), ptr, len);
            pthread_jit_write_protect_np(1);
            sys_icache_invalidate(ptr.cast::<c_void>(), len);

            ExecBuf { ptr, len }
        }
    }

    /// The entry pointer to the copied code.
    #[must_use]
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr
    }
}

impl Drop for ExecBuf {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr.cast::<c_void>(), self.len);
        }
    }
}

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
    /// prog slot. Heap-stable for the `*const SeqInfo` the prog stream holds.
    seq_infos: Vec<Box<SeqInfo>>,
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

/// Accumulates the code bytes, per-chain prog streams, and sequence metadata while
/// the program is walked. Two passes: lay everything out (this struct), then bind
/// the `ExecBuf`-relative pointers ([`NativeDecode::compile`]).
struct Compiler {
    code: Vec<u8>,
    progs: Vec<Vec<u64>>,
    seq_infos: Vec<SeqInfoBuild>,
    fixups: Vec<SeqFixup>,
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
        };
        let top = c.compile_chain(program);

        // The code layout is final; make it executable. Pointers into it are now
        // stable for the lifetime of the `ExecBuf`.
        let buf = ExecBuf::new(&c.code);
        let base = buf.as_ptr();

        // Box each chain's prog stream so its address is stable (the prog
        // pointers in `seq_infos` and the entry pointer alias into these).
        let progs = c.progs;

        // Materialize the `SeqInfo`s now that the code base is known. Box each so
        // its address is stable for the `*const SeqInfo` the prog slot holds.
        let mut seq_infos: Vec<Box<SeqInfo>> = Vec::with_capacity(c.seq_infos.len());
        for b in &c.seq_infos {
            // The element chain entry is the stencil at `base + entry_offset`.
            let element_entry: unsafe extern "C" fn(*mut Ctx) =
                unsafe { core::mem::transmute(base.add(b.element_entry_offset)) };
            seq_infos.push(Box::new(SeqInfo {
                field_offset: b.field_offset,
                stride: b.stride,
                elem_align: b.elem_align,
                min_wire: b.min_wire,
                thunks_ctx: b.thunks_ctx,
                from_raw_parts: b.from_raw_parts,
                element_entry,
                // Bound below, once `progs` is owned by `NativeDecode` (the box
                // above gives us a stable prog stream to point at).
                element_prog: core::ptr::null(),
            }));
        }

        let mut nd = NativeDecode {
            buf,
            entry_prog: top.prog_index,
            progs,
            seq_infos,
        };

        // Now that `nd.progs` is in its final home, bind the prog pointers: each
        // `SeqInfo.element_prog` to its element chain's stream, and each sequence
        // prog slot to its `SeqInfo`.
        for (b, info) in c.seq_infos.iter().zip(nd.seq_infos.iter_mut()) {
            info.element_prog = nd.progs[b.element_prog_index].as_ptr();
        }
        for f in &c.fixups {
            let ptr: *const SeqInfo = &*nd.seq_infos[f.seqinfo];
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
            alloc: jit_alloc,
            dealloc: jit_dealloc,
        };
        let entry: extern "C" fn(*mut Ctx) = unsafe { core::mem::transmute(self.buf.as_ptr()) };
        entry(&mut ctx);

        if ctx.status != 0 {
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

/// Patch an AArch64 `B`/`BL` (`BRANCH26`) at `site` in `code` to target byte
/// offset `target` within the same buffer. Both are buffer-relative; since the
/// branch is PC-relative the in-memory delta is identical.
fn patch_branch26(code: &mut [u8], site: usize, target: usize) {
    let instr = u32::from_le_bytes(code[site..site + 4].try_into().unwrap());
    let delta = (target as isize - site as isize) >> 2; // in instructions
    let imm26 = (delta as u32) & 0x03FF_FFFF;
    let patched = (instr & 0xFC00_0000) | imm26;
    code[site..site + 4].copy_from_slice(&patched.to_le_bytes());
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
    use phon_ir::ir::SeqOp;
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
}
