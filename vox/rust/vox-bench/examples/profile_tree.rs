//! Tight-loop Tree decode profiler for use with `nperf record` + `nperf
//! annotate`. Pre-resolves the JIT decoder so the loop body is just one
//! indirect call into the JIT-compiled stub.
//!
//! Usage:
//!   VOX_JIT_PERF=1 nperf record -p $(pgrep profile_tree) -o tree.nperf
//!   nperf annotate --jitdump=/tmp/jit-*.dump tree.nperf
use std::time::{Duration, Instant};

use divan::black_box;
use facet::Facet;
use vox_bench::shapes::{Tree, make_tree};
use vox_jit::cal::BorrowMode;
use vox_types::SchemaRegistry;

fn main() {
    let depth: u32 = std::env::var("TREE_DEPTH")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8);
    let value = make_tree(depth, 0xC0FFEE);
    let bytes = vox_postcard::to_vec(&value).expect("encode");
    let plan = vox_postcard::build_identity_plan(<Tree as Facet<'static>>::SHAPE);
    let registry = SchemaRegistry::new();

    let codec = std::env::var("TREE_CODEC").unwrap_or_else(|_| "jit".to_string());
    let secs = std::env::var("PROFILE_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(10);

    let mut iters: u64 = 0;
    match codec.as_str() {
        "jit" => {
            let runtime = vox_jit::global_runtime();
            let decoder = runtime
                .prepare_decoder(
                    0,
                    <Tree as Facet<'static>>::SHAPE,
                    &plan,
                    &registry,
                    BorrowMode::Owned,
                )
                .expect("prepare");

            // Warm.
            let _: Tree = vox_jit::decode_owned_with(decoder, &bytes).unwrap();

            let deadline = Instant::now() + Duration::from_secs(secs);
            eprintln!(
                "profiling Tree jit decode_owned_with depth={depth} for {secs}s — pid {}, bytes={}",
                std::process::id(),
                bytes.len()
            );

            while Instant::now() < deadline {
                for _ in 0..1_000 {
                    let v: Tree = vox_jit::decode_owned_with(decoder, black_box(&bytes)).unwrap();
                    black_box(&v);
                }
                iters += 1_000;
            }
        }
        "serde" => {
            // Warm.
            let _: Tree = postcard::from_bytes(&bytes).unwrap();

            let deadline = Instant::now() + Duration::from_secs(secs);
            eprintln!(
                "profiling Tree serde postcard::from_bytes depth={depth} for {secs}s — pid {}, bytes={}",
                std::process::id(),
                bytes.len()
            );

            while Instant::now() < deadline {
                for _ in 0..1_000 {
                    let v: Tree = postcard::from_bytes(black_box(&bytes)).unwrap();
                    black_box(&v);
                }
                iters += 1_000;
            }
        }
        other => panic!("unsupported TREE_CODEC={other:?}; expected `jit` or `serde`"),
    }
    eprintln!("done — {iters} iterations");
}
