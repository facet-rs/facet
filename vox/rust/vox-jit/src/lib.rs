//! Cranelift JIT backend for vox.
//!
//! Replaces the reflective interpreter on the hot decode path with
//! Cranelift-generated stubs. Falls back to the interpreter for unsupported
//! shapes, unsupported opaque types, or when calibration is unavailable.
//!
//! # Architecture
//!
//! - `vox_postcard::ir` — canonical IR types + pure interpreter (ir-architect)
//! - `codegen` — Cranelift backend that lowers `DecodeProgram` to machine code
//! - `cache` — in-memory stub cache (keyed by shape value + calibration generation)
//!
//! Entry point for callers: `JitRuntime`.

#![allow(unsafe_code)]

pub mod cache;
pub mod codegen;
pub mod helpers;

pub use cache::StubCache;
pub use codegen::{CodegenError, CraneliftBackend, host_isa_name};
pub use vox_jit_abi as abi;
pub use vox_jit_cal as cal;

use std::sync::Mutex;

use vox_jit_abi::{DecodeCacheKey, DecodeCtx, DecodeStatus};
use vox_jit_cal::CalibrationRegistry;

// ---------------------------------------------------------------------------
// JIT runtime — top-level handle
// ---------------------------------------------------------------------------

/// Process-local JIT runtime.
///
/// Holds the calibration registry, the Cranelift backend, and the stub cache.
/// Thread-safe via internal mutexes.
#[allow(dead_code)]
pub struct JitRuntime {
    cal: Mutex<CalibrationRegistry>,
    backend: Mutex<CraneliftBackend>,
    cache: StubCache,
}

impl JitRuntime {
    /// Create a new runtime. Fails if the host ISA is not supported by Cranelift.
    pub fn new() -> Result<Self, CodegenError> {
        Ok(Self {
            cal: Mutex::new(CalibrationRegistry::new()),
            backend: Mutex::new(CraneliftBackend::new()?),
            cache: StubCache::new(),
        })
    }

    /// Get the in-memory stub cache.
    pub fn cache(&self) -> &StubCache {
        &self.cache
    }

    /// Returns `true` if the `VOX_JIT_DISABLE` environment variable is set to `1`.
    ///
    /// When this returns `true`, all JIT paths should return `JitDecodeResult::FallBack`
    /// immediately, routing execution through the reflective interpreter. This is
    /// useful for differential testing (run the same input through both paths) and for
    /// production bisection without recompilation.
    pub fn force_fallback() -> bool {
        std::env::var_os("VOX_JIT_DISABLE").map_or(false, |v| v == "1")
    }

    /// Build a `DecodeCacheKey` for the given parameters.
    ///
    /// Per §Caching: if `descriptor_handle` is `None` (required calibration
    /// unavailable), the caller should NOT insert a stub — fall back to the
    /// IR interpreter instead. This method still builds the key for lookup
    /// purposes, but insert is the caller's responsibility to gate.
    ///
    /// The `local_shape` field uses `Shape`'s own `PartialEq`/`Hash` impls —
    /// not the pointer address. Two shapes at different addresses for the same
    /// Rust type produce the same cache key.
    pub fn decode_cache_key(
        &self,
        remote_schema_id: u64,
        local_shape: &'static facet_core::Shape,
        borrow_mode: bool,
        descriptor_handle: Option<vox_jit_abi::DescriptorHandle>,
    ) -> DecodeCacheKey {
        DecodeCacheKey {
            remote_schema_id,
            local_shape,
            borrow_mode,
            target_isa: host_isa_name(),
            descriptor_handle,
        }
    }
}

// ---------------------------------------------------------------------------
// Fallback interface
// ---------------------------------------------------------------------------

/// Result of a JIT decode attempt.
pub enum JitDecodeResult {
    /// The stub ran and produced `ctx.consumed` bytes consumed.
    Ok { bytes_consumed: usize },
    /// The stub ran but failed; `ctx` holds error position and init_count.
    Err {
        status: DecodeStatus,
        ctx: DecodeCtx,
    },
    /// No stub is compiled for this key; caller must use the IR interpreter.
    FallBack,
}
