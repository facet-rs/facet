//! The exec rail's acceptance tests: exec as a registered primitive
//! (`vix-core/docs/content/exec-rail.md`, "Acceptance").
//!
//! Every test here drives a real program through the ratchet runner and a real
//! process through the installed `ExecBackend`. What they pin is not that exec
//! *works* — the rung corpus does that — but the two pipeline laws that make
//! the rail worth building, and the identity claim that makes the move safe:
//!
//! 1. **Demanding the answer synchronizes.** The settled tail of an exec
//!    response is a fact about a *finished* process, so demanding it parks
//!    until termination.
//! 2. **Not demanding it doesn't.** A product announced through the output
//!    protocol resolves — and dependent work completes — while the process is
//!    still running. This is the `.rmeta` pipeline, pinned as fact.
//! 3. **Nothing re-keys.** The same program under the same capability is one
//!    demand, at any number of source sites, across runs.

use vix::runtime::{DemandKey, EventKind, ValueId};
use vixen_runtime::ratchet::{RatchetReport, SuiteRun, run_source};

/// The sequence number of the `Completed` event that published `identity`.
/// Event sequence is the scheduler's own causal order — a wall clock is never
/// consulted, so these assertions are not timing races.
fn completed_at(lane: &SuiteRun, identity: &ValueId) -> u64 {
    lane.events
        .iter()
        .find_map(|event| match &event.kind {
            EventKind::Completed {
                identity: completed,
                ..
            } if completed == identity => Some(event.sequence),
            _ => None,
        })
        .expect("a published value has a completion event")
}

/// The sequence number of the first `EffectSpawned` event — the moment the
/// process boundary was crossed.
fn spawned_at(lane: &SuiteRun) -> u64 {
    lane.events
        .iter()
        .find_map(|event| match &event.kind {
            EventKind::EffectSpawned { .. } => Some(event.sequence),
            _ => None,
        })
        .expect("the exec effect was spawned")
}

fn value_identity(lane: &SuiteRun, provenance: vix::vir::ValueIslandId) -> ValueId {
    lane.values
        .iter()
        .find(|value| value.provenance == provenance)
        .expect("the island published a value")
        .identity
        .clone()
}

fn passed(report: &RatchetReport) -> bool {
    report.passed()
}

/// Acceptance 1: **demanding the answer synchronizes.**
///
/// The program demands the settled tail of the exec response — the stdout the
/// termination grammar produced. The process prints its last line only after
/// sleeping, so an outcome containing that line is proof the demand did not
/// resolve until the process had run to completion; the program's own
/// `expect_eq` is the assertion. The event order underneath says the same
/// thing structurally: the task parked at the effect boundary after the spawn
/// and its `Completed` came later, so the scheduler waited on an event rather
/// than spinning or reading the filesystem early.
///
/// r[verify machine.primitive.exec-outcome]
/// r[verify machine.primitive.progressive-response]
#[test]
fn demanding_the_settled_answer_parks_until_the_process_terminates() {
    const SOURCE: &str = r#"
#[test]
fn settled_tail(sh: Sh) -> Stream<Check> {
    let out = exec sh`-c "printf 'first\n'; sleep 0.2; printf 'last\n'"`;
    yield expect_eq(out.stdout.collect().values(), ["first", "last"]);
}
"#;
    let report = run_source(SOURCE).expect("the settled-tail program runs");
    assert!(
        passed(&report),
        "the settled tail contains the line printed after the sleep: {report:#?}"
    );

    for lane in [&report.plain, &report.chaos] {
        assert_eq!(
            lane.counters.effect_spawns, 1,
            "one process was spawned for the one exec demand"
        );
        let spawned = spawned_at(lane);
        // The frame is off the recursive Rust stack while the process runs:
        // a Parked transition after the spawn, and a resume after it.
        let parked_after_spawn = lane.events.iter().any(|event| {
            event.sequence > spawned
                && matches!(
                    &event.kind,
                    EventKind::TaskTransition {
                        to: vix::runtime::TaskState::Parked,
                        ..
                    }
                )
        });
        assert!(
            parked_after_spawn,
            "the exec demand parked at the effect boundary rather than blocking the scheduler"
        );
        let outcome_completed = lane
            .events
            .iter()
            .filter_map(|event| match &event.kind {
                EventKind::Completed { .. } => Some(event.sequence),
                _ => None,
            })
            .max()
            .expect("the exec outcome completed");
        assert!(
            outcome_completed > spawned,
            "the answer resolved only after the process boundary was crossed"
        );
    }
}

