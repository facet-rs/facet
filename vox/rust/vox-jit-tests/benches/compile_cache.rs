//! Task #28: Stub compile time + cache behavior.
//!
//! Measures three things for each root shape:
//!
//!   (a) Cold compile latency — `lower_with_cal` + `compile_decode` from scratch.
//!       This is what the first caller pays per distinct type.
//!
//!   (b) Cache hit cost — a `HashMap::get` lookup by `DecodeCacheKey` at steady
//!       state. This is what every subsequent caller pays.
//!
//!   (c) Machine-code size in bytes per stub — reported via divan's counter API.
//!       Tells you whether the JIT is generating compact or bloated code.
//!
//! Run with:
//!   cargo bench -p vox-jit-tests --bench compile_cache
//!
//! Or in --test mode for a quick sanity check:
//!   cargo bench -p vox-jit-tests --bench compile_cache -- --test

use std::collections::HashMap;

use facet::Facet;
use vox_jit::{CraneliftBackend, abi::OwnedDecodeFn, host_isa_name};
use vox_jit_abi::DecodeCacheKey;
use vox_jit_cal::{BorrowMode, CalibrationRegistry};
use vox_postcard::{TranslationPlan, build_identity_plan, ir::lower_with_cal};
use vox_schema::SchemaRegistry;

fn main() {
    divan::main();
}

// ---------------------------------------------------------------------------
// Shared fixtures
// ---------------------------------------------------------------------------

#[derive(Facet, Debug)]
struct Msg {
    id: u64,
    seq: u32,
    flags: u16,
    kind: u8,
}

#[derive(Facet, Debug)]
struct NumBatch {
    values: Vec<u32>,
}

#[derive(Facet, Debug)]
struct Inner {
    x: i32,
    label: String,
}

#[derive(Facet, Debug)]
struct Outer {
    name: String,
    inner: Inner,
    count: u32,
}

#[derive(Facet, Debug)]
struct TextBatch {
    topic: String,
    lines: Vec<String>,
}

fn cal() -> CalibrationRegistry {
    let mut c = CalibrationRegistry::default();
    c.calibrate_string_for_type();
    c.calibrate_vec_for_type::<u32>();
    c.calibrate_vec_for_type::<u8>();
    c.calibrate_vec_for_type::<String>();
    c
}

fn registry() -> SchemaRegistry {
    SchemaRegistry::new()
}

fn compile_one<T: Facet<'static>>(
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
    cal: &CalibrationRegistry,
    backend: &mut CraneliftBackend,
) -> (OwnedDecodeFn, u32) {
    let program = lower_with_cal(plan, T::SHAPE, registry, Some(cal), BorrowMode::Owned)
        .expect("lower_with_cal failed");
    let (owned, code_bytes) = backend
        .compile_decode_with_size(&program, cal)
        .expect("compile_decode failed");
    (owned, code_bytes)
}

// ---------------------------------------------------------------------------
// (a) Cold compile latency
//
// Each bench iteration does the full pipeline: lower_with_cal + compile_decode.
// The backend is created once outside the loop (its setup cost is ~1 ms and
// belongs to process init, not per-type compile cost).
//
// bench_local is used because CraneliftBackend / JITModule is not Sync.
// ---------------------------------------------------------------------------

mod cold_compile {
    use super::*;

    #[divan::bench]
    fn msg(bencher: divan::Bencher) {
        let plan = build_identity_plan(Msg::SHAPE);
        let reg = registry();
        let cal = cal();
        // Measure code size once up front to report as a counter.
        let mut probe = CraneliftBackend::new().unwrap();
        let (_, code_bytes) = compile_one::<Msg>(&plan, &reg, &cal, &mut probe);

        let mut backend = CraneliftBackend::new().unwrap();
        bencher
            .counter(divan::counter::BytesCount::new(code_bytes as u64))
            .bench_local(|| compile_one::<Msg>(&plan, &reg, &cal, &mut backend));
    }

    #[divan::bench]
    fn num_batch(bencher: divan::Bencher) {
        let plan = build_identity_plan(NumBatch::SHAPE);
        let reg = registry();
        let cal = cal();
        let mut probe = CraneliftBackend::new().unwrap();
        let (_, code_bytes) = compile_one::<NumBatch>(&plan, &reg, &cal, &mut probe);

        let mut backend = CraneliftBackend::new().unwrap();
        bencher
            .counter(divan::counter::BytesCount::new(code_bytes as u64))
            .bench_local(|| compile_one::<NumBatch>(&plan, &reg, &cal, &mut backend));
    }

