//! Phase 05 — the scheduler resolves a registered effect primitive at the demand
//! layer: it evaluates the request island as a pure demand, folds the effect's
//! `(primitive recipe, request value)` into a content-addressed demand key,
//! consults the dedicated effect memo policy-aware, and on a miss dispatches the
//! primitive through its sole `EffectCtx` window. These certificates drive the
//! real value-island lane end to end — register a primitive, derive the compiler
//! manifest from that same set, compile the effectful source, and evaluate the
//! consumer island with the effect edge wired.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use vix::compiler::Compiler;
use vix::lowering::{LoweringCache, attribution_for};
use vix::runtime::{
    ChaosPolicy, EffectDemand, EventKind, EventLog, Evaluation, FailureValue, IslandInputs,
    Location, MachineError, Runtime,
};
use vix::runtime::primitive::{MemoPolicy, PrimitiveFailure, PrimitiveSet};

/// `probe where { n: Int } -> String`. An `Int` request field decodes inline
/// (a `String` field would resident-reference and reject at decode); a `String`
/// response is what the consumer compares in `expect_eq`.
#[derive(facet::Facet)]
struct ProbeRequest {
    n: i64,
}

const EFFECT_SOURCE: &str = "#[test]\nfn t() -> Stream<Check> {\n    let v = probe where { n: 5 };\n    yield expect_eq(v, \"9\");\n}\n";

/// A run of one effectful test source through the real value-island lane: the
/// consumer island evaluated `evaluations` times against a runtime carrying the
/// registered primitive, plus the observable resolution facts.
struct Outcome {
    begins: usize,
    effect_spawns: u64,
    dispatched: usize,
    results: Vec<Result<Evaluation, Box<MachineError>>>,
    events_first: Vec<EventKind>,
}

/// Register `probe` under `policy` with a begin-counting responder, derive the
/// compiler manifest from that registered set (so effect ids match by
/// construction), compile the effectful source, and evaluate the consumer island
/// `evaluations` times.
fn run_probe(
    policy: MemoPolicy,
    responder: impl Fn(i64) -> Result<String, PrimitiveFailure> + Send + Sync + 'static,
    evaluations: usize,
) -> Outcome {
    let begins = Arc::new(AtomicUsize::new(0));
    let begins_in_closure = begins.clone();
    let mut set = PrimitiveSet::new();
    set.register_function::<String, ProbeRequest, _>("probe", policy, move |req: ProbeRequest| {
        begins_in_closure.fetch_add(1, Ordering::SeqCst);
        responder(req.n)
    })
    .expect("probe registers");
    let manifest = set.compiler_manifest();

    let compilation = Compiler::new()
        .with_primitives(manifest)
        .compile(EFFECT_SOURCE)
        .expect("effect source compiles");
    let partitioned = compilation
        .module
        .partition_test(&compilation.module.tests[0]);
    let consumer = partitioned.islands[0].clone();
    let request = partitioned.effect_islands[0].island.clone();

    // Two caches: both lowered artifacts must be borrowed at once, and
    // `get_or_lower` takes `&mut self`.
    let mut request_cache = LoweringCache::default();
    let mut consumer_cache = LoweringCache::default();
    let request_lowered = request_cache.get_or_lower(&request).expect("request lowers");
    let consumer_lowered = consumer_cache
        .get_or_lower(&consumer)
        .expect("consumer lowers");
    let request_attr = attribution_for(&request);
    let consumer_attr = attribution_for(&consumer);
    let request_loc = Location::for_test_value("effect", "request");
    let consumer_loc = Location::for_test_value("effect", "consumer");

    let mut runtime = Runtime::new(EventLog::default()).with_primitives(Arc::new(set));
    let mut results = Vec::with_capacity(evaluations);
    let mut events_first = Vec::new();
    for iteration in 0..evaluations {
        let effects = [EffectDemand {
            request_island: request.id,
            request_location: &request_loc,
            request_lowered,
            request_attribution: &request_attr,
            request_arguments: &[],
            request_wires: &[],
        }];
        let result = runtime.evaluate(
            consumer.id,
            &consumer_loc,
            consumer_lowered,
            &consumer_attr,
            IslandInputs {
                arguments: &[],
                wires: &[],
                effects: &effects,
            },
            ChaosPolicy::default(),
        );
        if iteration == 0 {
            events_first = runtime
                .sink()
                .events()
                .iter()
                .map(|event| event.kind.clone())
                .collect();
        }
        results.push(result);
    }

    let dispatched = runtime
        .sink()
        .events()
        .iter()
        .filter(|event| matches!(event.kind, EventKind::EffectDispatched { .. }))
        .count();
    Outcome {
        begins: begins.load(Ordering::SeqCst),
        effect_spawns: runtime.counters().effect_spawns,
        dispatched,
        results,
        events_first,
    }
}

