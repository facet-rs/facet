//! Compare `try_decode_owned` (public API, full cache lookup) vs.
//! `decode_owned_with` (pre-resolved `&'static CompiledDecoder`) across all
//! 16 ManyVariants shapes. Each variant has a different payload size, so
//! this teases apart fixed dispatch overhead from real decode work.
//!
//! Run:
//!     cargo run --release --example many_variants_pre_curve -p vox-bench
use std::time::{Duration, Instant};

use divan::black_box;
use facet::Facet;
use vox_bench::shapes::{ManyVariants, make_many_variants};
use vox_jit::cal::BorrowMode;
use vox_types::SchemaRegistry;

fn bench(label: &str, body: impl Fn() -> u64) -> (u64, f64) {
    let secs = std::env::var("PROFILE_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(2);
    let deadline = Instant::now() + Duration::from_secs(secs);
    let start = Instant::now();
    let mut total: u64 = 0;
    while Instant::now() < deadline {
        total += body();
    }
    let elapsed = start.elapsed().as_secs_f64();
    let ns_per = (elapsed * 1e9) / total as f64;
    eprintln!("  {label:>20}  {total:>12} iters  {ns_per:>6.1} ns/iter");
    (total, ns_per)
}

fn main() {
    let plan = vox_postcard::build_identity_plan(<ManyVariants as Facet<'static>>::SHAPE);
    let registry = SchemaRegistry::new();
    let runtime = vox_jit::global_runtime();
    let decoder = runtime
        .prepare_decoder(0, <ManyVariants as Facet<'static>>::SHAPE, &plan, &registry, BorrowMode::Owned)
        .expect("prepare");

    println!(
        "{:<3} {:>6}  {:>13}  {:>13}  {:>13}",
        "var", "bytes", "try_decode_owned", "decode_owned_with", "delta (glue)"
    );
    for v in 0u32..16 {
        let value = make_many_variants(v);
        let bytes = vox_postcard::to_vec(&value).expect("encode");
        // Warm.
        let _: ManyVariants = vox_jit::decode_owned_with(decoder, &bytes).unwrap();
        let _: ManyVariants = runtime
            .try_decode_owned::<ManyVariants>(&bytes, 0, &plan, &registry)
            .unwrap()
            .unwrap();

        eprintln!("V{v:02} ({} bytes):", bytes.len());
        let (_, ns_pub) = bench("try_decode_owned", || {
            for _ in 0..10_000 {
                let v: ManyVariants = runtime
                    .try_decode_owned::<ManyVariants>(
                        black_box(&bytes),
                        0,
                        black_box(&plan),
                        black_box(&registry),
                    )
                    .unwrap()
                    .unwrap();
                black_box(&v);
            }
            10_000
        });
        let (_, ns_pre) = bench("decode_owned_with", || {
            for _ in 0..10_000 {
                let v: ManyVariants =
                    vox_jit::decode_owned_with(decoder, black_box(&bytes)).unwrap();
                black_box(&v);
            }
            10_000
        });

        println!(
            "V{v:02} {:>6}  {:>11.1} ns  {:>11.1} ns  {:>10.1} ns",
            bytes.len(),
            ns_pub,
            ns_pre,
            ns_pub - ns_pre
        );
    }
}
