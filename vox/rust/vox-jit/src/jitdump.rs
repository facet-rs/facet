//! Jitdump emitter for `perf` integration.
//!
//! Writes `/tmp/jit-<pid>.dump` in the format documented by
//! `linux/tools/perf/Documentation/jitdump-specification.txt`. Together with an
//! executable `mmap` of the file, this lets `perf record -k mono` capture the
//! mapping and `perf inject --jit` extract per-function ELF objects so
//! `perf annotate` can show disassembly of Cranelift-compiled stubs.
//!
//! Activated by `VOX_JIT_PERF=1`. No-op on non-Linux targets.

#[cfg(not(target_os = "linux"))]
pub fn record_load(_name: &str, _code_addr: *const u8, _code_size: u32) {}

#[cfg(target_os = "linux")]
pub use linux::record_load;

#[cfg(target_os = "linux")]
mod linux {
    use std::ffi::CString;
    use std::fs::{File, OpenOptions};
    use std::io::Write;
    use std::os::unix::io::AsRawFd;
    use std::sync::{Mutex, OnceLock};

    const JITDUMP_MAGIC: u32 = 0x4A695444; // "JiTD"
    const JITDUMP_VERSION: u32 = 1;
    const JIT_CODE_LOAD: u32 = 0;

    #[cfg(target_arch = "x86_64")]
    const ELF_MACH: u32 = 62;
    #[cfg(target_arch = "aarch64")]
    const ELF_MACH: u32 = 183;
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    const ELF_MACH: u32 = 0;

    struct Dump {
        file: File,
        // mmap kept alive for the lifetime of the process; perf needs the
        // executable mmap event in its recording to associate the file with
        // this pid.
        _mmap_addr: *mut libc::c_void,
        _mmap_len: usize,
        code_index: u64,
    }

    // SAFETY: the raw mmap pointer is never dereferenced from Rust; it's only
    // held to keep the mapping alive (and thus visible to perf).
    unsafe impl Send for Dump {}

    impl Dump {
        fn open() -> std::io::Result<Self> {
            let pid = unsafe { libc::getpid() } as u32;
            let path = format!("/tmp/jit-{pid}.dump");
            let file = OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .truncate(true)
                .open(&path)?;

            let mut hdr = [0u8; 40];
            hdr[0..4].copy_from_slice(&JITDUMP_MAGIC.to_ne_bytes());
            hdr[4..8].copy_from_slice(&JITDUMP_VERSION.to_ne_bytes());
            hdr[8..12].copy_from_slice(&40u32.to_ne_bytes());
            hdr[12..16].copy_from_slice(&ELF_MACH.to_ne_bytes());
            // pad1 = 0
            hdr[20..24].copy_from_slice(&pid.to_ne_bytes());
            hdr[24..32].copy_from_slice(&monotonic_ns().to_ne_bytes());
            // flags = 0
            (&file).write_all(&hdr)?;

            let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as usize;
            let addr = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    page,
                    libc::PROT_READ | libc::PROT_EXEC,
                    libc::MAP_PRIVATE,
                    file.as_raw_fd(),
                    0,
                )
            };
            if addr == libc::MAP_FAILED {
                return Err(std::io::Error::last_os_error());
            }

            Ok(Self {
                file,
                _mmap_addr: addr,
                _mmap_len: page,
                code_index: 0,
            })
        }

        fn record_load(
            &mut self,
            name: &str,
            code_addr: u64,
            code_size: u64,
        ) -> std::io::Result<()> {
            let cname = CString::new(name).unwrap_or_else(|_| CString::new("vox_jit").unwrap());
            let name_bytes = cname.as_bytes_with_nul();
            // 16-byte record prefix + 40-byte load fields + name + code
            let total = 16 + 40 + name_bytes.len() + code_size as usize;
            let pid = unsafe { libc::getpid() } as u32;
            let tid = unsafe { libc::syscall(libc::SYS_gettid) } as u32;

            let mut buf = Vec::with_capacity(total);
            buf.extend_from_slice(&JIT_CODE_LOAD.to_ne_bytes());
            buf.extend_from_slice(&(total as u32).to_ne_bytes());
            buf.extend_from_slice(&monotonic_ns().to_ne_bytes());
            buf.extend_from_slice(&pid.to_ne_bytes());
            buf.extend_from_slice(&tid.to_ne_bytes());
            buf.extend_from_slice(&code_addr.to_ne_bytes()); // vma
            buf.extend_from_slice(&code_addr.to_ne_bytes()); // code_addr
            buf.extend_from_slice(&code_size.to_ne_bytes());
            buf.extend_from_slice(&self.code_index.to_ne_bytes());
            buf.extend_from_slice(name_bytes);
            let code_slice = unsafe {
                std::slice::from_raw_parts(code_addr as *const u8, code_size as usize)
            };
            buf.extend_from_slice(code_slice);
            self.code_index += 1;
            (&self.file).write_all(&buf)
        }
    }

    fn monotonic_ns() -> u64 {
        let mut ts: libc::timespec = unsafe { std::mem::zeroed() };
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
        (ts.tv_sec as u64) * 1_000_000_000 + (ts.tv_nsec as u64)
    }

    fn enabled() -> bool {
        static CACHED: OnceLock<bool> = OnceLock::new();
        *CACHED.get_or_init(|| std::env::var_os("VOX_JIT_PERF").is_some_and(|v| v == "1"))
    }

    fn dump() -> Option<&'static Mutex<Dump>> {
        static D: OnceLock<Option<Mutex<Dump>>> = OnceLock::new();
        D.get_or_init(|| {
            if !enabled() {
                return None;
            }
            match Dump::open() {
                Ok(d) => Some(Mutex::new(d)),
                Err(e) => {
                    eprintln!("vox-jit: failed to open jitdump: {e}");
                    None
                }
            }
        })
        .as_ref()
    }

    pub fn record_load(name: &str, code_addr: *const u8, code_size: u32) {
        if code_size == 0 || code_addr.is_null() {
            return;
        }
        if let Some(m) = dump() {
            if let Ok(mut d) = m.lock() {
                let _ = d.record_load(name, code_addr as u64, code_size as u64);
            }
        }
    }
}
