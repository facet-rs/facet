//! Same ManyVariants V0 hot loop as `profile_many_variants_v0.rs`, but using
//! the pre-resolved direct-call API (`vox_jit::decode_owned_with` with a
//! cached `&'static CompiledDecoder`). All entry overhead — codec-mode env
//! check, cache lookup, ArcSwap load — is hoisted out of the loop.
//!
//! Comparing this to `profile_many_variants_v0.rs` shows what fraction of the
//! per-call cost is dispatch glue vs. real decode work.
use std::time::{Duration, Instant};

use divan::black_box;
use facet::Facet;
use vox_bench::shapes::{ManyVariants, make_many_variants};
use vox_jit::cal::BorrowMode;
use vox_types::SchemaRegistry;

fn main() {
    let value = make_many_variants(0);
    let bytes = vox_postcard::to_vec(&value).expect("encode");
    let plan = vox_postcard::build_identity_plan(<ManyVariants as Facet<'static>>::SHAPE);
    let registry = SchemaRegistry::new();

    let runtime = vox_jit::global_runtime();
    let decoder = runtime
        .prepare_decoder(
            0,
            <ManyVariants as Facet<'static>>::SHAPE,
            &plan,
            &registry,
            BorrowMode::Owned,
        )
        .expect("prepare");
    // Warm.
    let _: ManyVariants = vox_jit::decode_owned_with(decoder, &bytes).unwrap();

    let secs = std::env::var("PROFILE_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(8);
    let deadline = Instant::now() + Duration::from_secs(secs);
    eprintln!("profiling ManyVariants V0 decode_owned_with for {secs}s — pid {}", std::process::id());

    let mut iters: u64 = 0;
    while Instant::now() < deadline {
        for _ in 0..10_000 {
            let v: ManyVariants =
                vox_jit::decode_owned_with(decoder, black_box(&bytes)).unwrap();
            black_box(&v);
        }
        iters += 10_000;
    }
    eprintln!("done — {iters} iterations");
}
