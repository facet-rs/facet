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
/// clang-emitted machine code executing from a `MAP_JIT` page. A self-test that
/// the native execution path works on this machine; the real stencils plug into
/// the same substrate.
#[must_use]
pub fn smoke(x: i64) -> i64 {
    let buf = ExecBuf::new(crate::stencils::SMOKE);
    let f: extern "C" fn(i64) -> i64 = unsafe { core::mem::transmute(buf.as_ptr()) };
    f(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_compiler_emitted_machine_code() {
        assert_eq!(smoke(14), 43);
        assert_eq!(smoke(0), 1);
        assert_eq!(smoke(100), 301);
        assert_eq!(smoke(-1), -2);
    }
}
