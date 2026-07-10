use vix::compiler::Compiler;
use vix::diagnostic::DiagnosticCode;
use vix::lowering::{LoweringCache, source_map_for};
use vix::ratchet::run_source;
use vix::runtime::{DemandState, EventKind, MemoVerdict, TaskState};
use vix::surface::{SurfaceParser, ast};
use vix::vir::{Op as VirOp, Type as VirType, VariantPayload};
use weavy::task::Op as WeavyOp;

const RUNG_001: &str = include_str!("ratchet/001-harness.vix");
const RUNG_002: &str = include_str!("ratchet/002-arithmetic.vix");
const RUNG_003: &str = include_str!("ratchet/003-bindings.vix");
const RUNG_004: &str = include_str!("ratchet/004-functions.vix");
const RUNG_005: &str = include_str!("ratchet/005-tuples.vix");
const RUNG_006: &str = include_str!("ratchet/006-records.vix");
const RUNG_007: &str = include_str!("ratchet/007-enums.vix");
const RUNG_008: &str = include_str!("ratchet/008-spread.vix");
const RUNG_009: &str = include_str!("ratchet/009-structural-equality.vix");
const RUNG_010: &str = include_str!("ratchet/010-spaceship.vix");
const RUNG_011: &str = include_str!("ratchet/011-derived-comparisons.vix");
const RUNG_012: &str = include_str!("ratchet/012-total-order.vix");
const RUNG_013: &str = include_str!("ratchet/013-expression-statement.reject.vix");
const RUNG_014: &str = include_str!("ratchet/014-if-else.vix");
const RUNG_015: &str = include_str!("ratchet/015-boolean-operators.vix");
const RUNG_016: &str = include_str!("ratchet/016-match-expressions.vix");
const RUNG_017: &str = include_str!("ratchet/017-match-guards.vix");

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
        WeavyOp::Call { args, .. } if args.len() == 1 && args[0].size == 16
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

#[test]
fn rung_006_records_and_named_projection_run_through_vir_and_weavy() {
    let module = Compiler::new()
        .compile(RUNG_006)
        .expect("rung 006 compiles");
    assert_eq!(module.records.len(), 1);
    assert_eq!(module.records[0].name, "Point");
    assert_eq!(
        module.records[0]
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        ["x", "y"]
    );
    assert!(module.functions.iter().any(|function| {
        function
            .nodes
            .iter()
            .any(|node| matches!(node.op, VirOp::Record))
    }));
    let mut projected_fields = module
        .functions
        .iter()
        .flat_map(|function| &function.nodes)
        .filter_map(|node| match node.op {
            VirOp::Project { index } => Some(index),
            _ => None,
        })
        .collect::<Vec<_>>();
    projected_fields.sort_unstable();
    assert_eq!(projected_fields, [0, 1]);

    let partitioned = module.partition_test(&module.tests[0]);
    assert_eq!(partitioned.islands.len(), 2);
    let renamed_source = RUNG_006.replace("Point", "Pixel");
    let renamed = Compiler::new()
        .compile(&renamed_source)
        .expect("nominally renamed rung 006 compiles");
    let renamed = renamed.partition_test(&renamed.tests[0]);
    assert_ne!(
        partitioned.islands[0].canonical_recipe_bytes(),
        renamed.islands[0].canonical_recipe_bytes(),
        "a nominal record rename must change recipe identity"
    );

    let mut lowering_cache = LoweringCache::default();
    for island in &partitioned.islands {
        let lowered = lowering_cache
            .get_or_lower(island)
            .expect("rung 006 lowers to Weavy");
        assert!(lowered.program.fns.iter().all(|function| {
            function
                .code
                .iter()
                .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
        }));
        assert!(lowered.program.fns.iter().any(|function| {
            function
                .code
                .iter()
                .any(|op| matches!(op, WeavyOp::CopyI64 { .. }))
        }));
    }

    let report = run_source(RUNG_006).expect("rung 006 compiles and runs");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 2);
    assert_eq!(report.plain.checks, report.chaos.checks);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
    assert_eq!(report.chaos.counters.pure_host_calls, 0);
    assert_eq!(report.plain.receipt_count, 0);
    assert_eq!(report.chaos.receipt_count, 0);
}

