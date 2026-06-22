//! PHON typed construction benchmarks.
//!
//! These measure the path Vox-style callers cache before steady-state
//! encode/decode: Facet derivation, typed lowering, canonical effect stats, and
//! public `Codec::new()` construction.
//!
//! Run: `cargo run -p phon --release --features jit --example construction_bench`

use std::hint::black_box;
use std::time::Instant;

use facet::Facet;
use phon::api::Codec;
use phon_engine::{Registry, typed};
use phon_ir::{
    CanonicalMemLowered, Lowered, canonical_mem_lowered, canonical_mem_lowered_effect_stats,
};

#[derive(Debug, Clone, PartialEq, Facet)]
struct FlatScalars {
    sequence: u64,
    timestamp_ms: u64,
    confidence: f32,
    stable: bool,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct TranscriptChunk {
    text: String,
    utf16_len: u32,
    samples: Vec<f32>,
    alternatives: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct Timing {
    start_ms: u32,
    end_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum EditKind {
    Insert,
    Delete,
    Replace,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct CorrectionEdit {
    kind: EditKind,
    timing: Option<Timing>,
    original: String,
    replacement: String,
    ranker_score: f64,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum EngineError {
    SessionMissing { session_id: String },
    DecodeFailed { message: String },
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct EnginePayload {
    chunk: TranscriptChunk,
    edits: Vec<CorrectionEdit>,
    final_chunk: bool,
}

type EngineResponse = Result<Option<EnginePayload>, EngineError>;

fn bench(label: &str, iters: u64, mut f: impl FnMut()) -> f64 {
    let warmup = (iters / 20).clamp(10, 1_000);
    for _ in 0..warmup {
        f();
    }

    let started = Instant::now();
    for _ in 0..iters {
        f();
    }
    let ns = started.elapsed().as_nanos() as f64 / iters as f64;
    println!("  {label:<30} {ns:>10.1} ns/op");
    ns
}

fn lower_from_derived(derived: &phon::derive::Derived) -> Lowered {
    let reg = Registry::new(derived.schemas.clone());
    typed::lower_typed(&derived.descriptor, &derived.descriptor_blocks, &reg)
        .expect("construction bench shape should lower")
}

fn bench_shape<T>(label: &str, iters: u64)
where
    T: for<'facet> Facet<'facet>,
{
    println!("{label}");

    let derived = phon::derive::of::<T>().expect("construction bench shape should derive");
    let lowered = lower_from_derived(&derived);
    let canonical: CanonicalMemLowered<_> = canonical_mem_lowered(lowered.clone());

    bench("derive", iters, || {
        let derived = phon::derive::of::<T>().expect("construction bench shape should derive");
        black_box(derived.schemas.len());
    });
    bench("lower_typed", iters, || {
        let lowered = lower_from_derived(black_box(&derived));
        black_box(lowered.program.len());
        black_box(lowered.blocks.len());
    });
    bench("canonical effect stats", iters, || {
        let stats = canonical_mem_lowered_effect_stats(black_box(&canonical));
        black_box(stats.total.op_count);
        black_box(stats.total.opaque_count);
    });
    bench("Codec::new", (iters / 200).max(50), || {
        let codec = Codec::<T>::new().expect("construction bench codec should build");
        black_box(codec);
    });

    println!();
}

fn main() {
    println!("PHON typed construction throughput\n");
    bench_shape::<FlatScalars>("flat scalars", 100_000);
    bench_shape::<TranscriptChunk>("chunk with lists", 50_000);
    bench_shape::<EngineResponse>("engine response", 20_000);
}
