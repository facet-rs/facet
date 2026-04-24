//! Deterministic fuzz tests for the decode oracle.
//!
//! Each test runs the fuzz_oracle over multiple seeds and iterations per seed.
//! Valid payloads must always decode successfully; mutated payloads must
//! produce a recognized ErrorClass (or also succeed — some mutations happen to
//! be valid).

use facet::Facet;
use vox_postcard::{build_identity_plan, from_slice_with_plan, serialize::to_vec};
use vox_schema::SchemaRegistry;

use crate::fuzz::{
    Rng, fuzz_oracle, fuzz_oracle_three_way, gen_bool, gen_bytes, gen_i32, gen_option_u32,
    gen_string, gen_u32, gen_vec_string, gen_vec_u32,
};

// ---------------------------------------------------------------------------
// Helper: JIT forced-fallback closure for three-way fuzz.
//
// Sets VOX_CODEC=reflect to route through the reflective path via the same
// code path the JIT integration uses when a stub isn't compiled.
// ---------------------------------------------------------------------------

fn jit_fallback_engine<T>(
    bytes: &[u8],
    plan: &vox_postcard::TranslationPlan,
    registry: &vox_schema::SchemaRegistry,
) -> Result<Vec<u8>, vox_postcard::DeserializeError>
where
    T: facet::Facet<'static> + std::fmt::Debug,
{
    // SAFETY: env-var mutation is safe when tests are single-threaded per
    // test function (default for `cargo test`). The var is restored before
    // returning so other tests in the same process aren't affected.
    unsafe { std::env::set_var("VOX_CODEC", "reflect") };
    let result = from_slice_with_plan::<T>(bytes, plan, registry);
    unsafe { std::env::remove_var("VOX_CODEC") };
    result.map(|v| to_vec(&v).expect("re-encode jit-fallback result"))
}

// Miri runs ~100x slower — use a tiny corpus to keep the sweep under 30s.
#[cfg(not(miri))]
const SEEDS: &[u64] = &[0, 1, 42, 0xDEAD_BEEF, 0xCAFE_F00D, 999_999_999];
#[cfg(miri)]
const SEEDS: &[u64] = &[0, 42];

#[cfg(not(miri))]
const ITERS: usize = 32;
#[cfg(miri)]
const ITERS: usize = 4;

// ---------------------------------------------------------------------------
// Primitive types
// ---------------------------------------------------------------------------

#[test]
fn fuzz_u32() {
    let plan = build_identity_plan(u32::SHAPE);
    let registry = SchemaRegistry::new();
    fuzz_oracle::<u32>(&plan, &registry, gen_u32, SEEDS, ITERS);
}

#[test]
fn fuzz_i32() {
    let plan = build_identity_plan(i32::SHAPE);
    let registry = SchemaRegistry::new();
    fuzz_oracle::<i32>(&plan, &registry, gen_i32, SEEDS, ITERS);
}

#[test]
fn fuzz_bool() {
    let plan = build_identity_plan(bool::SHAPE);
    let registry = SchemaRegistry::new();
    fuzz_oracle::<bool>(&plan, &registry, gen_bool, SEEDS, ITERS);
}

// ---------------------------------------------------------------------------
// String and bytes
// ---------------------------------------------------------------------------

#[test]
fn fuzz_string() {
    let plan = build_identity_plan(String::SHAPE);
    let registry = SchemaRegistry::new();
    fuzz_oracle::<String>(&plan, &registry, gen_string, SEEDS, ITERS);
}

#[test]
fn fuzz_bytes() {
    let plan = build_identity_plan(<Vec<u8> as Facet>::SHAPE);
    let registry = SchemaRegistry::new();
    fuzz_oracle::<Vec<u8>>(&plan, &registry, gen_bytes, SEEDS, ITERS);
}

// ---------------------------------------------------------------------------
// Vec<T>
// ---------------------------------------------------------------------------

#[test]
fn fuzz_vec_u32() {
    let plan = build_identity_plan(<Vec<u32> as Facet>::SHAPE);
    let registry = SchemaRegistry::new();
    fuzz_oracle::<Vec<u32>>(&plan, &registry, gen_vec_u32, SEEDS, ITERS);
}

#[test]
fn fuzz_vec_string() {
    let plan = build_identity_plan(<Vec<String> as Facet>::SHAPE);
    let registry = SchemaRegistry::new();
    fuzz_oracle::<Vec<String>>(&plan, &registry, gen_vec_string, SEEDS, ITERS);
}

// ---------------------------------------------------------------------------
// Option<T>
// ---------------------------------------------------------------------------

