//! Sampled profiling gate for the vox JIT rollout.
//!
//! Measures the decode hot path at each rollout stage so that wall-clock
//! comparisons can confirm the reflective overhead is leaving the profile:
//!
//!   Stage 1 (baseline): reflective interpreter via `from_slice_with_plan`
//!   Stage 2 (IR):       IR interpreter via `from_slice_ir`
//!   Stage 3+ (JIT):     JIT stub (wire in via decode_jit module when ready)
//!
//! Run with:
//!   cargo bench -p vox-jit-tests --bench decode
//!
//! Profile with samply:
//!   cargo samply record cargo bench -p vox-jit-tests --bench decode -- --profile-time 5
//!
//! The key signal is: after each stage, `facet_reflect::Partial` and
//! `facet_reflect::Peek` should shrink (or disappear) from the hot frame list.
//!
//! Allocation counting (task #27):
//!   The `alloc_count` module uses `CountingAllocator` to measure allocations
//!   and bytes per engine per workload. The thesis: JIT removes generic-container
//!   bookkeeping allocs. Run with:
//!     cargo bench -p vox-jit-tests --bench decode alloc_count

use facet::Facet;
use facet_core::Shape;
use spec_proto::{GnarlyAttr, GnarlyEntry, GnarlyKind, GnarlyPayload};
use vox_jit::abi::{DecodeCtx, OwnedDecodeFn};
use vox_jit::{CodegenError, CraneliftBackend};
use vox_jit_cal::{BorrowMode, CalibrationRegistry};
use vox_postcard::{
    TranslationPlan, build_identity_plan, from_slice_with_plan,
    ir::{from_slice_ir, lower_with_cal},
    serialize::to_vec,
};
use vox_schema::SchemaRegistry;

// ---------------------------------------------------------------------------
// Global counting allocator (task #27)
//
// Wraps the system allocator and counts allocations + bytes globally.
// Use `AllocSnapshot::take()` before a decode, then `delta()` after to get
// the number of allocs and bytes for exactly one decode call.
// ---------------------------------------------------------------------------

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

struct CountingAllocator;

static ALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // Count realloc as a new allocation for bookkeeping purposes.
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

/// Snapshot of the global allocation counters.
#[derive(Clone, Copy)]
struct AllocSnapshot {
    count: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn take() -> Self {
        Self {
            count: ALLOC_COUNT.load(Ordering::Relaxed),
            bytes: ALLOC_BYTES.load(Ordering::Relaxed),
        }
    }

    fn delta(self) -> (u64, u64) {
        let count = ALLOC_COUNT.load(Ordering::Relaxed) - self.count;
        let bytes = ALLOC_BYTES.load(Ordering::Relaxed) - self.bytes;
        (count, bytes)
    }
}

fn calibrated_registry() -> CalibrationRegistry {
    let mut cal = CalibrationRegistry::default();
    cal.calibrate_string_for_type();
    cal.calibrate_vec_for_type::<u32>();
    cal.calibrate_vec_for_type::<u8>();
    cal.calibrate_vec_for_type::<String>();
    cal
}

fn main() {
    divan::main();
}

// ---------------------------------------------------------------------------
// Shared fixtures: types and pre-encoded payloads
// ---------------------------------------------------------------------------

/// A message with a mix of primitive fields — exercises the struct decode path.
#[derive(Facet, Debug, PartialEq, Clone)]
struct Msg {
    id: u64,
    seq: u32,
    flags: u16,
    kind: u8,
}

impl Msg {
    fn sample() -> Self {
        Self {
            id: 0xDEAD_BEEF_CAFE_F00D,
            seq: 12345,
            flags: 0xABCD,
            kind: 7,
        }
    }
}

/// A message with a Vec<String> — exercises the reflective list-assembly path.
#[derive(Facet, Debug, PartialEq, Clone)]
struct TextBatch {
    topic: String,
    lines: Vec<String>,
}

impl TextBatch {
    fn sample_n(n: usize) -> Self {
        Self {
            topic: "events".to_string(),
            lines: (0..n).map(|i| format!("line-{i}")).collect(),
        }
    }
}