/// Acceptance 2: **the rustc simulation.**
///
/// A producer writes an early product, announces it through the `vix-ready`
/// output protocol, then keeps running — sleeping, writing a second product,
/// and only then exiting. The program demands the early product and does
/// dependent work on it (a string comparison whose result is a separate
/// check), and separately demands the full outcome.
///
/// The pinned facts, in scheduler event order:
///
/// - the early product's value completed BEFORE the producer's aggregate
///   outcome did — i.e. before process termination, because the outcome is a
///   fact about a terminated process and its stdout proves the process ran
///   past the sleep;
/// - the protocol, not the effect's completion, was the serving authority
///   (`progressive_effect_protocol_publications == 1`, completion fallback
///   zero), which is exactly "the product was published while the process was
///   running";
/// - the dependent check consuming the early product also completed before the
///   aggregate outcome.
///
/// Together with the settled-tail test this pins that products progress
/// independently of termination and that the answer alone is the
/// synchronization point. This is the `.rmeta` pipeline as fact.
///
/// r[verify machine.primitive.progressive-response]
/// r[verify machine.primitive.exec-outcome]
#[test]
fn an_announced_product_and_its_dependent_work_complete_before_the_producer_exits() {
    const SOURCE: &str = r#"
#[test]
fn rmeta_pipeline(sh: ProgressiveSh) -> Stream<Check> {
    let producer = exec sh`-c "mkdir -p out; printf rmeta > out/lib.rmeta; printf 'vix-ready\tout/lib.rmeta\n'; sleep 0.3; printf done > out/lib.rlib; printf 'linked\n'"`;
    let early = (producer.tree / "out" / "lib.rmeta").text();
    yield expect_eq(early, "rmeta");
    yield expect_eq(producer.stdout.collect().values(), ["linked"]);
}
"#;
    let module = vixen_runtime::default_compiler()
        .compile(SOURCE)
        .expect("the rustc-simulation program compiles");
    let partitioned = module.partition_test(&module.tests[0]);
    let [projection] = partitioned.progressive_values.as_slice() else {
        panic!("the early product is a progressive value island");
    };
    let (producer_island, early_island) = (projection.producer, projection.id);

    let report = run_source(SOURCE).expect("the rustc-simulation program runs");
    assert!(
        passed(&report),
        "the early product reads `rmeta` and the settled stdout reads `linked`: {report:#?}"
    );

    for lane in [&report.plain, &report.chaos] {
        assert_eq!(
            lane.counters.progressive_effect_protocol_publications, 1,
            "the output protocol served the early product while the process ran"
        );
        assert_eq!(
            lane.counters.progressive_effect_completion_publications, 0,
            "the completion fallback was never needed — the product was announced in flight"
        );
        let producer_at = completed_at(lane, &value_identity(lane, producer_island));
        let early_at = completed_at(lane, &value_identity(lane, early_island));
        assert!(
            early_at < producer_at,
            "the early product resolved before the producer's aggregate outcome"
        );
        // The dependent work — the check that consumes the early product — is
        // itself complete before the producer settles. `expect_eq(early, ..)`
        // is the first-yielded check, and its value completes in the same
        // pre-termination window.
        let dependent_at = lane
            .events
            .iter()
            .filter_map(|event| match &event.kind {
                EventKind::Completed { .. } if event.sequence > early_at => Some(event.sequence),
                _ => None,
            })
            .min()
            .expect("dependent work completed after the early product");
        assert!(
            dependent_at < producer_at,
            "work depending on the early product completed before the producer terminated"
        );
    }
}