    #[divan::bench]
    fn outer(bencher: divan::Bencher) {
        let plan = build_identity_plan(Outer::SHAPE);
        let reg = registry();
        let cal = cal();
        let mut probe = CraneliftBackend::new().unwrap();
        let (_, code_bytes) = compile_one::<Outer>(&plan, &reg, &cal, &mut probe);

        let mut backend = CraneliftBackend::new().unwrap();
        bencher
            .counter(divan::counter::BytesCount::new(code_bytes as u64))
            .bench_local(|| compile_one::<Outer>(&plan, &reg, &cal, &mut backend));
    }

    #[divan::bench]
    fn text_batch(bencher: divan::Bencher) {
        let plan = build_identity_plan(TextBatch::SHAPE);
        let reg = registry();
        let cal = cal();
        let mut probe = CraneliftBackend::new().unwrap();
        let (_, code_bytes) = compile_one::<TextBatch>(&plan, &reg, &cal, &mut probe);

        let mut backend = CraneliftBackend::new().unwrap();
        bencher
            .counter(divan::counter::BytesCount::new(code_bytes as u64))
            .bench_local(|| compile_one::<TextBatch>(&plan, &reg, &cal, &mut backend));
    }
}

// ---------------------------------------------------------------------------
// (b) Cache hit cost
//
// Simulates a `HashMap<DecodeCacheKey, OwnedDecodeFn>` lookup — what the
// runtime does on every decode once the stub is compiled. Measures the
// hash + equality check overhead at various cache sizes.
// ---------------------------------------------------------------------------

mod cache_hit {
    use super::*;

    // Each entry uses a distinct remote_schema_id; local_shape is fixed (Msg).
    // This matches real cache usage: one shape looked up in a multi-entry cache.
    fn make_key(remote_schema_id: u64) -> DecodeCacheKey {
        DecodeCacheKey {
            remote_schema_id,
            local_shape: Msg::SHAPE,
            borrow_mode: BorrowMode::Owned,
            target_isa: host_isa_name(),
            descriptor_handle: None,
        }
    }

    #[divan::bench(args = [1, 4, 16, 64])]
    fn lookup_hit(bencher: divan::Bencher, cache_size: u64) {
        let mut map: HashMap<DecodeCacheKey, u64> = HashMap::new();
        for i in 0..cache_size {
            map.insert(make_key(i), i);
        }
        let target = make_key(cache_size / 2);

        bencher.bench(|| std::hint::black_box(map.get(&target)));
    }

    #[divan::bench(args = [1, 4, 16, 64])]
    fn lookup_miss(bencher: divan::Bencher, cache_size: u64) {
        let mut map: HashMap<DecodeCacheKey, u64> = HashMap::new();
        for i in 0..cache_size {
            map.insert(make_key(i), i);
        }
        let absent = make_key(cache_size + 9999);

        bencher.bench(|| std::hint::black_box(map.get(&absent)));
    }
}

// ---------------------------------------------------------------------------
// (c) Code size summary
//
// Single-shot: compile each shape once and print its code size in bytes.
// In --test mode this prints to stdout; in bench mode it just runs once.
// ---------------------------------------------------------------------------

mod code_size {
    use super::*;

    fn measure<T: Facet<'static>>(label: &str, backend: &mut CraneliftBackend) {
        let plan = build_identity_plan(T::SHAPE);
        let reg = registry();
        let cal = cal();
        let program = lower_with_cal(&plan, T::SHAPE, &reg, Some(&cal), BorrowMode::Owned)
            .expect("lower_with_cal failed");
        let (_owned, bytes) = backend
            .compile_decode_with_size(&program, &cal)
            .expect("compile failed");
        println!("code_size/{label}: {bytes} bytes");
    }

    #[divan::bench]
    fn print_sizes(bencher: divan::Bencher) {
        let mut backend = CraneliftBackend::new().unwrap();
        bencher.bench_local(|| {
            measure::<Msg>("Msg", &mut backend);
            measure::<NumBatch>("NumBatch", &mut backend);
            measure::<Outer>("Outer", &mut backend);
            measure::<TextBatch>("TextBatch", &mut backend);
        });
    }
}