/// A message with nested struct — exercises recursive Partial construction.
#[derive(Facet, Debug, PartialEq, Clone)]
struct Inner {
    x: i32,
    label: String,
}

#[derive(Facet, Debug, PartialEq, Clone)]
struct Outer {
    name: String,
    inner: Inner,
    count: u32,
}

impl Outer {
    fn sample() -> Self {
        Self {
            name: "outer".to_string(),
            inner: Inner {
                x: -42,
                label: "inner".to_string(),
            },
            count: 99,
        }
    }
}

/// A message with a Vec<u32> — exercises varint-dense list decode.
#[derive(Facet, Debug, PartialEq, Clone)]
struct NumBatch {
    values: Vec<u32>,
}

impl NumBatch {
    fn sample_n(n: usize) -> Self {
        Self {
            values: (0u32..n as u32).collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers: build identity plan + pre-encode
// ---------------------------------------------------------------------------

fn plan_for<T: Facet<'static>>() -> TranslationPlan {
    build_identity_plan(T::SHAPE)
}

fn encode<T: for<'a> Facet<'a>>(v: &T) -> Vec<u8> {
    to_vec(v).expect("encode failed")
}

// ---------------------------------------------------------------------------
// Stage 1 baseline: reflective interpreter
// ---------------------------------------------------------------------------

mod reflective {
    use super::*;

    #[divan::bench]
    fn decode_msg(bencher: divan::Bencher) {
        let plan = plan_for::<Msg>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&Msg::sample());

        bencher.bench(|| from_slice_with_plan::<Msg>(&bytes, &plan, &registry).unwrap());
    }

    #[divan::bench]
    fn decode_outer(bencher: divan::Bencher) {
        let plan = plan_for::<Outer>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&Outer::sample());

        bencher.bench(|| from_slice_with_plan::<Outer>(&bytes, &plan, &registry).unwrap());
    }

    #[divan::bench(args = [4, 16, 64, 256])]
    fn decode_text_batch(bencher: divan::Bencher, n: usize) {
        let plan = plan_for::<TextBatch>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&TextBatch::sample_n(n));

        bencher.bench(|| from_slice_with_plan::<TextBatch>(&bytes, &plan, &registry).unwrap());
    }

    #[divan::bench(args = [16, 64, 256, 1024])]
    fn decode_num_batch(bencher: divan::Bencher, n: usize) {
        let plan = plan_for::<NumBatch>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&NumBatch::sample_n(n));

        bencher.bench(|| from_slice_with_plan::<NumBatch>(&bytes, &plan, &registry).unwrap());
    }

    /// u32 raw decode — minimal baseline for per-field overhead comparison.
    #[divan::bench]
    fn decode_u32(bencher: divan::Bencher) {
        let plan = plan_for::<u32>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&100_000u32);

        bencher.bench(|| from_slice_with_plan::<u32>(&bytes, &plan, &registry).unwrap());
    }

    /// String decode — exercises UTF-8 validation + heap allocation.
    #[divan::bench]
    fn decode_string(bencher: divan::Bencher) {
        let plan = plan_for::<String>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&"hello, world! this is a typical short string".to_string());

        bencher.bench(|| from_slice_with_plan::<String>(&bytes, &plan, &registry).unwrap());
    }
}

// ---------------------------------------------------------------------------
// Stage 2: IR interpreter
// ---------------------------------------------------------------------------

mod ir_interp {
    use super::*;

    #[divan::bench]
    fn decode_msg(bencher: divan::Bencher) {
        let plan = plan_for::<Msg>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&Msg::sample());

