//! Hot-loop driver for `nperf record` + `nperf annotate`.
//!
//! Decodes the ManyVariants V0 fixture (1-byte payload — pure entry overhead)
//! in a tight loop for a fixed duration. Run under nperf to get
//! per-instruction sample counts inside `try_decode_owned` / `prepare_decoder`
//! and the JIT'd decoder, and find where ~50ns of dispatch overhead lands.
//!
//! Usage:
//!     cargo build --release --example profile_many_variants_v0 --manifest-path rust/vox-bench/Cargo.toml
//!     ~/nperf/target/release/nperf record \
//!         -F 4000 -l 6 \
//!         -p "$(pgrep -f profile_many_variants_v0)" \
//!         -o /tmp/mv-v0.nperf
//!     ~/nperf/target/release/nperf annotate /tmp/mv-v0.nperf \
//!         --jitdump <jitdump-from-VOX_JIT_PERF_DUMP> \
//!         -f try_decode_owned -f prepare_decoder
use std::time::{Duration, Instant};

use divan::black_box;
use facet::Facet;
use vox_bench::shapes::{ManyVariants, make_many_variants};
use vox_types::SchemaRegistry;

fn main() {
    let value = make_many_variants(0);
    let bytes = vox_postcard::to_vec(&value).expect("encode");
    let plan = vox_postcard::build_identity_plan(<ManyVariants as Facet<'static>>::SHAPE);
    let registry = SchemaRegistry::new();

    // Warm the JIT cache so compile-on-first-call doesn't pollute the profile.
    let _: ManyVariants = vox_jit::global_runtime()
        .try_decode_owned::<ManyVariants>(&bytes, 0, &plan, &registry)
        .unwrap()
        .unwrap();

    let secs = std::env::var("PROFILE_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(8);
    let deadline = Instant::now() + Duration::from_secs(secs);
    eprintln!("profiling ManyVariants V0 jit_decode for {secs}s — pid {}", std::process::id());

    let mut iters: u64 = 0;
    while Instant::now() < deadline {
        for _ in 0..10_000 {
            let v: ManyVariants = vox_jit::global_runtime()
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
        iters += 10_000;
    }
    eprintln!("done — {iters} iterations");
}
