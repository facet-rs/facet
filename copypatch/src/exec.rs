//! Executable memory for copied machine code.
//!
//! macOS on Apple Silicon enforces write-xor-execute on JIT pages: a `MAP_JIT`
//! region is either writable or executable per thread, toggled with
//! `pthread_jit_write_protect_np`, and the instruction cache must be flushed
//! after writing (`sys_icache_invalidate`). [`ExecBuf`] wraps that dance: it
//! takes a finished code buffer (already relocation-patched by the caller), copies
//! it into a fresh `MAP_JIT` page, makes it executable, and frees it on drop.
//!
//! Linux on x86-64 uses a normal anonymous writable mapping, copies the patched
//! code into it, then flips the region to read-execute with `mprotect`.

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use core::ffi::c_void;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
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
    /// `code` must already be fully patched (any continuation branches resolved,
    /// e.g. via [`patch_branch26`](crate::patch_branch26)) — this only copies and
    /// flips protection; it does not relocate.
    ///
    /// # Panics
    /// If `code` is empty or the `MAP_JIT` allocation fails.
    #[must_use]
    pub fn new(code: &[u8]) -> ExecBuf {
        assert!(!code.is_empty(), "cannot execute empty code");
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            return Self::new_apple_silicon(code);
        }
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            return Self::new_linux_x86_64(code);
        }
        #[allow(unreachable_code)]
        {
            let _ = code;
            panic!("ExecBuf is not available for this target");
        }
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn new_apple_silicon(code: &[u8]) -> ExecBuf {
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

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn new_linux_x86_64(code: &[u8]) -> ExecBuf {
        let len = code.len();
        unsafe {
            let ptr = libc::mmap(
                core::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            );
            assert!(ptr != libc::MAP_FAILED, "mmap failed");
            let ptr = ptr.cast::<u8>();

            core::ptr::copy_nonoverlapping(code.as_ptr(), ptr, len);
            let rc = libc::mprotect(ptr.cast(), len, libc::PROT_READ | libc::PROT_EXEC);
            assert_eq!(rc, 0, "mprotect(PROT_READ | PROT_EXEC) failed");

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
            libc::munmap(self.ptr.cast(), self.len);
        }
    }
}