        bencher.bench(|| from_slice_ir::<Msg>(&bytes, &plan, &registry, None).unwrap());
    }

    #[divan::bench]
    fn decode_outer(bencher: divan::Bencher) {
        let plan = plan_for::<Outer>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&Outer::sample());

        bencher.bench(|| from_slice_ir::<Outer>(&bytes, &plan, &registry, None).unwrap());
    }

    #[divan::bench(args = [4, 16, 64, 256])]
    fn decode_text_batch(bencher: divan::Bencher, n: usize) {
        let plan = plan_for::<TextBatch>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&TextBatch::sample_n(n));

        bencher.bench(|| from_slice_ir::<TextBatch>(&bytes, &plan, &registry, None).unwrap());
    }

    #[divan::bench(args = [16, 64, 256, 1024])]
    fn decode_num_batch(bencher: divan::Bencher, n: usize) {
        let plan = plan_for::<NumBatch>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&NumBatch::sample_n(n));

        bencher.bench(|| from_slice_ir::<NumBatch>(&bytes, &plan, &registry, None).unwrap());
    }

    #[divan::bench]
    fn decode_u32(bencher: divan::Bencher) {
        let plan = plan_for::<u32>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&100_000u32);

        bencher.bench(|| from_slice_ir::<u32>(&bytes, &plan, &registry, None).unwrap());
    }

    #[divan::bench]
    fn decode_string(bencher: divan::Bencher) {
        let plan = plan_for::<String>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&"hello, world! this is a typical short string".to_string());

        bencher.bench(|| from_slice_ir::<String>(&bytes, &plan, &registry, None).unwrap());
    }

    /// IR with calibration registry (Vec<u8>/String opaque paths enabled).
    #[divan::bench(args = [16, 64, 256, 1024])]
    fn decode_num_batch_calibrated(bencher: divan::Bencher, n: usize) {
        let plan = plan_for::<NumBatch>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&NumBatch::sample_n(n));
        let cal = calibrated_registry();

        bencher.bench(|| from_slice_ir::<NumBatch>(&bytes, &plan, &registry, Some(&cal)).unwrap());
    }

    #[divan::bench(args = [4, 16, 64, 256])]
    fn decode_text_batch_calibrated(bencher: divan::Bencher, n: usize) {
        let plan = plan_for::<TextBatch>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&TextBatch::sample_n(n));
        let cal = calibrated_registry();

        bencher.bench(|| from_slice_ir::<TextBatch>(&bytes, &plan, &registry, Some(&cal)).unwrap());
    }
}

// ---------------------------------------------------------------------------
// Stage 3: JIT (Cranelift stubs)
//
// Each bench tries to compile a stub at setup time. If the program contains
// an unsupported op, the bench falls back to `from_slice_ir` and is labeled
// [SlowPath] in its name so the reader knows the JIT number is actually the
// IR interpreter number.
// ---------------------------------------------------------------------------

mod jit {
    use super::*;
    use std::mem::MaybeUninit;