#[test]
fn rung_007_enums_payloads_and_match_run_through_vir_and_weavy() {
    let module = Compiler::new()
        .compile(RUNG_007)
        .expect("rung 007 compiles");
    assert_eq!(module.enums.len(), 1);
    let shape = &module.enums[0];
    assert_eq!(shape.name, "Shape");
    assert_eq!(shape.variants.len(), 2);
    assert!(matches!(
        &shape.variants[0].payload,
        VariantPayload::Tuple(elements) if elements == &[VirType::Int]
    ));
    assert!(matches!(
        &shape.variants[1].payload,
        VariantPayload::Record(fields)
            if fields.iter().map(|field| field.name.as_str()).collect::<Vec<_>>() == ["w", "h"]
    ));
    let enum_words = VirType::Enum(shape.clone())
        .word_width()
        .expect("Shape has a finite inline layout");
    assert_eq!(enum_words, 3);
    assert!(module.functions.iter().any(|function| {
        function
            .nodes
            .iter()
            .any(|node| matches!(&node.op, VirOp::Match { arms } if arms.len() == 2))
    }));
    assert!(module.functions.iter().any(|function| {
        function
            .nodes
            .iter()
            .any(|node| matches!(node.op, VirOp::VariantProject { .. }))
    }));

    let partitioned = module.partition_test(&module.tests[0]);
    assert_eq!(partitioned.islands.len(), 2);
    let mut lowering_cache = LoweringCache::default();
    for island in &partitioned.islands {
        let source_map = source_map_for(island);
        let variant_node = island
            .nodes
            .iter()
            .find(|node| matches!(node.op, VirOp::Variant { .. }))
            .expect("each check constructs one Shape variant");
        let VirOp::Variant { variant } = &variant_node.op else {
            unreachable!("variant node was selected above")
        };
        let trace_id = source_map
            .iter()
            .find(|entry| entry.function == island.function && entry.node == variant_node.id)
            .expect("variant node has source attribution")
            .trace_id;
        let lowered = lowering_cache
            .get_or_lower(island)
            .expect("rung 007 lowers to Weavy");
        let entry = &lowered.program.fns[0].code;
        let trace_pc = entry
            .iter()
            .position(|op| matches!(op, WeavyOp::Trace { id } if *id == trace_id))
            .expect("variant construction has a Weavy trace mark");
        assert!(
            entry[trace_pc + 1..trace_pc + 1 + enum_words]
                .iter()
                .all(|op| matches!(op, WeavyOp::ConstI64 { value: 0, .. }))
        );
        assert!(matches!(
            &entry[trace_pc + 1 + enum_words],
            WeavyOp::ConstI64 { value, .. } if *value == i64::from(*variant)
        ));
        assert!(lowered.program.fns.iter().any(|function| {
            function.code.iter().any(
                |op| matches!(op, WeavyOp::Call { args, .. } if args.len() == 1 && args[0].size == 24),
            )
        }));
        assert!(lowered.program.fns.iter().any(|function| {
            function
                .code
                .iter()
                .any(|op| matches!(op, WeavyOp::JumpIfZero { .. }))
        }));
        assert!(lowered.program.fns.iter().any(|function| {
            function
                .code
                .iter()
                .any(|op| matches!(op, WeavyOp::Jump { .. }))
        }));
        assert!(lowered.program.fns.iter().all(|function| {
            function
                .code
                .iter()
                .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
        }));
    }

    let report = run_source(RUNG_007).expect("rung 007 compiles and runs");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 2);
    assert_eq!(report.plain.checks, report.chaos.checks);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
    assert_eq!(report.chaos.counters.pure_host_calls, 0);
    assert_eq!(report.plain.receipt_count, 0);
    assert_eq!(report.chaos.receipt_count, 0);

    let area = module
        .functions
        .iter()
        .find(|function| function.name == "area")
        .expect("rung 007 contains area");
    let arms = area
        .nodes
        .iter()
        .find_map(|node| match &node.op {
            VirOp::Match { arms } => Some(arms),
            _ => None,
        })
        .expect("area contains a structured Match");
    let expected_variants = partitioned
        .islands
        .iter()
        .map(|island| {
            island
                .nodes
                .iter()
                .find_map(|node| match &node.op {
                    VirOp::Variant { variant } => Some(*variant),
                    _ => None,
                })
                .expect("each rung 007 island constructs one variant")
        })
        .collect::<Vec<_>>();
    let mut selected_arm_marks = vec![0usize; partitioned.islands.len()];
    for event in &report.plain.events {
        let EventKind::WeavyMark {
            task,
            function,
            node,
        } = &event.kind
        else {
            continue;
        };
        if *function != area.id {
            continue;
        }
        let Some(arm_index) = arms.iter().position(|arm| arm.nodes.contains(node)) else {
            continue;
        };
        let island_index = report
            .plain
            .events
            .iter()
            .find_map(|event| match &event.kind {
                EventKind::IslandEntered {
                    task: entered,
                    island,
                } if entered == task => Some(island.0 as usize),
                _ => None,
            })
            .expect("every marked task entered an island");
        assert_eq!(arms[arm_index].variant, expected_variants[island_index]);
        selected_arm_marks[island_index] += 1;
    }
    assert!(selected_arm_marks.into_iter().all(|marks| marks > 0));
}

