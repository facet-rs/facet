//! Deterministic pseudo-fuzzing for the decode path.
//!
//! Generates random compatible schemas and payloads without an external fuzzer.
//! Each test runs with multiple seeds to exercise a wide range of inputs.
//! When a real fuzzer (cargo-fuzz / AFL) is integrated, these generators
//! become the fuzzer harness bodies.
//!
//! Two categories:
//! 1. Valid payloads for a given plan — oracle must always succeed.
//! 2. Malformed payloads — oracle must always return a known error class,
//!    not panic or produce garbage.

use vox_postcard::{
    DeserializeError, TranslationPlan, from_slice_with_plan, ir::from_slice_ir, serialize::to_vec,
};
use vox_schema::SchemaRegistry;

use crate::differential::ErrorClass;

// ---------------------------------------------------------------------------
// Minimal seeded PRNG (xorshift64)
// ---------------------------------------------------------------------------

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0xdeadbeef_cafef00d)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub fn next_u8(&mut self) -> u8 {
        self.next_u64() as u8
    }

    pub fn next_usize(&mut self, max: usize) -> usize {
        if max == 0 {
            return 0;
        }
        (self.next_u64() as usize) % max
    }

    pub fn next_bool(&mut self) -> bool {
        self.next_u64() & 1 == 0
    }

    pub fn next_bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next_u8()).collect()
    }
}

// ---------------------------------------------------------------------------
// Postcard varint encoding helper
// ---------------------------------------------------------------------------

pub fn encode_varint(mut v: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
    out
}

// ---------------------------------------------------------------------------
// Valid payload generators for primitive types
// ---------------------------------------------------------------------------

/// Generate a random valid postcard-encoded u32.
pub fn gen_u32(rng: &mut Rng) -> Vec<u8> {
    encode_varint(rng.next_u64() & 0xFFFF_FFFF)
}

/// Generate a random valid postcard-encoded i32 (zigzag).
pub fn gen_i32(rng: &mut Rng) -> Vec<u8> {
    let v = rng.next_u64() as i64;
    let zigzag = ((v << 1) ^ (v >> 63)) as u64;
    encode_varint(zigzag)
}

/// Generate a random valid postcard-encoded String.
pub fn gen_string(rng: &mut Rng) -> Vec<u8> {
    // Use only valid ASCII so UTF-8 validation always passes.
    let len = rng.next_usize(64);
    let mut out = encode_varint(len as u64);
    for _ in 0..len {
        // printable ASCII: 0x20–0x7E
        out.push(0x20 + (rng.next_u8() % 0x5F));
    }
    out
}

/// Generate a random valid postcard-encoded Vec<u8> (as bytes primitive).
pub fn gen_bytes(rng: &mut Rng) -> Vec<u8> {
    let len = rng.next_usize(128);
    let mut out = encode_varint(len as u64);
    out.extend(rng.next_bytes(len));
    out
}

/// Generate a random valid bool.
pub fn gen_bool(rng: &mut Rng) -> Vec<u8> {
    vec![if rng.next_bool() { 1u8 } else { 0u8 }]
}

/// Generate a random valid Option<u32> (0x00 = None, 0x01 + payload = Some).
pub fn gen_option_u32(rng: &mut Rng) -> Vec<u8> {
    if rng.next_bool() {
        vec![0x00]
    } else {
        let mut out = vec![0x01];
        out.extend(gen_u32(rng));
        out
    }
}

/// Generate a random valid Vec<u32>.
pub fn gen_vec_u32(rng: &mut Rng) -> Vec<u8> {
    let count = rng.next_usize(32);
    let mut out = encode_varint(count as u64);
    for _ in 0..count {
        out.extend(gen_u32(rng));
    }
    out
}

/// Generate a random valid Vec<String>.
pub fn gen_vec_string(rng: &mut Rng) -> Vec<u8> {
    let count = rng.next_usize(16);
    let mut out = encode_varint(count as u64);
    for _ in 0..count {
        out.extend(gen_string(rng));
    }
    out
}

// ---------------------------------------------------------------------------
// Malformed payload generators
// ---------------------------------------------------------------------------

