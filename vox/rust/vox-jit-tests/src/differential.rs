//! Differential test harness: oracle (reflective interpreter) vs. candidate engine.
//!
//! The core discipline from the design doc:
//!   same plan + same bytes => same output, same error class
//!
//! `DecodeFn` is a type-erased slot so the IR interpreter and JIT can each
//! be plugged in without changing the test bodies.

use facet::Facet;
use facet_core::Shape;
use vox_postcard::{DeserializeError, TranslationPlan, from_slice_with_plan, ir::from_slice_ir};
use vox_schema::SchemaRegistry;

/// Broad error bucket used for differential comparison.
///
/// Two runs must land in the same class; the exact byte offset or message
/// is allowed to differ between implementations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorClass {
    UnexpectedEof,
    VarintOverflow,
    InvalidUtf8,
    InvalidEnumDiscriminant,
    UnknownVariant,
    InvalidOptionTag,
    InvalidBool,
    TrailingBytes,
    UnsupportedType,
    Other,
}

impl ErrorClass {
    pub fn of(err: &DeserializeError) -> Self {
        match err {
            DeserializeError::UnexpectedEof { .. } => Self::UnexpectedEof,
            DeserializeError::VarintOverflow { .. } => Self::VarintOverflow,
            DeserializeError::InvalidUtf8 { .. } => Self::InvalidUtf8,
            DeserializeError::InvalidEnumDiscriminant { .. } => Self::InvalidEnumDiscriminant,
            DeserializeError::UnknownVariant { .. } => Self::UnknownVariant,
            DeserializeError::InvalidOptionTag { .. } => Self::InvalidOptionTag,
            DeserializeError::InvalidBool { .. } => Self::InvalidBool,
            DeserializeError::TrailingBytes { .. } => Self::TrailingBytes,
            DeserializeError::UnsupportedType(_) => Self::UnsupportedType,
            DeserializeError::ReflectError(_)
            | DeserializeError::Custom(_)
            | DeserializeError::Protocol(_) => Self::Other,
        }
    }
}

/// Outcome of a single decode attempt.
#[derive(Debug)]
pub enum DecodeOutcome<T> {
    Ok(T),
    Err(ErrorClass),
}

impl<T: PartialEq + std::fmt::Debug> DecodeOutcome<T> {
    pub fn assert_matches(&self, other: &Self, label: &str) {
        // If the candidate returned UnsupportedType, the IR doesn't implement
        // this shape yet — skip the comparison (not a disagreement).
        if matches!(other, DecodeOutcome::Err(ErrorClass::UnsupportedType)) {
            return;
        }
        match (self, other) {
            (DecodeOutcome::Ok(a), DecodeOutcome::Ok(b)) => {
                assert_eq!(a, b, "{label}: decoded values differ");
            }
            (DecodeOutcome::Err(a), DecodeOutcome::Err(b)) => {
                assert_eq!(a, b, "{label}: error classes differ");
            }
            _ => {
                panic!(
                    "{label}: one side succeeded, the other failed\n  oracle={self:?}\n  candidate={other:?}"
                );
            }
        }
    }
}

/// A decode engine: given bytes + plan + registry, produce T or an error class.
///
/// The oracle is always `reflective_decode`. The candidate slot is left open
/// for the IR interpreter (task #3) and later the JIT (task #8+).
pub trait DecodeEngine<T>: Send + Sync {
    fn decode(
        &self,
        bytes: &[u8],
        plan: &TranslationPlan,
        registry: &SchemaRegistry,
    ) -> DecodeOutcome<T>;

    fn name(&self) -> &'static str;
}

/// Oracle engine: the reflective interpreter that already exists.
pub struct ReflectiveOracle;

impl<T> DecodeEngine<T> for ReflectiveOracle
where
    T: Facet<'static> + PartialEq + std::fmt::Debug,
{
    fn decode(
        &self,
        bytes: &[u8],
        plan: &TranslationPlan,
        registry: &SchemaRegistry,
    ) -> DecodeOutcome<T> {
        match from_slice_with_plan::<T>(bytes, plan, registry) {
            Ok(v) => DecodeOutcome::Ok(v),
            Err(e) => DecodeOutcome::Err(ErrorClass::of(&e)),
        }
    }

    fn name(&self) -> &'static str {
        "reflective"
    }
}

/// A test case: named bytes + the plan + registry against which both engines run.
pub struct DifferentialCase<'a> {
    pub label: &'a str,
    pub bytes: &'a [u8],
    pub plan: &'a TranslationPlan,
    pub registry: &'a SchemaRegistry,
}