#[test]
fn rung_008_record_spread_builds_a_fresh_value_through_vir_and_weavy() {
    let module = Compiler::new()
        .compile(RUNG_008)
        .expect("rung 008 compiles");
    let spread = module
        .functions
        .iter()
        .find(|function| function.name == "spread")
        .expect("rung 008 contains spread");
    let records = spread
        .nodes
        .iter()
        .filter(|node| matches!(node.op, VirOp::Record))
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 2, "base and update are distinct VIR values");
    let [base, moved] = records.as_slice() else {
        unreachable!("record count checked above")
    };
    assert_eq!(moved.inputs.len(), 2);
    let inherited_y = spread
        .nodes
        .iter()
        .find(|node| node.id == moved.inputs[1])
        .expect("the moved record's inherited y input exists");
    assert!(matches!(inherited_y.op, VirOp::Project { index: 1 }));
    assert_eq!(inherited_y.inputs, [base.id]);

    let partitioned = module.partition_test(&module.tests[0]);
    assert_eq!(partitioned.islands.len(), 3);
    let mut lowering_cache = LoweringCache::default();
    for island in &partitioned.islands {
        let lowered = lowering_cache
            .get_or_lower(island)
            .expect("rung 008 lowers to Weavy");
        assert!(lowered.program.fns.iter().all(|function| {
            function
                .code
                .iter()
                .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
        }));
    }

    let report = run_source(RUNG_008).expect("rung 008 compiles and runs");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 3);
    assert_eq!(report.plain.checks, report.chaos.checks);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
    assert_eq!(report.chaos.counters.pure_host_calls, 0);
    assert_eq!(report.plain.receipt_count, 0);
    assert_eq!(report.chaos.receipt_count, 0);
}

#[test]
fn rung_009_ambient_structural_equality_runs_through_vir_and_weavy() {
    let module = Compiler::new()
        .compile(RUNG_009)
        .expect("rung 009 compiles");
    let line = module
        .records
        .iter()
        .find(|record| record.name == "Line")
        .expect("rung 009 declares Line");
    assert_eq!(
        VirType::Record(line.clone()).word_width(),
        Some(4),
        "Line equality recursively covers four Int words"
    );
    let structural_equality = module
        .functions
        .iter()
        .find(|function| function.name == "structural_equality")
        .expect("rung 009 contains structural_equality");
    assert_eq!(
        structural_equality
            .nodes
            .iter()
            .filter(|node| matches!(node.op, VirOp::Eq))
            .count(),
        4,
        "three source equalities plus boolean-not canonicalized as equality with false"
    );

    let partitioned = module.partition_test(&module.tests[0]);
    assert_eq!(partitioned.islands.len(), 3);
    let mut lowering_cache = LoweringCache::default();
    for island in &partitioned.islands {
        let lowered = lowering_cache
            .get_or_lower(island)
            .expect("rung 009 lowers to Weavy");
        assert!(lowered.program.fns.iter().any(|function| {
            function
                .code
                .iter()
                .filter(|op| matches!(op, WeavyOp::EqI64 { .. }))
                .count()
                >= 4
                && function
                    .code
                    .iter()
                    .filter(|op| matches!(op, WeavyOp::MulI64 { .. }))
                    .count()
                    >= 3
        }));
        assert!(lowered.program.fns.iter().all(|function| {
            function
                .code
                .iter()
                .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
        }));
    }

    let report = run_source(RUNG_009).expect("rung 009 compiles and runs");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 3);
    assert_eq!(report.plain.checks, report.chaos.checks);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
    assert_eq!(report.chaos.counters.pure_host_calls, 0);
    assert_eq!(report.plain.receipt_count, 0);
    assert_eq!(report.chaos.receipt_count, 0);
}

