//! Smoke test for recursive Tree encode + decode round-trip.
use facet::Facet;
use vox_bench::shapes::{Tree, make_tree};
use vox_types::SchemaRegistry;

fn main() {
    for depth in 0..=4u32 {
        let value = make_tree(depth, 42);
        let reflective_bytes = vox_postcard::to_vec(&value).expect("encode");
        eprintln!(
            "depth {depth}: {} bytes (reflective encode)",
            reflective_bytes.len()
        );

        // JIT encode parity with reflective.
        let jit_bytes = vox_bench::jit_encode(&value);
        assert_eq!(
            jit_bytes, reflective_bytes,
            "JIT encode bytes diverge from reflective at depth {depth}"
        );

        let plan = vox_postcard::build_identity_plan(<Tree as Facet<'static>>::SHAPE);
        let registry = SchemaRegistry::new();

        let decoded: Tree = vox_jit::global_runtime()
            .try_decode_owned::<Tree>(&jit_bytes, 0, &plan, &registry)
            .expect("JIT available")
            .expect("decode ok");
        assert_eq!(decoded, value);
        eprintln!("depth {depth}: OK");
    }
    eprintln!("all good");
}