#[test]
fn fuzz_option_u32() {
    let plan = build_identity_plan(<Option<u32> as Facet>::SHAPE);
    let registry = SchemaRegistry::new();
    fuzz_oracle::<Option<u32>>(&plan, &registry, gen_option_u32, SEEDS, ITERS);
}

// ---------------------------------------------------------------------------
// Struct with all scalar fields (exercises struct decode path under fuzzing)
// ---------------------------------------------------------------------------

#[test]
fn fuzz_struct_scalars() {
    use crate::fixtures::Scalars;
    use vox_postcard::serialize::to_vec;

    let plan = build_identity_plan(Scalars::SHAPE);
    let registry = SchemaRegistry::new();

    let payload = |rng: &mut Rng| -> Vec<u8> {
        let s = Scalars {
            u8_val: rng.next_u8(),
            u16_val: (rng.next_u64() & 0xFFFF) as u16,
            u32_val: (rng.next_u64() & 0xFFFF_FFFF) as u32,
            u64_val: rng.next_u64(),
            i8_val: rng.next_u8() as i8,
            i16_val: (rng.next_u64() as i16).wrapping_abs(),
            i32_val: rng.next_u64() as i32,
            i64_val: rng.next_u64() as i64,
            // Avoid NaN: NaN != NaN breaks assert_eq even when both sides are identical.
            f32_val: {
                let b = rng.next_u64() as u32;
                let v = f32::from_bits(b);
                if v.is_nan() { 0.0f32 } else { v }
            },
            f64_val: {
                let b = rng.next_u64();
                let v = f64::from_bits(b);
                if v.is_nan() { 0.0f64 } else { v }
            },
            bool_val: rng.next_bool(),
        };
        to_vec(&s).unwrap()
    };

    fuzz_oracle::<Scalars>(&plan, &registry, payload, SEEDS, ITERS);
}

// ---------------------------------------------------------------------------
// Vec<String>-containing struct (exercises partial-init under fuzzing)
// ---------------------------------------------------------------------------

#[test]
fn fuzz_struct_vec_string() {
    use crate::fixtures::VecString;
    use vox_postcard::serialize::to_vec;

    let plan = build_identity_plan(VecString::SHAPE);
    let registry = SchemaRegistry::new();

    let payload = |rng: &mut Rng| -> Vec<u8> {
        let count = rng.next_usize(8);
        let tags: Vec<String> = (0..count)
            .map(|_| {
                let len = rng.next_usize(16);
                (0..len)
                    .map(|_| (0x41u8 + (rng.next_u8() % 26)) as char)
                    .collect()
            })
            .collect();
        to_vec(&VecString { tags }).unwrap()
    };

    fuzz_oracle::<VecString>(&plan, &registry, payload, SEEDS, ITERS);
}

// ---------------------------------------------------------------------------
// Option<u32>-containing struct (exercises option partial-init under fuzzing)
// ---------------------------------------------------------------------------

#[test]
fn fuzz_struct_option() {
    use crate::fixtures::WithOption;
    use vox_postcard::serialize::to_vec;

    let plan = build_identity_plan(WithOption::SHAPE);
    let registry = SchemaRegistry::new();

    let payload = |rng: &mut Rng| -> Vec<u8> {
        let maybe = if rng.next_bool() {
            Some(rng.next_u64() as u32)
        } else {
            None
        };
        let len = rng.next_usize(16);
        let name: String = (0..len)
            .map(|_| (0x61u8 + (rng.next_u8() % 26)) as char)
            .collect();
        to_vec(&WithOption { maybe, name }).unwrap()
    };

    fuzz_oracle::<WithOption>(&plan, &registry, payload, SEEDS, ITERS);
}

// ---------------------------------------------------------------------------
// Enum: unit variants (exercises discriminant decode under fuzzing)
// ---------------------------------------------------------------------------

#[test]
fn fuzz_enum_unit() {
    use crate::fixtures::Color;
    use crate::fuzz::encode_varint;

    let plan = build_identity_plan(Color::SHAPE);
    let registry = SchemaRegistry::new();

    // Only generate valid discriminants 0, 1, 2 (Red, Green, Blue).
    let payload = |rng: &mut Rng| -> Vec<u8> { encode_varint((rng.next_u64() % 3)) };

    fuzz_oracle::<Color>(&plan, &registry, payload, SEEDS, ITERS);
}

// ---------------------------------------------------------------------------
// High-iteration stress run: u32 with 1024 iters per seed
// ---------------------------------------------------------------------------

// Miri: 1024 iterations × 6 mutation strategies is too slow under Miri interpretation.
#[cfg_attr(miri, ignore)]
#[test]
fn fuzz_u32_stress() {
    let plan = build_identity_plan(u32::SHAPE);
    let registry = SchemaRegistry::new();
    fuzz_oracle::<u32>(&plan, &registry, gen_u32, &[0xF00D_CAFE, 0x1234_5678], 1024);
}