// r[related lang.value.structural-order]
// r[related machine.value.structural-order]
#[test]
fn rung_010_spaceship_returns_ambient_ordering_through_vir_and_weavy() {
    let module = Compiler::new()
        .compile(RUNG_010)
        .expect("rung 010 compiles");
    assert_eq!(VirType::ordering().word_width(), Some(1));
    let spaceship = module
        .functions
        .iter()
        .find(|function| function.name == "spaceship")
        .expect("rung 010 contains spaceship");
    assert_eq!(
        spaceship
            .nodes
            .iter()
            .filter(|node| matches!(node.op, VirOp::Compare))
            .count(),
        3
    );
    assert!(
        spaceship
            .nodes
            .iter()
            .filter(|node| matches!(node.op, VirOp::Compare))
            .all(|node| node.ty == VirType::ordering())
    );
    let mut ordering_variants = spaceship
        .nodes
        .iter()
        .filter_map(|node| match (&node.ty, &node.op) {
            (ty, VirOp::Variant { variant }) if *ty == VirType::ordering() => Some(*variant),
            _ => None,
        })
        .collect::<Vec<_>>();
    ordering_variants.sort_unstable();
    assert_eq!(ordering_variants, [0, 1, 2]);

    let partitioned = module.partition_test(&module.tests[0]);
    assert_eq!(partitioned.islands.len(), 3);
    let mut lowering_cache = LoweringCache::default();
    let mut saw_integer_order = false;
    let mut saw_value_bytes_order = false;
    for island in &partitioned.islands {
        let lowered = lowering_cache
            .get_or_lower(island)
            .expect("rung 010 lowers to Weavy");
        saw_integer_order |= lowered.program.fns.iter().any(|function| {
            function
                .code
                .iter()
                .any(|op| matches!(op, WeavyOp::LtI64 { .. }))
                && function
                    .code
                    .iter()
                    .any(|op| matches!(op, WeavyOp::GtI64 { .. }))
        });
        saw_value_bytes_order |= lowered.program.fns.iter().any(|function| {
            function
                .code
                .iter()
                .any(|op| matches!(op, WeavyOp::CompareValueBytes { .. }))
        });
        assert!(lowered.program.fns.iter().all(|function| {
            function
                .code
                .iter()
                .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
        }));
    }
    assert!(saw_integer_order);
    assert!(saw_value_bytes_order);

    let report = run_source(RUNG_010).expect("rung 010 compiles and runs");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 3);
    assert_eq!(report.plain.checks, report.chaos.checks);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
    assert_eq!(report.chaos.counters.pure_host_calls, 0);
    assert_eq!(report.plain.receipt_count, 0);
    assert_eq!(report.chaos.receipt_count, 0);
}

