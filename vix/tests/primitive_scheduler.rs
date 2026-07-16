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

/// `probe where { text: String } -> String`. A `String` request field realizes as
/// a store-resident `FrozenValue::Reference`; the scheduler must resolve it
/// against the store before decode (phase 06 Task 1), else the adapter's decode
/// rejects the reference as a machine-plane protocol violation.
#[derive(facet::Facet)]
struct TextRequest {
    text: String,
}

/// `probe_version where { text: String } -> Version`. A record response frames as
/// an aggregate FramedNode identity tree with EMPTY resident bytes (the weavy ABI
/// constraint); the consumer projects a scalar field off the realized record
/// input (phase 06 Task 2).
#[derive(facet::Facet)]
struct Version {
    major: i64,
    minor: i64,
    patch: i64,
}

const EFFECT_SOURCE: &str = "#[test]\nfn t() -> Stream<Check> {\n    let v = probe where { n: 5 };\n    yield expect_eq(v, \"9\");\n}\n";

const TEXT_SOURCE: &str = "#[test]\nfn t() -> Stream<Check> {\n    let v = probe where { text: \"1.2.3\" };\n    yield expect_eq(v, \"1.2.3\");\n}\n";

/// The consumer projects `v.major` off the realized record response.
const VERSION_PROJECT_SOURCE: &str = "#[test]\nfn t() -> Stream<Check> {\n    let v = probe_version where { text: \"1.2.3\" };\n    yield expect_eq(v.major, 1);\n}\n";

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

/// Register `probe` under `policy` with a begin-counting `Int`-request responder,
/// then drive `EFFECT_SOURCE` (`probe where { n: 5 } -> String`).
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
    drive(set, begins, EFFECT_SOURCE, evaluations)
}

/// Register `probe` under `policy` with a begin-counting `String`-request
/// responder, then drive `TEXT_SOURCE` (`probe where { text: "1.2.3" } -> String`).
/// This is the load-bearing reference-resolution path: the request record's
/// `String` field is a store reference the scheduler must resolve before decode.
fn run_text_probe(
    policy: MemoPolicy,
    responder: impl Fn(String) -> Result<String, PrimitiveFailure> + Send + Sync + 'static,
    evaluations: usize,
) -> Outcome {
    let begins = Arc::new(AtomicUsize::new(0));
    let begins_in_closure = begins.clone();
    let mut set = PrimitiveSet::new();
    set.register_function::<String, TextRequest, _>("probe", policy, move |req: TextRequest| {
        begins_in_closure.fetch_add(1, Ordering::SeqCst);
        responder(req.text)
    })
    .expect("probe registers");
    drive(set, begins, TEXT_SOURCE, evaluations)
}

/// Register `probe_version` under `policy` returning a `Version` record, then
/// drive `source`. The response is an aggregate: the request `String` field
/// resolves through the Task-1 reference resolver, and the record response frames
/// as an identity tree the consumer reads back as a realized record input.
fn run_version_probe(
    policy: MemoPolicy,
    source: &str,
    responder: impl Fn(String) -> Result<Version, PrimitiveFailure> + Send + Sync + 'static,
    evaluations: usize,
) -> Outcome {
    let begins = Arc::new(AtomicUsize::new(0));
    let begins_in_closure = begins.clone();
    let mut set = PrimitiveSet::new();
    set.register_function::<Version, TextRequest, _>(
        "probe_version",
        policy,
        move |req: TextRequest| {
            begins_in_closure.fetch_add(1, Ordering::SeqCst);
            responder(req.text)
        },
    )
    .expect("probe_version registers");
    drive(set, begins, source, evaluations)
}