/// Acceptance 3: **nothing re-keys**, at the identity the move preserves.
///
/// The rail derives the effect demand from the primitive's declaration rather
/// than from a hand-rolled exec arm, so the *closure hash* is now the generic
/// `vix.primitive.effect-plan.v1` framing rather than the retired
/// `vix.exec.plan.v1` one — an honest delta against the design's "byte-for-byte"
/// wording (see the exec-rail note). What the move preserves — and what
/// identity actually means here — is every discriminating property of the
/// tier-1 exec key:
///
/// - the same plan under the same capability is ONE demand, so a second
///   spelling at a second source site spawns nothing (rung 069's content);
/// - a different plan is a different demand;
/// - a different capability is a different demand, through the preimage's
///   ARGUMENTS, not its closure.
///
/// r[verify machine.primitive.capability-role]
/// r[verify machine.primitive.exec-identity]
#[test]
fn the_same_plan_under_the_same_capability_is_one_demand_at_every_site() {
    const SOURCE: &str = r#"
#[test]
fn one_demand(echo: Echo) -> Stream<Check> {
    let a = exec echo`"once"`;
    let b = exec echo`"once"`;
    yield expect_eq(a.stdout, b.stdout);
    yield ran_processes(1);
}
"#;
    let report = run_source(SOURCE).expect("the shared-plan program runs");
    assert!(passed(&report), "both spellings agree: {report:#?}");
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(
            lane.counters.effect_spawns, 1,
            "two source sites, one plan under one capability, one process"
        );
        assert!(
            lane.counters.memo_hits_exact >= 1,
            "the second site was served from the memo, not recomputed"
        );
    }
}

/// The other half of criterion 3: a different plan, and a different
/// capability, each re-key — so "one demand" above is a fact about identity
/// rather than an accident of the frontier collapsing everything.
///
/// r[verify machine.primitive.capability-role]
#[test]
fn a_different_plan_or_capability_is_a_different_demand() {
    const DIFFERENT_PLAN: &str = r#"
#[test]
fn distinct_plans(echo: Echo) -> Stream<Check> {
    let a = exec echo`"once"`;
    let b = exec echo`"twice"`;
    yield expect_eq(a.stdout.collect().values(), ["once"]);
    yield expect_eq(b.stdout.collect().values(), ["twice"]);
    yield ran_processes(2);
}
"#;
    let report = run_source(DIFFERENT_PLAN).expect("the distinct-plan program runs");
    assert!(passed(&report), "distinct plans both run: {report:#?}");
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(
            lane.counters.effect_spawns, 2,
            "a different normalized plan is a different demand"
        );
    }

    // Two capabilities whose PROGRAM is identical would still be two demands
    // if their identities differ; here they differ in both, which is the
    // ordinary case a build graph produces.
    const DIFFERENT_CAPABILITY: &str = r#"
#[test]
fn distinct_capabilities(echo: Echo, sh: Sh) -> Stream<Check> {
    let a = exec echo`"once"`;
    let b = exec sh`-c "printf 'once\n'"`;
    yield expect_eq(a.stdout.collect().values(), b.stdout.collect().values());
    yield ran_processes(2);
}
"#;
    let report = run_source(DIFFERENT_CAPABILITY).expect("the distinct-capability program runs");
    assert!(
        passed(&report),
        "distinct capabilities produce equal output through distinct demands: {report:#?}"
    );
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.effect_spawns, 2);
    }
}