#[test]
fn rung_011_relations_derive_from_spaceship_through_vir_and_weavy() {
    let module = Compiler::new()
        .compile(RUNG_011)
        .expect("rung 011 compiles");
    let derived = module
        .functions
        .iter()
        .find(|function| function.name == "derived_comparisons")
        .expect("rung 011 contains derived_comparisons");
    assert_eq!(
        derived
            .nodes
            .iter()
            .filter(|node| matches!(node.op, VirOp::Compare))
            .count(),
        3
    );
    assert_eq!(
        derived
            .nodes
            .iter()
            .filter(|node| matches!(node.op, VirOp::Eq))
            .count(),
        2
    );
    assert_eq!(
        derived
            .nodes
            .iter()
            .filter(|node| matches!(node.op, VirOp::Ne))
            .count(),
        1
    );

    let partitioned = module.partition_test(&module.tests[0]);
    assert_eq!(partitioned.islands.len(), 3);
    let mut lowering_cache = LoweringCache::default();
    let mut saw_mixed_tuple_order = false;
    for island in &partitioned.islands {
        let lowered = lowering_cache
            .get_or_lower(island)
            .expect("rung 011 lowers to Weavy");
        for function in &lowered.program.fns {
            let has_integer_order = function
                .code
                .iter()
                .any(|op| matches!(op, WeavyOp::LtI64 { .. }))
                && function
                    .code
                    .iter()
                    .any(|op| matches!(op, WeavyOp::GtI64 { .. }));
            let has_value_bytes_order = function
                .code
                .iter()
                .any(|op| matches!(op, WeavyOp::CompareValueBytes { .. }));
            saw_mixed_tuple_order |= has_integer_order && has_value_bytes_order;
            assert!(
                function
                    .code
                    .iter()
                    .all(|op| !matches!(op, WeavyOp::LeI64 { .. } | WeavyOp::GeI64 { .. }))
            );
            assert!(
                function.code.iter().all(|op| !matches!(
                    op,
                    WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }
                ))
            );
        }
    }
    assert!(saw_mixed_tuple_order);

    let report = run_source(RUNG_011).expect("rung 011 compiles and runs");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 3);
    assert_eq!(report.plain.checks, report.chaos.checks);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
    assert_eq!(report.chaos.counters.pure_host_calls, 0);
    assert_eq!(report.plain.receipt_count, 0);
    assert_eq!(report.chaos.receipt_count, 0);
}