/// Strategy for generating malformed bytes.
#[derive(Debug, Clone, Copy)]
pub enum MutationStrategy {
    /// Truncate at a random position.
    Truncate,
    /// Flip a random bit.
    BitFlip,
    /// Corrupt a random byte.
    ByteCorrupt,
    /// Prepend a high-varint length to make length claims exceed actual data.
    HugeLength,
    /// All bytes set to 0x80 (varint overflow on any int field).
    VarintOverflow,
    /// Replace a varint with a value that points past the buffer end.
    EofOnLength,
}

impl MutationStrategy {
    pub fn all() -> &'static [Self] {
        &[
            Self::Truncate,
            Self::BitFlip,
            Self::ByteCorrupt,
            Self::HugeLength,
            Self::VarintOverflow,
            Self::EofOnLength,
        ]
    }
}

/// Apply a mutation strategy to a valid byte payload.
pub fn mutate(bytes: &[u8], strategy: MutationStrategy, rng: &mut Rng) -> Vec<u8> {
    match strategy {
        MutationStrategy::Truncate => {
            if bytes.is_empty() {
                return vec![];
            }
            let cut = rng.next_usize(bytes.len());
            bytes[..cut].to_vec()
        }
        MutationStrategy::BitFlip => {
            let mut out = bytes.to_vec();
            if !out.is_empty() {
                let idx = rng.next_usize(out.len());
                let bit = 1u8 << (rng.next_usize(8));
                out[idx] ^= bit;
            }
            out
        }
        MutationStrategy::ByteCorrupt => {
            let mut out = bytes.to_vec();
            if !out.is_empty() {
                let idx = rng.next_usize(out.len());
                out[idx] = rng.next_u8();
            }
            out
        }
        MutationStrategy::HugeLength => {
            // Prepend a varint claiming a huge number of elements/bytes.
            let mut out = encode_varint(u64::MAX / 2);
            out.extend_from_slice(bytes);
            out
        }
        MutationStrategy::VarintOverflow => {
            // 11 bytes all with MSB set — forces varint overflow on any integer read.
            vec![0x80u8; 11]
        }
        MutationStrategy::EofOnLength => {
            // Claim a large length with nothing following.
            encode_varint(10_000)
        }
    }
}

// ---------------------------------------------------------------------------
// Fuzz runner: oracle must never panic on any input
// ---------------------------------------------------------------------------

