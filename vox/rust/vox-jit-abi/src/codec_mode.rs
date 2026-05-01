//! Process-wide codec configuration shared by every backend.
//!
//! Both the Rust Cranelift JIT (`vox-jit`) and the Swift codec FFI
//! (`vox-swift-abi`) consult these accessors so a single set of environment
//! variables governs every codec in the process. Each accessor caches its
//! reading in a `OnceLock`, so the env var is consulted at most once per
//! process per knob.

use std::sync::OnceLock;

/// Which decoder/encoder the RPC layer should use for this process.
///
/// Selected via the `VOX_CODEC` environment variable:
/// - `reflect` — facet-reflect oracle (slow, correctness baseline, Miri-safe).
/// - `interp`  — IR interpreter (shares lowering with JIT, Miri-safe).
/// - `jit`     — Cranelift JIT, falling back to `reflect` for shapes the JIT
///   cannot compile. This is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecMode {
    Reflect,
    Interp,
    Jit,
}

impl CodecMode {
    pub fn from_env() -> Self {
        static CACHED: OnceLock<CodecMode> = OnceLock::new();
        *CACHED.get_or_init(|| match std::env::var("VOX_CODEC").ok().as_deref() {
            Some("reflect") => CodecMode::Reflect,
            Some("interp") => CodecMode::Interp,
            Some("jit") | None => CodecMode::Jit,
            Some(other) => {
                panic!("VOX_CODEC must be one of 'reflect', 'interp', 'jit' (got {other:?})")
            }
        })
    }
}

/// Returns `true` when the current [`CodecMode`] is not `Jit`. Callers use
/// this to short-circuit JIT compilation and fall through to the reflective
/// or IR-interpreter path.
pub fn force_fallback() -> bool {
    CodecMode::from_env() != CodecMode::Jit
}

/// `VOX_JIT_REQUIRE_PURE=1` forbids any non-JIT path. When set, callers that
/// would otherwise drop to the interpreter must instead surface an error
/// (or panic in test/bench builds) so coverage gaps are visible.
pub fn require_pure_jit() -> bool {
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(|| std::env::var_os("VOX_JIT_REQUIRE_PURE").is_some_and(|v| v == "1"))
}

/// `VOX_JIT_ABORT_ON_SLOW_PATH=1` aborts the process when the JIT decides
/// to fall back to the interpreter for a shape it cannot compile. Used by
/// test and bench harnesses that want the failure to be impossible to miss.
pub fn abort_on_slow_path() -> bool {
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(|| std::env::var_os("VOX_JIT_ABORT_ON_SLOW_PATH").is_some_and(|v| v == "1"))
}

/// `VOX_JIT_DUMP=1` prints each compiled function's CLIF IR and machine-code
/// disassembly to stderr. Useful when reasoning about why the JIT is (or
/// isn't) as fast as expected.
pub fn dump_compiled() -> bool {
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(|| std::env::var_os("VOX_JIT_DUMP").is_some_and(|v| v == "1"))
}

/// `VOX_JIT_PERF=1` enables jitdump emission so `perf record`/`perf report`
/// can resolve JIT'd frames to symbol names.
pub fn jit_perf_enabled() -> bool {
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(|| std::env::var_os("VOX_JIT_PERF").is_some_and(|v| v == "1"))
}