/// Derive the compiler manifest from `set` (so effect ids match by construction),
/// compile `source`, lower the request + consumer islands, and evaluate the
/// consumer island `evaluations` times against a runtime carrying `set`. Shared
/// by every effect-primitive certificate so each varies only the registration
/// and the source.
fn drive(
    set: PrimitiveSet,
    begins: Arc<AtomicUsize>,
    source: &str,
    evaluations: usize,
) -> Outcome {
    let manifest = set.compiler_manifest();

    let compilation = Compiler::new()
        .with_primitives(manifest)
        .compile(source)
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

/// A `String` request field realizes as a store-resident `FrozenValue::Reference`.
/// The scheduler resolves that reference against the store before decode, so the
/// primitive sees a reference-free request tree, decodes the `String`, and echoes
/// it — the consumer's `expect_eq(v, "1.2.3")` resolves cleanly. Without the
/// resolver the adapter's `decode_value` rejects the reference and the effect
/// fails on the machine plane (an `Err` from `evaluate`).
#[test]
fn string_request_resolves_store_reference() {
    let outcome = run_text_probe(MemoPolicy::Hermetic, Ok, 1);
    let evaluation = outcome.results[0]
        .as_ref()
        .expect("the resolved string request decodes and dispatches, never a machine error");
    assert!(
        evaluation.failure.is_none(),
        "the echoed \"1.2.3\" matches, so the consumer passes, got {:?}",
        evaluation.failure
    );
    assert_eq!(
        outcome.begins, 1,
        "the primitive was begun once with the decoded string request"
    );
    assert_eq!(outcome.effect_spawns, 1, "one real dispatch");
}

/// A registered primitive returns a `Version` record. It frames as an aggregate
/// identity tree (empty resident bytes); the consumer reads it back as a realized
/// record input and projects `v.major`. Two Hermetic demands fold the same
/// aggregate response identity into the same consumer key, so the primitive is
/// begun exactly once.
#[test]
fn record_response_is_projected_by_the_consumer() {
    let outcome = run_version_probe(
        MemoPolicy::Hermetic,
        VERSION_PROJECT_SOURCE,
        |_| {
            Ok(Version {
                major: 1,
                minor: 2,
                patch: 3,
            })
        },
        2,
    );
    for result in &outcome.results {
        let evaluation = result
            .as_ref()
            .expect("the record response resolves and the projection evaluates");
        assert!(
            evaluation.failure.is_none(),
            "v.major == 1 passes, got {:?}",
            evaluation.failure
        );
    }
    assert_eq!(
        outcome.begins, 1,
        "the aggregate response folds into the consumer key — one dispatch across two demands"
    );
    assert_eq!(outcome.effect_spawns, 1, "one real dispatch");
    assert_eq!(outcome.dispatched, 1, "one dispatch event; the second demand is a memo hit");
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

// ---------------------------------------------------------------------------
// Phase 06 perf gate (report deliverable). This is a micro-benchmark, not a
// pass/fail assertion, so it is `#[ignore]`d out of the suite and run
// explicitly:
//
//   cargo test -p vix --test primitive_scheduler -- --ignored --nocapture
//
// It measures the marginal overhead a registered primitive adds over the pure
// demand machinery every island already pays. The claim it substantiates
// (design §Testing, "primitive-call overhead dominated by the demand machinery
// already paid by pure islands; conversion cost linear in value size") is that
// a Hermetic dispatch is the same order as a pure island evaluate plus a
// conversion cost that grows with the response value, and that a fully-memoized
// effectful demand is far cheaper than a dispatch.

/// Wall-time per op of evaluating `source` with a FRESH runtime each iteration,
/// so every effect demand is a real dispatch (never a memo hit): request eval +
/// reference resolution + decode + closure + response encode/intern + memo
/// insert + consumer eval.
fn bench_effect_dispatch(set: PrimitiveSet, source: &str, iterations: usize) -> f64 {
    let arc = Arc::new(set);
    let manifest = arc.compiler_manifest();
    let compilation = Compiler::new()
        .with_primitives(manifest)
        .compile(source)
        .expect("bench source compiles");
    let partitioned = compilation
        .module
        .partition_test(&compilation.module.tests[0]);
    let consumer = partitioned.islands[0].clone();
    let request = partitioned.effect_islands[0].island.clone();
    let mut request_cache = LoweringCache::default();
    let mut consumer_cache = LoweringCache::default();
    let request_lowered = request_cache.get_or_lower(&request).expect("request lowers");
    let consumer_lowered = consumer_cache
        .get_or_lower(&consumer)
        .expect("consumer lowers");
    let request_attr = attribution_for(&request);
    let consumer_attr = attribution_for(&consumer);
    let request_loc = Location::for_test_value("perf", "request");
    let consumer_loc = Location::for_test_value("perf", "consumer");
    let start = std::time::Instant::now();
    for _ in 0..iterations {
        let mut runtime = Runtime::new(EventLog::default()).with_primitives(arc.clone());
        let effects = [EffectDemand {
            request_island: request.id,
            request_location: &request_loc,
            request_lowered,
            request_attribution: &request_attr,
            request_arguments: &[],
            request_wires: &[],
        }];
        runtime
            .evaluate(
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
            )
            .expect("bench dispatch evaluates");
    }
    start.elapsed().as_nanos() as f64 / iterations as f64
}

/// Wall-time per op of a fully-memoized effectful demand: ONE Hermetic runtime,
/// warmed once, then `iterations` more evaluates that all hit the effect memo
/// (no `begin`) and then the consumer memo.
fn bench_effect_memo_hits(set: PrimitiveSet, source: &str, iterations: usize) -> f64 {
    let arc = Arc::new(set);
    let manifest = arc.compiler_manifest();
    let compilation = Compiler::new()
        .with_primitives(manifest)
        .compile(source)
        .expect("bench source compiles");
    let partitioned = compilation
        .module
        .partition_test(&compilation.module.tests[0]);
    let consumer = partitioned.islands[0].clone();
    let request = partitioned.effect_islands[0].island.clone();
    let mut request_cache = LoweringCache::default();
    let mut consumer_cache = LoweringCache::default();
    let request_lowered = request_cache.get_or_lower(&request).expect("request lowers");
    let consumer_lowered = consumer_cache
        .get_or_lower(&consumer)
        .expect("consumer lowers");
    let request_attr = attribution_for(&request);
    let consumer_attr = attribution_for(&consumer);
    let request_loc = Location::for_test_value("perf", "request");
    let consumer_loc = Location::for_test_value("perf", "consumer");
    let mut runtime = Runtime::new(EventLog::default()).with_primitives(arc);
    let evaluate = |runtime: &mut Runtime<EventLog>| {
        let effects = [EffectDemand {
            request_island: request.id,
            request_location: &request_loc,
            request_lowered,
            request_attribution: &request_attr,
            request_arguments: &[],
            request_wires: &[],
        }];
        runtime
            .evaluate(
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
            )
            .expect("bench memo-hit evaluates");
    };
    evaluate(&mut runtime); // warm: the single real dispatch
    let start = std::time::Instant::now();
    for _ in 0..iterations {
        evaluate(&mut runtime);
    }
    start.elapsed().as_nanos() as f64 / iterations as f64
}

/// Wall-time per op of a pure island evaluate with a FRESH runtime each
/// iteration — the baseline demand machinery every island pays, with no effect.
fn bench_pure_island(source: &str, iterations: usize) -> f64 {
    let compilation = Compiler::new().compile(source).expect("pure source compiles");
    let partitioned = compilation
        .module
        .partition_test(&compilation.module.tests[0]);
    let island = partitioned.islands[0].clone();
    let mut cache = LoweringCache::default();
    let lowered = cache.get_or_lower(&island).expect("island lowers");
    let attribution = attribution_for(&island);
    let location = Location::for_test_value("perf", "pure");
    let start = std::time::Instant::now();
    for _ in 0..iterations {
        let mut runtime = Runtime::new(EventLog::default());
        runtime
            .evaluate(
                island.id,
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
            .expect("bench pure evaluates");
    }
    start.elapsed().as_nanos() as f64 / iterations as f64
}

fn hermetic_string_probe_set() -> PrimitiveSet {
    let mut set = PrimitiveSet::new();
    set.register_function::<String, ProbeRequest, _>("probe", MemoPolicy::Hermetic, |_| {
        Ok("9".to_owned())
    })
    .expect("probe registers");
    set
}

fn hermetic_record_probe_set() -> PrimitiveSet {
    let mut set = PrimitiveSet::new();
    set.register_function::<Version, TextRequest, _>(
        "probe_version",
        MemoPolicy::Hermetic,
        |_| {
            Ok(Version {
                major: 1,
                minor: 2,
                patch: 3,
            })
        },
    )
    .expect("probe_version registers");
    set
}

#[test]
#[ignore = "perf micro-benchmark; run explicitly with --ignored --nocapture"]
fn primitive_call_overhead_micro_benchmark() {
    const ITERATIONS: usize = 4000;
    let pure_source = "#[test]\nfn t() -> Stream<Check> {\n    yield expect_eq(1 + 1, 2);\n}\n";

    let pure = bench_pure_island(pure_source, ITERATIONS);
    let dispatch_scalar = bench_effect_dispatch(hermetic_string_probe_set(), EFFECT_SOURCE, ITERATIONS);
    let dispatch_record =
        bench_effect_dispatch(hermetic_record_probe_set(), VERSION_PROJECT_SOURCE, ITERATIONS);
    let memo_hit = bench_effect_memo_hits(hermetic_string_probe_set(), EFFECT_SOURCE, ITERATIONS);

    println!("PERF (ns/op over {ITERATIONS} iterations):");
    println!("  pure_island_evaluate      = {pure:>9.0}");
    println!("  hermetic_dispatch_scalar  = {dispatch_scalar:>9.0}  (Int req / String resp)");
    println!("  hermetic_dispatch_record  = {dispatch_record:>9.0}  (String req / Version resp)");
    println!("  effectful_memo_hit        = {memo_hit:>9.0}");
    println!(
        "  dispatch_overhead_scalar  = {:>9.0}  (dispatch - pure)",
        dispatch_scalar - pure
    );
    println!(
        "  conversion_delta_record   = {:>9.0}  (record - scalar dispatch)",
        dispatch_record - dispatch_scalar
    );
}