/// Run `iters` rounds of valid+mutated inputs through the oracle.
/// Valid inputs must decode successfully; mutated inputs must either succeed
/// or produce a recognized error class (never panic or produce unknown errors).
pub fn fuzz_oracle<T>(
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
    payload_gen: impl Fn(&mut Rng) -> Vec<u8>,
    seeds: &[u64],
    iters_per_seed: usize,
) where
    T: facet::Facet<'static> + std::fmt::Debug,
{
    let known_classes = [
        ErrorClass::UnexpectedEof,
        ErrorClass::VarintOverflow,
        ErrorClass::InvalidUtf8,
        ErrorClass::InvalidEnumDiscriminant,
        ErrorClass::UnknownVariant,
        ErrorClass::InvalidOptionTag,
        ErrorClass::InvalidBool,
        ErrorClass::TrailingBytes,
        ErrorClass::UnsupportedType,
        ErrorClass::Other,
    ];

    for &seed in seeds {
        let mut rng = Rng::new(seed);

        for _ in 0..iters_per_seed {
            // Generate a valid payload and verify oracle succeeds.
            let valid_bytes = payload_gen(&mut rng);
            let oracle_result = from_slice_with_plan::<T>(&valid_bytes, plan, registry);
            assert!(
                oracle_result.is_ok(),
                "oracle failed on valid generated bytes (seed={seed}): {:?}\nbytes={valid_bytes:?}",
                oracle_result.unwrap_err()
            );

            // IR must agree on valid payloads (skip if IR returns UnsupportedType —
            // that means the shape isn't implemented yet, not a real disagreement).
            let ir_result = from_slice_ir::<T>(&valid_bytes, plan, registry, None);
            match (&oracle_result, &ir_result) {
                (Ok(a), Ok(b)) => {
                    // Re-encode both and compare bytes: handles NaN (NaN != NaN in PartialEq
                    // but identical NaN bit patterns produce identical bytes when serialized).
                    let a_bytes = to_vec(a).expect("re-encode oracle result");
                    let b_bytes = to_vec(b).expect("re-encode IR result");
                    assert_eq!(
                        a_bytes, b_bytes,
                        "IR and oracle produced different values (seed={seed})\nbytes={valid_bytes:?}\noracle={a:?}\nir={b:?}"
                    );
                }
                (Ok(_), Err(e)) if !matches!(ErrorClass::of(e), ErrorClass::UnsupportedType) => {
                    panic!(
                        "oracle succeeded but IR failed (seed={seed}): {e}\nbytes={valid_bytes:?}"
                    )
                }
                _ => {}
            }

            // Apply each mutation and verify oracle returns a recognized class (or Ok).
            // Also verify IR agrees with the oracle's error class on mutations.
            for &strategy in MutationStrategy::all() {
                let mut mut_rng = Rng::new(seed.wrapping_add(strategy as u64 * 1337));
                let bad_bytes = mutate(&valid_bytes, strategy, &mut mut_rng);

                let oracle_outcome = from_slice_with_plan::<T>(&bad_bytes, plan, registry);
                let ir_outcome = from_slice_ir::<T>(&bad_bytes, plan, registry, None);

                let oracle_class = match &oracle_outcome {
                    Ok(_) => None,
                    Err(e) => {
                        let class = ErrorClass::of(e);
                        assert!(
                            known_classes.contains(&class),
                            "oracle returned unrecognized error class {class:?} for mutation {strategy:?} (seed={seed}): {e}"
                        );
                        Some(class)
                    }
                };

                // IR must be in the same class as oracle (Ok matches Ok, Err class matches).
                // Skip if IR returns UnsupportedType — shape not yet implemented.
                match (&oracle_class, &ir_outcome) {
                    (_, Err(e)) if matches!(ErrorClass::of(e), ErrorClass::UnsupportedType) => {}
                    (None, Ok(_)) => {} // both Ok
                    (None, Err(e)) => panic!(
                        "oracle Ok but IR failed for mutation {strategy:?} (seed={seed}): {e}\nbytes={bad_bytes:?}"
                    ),
                    (Some(_expected), Ok(_)) => {
                        // IR succeeded where oracle failed — allowed (trailing bytes etc.)
                    }
                    (Some(expected), Err(e)) => {
                        let ir_class = ErrorClass::of(e);
                        assert_eq!(
                            ir_class, *expected,
                            "IR error class {ir_class:?} != oracle class {expected:?} for mutation {strategy:?} (seed={seed}): {e}"
                        );
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Three-way fuzz: reflective × IR × extra engine
// ---------------------------------------------------------------------------

/// Type-erased decode result used for cross-engine comparison inside fuzz_oracle_three_way.
pub enum FuzzOutcome {
    Ok(Vec<u8>),     // re-encoded bytes of the decoded value
    Err(ErrorClass), // error class bucket
}

/// Run a three-way differential fuzz: reflective oracle, IR interpreter, and
/// one additional engine supplied as a closure.
///
/// The third engine receives `(bytes, plan, registry)` and returns
/// `Result<Vec<u8>, DeserializeError>` where the `Ok` variant holds
/// the re-encoded bytes of the decoded value (caller is responsible for
/// encoding — this keeps the closure generic over T without requiring
/// an extra type parameter on the function).
///
/// Divergences are minimised to a repro case by reporting the exact bytes,
/// seed, and iteration where the mismatch occurred.
pub fn fuzz_oracle_three_way<T>(
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
    payload_gen: impl Fn(&mut Rng) -> Vec<u8>,
    seeds: &[u64],
    iters_per_seed: usize,
    third_engine_name: &str,
    third_engine: impl Fn(&[u8], &TranslationPlan, &SchemaRegistry) -> Result<Vec<u8>, DeserializeError>,
) where
    T: facet::Facet<'static> + std::fmt::Debug,
{
    for &seed in seeds {
        let mut rng = Rng::new(seed);

        for iter in 0..iters_per_seed {
            let valid_bytes = payload_gen(&mut rng);

            // --- Oracle (reflective) ---
            let oracle_result = from_slice_with_plan::<T>(&valid_bytes, plan, registry);
            assert!(
                oracle_result.is_ok(),
                "oracle failed on valid bytes (seed={seed} iter={iter}): {:?}\nbytes={valid_bytes:?}",
                oracle_result.unwrap_err()
            );
            let oracle_encoded = to_vec(oracle_result.as_ref().unwrap()).expect("re-encode oracle");

            // --- IR interpreter ---
            let ir_result = from_slice_ir::<T>(&valid_bytes, plan, registry, None);
            match &ir_result {
                Ok(v) => {
                    let ir_encoded = to_vec(v).expect("re-encode IR");
                    assert_eq!(
                        oracle_encoded, ir_encoded,
                        "reflective vs IR divergence (seed={seed} iter={iter})\nbytes={valid_bytes:?}"
                    );
                }
                Err(e) if !matches!(ErrorClass::of(e), ErrorClass::UnsupportedType) => {
                    panic!(
                        "oracle succeeded but IR failed (seed={seed} iter={iter}): {e}\nbytes={valid_bytes:?}"
                    );
                }
                _ => {} // UnsupportedType — IR doesn't implement this shape yet
            }

            // --- Third engine ---
            let third_result = third_engine(&valid_bytes, plan, registry);
            match &third_result {
                Ok(third_encoded) => {
                    assert_eq!(
                        oracle_encoded, *third_encoded,
                        "reflective vs {third_engine_name} divergence (seed={seed} iter={iter})\nbytes={valid_bytes:?}"
                    );
                }
                Err(e) if !matches!(ErrorClass::of(e), ErrorClass::UnsupportedType) => {
                    panic!(
                        "oracle succeeded but {third_engine_name} failed (seed={seed} iter={iter}): {e}\nbytes={valid_bytes:?}"
                    );
                }
                _ => {} // UnsupportedType fallback — skip
            }

            // --- Mutation round: all three must agree on error class ---
            for &strategy in MutationStrategy::all() {
                let mut mut_rng = Rng::new(seed.wrapping_add(strategy as u64 * 1337 + iter as u64));
                let bad_bytes = mutate(&valid_bytes, strategy, &mut mut_rng);

                let oracle_class = match from_slice_with_plan::<T>(&bad_bytes, plan, registry) {
                    Ok(_) => None,
                    Err(e) => Some(ErrorClass::of(&e)),
                };

                let ir_class = match from_slice_ir::<T>(&bad_bytes, plan, registry, None) {
                    Ok(_) => None,
                    Err(e) => {
                        let c = ErrorClass::of(&e);
                        if c == ErrorClass::UnsupportedType {
                            None
                        } else {
                            Some(c)
                        }
                    }
                };

                let third_class = match third_engine(&bad_bytes, plan, registry) {
                    Ok(_) => None,
                    Err(e) => {
                        let c = ErrorClass::of(&e);
                        if c == ErrorClass::UnsupportedType {
                            None
                        } else {
                            Some(c)
                        }
                    }
                };

                // IR must agree with oracle when both produce an error.
                if let (Some(oc), Some(ic)) = (&oracle_class, &ir_class) {
                    assert_eq!(
                        ic, oc,
                        "IR error class {ic:?} != oracle {oc:?} for {strategy:?} (seed={seed} iter={iter})\nbytes={bad_bytes:?}"
                    );
                }

                // Third engine must agree with oracle when both produce an error.
                if let (Some(oc), Some(tc)) = (&oracle_class, &third_class) {
                    assert_eq!(
                        tc, oc,
                        "{third_engine_name} error class {tc:?} != oracle {oc:?} for {strategy:?} (seed={seed} iter={iter})\nbytes={bad_bytes:?}"
                    );
                }
            }
        }
    }
}
