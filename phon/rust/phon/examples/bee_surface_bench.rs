//! Bee-shaped typed codec benchmarks: interpreter vs native copy-and-patch JIT.
//!
//! These payloads mirror the hot Vox/Bee method roots used by the JIT surface
//! audit. The measurement is steady-state encode/decode after deriving,
//! lowering, and native compilation; Vox caches that work outside the RPC hot
//! path.
//!
//! Run: `cargo run -p phon --release --features jit --example bee_surface_bench`

use std::fmt::Debug;
use std::hint::black_box;
use std::mem::MaybeUninit;
use std::time::Instant;

use facet::Facet;
use phon_engine::{Registry, typed};
use phon_ir::Lowered;

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum BeeError {
    EngineNotLoaded,
    SessionNotFound { session_id: String },
    LoadFailed { message: String },
    TranscriptionError { message: String },
    CorrectionError { message: String },
    NotImplemented,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct Confidence {
    mean_lp: f32,
    min_lp: f32,
    mean_m: f32,
    min_m: f32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct AlignedWord {
    word: String,
    start: f64,
    end: f64,
    confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct CorrectionEdit {
    edit_id: String,
    span_start: u32,
    span_end: u32,
    original: String,
    replacement: String,
    term: String,
    alias_id: i32,
    ranker_prob: f64,
    gate_prob: f64,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct FeedResult {
    text: String,
    committed_utf16_len: u32,
    alignments: Vec<AlignedWord>,
    is_final: bool,
    detected_language: String,
    correction_edits: Vec<CorrectionEdit>,
    correction_session_id: String,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct FeedArgs {
    session_id: String,
    samples: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct SetMarkedTextArgs {
    text: String,
    animation_budget_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct AdvanceTranscriptArgs {
    text: String,
    committed_len: u32,
    animation_budget_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct ImeKeyEventArgs {
    event_type: String,
    key_code: u32,
    characters: String,
}

fn lower<'facet, T>() -> Lowered
where
    T: Facet<'facet>,
{
    let derived = phon::derive::of::<T>().expect("Bee bench shape should derive");
    let reg = Registry::new(derived.schemas.clone());
    typed::lower_typed(&derived.descriptor, &derived.descriptor_blocks, &reg)
        .expect("Bee bench shape should lower")
}

fn encode_interp(lowered: &Lowered, base: *const u8) {
    let bytes = unsafe { typed::encode_with(lowered, base) };
    black_box(bytes);
}

fn decode_interp<T>(lowered: &Lowered, wire: &[u8]) -> T {
    let mut slot = MaybeUninit::<T>::uninit();
    unsafe { typed::decode_with(lowered, wire, slot.as_mut_ptr().cast::<u8>()) }
        .expect("interpreter decode should succeed");
    unsafe { slot.assume_init() }
}

#[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
fn decode_jit<T>(jit: &phon_jit::native::NativeDecode, wire: &[u8]) -> T {
    let mut slot = MaybeUninit::<T>::uninit();
    unsafe { jit.run(wire, slot.as_mut_ptr().cast::<u8>()) }.expect("native decode should succeed");
    unsafe { slot.assume_init() }
}

fn bench(label: &str, iters: u64, mut f: impl FnMut()) -> f64 {
    for _ in 0..(iters / 20).max(1_000) {
        f();
    }

    let started = Instant::now();
    for _ in 0..iters {
        f();
    }
    let ns = started.elapsed().as_nanos() as f64 / iters as f64;
    println!("  {label:<22} {ns:>10.1} ns/op");
    ns
}

fn bench_case<'facet, T>(label: &str, value: T, iters: u64)
where
    T: Facet<'facet> + PartialEq + Debug,
{
    let lowered = lower::<T>();
    let base = core::ptr::from_ref(&value).cast::<u8>();
    let wire = unsafe { typed::encode_with(&lowered, base) };
    let decoded = decode_interp::<T>(&lowered, &wire);
    assert_eq!(decoded, value, "{label}: interpreter round-trip mismatch");

    println!("{label}  ->  {} wire bytes", wire.len());

    let enc_i = bench("encode interpreter", iters, || {
        encode_interp(&lowered, base)
    });
    let dec_i = bench("decode interpreter", iters, || {
        let decoded = decode_interp::<T>(&lowered, &wire);
        black_box(decoded);
    });

    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    {
        use phon_jit::native::{NativeDecode, NativeEncode};

        let native_encode = NativeEncode::compile_lowered(&lowered);
        let native_decode = NativeDecode::compile_lowered(&lowered);

        let jit_wire = unsafe { native_encode.run(base) };
        assert_eq!(jit_wire, wire, "{label}: native encode mismatch");

        let decoded = decode_jit::<T>(&native_decode, &wire);
        assert_eq!(decoded, value, "{label}: native decode mismatch");

        let enc_j = bench("encode jit", iters, || {
            let bytes = unsafe { native_encode.run(base) };
            black_box(bytes);
        });
        let dec_j = bench("decode jit", iters, || {
            let decoded = decode_jit::<T>(&native_decode, &wire);
            black_box(decoded);
        });

        println!(
            "  speedup               encode {:>5.2}x   decode {:>5.2}x\n",
            enc_i / enc_j,
            dec_i / dec_j
        );
    }

    #[cfg(not(all(feature = "jit", target_os = "macos", target_arch = "aarch64")))]
    {
        let _ = (enc_i, dec_i);
        println!("  native JIT unavailable for this build target\n");
    }
}

fn make_feed_args() -> FeedArgs {
    FeedArgs {
        session_id: "bee-session-hot-path".to_string(),
        samples: (0..4096)
            .map(|i| ((i as f32) * 0.003_906_25).sin())
            .collect(),
    }
}

fn make_feed_response() -> Result<Option<FeedResult>, BeeError> {
    let alignments = (0..24)
        .map(|i| AlignedWord {
            word: format!("word-{i:02}"),
            start: i as f64 * 0.08,
            end: i as f64 * 0.08 + 0.07,
            confidence: Confidence {
                mean_lp: -0.25,
                min_lp: -0.9,
                mean_m: 0.85,
                min_m: 0.42,
            },
        })
        .collect();

    let correction_edits = (0..6)
        .map(|i| CorrectionEdit {
            edit_id: format!("edit-{i:02}"),
            span_start: i * 3,
            span_end: i * 3 + 2,
            original: format!("raw phrase {i}"),
            replacement: format!("corrected phrase {i}"),
            term: format!("term-{i}"),
            alias_id: i as i32,
            ranker_prob: 0.72 + i as f64 * 0.01,
            gate_prob: 0.81 + i as f64 * 0.01,
        })
        .collect();

    Ok(Some(FeedResult {
        text: "bee hot path transcript with a handful of aligned words".to_string(),
        committed_utf16_len: 54,
        alignments,
        is_final: false,
        detected_language: "en".to_string(),
        correction_edits,
        correction_session_id: "corr-session-0001".to_string(),
    }))
}

fn make_marked_text_args() -> SetMarkedTextArgs {
    SetMarkedTextArgs {
        text: "partial dictated text".to_string(),
        animation_budget_ms: 90,
    }
}

fn make_advance_transcript_args() -> AdvanceTranscriptArgs {
    AdvanceTranscriptArgs {
        text: "committed prefix plus live dictated suffix".to_string(),
        committed_len: 17,
        animation_budget_ms: 120,
    }
}

fn make_key_event_args() -> ImeKeyEventArgs {
    ImeKeyEventArgs {
        event_type: "keyDown".to_string(),
        key_code: 49,
        characters: " ".to_string(),
    }
}

fn main() {
    println!("Bee/Vox typed codec steady-state throughput\n");
    bench_case(
        "feed(args): String + [f32; 4096]",
        make_feed_args(),
        120_000,
    );
    bench_case(
        "feed(response): Result<Option<FeedResult>, BeeError>",
        make_feed_response(),
        90_000,
    );
    bench_case("setMarkedText(args)", make_marked_text_args(), 500_000);
    bench_case(
        "advanceTranscript(args)",
        make_advance_transcript_args(),
        500_000,
    );
    bench_case("imeKeyEvent(args)", make_key_event_args(), 500_000);
}