/// One Hermetic effect dispatches exactly once; a second demand of the same
/// consumer folds the same effect response identity into the same consumer
/// demand key, so the effect memo (and then the consumer memo) hits and the
/// primitive is never begun again.
#[test]
fn hermetic_effect_dispatches_once_then_memoizes() {
    let outcome = run_probe(MemoPolicy::Hermetic, |_| Ok("9".to_owned()), 2);
    for result in &outcome.results {
        let evaluation = result.as_ref().expect("effectful consumer evaluates");
        assert!(
            evaluation.failure.is_none(),
            "the probe response resolves cleanly, got {:?}",
            evaluation.failure
        );
    }
    assert_eq!(
        outcome.begins, 1,
        "the Hermetic primitive is begun exactly once across two demands"
    );
    assert_eq!(
        outcome.effect_spawns, 1,
        "effect_spawns counts the single real dispatch, not the memo hit"
    );
    assert_eq!(
        outcome.dispatched, 1,
        "exactly one EffectDispatched event — a memo hit emits none"
    );
    let dispatched_first = outcome
        .events_first
        .iter()
        .filter(|kind| matches!(kind, EventKind::EffectDispatched { .. }))
        .count();
    assert_eq!(
        dispatched_first, 1,
        "the dispatch happened on the first demand"
    );
}

/// A Volatile effect skips both the effect-memo lookup and insert, so every
/// consumer demand re-dispatches the primitive — two demands, two begins.
#[test]
fn volatile_effect_redispatches_every_demand() {
    let outcome = run_probe(MemoPolicy::Volatile, |_| Ok("9".to_owned()), 2);
    for result in &outcome.results {
        result.as_ref().expect("effectful consumer evaluates");
    }
    assert_eq!(
        outcome.begins, 2,
        "a Volatile primitive is begun on every demand — no effect memo"
    );
    assert_eq!(
        outcome.effect_spawns, 2,
        "each Volatile dispatch is counted"
    );
    assert_eq!(outcome.dispatched, 2, "two dispatch events, one per demand");
}

/// A primitive that reports `Completion::Failed` produces a generic
/// language-failure value keyed by the effect recipe. It dispatches (a real
/// begin) and, folded into the consumer demand, fails the consumer on the
/// language plane — never a machine crash.
#[test]
fn failed_completion_is_a_language_failure() {
    let outcome = run_probe(
        MemoPolicy::Hermetic,
        |_| {
            Err(PrimitiveFailure {
                code: "unavailable".to_owned(),
                message: "probe refused".to_owned(),
            })
        },
        1,
    );
    let evaluation = outcome.results[0]
        .as_ref()
        .expect("a Failed completion is a language failure, never a machine error");
    assert!(
        matches!(evaluation.failure, Some(FailureValue::Primitive { .. })),
        "the consumer carries the generic primitive language failure, got {:?}",
        evaluation.failure
    );
    assert_eq!(
        outcome.begins, 1,
        "the primitive was actually begun before it reported failure"
    );
    assert_eq!(
        outcome.effect_spawns, 1,
        "a failed dispatch is still a dispatch"
    );
}

/// An effect-free test never enters the effect path: no dispatch, no spawn, and
/// the pure value island evaluates exactly as before.
#[test]
fn effect_free_test_touches_no_effect_machinery() {
    let source = "#[test]\nfn t() -> Stream<Check> {\n    yield expect_eq(1 + 1, 2);\n}\n";
    let compilation = Compiler::new().compile(source).expect("pure source compiles");
    let partitioned = compilation
        .module
        .partition_test(&compilation.module.tests[0]);
    assert!(
        partitioned.effect_islands.is_empty(),
        "the pure test has no request islands"
    );
    let consumer = partitioned.islands[0].clone();
    let mut cache = LoweringCache::default();
    let lowered = cache.get_or_lower(&consumer).expect("consumer lowers");
    assert!(
        lowered.effect_inputs.is_empty(),
        "the pure artifact carries no effect edges"
    );
    let attribution = attribution_for(&consumer);
    let location = Location::for_test_value("effect", "pure");

    let mut runtime = Runtime::new(EventLog::default());
    let result = runtime
        .evaluate(
            consumer.id,
            &location,
            lowered,
            &attribution,
            IslandInputs {
                arguments: &[],
                wires: &[],
                effects: &[],
            },
            ChaosPolicy::default(),
        )
        .expect("the pure island evaluates");
    assert!(result.failure.is_none(), "1 + 1 == 2 passes");
    assert_eq!(
        runtime.counters().effect_spawns,
        0,
        "an effect-free evaluation dispatches nothing"
    );
    assert!(
        !runtime
            .sink()
            .events()
            .iter()
            .any(|event| matches!(event.kind, EventKind::EffectDispatched { .. })),
        "no EffectDispatched event on the pure path"
    );
}