/// The identity claim, at the demand key itself: the derivation is a pure
/// function of the primitive declaration, the normalized plan, and the
/// capability identity — with NO source location in it. Pinned by
/// construction so a future change to the framing is a deliberate, visible
/// re-keying rather than a silent one.
///
/// r[verify machine.primitive.capability-role]
#[test]
fn the_effect_demand_key_is_a_pure_function_of_plan_and_capability() {
    use vix::runtime::{
        ArgRole, DemandPreimage, FramedNode, PrimitiveField, PrimitiveFieldValue, PrimitiveValue,
        PrimitiveValueBody, RequestShape, declared_effect_preimage, exec_primitive_id,
        exec_request_type,
    };
    use vix::vir::Type;

    // The exec outcome's `tree` field is the embedder-declared `Tree` host
    // type; reserve its schema name as the runtime does (idempotent).
    vixen_primitives::register_host_types();
    let capability_ty = vix::compiler::capability_type("Echo");
    let shape = RequestShape {
        args: vec![
            ArgRole::Capability {
                expected: capability_ty.clone(),
            },
            ArgRole::Value {
                expected: Type::Array(Box::new(Type::String)),
            },
        ],
        request_ty: exec_request_type(&capability_ty),
        result: vix::compiler::exec_outcome_type(),
        primitive: exec_primitive_id(),
    };

    let request = |program: &str, argv: &[&str]| -> PrimitiveValue {
        let capability = PrimitiveValue {
            schema: capability_ty.schema_ref(),
            body: PrimitiveValueBody::Product(vec![
                PrimitiveField {
                    schema: Type::String.schema_ref(),
                    value: PrimitiveFieldValue::Child(Box::new(PrimitiveValue::bytes(
                        Type::String.schema_ref(),
                        program.as_bytes().to_vec(),
                    ))),
                },
                PrimitiveField {
                    schema: Type::option(Type::String).schema_ref(),
                    value: PrimitiveFieldValue::Child(Box::new(PrimitiveValue {
                        schema: Type::option(Type::String).schema_ref(),
                        body: PrimitiveValueBody::Variant {
                            tag: vix::vir::OPTION_NONE_VARIANT,
                            fields: Vec::new(),
                        },
                    })),
                },
                PrimitiveField {
                    schema: Type::Array(Box::new(Type::String)).schema_ref(),
                    value: PrimitiveFieldValue::Child(Box::new(PrimitiveValue {
                        schema: Type::Array(Box::new(Type::String)).schema_ref(),
                        body: PrimitiveValueBody::Sequence {
                            element_schema: Type::String.schema_ref(),
                            elements: Vec::new(),
                        },
                    })),
                },
            ]),
        };
        let argv_ty = Type::Array(Box::new(Type::String));
        let argv = PrimitiveValue {
            schema: argv_ty.schema_ref(),
            body: PrimitiveValueBody::Sequence {
                element_schema: Type::String.schema_ref(),
                elements: argv
                    .iter()
                    .map(|element| {
                        PrimitiveValue::bytes(
                            Type::String.schema_ref(),
                            element.as_bytes().to_vec(),
                        )
                    })
                    .collect(),
            },
        };
        PrimitiveValue {
            schema: shape.request_ty.schema_ref(),
            body: PrimitiveValueBody::Product(vec![
                PrimitiveField {
                    schema: capability.schema.clone(),
                    value: PrimitiveFieldValue::Child(Box::new(capability)),
                },
                PrimitiveField {
                    schema: argv.schema.clone(),
                    value: PrimitiveFieldValue::Child(Box::new(argv)),
                },
            ]),
        }
    };
    let preimage = |program: &str, argv: &[&str]| -> DemandPreimage {
        let request = request(program, argv);
        declared_effect_preimage(
            &shape.primitive,
            Some(&shape),
            &request.identity(),
            &request,
        )
    };
    let key = |program: &str, argv: &[&str]| DemandKey::from_preimage(&preimage(program, argv));

    // The capability's identity IS the preimage argument — never its program
    // bytes, and never the plan.
    let base = preimage("echo", &["once"]);
    let string_ty = Type::String;
    let toolchain_ty = Type::option(Type::String);
    let targets_ty = Type::Array(Box::new(Type::String));
    let capability_identity = FramedNode::Variant {
        schema: capability_ty.schema_ref(),
        tag: 0,
        fields: vec![
            vix::runtime::FramedField {
                schema: string_ty.schema_ref(),
                value: vix::runtime::FramedValue::Optional(Some(
                    FramedNode::leaf(string_ty.schema_ref(), b"echo".to_vec()).identity(),
                )),
            },
            vix::runtime::FramedField {
                schema: toolchain_ty.schema_ref(),
                value: vix::runtime::FramedValue::Optional(Some(
                    FramedNode::Variant {
                        schema: toolchain_ty.schema_ref(),
                        tag: vix::vir::OPTION_NONE_VARIANT.into(),
                        fields: Vec::new(),
                    }
                    .identity(),
                )),
            },
            vix::runtime::FramedField {
                schema: targets_ty.schema_ref(),
                value: vix::runtime::FramedValue::Optional(Some(
                    FramedNode::SeqChildren {
                        schema: targets_ty.schema_ref(),
                        element_schema: string_ty.schema_ref(),
                        children: Vec::new(),
                    }
                    .identity(),
                )),
            },
        ],
    }
    .identity();
    assert_eq!(
        base.arguments,
        vec![capability_identity],
        "the capability argument's identity keys the demand, in declaration order"
    );

    // Determinism, and the two discriminations, at the key.
    assert_eq!(key("echo", &["once"]), key("echo", &["once"]));
    assert_ne!(
        key("echo", &["once"]),
        key("echo", &["twice"]),
        "a different plan re-keys"
    );
    assert_ne!(
        key("echo", &["once"]),
        key("sh", &["once"]),
        "a different capability re-keys"
    );
    // …and each re-keys through its OWN half of the preimage.
    assert_eq!(base.closure, preimage("sh", &["once"]).closure);
    assert_ne!(base.arguments, preimage("sh", &["once"]).arguments);
    assert_ne!(base.closure, preimage("echo", &["twice"]).closure);
    assert_eq!(base.arguments, preimage("echo", &["twice"]).arguments);
}