// r[related lang.value.structural-order]
// r[related machine.value.structural-order]
#[test]
fn rung_012_record_order_is_total_structural_and_declaration_ordered() {
    let module = Compiler::new()
        .compile(RUNG_012)
        .expect("rung 012 compiles");
    let version = module
        .records
        .iter()
        .find(|record| record.name == "V")
        .expect("rung 012 declares V");
    assert_eq!(VirType::Record(version.clone()).word_width(), Some(2));
    let total_order = module
        .functions
        .iter()
        .find(|function| function.name == "total_order")
        .expect("rung 012 contains total_order");
    assert_eq!(
        total_order
            .nodes
            .iter()
            .filter(|node| matches!(node.op, VirOp::Compare))
            .count(),
        2
    );

    let partitioned = module.partition_test(&module.tests[0]);
    assert_eq!(partitioned.islands.len(), 2);
    let mut lowering_cache = LoweringCache::default();
    for island in &partitioned.islands {
        let lowered = lowering_cache
            .get_or_lower(island)
            .expect("rung 012 lowers to Weavy");
        let entry = &lowered.program.fns[0];
        let compared_fields = entry
            .code
            .iter()
            .filter_map(|op| match op {
                WeavyOp::LtI64 { a, b, .. } => Some((*a, *b)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(compared_fields.len(), 2);
        assert_eq!(compared_fields[1].0, compared_fields[0].0 + 8);
        assert_eq!(compared_fields[1].1, compared_fields[0].1 + 8);
        assert!(
            entry
                .code
                .iter()
                .any(|op| matches!(op, WeavyOp::Jump { .. }))
        );
        assert!(
            entry
                .code
                .iter()
                .all(|op| !matches!(op, WeavyOp::CompareValueBytes { .. }))
        );
        assert!(
            entry
                .code
                .iter()
                .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
        );
    }

    let report = run_source(RUNG_012).expect("rung 012 compiles and runs");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 2);
    assert_eq!(report.plain.checks, report.chaos.checks);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
    assert_eq!(report.chaos.counters.pure_host_calls, 0);
    assert_eq!(report.plain.receipt_count, 0);
    assert_eq!(report.chaos.receipt_count, 0);
}

#[test]
fn rung_013_expression_statement_is_rejected_with_declared_message_and_line() {
    let (expected_message, expected_line) = reject_header(RUNG_013);
    let source = SurfaceParser::new()
        .parse(RUNG_013)
        .expect("rung 013 diagnostic production parses");
    let function = source
        .items
        .iter()
        .find_map(|item| match item {
            ast::Item::Fn(function) if function.name.value == "f" => Some(function),
            _ => None,
        })
        .expect("rung 013 contains f");
    assert!(matches!(
        function.body.stmts.as_slice(),
        [ast::Stmt::Expression(_)]
    ));

    let diagnostics = Compiler::new()
        .compile(RUNG_013)
        .expect_err("rung 013 must be rejected");
    assert_eq!(diagnostics.entries.len(), 1);
    let diagnostic = &diagnostics.entries[0];
    assert_eq!(diagnostic.code, DiagnosticCode::ExpressionStatement);
    assert_eq!(diagnostic.message(), expected_message);
    assert_eq!(
        source_line(RUNG_013, diagnostic.primary.start),
        expected_line
    );
}

#[test]
fn rung_014_if_else_is_an_expression_through_vir_and_weavy() {
    let module = Compiler::new()
        .compile(RUNG_014)
        .expect("rung 014 compiles");
    let sign = module
        .functions
        .iter()
        .find(|function| function.name == "sign")
        .expect("rung 014 contains sign");
    let conditionals = sign
        .nodes
        .iter()
        .filter(|node| matches!(node.op, VirOp::If { .. }))
        .collect::<Vec<_>>();
    assert_eq!(conditionals.len(), 2);
    assert!(conditionals.iter().all(|node| {
        let VirOp::If {
            consequent,
            alternative,
        } = &node.op
        else {
            unreachable!("conditionals were selected above")
        };
        node.ty == VirType::Int
            && node.inputs.len() == 1
            && consequent.nodes.contains(&consequent.output)
            && alternative.nodes.contains(&alternative.output)
    }));
    assert!(sign.nodes.iter().all(|node| !matches!(node.op, VirOp::Sub)));
    assert!(
        sign.nodes
            .iter()
            .any(|node| matches!(node.op, VirOp::Int(-1)))
    );

    let partitioned = module.partition_test(&module.tests[0]);
    assert_eq!(partitioned.islands.len(), 3);
    let mut lowering_cache = LoweringCache::default();
    for island in &partitioned.islands {
        let source_map = source_map_for(island);
        let conditional_trace_ids = conditionals
            .iter()
            .map(|conditional| {
                source_map
                    .iter()
                    .find(|entry| entry.function == sign.id && entry.node == conditional.id)
                    .expect("conditional has source attribution")
                    .trace_id
            })
            .collect::<Vec<_>>();
        let lowered = lowering_cache
            .get_or_lower(island)
            .expect("rung 014 lowers to Weavy");
        for trace_id in conditional_trace_ids {
            assert!(lowered.program.fns.iter().any(|function| {
                function.code.windows(2).any(|ops| {
                    matches!(ops[0], WeavyOp::Trace { id } if id == trace_id)
                        && matches!(ops[1], WeavyOp::JumpIfZero { .. })
                })
            }));
        }
        assert!(lowered.program.fns.iter().all(|function| {
            function
                .code
                .iter()
                .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
        }));
    }

    let report = run_source(RUNG_014).expect("rung 014 compiles and runs");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 3);
    assert_eq!(report.plain.checks, report.chaos.checks);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
    assert_eq!(report.chaos.counters.pure_host_calls, 0);
    assert_eq!(report.plain.receipt_count, 0);
    assert_eq!(report.chaos.receipt_count, 0);
}

#[test]
fn rung_015_boolean_operators_reuse_structured_conditionals() {
    let module = Compiler::new()
        .compile(RUNG_015)
        .expect("rung 015 compiles");
    let function = &module.functions[module.tests[0].function.0 as usize];
    let conditionals = function
        .nodes
        .iter()
        .filter(|node| matches!(node.op, VirOp::If { .. }))
        .collect::<Vec<_>>();
    assert_eq!(conditionals.len(), 3);
    assert!(
        conditionals
            .iter()
            .all(|node| node.ty == VirType::Bool && node.inputs.len() == 1)
    );
    assert!(
        function
            .nodes
            .iter()
            .filter(|node| matches!(node.op, VirOp::Eq))
            .count()
            >= 2
    );

    let partitioned = module.partition_test(&module.tests[0]);
    assert_eq!(partitioned.islands.len(), 4);
    let mut lowered_conditionals = 0;
    let mut lowering_cache = LoweringCache::default();
    for island in &partitioned.islands {
        let source_map = source_map_for(island);
        let lowered = lowering_cache
            .get_or_lower(island)
            .expect("rung 015 lowers to Weavy");
        for conditional in island
            .nodes
            .iter()
            .filter(|node| matches!(node.op, VirOp::If { .. }))
        {
            lowered_conditionals += 1;
            let trace_id = source_map
                .iter()
                .find(|entry| entry.function == island.function && entry.node == conditional.id)
                .expect("short-circuit conditional has source attribution")
                .trace_id;
            assert!(lowered.program.fns[0].code.windows(2).any(|ops| {
                matches!(ops[0], WeavyOp::Trace { id } if id == trace_id)
                    && matches!(ops[1], WeavyOp::JumpIfZero { .. })
            }));
        }
        assert!(lowered.program.fns.iter().all(|function| {
            function
                .code
                .iter()
                .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
        }));
    }
    assert_eq!(lowered_conditionals, 3);

    let report = run_source(RUNG_015).expect("rung 015 compiles and runs");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 4);
    assert_eq!(report.plain.checks, report.chaos.checks);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
    assert_eq!(report.chaos.counters.pure_host_calls, 0);
    assert_eq!(report.plain.receipt_count, 0);
    assert_eq!(report.chaos.receipt_count, 0);
}