    /// Try to compile a JIT stub for `T`. Returns the owned fn pointer on
    /// success, or a `CodegenError` (caller must use SlowPath instead).
    fn try_compile<T: Facet<'static>>(
        plan: &TranslationPlan,
        registry: &SchemaRegistry,
        cal: &CalibrationRegistry,
        backend: &mut CraneliftBackend,
    ) -> Result<OwnedDecodeFn, CodegenError> {
        let program = lower_with_cal(plan, T::SHAPE, registry, Some(cal), BorrowMode::Owned)
            .map_err(|e| CodegenError::UnsupportedOp(format!("{e:?}")))?;
        let owned = backend.compile_decode_owned(&program, cal)?;
        Ok(owned)
    }

    /// Decode `bytes` via a compiled stub into a `MaybeUninit<T>`, assume_init, drop.
    ///
    /// SAFETY: `owned_fn` must be a valid stub compiled for `T`; `bytes` must be
    /// a valid postcard encoding of a `T` value.
    unsafe fn decode_via_stub<T>(owned_fn: OwnedDecodeFn, bytes: &[u8]) -> T {
        let mut out = MaybeUninit::<T>::uninit();
        let mut ctx = DecodeCtx::new(bytes);
        let status = unsafe { owned_fn(&mut ctx, out.as_mut_ptr() as *mut u8) };
        assert!(status.is_ok(), "JIT stub returned {status:?}");
        unsafe { out.assume_init() }
    }

    #[divan::bench]
    fn decode_msg(bencher: divan::Bencher) {
        let plan = plan_for::<Msg>();
        let registry = SchemaRegistry::new();
        let cal = calibrated_registry();
        let bytes = encode(&Msg::sample());
        let mut backend = CraneliftBackend::new().unwrap();

        match try_compile::<Msg>(&plan, &registry, &cal, &mut backend) {
            Ok(owned_fn) => {
                bencher.bench(|| unsafe { decode_via_stub::<Msg>(owned_fn, &bytes) });
            }
            Err(e) => {
                // [SlowPath]: Msg should compile cleanly — flag if it doesn't
                eprintln!(
                    "[SlowPath] decode_msg: JIT compile failed ({e:?}); using IR interpreter"
                );
                bencher
                    .bench(|| from_slice_ir::<Msg>(&bytes, &plan, &registry, Some(&cal)).unwrap());
            }
        }
    }

    #[divan::bench]
    fn decode_outer(bencher: divan::Bencher) {
        let plan = plan_for::<Outer>();
        let registry = SchemaRegistry::new();
        let cal = calibrated_registry();
        let bytes = encode(&Outer::sample());
        let mut backend = CraneliftBackend::new().unwrap();

        match try_compile::<Outer>(&plan, &registry, &cal, &mut backend) {
            Ok(owned_fn) => {
                bencher.bench(|| unsafe { decode_via_stub::<Outer>(owned_fn, &bytes) });
            }
            Err(_) => {
                eprintln!("[SlowPath] decode_outer: JIT compile failed; using IR interpreter");
                bencher.bench(|| {
                    from_slice_ir::<Outer>(&bytes, &plan, &registry, Some(&cal)).unwrap()
                });
            }
        }
    }

    #[divan::bench(args = [4, 16, 64, 256])]
    fn decode_text_batch(bencher: divan::Bencher, n: usize) {
        let plan = plan_for::<TextBatch>();
        let registry = SchemaRegistry::new();
        let cal = calibrated_registry();
        let bytes = encode(&TextBatch::sample_n(n));
        let mut backend = CraneliftBackend::new().unwrap();

        match try_compile::<TextBatch>(&plan, &registry, &cal, &mut backend) {
            Ok(owned_fn) => {
                bencher.bench(|| unsafe { decode_via_stub::<TextBatch>(owned_fn, &bytes) });
            }
            Err(_) => {
                eprintln!(
                    "[SlowPath] decode_text_batch/{n}: JIT compile failed; using IR interpreter"
                );
                bencher.bench(|| {
                    from_slice_ir::<TextBatch>(&bytes, &plan, &registry, Some(&cal)).unwrap()
                });
            }
        }
    }

    #[divan::bench(args = [16, 64, 256, 1024])]
    fn decode_num_batch(bencher: divan::Bencher, n: usize) {
        let plan = plan_for::<NumBatch>();
        let registry = SchemaRegistry::new();
        let cal = calibrated_registry();
        let bytes = encode(&NumBatch::sample_n(n));
        let mut backend = CraneliftBackend::new().unwrap();

        match try_compile::<NumBatch>(&plan, &registry, &cal, &mut backend) {
            Ok(owned_fn) => {
                bencher.bench(|| unsafe { decode_via_stub::<NumBatch>(owned_fn, &bytes) });
            }
            Err(_) => {
                eprintln!(
                    "[SlowPath] decode_num_batch/{n}: JIT compile failed; using IR interpreter"
                );
                bencher.bench(|| {
                    from_slice_ir::<NumBatch>(&bytes, &plan, &registry, Some(&cal)).unwrap()
                });
            }
        }
    }

    #[divan::bench]
    fn decode_u32(bencher: divan::Bencher) {
        let plan = plan_for::<u32>();
        let registry = SchemaRegistry::new();
        let cal = calibrated_registry();
        let bytes = encode(&100_000u32);
        let mut backend = CraneliftBackend::new().unwrap();

        match try_compile::<u32>(&plan, &registry, &cal, &mut backend) {
            Ok(owned_fn) => {
                bencher.bench(|| unsafe { decode_via_stub::<u32>(owned_fn, &bytes) });
            }
            Err(_) => {
                eprintln!("[SlowPath] decode_u32: JIT compile failed; using IR interpreter");
                bencher
                    .bench(|| from_slice_ir::<u32>(&bytes, &plan, &registry, Some(&cal)).unwrap());
            }
        }
    }

    #[divan::bench]
    fn decode_string(bencher: divan::Bencher) {
        let plan = plan_for::<String>();
        let registry = SchemaRegistry::new();
        let cal = calibrated_registry();
        let bytes = encode(&"hello, world! this is a typical short string".to_string());
        let mut backend = CraneliftBackend::new().unwrap();

        match try_compile::<String>(&plan, &registry, &cal, &mut backend) {
            Ok(owned_fn) => {
                bencher.bench(|| unsafe { decode_via_stub::<String>(owned_fn, &bytes) });
            }
            Err(_) => {
                eprintln!("[SlowPath] decode_string: JIT compile failed; using IR interpreter");
                bencher.bench(|| {
                    from_slice_ir::<String>(&bytes, &plan, &registry, Some(&cal)).unwrap()
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Allocation counting (task #27)
//
// Each bench runs exactly one decode call and reports (alloc_count, alloc_bytes)
// via eprintln so the output is visible in `cargo bench` output.
//
// The thesis: JIT removes the generic-container bookkeeping allocs that the
// reflective path incurs (Partial::push, Vec<FieldInit>, etc.).
//
// The divan bench body runs the decode and prints allocation deltas.
// Wall-clock numbers here are less important than the alloc counts.
// ---------------------------------------------------------------------------

mod alloc_count {
    use super::*;
    use std::mem::MaybeUninit;

    fn report(label: &str, count: u64, bytes: u64) {
        eprintln!("  {label}: {count} allocs, {bytes} bytes");
    }

    unsafe fn decode_via_stub<T>(owned_fn: OwnedDecodeFn, bytes: &[u8]) -> T {
        let mut out = MaybeUninit::<T>::uninit();
        let mut ctx = DecodeCtx::new(bytes);
        let status = unsafe { owned_fn(&mut ctx, out.as_mut_ptr() as *mut u8) };
        assert!(status.is_ok(), "JIT stub returned {status:?}");
        unsafe { out.assume_init() }
    }

    #[divan::bench]
    fn msg_reflective(bencher: divan::Bencher) {
        let plan = plan_for::<Msg>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&Msg::sample());

        bencher.bench(|| {
            let snap = AllocSnapshot::take();
            let v = from_slice_with_plan::<Msg>(&bytes, &plan, &registry).unwrap();
            let (c, b) = snap.delta();
            report("msg/reflective", c, b);
            v
        });
    }

    #[divan::bench]
    fn msg_ir(bencher: divan::Bencher) {
        let plan = plan_for::<Msg>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&Msg::sample());

        bencher.bench(|| {
            let snap = AllocSnapshot::take();
            let v = from_slice_ir::<Msg>(&bytes, &plan, &registry, None).unwrap();
            let (c, b) = snap.delta();
            report("msg/ir", c, b);
            v
        });
    }

    #[divan::bench]
    fn msg_ir_calibrated(bencher: divan::Bencher) {
        let plan = plan_for::<Msg>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&Msg::sample());
        let cal = calibrated_registry();

        bencher.bench(|| {
            let snap = AllocSnapshot::take();
            let v = from_slice_ir::<Msg>(&bytes, &plan, &registry, Some(&cal)).unwrap();
            let (c, b) = snap.delta();
            report("msg/ir-cal", c, b);
            v
        });
    }

    #[divan::bench(args = [1, 4, 16])]
    fn gnarly_reflective(bencher: divan::Bencher, n: usize) {
        let plan = plan_for::<GnarlyPayload>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&make_gnarly_payload(n, 0));

        bencher.bench(|| {
            let snap = AllocSnapshot::take();
            let v = from_slice_with_plan::<GnarlyPayload>(&bytes, &plan, &registry).unwrap();
            let (c, b) = snap.delta();
            report(&format!("gnarly/{n}/reflective"), c, b);
            v
        });
    }

    #[divan::bench(args = [1, 4, 16])]
    fn gnarly_ir(bencher: divan::Bencher, n: usize) {
        let plan = plan_for::<GnarlyPayload>();
        let registry = SchemaRegistry::new();
        let cal = gnarly_registry();
        let bytes = encode(&make_gnarly_payload(n, 0));

        bencher.bench(|| {
            let snap = AllocSnapshot::take();
            let v = from_slice_ir::<GnarlyPayload>(&bytes, &plan, &registry, Some(&cal)).unwrap();
            let (c, b) = snap.delta();
            report(&format!("gnarly/{n}/ir"), c, b);
            v
        });
    }

    #[divan::bench(args = [1, 4, 16])]
    fn gnarly_jit(bencher: divan::Bencher, n: usize) {
        let plan = plan_for::<GnarlyPayload>();
        let registry = SchemaRegistry::new();
        let cal = gnarly_registry();
        let bytes = encode(&make_gnarly_payload(n, 0));
        let mut backend = CraneliftBackend::new().unwrap();
        let program = lower_with_cal(
            &plan,
            GnarlyPayload::SHAPE,
            &registry,
            Some(&cal),
            BorrowMode::Owned,
        )
        .expect("gnarly lower should succeed");
        let owned_fn = backend
            .compile_decode_owned(&program, &cal)
            .expect("gnarly compile should succeed");

        bencher.bench(|| {
            let snap = AllocSnapshot::take();
            let v = unsafe { decode_via_stub::<GnarlyPayload>(owned_fn, &bytes) };
            let (c, b) = snap.delta();
            report(&format!("gnarly/{n}/jit"), c, b);
            v
        });
    }

    #[divan::bench(args = [16, 256])]
    fn num_batch_reflective(bencher: divan::Bencher, n: usize) {
        let plan = plan_for::<NumBatch>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&NumBatch::sample_n(n));

        bencher.bench(|| {
            let snap = AllocSnapshot::take();
            let v = from_slice_with_plan::<NumBatch>(&bytes, &plan, &registry).unwrap();
            let (c, b) = snap.delta();
            report(&format!("num_batch/{n}/reflective"), c, b);
            v
        });
    }

    #[divan::bench(args = [16, 256])]
    fn num_batch_ir(bencher: divan::Bencher, n: usize) {
        let plan = plan_for::<NumBatch>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&NumBatch::sample_n(n));

        bencher.bench(|| {
            let snap = AllocSnapshot::take();
            let v = from_slice_ir::<NumBatch>(&bytes, &plan, &registry, None).unwrap();
            let (c, b) = snap.delta();
            report(&format!("num_batch/{n}/ir"), c, b);
            v
        });
    }

    #[divan::bench(args = [16, 256])]
    fn num_batch_ir_calibrated(bencher: divan::Bencher, n: usize) {
        let plan = plan_for::<NumBatch>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&NumBatch::sample_n(n));
        let cal = calibrated_registry();

        bencher.bench(|| {
            let snap = AllocSnapshot::take();
            let v = from_slice_ir::<NumBatch>(&bytes, &plan, &registry, Some(&cal)).unwrap();
            let (c, b) = snap.delta();
            report(&format!("num_batch/{n}/ir-cal"), c, b);
            v
        });
    }

    #[divan::bench(args = [4, 64])]
    fn text_batch_reflective(bencher: divan::Bencher, n: usize) {
        let plan = plan_for::<TextBatch>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&TextBatch::sample_n(n));

        bencher.bench(|| {
            let snap = AllocSnapshot::take();
            let v = from_slice_with_plan::<TextBatch>(&bytes, &plan, &registry).unwrap();
            let (c, b) = snap.delta();
            report(&format!("text_batch/{n}/reflective"), c, b);
            v
        });
    }

    #[divan::bench(args = [4, 64])]
    fn text_batch_ir(bencher: divan::Bencher, n: usize) {
        let plan = plan_for::<TextBatch>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&TextBatch::sample_n(n));

        bencher.bench(|| {
            let snap = AllocSnapshot::take();
            let v = from_slice_ir::<TextBatch>(&bytes, &plan, &registry, None).unwrap();
            let (c, b) = snap.delta();
            report(&format!("text_batch/{n}/ir"), c, b);
            v
        });
    }

    #[divan::bench(args = [4, 64])]
    fn text_batch_ir_calibrated(bencher: divan::Bencher, n: usize) {
        let plan = plan_for::<TextBatch>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&TextBatch::sample_n(n));
        let cal = calibrated_registry();

        bencher.bench(|| {
            let snap = AllocSnapshot::take();
            let v = from_slice_ir::<TextBatch>(&bytes, &plan, &registry, Some(&cal)).unwrap();
            let (c, b) = snap.delta();
            report(&format!("text_batch/{n}/ir-cal"), c, b);
            v
        });
    }
}

