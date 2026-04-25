//! dhat heap profile of a single JIT-decode of `(GnarlyPayload,)` at n=16.
//!
//! Run:
//!     cargo run --release --example decode_dhat --features dhat-heap
//!
//! Caveat on backtraces: libunwind cannot walk through Cranelift-JIT'd code
//! (no `.eh_frame` registration via `__register_frame`), so dhat backtraces
//! for JIT-triggered allocations collapse to a single alloc-machinery frame.
//! The pp-level breakdown (counts × sizes) is still accurate and complete —
//! we just have to map size classes back to fields by hand using the known
//! struct layout. That's what `examples/decode_dhat.py` does.
//!
//! The fixture (payload construction, JIT cache warm-up) runs *before* the
//! profiler is started so its allocations are not counted. Inside the profiled
//! scope we decode args once and response once, drop both, and let the
//! profiler write `dhat-heap.json` on drop.
use facet::Facet;
use spec_proto::GnarlyPayload;
use vox_bench::{jit_decode, make_gnarly_payload};
use vox_types::VoxError;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

type GnarlyArgs = (GnarlyPayload,);
type GnarlyResponse = Result<GnarlyPayload, VoxError<std::convert::Infallible>>;

fn main() {
    // ---- Fixture (NOT profiled) ------------------------------------------------
    let payload_args = (make_gnarly_payload(16, 0),);
    let bytes_args = vox_postcard::to_vec(&payload_args).expect("encode args fixture");
    let plan_args = vox_postcard::build_identity_plan(<GnarlyArgs as Facet<'static>>::SHAPE);
    let registry = vox_types::SchemaRegistry::new();

    let payload_resp: GnarlyResponse = Ok(make_gnarly_payload(16, 0));
    let bytes_resp = vox_postcard::to_vec(&payload_resp).expect("encode response fixture");
    let plan_resp = vox_postcard::build_identity_plan(<GnarlyResponse as Facet<'static>>::SHAPE);

    // Warm the JIT cache so the JIT-compile artifacts and one-time decode
    // helpers don't pollute the profile.
    let _: GnarlyArgs = jit_decode(&bytes_args, &plan_args, &registry);
    let _: GnarlyResponse = jit_decode(&bytes_resp, &plan_resp, &registry);

    // ---- Profiled section ------------------------------------------------------
    // trim_backtraces(None) keeps the small bit of context that exists (we
    // can at least see the dhat::Alloc / std::alloc frames) instead of
    // dhat collapsing them into nothing.
    let profiler = dhat::Profiler::builder().trim_backtraces(None).build();

    let result_args: GnarlyArgs = jit_decode(&bytes_args, &plan_args, &registry);
    std::hint::black_box(&result_args);
    drop(result_args);

    let result_resp: GnarlyResponse = jit_decode(&bytes_resp, &plan_resp, &registry);
    std::hint::black_box(&result_resp);
    drop(result_resp);

    drop(profiler);
    eprintln!("wrote dhat-heap.json — open in dh_view.html");
}