// ---------------------------------------------------------------------------
// High-iteration stress run: Vec<String> with more seeds
// ---------------------------------------------------------------------------

// Miri: 64 iterations × 5 seeds × 6 strategies is too slow under Miri.
#[cfg_attr(miri, ignore)]
#[test]
fn fuzz_vec_string_stress() {
    let plan = build_identity_plan(<Vec<String> as Facet>::SHAPE);
    let registry = SchemaRegistry::new();
    fuzz_oracle::<Vec<String>>(
        &plan,
        &registry,
        gen_vec_string,
        &[100, 200, 300, 400, 500],
        64,
    );
}

// ---------------------------------------------------------------------------
// Three-way differential fuzz: reflective × IR × JIT-fallback
//
// These tests run the same workloads through all three engines and assert
// identical output. The JIT leg uses VOX_CODEC=reflect (forced fallback
// through the reflective path).
// ---------------------------------------------------------------------------

#[test]
fn three_way_fuzz_u32() {
    let plan = build_identity_plan(u32::SHAPE);
    let registry = SchemaRegistry::new();
    fuzz_oracle_three_way::<u32>(
        &plan,
        &registry,
        gen_u32,
        SEEDS,
        ITERS,
        "jit-fallback",
        jit_fallback_engine::<u32>,
    );
}

#[test]
fn three_way_fuzz_string() {
    let plan = build_identity_plan(String::SHAPE);
    let registry = SchemaRegistry::new();
    fuzz_oracle_three_way::<String>(
        &plan,
        &registry,
        gen_string,
        SEEDS,
        ITERS,
        "jit-fallback",
        jit_fallback_engine::<String>,
    );
}

#[test]
fn three_way_fuzz_vec_u32() {
    let plan = build_identity_plan(<Vec<u32> as Facet>::SHAPE);
    let registry = SchemaRegistry::new();
    fuzz_oracle_three_way::<Vec<u32>>(
        &plan,
        &registry,
        gen_vec_u32,
        SEEDS,
        ITERS,
        "jit-fallback",
        jit_fallback_engine::<Vec<u32>>,
    );
}

#[test]
fn three_way_fuzz_vec_string() {
    let plan = build_identity_plan(<Vec<String> as Facet>::SHAPE);
    let registry = SchemaRegistry::new();
    fuzz_oracle_three_way::<Vec<String>>(
        &plan,
        &registry,
        gen_vec_string,
        SEEDS,
        ITERS,
        "jit-fallback",
        jit_fallback_engine::<Vec<String>>,
    );
}

#[test]
fn three_way_fuzz_struct_scalars() {
    use crate::fixtures::Scalars;

    let plan = build_identity_plan(Scalars::SHAPE);
    let registry = SchemaRegistry::new();

    let payload = |rng: &mut Rng| -> Vec<u8> {
        let s = Scalars {
            u8_val: rng.next_u8(),
            u16_val: (rng.next_u64() & 0xFFFF) as u16,
            u32_val: (rng.next_u64() & 0xFFFF_FFFF) as u32,
            u64_val: rng.next_u64(),
            i8_val: rng.next_u8() as i8,
            i16_val: (rng.next_u64() as i16).wrapping_abs(),
            i32_val: rng.next_u64() as i32,
            i64_val: rng.next_u64() as i64,
            f32_val: {
                let v = f32::from_bits(rng.next_u64() as u32);
                if v.is_nan() { 0.0 } else { v }
            },
            f64_val: {
                let v = f64::from_bits(rng.next_u64());
                if v.is_nan() { 0.0 } else { v }
            },
            bool_val: rng.next_bool(),
        };
        to_vec(&s).unwrap()
    };

    fuzz_oracle_three_way::<Scalars>(
        &plan,
        &registry,
        payload,
        SEEDS,
        ITERS,
        "jit-fallback",
        jit_fallback_engine::<Scalars>,
    );
}

#[test]
fn three_way_fuzz_enum_unit() {
    use crate::fixtures::Color;
    use crate::fuzz::encode_varint;

    let plan = build_identity_plan(Color::SHAPE);
    let registry = SchemaRegistry::new();

    let payload = |rng: &mut Rng| -> Vec<u8> { encode_varint((rng.next_u64() % 3)) };

    fuzz_oracle_three_way::<Color>(
        &plan,
        &registry,
        payload,
        SEEDS,
        ITERS,
        "jit-fallback",
        jit_fallback_engine::<Color>,
    );
}
