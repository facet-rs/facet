use vix::compiler::Compiler;
use vix::lowering::{LoweringCache, source_map_for};
use vix::ratchet::run_source;
use vix::runtime::{DemandState, EventKind, MemoVerdict, TaskState};
use vix::vir::Op as VirOp;
use weavy::task::Op as WeavyOp;

const RUNG_001: &str = include_str!("ratchet/001-harness.vix");
const RUNG_002: &str = include_str!("ratchet/002-arithmetic.vix");
const RUNG_003: &str = include_str!("ratchet/003-bindings.vix");
const RUNG_004: &str = include_str!("ratchet/004-functions.vix");
const RUNG_005: &str = include_str!("ratchet/005-tuples.vix");

/// The first rung is an architectural certificate, not just a boolean test.
///
/// r[verify machine.identity.value-identity-pair]
/// r[verify machine.identity.hash-at-construction]
/// r[verify machine.store.handle-opaque]
/// r[verify machine.store.dedup]
/// r[verify machine.memo.demand-key]
/// r[verify machine.memo.no-recompute-at-lookup]
/// r[verify machine.obs.event-vocabulary]
/// r[verify machine.obs.event-sink]
/// r[verify machine.scheduler.chaos-kill-oracle]
/// r[verify machine.scheduler.replay-is-semantics]
#[test]
fn rung_001_certifies_the_new_compiler_and_runtime_spine() {
    let module = Compiler::new()
        .compile(RUNG_001)
        .expect("rung 001 compiles");
    assert_eq!(module.tests.len(), 1);
    assert_eq!(module.tests[0].name, "the_ratchet_begins");

    let rendered_vir = module.render();
    assert!(rendered_vir.contains("Bool(true)"));
    assert!(rendered_vir.contains("Expect"));

    let partitioned = module.partition_test(&module.tests[0]);
    assert_eq!(partitioned.islands.len(), 1);
    let mut lowering_cache = LoweringCache::default();
    let lowered = lowering_cache
        .get_or_lower(&partitioned.islands[0])
        .expect("rung 001 lowers to Weavy");
    let source_map = source_map_for(&partitioned.islands[0]);
    assert_eq!(source_map.len(), 2);
    let rendered_weavy = lowered.render();
    let recipe = lowered.recipe;
    assert!(rendered_weavy.contains("Trace { id: 0 }"));
    assert!(rendered_weavy.contains("ConstI64 { dst: 0, value: 1 }"));
    assert!(rendered_weavy.contains("CopyI64 { dst: 8, src: 0 }"));
    assert!(rendered_weavy.contains("Ret { src: 8, size: 8 }"));

    let shifted_source = format!("\n{RUNG_001}");
    let shifted_module = Compiler::new()
        .compile(&shifted_source)
        .expect("span-only edit compiles");
    let shifted = shifted_module.partition_test(&shifted_module.tests[0]);
    let shifted_lowered = lowering_cache
        .get_or_lower(&shifted.islands[0])
        .expect("span-only edit reuses bytecode");
    assert_eq!(shifted_lowered.recipe, recipe);
    let shifted_source_map = source_map_for(&shifted.islands[0]);
    assert_ne!(source_map[0].span, shifted_source_map[0].span);
    assert_eq!(lowering_cache.counters().misses, 1);
    assert_eq!(lowering_cache.counters().hits, 1);

    let report = run_source(RUNG_001).expect("rung 001 runs");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 1);
    assert_eq!(report.plain.checks, report.chaos.checks);

    assert_eq!(report.plain.counters.memo_misses, 1);
    assert_eq!(report.plain.counters.memo_hits_exact, 0);
    assert_eq!(report.plain.counters.memo_hit_allocations, 0);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
    assert_eq!(report.plain.counters.effect_spawns, 0);
    assert_eq!(report.plain.counters.store_interns, 1);
    assert_eq!(report.plain.counters.scheduler_requests, 1);
    assert_eq!(report.plain.counters.task_spawns, 1);
    assert_eq!(report.plain.counters.task_discards, 0);
    assert_eq!(report.plain.receipt_count, 0);

    assert_eq!(report.chaos.counters.memo_misses, 1);
    assert_eq!(report.chaos.counters.store_interns, 1);
    assert_eq!(report.chaos.counters.scheduler_requests, 2);
    assert_eq!(report.chaos.counters.task_spawns, 2);
    assert_eq!(report.chaos.counters.task_discards, 1);
    assert_eq!(report.chaos.receipt_count, 0);

    assert_contiguous_sequences(&report.plain.events);
    assert_contiguous_sequences(&report.chaos.events);
    assert!(report.plain.events.iter().any(|event| matches!(
        event.kind,
        EventKind::Memo {
            verdict: MemoVerdict::Miss,
            ..
        }
    )));
    assert!(report.plain.events.iter().any(|event| matches!(
        event.kind,
        EventKind::DemandTransition {
            from: DemandState::Running,
            to: DemandState::Ready,
            ..
        }
    )));
    assert!(report.plain.events.iter().any(|event| matches!(
        event.kind,
        EventKind::WeavyMark { node, .. } if node.0 == 0
    )));
    assert!(report.plain.events.iter().any(|event| matches!(
        event.kind,
        EventKind::WeavyMark { node, .. } if node.0 == 1
    )));
    assert!(report.chaos.events.iter().any(|event| matches!(
        event.kind,
        EventKind::TaskTransition {
            from: TaskState::Running,
            to: TaskState::Discarded,
            ..
        }
    )));
    assert!(report.chaos.events.iter().any(|event| matches!(
        event.kind,
        EventKind::DemandTransition {
            from: DemandState::Running,
            to: DemandState::Queued,
            ..
        }
    )));
}