// ---------------------------------------------------------------------------
// Gnarly workload (task #33)
//
// GnarlyPayload is a deep, heterogeneous type: nested structs, enums with
// payload, Vec<Vec<u8>>, Option<String>, etc. It exercises every IR path.
//
// Calibration is done via `get_or_calibrate_by_shape` walking the shape tree
// so we don't need hand-written per-type registrations.
// ---------------------------------------------------------------------------

/// Walk a shape tree and pre-register all List/Pointer shapes via on-demand
/// calibration. Called before `lower_with_cal` so `lookup_by_shape` hits.
fn register_shape_tree(shape: &'static Shape, cal: &mut CalibrationRegistry) {
    use facet_core::{Def, Type, UserType};

    match shape.def {
        Def::List(_) | Def::Pointer(_) => {
            cal.get_or_calibrate_by_shape(shape);
        }
        _ => {}
    }

    match shape.ty {
        Type::User(UserType::Struct(st)) => {
            for field in st.fields {
                register_shape_tree(field.shape(), cal);
            }
        }
        Type::User(UserType::Enum(et)) => {
            for variant in et.variants {
                for field in variant.data.fields {
                    register_shape_tree(field.shape(), cal);
                }
            }
        }
        _ => {}
    }

    // Recurse into inner shapes for Option/List/Pointer/Array.
    match shape.def {
        Def::Option(opt) => register_shape_tree(opt.t, cal),
        Def::List(list) => register_shape_tree(list.t, cal),
        Def::Pointer(ptr) => {
            if let Some(inner) = ptr.pointee() {
                register_shape_tree(inner, cal);
            }
        }
        Def::Array(arr) => register_shape_tree(arr.t, cal),
        _ => {}
    }
}