#[test]
fn rung_016_match_is_a_value_selecting_one_control_region() {
    let module = Compiler::new()
        .compile(RUNG_016)
        .expect("rung 016 compiles");
    let go = module
        .functions
        .iter()
        .find(|function| function.name == "go")
        .expect("rung 016 contains go");
    let arms = go
        .nodes
        .iter()
        .find_map(|node| match &node.op {
            VirOp::Match { arms } if node.ty == VirType::Bool => Some(arms),
            _ => None,
        })
        .expect("go contains a value-producing Match");
    assert_eq!(arms.len(), 3);
    assert!(arms.iter().all(|arm| arm.nodes.contains(&arm.output)));

    let partitioned = module.partition_test(&module.tests[0]);
    assert_eq!(partitioned.islands.len(), 2);
    let mut lowering_cache = LoweringCache::default();
    for island in &partitioned.islands {
        let lowered = lowering_cache
            .get_or_lower(island)
            .expect("rung 016 lowers to Weavy");
        assert!(lowered.program.fns.iter().any(|function| {
            function
                .code
                .iter()
                .any(|op| matches!(op, WeavyOp::JumpIfZero { .. }))
        }));
        assert!(lowered.program.fns.iter().any(|function| {
            function
                .code
                .iter()
                .any(|op| matches!(op, WeavyOp::Jump { .. }))
        }));
        assert!(lowered.program.fns.iter().all(|function| {
            function
                .code
                .iter()
                .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
        }));
    }

    let report = run_source(RUNG_016).expect("rung 016 compiles and runs");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 2);
    assert_eq!(report.plain.checks, report.chaos.checks);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
    assert_eq!(report.chaos.counters.pure_host_calls, 0);
    assert_eq!(report.plain.receipt_count, 0);
    assert_eq!(report.chaos.receipt_count, 0);
}

#[test]
fn rung_017_match_guards_select_the_first_matching_arm() {
    let report = run_source(RUNG_017).expect("rung 017 compiles and runs");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 4);
    assert_eq!(report.plain.checks, report.chaos.checks);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
    assert_eq!(report.chaos.counters.pure_host_calls, 0);
    assert_eq!(report.plain.receipt_count, 0);
    assert_eq!(report.chaos.receipt_count, 0);
}

fn reject_header(source: &str) -> (&str, usize) {
    let mut message = None;
    let mut line = None;
    for header in source.lines().take_while(|line| line.starts_with("//!")) {
        if let Some(value) = header.strip_prefix("//! reject: ") {
            message = Some(value);
        }
        if let Some(value) = header.strip_prefix("//! at: ") {
            line = Some(value.parse::<usize>().expect("reject line is an integer"));
        }
    }
    (
        message.expect("reject rung declares a message"),
        line.expect("reject rung declares a line"),
    )
}

fn source_line(source: &str, byte: u32) -> usize {
    let byte = usize::try_from(byte).expect("source byte offset fits usize");
    source.as_bytes()[..byte]
        .iter()
        .filter(|&&byte| byte == b'\n')
        .count()
        + 1
}

fn assert_contiguous_sequences(events: &[vix::runtime::Event]) {
    assert!(events.iter().enumerate().all(|(index, event)| {
        event.sequence == u64::try_from(index).expect("event count fits u64")
    }));
}