#[test]
fn rung_002_integer_arithmetic_runs_through_vir_and_weavy() {
    let report = run_source(RUNG_002).expect("rung 002 compiles and runs");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 3);
    assert_eq!(report.plain.checks, report.chaos.checks);
    assert_eq!(report.plain.receipt_count, 0);
    assert_eq!(report.chaos.receipt_count, 0);
}

#[test]
fn rung_003_lexical_bindings_and_strings_run_through_vir_and_weavy() {
    let module = Compiler::new()
        .compile(RUNG_003)
        .expect("rung 003 compiles");
    let rendered_vir = module.render();
    assert!(rendered_vir.contains("String(\"hello\")"));
    assert!(rendered_vir.contains("Eq"));

    let partitioned = module.partition_test(&module.tests[0]);
    let mut lowering_cache = LoweringCache::default();
    let lowered = lowering_cache
        .get_or_lower(&partitioned.islands[0])
        .expect("rung 003 lowers to Weavy");
    assert!(
        lowered
            .constants
            .iter()
            .any(|constant| constant.bytes.as_slice() == b"hello")
    );
    assert!(lowered.render().contains("EqI64"));

    let report = run_source(RUNG_003).expect("rung 003 compiles and runs");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 2);
    assert_eq!(report.plain.checks, report.chaos.checks);
    assert_eq!(report.plain.receipt_count, 0);
    assert_eq!(report.chaos.receipt_count, 0);
}

#[test]
fn rung_004_functions_and_application_run_through_vir_and_weavy() {
    let module = Compiler::new()
        .compile(RUNG_004)
        .expect("rung 004 compiles");
    let rendered_vir = module.render();
    assert!(rendered_vir.contains("Parameter"));
    assert!(rendered_vir.contains("Call(FunctionId"));

    let partitioned = module.partition_test(&module.tests[0]);
    assert_eq!(partitioned.islands.len(), 2);
    let mut lowering_cache = LoweringCache::default();
    for (index, island) in partitioned.islands.iter().enumerate() {
        assert_eq!(island.callees.len(), index + 1);
        let lowered = lowering_cache
            .get_or_lower(island)
            .expect("rung 004 lowers to Weavy");
        assert_eq!(lowered.program.fns.len(), index + 2);
        assert!(lowered.render().contains("Call {"));
    }

    let report = run_source(RUNG_004).expect("rung 004 compiles and runs");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 2);
    assert_eq!(report.plain.checks, report.chaos.checks);
    assert_eq!(report.plain.receipt_count, 0);
    assert_eq!(report.chaos.receipt_count, 0);
    assert!(report.plain.events.iter().any(|event| matches!(
        event.kind,
        EventKind::WeavyFrameEntered { function, .. } if function.0 == 0
    )));
    assert!(report.plain.events.iter().any(|event| matches!(
        event.kind,
        EventKind::WeavyFrameEntered { function, .. } if function.0 == 1
    )));
}

#[test]
fn rung_005_tuples_and_positional_projection_run_through_vir_and_weavy() {
    let module = Compiler::new()
        .compile(RUNG_005)
        .expect("rung 005 compiles");
    assert!(module.functions.iter().any(|function| {
        function
            .nodes
            .iter()
            .any(|node| matches!(node.op, VirOp::Tuple))
    }));
    assert!(module.functions.iter().any(|function| {
        function
            .nodes
            .iter()
            .any(|node| matches!(node.op, VirOp::Project { .. }))
    }));

    let partitioned = module.partition_test(&module.tests[0]);
    assert_eq!(partitioned.islands.len(), 3);
    let mut lowering_cache = LoweringCache::default();
    let lowered = lowering_cache
        .get_or_lower(&partitioned.islands[2])
        .expect("rung 005 lowers to Weavy");
    assert_eq!(lowered.program.fns.len(), 2);
    assert!(lowered.program.fns[0].code.iter().any(|op| matches!(
        op,
        WeavyOp::Call { args, .. } if args.iter().any(|argument| argument.size == 16)
    )));
    assert!(
        lowered.program.fns[1]
            .code
            .iter()
            .any(|op| matches!(op, WeavyOp::Ret { size: 16, .. }))
    );
    assert!(lowered.program.fns.iter().all(|function| {
        function
            .code
            .iter()
            .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
    }));

    let report = run_source(RUNG_005).expect("rung 005 compiles and runs");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 3);
    assert_eq!(report.plain.checks, report.chaos.checks);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
    assert_eq!(report.chaos.counters.pure_host_calls, 0);
    assert_eq!(report.plain.receipt_count, 0);
    assert_eq!(report.chaos.receipt_count, 0);
}

fn assert_contiguous_sequences(events: &[vix::runtime::Event]) {
    assert!(events.iter().enumerate().all(|(index, event)| {
        event.sequence == u64::try_from(index).expect("event count fits u64")
    }));
}