fn gnarly_registry() -> CalibrationRegistry {
    let mut cal = CalibrationRegistry::default();
    // Pre-register common primitives first, then walk the full GnarlyPayload tree.
    cal.calibrate_string_for_type();
    cal.calibrate_vec_for_type::<u8>();
    register_shape_tree(GnarlyPayload::SHAPE, &mut cal);
    cal
}

fn make_gnarly_payload(entry_count: usize, seq: usize) -> GnarlyPayload {
    let entries = (0..entry_count)
        .map(|i| {
            let attrs = vec![
                GnarlyAttr {
                    key: "owner".to_string(),
                    value: format!("user-{seq}-{i}"),
                },
                GnarlyAttr {
                    key: "class".to_string(),
                    value: format!("hot-path-{}", (seq + i) % 17),
                },
                GnarlyAttr {
                    key: "etag".to_string(),
                    value: format!("etag-{seq:08x}-{i:08x}"),
                },
            ];
            let chunks = (0..3)
                .map(|j| {
                    let len = 32 * (j + 1);
                    vec![((seq + i + j) & 0xff) as u8; len]
                })
                .collect();
            let kind = match i % 3 {
                0 => GnarlyKind::File {
                    mime: "application/octet-stream".to_string(),
                    tags: vec![
                        "warm".to_string(),
                        "cacheable".to_string(),
                        format!("tag-{seq}-{i}"),
                    ],
                },
                1 => GnarlyKind::Directory {
                    child_count: i as u32 + 3,
                    children: vec![
                        format!("child-{seq}-{i}-0"),
                        format!("child-{seq}-{i}-1"),
                        format!("child-{seq}-{i}-2"),
                    ],
                },
                _ => GnarlyKind::Symlink {
                    target: format!("/target/{seq}/{i}/nested/item"),
                    hops: vec![1, 2, 3, i as u32],
                },
            };
            GnarlyEntry {
                id: seq as u64 * 1_000_000 + i as u64,
                parent: if i == 0 {
                    None
                } else {
                    Some(seq as u64 * 1_000_000 + i as u64 - 1)
                },
                name: format!("entry-{seq}-{i}"),
                path: format!("/mount/very/deep/path/with/component/{seq}/{i}/file.bin"),
                attrs,
                chunks,
                kind,
            }
        })
        .collect();
    GnarlyPayload {
        revision: seq as u64,
        mount: format!("/mnt/bench-fast-path-{seq:08x}"),
        entries,
        footer: Some(format!("benchmark footer {seq}")),
        digest: vec![(seq & 0xff) as u8; 64],
    }
}

