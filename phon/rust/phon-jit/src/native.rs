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

use crate::stencils::{DONE, SCALAR, SCALAR_CONT};

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
}

/// A JIT-compiled decoder for a scalar [`MemProgram`]: a `MAP_JIT` page of copied
/// `scalar` stencils with their continuations patched to chain, ending at `done`.
pub struct NativeDecode {
    buf: ExecBuf,
    /// `[offset, size, align]` per op, read by the stencils through `Ctx.prog`.
    prog: Vec<u64>,
}

impl NativeDecode {
    /// Compile a scalar program to native machine code.
    ///
    /// # Panics
    /// If the program contains an op the native backend does not support yet.
    // r[impl ir.stencils]
    #[must_use]
    pub fn compile(program: &MemProgram) -> NativeDecode {
        let mut prog = Vec::with_capacity(program.len() * 3);
        for op in program {
            match op {
                MemOp::Scalar { offset, size, align } => {
                    prog.push(*offset as u64);
                    prog.push(*size as u64);
                    prog.push(*align as u64);
                }
                MemOp::Sequence(_) => {
                    panic!("phon-jit native: sequences are interpreter-only for now")
                }
            }
        }
        let n_ops = program.len();

        // Lay out one `scalar` copy per op, then `done`.
        let mut code = Vec::new();
        let mut starts = Vec::with_capacity(n_ops);
        for _ in 0..n_ops {
            starts.push(code.len());
            code.extend_from_slice(SCALAR);
        }
        let done_start = code.len();
        code.extend_from_slice(DONE);

        // Patch each copy's continuation branches to the next op (the last to
        // `done`).
        for (i, &op_start) in starts.iter().enumerate() {
            let next = if i + 1 < n_ops {
                starts[i + 1]
            } else {
                done_start
            };
            for &rel in SCALAR_CONT {
                patch_branch26(&mut code, op_start + rel, next);
            }
        }

        NativeDecode {
            buf: ExecBuf::new(&code),
            prog,
        }
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
            prog: self.prog.as_ptr(),
            status: 0,
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
}