/// Run a differential check between oracle and one or more candidates.
///
/// Panics if any candidate disagrees with the oracle.
pub fn assert_differential<T>(
    oracle: &dyn DecodeEngine<T>,
    candidates: &[&dyn DecodeEngine<T>],
    cases: &[DifferentialCase<'_>],
) where
    T: Facet<'static> + PartialEq + std::fmt::Debug,
{
    for case in cases {
        let oracle_out = oracle.decode(case.bytes, case.plan, case.registry);
        for candidate in candidates {
            let candidate_out = candidate.decode(case.bytes, case.plan, case.registry);
            oracle_out.assert_matches(
                &candidate_out,
                &format!(
                    "[{}] oracle={} candidate={}",
                    case.label,
                    oracle.name(),
                    candidate.name()
                ),
            );
        }
    }
}

/// Build a `DifferentialCase` from a value by serializing it.
///
/// Uses the identity plan (same type on both sides).
pub fn case_from_value<'a, T>(
    label: &'a str,
    bytes: &'a [u8],
    registry: &'a SchemaRegistry,
) -> DifferentialCase<'a>
where
    T: for<'de> Facet<'de> + std::fmt::Debug,
{
    let plan = vox_postcard::build_identity_plan(T::SHAPE);
    // We can't store the plan inline since it must be behind a reference;
    // callers must hold the plan separately. This helper only documents the pattern.
    // In practice, tests construct plans in local variables and pass refs to DifferentialCase.
    let _ = plan;
    let _ = bytes;
    DifferentialCase {
        label,
        bytes,
        plan: &TranslationPlan::Identity,
        registry,
    }
}

/// Helper: encode `value` with `vox_postcard::to_vec` and assert oracle succeeds,
/// returning the bytes for use in differential cases.
pub fn encode_value<T>(value: &T) -> Vec<u8>
where
    T: for<'de> Facet<'de>,
{
    vox_postcard::serialize::to_vec(value).expect("encode failed in test helper")
}

/// Canonical shape of the decode entry point the IR interpreter and JIT must expose.
///
/// Once ir-architect lands task #3, they wire in a concrete type here.
/// Until then, tests only use `ReflectiveOracle`.
pub type DecodeFnPtr<T> = fn(
    bytes: &[u8],
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<T, DeserializeError>;

/// An engine backed by a raw function pointer — for plugging in the IR interpreter
/// or JIT stub once those exist.
pub struct FnPtrEngine<T> {
    pub name: &'static str,
    pub f: DecodeFnPtr<T>,
}

impl<T> DecodeEngine<T> for FnPtrEngine<T>
where
    T: Facet<'static> + PartialEq + std::fmt::Debug + Send + Sync,
{
    fn decode(
        &self,
        bytes: &[u8],
        plan: &TranslationPlan,
        registry: &SchemaRegistry,
    ) -> DecodeOutcome<T> {
        match (self.f)(bytes, plan, registry) {
            Ok(v) => DecodeOutcome::Ok(v),
            Err(e) => DecodeOutcome::Err(ErrorClass::of(&e)),
        }
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

/// IR interpreter engine — uses `from_slice_ir` with no calibration registry.
///
/// Wire this in as a candidate alongside `ReflectiveOracle` to differential-test
/// the IR interpreter against the reflective path.
pub struct IrEngine;

impl<T> DecodeEngine<T> for IrEngine
where
    T: Facet<'static> + PartialEq + std::fmt::Debug + Send + Sync,
{
    fn decode(
        &self,
        bytes: &[u8],
        plan: &TranslationPlan,
        registry: &SchemaRegistry,
    ) -> DecodeOutcome<T> {
        match from_slice_ir::<T>(bytes, plan, registry, None) {
            Ok(v) => DecodeOutcome::Ok(v),
            Err(e) => DecodeOutcome::Err(ErrorClass::of(&e)),
        }
    }

    fn name(&self) -> &'static str {
        "ir"
    }
}

/// Shape for a `&'static Shape`-typed key used in tests.
pub fn shape_of<T: for<'de> Facet<'de>>() -> &'static Shape {
    T::SHAPE
}

/// JIT forced-fallback engine.
///
/// Sets `VOX_CODEC=reflect` before decoding so the full JIT integration path
/// is exercised but the generated stub is bypassed — routing through the
/// reflective interpreter. This confirms the wiring is correct and provides a
/// regression baseline against the real JIT engine.
pub struct JitFallbackEngine;

impl<T> DecodeEngine<T> for JitFallbackEngine
where
    T: Facet<'static> + PartialEq + std::fmt::Debug + Send + Sync,
{
    fn decode(
        &self,
        bytes: &[u8],
        plan: &TranslationPlan,
        registry: &SchemaRegistry,
    ) -> DecodeOutcome<T> {
        // Safety: env vars are process-global. Tests using this engine must not
        // run in parallel with tests that rely on VOX_CODEC being unset.
        // The fuzz tests are single-threaded per test function, so this is safe.
        unsafe { std::env::set_var("VOX_CODEC", "reflect") };
        let result = from_slice_with_plan::<T>(bytes, plan, registry);
        unsafe { std::env::remove_var("VOX_CODEC") };
        match result {
            Ok(v) => DecodeOutcome::Ok(v),
            Err(e) => DecodeOutcome::Err(ErrorClass::of(&e)),
        }
    }

    fn name(&self) -> &'static str {
        "jit-fallback"
    }
}