mod gnarly {
    use super::*;

    fn try_lower_gnarly(
        plan: &TranslationPlan,
        registry: &SchemaRegistry,
        cal: &CalibrationRegistry,
    ) -> Result<vox_postcard::ir::DecodeProgram, String> {
        lower_with_cal(
            plan,
            GnarlyPayload::SHAPE,
            registry,
            Some(cal),
            BorrowMode::Owned,
        )
        .map_err(|e| format!("{e:?}"))
    }

    fn try_compile_gnarly(
        plan: &TranslationPlan,
        registry: &SchemaRegistry,
        cal: &CalibrationRegistry,
        backend: &mut CraneliftBackend,
    ) -> Result<OwnedDecodeFn, CodegenError> {
        let program = lower_with_cal(
            plan,
            GnarlyPayload::SHAPE,
            registry,
            Some(cal),
            BorrowMode::Owned,
        )
        .map_err(|e| CodegenError::UnsupportedOp(format!("{e:?}")))?;
        let owned = backend.compile_decode_owned(&program, cal)?;
        Ok(owned)
    }

    unsafe fn decode_gnarly_via_stub(owned_fn: OwnedDecodeFn, bytes: &[u8]) -> GnarlyPayload {
        let mut out = std::mem::MaybeUninit::<GnarlyPayload>::uninit();
        let mut ctx = DecodeCtx::new(bytes);
        let status = unsafe { owned_fn(&mut ctx, out.as_mut_ptr() as *mut u8) };
        assert!(status.is_ok(), "JIT stub returned {status:?}");
        unsafe { out.assume_init() }
    }

    #[divan::bench(args = [1, 4, 16])]
    fn reflective(bencher: divan::Bencher, n: usize) {
        let plan = plan_for::<GnarlyPayload>();
        let registry = SchemaRegistry::new();
        let bytes = encode(&make_gnarly_payload(n, 0));

        bencher.bench(|| from_slice_with_plan::<GnarlyPayload>(&bytes, &plan, &registry).unwrap());
    }

    #[divan::bench(args = [1, 4, 16])]
    fn ir_interp(bencher: divan::Bencher, n: usize) {
        let plan = plan_for::<GnarlyPayload>();
        let registry = SchemaRegistry::new();
        let cal = gnarly_registry();
        let bytes = encode(&make_gnarly_payload(n, 0));

        // Probe lowering: if any shape hits SlowPath at lower time (e.g. unstable
        // enum repr), fall back to the reflective interpreter and name the blocker.
        match try_lower_gnarly(&plan, &registry, &cal) {
            Ok(_) => {
                bencher.bench(|| {
                    from_slice_ir::<GnarlyPayload>(&bytes, &plan, &registry, Some(&cal)).unwrap()
                });
            }
            Err(e) => {
                eprintln!("[SlowPath] gnarly/ir_interp/{n}: lower failed ({e}); using reflective");
                bencher.bench(|| {
                    from_slice_with_plan::<GnarlyPayload>(&bytes, &plan, &registry).unwrap()
                });
            }
        }
    }

    #[divan::bench(args = [1, 4, 16])]
    fn jit(bencher: divan::Bencher, n: usize) {
        let plan = plan_for::<GnarlyPayload>();
        let registry = SchemaRegistry::new();
        let cal = gnarly_registry();
        let bytes = encode(&make_gnarly_payload(n, 0));
        let mut backend = CraneliftBackend::new().unwrap();

        match try_compile_gnarly(&plan, &registry, &cal, &mut backend) {
            Ok(owned_fn) => {
                bencher.bench(|| unsafe { decode_gnarly_via_stub(owned_fn, &bytes) });
            }
            Err(e) => {
                eprintln!(
                    "[SlowPath] gnarly/jit/{n}: JIT compile failed ({e:?}); using reflective"
                );
                bencher.bench(|| {
                    from_slice_with_plan::<GnarlyPayload>(&bytes, &plan, &registry).unwrap()
                });
            }
        }
    }
}
