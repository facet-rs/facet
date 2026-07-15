use std::collections::BTreeSet;
use std::path::Path;

use vix::budget::{BudgetOutcome, ChildReport, run_source_under_declared_budget};
use vix::compiler::Compiler;
use vix::diagnostic::{DiagnosticCode, DiagnosticPayload, DiagnosticSeverity};
use vix::lowering::{LoweringCache, attribution_for, source_map_for};
use vix::modules::ModuleSource;
use vix::ratchet::{
    RunError, SnapshotExpectations, run_source, run_source_innards, run_source_rerun_audit,
    run_source_rerun_audit_with_lane, run_source_revision_audit,
    run_source_revision_audit_with_lane, run_source_with_modules, run_source_with_snapshots,
    run_source_with_snapshots_and_lane,
};
use vix::runtime::{
    DemandState, EventKind, FailureValue, MemoVerdict, ProcessTermination, SnapshotOutcome,
    TaskState,
};
use vix::surface::{SurfaceParser, ast};
use vix::vir::{
    ArrayMapExecutionShape, ArrayMapGrainKey, EffectKind, FunctionId, GeneratorControl,
    GeneratorStep, NodeRef, OPTION_NONE_VARIANT, OPTION_SOME_VARIANT, Op as VirOp, Type as VirType,
    VariantPayload, canonical_recipe,
};
use weavy::task::Op as WeavyOp;
use weavy::{LaneRequest, PayloadKind, RegionShape, ValueShapeKind, WordKind};

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
const RUNG_018: &str = include_str!("ratchet/018-non-exhaustive.reject.vix");
const RUNG_019: &str = include_str!("ratchet/019-let-destructuring.vix");
const RUNG_020: &str = include_str!("ratchet/020-match-destructuring.vix");
const RUNG_021: &str = include_str!("ratchet/021-closure-destructuring.vix");
const RUNG_022: &str = include_str!("ratchet/022-record-patterns.vix");
const RUNG_023: &str = include_str!("ratchet/023-option.vix");
const RUNG_024: &str = include_str!("ratchet/024-user-result.vix");
const RUNG_025: &str = include_str!("ratchet/025-ordering-enum.vix");
const RUNG_026: &str = include_str!("ratchet/026-arrays.vix");
const RUNG_027: &str = include_str!("ratchet/027-array-map.vix");
const RUNG_028: &str = include_str!("ratchet/028-array-enumerate.vix");
const RUNG_029: &str = include_str!("ratchet/029-array-fold.vix");
const RUNG_030: &str = include_str!("ratchet/030-array-predicates.vix");
const RUNG_031: &str = include_str!("ratchet/031-split-last.vix");
const RUNG_032: &str = include_str!("ratchet/032-pop.reject.vix");
const RUNG_033: &str = include_str!("ratchet/033-multiset-conversion.vix");
const RUNG_034: &str = include_str!("ratchet/034-multiset-filter.vix");
const RUNG_035: &str = include_str!("ratchet/035-canonical-order.vix");
const RUNG_036: &str = include_str!("ratchet/036-multiset-fold.vix");
const RUNG_037: &str = include_str!("ratchet/037-filter-map-flat-map.vix");
const RUNG_038: &str = include_str!("ratchet/038-find-split-min-max.vix");
const RUNG_039: &str = include_str!("ratchet/039-indexed-roundtrip.vix");
const RUNG_040: &str = include_str!("ratchet/040-sorted-by.vix");
const RUNG_041: &str = include_str!("ratchet/041-maps.vix");
const RUNG_042: &str = include_str!("ratchet/042-map-overwrite.vix");
const RUNG_043: &str = include_str!("ratchet/043-map-keys-canonical.vix");
const RUNG_044: &str = include_str!("ratchet/044-sets.vix");
const RUNG_045: &str = include_str!("ratchet/045-strings.vix");
const RUNG_046: &str = include_str!("ratchet/046-paths.vix");
const RUNG_047: &str = include_str!("ratchet/047-string-to-path.reject.vix");
const RUNG_048: &str = include_str!("ratchet/048-closures-capture.vix");
const RUNG_049: &str = include_str!("ratchet/049-recursion.vix");
const RUNG_050: &str = include_str!("ratchet/050-deep-tail-recursion.vix");
const RUNG_052: &str = include_str!("ratchet/052-higher-order.vix");
const RUNG_060: &str = include_str!("ratchet/060-snapshot-record.vix");
const RUNG_061: &str = include_str!("ratchet/061-snapshot-canonical.vix");
const RUNG_062: &str = include_str!("ratchet/062-json-decode.vix");
const RUNG_063: &str = include_str!("ratchet/063-toml-decode.vix");
const RUNG_064: &str = include_str!("ratchet/064-decode-optional.vix");
const RUNG_065: &str = include_str!("ratchet/065-decode-enum-forms.vix");
const RUNG_066: &str = include_str!("ratchet/066-decode-failure.vix");
const RUNG_067: &str = include_str!("ratchet/067-exec-echo.vix");
const RUNG_068: &str = include_str!("ratchet/068-exec-failure-is-result.vix");
const RUNG_069: &str = include_str!("ratchet/069-exec-memoized.vix");
const RUNG_070: &str = include_str!("ratchet/070-undeclared-capability.reject.vix");
const RUNG_071: &str = include_str!("ratchet/071-tree-projection.vix");
const RUNG_072: &str = include_str!("ratchet/072-glob.vix");
const RUNG_075: &str = include_str!("ratchet/075-fetch-pinned.vix");
const RUNG_076: &str = include_str!("ratchet/076-fetch-memoized.vix");
const RUNG_077: &str = include_str!("ratchet/077-archive-extract.vix");
const RUNG_078: &str = include_str!("ratchet/078-receipts-record-reads.vix");
const RUNG_079: &str = include_str!("ratchet/079-cross-run-reuse.vix");
const RUNG_080: &str = include_str!("ratchet/080-early-cutoff.vix");
const RUNG_081: &str = include_str!("ratchet/081-projection-reuse.vix");
const RUNG_082: &str = include_str!("ratchet/082-flaky-detected.vix");
const RUNG_101: &str = include_str!("ratchet/101-body-edit-early-cutoff.vix");
const RUNG_101_V2: &str = include_str!("ratchet/101-body-edit-early-cutoff.v2.vix");
const RUNG_102: &str = include_str!("ratchet/102-body-edit-negative-control.vix");
const RUNG_102_V2: &str = include_str!("ratchet/102-body-edit-negative-control.v2.vix");
const RUNG_103: &str = include_str!("ratchet/103-rename-is-cold.vix");
const RUNG_103_V2: &str = include_str!("ratchet/103-rename-is-cold.v2.vix");
const RUNG_104: &str = include_str!("ratchet/104-wrapper-refactor-warm.vix");
const RUNG_104_V2: &str = include_str!("ratchet/104-wrapper-refactor-warm.v2.vix");
const RUNG_105: &str = include_str!("ratchet/105-reuse-not-recompute.vix");
const RUNG_106: &str = include_str!("ratchet/106-imports.vix");
const RUNG_107: &str = include_str!("ratchet/107-visibility.reject.vix");
const RUNG_108: &str = include_str!("ratchet/108-import-std.vix");
const RUNG_109: &str = include_str!("ratchet/109-name-collision.reject.vix");
const RUNG_110: &str = include_str!("ratchet/110-module-memo-boundary.vix");
const LIB_GEOMETRY: &str = include_str!("ratchet/lib/geometry.vix");
const RUNG_138: &str = include_str!("ratchet/138-map-accumulator.vix");
const RUNG_144: &str = include_str!("ratchet/144-unused-collection-result.warn.vix");
const RUNG_145: &str = include_str!("ratchet/145-push.reject.vix");
const RUNG_146: &str = include_str!("ratchet/146-insert.reject.vix");

fn frame_index(functions: &[FunctionId], function: FunctionId) -> usize {
    functions
        .iter()
        .position(|candidate| *candidate == function)
        .expect("function has a lowered Weavy frame")
}

fn assert_pc_maps_complete(lowered: &vix::lowering::LoweringArtifact) {
    assert_eq!(lowered.program().fns.len(), lowered.pc_nodes.len());
    for (frame, function) in lowered.program().fns.iter().enumerate() {
        assert_eq!(
            function.code.len(),
            lowered.pc_nodes[frame].len(),
            "frame {frame} must attribute every Weavy pc",
        );
        for pc in 0..function.code.len() {
            assert!(
                lowered.node_for_pc(frame as u32, pc as u32).is_some(),
                "frame {frame} pc {pc} must resolve to a VIR node",
            );
        }
    }
}

fn pcs_for_node(
    lowered: &vix::lowering::LoweringArtifact,
    frame: usize,
    node: NodeRef,
) -> Vec<usize> {
    lowered.pc_nodes[frame]
        .iter()
        .enumerate()
        .filter_map(|(pc, owner)| (*owner == node).then_some(pc))
        .collect()
}

fn with_first_lowered<T>(
    source: &str,
    inspect: impl FnOnce(&vix::lowering::LoweringArtifact) -> T,
) -> T {
    let module = Compiler::new().compile(source).expect("source compiles");
    let partitioned = module.partition_test(&module.tests[0]);
    let mut lowering_cache = LoweringCache::default();
    let lowered = lowering_cache
        .get_or_lower(&partitioned.islands[0])
        .expect("source lowers to Weavy");
    inspect(lowered)
}

fn shape_words(shape: &RegionShape) -> Vec<Vec<WordKind>> {
    shape
        .words
        .iter()
        .map(|kinds| kinds.as_slice().to_vec())
        .collect()
}

fn region_byte_len(region: &weavy::FrameRegion) -> usize {
    region.shape.checked_byte_len().expect("region size fits")
}

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
    assert!(rendered_weavy.contains("ConstI64 { dst: 8, value: 1 }"));
    assert!(rendered_weavy.contains("CopyI64 { dst: 16, src: 8 }"));
    assert!(rendered_weavy.contains("Ret { src: 16, size: 8 }"));

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

    let innards = run_source_innards(RUNG_001).expect("rung 001 diagnostic lane runs");
    assert!(innards.passed());
    assert!(innards.agrees());

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
    assert!(
        report
            .plain
            .events
            .iter()
            .all(|event| !matches!(event.kind, EventKind::WeavyMark { .. })),
        "production execution strips interior trace marks",
    );
    assert!(innards.plain.events.iter().any(|event| matches!(
        event.kind,
        EventKind::WeavyMark { node, .. } if node.0 == 0
    )));
    assert!(innards.plain.events.iter().any(|event| matches!(
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
fn pc_node_attribution_is_cached_but_node_spans_stay_per_compilation() {
    let module = Compiler::new()
        .compile(RUNG_001)
        .expect("rung 001 compiles");
    let partitioned = module.partition_test(&module.tests[0]);
    let island = &partitioned.islands[0];
    let attribution = attribution_for(island);
    let output_node = NodeRef {
        function: island.function,
        node: island.output,
    };
    let output_source = attribution
        .source_for_node(output_node)
        .expect("output node has per-compilation source attribution");

    let mut lowering_cache = LoweringCache::default();
    let lowered = lowering_cache
        .get_or_lower(island)
        .expect("rung 001 lowers to Weavy");
    assert_pc_maps_complete(lowered);
    let root_frame = frame_index(&attribution.functions, island.function);
    let last_pc = lowered.program().fns[root_frame].code.len() - 1;
    assert_eq!(
        lowered.node_for_pc(root_frame as u32, last_pc as u32),
        Some(output_node),
        "synthetic return belongs to the function output node",
    );
    let recipe = lowered.recipe;
    let rendered = lowered.render();
    let pc_nodes = lowered.pc_nodes.clone();

    let shifted_source = format!("\n\n{RUNG_001}");
    let shifted_module = Compiler::new()
        .compile(&shifted_source)
        .expect("span-only edit compiles");
    let shifted_partitioned = shifted_module.partition_test(&shifted_module.tests[0]);
    let shifted_island = &shifted_partitioned.islands[0];
    let shifted_attribution = attribution_for(shifted_island);
    let shifted_source = shifted_attribution
        .source_for_node(output_node)
        .expect("shifted output node has source attribution");
    let shifted_lowered = lowering_cache
        .get_or_lower(shifted_island)
        .expect("span-only edit reuses bytecode");
    assert_pc_maps_complete(shifted_lowered);
    assert_eq!(shifted_lowered.recipe, recipe);
    assert_eq!(shifted_lowered.render(), rendered);
    assert_eq!(shifted_lowered.pc_nodes, pc_nodes);
    assert_ne!(output_source.span, shifted_source.span);
    assert_eq!(lowering_cache.counters().misses, 1);
    assert_eq!(lowering_cache.counters().hits, 1);
}

#[test]
fn scalar_contract_verifies_through_real_weavy_program_path() {
    const SOURCE: &str = r#"
#[test]
fn scalar_contract() -> Stream<Check> {
    yield expect_eq((1 + 2) * 3, 9);
}
"#;

    with_first_lowered(SOURCE, |lowered| {
        assert!(std::ptr::eq(
            lowered.program(),
            lowered.executable().program().program()
        ));
        assert!(std::ptr::eq(
            lowered.contract(),
            lowered.executable().program().contract()
        ));
    });
}

#[test]
fn emitted_contract_covers_product_enum_nested_and_zero_word_shapes() {
    const SOURCE: &str = r#"
struct Empty {}
struct Wrap { pair: (Bool, String), empty: Empty }

enum Outcome {
    Flag(Bool),
    Label(String),
}

fn has_empty(empty: Empty) -> Bool {
    true
}

#[test]
fn structural_contracts() -> Stream<Check> {
    let empty = Empty {};
    let nested = ((true, "ok"), false);
    let wrapped = Wrap { pair: (true, "ok"), empty };
    let outcome = Outcome::Label("ok");
    yield expect(has_empty(wrapped.empty)
        && nested.1 == false
        && wrapped.pair.0
        && match outcome {
        Outcome::Flag(value) => value,
        Outcome::Label(label) => label == "ok",
    });
}
"#;

    with_first_lowered(SOURCE, |lowered| {
        let contract = lowered.contract();
        assert!(
            contract
                .functions
                .iter()
                .flat_map(|function| &function.frame.regions)
                .any(|region| region.shape.words.is_empty() && region.value_shape.is_some())
        );
        assert!(contract.schemas.iter().any(|schema| {
            schema.inline.words.is_empty()
                && schema.value_shape.is_some()
                && matches!(schema.payload, PayloadKind::Inline)
        }));

        let nested_product = contract.value_shapes.iter().any(|shape| match &shape.kind {
            ValueShapeKind::Product { fields } => {
                fields.len() == 2
                    && fields[0].shape.words.len() == 2
                    && fields[0].value_shape.is_some()
                    && fields[1].shape.words.len() == 1
                    && fields[1].value_shape.is_none()
            }
            ValueShapeKind::Enum { .. } => false,
        });
        assert!(
            nested_product,
            "nested aggregate fields must carry nested ValueShapeRef"
        );

        let enum_shape = contract
            .value_shapes
            .iter()
            .find_map(|shape| match &shape.kind {
                ValueShapeKind::Enum { selector, variants }
                    if variants.len() == 2
                        && selector.offset == 0
                        && selector.shape == RegionShape::word(WordKind::Scalar) =>
                {
                    Some((shape, variants))
                }
                _ => None,
            })
            .expect("Outcome emits a selector-correlated enum value shape");
        assert_eq!(
            shape_words(&enum_shape.0.shape),
            vec![
                vec![WordKind::Scalar],
                vec![
                    WordKind::Scalar,
                    WordKind::Handle(
                        lowered
                            .contract()
                            .schemas
                            .iter()
                            .position(|schema| matches!(
                                schema.payload,
                                PayloadKind::OpaqueBytes {
                                    byte_comparable: true
                                }
                            ))
                            .map(|index| weavy::SchemaRef(index as u32))
                            .expect("String schema is present")
                    )
                ]
            ]
        );
        assert_eq!(enum_shape.1[0].fields.len(), 1);
        assert_eq!(enum_shape.1[0].fields[0].offset, 8);
        assert_eq!(
            enum_shape.1[0].fields[0].shape,
            RegionShape::word(WordKind::Scalar)
        );
        assert_eq!(enum_shape.1[1].fields.len(), 1);
        assert_eq!(enum_shape.1[1].fields[0].offset, 8);
        assert!(matches!(
            enum_shape.1[1].fields[0].shape.words[0].as_slice(),
            [WordKind::Handle(_)]
        ));
    });
}

#[test]
fn closure_values_carry_exact_signature_call_contract() {
    with_first_lowered(RUNG_021, |lowered| {
        assert!(!lowered.contract().calls.is_empty());
        let callable_function = lowered
            .contract()
            .functions
            .iter()
            .find(|function| function.call_contract.is_some())
            .expect("closure target carries an indirect-call contract");
        let call_contract = callable_function
            .call_contract
            .expect("call contract is present");
        let call = &lowered.contract().calls[call_contract.0 as usize];
        assert_eq!(call.entries.len(), 1);

        let closure_region = lowered
            .contract()
            .functions
            .iter()
            .flat_map(|function| &function.frame.regions)
            .find(|region| {
                matches!(
                    region.shape.words.as_slice(),
                    [callable, scalar]
                        if matches!(callable.as_slice(), [WordKind::Callable(_)])
                            && scalar.as_slice() == [WordKind::Scalar]
                )
            })
            .expect("closure value is Callable(call-contract) plus environment scalar");
        assert!(closure_region.value_shape.is_some());
    });
}

#[test]
fn direct_string_call_contract_entries_match_argcopy_abi() {
    const SOURCE: &str = r#"
fn check_string(value: String) -> Bool {
    value == "hello"
}

#[test]
fn direct_string_call() -> Stream<Check> {
    yield expect(check_string("hello"));
}
"#;

    with_first_lowered(SOURCE, |lowered| {
        assert_eq!(lowered.program().fns.len(), 2);
        let root = &lowered.contract().functions[0];
        let callee = &lowered.contract().functions[1];
        // One reserved word: the scheduler-owned primitive host plan slot.
        let scheduler_header_bytes: u32 = u32::try_from(region_byte_len(&callee.frame.regions[0]))
            .expect("header region fits u32");
        assert_eq!(scheduler_header_bytes, 8);
        assert_eq!(lowered.constants.len(), 2, "one publication per NodeRef");
        let root_function = lowered.constants[0].root.function;
        let string_schema = lowered.constants[0].root.schema;
        let declared_string = &lowered.contract().schemas[string_schema.0 as usize];
        assert_eq!(
            declared_string.inline,
            RegionShape::word(WordKind::Handle(string_schema))
        );
        assert!(matches!(
            declared_string.payload,
            PayloadKind::OpaqueBytes {
                byte_comparable: true
            }
        ));
        for (root_entry, constant) in lowered.constants.iter().enumerate() {
            assert_eq!(constant.root.function, root_function);
            assert_eq!(constant.root.schema, string_schema);
            assert_eq!(constant.owner.schema, string_schema);
            assert_eq!(constant.root.schema, constant.owner.schema);
            if constant.owner.function == root_function {
                assert_eq!(constant.owner.function, constant.node.function);
                assert_eq!(constant.root.entry, root_entry);
                assert_eq!(constant.owner.entry, root_entry);
                assert_eq!(constant.root.slot.byte_offset(), scheduler_header_bytes);
                assert_eq!(constant.owner.slot.byte_offset(), scheduler_header_bytes);
            } else {
                assert_eq!(constant.owner.function, constant.node.function);
                assert_ne!(constant.owner.function, root_function);
                assert_eq!(constant.root.entry, root_entry);
                assert_eq!(constant.owner.entry, 1);
                assert_eq!(
                    constant.root.slot.byte_offset(),
                    scheduler_header_bytes + 24
                );
                assert_eq!(
                    constant.owner.slot.byte_offset(),
                    scheduler_header_bytes + 8
                );
            }
        }
        assert_eq!(
            root.entries.len(),
            2,
            "root declares its local argument string and the foreign callee string constant"
        );
        assert_eq!(
            callee.entries.len(),
            2,
            "callee entries are parameter first, then local string constant"
        );
        let root_call = lowered.program().fns[0]
            .code
            .iter()
            .find_map(|op| match op {
                WeavyOp::Call { args, .. } => Some(args),
                _ => None,
            })
            .expect("root calls check_string directly");
        assert_eq!(root_call.len(), callee.entries.len());
        assert_eq!(root_call[1].dst, scheduler_header_bytes + 8);
        assert_eq!(root_call[1].size, 8);
        for (arg, entry) in root_call.iter().zip(&callee.entries) {
            let region = &callee.frame.regions[entry.0 as usize];
            assert_eq!(arg.dst, region.offset);
            assert_eq!(arg.size as usize, region_byte_len(region));
        }
    });
}

#[test]
fn frame_contract_regions_do_not_overlap_for_word_regions() {
    with_first_lowered(RUNG_004, |lowered| {
        for function in &lowered.contract().functions {
            for (left_index, left) in function.frame.regions.iter().enumerate() {
                let left_start = left.offset as usize;
                let left_end = left_start + region_byte_len(left);
                for (right_index, right) in function.frame.regions.iter().enumerate() {
                    if left_index >= right_index {
                        continue;
                    }
                    let right_start = right.offset as usize;
                    let right_end = right_start + region_byte_len(right);
                    assert!(
                        left_end == left_start
                            || right_end == right_start
                            || left_end <= right_start
                            || right_end <= left_start,
                        "non-empty frame contract regions {left_index} and {right_index} overlap",
                    );
                }
            }
        }
    });
}

#[test]
fn cached_contract_is_stable_across_span_only_edits() {
    let module = Compiler::new()
        .compile(RUNG_003)
        .expect("rung 003 compiles");
    let partitioned = module.partition_test(&module.tests[0]);
    let mut lowering_cache = LoweringCache::default();
    let lowered = lowering_cache
        .get_or_lower(&partitioned.islands[0])
        .expect("rung 003 lowers");
    let recipe = lowered.recipe;
    let contract = lowered.contract().clone();

    let shifted_source = format!("\n\n{RUNG_003}");
    let shifted_module = Compiler::new()
        .compile(&shifted_source)
        .expect("span-only edit compiles");
    let shifted = shifted_module.partition_test(&shifted_module.tests[0]);
    let shifted_lowered = lowering_cache
        .get_or_lower(&shifted.islands[0])
        .expect("span-only edit reuses lowering artifact");

    assert_eq!(shifted_lowered.recipe, recipe);
    assert_eq!(shifted_lowered.contract(), &contract);
    assert_eq!(lowering_cache.counters().misses, 1);
    assert_eq!(lowering_cache.counters().hits, 1);
}

#[test]
fn accepted_rungs_verify_and_execute_through_one_executable() {
    const ACCEPTED: &[&str] = &[
        RUNG_001, RUNG_002, RUNG_003, RUNG_004, RUNG_005, RUNG_006, RUNG_007, RUNG_008, RUNG_009,
        RUNG_010, RUNG_011, RUNG_012, RUNG_014, RUNG_015, RUNG_016, RUNG_017, RUNG_019, RUNG_020,
        RUNG_021, RUNG_022, RUNG_023, RUNG_024, RUNG_025, RUNG_026, RUNG_027,
    ];

    for source in ACCEPTED {
        let module = Compiler::new()
            .compile(source)
            .expect("accepted rung compiles");
        let mut lowering_cache = LoweringCache::default();
        for test in &module.tests {
            let partitioned = module.partition_test(test);
            for island in &partitioned.islands {
                let lowered = lowering_cache
                    .get_or_lower(island)
                    .expect("accepted artifact verifies before execution");
                let verified = lowered.executable().program();
                assert!(std::ptr::eq(verified.program(), lowered.program()));
                assert!(std::ptr::eq(verified.contract(), lowered.contract()));
                assert!(lowered.program().fns.iter().all(|function| {
                    function.code.iter().all(|op| {
                        !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. })
                    })
                }));
            }
        }
        let report = run_source(source).expect("accepted rung executes through Executable");
        assert!(report.passed());
        assert!(report.agrees());
        assert_eq!(report.plain.checks, report.chaos.checks);
    }
}

fn assert_ordered_freeze_published(source: &str) {
    let report = run_source(source).expect("shared ordered aggregate freezes in production");
    assert!(report.passed(), "all checks pass: {report:?}");
    assert!(report.agrees(), "plain and chaos agree: {report:?}");
    for lane in [&report.plain, &report.chaos] {
        assert!(lane.counters.value_island_spawns >= 1);
        assert_eq!(lane.counters.successful_aggregate_freezes, 1);
        assert_eq!(lane.values.len(), 1);
        assert!(lane.values[0].failure.is_none());
    }
}

#[test]
fn rung_028_array_stream_collects_position_keyed_rows() {
    let compilation = Compiler::new()
        .compile(RUNG_028)
        .expect("rung 028 compiles to typed stream VIR");
    let function = compilation
        .functions
        .iter()
        .find(|function| function.name == "array_enumerate")
        .expect("rung 028 test function exists");
    let stream = function
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::ArrayStream))
        .expect("array stream is a distinct VIR recipe");
    assert_eq!(stream.ty, VirType::stream(VirType::Int, VirType::String));
    assert_eq!(stream.effect.kind, EffectKind::Codata);
    let collect = function
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::StreamCollect))
        .expect("collect is the explicit stream materialization boundary");
    assert_eq!(collect.ty, VirType::map(VirType::Int, VirType::String));
    assert!(collect.effect.fallible);

    assert_ordered_freeze_published(RUNG_028);
}

#[test]
fn rung_029_array_fold_runs_in_authored_position_order() {
    let compilation = Compiler::new()
        .compile(RUNG_029)
        .expect("rung 029 compiles to VIR");
    let partitioned = compilation.partition_test(&compilation.tests[0]);
    let mut cache = LoweringCache::default();
    let mut string_concats = 0usize;
    for island in partitioned
        .values
        .iter()
        .map(|value| &value.island)
        .chain(partitioned.wire_islands.iter().map(|value| &value.island))
        .chain(partitioned.islands.iter())
    {
        let lowered = cache
            .get_or_lower(island)
            .expect("rung 029 verifies before production execution");
        for op in lowered
            .program()
            .fns
            .iter()
            .flat_map(|function| &function.code)
        {
            match op {
                WeavyOp::StringConcat { .. } => string_concats += 1,
                WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. } => {
                    panic!("rung 029 must not lower through a host call: {op:?}")
                }
                _ => {}
            }
        }
    }
    assert!(
        string_concats >= 2,
        "the two string folds must each lower a verifier-admitted StringConcat op, saw {string_concats}"
    );

    let report = run_source(RUNG_029).expect("rung 029 executes through verified production path");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 3);
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
    }
}

const PURE_STRING_CONCAT: &str = r#"
#[test]
fn concat() -> Stream<Check> {
    let hello = "hel" + "lo";
    yield expect_eq(hello, "hello");
    yield expect_eq(hello + " world", "hello world");
    yield expect_eq(("a" + "b") + ("c" + "d"), "abcd");
}
"#;

#[test]
fn pure_string_concatenation_runs_through_verified_path() {
    let compilation = Compiler::new()
        .compile(PURE_STRING_CONCAT)
        .expect("string concatenation compiles to VIR");
    let partitioned = compilation.partition_test(&compilation.tests[0]);
    let mut cache = LoweringCache::default();
    // Nested, result-feeding, and equality-fed concatenations lower to
    // verifier-admitted StringConcat ops, and none of them escapes to a host
    // call.
    let mut string_concats = 0usize;
    for island in &partitioned.islands {
        let lowered = cache
            .get_or_lower(island)
            .expect("string concatenation verifies before production execution");
        for op in lowered
            .program()
            .fns
            .iter()
            .flat_map(|function| &function.code)
        {
            match op {
                WeavyOp::StringConcat { .. } => string_concats += 1,
                WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. } => {
                    panic!("string concatenation must not lower through a host call: {op:?}")
                }
                _ => {}
            }
        }
    }
    assert!(
        string_concats >= 3,
        "each concatenating `+` lowers to a StringConcat op, saw {string_concats}"
    );

    let report =
        run_source(PURE_STRING_CONCAT).expect("string concatenation executes through Weavy");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 3);
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
    }
}

#[test]
fn rung_030_array_predicates_run_through_verified_production_path() {
    let report = run_source(RUNG_030).expect("rung 030 executes through verified production path");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 4);
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
    }
}

#[test]
fn rung_033_array_stream_preserves_position_keys() {
    let compilation = Compiler::new()
        .compile(RUNG_033)
        .expect("rung 033 compiles to typed stream VIR");
    let function = compilation
        .functions
        .iter()
        .find(|function| function.name == "multiset_conversion")
        .expect("rung 033 test function exists");
    let stream = function
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::ArrayStream))
        .expect("array stream is a distinct codata recipe");
    assert_eq!(stream.ty, VirType::stream(VirType::Int, VirType::Int));
    assert_eq!(stream.effect.kind, EffectKind::Codata);
    let collect = function
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::StreamCollect))
        .expect("collect materializes the position-keyed map");
    assert_eq!(collect.ty, VirType::map(VirType::Int, VirType::Int));

    assert_ordered_freeze_published(RUNG_033);
}

#[test]
fn rung_034_stream_filter_preserves_survivor_keys() {
    let compilation = Compiler::new()
        .compile(RUNG_034)
        .expect("rung 034 compiles to a typed filter recipe");
    let function = compilation
        .functions
        .iter()
        .find(|function| function.name == "multiset_filter")
        .expect("rung 034 test function exists");
    let filter = function
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::StreamFilter))
        .expect("filter remains a distinct codata recipe until collection");
    assert_eq!(filter.ty, VirType::stream(VirType::Int, VirType::Int));
    assert_eq!(filter.effect.kind, EffectKind::Codata);

    assert_ordered_freeze_published(RUNG_034);
}

// Rung 038 — deterministic selection (`find_min`/`find_max`) and decomposition
// (`split_min`). `find_min`/`find_max` retain the stream and return `Option<V>`,
// invoking the predicate through the verified callable ABI; `split_min` removes
// exactly one selected element and returns `Option<(V, [V])>`. Values
// are compared in structural-semantic order with the stable stream key as the
// tie-breaker, so a duplicate equal value remains in the rest.
#[test]
fn rung_038_selection_and_decomposition_compiles() {
    let module = Compiler::new()
        .compile(RUNG_038)
        .expect("rung 038 compiles selection and decomposition into typed stream VIR");
    assert_eq!(module.tests.len(), 1);
    let test = &module.tests[0];
    assert_eq!(test.name, "find_split_min_max");
    let function = &module.functions[test.function.0 as usize];

    // `find_min`/`find_max` are structural-order selections over the retained
    // stream: each is a distinct op returning `Option<Int>` while the stream is
    // not consumed.
    let find_min = function
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::StreamFindMin))
        .expect("find_min is a distinct selection op");
    let find_max = function
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::StreamFindMax))
        .expect("find_max is a distinct selection op");
    assert_eq!(find_min.ty, VirType::option(VirType::Int));
    assert_eq!(find_max.ty, VirType::option(VirType::Int));

    // `split_min` decomposes the stream into the selected value and the ordered
    // dense remainder, keeping duplicate equal values: `Option<(Int, [Int])>`.
    let split_min = function
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::StreamSplitMin))
        .expect("split_min is a distinct decomposition op");
    assert_eq!(
        split_min.ty,
        VirType::option(VirType::Tuple(vec![
            VirType::Int,
            VirType::array(VirType::Int),
        ])),
        "split_min returns the selected value paired with the ordered dense remainder"
    );

    // The `match` over `split_min` makes the generator branch-dependent: the
    // taken Some arm publishes three checks, the untaken None arm one.
    assert!(test.generator.has_conditional_sites());
}

// The whole rung 038 fixture is a branch-dependent generator (its `match` over
// `split_min` owns the Some/None yield sites). The checked lowering executes the
// taken Some arm and does not publish the untaken None-arm check.
#[test]
fn rung_038_executes_on_verified_path() {
    let report = run_source(RUNG_038).expect("rung 038 executes through verified production path");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 5);
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
    }
}

// `find_min`/`find_max` execute through the verified callable ABI without
// consuming their keyed-codata source. Stream completion is explicit through
// `split_min`'s `[V]` result, whose Array methods own cardinality and membership.
#[test]
fn rung_038_selection_executes_on_verified_path() {
    const SOURCE: &str = r#"
#[test]
fn selection() -> Stream<Check> {
    let ms = [5, 3, 9, 3].stream();
    yield expect_eq(ms.find_min(|n| n > 3), Some(5));
    yield expect_eq(ms.find_min(|_| true), Some(3));
    yield expect_eq(ms.find_max(|_| true), Some(9));
    yield expect_none(ms.find_min(|n| n > 100));
}
"#;
    let report =
        run_source(SOURCE).expect("stream selection executes through verified production path");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 4);
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
    }
}

// `split_min` realizes a dense remainder at completion, so the selected key is
// omitted once while an equal duplicate at another key remains. This is driven
// through the production path independently of the canonical fixture.
#[test]
fn rung_038_split_min_executes_on_verified_path() {
    const SOURCE: &str = r#"
#[test]
fn decompose() -> Stream<Check> {
    let ms = [5, 3, 9, 3].stream();
    yield match ms.split_min() {
        Some((least, rest)) => {
            yield expect_eq(least, 3);
            yield expect_eq(rest, [5, 9, 3]);
        },
        None => expect(false),
    };
    let empty: [Int] = [];
    yield expect_none(empty.stream().split_min());
}
"#;
    let compilation = Compiler::new()
        .compile(SOURCE)
        .expect("split_min compiles to typed decomposition VIR");
    let split_min = compilation.functions[0]
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::StreamSplitMin))
        .expect("split_min lowers to a distinct decomposition op");
    assert_eq!(
        split_min.ty,
        VirType::option(VirType::Tuple(vec![
            VirType::Int,
            VirType::array(VirType::Int),
        ]))
    );

    let report = run_source(SOURCE).expect("split_min executes through verified production path");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 3);
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
    }
}

#[test]
fn array_split_last_is_a_pure_optional_partition() {
    const SOURCE: &str = r#"
#[test]
fn split_last_values() -> Stream<Check> {
    yield expect_eq([1, 2, 3].split_last(), Some((3, [1, 2])));
    let empty: [Int] = [];
    yield expect_eq(empty.split_last(), None);
}
"#;
    let compilation = Compiler::new()
        .compile(SOURCE)
        .expect("split_last compiles to typed VIR independently of generator codata");
    let split_nodes = compilation.functions[0]
        .nodes
        .iter()
        .filter(|node| matches!(node.op, VirOp::ArraySplitLast))
        .collect::<Vec<_>>();
    assert_eq!(split_nodes.len(), 2);
    assert!(split_nodes.iter().all(|node| {
        node.ty
            == VirType::option(VirType::Tuple(vec![
                VirType::Int,
                VirType::array(VirType::Int),
            ]))
    }));

    let report = run_source(SOURCE).expect("split_last executes through verified production path");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 2);
    assert_eq!(report.plain.checks, report.chaos.checks);
}

// Rung 031 is the first faithful dynamic `#[test] -> Stream<Check>` generator:
// its outer `yield match xs.split_last()` decides at runtime whether the taken
// arm publishes three checks (Some) or one (None), followed by one later
// unconditional empty-array check. This certifies the compile/VIR boundary: the
// generator/codata VIR is built with real Match control and static yield sites.
// The runtime fold is a later checkpoint.
#[test]
fn rung_031_split_last_compiles_to_generator_codata() {
    let module = Compiler::new()
        .compile(RUNG_031)
        .expect("rung 031 compiles into generator/codata VIR");
    assert_eq!(module.tests.len(), 1);
    let test = &module.tests[0];
    assert_eq!(test.name, "split_last");
    let function = &module.functions[test.function.0 as usize];
    let generator = &test.generator;

    // The generator lowers to two ordered steps: a real Match dispatch on
    // `xs.split_last()`, then one later unconditional empty-array check.
    assert_eq!(generator.steps.len(), 2);
    let GeneratorStep::Match { scrutinee, arms } = &generator.steps[0] else {
        panic!("first generator step is the split_last match")
    };
    // The scrutinee is the real optional-partition dispatch value, not a folded
    // constant.
    assert_eq!(
        function.nodes[scrutinee.0 as usize].op,
        VirOp::ArraySplitLast
    );
    assert_eq!(arms.len(), 2);

    // Taken arms own their sites: three in Some, one in None. Untaken arms
    // publish nothing, so no phantom passing checks exist.
    let some_arm = arms
        .iter()
        .find(|arm| arm.variant == OPTION_SOME_VARIANT)
        .expect("Some arm exists");
    let none_arm = arms
        .iter()
        .find(|arm| arm.variant == OPTION_NONE_VARIANT)
        .expect("None arm exists");
    assert_eq!(some_arm.body.steps.len(), 3, "three Some-arm yield sites");
    assert_eq!(none_arm.body.steps.len(), 1, "one None-arm yield site");
    assert!(
        !some_arm.bindings.is_empty(),
        "the Some arm owns its (last, rest) payload projections",
    );
    assert!(
        none_arm.bindings.is_empty(),
        "the None arm binds no payload",
    );

    // The second step is the later unconditional empty-array check.
    assert!(matches!(&generator.steps[1], GeneratorStep::Yield(_)));

    // Control ownership over every published site.
    let owned = generator.owned_sites();
    assert_eq!(owned.len(), 5, "five static yield sites total");
    let unconditional = owned.iter().filter(|owned| owned.owner.is_empty()).count();
    assert_eq!(
        unconditional, 1,
        "exactly one later unconditional empty-array check",
    );
    let some_sites = owned
        .iter()
        .filter(|owned| {
            matches!(
                owned.owner.as_slice(),
                [GeneratorControl::MatchArm {
                    variant: OPTION_SOME_VARIANT,
                    ..
                }]
            )
        })
        .count();
    let none_sites = owned
        .iter()
        .filter(|owned| {
            matches!(
                owned.owner.as_slice(),
                [GeneratorControl::MatchArm {
                    variant: OPTION_NONE_VARIANT,
                    ..
                }]
            )
        })
        .count();
    assert_eq!(some_sites, 3);
    assert_eq!(none_sites, 1);
    // Every conditional site is owned by the same real split_last dispatch.
    for owned in &owned {
        if let [
            GeneratorControl::MatchArm {
                scrutinee: owner_scrutinee,
                ..
            },
        ] = owned.owner.as_slice()
        {
            assert_eq!(*owner_scrutinee, *scrutinee);
        }
    }

    // Each site is a parameterized pure check recipe (`Op::Expect` over captured
    // values), never an evaluated boolean or a host call.
    for owned in &owned {
        let check_node = owned
            .site
            .value_check()
            .expect("rung 031 sites are all value checks");
        let check = &function.nodes[check_node.0 as usize];
        assert_eq!(check.op, VirOp::Expect);
        assert_eq!(check.ty, VirType::Check);
    }

    // The generator's sites are branch-dependent: the runtime seam.
    assert!(generator.has_conditional_sites());

    // Stable, span-insensitive recipe identity. A span-shifted copy of the same
    // source produces byte-identical per-site recipe identities.
    let shifted_source = format!("// generator-codata span shift\n\n{RUNG_031}");
    let shifted = Compiler::new()
        .compile(&shifted_source)
        .expect("span-shifted rung 031 compiles");
    let shifted_function = &shifted.functions[shifted.tests[0].function.0 as usize];
    let shifted_owned = shifted.tests[0].generator.owned_sites();
    assert_eq!(shifted_owned.len(), owned.len());
    for (base, shifted) in owned.iter().zip(&shifted_owned) {
        assert_eq!(
            canonical_recipe(function, base.site.value_check().expect("value-check site")),
            canonical_recipe(
                shifted_function,
                shifted.site.value_check().expect("value-check site")
            ),
            "recipe identity is span-insensitive",
        );
    }

    // The three Some-arm recipes are distinct parameterized checks (different
    // captured values), proving the recipe is not a collapsed boolean.
    let some_recipes = owned
        .iter()
        .filter(|owned| {
            matches!(
                owned.owner.as_slice(),
                [GeneratorControl::MatchArm {
                    variant: OPTION_SOME_VARIANT,
                    ..
                }]
            )
        })
        .map(|owned| {
            canonical_recipe(
                function,
                owned.site.value_check().expect("value-check site"),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(some_recipes.len(), 3);
    assert_ne!(some_recipes[0], some_recipes[1]);
    assert_ne!(some_recipes[1], some_recipes[2]);
    assert_ne!(some_recipes[0], some_recipes[2]);
}

// A branch-dependent generator now executes through the provenance-keyed runtime:
// one verified Weavy generator task follows the real `split_last` match and
// publishes only the taken sites, then those descriptors become ordinary pure
// check demands. The taken `Some` arm publishes sites 0,1,2; the later
// unconditional site 4 always publishes; the untaken `None` arm (site 3,
// `expect(false)`) publishes nothing, so there is no phantom failing check.
// The generator task is one verified Weavy program that runs only the real
// `split_last` match control and publishes the taken sites. It never lowers a
// site's `Op::Expect` check operands, and it uses no host/legacy path.
#[test]
fn rung_031_generator_task_is_verified_control_and_publish_only() {
    let module = Compiler::new()
        .compile(RUNG_031)
        .expect("rung 031 compiles into generator/codata VIR");
    let island = module
        .generator_task_island(&module.tests[0])
        .expect("rung 031 generator task builds without a control-flow scrutinee boundary");

    // VIR certificate: the generator island carries real control (Match) over the
    // real scrutinee (ArraySplitLast) and publishes sites, but lowers no check
    // operand (no Expect/Eq/Ne/IsVariant).
    assert!(
        island
            .nodes
            .iter()
            .any(|node| matches!(node.op, VirOp::Match { .. })),
        "generator runs real match control",
    );
    assert!(
        island
            .nodes
            .iter()
            .any(|node| matches!(node.op, VirOp::ArraySplitLast)),
        "generator computes the real split_last scrutinee",
    );
    assert_eq!(
        island
            .nodes
            .iter()
            .filter(|node| matches!(node.op, VirOp::PublishSite(_)))
            .count(),
        5,
        "one PublishSite per static yield site (0,1,2 Some; 3 None; 4 unconditional)",
    );
    assert!(
        island.nodes.iter().all(|node| !matches!(
            node.op,
            VirOp::Expect | VirOp::Eq | VirOp::Ne | VirOp::IsVariant { .. }
        )),
        "generator lowers no check operand",
    );

    // The island lowers to a verified Executable whose program publishes and runs
    // real variant control, with no host call.
    let mut cache = LoweringCache::default();
    let artifact = cache
        .get_or_lower(&island)
        .expect("generator task lowers to a verified executable");
    let ops = artifact
        .program()
        .fns
        .iter()
        .flat_map(|function| function.code.iter())
        .collect::<Vec<_>>();
    assert!(
        ops.iter().any(|op| matches!(op, WeavyOp::Publish { .. })),
        "generator program publishes taken sites",
    );
    assert!(
        ops.iter()
            .any(|op| matches!(op, WeavyOp::EnumIsVariant { .. })),
        "generator program runs real variant control",
    );
    assert!(
        !ops.iter()
            .any(|op| matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. })),
        "generator program uses no host/legacy path",
    );
}

#[test]
fn rung_031_conditional_generator_executes_taken_sites() {
    let report = run_source(RUNG_031)
        .expect("rung 031 conditional generator executes through the provenance-keyed runtime");
    assert!(
        report.agrees(),
        "plain and chaos agree on the check family: {report:?}"
    );
    assert!(report.passed(), "every taken check passes: {report:?}");
    assert_eq!(
        report.plain.checks.len(),
        4,
        "taken Some sites 0,1,2 plus unconditional site 4, no None-arm phantom",
    );
    assert!(report.plain.checks.iter().all(|check| check.passed));
    assert_eq!(report.plain.checks, report.chaos.checks);
}

// Supplemental production-path certificate for the other taken arm: an empty
// scrutinee takes the None arm, so only the None-arm site and the later
// unconditional site publish. The Some arm publishes nothing, so its
// payload-projecting checks are never demanded — no phantom, no invalid
// projection of an absent Some payload.
#[test]
fn rung_031_taken_none_generator_publishes_only_the_none_arm() {
    const SOURCE: &str = r#"
#[test]
fn taken_none() -> Stream<Check> {
    let xs: [Int] = [];
    yield match xs.split_last() {
        Some((last, rest)) => {
            yield expect_eq(last, 0);
            yield expect_eq(rest, xs);
        },
        None => expect(true),
    };
    yield expect_eq(xs.len(), 0);
}
"#;
    let report = run_source(SOURCE).expect("taken-None generator executes through the runtime");
    assert!(report.agrees(), "plain and chaos agree: {report:?}");
    assert!(
        report.passed(),
        "the None arm and the unconditional site pass: {report:?}"
    );
    assert_eq!(
        report.plain.checks.len(),
        2,
        "None-arm site plus the later unconditional site; the Some arm publishes nothing",
    );
    assert_eq!(report.plain.checks, report.chaos.checks);
}

// Direct construction without publication retains the typed control boundary.
// Production partitioning publishes the shared array selected by that control
// and feeds its ordinary value identity into generator control and both checks.
#[test]
fn rung_031_generator_control_flow_scrutinee_uses_shared_publication() {
    const SOURCE: &str = r#"
#[test]
fn control_scrutinee() -> Stream<Check> {
    let xs = [1, 2, 3];
    let ys: [Int] = [];
    let flag = true;
    yield match (if flag { xs } else { ys }).split_last() {
        Some((last, rest)) => {
            yield expect_eq(last, 3);
            yield expect_eq(rest, [1, 2]);
        },
        None => expect(false),
    };
}
"#;
    let module = Compiler::new()
        .compile(SOURCE)
        .expect("a control-flow scrutinee still compiles to VIR");
    module
        .generator_task_island(&module.tests[0])
        .expect_err("a control-flow scrutinee is a typed boundary, not a panic");
    let report = run_source(SOURCE).expect("production partition publishes the shared selection");
    assert!(report.passed(), "taken checks pass: {report:?}");
    assert!(report.agrees(), "plain and chaos agree: {report:?}");
    for lane in [&report.plain, &report.chaos] {
        assert!(lane.counters.value_island_spawns >= 1);
        assert_eq!(lane.counters.successful_aggregate_freezes, 1);
        assert_eq!(lane.values.len(), 1);
    }
}

// A generator scrutinee may call a pure helper. Its synthetic transitive callees
// are collected exactly as a check island's, so the Call target is never absent.
#[test]
fn rung_031_generator_scrutinee_may_call_a_pure_helper() {
    const SOURCE: &str = r#"
fn classify(n: Int) -> Option<Int> {
    if n > 0 {
        Some(n)
    } else {
        None
    }
}

#[test]
fn helper_scrutinee() -> Stream<Check> {
    yield match classify(5) {
        Some(v) => {
            yield expect_eq(v, 5);
        },
        None => expect(false),
    };
    yield expect(true);
}
"#;
    let module = Compiler::new().compile(SOURCE).expect("source compiles");
    let island = module
        .generator_task_island(&module.tests[0])
        .expect("helper-scrutinee generator builds");
    assert!(
        !island.callees.is_empty(),
        "the pure helper is collected as a synthetic generator callee",
    );
    let report = run_source(SOURCE).expect("helper-scrutinee generator executes");
    assert!(report.agrees(), "plain and chaos agree: {report:?}");
    assert!(
        report.passed(),
        "taken Some check and the unconditional check pass: {report:?}"
    );
    assert_eq!(report.plain.checks.len(), 2);
}

// A language failure raised while computing the generator's scrutinee control
// stays on the typed language plane — it is not reclassified as a machine fault.
#[test]
fn rung_031_generator_scrutinee_language_failure_is_a_language_boundary() {
    const SOURCE: &str = r#"
fn risky(xs: [Int]) -> Option<Int> {
    Some(xs[10])
}

#[test]
fn faulting_scrutinee() -> Stream<Check> {
    let xs = [1];
    yield match risky(xs) {
        Some(v) => {
            yield expect_eq(v, 0);
        },
        None => expect(false),
    };
}
"#;
    match run_source(SOURCE) {
        Err(RunError::GeneratorLanguageFailure {
            test,
            failure,
            context,
        }) => {
            assert_eq!(test, "faulting_scrutinee");
            assert!(
                matches!(failure.as_ref(), FailureValue::IndexOutOfBounds { .. }),
                "the scrutinee's out-of-bounds index is preserved as a typed failure: {failure:?}",
            );
            assert!(
                context.is_some(),
                "the language failure carries its source context",
            );
        }
        other => panic!("expected a typed generator language failure, got {other:?}"),
    }
}

#[test]
fn rung_035_sorted_observes_values_in_canonical_order() {
    let compilation = Compiler::new()
        .compile(RUNG_035)
        .expect("rung 035 compiles to a typed sorting recipe");
    let function = compilation
        .functions
        .iter()
        .find(|function| function.name == "canonical_order")
        .expect("rung 035 test function exists");
    let sorted = function
        .nodes
        .iter()
        .filter(|node| matches!(node.op, VirOp::ArraySorted))
        .collect::<Vec<_>>();
    assert_eq!(
        sorted.len(),
        2,
        "each canonical-order check sorts one projected array"
    );
    assert!(
        sorted
            .iter()
            .any(|node| node.ty == VirType::array(VirType::Int)),
        "integers sort in structural order"
    );
    assert!(
        sorted
            .iter()
            .any(|node| node.ty == VirType::array(VirType::String)),
        "strings sort in structural order"
    );

    let partitioned = compilation.partition_test(&compilation.tests[0]);
    let mut cache = LoweringCache::default();
    for island in &partitioned.islands {
        let lowered = cache
            .get_or_lower(island)
            .expect("rung 035 verifies before production execution");
        assert!(
            lowered
                .program()
                .fns
                .iter()
                .flat_map(|function| &function.code)
                .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. })),
            "sorting never leaves the verified machine"
        );
    }
    let report = run_source(RUNG_035).expect("rung 035 executes through verified production path");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 2);
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert!(lane.checks.iter().all(|check| check.passed));
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
    }
}

#[test]
fn rung_036_multiset_fold_runs_through_verified_execution_without_host_calls() {
    let module = Compiler::new()
        .compile(RUNG_036)
        .expect("rung 036 compiles through the canonical surface");
    let partitioned = module.partition_test(&module.tests[0]);
    let mut lowering_cache = LoweringCache::default();
    for island in &partitioned.islands {
        let lowered = lowering_cache
            .get_or_lower(island)
            .expect("rung 036 lowers through verified Weavy execution");
        assert!(lowered.program().fns.iter().all(|function| {
            function
                .code
                .iter()
                .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
        }));
    }

    let report = run_source(RUNG_036).expect("rung 036 runs through Executable");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 2);
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert!(lane.checks.iter().all(|check| check.passed));
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
        if std::env::var("WEAVY_JIT").as_deref() == Ok("0") {
            assert!(
                lane.events
                    .iter()
                    .filter_map(|event| match event.kind {
                        EventKind::ExecutionLane { facts, .. } => Some(facts),
                        _ => None,
                    })
                    .all(|facts| matches!(
                        facts,
                        vix::runtime::ExecutionFacts {
                            selected: vix::runtime::ExecutionLaneFact::Interpreter,
                            fallback: Some(
                                vix::runtime::ExecutionFallbackFact::DisabledByEnvironment
                            ),
                            ..
                        }
                    ))
            );
        }
    }
}

#[test]
fn rung_047_string_to_path_remains_rejected_at_the_declared_boundary() {
    let (expected_message, _) = reject_header(RUNG_047);
    let diagnostics = Compiler::new()
        .compile(RUNG_047)
        .expect_err("rung 047 must reject implicit String-to-Path conversion");
    assert_eq!(diagnostics.entries.len(), 1);
    let diagnostic = &diagnostics.entries[0];
    assert_eq!(diagnostic.code, DiagnosticCode::StringIsNotPath);
    assert_eq!(diagnostic.message(), expected_message);
}

// Rung 037 — filter_map preserves source keys for `Some` rows; flat_map composes
// outer/inner keys deterministically. Both remain codata recipes until an explicit
// collect materializes an ordered Map through the verified production path, with no
// host call and no dense-array substitute for the keyed collection.
#[test]
fn rung_037_filter_map_and_flat_map_run_through_verified_production_path() {
    let module = Compiler::new()
        .compile(RUNG_037)
        .expect("rung 037 compiles through the canonical surface");
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "filter_map_flat_map")
        .expect("rung 037 test function exists");

    // filter_map is a distinct key-preserving codata recipe: Stream<Int, Int>.
    let filter_map = function
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::StreamFilterMap))
        .expect("filter_map is a distinct codata recipe");
    assert_eq!(filter_map.ty, VirType::stream(VirType::Int, VirType::Int));
    assert_eq!(filter_map.effect.kind, EffectKind::Codata);

    // flat_map composes the outer position key with the inner position key into a
    // tuple key: Stream<(Int, Int), Int>.
    let flat_map = function
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::StreamFlatMap))
        .expect("flat_map is a distinct codata recipe");
    assert_eq!(
        flat_map.ty,
        VirType::stream(
            VirType::Tuple(vec![VirType::Int, VirType::Int]),
            VirType::Int,
        ),
    );
    assert_eq!(flat_map.effect.kind, EffectKind::Codata);

    // Every lowered frame stays inside the verified machine: no host calls.
    let partitioned = module.partition_test(&module.tests[0]);
    let mut lowering_cache = LoweringCache::default();
    for island in &partitioned.islands {
        let lowered = lowering_cache
            .get_or_lower(island)
            .expect("rung 037 lowers through verified Weavy execution");
        assert!(lowered.program().fns.iter().all(|function| {
            function
                .code
                .iter()
                .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
        }));
        assert!(
            lowered
                .program()
                .fns
                .iter()
                .flat_map(|function| &function.code)
                .any(|op| matches!(op, WeavyOp::OrderedInsertCommit { .. })),
            "collect materializes rows through the ordered collection substrate",
        );
    }

    let report = run_source(RUNG_037).expect("rung 037 runs through Executable");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 2);
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert!(lane.checks.iter().all(|check| check.passed));
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
    }
}

// Rung 039 — filtering a position-keyed stream carries the surviving source
// positions through to the collected Map: `%{0 => 50, 2 => 40, 3 => 20}`. The
// filter remains a codata recipe until an explicit collect materializes the
// keyed Map through the verified production path, with no host call and no
// dense-array substitute that would renumber the survivors.
#[test]
fn rung_039_indexed_roundtrip_carries_survivor_positions_through_collect() {
    let module = Compiler::new()
        .compile(RUNG_039)
        .expect("rung 039 compiles through the canonical surface");
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "indexed_roundtrip")
        .expect("rung 039 test function exists");

    // The survivor filter is a distinct key-preserving codata recipe over the
    // position-keyed array stream: Stream<Int, Int>.
    let filter = function
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::StreamFilter))
        .expect("filter remains a distinct codata recipe until collection");
    assert_eq!(filter.ty, VirType::stream(VirType::Int, VirType::Int));
    assert_eq!(filter.effect.kind, EffectKind::Codata);

    // Collect materializes the position-keyed Map, not a renumbered dense array.
    let collect = function
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::StreamCollect))
        .expect("collect is the explicit stream materialization boundary");
    assert_eq!(collect.ty, VirType::map(VirType::Int, VirType::Int));

    // Every lowered frame stays inside the verified machine: no host calls, and
    // collection materializes rows through the ordered collection substrate.
    let partitioned = module.partition_test(&module.tests[0]);
    let mut lowering_cache = LoweringCache::default();
    for island in &partitioned.islands {
        let lowered = lowering_cache
            .get_or_lower(island)
            .expect("rung 039 lowers through verified Weavy execution");
        assert!(lowered.program().fns.iter().all(|function| {
            function
                .code
                .iter()
                .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
        }));
        assert!(
            lowered
                .program()
                .fns
                .iter()
                .flat_map(|function| &function.code)
                .any(|op| matches!(op, WeavyOp::OrderedInsertCommit { .. })),
            "collect materializes survivors through the ordered collection substrate",
        );
    }

    let report = run_source(RUNG_039).expect("rung 039 runs through Executable");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 1);
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert!(lane.checks.iter().all(|check| check.passed));
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
    }
}

// Rung 040 — `.sorted where { order: by_key(|x| x.weight) }` sorts the collected
// values by the caller-supplied `Order<Row>`. `by_key(f)` is an ordinary typed
// Vix recipe: it compares the structural order of the extracted key `f(x)`, and
// breaks equal keys by the structural order of the whole source row. The two
// weight-2 rows "b" and "a" therefore settle as `a` before `b` (structural row
// order), yielding the unchanged expected order `c, a, b`. Sorting lowers
// entirely through the verified machine with no host call.
#[test]
fn rung_040_sorted_with_order_runs_through_verified_production_path() {
    let module = Compiler::new()
        .compile(RUNG_040)
        .expect("rung 040 compiles through the canonical surface");

    // Every lowered frame stays inside the verified machine: no host calls, and
    // the caller's Order sorts through the same structural sort substrate as the
    // zero-argument `.sorted()` path.
    let partitioned = module.partition_test(&module.tests[0]);
    let mut lowering_cache = LoweringCache::default();
    for island in &partitioned.islands {
        let lowered = lowering_cache
            .get_or_lower(island)
            .expect("rung 040 lowers through verified Weavy execution");
        assert!(lowered.program().fns.iter().all(|function| {
            function
                .code
                .iter()
                .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
        }));
    }

    let report = run_source(RUNG_040).expect("rung 040 runs through Executable");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 3);
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert!(lane.checks.iter().all(|check| check.passed));
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
        if std::env::var("WEAVY_JIT").as_deref() == Ok("0") {
            assert!(
                lane.events
                    .iter()
                    .filter_map(|event| match event.kind {
                        EventKind::ExecutionLane { facts, .. } => Some(facts),
                        _ => None,
                    })
                    .all(|facts| matches!(
                        facts,
                        vix::runtime::ExecutionFacts {
                            selected: vix::runtime::ExecutionLaneFact::Interpreter,
                            fallback: Some(
                                vix::runtime::ExecutionFallbackFact::DisabledByEnvironment
                            ),
                            ..
                        }
                    ))
            );
        }
    }
}

// Rung 045 — string operations: `+` concat (already green from rung 029), plus
// `contains`, `split_once`, and `parse_int`. Each new method is a typed Vix recipe
// over the resident semantic byte sequence of the string value (never over the
// opaque handle integer). `contains(needle)` decides membership; `split_once(delim)`
// partitions on the first delimiter occurrence into a `(String, String)` pair;
// `parse_int()` reads the decimal integer the bytes denote. All three lower entirely
// through the verified machine with no host call, and their resident results feed
// semantic equality exactly like string literals.
#[test]
fn rung_045_string_operations_run_through_verified_production_path() {
    let module = Compiler::new()
        .compile(RUNG_045)
        .expect("rung 045 compiles through the canonical surface");

    // Every lowered frame stays inside the verified machine: no host calls. The
    // three new string operations lower to verifier-admitted string-byte ops.
    let partitioned = module.partition_test(&module.tests[0]);
    let mut lowering_cache = LoweringCache::default();
    for island in &partitioned.islands {
        let lowered = lowering_cache
            .get_or_lower(island)
            .expect("rung 045 lowers through verified Weavy execution");
        assert!(lowered.program().fns.iter().all(|function| {
            function
                .code
                .iter()
                .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
        }));
    }

    let report = run_source(RUNG_045).expect("rung 045 runs through Executable");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 5);
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert!(lane.checks.iter().all(|check| check.passed));
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
        if std::env::var("WEAVY_JIT").as_deref() == Ok("0") {
            assert!(
                lane.events
                    .iter()
                    .filter_map(|event| match event.kind {
                        EventKind::ExecutionLane { facts, .. } => Some(facts),
                        _ => None,
                    })
                    .all(|facts| matches!(
                        facts,
                        vix::runtime::ExecutionFacts {
                            selected: vix::runtime::ExecutionLaneFact::Interpreter,
                            fallback: Some(
                                vix::runtime::ExecutionFallbackFact::DisabledByEnvironment
                            ),
                            ..
                        }
                    ))
            );
        }
    }
}

#[test]
fn rung_046_paths_join_and_render_through_verified_execution() {
    let module = Compiler::new()
        .compile(RUNG_046)
        .expect("rung 046 compiles through the canonical Path surface");
    let partitioned = module.partition_test(&module.tests[0]);
    let mut lowering_cache = LoweringCache::default();
    let mut has_path_join = false;
    for island in &partitioned.islands {
        let lowered = lowering_cache
            .get_or_lower(island)
            .expect("rung 046 lowers through verified Weavy execution");
        assert!(lowered.program().fns.iter().all(|function| {
            function
                .code
                .iter()
                .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
        }));
        assert!(lowered.program().fns.iter().any(|function| {
            function
                .code
                .iter()
                .any(|op| matches!(op, WeavyOp::ByteProject { .. }))
        }));
        has_path_join |= lowered.program().fns.iter().any(|function| {
            function
                .code
                .iter()
                .any(|op| matches!(op, WeavyOp::PathJoin { .. }))
        });
    }
    assert!(
        has_path_join,
        "rung 046 lowers joins through the verified PathJoin op"
    );

    let report = run_source(RUNG_046).expect("rung 046 runs through Executable");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 3);
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
    }
}

#[test]
fn structured_if_and_match_emit_control_and_arm_pcs_with_distinct_owners() {
    const SOURCE: &str = r#"
enum Flag {
    A,
    B,
}

fn choose(flag: Flag) -> Int {
    match flag {
        Flag::A => if true { 10 } else { 11 },
        Flag::B => 20,
    }
}

#[test]
fn attribution() -> Stream<Check> {
    yield expect_eq(choose(Flag::A), 10);
    yield expect_eq(choose(Flag::B), 20);
}
"#;

    let module = Compiler::new().compile(SOURCE).expect("source compiles");
    let choose = module
        .functions
        .iter()
        .find(|function| function.name == "choose")
        .expect("choose function exists");
    let match_node = choose
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::Match { .. }))
        .expect("choose lowers through Match");
    let if_node = choose
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::If { .. }))
        .expect("first arm lowers through If");
    let VirOp::Match { arms } = &match_node.op else {
        unreachable!("match node selected above")
    };
    let arm_a_output = NodeRef {
        function: choose.id,
        node: arms[0].output,
    };
    let arm_b_output = NodeRef {
        function: choose.id,
        node: arms[1].output,
    };
    let VirOp::If {
        consequent,
        alternative,
    } = &if_node.op
    else {
        unreachable!("if node selected above")
    };
    let consequent_output = NodeRef {
        function: choose.id,
        node: consequent.output,
    };
    let alternative_output = NodeRef {
        function: choose.id,
        node: alternative.output,
    };

    let partitioned = module.partition_test(&module.tests[0]);
    let island = &partitioned.islands[0];
    let attribution = attribution_for(island);
    let mut lowering_cache = LoweringCache::default();
    let lowered = lowering_cache
        .get_or_lower(island)
        .expect("source lowers to Weavy");
    assert_pc_maps_complete(lowered);

    let choose_frame = frame_index(&attribution.functions, choose.id);
    let choose_code = &lowered.program().fns[choose_frame].code;
    let match_ref = NodeRef {
        function: choose.id,
        node: match_node.id,
    };
    let if_ref = NodeRef {
        function: choose.id,
        node: if_node.id,
    };

    assert!(choose_code.iter().enumerate().any(|(pc, op)| {
        matches!(
            op,
            WeavyOp::EqI64 { .. } | WeavyOp::JumpIfZero { .. } | WeavyOp::Jump { .. }
        ) && lowered.node_for_pc(choose_frame as u32, pc as u32) == Some(match_ref)
    }));
    assert!(choose_code.iter().enumerate().any(|(pc, op)| {
        matches!(op, WeavyOp::JumpIfZero { .. } | WeavyOp::Jump { .. })
            && lowered.node_for_pc(choose_frame as u32, pc as u32) == Some(if_ref)
    }));

    let if_owned_pcs = pcs_for_node(lowered, choose_frame, if_ref);
    assert!(
        if_owned_pcs.iter().any(|pc| matches!(
            choose_code[*pc],
            WeavyOp::JumpIfZero { .. } | WeavyOp::Jump { .. } | WeavyOp::CopyI64 { .. }
        )),
        "the nested if node should own its dispatch and merge scaffolding",
    );
    assert!(
        pcs_for_node(lowered, choose_frame, consequent_output)
            .iter()
            .any(|pc| matches!(choose_code[*pc], WeavyOp::ConstI64 { value: 10, .. })),
        "the consequent literal must own its emitted Weavy pc",
    );
    assert!(
        pcs_for_node(lowered, choose_frame, alternative_output)
            .iter()
            .any(|pc| matches!(choose_code[*pc], WeavyOp::ConstI64 { value: 11, .. })),
        "the alternative literal must own its emitted Weavy pc",
    );

    let arm_a_pcs = pcs_for_node(lowered, choose_frame, arm_a_output);
    assert!(
        arm_a_pcs.iter().any(|pc| matches!(
            choose_code[*pc],
            WeavyOp::CopyI64 { .. } | WeavyOp::Ret { .. }
        )),
        "arm A output must own at least one lowered Weavy pc",
    );
    let arm_b_pcs = pcs_for_node(lowered, choose_frame, arm_b_output);
    assert!(
        arm_b_pcs
            .iter()
            .any(|pc| matches!(choose_code[*pc], WeavyOp::ConstI64 { value: 20, .. })),
        "arm B output must own its literal pc",
    );

    let last_pc = choose_code.len() - 1;
    assert_eq!(
        lowered.node_for_pc(choose_frame as u32, last_pc as u32),
        Some(NodeRef {
            function: choose.id,
            node: choose.output.expect("choose has an output node"),
        }),
    );
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
    assert!(lowered.constants.iter().any(|constant| {
        constant.bytes.as_slice() == b"hello"
            && constant.root.function.0 == 0
            && constant.owner.function.0 == 0
            && constant.root.entry == 0
            && constant.owner.entry == 0
            && constant.root.slot == constant.owner.slot
            && constant.root.schema == constant.owner.schema
    }));
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
        assert_eq!(lowered.program().fns.len(), index + 2);
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
    assert_eq!(lowered.program().fns.len(), 2);
    assert!(lowered.program().fns[0].code.iter().any(|op| matches!(
        op,
        WeavyOp::Call { args, .. } if args.len() == 1 && args[0].size == 16
    )));
    assert!(
        lowered.program().fns[1]
            .code
            .iter()
            .any(|op| matches!(op, WeavyOp::Ret { size: 16, .. }))
    );
    assert!(lowered.program().fns.iter().all(|function| {
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
        assert!(lowered.program().fns.iter().all(|function| {
            function
                .code
                .iter()
                .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
        }));
        assert!(lowered.program().fns.iter().any(|function| {
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
    assert_eq!(
        VirType::Enum(shape.clone()).word_width(),
        Some(3),
        "Shape has a finite inline layout"
    );
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
        let entry = &lowered.program().fns[0].code;
        let trace_pc = entry
            .iter()
            .position(|op| matches!(op, WeavyOp::Trace { id } if *id == trace_id))
            .expect("variant construction has a Weavy trace mark");
        assert!(matches!(
            &entry[trace_pc + 1],
            WeavyOp::EnumConstruct { variant: lowered_variant, .. } if lowered_variant == variant
        ));
        assert!(lowered.program().fns.iter().any(|function| {
            function.code.iter().any(
                |op| matches!(op, WeavyOp::Call { args, .. } if args.len() == 1 && args[0].size == 24),
            )
        }));
        assert!(lowered.program().fns.iter().any(|function| {
            function
                .code
                .iter()
                .any(|op| matches!(op, WeavyOp::JumpIfZero { .. }))
        }));
        assert!(lowered.program().fns.iter().any(|function| {
            function
                .code
                .iter()
                .any(|op| matches!(op, WeavyOp::Jump { .. }))
        }));
        assert!(lowered.program().fns.iter().all(|function| {
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

    let innards = run_source_innards(RUNG_007).expect("rung 007 diagnostic lane runs");
    assert!(innards.passed());
    assert!(innards.agrees());

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
    assert_eq!(
        expected_variants,
        arms.iter().map(|arm| arm.variant).collect::<Vec<_>>(),
    );
    let mut selected_arm_marks = vec![0usize; partitioned.islands.len()];
    for event in &innards.plain.events {
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
        let island_index = innards
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
    assert!(
        report
            .plain
            .events
            .iter()
            .all(|event| !matches!(event.kind, EventKind::WeavyMark { .. })),
        "production execution strips branch-interior trace marks",
    );
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
        assert!(lowered.program().fns.iter().all(|function| {
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
        assert!(lowered.program().fns.iter().any(|function| {
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
        assert!(lowered.program().fns.iter().all(|function| {
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
        saw_integer_order |= lowered.program().fns.iter().any(|function| {
            function
                .code
                .iter()
                .any(|op| matches!(op, WeavyOp::LtI64 { .. }))
                && function
                    .code
                    .iter()
                    .any(|op| matches!(op, WeavyOp::GtI64 { .. }))
        });
        saw_value_bytes_order |= lowered.program().fns.iter().any(|function| {
            function
                .code
                .iter()
                .any(|op| matches!(op, WeavyOp::CompareValueBytes { .. }))
        });
        assert!(lowered.program().fns.iter().all(|function| {
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
        for function in &lowered.program().fns {
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
        let entry = &lowered.program().fns[0];
        let projected_fields = entry
            .code
            .iter()
            .filter(|op| matches!(op, WeavyOp::ProductProject { .. }))
            .count();
        assert_eq!(projected_fields, 4);
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

// r[verify lang.diagnostics.typed]
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
            assert!(lowered.program().fns.iter().any(|function| {
                function.code.windows(2).any(|ops| {
                    matches!(ops[0], WeavyOp::Trace { id } if id == trace_id)
                        && matches!(ops[1], WeavyOp::JumpIfZero { .. })
                })
            }));
        }
        assert!(lowered.program().fns.iter().all(|function| {
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
            assert!(lowered.program().fns[0].code.windows(2).any(|ops| {
                matches!(ops[0], WeavyOp::Trace { id } if id == trace_id)
                    && matches!(ops[1], WeavyOp::JumpIfZero { .. })
            }));
        }
        assert!(lowered.program().fns.iter().all(|function| {
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
        assert!(lowered.program().fns.iter().any(|function| {
            function
                .code
                .iter()
                .any(|op| matches!(op, WeavyOp::JumpIfZero { .. }))
        }));
        assert!(lowered.program().fns.iter().any(|function| {
            function
                .code
                .iter()
                .any(|op| matches!(op, WeavyOp::Jump { .. }))
        }));
        assert!(lowered.program().fns.iter().all(|function| {
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

// r[verify lang.diagnostics.typed]
// r[verify lang.diagnostics.non-exhaustive-match]
#[test]
fn rung_018_non_exhaustive_match_is_rejected_with_declared_message_and_line() {
    let (expected_message, expected_line) = reject_header(RUNG_018);
    let diagnostics = Compiler::new()
        .compile(RUNG_018)
        .expect_err("rung 018 must be rejected");
    assert_eq!(diagnostics.entries.len(), 1);
    let diagnostic = &diagnostics.entries[0];
    assert_eq!(diagnostic.code, DiagnosticCode::NonExhaustiveMatch);
    assert_eq!(diagnostic.message(), expected_message);
    assert!(matches!(
        &diagnostic.payload,
        vix::diagnostic::DiagnosticPayload::Match { missing }
            if missing.iter().map(String::as_str).eq(["Amber"])
    ));
    assert_eq!(diagnostic.labels.len(), 1);
    assert_eq!(source_line(RUNG_018, diagnostic.labels[0].span.start), 7);
    assert_eq!(
        source_line(RUNG_018, diagnostic.primary.start),
        expected_line
    );
}

#[test]
fn rung_019_tuple_let_destructures_one_value_through_vir_and_weavy() {
    let module = Compiler::new()
        .compile(RUNG_019)
        .expect("rung 019 compiles");
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "let_destructuring")
        .expect("rung 019 contains let_destructuring");
    let projections = function
        .nodes
        .iter()
        .filter(|node| matches!(node.op, VirOp::Project { .. }))
        .collect::<Vec<_>>();
    assert_eq!(projections.len(), 2);
    assert!(matches!(projections[0].op, VirOp::Project { index: 0 }));
    assert!(matches!(projections[1].op, VirOp::Project { index: 1 }));
    assert_eq!(projections[0].inputs, projections[1].inputs);
    assert!(matches!(
        function.nodes[projections[0].inputs[0].0 as usize].op,
        VirOp::Call(_)
    ));

    let report = run_source(RUNG_019).expect("rung 019 compiles and runs");
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
fn rung_020_tuple_match_patterns_select_and_bind_in_source_order() {
    let module = Compiler::new()
        .compile(RUNG_020)
        .expect("rung 020 compiles");
    let describe = module
        .functions
        .iter()
        .find(|function| function.name == "describe")
        .expect("rung 020 contains describe");
    let (arms, fallback) = describe
        .nodes
        .iter()
        .find_map(|node| match &node.op {
            VirOp::OrderedMatch { arms, fallback } => Some((arms, fallback)),
            _ => None,
        })
        .expect("describe contains an ordered tuple-pattern match");
    assert_eq!(arms.len(), 2);
    assert!(arms.iter().all(|arm| {
        arm.condition.nodes.contains(&arm.condition.output)
            && arm.body.nodes.contains(&arm.body.output)
    }));
    assert!(fallback.nodes.contains(&fallback.output));
    assert!(
        describe
            .nodes
            .iter()
            .any(|node| matches!(node.op, VirOp::If { .. }))
    );

    let report = run_source(RUNG_020).expect("rung 020 compiles and runs");
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
fn rung_021_closure_parameters_destructure_callable_values() {
    let module = Compiler::new()
        .compile(RUNG_021)
        .expect("rung 021 compiles");
    assert_eq!(module.functions.len(), 3);
    assert_eq!(
        module
            .functions
            .iter()
            .map(|function| function.name.as_str())
            .collect::<Vec<_>>(),
        [
            "closure_destructuring",
            "closure_destructuring::closure#0",
            "closure_destructuring::closure#1",
        ]
    );
    assert_eq!(
        module
            .functions
            .iter()
            .flat_map(|function| &function.nodes)
            .filter(|node| matches!(node.op, VirOp::Closure(_)))
            .count(),
        2
    );
    assert_eq!(
        module
            .functions
            .iter()
            .flat_map(|function| &function.nodes)
            .filter(|node| matches!(node.op, VirOp::CallValue))
            .count(),
        2
    );
    assert_eq!(
        module
            .functions
            .iter()
            .skip(1)
            .flat_map(|function| &function.nodes)
            .filter(|node| matches!(node.op, VirOp::Project { .. }))
            .count(),
        6,
        "both closure parameter patterns lower to ordinary tuple projections"
    );

    let partitioned = module.partition_test(&module.tests[0]);
    assert_eq!(partitioned.islands.len(), 1);
    assert_eq!(partitioned.islands[0].callees.len(), 2);
    let mut lowering_cache = LoweringCache::default();
    let lowered = lowering_cache
        .get_or_lower(&partitioned.islands[0])
        .expect("rung 021 lowers to Weavy");
    assert_eq!(lowered.program().fns.len(), 3);
    assert_eq!(
        lowered
            .program()
            .fns
            .iter()
            .flat_map(|function| &function.code)
            .filter(|op| matches!(op, WeavyOp::CallIndirect { .. }))
            .count(),
        2
    );
    for closure in partitioned.islands[0]
        .nodes
        .iter()
        .filter_map(|node| match node.op {
            VirOp::Closure(target) => Some((node.id, target)),
            _ => None,
        })
    {
        let (node, target) = closure;
        let region = partitioned.islands[0]
            .nodes
            .iter()
            .position(|candidate| candidate.id == node)
            .expect("closure node has a canonical contract region");
        let shape = lowered.contract().functions[0].frame.regions[region + 1]
            .value_shape
            .expect("closure node has a structural value shape");
        let ValueShapeKind::Product { fields } =
            &lowered.contract().value_shapes[shape.0 as usize].kind
        else {
            panic!("closure is a product-shaped callable value");
        };
        let [WordKind::Callable(signature)] = fields[0].shape.words[0].as_slice() else {
            panic!("closure field zero is its callable signature");
        };
        let target_frame = partitioned.islands[0]
            .callees
            .iter()
            .position(|function| function.id == target)
            .expect("closure target is in the island")
            + 1;
        assert_eq!(
            Some(*signature),
            lowered.contract().functions[target_frame].call_contract,
            "closure target ABI is its source-level callable signature"
        );
    }
    assert!(lowered.program().fns.iter().all(|function| {
        function
            .code
            .iter()
            .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
    }));

    let report = run_source(RUNG_021).expect("rung 021 compiles and runs");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 1);
    assert_eq!(report.plain.checks, report.chaos.checks);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
    assert_eq!(report.chaos.counters.pure_host_calls, 0);
    assert_eq!(report.plain.receipt_count, 0);
    assert_eq!(report.chaos.receipt_count, 0);
}

// r[verify lang.pattern.record]
#[test]
fn rung_022_nested_record_patterns_project_named_fields() {
    let module = Compiler::new()
        .compile(RUNG_022)
        .expect("rung 022 compiles");
    assert_eq!(
        module
            .records
            .iter()
            .map(|record| record.name.as_str())
            .collect::<Vec<_>>(),
        ["Point", "Line"]
    );
    let is_vertical = module
        .functions
        .iter()
        .find(|function| function.name == "is_vertical")
        .expect("rung 022 contains is_vertical");
    let projections = is_vertical
        .nodes
        .iter()
        .filter_map(|node| match node.op {
            VirOp::Project { index } => Some(index),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(projections, [0, 0, 1, 0]);
    let (arms, fallback) = is_vertical
        .nodes
        .iter()
        .find_map(|node| match &node.op {
            VirOp::OrderedMatch { arms, fallback } => Some((arms, fallback)),
            _ => None,
        })
        .expect("record pattern lowers to an ordered match region");
    assert!(arms.is_empty());
    assert!(fallback.nodes.contains(&fallback.output));

    let without_rest = RUNG_022.replace(", ..", "");
    let diagnostics = Compiler::new()
        .compile(&without_rest)
        .expect_err("omitting record fields requires an explicit rest pattern");
    assert_eq!(diagnostics.entries[0].code, DiagnosticCode::MissingField);

    let partitioned = module.partition_test(&module.tests[0]);
    assert_eq!(partitioned.islands.len(), 1);
    assert_eq!(partitioned.islands[0].callees.len(), 1);
    let mut lowering_cache = LoweringCache::default();
    let lowered = lowering_cache
        .get_or_lower(&partitioned.islands[0])
        .expect("rung 022 lowers to Weavy");
    assert_eq!(lowered.program().fns.len(), 2);
    assert!(lowered.program().fns.iter().all(|function| {
        function
            .code
            .iter()
            .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
    }));

    let report = run_source(RUNG_022).expect("rung 022 compiles and runs");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 1);
    assert_eq!(report.plain.checks, report.chaos.checks);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
    assert_eq!(report.chaos.counters.pure_host_calls, 0);
    assert_eq!(report.plain.receipt_count, 0);
    assert_eq!(report.chaos.receipt_count, 0);
}

// r[verify machine.value.option-no-store-alloc]
#[test]
fn rung_023_option_construction_matching_and_checks() {
    let module = Compiler::new()
        .compile(RUNG_023)
        .expect("rung 023 compiles");
    let checked_div = module
        .functions
        .iter()
        .find(|function| function.name == "checked_div")
        .expect("rung 023 contains checked_div");
    let VirType::Enum(option) = &checked_div.return_type else {
        panic!("checked_div returns Option<Int>")
    };
    assert_eq!(option.name, "Option<Int>");
    assert_eq!(
        option
            .variants
            .iter()
            .map(|variant| variant.name.as_str())
            .collect::<Vec<_>>(),
        ["Some", "None"]
    );
    assert_eq!(checked_div.return_type.option_inner(), Some(&VirType::Int));
    assert_eq!(checked_div.return_type.word_width(), Some(2));
    assert_eq!(
        checked_div
            .nodes
            .iter()
            .filter(|node| matches!(node.op, VirOp::Div))
            .count(),
        1
    );
    assert_eq!(
        checked_div
            .nodes
            .iter()
            .filter_map(|node| match node.op {
                VirOp::Variant { variant } => Some(variant),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [1, 0],
        "None and Some are ordinary Option variants"
    );
    let test = module
        .functions
        .iter()
        .find(|function| function.name == "option")
        .expect("rung 023 contains option test");
    assert_eq!(
        test.nodes
            .iter()
            .filter(|node| matches!(node.op, VirOp::IsVariant { .. }))
            .count(),
        2
    );
    assert!(
        test.nodes
            .iter()
            .any(|node| matches!(&node.op, VirOp::Match { arms } if arms.len() == 2))
    );

    let partitioned = module.partition_test(&module.tests[0]);
    assert_eq!(partitioned.islands.len(), 3);
    let mut lowering_cache = LoweringCache::default();
    let mut division_ops = 0usize;
    for island in &partitioned.islands {
        let lowered = lowering_cache
            .get_or_lower(island)
            .expect("rung 023 lowers to Weavy");
        division_ops += lowered
            .program()
            .fns
            .iter()
            .flat_map(|function| &function.code)
            .filter(|op| matches!(op, WeavyOp::DivI64 { .. }))
            .count();
        assert!(lowered.program().fns.iter().all(|function| {
            function
                .code
                .iter()
                .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
        }));
    }
    assert!(division_ops > 0);

    let report = run_source(RUNG_023).expect("rung 023 compiles and runs");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 3);
    assert_eq!(report.plain.checks, report.chaos.checks);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
    assert_eq!(report.chaos.counters.pure_host_calls, 0);
    assert_eq!(report.plain.receipt_count, 0);
    assert_eq!(report.chaos.receipt_count, 0);
}

// r[verify lang.types.generic-enum-monomorphized]
#[test]
fn rung_024_user_generic_enums_construct_and_compare() {
    let local_application = Compiler::new()
        .compile(
            r#"
enum Outcome<T> {
    Ok(T),
    Err(String),
}

fn local_application(flag: Bool) -> Bool {
    let result: Outcome<Bool> = Outcome::Ok(flag);
    match result {
        Outcome::Ok(value) => value,
        Outcome::Err(_) => false,
    }
}
"#,
        )
        .expect("a generic enum application local to a function is resolved");
    assert_eq!(local_application.enums[0].name, "Outcome<Bool>");

    let module = Compiler::new()
        .compile(RUNG_024)
        .expect("rung 024 compiles");
    assert_eq!(module.enums.len(), 1);
    let outcome = &module.enums[0];
    assert_eq!(outcome.name, "Outcome<Bool>");
    assert_eq!(
        outcome
            .variants
            .iter()
            .map(|variant| variant.name.as_str())
            .collect::<Vec<_>>(),
        ["Ok", "Err"]
    );
    assert!(matches!(
        &outcome.variants[0].payload,
        VariantPayload::Tuple(payload) if payload == &[VirType::Bool]
    ));
    assert!(matches!(
        &outcome.variants[1].payload,
        VariantPayload::Tuple(payload) if payload == &[VirType::String]
    ));
    let outcome_type = VirType::Enum(outcome.clone());
    assert_eq!(outcome_type.word_width(), Some(2));

    let parse_flag = module
        .functions
        .iter()
        .find(|function| function.name == "parse_flag")
        .expect("rung 024 contains parse_flag");
    assert_eq!(parse_flag.return_type, outcome_type);
    assert_eq!(
        parse_flag
            .nodes
            .iter()
            .filter_map(|node| match node.op {
                VirOp::Variant { variant } => Some(variant),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [0, 0, 1]
    );
    let (arms, fallback) = parse_flag
        .nodes
        .iter()
        .find_map(|node| match &node.op {
            VirOp::OrderedMatch { arms, fallback } => Some((arms, fallback)),
            _ => None,
        })
        .expect("string patterns lower to ordered control regions");
    assert_eq!(arms.len(), 2);
    assert!(fallback.nodes.contains(&fallback.output));

    let partitioned = module.partition_test(&module.tests[0]);
    assert_eq!(partitioned.islands.len(), 2);
    let mut lowering_cache = LoweringCache::default();
    for island in &partitioned.islands {
        let lowered = lowering_cache
            .get_or_lower(island)
            .expect("rung 024 lowers to Weavy");
        assert!(lowered.program().fns.iter().all(|function| {
            function
                .code
                .iter()
                .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
        }));
    }

    let report = run_source(RUNG_024).expect("rung 024 compiles and runs");
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
fn enum_equality_cross_variant_is_false_without_projection_fault() {
    const SOURCE: &str = r#"
enum Outcome<T> {
    Ok(T),
    Err(String),
}

#[test]
fn enum_equality() -> Stream<Check> {
    let ok: Outcome<Bool> = Outcome::Ok(true);
    let err: Outcome<Bool> = Outcome::Err("no");
    let same_left: Outcome<Bool> = Outcome::Err("same");
    let same_right: Outcome<Bool> = Outcome::Err("same");
    yield expect_eq(ok == err, false);
    yield expect_eq(ok != err, true);
    yield expect_eq(same_left == same_right, true);
}
"#;

    let module = Compiler::new().compile(SOURCE).expect("source compiles");
    let partitioned = module.partition_test(&module.tests[0]);
    let mut lowering_cache = LoweringCache::default();
    assert!(partitioned.islands.iter().any(|island| {
        lowering_cache
            .get_or_lower(island)
            .expect("typed enum equality lowers")
            .program()
            .fns
            .iter()
            .flat_map(|function| &function.code)
            .any(|op| matches!(op, WeavyOp::CompareValueBytes { .. }))
    }));

    let report = run_source(SOURCE).expect("enum equality runs through Executable");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks, report.chaos.checks);
}

#[test]
fn executable_lane_facts_are_observable_after_spawn_and_replay_stable() {
    let report = run_source(RUNG_001).expect("rung 001 executes through Executable");
    let facts = |events: &[vix::runtime::Event]| {
        events
            .iter()
            .filter_map(|event| match event.kind {
                EventKind::ExecutionLane { facts, .. } => Some(facts),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let plain = facts(&report.plain.events);
    let chaos = facts(&report.chaos.events);
    assert!(
        !plain.is_empty(),
        "a task that reaches Weavy spawn emits lane facts"
    );
    assert_eq!(plain, chaos, "discard-before-spawn does not add lane facts");
    if std::env::var("WEAVY_JIT").as_deref() == Ok("0") {
        assert!(plain.iter().all(|facts| matches!(
            facts,
            vix::runtime::ExecutionFacts {
                selected: vix::runtime::ExecutionLaneFact::Interpreter,
                fallback: Some(vix::runtime::ExecutionFallbackFact::DisabledByEnvironment),
                ..
            }
        )));
    }
}

// r[verify lang.value.ordering-is-enum]
#[test]
fn rung_025_ordering_is_an_ordinary_matchable_enum() {
    let ordering = VirType::ordering();
    let VirType::Enum(ordering_enum) = &ordering else {
        panic!("Ordering is represented as an enum");
    };
    assert_eq!(
        ordering_enum
            .variants
            .iter()
            .map(|variant| variant.name.as_str())
            .collect::<Vec<_>>(),
        ["Less", "Equal", "Greater"]
    );

    let module = Compiler::new()
        .compile(RUNG_025)
        .expect("rung 025 compiles");
    let describe = module
        .functions
        .iter()
        .find(|function| function.name == "describe")
        .expect("rung 025 contains describe");
    assert_eq!(describe.return_type, VirType::String);
    assert!(
        describe
            .nodes
            .iter()
            .any(|node| node.ty == ordering && matches!(node.op, VirOp::Compare))
    );
    let variants = describe
        .nodes
        .iter()
        .find_map(|node| match &node.op {
            VirOp::Match { arms } => Some(arms.iter().map(|arm| arm.variant).collect::<Vec<_>>()),
            _ => None,
        })
        .expect("Ordering match lowers through the ordinary enum Match op");
    assert_eq!(variants, [0, 1, 2]);

    let partitioned = module.partition_test(&module.tests[0]);
    assert_eq!(partitioned.islands.len(), 2);
    let mut lowering_cache = LoweringCache::default();
    for island in &partitioned.islands {
        let lowered = lowering_cache
            .get_or_lower(island)
            .expect("rung 025 lowers to Weavy");
        assert!(lowered.program().fns.iter().any(|function| {
            function
                .code
                .iter()
                .any(|op| matches!(op, WeavyOp::LtI64 { .. }))
                && function
                    .code
                    .iter()
                    .any(|op| matches!(op, WeavyOp::GtI64 { .. }))
                && function
                    .code
                    .iter()
                    .any(|op| matches!(op, WeavyOp::EqI64 { .. }))
        }));
        assert!(lowered.program().fns.iter().all(|function| {
            function
                .code
                .iter()
                .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
        }));
    }

    let report = run_source(RUNG_025).expect("rung 025 compiles and runs");
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
fn rung_026_arrays_run_through_verified_execution_without_publication() {
    let report = run_source(RUNG_026).expect("rung 026 compiles and runs through Executable");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 3);
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert!(lane.checks.iter().all(|check| check.passed));
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
        let published_schemas = lane
            .values
            .iter()
            .map(|value| value.identity.schema.clone())
            .collect::<BTreeSet<_>>();
        assert!(lane.events.iter().all(|event| match &event.kind {
            EventKind::StoreAlloc { identity, .. } => {
                identity.schema == VirType::Check.schema_ref()
                    || published_schemas.contains(&identity.schema)
            }
            _ => true,
        }));
        if std::env::var("WEAVY_JIT").as_deref() == Ok("0") {
            assert!(
                lane.events
                    .iter()
                    .filter_map(|event| match event.kind {
                        EventKind::ExecutionLane { facts, .. } => Some(facts),
                        _ => None,
                    })
                    .all(|facts| matches!(
                        facts,
                        vix::runtime::ExecutionFacts {
                            selected: vix::runtime::ExecutionLaneFact::Interpreter,
                            fallback: Some(
                                vix::runtime::ExecutionFallbackFact::DisabledByEnvironment
                            ),
                            ..
                        }
                    ))
            );
        }
    }
}

#[test]
fn rung_027_array_map_runs_through_shared_publication() {
    let module = Compiler::new()
        .compile(RUNG_027)
        .expect("rung 027 compiles to graph VIR");
    let partitioned = module.partition_test(&module.tests[0]);
    assert_eq!(partitioned.islands.len(), 4);
    assert_eq!(partitioned.values.len(), 1);
    let mut cache = LoweringCache::default();
    let value_island = &partitioned.values[0].island;
    let [decision] = value_island.array_map_partitions.as_slice() else {
        panic!("the shared rung 027 value island owns one ArrayMap decision")
    };
    assert_eq!(decision.shape, ArrayMapExecutionShape::MaterializedLoop);
    let lowered = cache
        .get_or_lower(value_island)
        .expect("rung 027 fused value island verifies");
    let map_pcs = pcs_for_node(lowered, 0, decision.node);
    assert!(
        map_pcs
            .iter()
            .any(|pc| matches!(lowered.program().fns[0].code[*pc], WeavyOp::ArrayNew { .. })),
        "the shared value island materializes its published dense array",
    );
    assert!(
        map_pcs.iter().any(|pc| matches!(
            lowered.program().fns[0].code[*pc],
            WeavyOp::ArrayStore { .. }
        )),
        "the materialization loop fills the published dense array",
    );

    let report = run_source(RUNG_027).expect("rung 027 compiles and runs through Executable");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 4);
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert!(lane.checks.iter().all(|check| check.passed));
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
        let published_schemas = lane
            .values
            .iter()
            .map(|value| value.identity.schema.clone())
            .collect::<BTreeSet<_>>();
        assert!(lane.events.iter().all(|event| match &event.kind {
            EventKind::StoreAlloc { identity, .. } => {
                identity.schema == VirType::Check.schema_ref()
                    || published_schemas.contains(&identity.schema)
            }
            _ => true,
        }));
        if std::env::var("WEAVY_JIT").as_deref() == Ok("0") {
            assert!(
                lane.events
                    .iter()
                    .filter_map(|event| match event.kind {
                        EventKind::ExecutionLane { facts, .. } => Some(facts),
                        _ => None,
                    })
                    .all(|facts| matches!(
                        facts,
                        vix::runtime::ExecutionFacts {
                            selected: vix::runtime::ExecutionLaneFact::Interpreter,
                            fallback: Some(
                                vix::runtime::ExecutionFallbackFact::DisabledByEnvironment
                            ),
                            ..
                        }
                    ))
            );
        }
    }
}

#[test]
fn rung_048_captured_closures_run_directly_and_through_array_map() {
    let module = Compiler::new()
        .compile(RUNG_048)
        .expect("rung 048 compiles with a by-value closure environment");
    let root = &module.functions[module.tests[0].function.0 as usize];
    let closure = root
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::Closure(_)))
        .expect("rung 048 constructs one closure value");
    let VirOp::Closure(target) = closure.op else {
        unreachable!("selected node is a closure")
    };
    assert!(
        root.nodes
            .iter()
            .any(|node| matches!(node.op, VirOp::CallValue))
    );
    assert!(
        root.nodes
            .iter()
            .any(|node| matches!(node.op, VirOp::ArrayMap { .. }))
    );

    let partitioned = module.partition_test(&module.tests[0]);
    let island = &partitioned.islands[0];
    let mut cache = LoweringCache::default();
    let lowered = cache
        .get_or_lower(island)
        .expect("rung 048 lowers its captured closure through verified Weavy");
    let attribution = attribution_for(island);
    let target_frame = frame_index(&attribution.functions, target);
    let callable = lowered.contract().functions[target_frame]
        .call_contract
        .expect("captured closure has a verified callable ABI");
    let contract = &lowered.contract().calls[callable.0 as usize];
    assert_eq!(contract.entries.len(), 1, "public argument only");
    assert_eq!(
        lowered.contract().functions[target_frame].environment.len(),
        1,
        "captured Int stays in the closure environment rather than the semantic ABI",
    );
    let capture = *closure
        .inputs
        .first()
        .expect("captured closure records its construction input");
    let capture_region = island
        .nodes
        .iter()
        .position(|node| node.id == capture)
        .expect("capture remains in the island");
    let capture_region = lowered.contract().functions[0].frame.regions[capture_region].offset;
    let direct = root
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::CallValue))
        .expect("direct closure call exists");
    let direct_pcs = pcs_for_node(
        lowered,
        0,
        NodeRef {
            function: root.id,
            node: direct.id,
        },
    );
    let direct_call = direct_pcs
        .iter()
        .filter_map(|pc| match &lowered.program().fns[0].code[*pc] {
            WeavyOp::CallIndirect { args, .. } => Some(args),
            _ => None,
        })
        .next()
        .expect("direct closure call lowers to CallIndirect");
    assert_eq!(
        direct_call.len(),
        1,
        "only the public argument crosses CallIndirect"
    );
    assert_ne!(capture_region, direct_call[0].src);
    assert!(lowered.program().fns.iter().all(|function| {
        function
            .code
            .iter()
            .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
    }));
    assert!(
        lowered.program().fns[0]
            .code
            .iter()
            .any(|op| matches!(op, WeavyOp::CallIndirect { .. }))
    );
    let recipe = lowered.recipe;
    let calls = lowered.contract().calls.clone();

    let shifted = Compiler::new()
        .compile(&format!("\n{RUNG_048}"))
        .expect("span-only rung 048 edit compiles");
    let shifted_partitioned = shifted.partition_test(&shifted.tests[0]);
    let shifted_lowered = cache
        .get_or_lower(&shifted_partitioned.islands[0])
        .expect("span-only rung 048 edit lowers");
    assert_eq!(recipe, shifted_lowered.recipe);
    assert_eq!(calls, shifted_lowered.contract().calls);

    let report = run_source(RUNG_048).expect("rung 048 runs through Executable");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 2);
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
    }
}

#[test]
fn noncapturing_direct_closure_has_only_its_semantic_argument() {
    const SOURCE: &str = r#"
#[test]
fn direct_inc() -> Stream<Check> {
    let inc = |n| n + 1;
    yield expect_eq(inc(1), 2);
}
"#;
    let module = Compiler::new()
        .compile(SOURCE)
        .expect("noncapturing closure compiles");
    let partitioned = module.partition_test(&module.tests[0]);
    let mut cache = LoweringCache::default();
    let lowered = cache
        .get_or_lower(&partitioned.islands[0])
        .expect("noncapturing direct closure verifies");
    let direct = module.functions[module.tests[0].function.0 as usize]
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::CallValue))
        .expect("direct call exists");
    let args = pcs_for_node(
        lowered,
        0,
        NodeRef {
            function: module.tests[0].function,
            node: direct.id,
        },
    )
    .into_iter()
    .filter_map(|pc| match &lowered.program().fns[0].code[pc] {
        WeavyOp::CallIndirect { args, .. } => Some(args),
        _ => None,
    })
    .next()
    .expect("direct call lowers indirectly");
    assert_eq!(args.len(), 1);
    let report = run_source(SOURCE).expect("noncapturing direct closure executes");
    assert!(report.passed() && report.agrees());
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
    }
}

#[test]
fn rung_049_plain_recursion_uses_stable_verified_call_abi() {
    let module = Compiler::new()
        .compile(RUNG_049)
        .expect("rung 049 compiles with recursive fib calls");
    let fib = module
        .functions
        .iter()
        .find(|function| function.name == "fib")
        .expect("rung 049 declares fib");
    assert!(
        fib.nodes
            .iter()
            .any(|node| matches!(node.op, VirOp::Call(callee) if callee == fib.id))
    );

    let partitioned = module.partition_test(&module.tests[0]);
    let island = &partitioned.islands[0];
    let mut cache = LoweringCache::default();
    let lowered = cache
        .get_or_lower(island)
        .expect("rung 049 lowers recursive fib through verified Weavy");
    let attribution = attribution_for(island);
    let fib_frame = frame_index(&attribution.functions, fib.id);
    let callable = lowered.contract().functions[fib_frame]
        .call_contract
        .expect("recursive fib has a verified callable ABI");
    let contract = &lowered.contract().calls[callable.0 as usize];
    assert_eq!(contract.entries.len(), 1, "fib ABI has one Int argument");
    assert!(
        lowered.program().fns[fib_frame]
            .code
            .iter()
            .any(|op| matches!(op, WeavyOp::Call { callee, .. } if callee.0 as usize == fib_frame))
    );
    assert!(lowered.program().fns.iter().all(|function| {
        function
            .code
            .iter()
            .all(|op| !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. }))
    }));
    let recipe = lowered.recipe;
    let calls = lowered.contract().calls.clone();

    let shifted = Compiler::new()
        .compile(&format!("\n{RUNG_049}"))
        .expect("span-only rung 049 edit compiles");
    let shifted_partitioned = shifted.partition_test(&shifted.tests[0]);
    let shifted_lowered = cache
        .get_or_lower(&shifted_partitioned.islands[0])
        .expect("span-only rung 049 edit lowers");
    assert_eq!(recipe, shifted_lowered.recipe);
    assert_eq!(calls, shifted_lowered.contract().calls);

    let report = run_source(RUNG_049).expect("rung 049 runs through Executable");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 1);
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
    }
}

#[test]
fn recursive_named_parameters_remain_call_arguments() {
    const SOURCE: &str = r#"
fn sum_to(n: Int) where { acc: Int, step: Int } -> Int {
    if n == 0 {
        acc
    } else {
        sum_to(n - 1) where { acc: acc + step, step }
    }
}

#[test]
fn recursive_named_arguments() -> Stream<Check> {
    yield expect_eq(sum_to(4) where { acc: 0, step: 3 }, 12);
}
"#;

    let module = Compiler::new()
        .compile(SOURCE)
        .expect("recursive named-parameter source compiles");
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "sum_to")
        .expect("sum_to function is present");
    assert_eq!(function.parameters.len(), 3);
    assert!(
        function
            .nodes
            .iter()
            .any(|node| matches!(node.op, VirOp::Call(callee) if callee == function.id))
    );

    let partitioned = module.partition_test(&module.tests[0]);
    let island = &partitioned.islands[0];
    let mut cache = LoweringCache::default();
    let lowered = cache
        .get_or_lower(island)
        .expect("recursive named parameters lower as ordinary call arguments");
    let attribution = attribution_for(island);
    let frame = frame_index(&attribution.functions, function.id);
    let function_contract = &lowered.contract().functions[frame];
    assert!(function_contract.environment.is_empty());
    let callable = function_contract
        .call_contract
        .expect("recursive function has a stable callable contract");
    assert_eq!(
        lowered.contract().calls[callable.0 as usize].entries.len(),
        3
    );

    let report = run_source(SOURCE).expect("recursive named arguments execute");
    assert!(report.passed() && report.agrees());
    assert_eq!(report.plain.checks.len(), 1);
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
    }
}

#[test]
fn rung_052_functions_are_first_class_arguments_and_results() {
    let module = Compiler::new()
        .compile(RUNG_052)
        .expect("rung 052 compiles: functions are first-class arguments and results");

    // `twice` takes a function and returns a function.
    let twice = module
        .functions
        .iter()
        .find(|function| function.name == "twice")
        .expect("rung 052 declares twice");
    let VirType::Function { parameter, result } = &twice.parameters[0].ty else {
        panic!("twice's parameter is a function value");
    };
    assert_eq!(parameter.as_ref(), &VirType::Int);
    assert_eq!(result.as_ref(), &VirType::Int);
    assert!(matches!(twice.return_type, VirType::Function { .. }));

    // The returned closure `|n| f(f(n))` captures the callable argument `f`,
    // read free from inside a call — the strong free-variable analysis records
    // it as the closure's construction input even though it never appears in a
    // bare binary/identifier position.
    let returned_closure = twice
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::Closure(_)))
        .expect("twice returns a closure value");
    let VirOp::Closure(returned_target) = returned_closure.op else {
        unreachable!("selected node is a closure")
    };
    assert_eq!(
        returned_closure.inputs.len(),
        1,
        "the returned closure captures exactly its callable argument f",
    );
    let target_fn = module
        .functions
        .iter()
        .find(|function| function.id == returned_target)
        .expect("closure target is a module function");
    // The capture is itself a callable value (fn(Int)->Int), so the closure's
    // environment transports a full callable, not merely a scalar.
    let capture_ty = &target_fn.parameters[1].ty;
    assert!(matches!(capture_ty, VirType::Function { .. }));

    // Direct invocation and Array.map both invoke closure values indirectly.
    assert!(
        module
            .functions
            .iter()
            .flat_map(|function| &function.nodes)
            .any(|node| matches!(node.op, VirOp::CallValue)),
        "higher-order values are invoked through CallValue",
    );

    let report = run_source(RUNG_052).expect("rung 052 runs through Executable");
    assert!(report.warnings.entries.is_empty());
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 2);
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert!(lane.checks.iter().all(|check| check.passed));
        assert_eq!(lane.counters.pure_host_calls, 0);
    }

    // Certificate: the boxed-environment closure executes natively through the
    // JIT by default and on the interpreter under WEAVY_JIT=0 — never a
    // per-program lane fallback — and both lanes produce identical checks.
    let expected_lane = if std::env::var("WEAVY_JIT").as_deref() == Ok("0")
        || !weavy::jit::task_lane::available()
    {
        vix::runtime::ExecutionLaneFact::Interpreter
    } else {
        vix::runtime::ExecutionLaneFact::Native
    };
    for lane in [&report.plain, &report.chaos] {
        let selections = lane
            .events
            .iter()
            .filter_map(|event| match event.kind {
                EventKind::ExecutionLane { facts, .. } => Some(facts.selected),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(!selections.is_empty(), "rung 052 records an execution lane");
        assert!(
            selections.iter().all(|selected| *selected == expected_lane),
            "rung 052 selects {expected_lane:?}, got {selections:?}",
        );
    }
}

#[test]
fn rung_050_deep_tail_recursion_runs_under_its_declared_budget() {
    let outcome = run_source_under_declared_budget(
        Path::new(env!("CARGO_BIN_EXE_vix-budget-child")),
        RUNG_050,
    );
    assert!(
        matches!(
            &outcome,
            BudgetOutcome::Within {
                report: ChildReport::RanSource { passed: true, .. },
            }
        ),
        "unexpected budget outcome: {outcome:?}"
    );
}

/// The ordinary ratchet path is production execution, not an innards
/// diagnostic capture. The self-tail pollpoint remains in lowered code for
/// attribution, but must not retain one `WeavyMark` per iteration.
#[test]
fn rung_050_ratchet_execution_strips_per_iteration_marks() {
    let report = run_source(RUNG_050).expect("rung 050 runs through the production ratchet");
    for lane in [&report.plain, &report.chaos] {
        let marks = lane
            .events
            .iter()
            .filter(|event| matches!(event.kind, EventKind::WeavyMark { .. }))
            .count();
        assert_eq!(
            marks, 0,
            "production ratchet retains no per-iteration innards marks: {marks}",
        );
    }
}

/// Rung 138 — the map twin of rung 051: a 200k-element fold that seeds an
/// empty `%{}` map and appends `(i.to_string(), i)` per element. This is the
/// canonical bidirectional-typing rung: the accumulator type `Map<String, Int>`
/// is inferred from the closure body's `+` dispatch over an empty map seed,
/// with no external annotation. The fold runs through the production verified
/// interpreter and native JIT lanes, and the `store_interns_at_most(10)`
/// trace check (the fixture's own counter) proves the persistent ordered map
/// does not intern per update — each `+` is one in-frame AVL insert, not a
/// store allocation.
#[test]
fn rung_138_map_accumulator_runs_through_production_path() {
    let report = run_source(RUNG_138).expect("rung 138 runs through the production ratchet");
    assert!(report.agrees(), "plain and chaos agree: {report:?}");
    assert!(report.passed(), "every rung 138 check passes: {report:?}");
    assert_eq!(
        report.plain.checks.len(),
        3,
        "rung 138 publishes exactly three checks: {report:?}"
    );
    // The fixture's own `store_interns_at_most(10)` trace check enforces the
    // no-per-update-interning invariant in-run; the counter is also observable
    // directly and stays well below the 200k element count.
    assert!(
        report.plain.counters.store_interns <= 10,
        "map fold interns at most once on publication: {}",
        report.plain.counters.store_interns
    );
}

// ---------------------------------------------------------------------------
// Rung 138 — adversarial bidirectional-typing certificates.
//
// The canonical rung above proves the 200k-element map fold. These
// certificates exercise the general accumulator inference over nested and
// generic collection seeds, empty-set folds, and the copy path for a fold
// whose body is not the strict append grain (which must keep its diagnostic).
// ---------------------------------------------------------------------------

/// An empty set seed `#{}` infers `Set<T>` from the closure body's `+`
/// dispatch: `acc + i` with `i : Int` fixes `A = Set<Int>`.
#[test]
fn rung_138_empty_set_seed_infers_accumulator_from_body() {
    const SOURCE: &str = r#"
#[test]
fn set_accumulator() -> Stream<Check> {
    let n = 1000;
    let s = (range where { from: 0, to: n }).fold(%[], |acc, i| acc + i);
    yield expect_eq(s.len(), n);
    yield expect_eq(s.has(500), true);
    yield expect_eq(s.has(1000), false);
}
"#;
    let report = run_source(SOURCE).expect("set-seed fold runs");
    assert!(report.agrees(), "plain and chaos agree: {report:?}");
    assert!(report.passed(), "set-seed fold checks pass: {report:?}");
    assert_eq!(report.plain.checks.len(), 3);
}

/// A nested map fold: `%{}` seed whose appended value is itself a map built by
/// an inner fold over a literal array. The accumulator type
/// `Map<String, Map<String, Int>>` is inferred from the `(k, inner_map)` tuple
/// the body appends, where `inner_map` is itself an empty-seed fold resolved by
/// the same inference. The inner fold's appended expression captures only its
/// own element parameter (no enclosing binding), so both inferences are
/// self-contained.
#[test]
fn rung_138_nested_map_fold_infers_nested_accumulator() {
    const SOURCE: &str = r#"
#[test]
fn nested_map_accumulator() -> Stream<Check> {
    let outer = [10, 20, 30];
    let m = outer.fold(%{}, |acc, k| acc + (k.to_string(), [1, 2, 3].fold(%{}, |inner, v| inner + (v.to_string(), v))));
    yield expect_eq(m.len(), 3);
    yield expect_eq(m.get("20").get("3"), 3);
    yield expect_eq(m.get("30").len(), 3);
}
"#;
    let report = run_source(SOURCE).expect("nested map fold runs");
    assert!(report.agrees(), "plain and chaos agree: {report:?}");
    assert!(report.passed(), "nested map fold checks pass: {report:?}");
    assert_eq!(report.plain.checks.len(), 3);
}
/// A generic-typed map fold: the value type is a tuple `(Int, String)`,
/// exercising inference where `EXPR : (K, (V1, V2))` fixes
/// `A = Map<String, (Int, String)>`.
#[test]
fn rung_138_map_fold_with_tuple_value_type() {
    const SOURCE: &str = r#"
#[test]
fn tuple_value_map() -> Stream<Check> {
    let xs = [1, 2, 3];
    let m = xs.fold(%{}, |acc, i| acc + (i.to_string(), (i, i.to_string())));
    yield expect_eq(m.len(), 3);
    yield expect_eq(m.get("2"), (2, "2"));
}
"#;
    let report = run_source(SOURCE).expect("tuple-value map fold runs");
    assert!(report.agrees(), "plain and chaos agree: {report:?}");
    assert!(
        report.passed(),
        "tuple-value map fold checks pass: {report:?}"
    );
    assert_eq!(report.plain.checks.len(), 2);
}

/// An external expected type flows through the fold: `let m: Map<String, Int>`
/// annotates the accumulator, so the empty seed is typed directly without
/// body inference. This is the other arm of bidirectional typing.
#[test]
fn rung_138_external_expected_type_flows_into_empty_seed() {
    const SOURCE: &str = r#"
#[test]
fn annotated_map_fold() -> Stream<Check> {
    let m: Map<String, Int> = [1, 2, 3].fold(%{}, |acc, i| acc + (i.to_string(), i));
    yield expect_eq(m.len(), 3);
    yield expect_eq(m.get("2"), 2);
}
"#;
    let report = run_source(SOURCE).expect("annotated map fold runs");
    assert!(report.agrees(), "plain and chaos agree: {report:?}");
    assert!(
        report.passed(),
        "annotated map fold checks pass: {report:?}"
    );
    assert_eq!(report.plain.checks.len(), 2);
}

/// A genuinely ambiguous empty literal — an empty map seed whose closure body
/// is NOT the `acc + EXPR` append grain — keeps the existing diagnostic rather
/// than silently guessing. The inference returns `None` and the copy path owns
/// the empty-map-literal error.
#[test]
fn rung_138_ambiguous_empty_seed_without_append_grain_stays_red() {
    const SOURCE: &str = r#"
#[test]
fn ambiguous_seed() -> Stream<Check> {
    let m = [1, 2, 3].fold(%{}, |acc, i| i);
    yield expect_eq(m.len(), 0);
}
"#;
    let RunError::Diagnostics(diagnostics) = run_source(SOURCE).expect_err("ambiguous seed is red")
    else {
        panic!("an empty map seed without the append grain stays a typed red boundary");
    };
    assert_eq!(diagnostics.entries.len(), 1);
    assert_eq!(
        diagnostics.entries[0].message(),
        "an empty map literal needs an expected key and value type"
    );
}

#[test]
// r[verify lang.collection.array-map]
fn array_map_vir_wiring_infers_both_sides_of_the_callable_signature() {
    const SOURCE: &str = r#"
#[test]
fn typed_map() -> Stream<Check> {
    let xs = [true, false, true];
    let ys = xs.map(|selected| if selected { 7 } else { 11 });
    yield expect_eq(ys[1], 11);
}
"#;
    let module = Compiler::new()
        .compile(SOURCE)
        .expect("array map source checks and lowers to VIR");
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "typed_map")
        .expect("test function exists");
    let map = function
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::ArrayMap { .. }))
        .expect("method call becomes ArrayMap VIR");
    let VirOp::ArrayMap { grain } = map.op else {
        unreachable!("selected ArrayMap node")
    };
    assert_eq!(grain.key, ArrayMapGrainKey::InputPosition);
    assert_eq!(grain.origin, ArrayMapGrainKey::InputPosition);
    assert_eq!(map.ty, VirType::array(VirType::Int));
    assert_eq!(map.effect.kind, EffectKind::Pure);
    assert!(!map.effect.fallible);
    assert!(!map.effect.placed);
    let [array_id, closure_id] = map.inputs.as_slice() else {
        panic!("ArrayMap has array and typed callable inputs")
    };
    assert!(matches!(
        function.nodes[array_id.0 as usize].op,
        VirOp::Array
    ));
    let VirOp::Closure(closure_function) = function.nodes[closure_id.0 as usize].op else {
        panic!("ArrayMap callable input is the generated closure value")
    };
    let closure = &module.functions[closure_function.0 as usize];
    assert_eq!(closure.parameters.len(), 1);
    assert_eq!(closure.parameters[0].ty, VirType::Bool);
    assert_eq!(closure.return_type, VirType::Int);
    assert_eq!(
        function.nodes[closure_id.0 as usize].ty,
        (VirType::Function {
            parameter: Box::new(VirType::Bool),
            result: Box::new(VirType::Int),
        })
    );

    let partitioned = module.partition_test(&module.tests[0]);
    assert_eq!(partitioned.islands[0].array_map_partitions.len(), 1);
    assert_eq!(
        partitioned.islands[0].array_map_partitions[0].shape,
        ArrayMapExecutionShape::FusedProjection,
    );
    assert!(partitioned.render().contains("FusedProjection"));
    let recipe = partitioned.islands[0].canonical_recipe_bytes();
    let mut alternative_partition = partitioned.islands[0].clone();
    alternative_partition.array_map_partitions[0].shape = ArrayMapExecutionShape::MaterializedLoop;
    assert_eq!(
        alternative_partition.canonical_recipe_bytes(),
        recipe,
        "partition shape is not semantic recipe identity",
    );

    let shifted = Compiler::new()
        .compile(&format!("\n\n{SOURCE}"))
        .expect("span-only edit compiles");
    assert_eq!(
        module.partition_test(&module.tests[0]).islands[0].canonical_recipe_bytes(),
        shifted.partition_test(&shifted.tests[0]).islands[0].canonical_recipe_bytes(),
        "ArrayMap canonical encoding excludes source offsets",
    );
}

#[test]
// r[verify lang.collection.array-map]
// r[verify machine.island.partition]
fn array_map_materializes_runtime_length_across_a_typed_call_boundary() {
    const SOURCE: &str = r#"
struct Pair { left: Int, right: Int }

fn mapped_pairs(xs: [Int]) -> [Pair] {
    xs.map(|n| Pair { left: n * n + 3, right: n * 2 - 1 })
}

#[test]
fn general_array_map() -> Stream<Check> {
    let base = 4;
    let xs = [base - 3, base - 2, base - 1, base, base + 1];
    let ys = mapped_pairs(xs);
    let index = base - 2;
    yield expect_eq(ys[index].left, 12);
    yield expect_eq(ys[index].right, 5);
    yield expect_eq(ys.len(), 5);
}
"#;
    let module = Compiler::new()
        .compile(SOURCE)
        .expect("general array map compiles to VIR");
    let helper = module
        .functions
        .iter()
        .find(|function| function.name == "mapped_pairs")
        .expect("mapping helper exists");
    let map = helper
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::ArrayMap { .. }))
        .expect("helper contains ArrayMap VIR");
    let [_, closure_node] = map.inputs.as_slice() else {
        panic!("ArrayMap has source and callable")
    };
    let VirOp::Closure(mapper) = helper.nodes[closure_node.0 as usize].op else {
        panic!("helper map uses a generated callable")
    };

    let partitioned = module.partition_test(&module.tests[0]);
    let island = partitioned
        .values
        .iter()
        .map(|value| &value.island)
        .chain(partitioned.islands.iter())
        .find(|island| {
            island.array_map_partitions.iter().any(|decision| {
                decision.node
                    == (NodeRef {
                        function: helper.id,
                        node: map.id,
                    })
            })
        })
        .expect("helper ArrayMap belongs to one partitioned island");
    let decision = island
        .array_map_partitions
        .iter()
        .find(|decision| {
            decision.node
                == (NodeRef {
                    function: helper.id,
                    node: map.id,
                })
        })
        .expect("helper ArrayMap has a partition decision");
    assert_eq!(decision.shape, ArrayMapExecutionShape::MaterializedLoop);
    assert!(partitioned.render().contains("MaterializedLoop"));

    let mut lowering_cache = LoweringCache::default();
    let lowered = lowering_cache
        .get_or_lower(island)
        .expect("runtime-length ArrayMap verifies before execution");
    assert_pc_maps_complete(lowered);
    let attribution = attribution_for(island);
    let helper_frame = frame_index(&attribution.functions, helper.id);
    let mapper_frame = frame_index(&attribution.functions, mapper);
    let helper_ops = &lowered.program().fns[helper_frame].code;
    assert!(
        helper_ops
            .iter()
            .any(|op| matches!(op, WeavyOp::LoadArrayLen { .. }))
    );
    assert!(
        helper_ops
            .iter()
            .any(|op| matches!(op, WeavyOp::ArrayNew { .. }))
    );
    assert!(
        helper_ops
            .iter()
            .any(|op| matches!(op, WeavyOp::LoadArray { .. }))
    );
    assert!(
        helper_ops
            .iter()
            .any(|op| matches!(op, WeavyOp::CallIndirect { .. }))
    );
    assert!(
        helper_ops
            .iter()
            .any(|op| matches!(op, WeavyOp::ArrayStore { .. }))
    );
    assert!(helper_ops.iter().enumerate().any(|(pc, op)| {
        matches!(op, WeavyOp::Jump { target } if usize::try_from(*target).is_ok_and(|target| target < pc))
    }));
    let contract = lowered.contract();
    let mapper_contract = &contract.functions[mapper_frame];
    let callable = mapper_contract
        .call_contract
        .expect("mapper has an exact callable ABI");
    assert_eq!(
        contract.calls[callable.0 as usize].result.shape,
        mapper_contract.frame.regions[mapper_contract.result.0 as usize].shape,
        "CallIndirect returns the same hidden typed outcome as the mapper function",
    );

    let report = run_source(SOURCE).expect("general ArrayMap runs through Executable");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 3);
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
        let published_schemas = lane
            .values
            .iter()
            .map(|value| value.identity.schema.clone())
            .collect::<BTreeSet<_>>();
        assert!(lane.events.iter().all(|event| match &event.kind {
            EventKind::StoreAlloc { identity, .. } => {
                identity.schema == VirType::Check.schema_ref()
                    || published_schemas.contains(&identity.schema)
            }
            _ => true,
        }));
    }
}

#[test]
fn array_bearing_islands_propagate_non_map_indirect_call_outcomes() {
    const SOURCE: &str = r#"
#[test]
fn array_and_call_value() -> Stream<Check> {
    let xs = [1, 2, 3];
    let transform = |n: Int| n * 3 + 1;
    let transformed = transform(xs.len());
    yield expect_eq(transformed, 10);
}
"#;
    let module = Compiler::new()
        .compile(SOURCE)
        .expect("array-bearing indirect call compiles");
    let root = &module.functions[module.tests[0].function.0 as usize];
    let call = root
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::CallValue))
        .expect("source contains a non-map CallValue");
    let VirOp::Closure(target) = root.nodes[call.inputs[0].0 as usize].op else {
        panic!("CallValue callee is a generated closure")
    };
    let partitioned = module.partition_test(&module.tests[0]);
    let island = &partitioned.islands[0];
    let mut cache = LoweringCache::default();
    let lowered = cache
        .get_or_lower(island)
        .expect("array-bearing CallValue verifies with hidden outcomes");
    let attribution = attribution_for(island);
    let target_frame = frame_index(&attribution.functions, target);
    let target_contract = &lowered.contract().functions[target_frame];
    let callable = target_contract
        .call_contract
        .expect("non-map closure has an exact callable ABI");
    assert_eq!(
        lowered.contract().calls[callable.0 as usize].result.shape,
        target_contract.frame.regions[target_contract.result.0 as usize].shape,
    );
    let call_pcs = pcs_for_node(
        lowered,
        0,
        NodeRef {
            function: root.id,
            node: call.id,
        },
    );
    assert!(call_pcs.iter().any(|pc| matches!(
        lowered.program().fns[0].code[*pc],
        WeavyOp::CallIndirect { .. }
    )));
    assert!(call_pcs.iter().any(|pc| matches!(
        lowered.program().fns[0].code[*pc],
        WeavyOp::EnumProjectChecked { variant: 0, .. }
    )));

    let report = run_source(SOURCE).expect("array-bearing CallValue executes");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 1);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
    assert_eq!(report.chaos.counters.pure_host_calls, 0);
}

#[test]
fn repeated_map_projection_materializes_once_inside_the_island() {
    const SOURCE: &str = r#"
#[test]
fn repeated_projection() -> Stream<Check> {
    let ys = [2, 3, 4].map(|n| n * n);
    let index = 1;
    yield expect_eq(ys[index] + ys[index], 18);
}
"#;
    let module = Compiler::new()
        .compile(SOURCE)
        .expect("repeated projection source compiles");
    let partitioned = module.partition_test(&module.tests[0]);
    let island = &partitioned.islands[0];
    assert_eq!(island.array_map_partitions.len(), 1);
    assert_eq!(
        island.array_map_partitions[0].shape,
        ArrayMapExecutionShape::MaterializedLoop,
    );
    let mut cache = LoweringCache::default();
    let lowered = cache
        .get_or_lower(island)
        .expect("shared mapped result verifies");
    assert_eq!(
        lowered.program().fns[0]
            .code
            .iter()
            .filter(|op| matches!(op, WeavyOp::CallIndirect { .. }))
            .count(),
        1,
        "one mapper call site lives in the materialization loop",
    );
    let report = run_source(SOURCE).expect("shared mapped result executes");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 1);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
    assert_eq!(report.chaos.counters.pure_host_calls, 0);
}

#[test]
// r[verify lang.collection.array-index]
// r[verify machine.error.index-out-of-bounds]
fn array_index_out_of_bounds_is_a_memoized_language_failure() {
    let source = r#"
#[test]
fn oob() -> Stream<Check> {
    let xs = [10, 20];
    yield expect_eq(xs[7], 0);
}
"#;
    let report = run_source(source).expect("language failure is report data");
    for lane in [&report.plain, &report.chaos] {
        let [check] = lane.checks.as_slice() else {
            panic!("one OOB check")
        };
        assert!(!check.passed);
        assert!(matches!(
            check.failure,
            Some(vix::runtime::FailureValue::IndexOutOfBounds {
                index: 7,
                length: 2,
                subject: None,
                ..
            })
        ));
        assert_eq!(lane.receipt_count, 0);
        assert_eq!(lane.counters.pure_host_calls, 0);
    }
    assert_eq!(report.plain.checks, report.chaos.checks);
}

#[test]
fn array_vir_wiring_and_dynamic_index_run_through_verified_execution() {
    const SOURCE: &str = r#"
fn double(x: Int) -> Int {
    x * 2
}

#[test]
fn computed_array_and_dynamic_index() -> Stream<Check> {
    let n = 5;
    let xs = [double(n), n + 1, double(4)];
    let index = n - 4;
    yield expect_eq(xs[index], 6);
    yield expect_eq(xs.len(), 3);
}
"#;

    let module = Compiler::new()
        .compile(SOURCE)
        .expect("array source checks and lowers to VIR");
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "computed_array_and_dynamic_index")
        .expect("test function exists");
    let double = module
        .functions
        .iter()
        .find(|function| function.name == "double")
        .expect("double function exists");
    let array = function
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::Array))
        .expect("array literal becomes an Array VIR node");
    assert_eq!(array.ty, VirType::array(VirType::Int));
    assert_eq!(array.ty.word_width(), Some(1));
    assert_eq!(array.inputs.len(), 3);
    assert_eq!(array.effect.kind, EffectKind::Pure);
    assert!(!array.effect.fallible);
    assert!(!array.effect.placed);
    let [first_id, middle_id, third_id] = array.inputs.as_slice() else {
        panic!("array literal has exactly three authored inputs");
    };
    let first = &function.nodes[first_id.0 as usize];
    let middle = &function.nodes[middle_id.0 as usize];
    let third = &function.nodes[third_id.0 as usize];
    assert!(matches!(first.op, VirOp::Call(callee) if callee == double.id));
    assert_eq!(first.inputs.len(), 1);
    assert!(matches!(
        function.nodes[first.inputs[0].0 as usize].op,
        VirOp::Int(5)
    ));
    assert!(matches!(middle.op, VirOp::Add));
    assert_eq!(middle.inputs.len(), 2);
    assert!(matches!(
        function.nodes[middle.inputs[0].0 as usize].op,
        VirOp::Int(5)
    ));
    assert!(matches!(
        function.nodes[middle.inputs[1].0 as usize].op,
        VirOp::Int(1)
    ));
    assert!(matches!(third.op, VirOp::Call(callee) if callee == double.id));
    assert_eq!(third.inputs.len(), 1);
    assert!(matches!(
        function.nodes[third.inputs[0].0 as usize].op,
        VirOp::Int(4)
    ));

    let index = function
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::ArrayIndex))
        .expect("dynamic indexing becomes an ArrayIndex VIR node");
    let dynamic_sub = function
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::Sub))
        .expect("dynamic index expression becomes a Sub VIR node");
    assert_eq!(index.ty, VirType::Int);
    assert_eq!(index.inputs, [array.id, dynamic_sub.id]);
    assert_eq!(index.effect.kind, EffectKind::Pure);
    assert!(index.effect.fallible);
    assert!(!index.effect.placed);
    assert_eq!(dynamic_sub.inputs.len(), 2);
    assert!(matches!(
        function.nodes[dynamic_sub.inputs[0].0 as usize].op,
        VirOp::Int(5)
    ));
    assert!(matches!(
        function.nodes[dynamic_sub.inputs[1].0 as usize].op,
        VirOp::Int(4)
    ));
    assert_eq!(
        &SOURCE[index.span.start as usize..index.span.end as usize],
        "xs[index]"
    );

    let array_lengths = function
        .nodes
        .iter()
        .filter(|node| matches!(node.op, VirOp::ArrayLen))
        .collect::<Vec<_>>();
    assert_eq!(array_lengths.len(), 1);
    let array_length = array_lengths[0];
    assert_eq!(array_length.ty, VirType::Int);
    assert_eq!(array_length.inputs, [array.id]);
    assert_eq!(array_length.effect.kind, EffectKind::Pure);
    assert!(!array_length.effect.fallible);
    assert!(!array_length.effect.placed);
    assert_eq!(
        &SOURCE[array_length.span.start as usize..array_length.span.end as usize],
        "xs.len()"
    );

    let report = run_source(SOURCE).expect("computed arrays execute through the verified runtime");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 2);
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert!(lane.checks.iter().all(|check| check.passed));
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
        let published_schemas = lane
            .values
            .iter()
            .map(|value| value.identity.schema.clone())
            .collect::<BTreeSet<_>>();
        assert!(lane.events.iter().all(|event| match &event.kind {
            EventKind::StoreAlloc { identity, .. } => {
                identity.schema == VirType::Check.schema_ref()
                    || published_schemas.contains(&identity.schema)
            }
            _ => true,
        }));
    }

    let diagnostics = Compiler::new()
        .compile("#[test] fn empty() -> Stream<Check> { let xs = []; yield expect(true); }")
        .expect_err("empty arrays remain untyped without a source type context");
    assert_eq!(diagnostics.entries.len(), 1);
    assert_eq!(
        diagnostics.entries[0].code,
        DiagnosticCode::UnsupportedExpression
    );
    assert_eq!(
        diagnostics.entries[0].payload,
        vix::diagnostic::DiagnosticPayload::Unsupported {
            construct: "an empty array literal needs an expected element type".to_owned(),
        }
    );
}

#[test]
fn collection_addition_has_distinct_typed_array_grains() {
    const SOURCE: &str = r#"
#[test]
fn collection_addition() -> Stream<Check> {
    let xs = [1, 2];
    let ys = xs + 3;
    let zs = ys ++ [4, 5];
    yield expect_eq(xs.len(), 2);
    yield expect_eq(xs[1], 2);
    yield expect_eq(ys[2], 3);
    yield expect_eq(zs[0], 1);
    yield expect_eq(zs[2], 3);
    yield expect_eq(zs[4], 5);
    yield expect_eq(zs.len(), 5);
}

#[test]
fn structural_collection_addition() -> Stream<Check> {
    let empty: [(Int, Bool)] = [];
    let xs = empty + (1, true);
    let ys = xs + (2, false);
    let zs = ys ++ [(3, true)];
    yield expect_eq(empty.len(), 0);
    yield expect_eq(xs[0], (1, true));
    yield expect_eq(ys[1], (2, false));
    yield expect_eq(zs[2], (3, true));
}
"#;

    let module = Compiler::new()
        .compile(SOURCE)
        .expect("array + and ++ compile to VIR");
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "collection_addition")
        .expect("test function exists");
    let append = function
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::ArrayAppend))
        .expect("one-item + becomes ArrayAppend");
    let concat = function
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::ArrayConcat))
        .expect("whole-collection ++ becomes ArrayConcat");
    let [append_array, appended_element] = append.inputs.as_slice() else {
        panic!("ArrayAppend has one array and one element input");
    };
    let [concat_left, concat_right] = concat.inputs.as_slice() else {
        panic!("ArrayConcat has two array inputs");
    };

    assert!(matches!(
        function.nodes[append_array.0 as usize].op,
        VirOp::Array
    ));
    assert!(matches!(
        function.nodes[appended_element.0 as usize].op,
        VirOp::Int(3)
    ));
    assert_eq!(*concat_left, append.id);
    assert!(matches!(
        function.nodes[concat_right.0 as usize].op,
        VirOp::Array
    ));
    for node in [append, concat] {
        assert_eq!(node.ty, VirType::array(VirType::Int));
        assert_eq!(node.effect.kind, EffectKind::Pure);
        assert!(!node.effect.fallible);
        assert!(!node.effect.placed);
    }
    assert_eq!(
        &SOURCE[append.span.start as usize..append.span.end as usize],
        "xs + 3"
    );
    assert_eq!(
        &SOURCE[concat.span.start as usize..concat.span.end as usize],
        "ys ++ [4, 5]"
    );

    let report = run_source(SOURCE).expect("array + and ++ run through verified execution");
    assert!(report.warnings.entries.is_empty());
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 11);
    for lane in [&report.plain, &report.chaos] {
        assert!(lane.checks.iter().all(|check| check.passed));
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
    }
}

#[test]
// r[verify lang.diagnostic.must-use]
fn unused_collection_result_is_a_typed_warning() {
    let (expected_message, expected_line) = warning_header(RUNG_144);
    let compilation = Compiler::new()
        .compile(RUNG_144)
        .expect("warning rungs remain valid programs");

    assert_eq!(compilation.warnings.entries.len(), 1);
    let warning = &compilation.warnings.entries[0];
    assert_eq!(warning.code, DiagnosticCode::UnusedMustUse);
    assert_eq!(warning.code.severity(), DiagnosticSeverity::Warning);
    assert_eq!(warning.message(), expected_message);
    assert_eq!(source_line(RUNG_144, warning.primary.start), expected_line);
    assert_eq!(
        &RUNG_144[warning.primary.start as usize..warning.primary.end as usize],
        "xs + 4"
    );
    assert_eq!(compilation.module.functions.len(), 1);

    const ALL_MARKERS: &str = r#"
struct Inputs {
    xs: [Int],
    map: Map<String, Int>,
    set: Set<Int>,
}

fn unused_collection_results(inputs: Inputs) -> Int {
    let array_all = inputs.xs ++ inputs.xs;
    let map_one = inputs.map + ("x", 1);
    let map_all = inputs.map ++ inputs.map;
    let rebound = inputs.map.with ("x", 1);
    let set_one = inputs.set + 1;
    let set_all = inputs.set ++ inputs.set;
    0
}
"#;
    let compilation = Compiler::new()
        .compile(ALL_MARKERS)
        .expect("must-use markers do not reject the program");
    assert_eq!(
        compilation
            .warnings
            .entries
            .iter()
            .map(|warning| warning.message())
            .collect::<Vec<_>>(),
        [
            "unused result of `++`",
            "unused result of `+`",
            "unused result of `++`",
            "unused result of `with`",
            "unused result of `+`",
            "unused result of `++`",
        ]
    );
    assert!(
        compilation
            .warnings
            .entries
            .iter()
            .all(|warning| warning.code.severity() == DiagnosticSeverity::Warning)
    );
}

#[test]
fn map_and_set_surface_has_distinct_typed_vir_grains() {
    let map_compilation = Compiler::new()
        .compile(RUNG_041)
        .expect("map surface compiles to VIR");
    assert!(map_compilation.warnings.entries.is_empty());
    let maps = map_compilation
        .functions
        .iter()
        .find(|function| function.name == "maps")
        .expect("rung 041 contains maps");
    assert_eq!(
        maps.nodes
            .iter()
            .filter(|node| matches!(node.op, VirOp::Map))
            .count(),
        1
    );
    assert_eq!(
        maps.nodes
            .iter()
            .filter(|node| matches!(node.op, VirOp::MapAdd))
            .count(),
        2
    );
    assert_eq!(
        maps.nodes
            .iter()
            .filter(|node| matches!(node.op, VirOp::MapGet))
            .count(),
        1
    );
    assert_eq!(
        maps.nodes
            .iter()
            .filter(|node| matches!(node.op, VirOp::MapHas))
            .count(),
        2
    );
    for node in maps
        .nodes
        .iter()
        .filter(|node| matches!(node.op, VirOp::Map | VirOp::MapAdd))
    {
        assert_eq!(node.ty, VirType::map(VirType::String, VirType::Int));
    }
    assert!(
        maps.nodes
            .iter()
            .filter(|node| matches!(node.op, VirOp::MapAdd | VirOp::MapGet))
            .all(|node| node.effect.fallible)
    );
    assert!(
        maps.nodes
            .iter()
            .filter(|node| matches!(node.op, VirOp::MapHas | VirOp::MapLen))
            .all(|node| !node.effect.fallible && node.effect.kind == EffectKind::Pure)
    );

    let overwrite = Compiler::new()
        .compile(RUNG_042)
        .expect("map with compiles to VIR");
    let overwrite = overwrite
        .functions
        .iter()
        .find(|function| function.name == "map_overwrite")
        .expect("rung 042 contains map_overwrite");
    let with = overwrite
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::MapWith))
        .expect("with has its own VIR grain");
    assert_eq!(with.inputs.len(), 3);
    assert!(!with.effect.fallible);
    assert_eq!(
        &RUNG_042[with.span.start as usize..with.span.end as usize],
        "m.with (\"k\", 2)"
    );

    let keys = Compiler::new()
        .compile(RUNG_043)
        .expect("map concatenation and keys compile to VIR");
    let keys = keys
        .functions
        .iter()
        .find(|function| function.name == "map_keys_canonical")
        .expect("rung 043 contains map_keys_canonical");
    assert!(
        keys.nodes
            .iter()
            .any(|node| matches!(node.op, VirOp::MapConcat) && node.effect.fallible)
    );
    assert!(keys.nodes.iter().any(|node| {
        matches!(node.op, VirOp::MapKeys)
            && node.ty == VirType::array(VirType::String)
            && !node.effect.fallible
    }));

    let sets = Compiler::new()
        .compile(RUNG_044)
        .expect("set surface compiles to VIR");
    let sets = sets
        .functions
        .iter()
        .find(|function| function.name == "sets")
        .expect("rung 044 contains sets");
    for expected in [
        VirOp::Set,
        VirOp::SetAdd,
        VirOp::SetConcat,
        VirOp::SetHas,
        VirOp::SetLen,
        VirOp::SetValues,
    ] {
        assert!(
            sets.nodes
                .iter()
                .any(|node| core::mem::discriminant(&node.op) == core::mem::discriminant(&expected)),
            "set surface emits {expected:?}",
        );
    }
    assert!(sets.nodes.iter().any(|node| {
        matches!(node.op, VirOp::SetValues)
            && node.ty == VirType::array(VirType::Int)
            && !node.effect.fallible
    }));

    for source in [RUNG_041, RUNG_042, RUNG_043, RUNG_044] {
        let report = run_source(source).expect("ordered collection publishes through production");
        assert!(report.passed());
        assert!(report.agrees());
    }
}

#[test]
fn map_failures_are_typed_attributed_and_replay_stable() {
    const MISSING: &str = r#"
fn required_value(m: Map<String, Int>) -> Int {
    m.get("missing")
}

#[test]
fn missing_key() -> Stream<Check> {
    let m: Map<String, Int> = %{};
    let value = required_value(m);
    yield expect_eq(value, 0);
}
"#;
    const DUPLICATES: &str = r#"
fn duplicate_row(m: Map<String, Int>) -> Map<String, Int> {
    m + ("k", 2)
}

fn overlapping_maps() -> Map<String, Int> {
    %{"a" => 1} ++ %{"a" => 2}
}

#[test]
fn duplicate_row_check() -> Stream<Check> {
    let collision = duplicate_row(%{"k" => 1});
    yield expect_eq(collision.len(), 0);
}

#[test]
fn overlapping_maps_check() -> Stream<Check> {
    let collision = overlapping_maps();
    yield expect_eq(collision.len(), 0);
}
"#;

    let missing = run_source(MISSING).expect("missing Map key is a language failure report");
    assert_eq!(missing.plain.checks, missing.chaos.checks);
    for lane in [&missing.plain, &missing.chaos] {
        let [check] = lane.checks.as_slice() else {
            panic!("one missing-key check")
        };
        assert!(!check.passed);
        assert!(matches!(
            check.failure,
            Some(vix::runtime::FailureValue::MissingKey { .. })
        ));
        let context = check
            .failure_context
            .as_ref()
            .expect("missing key resolves through current source attribution");
        assert_eq!(
            &MISSING[context.span.start as usize..context.span.end as usize],
            "m.get(\"missing\")"
        );
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
    }

    let duplicates =
        run_source(DUPLICATES).expect("duplicate Map keys are language failure reports");
    assert_eq!(duplicates.plain.checks, duplicates.chaos.checks);
    for lane in [&duplicates.plain, &duplicates.chaos] {
        let [row, merge] = lane.checks.as_slice() else {
            panic!("one row collision and one merge collision")
        };
        for (check, operation) in [
            (row, "m + (\"k\", 2)"),
            (merge, "%{\"a\" => 1} ++ %{\"a\" => 2}"),
        ] {
            assert!(!check.passed);
            assert!(matches!(
                check.failure,
                Some(vix::runtime::FailureValue::DuplicateKey { .. })
            ));
            let context = check
                .failure_context
                .as_ref()
                .expect("duplicate key resolves through current source attribution");
            assert_eq!(
                &DUPLICATES[context.span.start as usize..context.span.end as usize],
                operation
            );
        }
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
    }
}

#[test]
fn string_failures_are_typed_attributed_and_replay_stable() {
    for (source, operation, expected) in [
        (
            r#"#[test]
fn missing() -> Stream<Check> { let value = "a".split_once("/"); yield expect_eq(value.0, ""); }"#,
            "\"a\".split_once(\"/\")",
            "MissingDelimiter",
        ),
        (
            r#"#[test]
fn invalid() -> Stream<Check> { let value = "nope".parse_int(); yield expect_eq(value, 0); }"#,
            "\"nope\".parse_int()",
            "InvalidInteger",
        ),
        (
            r#"#[test]
fn overflow() -> Stream<Check> { let value = "9223372036854775808".parse_int(); yield expect_eq(value, 0); }"#,
            "\"9223372036854775808\".parse_int()",
            "IntegerOverflow",
        ),
    ] {
        let report = run_source(source).expect("string failure becomes a production report");
        assert_eq!(report.plain.checks, report.chaos.checks);
        for lane in [&report.plain, &report.chaos] {
            let [check] = lane.checks.as_slice() else {
                panic!("one failed check")
            };
            assert!(!check.passed);
            assert!(
                format!("{:?}", check.failure.as_ref().expect("typed failure"))
                    .starts_with(expected)
            );
            let context = check
                .failure_context
                .as_ref()
                .expect("operation source attribution");
            assert_eq!(
                &source[context.span.start as usize..context.span.end as usize],
                operation
            );
            assert_eq!(lane.counters.pure_host_calls, 0);
            assert_eq!(lane.receipt_count, 0);
        }
    }
}

#[test]
fn map_values_follow_canonical_key_order() {
    const SOURCE: &str = r#"
#[test]
fn map_values() -> Stream<Check> {
    let m = %{"b" => 2, "a" => 1, "c" => 3};
    yield expect_eq(m.values(), [1, 2, 3]);
}
"#;
    let compilation = Compiler::new()
        .compile(SOURCE)
        .expect("Map.values source compiles");
    assert!(
        compilation.functions[0]
            .nodes
            .iter()
            .any(|node| matches!(node.op, VirOp::MapValues))
    );

    let report = run_source(SOURCE).expect("Map.values runs through verified production path");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
    }
}

#[test]
fn string_plus_remains_distinct_from_collection_addition() {
    const SOURCE: &str = r#"
fn suffixed(value: String) -> String {
    value + "!"
}
"#;
    let compilation = Compiler::new()
        .compile(SOURCE)
        .expect("string + compiles to its own VIR grain");
    assert!(compilation.warnings.entries.is_empty());
    let concat = compilation.functions[0]
        .nodes
        .iter()
        .find(|node| matches!(node.op, VirOp::StringConcat))
        .expect("string + becomes StringConcat");
    assert_eq!(concat.ty, VirType::String);
    assert_eq!(concat.effect, vix::vir::EffectFacts::PURE);
    assert_eq!(
        &SOURCE[concat.span.start as usize..concat.span.end as usize],
        "value + \"!\""
    );
}

#[test]
fn mutation_shaped_collection_methods_are_unknown() {
    for source in [RUNG_032, RUNG_145, RUNG_146] {
        let (expected_message, expected_line) = reject_header(source);
        let diagnostics = Compiler::new()
            .compile(source)
            .expect_err("mutation-shaped array methods remain absent");
        assert_eq!(diagnostics.entries.len(), 1);
        let diagnostic = &diagnostics.entries[0];
        assert_eq!(diagnostic.code, DiagnosticCode::UnknownMethod);
        assert_eq!(diagnostic.message(), expected_message);
        assert_eq!(source_line(source, diagnostic.primary.start), expected_line);
    }
}

/// The `//! uses:` harness directive (testing.md, "The two kinds of `Check`" /
/// directives section): each header line names a library module file presented
/// to the compiler as the module its file stem names. The directive lines are
/// consumed — removed, not blanked — so `//! at:` line numbers in reject
/// headers refer to the source as compiled.
fn modules_fixture(source: &str) -> (String, Vec<ModuleSource<'static>>) {
    let mut modules = Vec::new();
    let mut body = String::new();
    for line in source.lines() {
        if let Some(path) = line.strip_prefix("//! uses: ") {
            let stem = path
                .rsplit('/')
                .next()
                .and_then(|file| file.strip_suffix(".vix"))
                .expect("uses: directive names a .vix file");
            let module = match stem {
                "geometry" => ModuleSource {
                    name: "geometry",
                    source: LIB_GEOMETRY,
                },
                other => panic!("uses: directive names an unbundled module `{other}`"),
            };
            modules.push(module);
            continue;
        }
        body.push_str(line);
        body.push('\n');
    }
    (body, modules)
}

/// Rung 106 — modules exist: a type and a function import from another file
/// and compose through the production path, plain and chaos agreeing.
#[test]
fn rung_106_imports_run_through_production_path() {
    let (source, modules) = modules_fixture(RUNG_106);
    let report = run_source_with_modules(&source, &modules).expect("rung 106 compiles and runs");
    assert!(report.passed(), "rung 106 checks pass: {report:?}");
    assert!(report.agrees(), "plain and chaos agree");
    assert_eq!(report.plain.checks.len(), 1);
    assert_eq!(report.plain.checks, report.chaos.checks);
}

/// Rung 107 (reject) — a non-`pub` item is not importable: exactly the
/// declared diagnostic (`private`) at the declared line, anchored on the
/// import that names the private item.
#[test]
fn rung_107_private_import_is_rejected() {
    let (source, modules) = modules_fixture(RUNG_107);
    let (expected_message, expected_line) = reject_header(RUNG_107);
    let diagnostics = Compiler::new()
        .compile_with_modules(&source, &modules)
        .expect_err("private items do not import");
    assert_eq!(diagnostics.entries.len(), 1);
    let diagnostic = &diagnostics.entries[0];
    assert_eq!(diagnostic.code, DiagnosticCode::PrivateImport);
    assert_eq!(diagnostic.message(), expected_message);
    assert_eq!(
        source_line(&source, diagnostic.primary.start),
        expected_line
    );
    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::Name {
            name: "geometry::private_helper".to_owned(),
        },
        "the typed payload names the private item",
    );
}

/// Rung 108 — imported modules compose with std across the boundary: an
/// imported function flows through `map`, and `sorted` orders the results
/// (the seam the old corpus couldn't cross: appended fixture code calling
/// imported std helpers).
#[test]
fn rung_108_std_composes_across_module_boundaries() {
    let (source, modules) = modules_fixture(RUNG_108);
    let report = run_source_with_modules(&source, &modules).expect("rung 108 compiles and runs");
    assert!(report.passed(), "rung 108 checks pass: {report:?}");
    assert!(report.agrees(), "plain and chaos agree");
    assert_eq!(report.plain.checks.len(), 1);
    assert_eq!(report.plain.checks, report.chaos.checks);
}

/// Rung 109 (reject) — importing a name that's also declared locally is an
/// unqualified collision: exactly the declared diagnostic (`duplicate name`)
/// at the declared line, anchored on the *second* binding site (the local
/// declaration following the import).
#[test]
fn rung_109_import_local_collision_is_rejected() {
    let (source, modules) = modules_fixture(RUNG_109);
    let (expected_message, expected_line) = reject_header(RUNG_109);
    let diagnostics = Compiler::new()
        .compile_with_modules(&source, &modules)
        .expect_err("an imported name may not be redeclared locally");
    assert_eq!(diagnostics.entries.len(), 1);
    let diagnostic = &diagnostics.entries[0];
    assert_eq!(diagnostic.code, DiagnosticCode::DuplicateDefinition);
    assert_eq!(diagnostic.message(), expected_message);
    assert_eq!(
        source_line(&source, diagnostic.primary.start),
        expected_line
    );
    assert_eq!(
        diagnostic.payload,
        DiagnosticPayload::Name {
            name: "Point".to_owned(),
        },
        "the typed payload names the colliding name",
    );
}

/// Rung 110's foundation contract, certified the way the decode identity
/// oracle is: memo/lowering identity is stable across module boundaries — an
/// island's canonical recipe must not depend on which module spelled it.
///
/// Two compilations demand the same `geometry::magnitude_sq(geometry::Point
/// { x: 6, y: 8 })` invocation from *differently shaped* importers (spelling
/// B adds an unrelated imported module and an unrelated root declaration, so
/// every `FunctionId` and item ordinal shifts). The invocation's wire island
/// must produce byte-identical canonical VIR, one lowered recipe, one demand
/// key — and lowering the second spelling through the first's cache must be
/// a lookup, not a recompute. A different argument value is the
/// discriminating negative control.
#[test]
fn rung_110_recipe_identity_is_importer_independent() {
    const SPELLING_A: &str = "\
import geometry::{Point, magnitude_sq};

#[test]
fn t() -> Stream<Check> {
    let a = magnitude_sq(Point { x: 6, y: 8 });
    let b = magnitude_sq(Point { x: 6, y: 8 });
    yield expect_eq(a, b);
    yield expect_eq(a + b, 200);
}
";
    const SPELLING_B: &str = "\
import noise::{triple, Widget};
import geometry::{Point, magnitude_sq};

fn pad(w: Widget) -> Int { triple(w.id) + 1 }

#[test]
fn t() -> Stream<Check> {
    let a = magnitude_sq(Point { x: 6, y: 8 });
    let b = magnitude_sq(Point { x: 6, y: 8 });
    yield expect_eq(a, b);
    yield expect_eq(a + b, 200);
}
";
    const CONTROL: &str = "\
import geometry::{Point, magnitude_sq};

#[test]
fn t() -> Stream<Check> {
    let a = magnitude_sq(Point { x: 8, y: 6 });
    let b = magnitude_sq(Point { x: 8, y: 6 });
    yield expect_eq(a, b);
    yield expect_eq(a + b, 200);
}
";
    const NOISE: &str = "\
pub fn triple(n: Int) -> Int { n * 3 }
pub struct Widget { id: Int }
";
    let geometry = ModuleSource {
        name: "geometry",
        source: LIB_GEOMETRY,
    };
    let noise = ModuleSource {
        name: "noise",
        source: NOISE,
    };

    // The two structurally equal spelled invocations collapse to ONE wire
    // island before the runtime exists: one recipe is a compile-time fact.
    let partition = |source: &str, modules: &[ModuleSource]| {
        let module = Compiler::new()
            .compile_with_modules(source, modules)
            .expect("spelling compiles");
        let partitioned = module.partition_test(&module.tests[0]);
        assert_eq!(
            partitioned.wire_islands.len(),
            1,
            "both spelled invocations share one wire island",
        );
        (module, partitioned)
    };
    let (_module_a, partitioned_a) = partition(SPELLING_A, &[geometry]);
    let (_module_b, partitioned_b) = partition(SPELLING_B, &[noise, geometry]);
    let (_module_c, partitioned_c) = partition(CONTROL, &[geometry]);
    let wire_a = &partitioned_a.wire_islands[0].island;
    let wire_b = &partitioned_b.wire_islands[0].island;
    let wire_c = &partitioned_c.wire_islands[0].island;
    assert_ne!(
        wire_a.function, wire_b.function,
        "the reshaped importer shifts raw FunctionIds — identity must not ride on them",
    );
    assert_eq!(
        wire_a.canonical_recipe_bytes(),
        wire_b.canonical_recipe_bytes(),
        "one canonical recipe regardless of which module set spelled the demand",
    );
    assert_ne!(
        wire_a.canonical_recipe_bytes(),
        wire_c.canonical_recipe_bytes(),
        "a different argument is a different recipe (discriminating control)",
    );

    // One lowered recipe, one demand key; the second spelling's lowering is a
    // cache lookup, never a recompute.
    let mut cache = LoweringCache::default();
    let lowered_a = cache.get_or_lower(wire_a).expect("wire island lowers");
    let (recipe_a, key_a) = (lowered_a.recipe, lowered_a.demand_key);
    let lowered_b = cache.get_or_lower(wire_b).expect("wire island re-lowers");
    assert_eq!(recipe_a, lowered_b.recipe, "one lowered recipe");
    assert_eq!(key_a, lowered_b.demand_key, "one demand key");
    assert_eq!(cache.counters().misses, 1);
    assert_eq!(cache.counters().hits, 1);
    let lowered_c = cache.get_or_lower(wire_c).expect("control lowers");
    assert_ne!(key_a, lowered_c.demand_key, "control demand key diverges");

    // Production path: both spellings run green, agree across lanes, and the
    // second await of the shared cross-module demand is a memo lookup.
    let report_a = run_source_with_modules(SPELLING_A, &[geometry]).expect("spelling A runs");
    let report_b =
        run_source_with_modules(SPELLING_B, &[noise, geometry]).expect("spelling B runs");
    for report in [&report_a, &report_b] {
        assert!(report.passed(), "cross-module memo run passes: {report:?}");
        assert!(report.agrees(), "plain and chaos agree");
        assert!(
            report.plain.counters.memo_hits_exact >= 1,
            "the second await of the shared invocation is a lookup: {:?}",
            report.plain.counters,
        );
    }
    assert_eq!(
        report_a.plain.checks, report_b.plain.checks,
        "identical check identities (ValueId) regardless of importer shape",
    );
}

/// Rung 110's fixture itself stays red at a *typed* boundary, pinned exactly:
/// its `never_demanded(magnitude_sq(Point { x: 6, y: 8 }))` names a
/// described-wire selector with a record-literal argument, and described
/// selectors admit only closed scalar literals today (`WireArg::Int/Bool`).
/// The cross-run `//! rerun` scope is the second open seam (PORT-NOTES.md:
/// rerun-phase scoping is an explicit design PROPOSAL, not implementable
/// surface). This pin goes green-red loudly when either seam moves.
#[test]
fn rung_110_stops_at_the_described_wire_record_literal_boundary() {
    let (source, modules) = modules_fixture(RUNG_110);
    let diagnostics = Compiler::new()
        .compile_with_modules(&source, &modules)
        .expect_err("a record-literal described-wire argument is a typed boundary");
    assert_eq!(diagnostics.entries.len(), 1);
    let diagnostic = &diagnostics.entries[0];
    assert_eq!(diagnostic.code, DiagnosticCode::UnsupportedExpression);
    assert_eq!(
        diagnostic.message(),
        "a described-wire argument must be a closed scalar literal",
    );
    assert_eq!(
        &source[diagnostic.primary.start as usize..diagnostic.primary.end as usize],
        "Point { x: 6, y: 8 }",
        "the boundary anchors on the record literal inside never_demanded",
    );
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

fn warning_header(source: &str) -> (&str, usize) {
    let mut message = None;
    let mut line = None;
    for header in source.lines().take_while(|line| line.starts_with("//!")) {
        if let Some(value) = header.strip_prefix("//! warn: ") {
            message = Some(value);
        }
        if let Some(value) = header.strip_prefix("//! at: ") {
            line = Some(value.parse::<usize>().expect("warning line is an integer"));
        }
    }
    (
        message.expect("warning rung declares a message"),
        line.expect("warning rung declares a line"),
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

/// Certifies the **constant-fold subset** of the typed-decode gate
/// (FOUNDATION.md, "To score past 066"). These rungs decode compile-time-
/// constant document literals against the `let`/call-site target type in a
/// single `FormatParser` pass inside `Compiler::compile`, and lower the result
/// to the exact typed-construction VIR a hand-written literal produces. This is
/// a legitimate as-if fold of the doctrine primitive's constant-input case — it
/// is *not* the runtime `r[machine.primitive.typed-deserialization]` doc-parse
/// primitive, which serves dynamic documents and returns a runtime `Outcome`.
///
/// The zero-`HostCall`/`HostCallYield` assertion below is therefore an **as-if
/// optimization certificate** — proof the fold keeps the parse out of the
/// verified machine — not proof the runtime primitive exists. The runtime seam
/// for dynamic/unknown-target decodes is named explicitly elsewhere
/// (`DiagnosticCode::RuntimeDecodeUnavailable`), never satisfied here.
fn assert_typed_decode_rung(source: &str, checks: usize) {
    let module = Compiler::new()
        .compile(source)
        .expect("typed-decode rung compiles through the canonical surface");
    let partitioned = module.partition_test(&module.tests[0]);
    let mut lowering_cache = LoweringCache::default();
    for island in &partitioned.islands {
        let lowered = lowering_cache
            .get_or_lower(island)
            .expect("typed-decode rung lowers through verified Weavy execution");
        assert!(
            lowered.program().fns.iter().all(|function| {
                function.code.iter().all(|op| {
                    !matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. })
                })
            }),
            "decode lowers to typed construction, never a machine host call",
        );
    }

    let report = run_source(source).expect("typed-decode rung runs through Executable");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), checks);
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert!(lane.checks.iter().all(|check| check.passed));
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
        if std::env::var("WEAVY_JIT").as_deref() == Ok("0") {
            assert!(
                lane.events
                    .iter()
                    .filter_map(|event| match event.kind {
                        EventKind::ExecutionLane { facts, .. } => Some(facts),
                        _ => None,
                    })
                    .all(|facts| matches!(
                        facts,
                        vix::runtime::ExecutionFacts {
                            selected: vix::runtime::ExecutionLaneFact::Interpreter,
                            fallback: Some(
                                vix::runtime::ExecutionFallbackFact::DisabledByEnvironment
                            ),
                            ..
                        }
                    ))
            );
        }
    }
}

/// Rung 062 — JSON lands directly on a struct via the type-directed decoder.
#[test]
fn rung_062_json_decode_lands_json_on_a_struct() {
    assert_typed_decode_rung(RUNG_062, 3);
}

/// Rung 063 — TOML manifests decode into nested structs with no Doc-walking.
#[test]
fn rung_063_toml_decode_builds_nested_structs() {
    assert_typed_decode_rung(RUNG_063, 2);
}

/// Rung 064 — absent fields decode to `Option::None`, present ones to `Some`,
/// through the same decoder and the same typed construction.
#[test]
fn rung_064_absent_fields_decode_to_option_none() {
    assert_typed_decode_rung(RUNG_064, 2);
}

/// Rung 065 — the string-or-table enum form (the Cargo dependency shape): a
/// scalar string decodes to the short single-`String` tuple variant, an object
/// to the detailed record variant, both through the same decoder and lowered to
/// ordinary `Op::Variant` construction.
#[test]
fn rung_065_decodes_string_or_table_enum_forms() {
    assert_typed_decode_rung(RUNG_065, 3);
}

/// Adversarial identity oracle for the constant fold: a successfully folded
/// decoded value must be *the same value* as the equivalent authored typed
/// construction — proven through the production canonical-recipe and Store
/// framed-identity paths, not merely by both `expect_eq`s passing.
///
/// Two programs identical except decode-vs-literal fold to identical canonical
/// VIR, so they share a `RecipeId` and `DemandKey`; run through `run_source`
/// they intern the *same* framed value identities and produce identical check
/// identities. A negative control (a different authored value) must diverge, so
/// the oracle is discriminating rather than trivially true.
#[test]
fn decoded_value_is_identity_equivalent_to_authored_construction() {
    const AUTHORED: &str = "\
struct PkgRow { name: String, vers: String, yanked: Bool }
#[test]
fn t() -> Stream<Check> {
    let row = PkgRow { name: \"mio\", vers: \"0.8.11\", yanked: false };
    yield expect_eq(row.name, \"mio\");
    yield expect_eq(row.vers, \"0.8.11\");
}
";
    const DECODED: &str = "\
struct PkgRow { name: String, vers: String, yanked: Bool }
#[test]
fn t() -> Stream<Check> {
    let row: PkgRow = json_decode(\"{\\\"name\\\":\\\"mio\\\",\\\"vers\\\":\\\"0.8.11\\\",\\\"yanked\\\":false}\");
    yield expect_eq(row.name, \"mio\");
    yield expect_eq(row.vers, \"0.8.11\");
}
";
    // A different authored value: same shape, different `vers`. The fold must be
    // discriminating — this must NOT collide with the decoded recipe/identity.
    const OTHER: &str = "\
struct PkgRow { name: String, vers: String, yanked: Bool }
#[test]
fn t() -> Stream<Check> {
    let row = PkgRow { name: \"mio\", vers: \"9.9.9\", yanked: false };
    yield expect_eq(row.name, \"mio\");
    yield expect_eq(row.vers, \"0.8.11\");
}
";

    // 1. Canonical-recipe / framed-identity path: the folded decode is
    //    byte-identical VIR to the authored construction, hence one recipe and
    //    one demand key; the negative control diverges.
    let lower = |source: &str| {
        let module = Compiler::new().compile(source).expect("compiles");
        let partitioned = module.partition_test(&module.tests[0]);
        let canonical = partitioned.islands[0].canonical_recipe_bytes();
        let mut cache = LoweringCache::default();
        let lowered = cache.get_or_lower(&partitioned.islands[0]).expect("lowers");
        (canonical, lowered.recipe, lowered.demand_key)
    };
    let (authored_vir, authored_recipe, authored_key) = lower(AUTHORED);
    let (decoded_vir, decoded_recipe, decoded_key) = lower(DECODED);
    let (other_vir, other_recipe, _other_key) = lower(OTHER);
    assert_eq!(
        authored_vir, decoded_vir,
        "the fold emits byte-identical canonical VIR to the authored construction"
    );
    assert_eq!(authored_recipe, decoded_recipe, "one canonical recipe");
    assert_eq!(authored_key, decoded_key, "one demand key");
    assert_ne!(
        authored_vir, other_vir,
        "a different authored value must produce a different recipe"
    );
    assert_ne!(authored_recipe, other_recipe);

    // 2. Production Store path: running both interns the same framed value
    //    identities and yields identical check identities.
    let store_identities = |run: &vix::ratchet::SuiteRun| {
        run.events
            .iter()
            .filter_map(|event| match &event.kind {
                EventKind::StoreAlloc { identity, .. } => Some(identity.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let authored_run = run_source(AUTHORED).expect("authored runs");
    let decoded_run = run_source(DECODED).expect("decoded runs");
    assert!(authored_run.passed() && decoded_run.passed());
    let authored_ids = store_identities(&authored_run.plain);
    let decoded_ids = store_identities(&decoded_run.plain);
    assert!(
        !authored_ids.is_empty(),
        "the construction interns at least one framed value"
    );
    assert_eq!(
        authored_ids, decoded_ids,
        "decode and authored construction intern the same framed store identities"
    );
    assert_eq!(
        authored_run.plain.checks, decoded_run.plain.checks,
        "identical check identities (ValueId) through the production path"
    );
}

/// A decode whose document is not a compile-time-constant literal cannot be
/// folded; it builds an ordinary typed request value and lowers through the one
/// generic registered-primitive invocation.
#[test]
fn nonliteral_decode_names_the_runtime_seam() {
    const SOURCE: &str = "\
struct PkgRow { name: String }
#[test]
fn t() -> Stream<Check> {
    let src = \"{}\";
    let row: PkgRow = json_decode(src);
    yield expect_eq(row.name, \"x\");
}
";
    let compilation = Compiler::new()
        .compile(SOURCE)
        .expect("a nonliteral decode document lowers to the runtime seam");
    assert!(
        compilation.module.functions[0]
            .nodes
            .iter()
            .any(|node| matches!(
                &node.op,
                VirOp::InvokePrimitive { primitive }
                    if primitive == &vix::vir::decode_primitive_id()
            )),
        "dynamic JSON decode is a generic registered invocation: {compilation:#?}"
    );
}

/// A nonliteral fallible decode crosses the registered primitive authority.
/// This is deliberately not a fold: the authored graph contains one generic
/// invocation, the runtime records exactly one dispatch and one source-read
/// receipt per dynamic document, and both the success and structured
/// `Err(DecodeError)` branch execute through ordinary Result matching.
#[test]
fn dynamic_json_and_toml_decode_use_one_registered_invocation_per_document() {
    const JSON_OK: &str = "\
struct PkgRow { name: String }
fn check(src: String) -> Bool {
    match try_json_decode<PkgRow>(src) {
        Ok(row) => row.name == \"mio\",
        Err(_) => false,
    }
}
#[test]
fn t() -> Stream<Check> {
    let src = \"{\\\"name\\\":\\\"mio\\\"}\";
    yield expect(check(src));
}
";
    const JSON_ERR: &str = "\
struct PkgRow { name: String }
fn check(src: String) -> Bool {
    match try_json_decode<PkgRow>(src) {
        Ok(_) => false,
        Err(error) => error.path == \"name\"
            && error.document_offset == 8
            && error.document_len == 2,
    }
}
#[test]
fn t() -> Stream<Check> {
    let src = \"{\\\"name\\\":42}\";
    yield expect(check(src));
}
";
    const TOML_OK: &str = "\
struct PkgRow { name: String }
fn check(src: String) -> Bool {
    match try_toml_decode<PkgRow>(src) {
        Ok(row) => row.name == \"mio\",
        Err(_) => false,
    }
}
#[test]
fn t() -> Stream<Check> {
    let src = \"name = \\\"mio\\\"\\n\";
    yield expect(check(src));
}
";

    for (source, expected_documents) in [(JSON_OK, 1u64), (JSON_ERR, 1), (TOML_OK, 1)] {
        let module = Compiler::new()
            .compile(source)
            .expect("dynamic decode compiles");
        assert_eq!(
            module
                .functions
                .iter()
                .flat_map(|function| &function.nodes)
                .filter(|node| matches!(node.op, VirOp::InvokePrimitive { .. }))
                .count() as u64,
            expected_documents,
            "one generic primitive invocation for one dynamic document",
        );
        let report = run_source(source).expect("dynamic decode runs");
        assert!(report.passed() && report.agrees());
        for lane in [&report.plain, &report.chaos] {
            assert_eq!(lane.counters.primitive_invocations, expected_documents);
            assert!(
                lane.counters.peak_primitive_parked_frames >= expected_documents,
                "every yielded primitive frame is retained off-stack: {lane:#?}",
            );
            assert_eq!(
                lane.counters.completion_inbox_receipts, expected_documents,
                "every primitive completion crosses the unified inbox",
            );
            assert_eq!(lane.counters.document_parse_host_calls, 0);
            assert_eq!(lane.counters.pure_host_calls, 0);
            assert_eq!(lane.receipt_count, expected_documents);

            let parked = lane
                .events
                .iter()
                .filter_map(|event| match &event.kind {
                    EventKind::PrimitiveParked { task, key, site } => Some((*task, *key, *site)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let resumed = lane
                .events
                .iter()
                .filter_map(|event| match &event.kind {
                    EventKind::PrimitiveResumed { task, key, site } => Some((*task, *key, *site)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(parked.len() as u64, expected_documents);
            assert_eq!(
                resumed, parked,
                "the inbox resumes the exact retained task and verified frame/PC",
            );
        }
    }
}

/// A malformed constant document fails as a *structured typed* diagnostic — a
/// stable kind label, a structured field path, and the offending document byte
/// span — never a stringly `UnsupportedExpression`. No identity depends on the
/// rendered prose.
#[test]
fn malformed_literal_decode_is_a_structured_typed_failure() {
    const SOURCE: &str = "\
struct PkgRow { name: String, vers: String }
#[test]
fn t() -> Stream<Check> {
    let row: PkgRow = json_decode(\"{\\\"name\\\": 42, \\\"vers\\\": \\\"x\\\"}\");
    yield expect_eq(row.vers, \"x\");
}
";
    let diagnostics = Compiler::new()
        .compile(SOURCE)
        .expect_err("an integer where a String is expected fails the fold");
    assert_eq!(diagnostics.entries.len(), 1);
    assert_eq!(diagnostics.entries[0].code, DiagnosticCode::DecodeFailed);
    let DiagnosticPayload::Decode {
        format,
        target,
        kind,
        path,
        doc_offset,
        doc_len,
        ..
    } = &diagnostics.entries[0].payload
    else {
        panic!("expected a structured Decode payload");
    };
    assert_eq!(format, "JSON");
    assert_eq!(target, "PkgRow");
    assert_eq!(kind, "expected-scalar");
    assert_eq!(path, &["name".to_owned()]);
    // The document span points at the offending `42` inside the decoded literal.
    let document = "{\"name\": 42, \"vers\": \"x\"}";
    let offset = doc_offset.expect("document offset preserved") as usize;
    let len = doc_len.expect("document length preserved") as usize;
    assert_eq!(&document[offset..offset + len], "42");
}

/// Rung 066 — a decode that can fail returns a `Result<T, DecodeError>` value,
/// not a crash. `try_json_decode<PkgRow>(...)` names the target schema at the
/// call site; the failing document (an integer where `name: String` is expected)
/// produces `Err(e)`, and `e.message` — a rendered projection over the typed
/// error's structural fields — contains the offending field name.
///
/// On this compile-time-constant document the decode is the **constant fold** of
/// the runtime doc-parse primitive: it runs once at compile time and its typed
/// `Err` value is emitted as ordinary typed construction (`Op::Variant` +
/// `Op::Record`), so the run performs no host call and records no receipt. The
/// chaos lane agrees with plain, as the foundation requires from rung 001.
#[test]
fn rung_066_decode_failure_is_a_typed_result() {
    let report = run_source(RUNG_066).expect("rung 066 runs through Executable");
    assert!(report.passed());
    assert!(report.agrees());
    assert_eq!(report.plain.checks.len(), 1);
    assert_eq!(report.plain.checks, report.chaos.checks);
    for lane in [&report.plain, &report.chaos] {
        assert!(lane.checks.iter().all(|check| check.passed));
        // The literal decode is folded, so nothing reaches the machine as a host
        // call or a recorded read: the fold is the constant-folded subset of the
        // runtime primitive.
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
    }
}

/// The fold-is-an-optimization proof for the fallible surface. A successfully
/// decoded `Ok(row)` payload must be *the same value* as the equivalent authored
/// typed construction — the F2 corrective seam: the literal fold is provably the
/// constant-folded form of the same typed-construction primitive.
///
/// The decoded program binds the `Ok` payload through an ordinary match and
/// yields the same projections an authored `PkgRow` construction does; the two
/// produce identical check identities (`ValueId`) through the production Store
/// path. A negative control (a different authored value) must diverge, so the
/// oracle is discriminating rather than trivially true. (Fold determinism —
/// identical canonical VIR across independent compiles — is certified
/// separately by `try_decode_fold_is_deterministic_canonical_vir`, keeping each
/// certificate's wall time bounded on a contended machine.)
#[test]
fn decoded_result_ok_payload_is_identity_equivalent_to_authored_construction() {
    const AUTHORED: &str = "\
struct PkgRow { name: String, vers: String }
#[test]
fn t() -> Stream<Check> {
    let row = PkgRow { name: \"mio\", vers: \"0.8.11\" };
    yield expect_eq(row.name, \"mio\");
    yield expect_eq(row.vers, \"0.8.11\");
}
";
    const DECODED: &str = "\
struct PkgRow { name: String, vers: String }
#[test]
fn t() -> Stream<Check> {
    yield match try_json_decode<PkgRow>(\"{\\\"name\\\":\\\"mio\\\",\\\"vers\\\":\\\"0.8.11\\\"}\") {
        Ok(row) => expect_eq(row.name, \"mio\"),
        Err(_) => expect(false),
    };
    yield match try_json_decode<PkgRow>(\"{\\\"name\\\":\\\"mio\\\",\\\"vers\\\":\\\"0.8.11\\\"}\") {
        Ok(row) => expect_eq(row.vers, \"0.8.11\"),
        Err(_) => expect(false),
    };
}
";
    // A different authored value: same shape, different `vers`. The decoded Ok
    // payload must NOT collide with this control's identity.
    const OTHER: &str = "\
struct PkgRow { name: String, vers: String }
#[test]
fn t() -> Stream<Check> {
    let row = PkgRow { name: \"mio\", vers: \"9.9.9\" };
    yield expect_eq(row.name, \"mio\");
    yield expect_eq(row.vers, \"0.8.11\");
}
";

    // Production Store path: the decoded Ok payload yields identical check
    // identities to the authored construction; the negative control diverges.
    let authored_run = run_source(AUTHORED).expect("authored runs");
    let decoded_run = run_source(DECODED).expect("decoded runs");
    let other_run = run_source(OTHER).expect("control runs");
    assert!(authored_run.passed() && decoded_run.passed());
    // Compare the checked *value* identities (schema + content), independent of
    // yield-site provenance — the decoded program binds its payload through a
    // match, so its yield sites are numbered differently, but the value each
    // check observes must be the same content-addressed value.
    let check_value_ids = |run: &vix::ratchet::SuiteRun| {
        run.checks
            .iter()
            .map(|check| check.identity.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        check_value_ids(&authored_run.plain),
        check_value_ids(&decoded_run.plain),
        "the decoded Ok payload is the same value as the authored construction"
    );
    assert_ne!(
        check_value_ids(&authored_run.plain),
        check_value_ids(&other_run.plain),
        "a different authored value produces a different checked value identity"
    );
}

/// Two independent compiles of the same `try_json_decode<T>` program fold to
/// byte-identical canonical VIR — one recipe, one demand key — proving the fold
/// is deterministic and content-addressed.
#[test]
fn try_decode_fold_is_deterministic_canonical_vir() {
    const DECODED: &str = "\
struct PkgRow { name: String, vers: String }
#[test]
fn t() -> Stream<Check> {
    yield match try_json_decode<PkgRow>(\"{\\\"name\\\":\\\"mio\\\",\\\"vers\\\":\\\"0.8.11\\\"}\") {
        Ok(row) => expect_eq(row.name, \"mio\"),
        Err(_) => expect(false),
    };
}
";
    let lower = |source: &str| {
        let module = Compiler::new().compile(source).expect("compiles");
        let partitioned = module.partition_test(&module.tests[0]);
        partitioned.islands[0].canonical_recipe_bytes()
    };
    assert_eq!(
        lower(DECODED),
        lower(DECODED),
        "the fold is deterministic canonical VIR"
    );
}

// ---------------------------------------------------------------------------
// Rungs 067–070 — the exec band: run+capture, failure-as-value, memoized exec,
// undeclared capability (reject).
// ---------------------------------------------------------------------------

/// The exec band's surface parses through the canonical grammar: capability-
/// tagged backtick command templates, `exec`, postfix `?`, and Ok/Err patterns
/// all land in the generated typed AST.
#[test]
fn exec_band_surface_parses() {
    let parser = SurfaceParser::new();
    for (rung, source) in [
        (67, RUNG_067),
        (68, RUNG_068),
        (69, RUNG_069),
        (70, RUNG_070),
    ] {
        parser
            .parse(source)
            .unwrap_or_else(|error| panic!("rung {rung:03} parses: {error:?}"));
    }
}

/// Rung 070 — using a tool the test did not declare is not a special error: a
/// command's tag is a capability VALUE and a test's capabilities are parameters
/// the harness supplies, so `cc` is an ordinary unbound identifier. The primary
/// span names the declaration that cannot name it (attributes included); the
/// use site is a label.
///
/// r[verify lang.diagnostics.typed]
/// r[verify machine.primitive.capabilities-by-identity]
#[test]
fn rung_070_undeclared_capability_is_an_unbound_identifier() {
    let (expected_message, expected_line) = reject_header(RUNG_070);
    let diagnostics = Compiler::new()
        .compile(RUNG_070)
        .expect_err("rung 070 must be rejected");
    assert_eq!(diagnostics.entries.len(), 1);
    let diagnostic = &diagnostics.entries[0];
    assert_eq!(diagnostic.code, DiagnosticCode::UnboundIdentifier);
    assert_eq!(diagnostic.message(), expected_message);
    assert_eq!(
        source_line(RUNG_070, diagnostic.primary.start),
        expected_line
    );
    // The use site stays visible as a related label on the `cc` tag.
    assert_eq!(diagnostic.labels.len(), 1);
    assert_eq!(
        &RUNG_070[diagnostic.labels[0].span.start as usize..diagnostic.labels[0].span.end as usize],
        "cc"
    );
}

/// Rungs 067–069 execute through the scheduler-owned effect primitive. The
/// successful echo result crosses the ordinary frozen value-input boundary into
/// its verified check island; the effect task itself records a real
/// spawn/park/resume lifecycle and a capability read receipt.
///
/// r[verify machine.primitive.exec-outcome]
/// r[verify machine.primitive.capabilities-by-identity]
#[test]
fn rung_067_exec_echo_runs_through_the_capability_effect_demand() {
    let report = run_source(RUNG_067).expect("rung 067 runs");
    assert!(report.passed(), "rung 067 passes: {report:?}");
    assert!(report.agrees());

    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.effect_spawns, 1);
        assert_eq!(lane.receipt_count, 1);
        assert!(lane.events.iter().any(|event| matches!(
            event.kind,
            EventKind::TaskTransition {
                to: TaskState::Parked,
                ..
            }
        )));
        assert!(lane.events.iter().any(|event| matches!(
            event.kind,
            EventKind::TaskTransition {
                from: TaskState::Parked,
                to: TaskState::Running,
                ..
            }
        )));
    }
}

/// A nonzero exit is retained as `ProcessFailure` in the effect demand and
/// becomes `Result::Err` only at the scheduler-owned postfix-catch boundary.
///
/// r[verify machine.primitive.exit-status-is-not-a-value]
#[test]
fn rung_068_process_failure_is_caught_as_a_typed_result() {
    let report = run_source(RUNG_068).expect("rung 068 runs");
    assert!(report.passed(), "rung 068 passes: {report:?}");
    assert!(report.agrees());

    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.effect_spawns, 1);
        assert_eq!(lane.receipt_count, 1);
        assert!(lane.values.iter().any(|value| matches!(
            value.failure,
            Some(FailureValue::ProcessFailure {
                termination: ProcessTermination::Exited { code: 1 },
                ..
            })
        )));
    }
}

/// The plan × capability demand preimage is shared across source sites: the
/// second syntactically separate `exec echo` observes the same completed demand
/// and cannot spawn another process.
///
/// r[verify machine.primitive.exec-identity]
/// r[verify machine.memo.no-recompute-at-lookup]
#[test]
fn rung_069_exec_memoizes_by_plan_and_capability_identity() {
    let report = run_source(RUNG_069).expect("rung 069 runs");
    assert!(report.passed(), "rung 069 passes: {report:?}");
    assert!(report.agrees());

    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.effect_spawns, 1);
        assert!(lane.counters.memo_hits_exact >= 1);
        assert_eq!(
            lane.events
                .iter()
                .filter(|event| matches!(event.kind, EventKind::EffectSpawned { .. }))
                .count(),
            1
        );
    }
}

// ---------------------------------------------------------------------------
// Rung 050 — verifier-visible self-tail loop (auxiliary certificates).
//
// The canonical rung above composes these machine certificates with the
// separately-landed TraceCheck and outer-budget substrate: a terminal self-call
// lowers to an in-place loop backedge (no Weavy Call, no new frame), ordinary
// and non-self recursion keep the Weavy Call, and the loop runs identically and
// cheaply in the interpreter and JIT with no per-iteration scheduler/memo/host
// machinery.
//
// r[impl machine.safepoint.two-classes]
// ---------------------------------------------------------------------------

const TAIL_LOOP_SOURCE: &str = r#"
fn count_up(n: Int) where { limit: Int, acc: Int } -> Int {
    if n == limit { acc } else { count_up(n + 1) where { limit, acc: acc + n } }
}

#[test]
fn drive() -> Stream<Check> {
    yield expect_eq(count_up(0) where { limit: 5, acc: 0 }, 10);
}
"#;

/// A tail call whose callee is a *different* function stays an ordinary Weavy
/// Call: only terminal self-calls become loops.
const TAIL_TO_OTHER_SOURCE: &str = r#"
fn inc(n: Int) -> Int { n + 1 }
fn wrap(n: Int) -> Int { inc(n) }

#[test]
fn drive() -> Stream<Check> {
    yield expect_eq(wrap(41), 42);
}
"#;

struct LoweredProgram {
    program: weavy::task::Program,
    pc_nodes: Vec<Vec<NodeRef>>,
    frames: Vec<FunctionId>,
    module: vix::vir::Module,
}

impl LoweredProgram {
    fn frame_of(&self, name: &str) -> usize {
        let id = self
            .module
            .functions
            .iter()
            .find(|function| function.name == name)
            .expect("named function exists")
            .id;
        self.frames
            .iter()
            .position(|frame| *frame == id)
            .expect("function has a lowered frame")
    }

    fn code(&self, name: &str) -> &[WeavyOp] {
        &self.program.fns[self.frame_of(name)].code
    }
}

fn lower_program(source: &str) -> LoweredProgram {
    let module = Compiler::new()
        .compile(source)
        .expect("source compiles")
        .module;
    let partitioned = module.partition_test(&module.tests[0]);
    let mut cache = LoweringCache::default();
    let lowered = cache
        .get_or_lower(&partitioned.islands[0])
        .expect("source lowers to Weavy");
    let program = lowered.program().clone();
    let pc_nodes = lowered.pc_nodes.clone();
    let frames = attribution_for(&partitioned.islands[0]).functions;
    LoweredProgram {
        program,
        pc_nodes,
        frames,
        module,
    }
}

/// A backedge is a `Jump` whose target is at or before its own instruction —
/// the only shape that closes a loop within one function.
fn backedges(code: &[WeavyOp]) -> Vec<usize> {
    code.iter()
        .enumerate()
        .filter_map(|(pc, op)| match op {
            WeavyOp::Jump { target } if (*target as usize) <= pc => Some(pc),
            _ => None,
        })
        .collect()
}

fn has_weavy_call(code: &[WeavyOp]) -> bool {
    code.iter()
        .any(|op| matches!(op, WeavyOp::Call { .. } | WeavyOp::CallIndirect { .. }))
}

/// The standalone one-function Weavy program for a tail-recursive function.
/// After the transform a tail-recursive function contains no `Call`, so it is
/// self-contained and can be spawned directly at `FnId(0)`.
fn standalone_tail_loop(name: &str) -> weavy::task::Program {
    let lowered = lower_program(TAIL_LOOP_SOURCE);
    let frame = lowered.frame_of(name);
    assert!(
        !has_weavy_call(&lowered.program.fns[frame].code),
        "a standalone tail loop must contain no Weavy Call",
    );
    weavy::task::Program {
        fns: vec![lowered.program.fns[frame].clone()],
    }
}

// r[impl machine.safepoint.two-classes]
#[test]
fn tail_self_call_lowers_to_a_loop_backedge() {
    let lowered = lower_program(TAIL_LOOP_SOURCE);
    let frame = lowered.frame_of("count_up");
    let code = &lowered.program.fns[frame].code;

    // The recursion is a loop, not a frame-allocating call.
    assert!(
        !has_weavy_call(code),
        "a terminal self-call must not lower to a Weavy Call",
    );

    // Exactly one backedge, returning to the loop entry (pc 0).
    let edges = backedges(code);
    assert_eq!(edges.len(), 1, "one loop backedge");
    let backedge_pc = edges[0];
    assert!(
        matches!(code[backedge_pc], WeavyOp::Jump { target: 0 }),
        "the backedge targets the loop entry",
    );

    // The backedge stays attributed (PC -> VIR node) to the terminal self-call.
    let owner = lowered.pc_nodes[frame][backedge_pc];
    let count_up = lowered
        .module
        .functions
        .iter()
        .find(|function| function.name == "count_up")
        .expect("count_up exists");
    assert_eq!(owner.function, count_up.id);
    let node = count_up
        .nodes
        .iter()
        .find(|node| node.id == owner.node)
        .expect("backedge attributes to a VIR node");
    assert!(
        matches!(node.op, VirOp::Call(callee) if callee == count_up.id),
        "the backedge attributes to the terminal self-call node",
    );
}

// r[impl machine.safepoint.two-classes]
#[test]
fn non_tail_self_recursion_keeps_a_weavy_call() {
    // rung 049: `fib(n - 1) + fib(n - 2)` — the calls are operands of `+`, not
    // tail values, so each remains a Weavy Call and no loop forms.
    let lowered = lower_program(RUNG_049);
    let code = lowered.code("fib");
    assert!(
        has_weavy_call(code),
        "non-tail self-recursion still lowers to a Weavy Call",
    );
    assert!(
        backedges(code).is_empty(),
        "non-tail self-recursion forms no loop backedge",
    );
}

// r[impl machine.safepoint.two-classes]
#[test]
fn tail_call_to_another_function_keeps_a_weavy_call() {
    // `wrap` ends in `inc(n)` — a tail call, but to a different function, so it
    // keeps the Weavy Call path.
    let lowered = lower_program(TAIL_TO_OTHER_SOURCE);
    let code = lowered.code("wrap");
    assert!(
        has_weavy_call(code),
        "a tail call to another function stays a Weavy Call",
    );
    assert!(
        backedges(code).is_empty(),
        "a non-self tail call forms no loop backedge",
    );
}

// The 10,000,000-iteration result, driven through the verified Executable in the
// lane the environment selects (native by default; the interpreter under
// WEAVY_JIT=0). Production trace mode keeps the interior pollpoints inert.
//
// r[impl machine.safepoint.two-classes]
#[test]
fn tail_loop_executes_ten_million_iterations_in_the_selected_lane() {
    use weavy::exec::Executable;
    use weavy::task::{FnId, TaskStep, TraceMode};

    let module = Compiler::new()
        .compile(TAIL_LOOP_SOURCE)
        .expect("source compiles")
        .module;
    let partitioned = module.partition_test(&module.tests[0]);
    let mut cache = LoweringCache::default();
    let lowered = cache
        .get_or_lower(&partitioned.islands[0])
        .expect("source lowers to Weavy");
    let count_up = module
        .functions
        .iter()
        .find(|function| function.name == "count_up")
        .expect("count_up exists")
        .id;
    let frame = attribution_for(&partitioned.islands[0])
        .functions
        .iter()
        .position(|function| *function == count_up)
        .expect("count_up has a frame");

    let verified = lowered
        .program()
        .clone()
        .verify(lowered.contract().clone())
        .expect("lowered tail loop re-verifies");
    let executable = std::rc::Rc::new(Executable::with_trace_mode(verified, TraceMode::Production));
    let mut task = executable
        .spawn(FnId(frame as u32))
        .expect("count_up spawns as a verified entry");
    task.write_entry_i64(0, 0).expect("n");
    task.write_entry_i64(1, 10_000_000).expect("limit");
    task.write_entry_i64(2, 0).expect("acc");

    // A pure tail loop runs to completion in a single drive: it never parks on
    // demand and never yields codata.
    let mut ready: [bool; 0] = [];
    let step = task.drive(&mut ready, &[]).expect("tail loop drives");
    assert_eq!(
        step,
        TaskStep::Done,
        "a pure tail loop completes in one drive without parking or yielding",
    );
    assert_eq!(
        task.result_i64().expect("scalar result"),
        49_999_995_000_000,
        "sum of 0..10_000_000 through the self-tail loop",
    );
    assert_eq!(
        task.trace().len(),
        2,
        "the tail loop records only its frame entry and exit",
    );
    assert!(
        task.frame_arena_bytes() < 4096,
        "the tail loop keeps one bounded frame arena, observed {} bytes",
        task.frame_arena_bytes(),
    );
    // The backedge appends no per-iteration instrumentation mark.
    assert!(
        !task
            .trace()
            .iter()
            .any(|event| matches!(event, weavy::task::TaskEvent::Mark(_))),
        "Production-mode tail loop records no instrumentation marks",
    );
}

/// Innards remains an explicit diagnostic lane: it retains source-attributed
/// marks while preserving the structural frame trace that production also
/// reports. The ordinary lowering path must never select this mode implicitly.
#[test]
fn tail_loop_innards_diagnostic_lane_retains_marks() {
    use weavy::exec::Executable;
    use weavy::task::{FnId, TaskEvent, TaskStep, TraceMode};

    let module = Compiler::new()
        .compile(TAIL_LOOP_SOURCE)
        .expect("source compiles")
        .module;
    let partitioned = module.partition_test(&module.tests[0]);
    let mut cache = LoweringCache::default();
    let lowered = cache
        .get_or_lower(&partitioned.islands[0])
        .expect("source lowers to Weavy");
    let count_up = module
        .functions
        .iter()
        .find(|function| function.name == "count_up")
        .expect("count_up exists")
        .id;
    let frame = attribution_for(&partitioned.islands[0])
        .functions
        .iter()
        .position(|function| *function == count_up)
        .expect("count_up has a frame");
    let verified = lowered
        .program()
        .clone()
        .verify(lowered.contract().clone())
        .expect("lowered tail loop re-verifies");
    let executable = std::rc::Rc::new(Executable::with_trace_mode(verified, TraceMode::Innards));
    let mut task = executable
        .spawn(FnId(frame as u32))
        .expect("count_up spawns as a verified entry");
    task.write_entry_i64(0, 0).expect("n");
    task.write_entry_i64(1, 5).expect("limit");
    task.write_entry_i64(2, 0).expect("acc");
    let mut ready: [bool; 0] = [];
    assert_eq!(
        task.drive(&mut ready, &[]).expect("tail loop drives"),
        TaskStep::Done
    );
    assert!(
        task.trace()
            .iter()
            .any(|event| matches!(event, TaskEvent::Mark(_))),
        "the explicit innards lane retains source-attributed marks",
    );
    assert!(
        matches!(task.trace().first(), Some(TaskEvent::FrameEntered(_)))
            && matches!(task.trace().last(), Some(TaskEvent::FrameExited(_))),
        "diagnostic marks do not replace structural frame events",
    );
}

// The interpreter and JIT lanes agree on the 10,000,000-iteration result in one
// process, and neither records a per-iteration mark.
//
// r[impl machine.safepoint.two-classes]
#[test]
fn tail_loop_interpreter_and_jit_agree() {
    use weavy::jit::task_lane::{JitProgram, JitTask};
    use weavy::task::{FnId, Task, TaskEvent, TraceMode};

    let program = standalone_tail_loop("count_up");

    let mut interpreter = Task::spawn_with_mode(&program, FnId(0), TraceMode::Production);
    interpreter.write_i64(8, 0);
    interpreter.write_i64(16, 10_000_000);
    interpreter.write_i64(24, 0);
    interpreter.run(&program, &mut [], &[]);
    let interpreted = interpreter.result_i64();
    assert_eq!(interpreted, 49_999_995_000_000);
    assert!(
        !interpreter
            .trace
            .iter()
            .any(|event| matches!(event, TaskEvent::Mark(_))),
    );

    // The JIT lane is present on native targets unless disabled by the
    // environment (WEAVY_JIT=0); when it is, this run is interpreter-only and
    // there is nothing to compare against.
    match JitProgram::compile_with_mode(&program, TraceMode::Production) {
        Some(jit) => {
            let mut native = JitTask::spawn(&jit, FnId(0));
            native.write_i64(8, 0);
            native.write_i64(16, 10_000_000);
            native.write_i64(24, 0);
            native.run(&jit, &mut [], &[]);
            assert_eq!(
                native.result_i64(),
                interpreted,
                "interpreter and JIT agree on the tail-loop result",
            );
            assert_eq!(
                native.trace, interpreter.trace,
                "interpreter and JIT record the same (mark-free) Production trace",
            );
        }
        None => assert!(
            !weavy::jit::task_lane::available(),
            "the JIT compiles the tail loop whenever the native lane is available",
        ),
    }
}

// Strongest currently-available counter evidence for the rung's budget intent:
// the backedge performs no per-iteration machinery, so the task event trace does
// not grow with iteration count. Canonical rung 050 asserts this through
// TraceCheck budgets (scheduler_requests/memo_entries), which are separately
// owned; the machine-level invariant is that the loop's trace length is constant
// in the iteration count and carries no marks.
//
// r[impl machine.safepoint.two-classes]
#[test]
fn tail_loop_backedge_adds_no_per_iteration_machinery() {
    use weavy::task::{FnId, Task, TaskEvent, TraceMode};

    let program = standalone_tail_loop("count_up");
    let run = |iterations: i64| {
        let mut task = Task::spawn_with_mode(&program, FnId(0), TraceMode::Production);
        task.write_i64(8, 0);
        task.write_i64(16, iterations);
        task.write_i64(24, 0);
        task.run(&program, &mut [], &[]);
        let marks = task
            .trace
            .iter()
            .filter(|event| matches!(event, TaskEvent::Mark(_)))
            .count();
        (task.result_i64(), task.trace.len(), marks)
    };

    let (small_result, small_len, small_marks) = run(1_000);
    let (large_result, large_len, large_marks) = run(1_000_000);

    assert_eq!(small_result, 499_500);
    assert_eq!(large_result, 499_999_500_000);
    assert_eq!(small_marks, 0, "no instrumentation marks at any scale");
    assert_eq!(large_marks, 0, "no instrumentation marks at any scale");
    assert_eq!(
        small_len, large_len,
        "the tail-loop task trace length is constant in the iteration count",
    );
}

const DEP_MIO_GOLDEN: &str =
    "Dep {\n    name: \"mio\",\n    req: \"^0.8\",\n    optional: false,\n}";
const GREEK_LETTERS_GOLDEN: &str = "[\n    \"alpha\",\n    \"beta\",\n    \"gamma\",\n]";

/// The one snapshot check in `report`, asserting lanes agree and it is the only
/// check. Goldens are supplied to `run_source_with_snapshots`, so the verdict is
/// the ratchet's own — not a post-hoc Rust `assert_eq!` over an always-pass run.
fn sole_snapshot(report: &vix::ratchet::RatchetReport) -> SnapshotOutcome {
    assert!(
        report.agrees(),
        "plain and chaos lanes agree on the snapshot"
    );
    assert_eq!(report.plain.checks.len(), 1);
    assert_eq!(report.plain.checks, report.chaos.checks);
    report.plain.checks[0]
        .snapshot
        .as_ref()
        .expect("a snapshot check")
        .outcome
        .clone()
}

#[test]
fn rung_060_snapshots_render_any_value_structurally() {
    // The struct value renders structurally (no Debug impls) and the ratchet's
    // own verdict is driven by the harness snapshot oracle keyed by test + name.
    let oracle = SnapshotExpectations::new().with("snapshot_record", "dep-mio", DEP_MIO_GOLDEN);
    let report = run_source_with_snapshots(RUNG_060, &oracle).expect("rung 060 compiles and runs");
    assert!(report.passed(), "rung 060 snapshot check passes");
    assert_eq!(sole_snapshot(&report), SnapshotOutcome::Matched);
    let snapshot = report.plain.checks[0].snapshot.as_ref().unwrap();
    assert_eq!(snapshot.name, "dep-mio");
    assert_eq!(snapshot.rendered, DEP_MIO_GOLDEN);
}

#[test]
fn rung_060_golden_drift_makes_the_run_fail() {
    // The load-bearing property the audit demanded: perturb the golden and the
    // ratchet verdict goes red — a changed rendering flips report.passed().
    let drifted = SnapshotExpectations::new().with(
        "snapshot_record",
        "dep-mio",
        "Dep {\n    name: \"MIO\",\n    req: \"^0.8\",\n    optional: false,\n}",
    );
    let report = run_source_with_snapshots(RUNG_060, &drifted).expect("rung 060 still runs");
    assert!(!report.passed(), "a drifted golden must fail the run");
    assert!(matches!(
        sole_snapshot(&report),
        SnapshotOutcome::Mismatch { .. }
    ));
}

#[test]
fn rung_060_missing_golden_is_a_red_check_not_a_vacuous_pass() {
    // No expectation supplied: the snapshot must NOT vacuously pass.
    let report = run_source(RUNG_060).expect("rung 060 runs with no oracle");
    assert!(!report.passed(), "a snapshot with no golden must not pass");
    assert_eq!(sole_snapshot(&report), SnapshotOutcome::MissingExpected);
}

#[test]
fn rung_061_snapshots_sorted_stream_values_are_canonical() {
    // Snapshotting a sorted stream projection renders in canonical value order,
    // stable forever: alpha, beta, gamma regardless of authored order.
    let oracle = SnapshotExpectations::new().with(
        "snapshot_canonical",
        "greek-letters",
        GREEK_LETTERS_GOLDEN,
    );
    let report = run_source_with_snapshots(RUNG_061, &oracle).expect("rung 061 compiles and runs");
    assert!(report.passed(), "rung 061 snapshot check passes");
    assert_eq!(sole_snapshot(&report), SnapshotOutcome::Matched);
    let snapshot = report.plain.checks[0].snapshot.as_ref().unwrap();
    assert_eq!(snapshot.name, "greek-letters");
    assert_eq!(snapshot.rendered, GREEK_LETTERS_GOLDEN);
}

// --- Snapshot adversarial coverage (audit findings 2, 3, 4, 6) ---

/// Run inline snapshot source against an oracle and return the checks in site
/// order (plain lane); asserts lanes agree so every assertion is lane-stable.
fn snapshot_checks(source: &str, oracle: &SnapshotExpectations) -> Vec<vix::ratchet::CheckRun> {
    let report = run_source_with_snapshots(source, oracle).expect("snapshot source runs");
    assert!(report.agrees(), "plain and chaos lanes agree");
    report.plain.checks.clone()
}

fn snapshot_outcome(check: &vix::ratchet::CheckRun) -> SnapshotOutcome {
    check
        .snapshot
        .as_ref()
        .expect("a snapshot check")
        .outcome
        .clone()
}

#[test]
fn snapshots_render_scalars_and_strings_without_aborting() {
    // Every currently representable leaf T renders through a semantic value:
    // Int (incl. negative), Bool, and String. None aborts the run.
    const SRC: &str = r#"#[test]
fn scalars() -> Stream<Check> {
    yield expect_snapshot(42, "int");
    yield expect_snapshot(-7, "neg");
    yield expect_snapshot(true, "yes");
    yield expect_snapshot(false, "no");
    yield expect_snapshot("hi", "str");
}
"#;
    let oracle = SnapshotExpectations::new()
        .with("scalars", "int", "42")
        .with("scalars", "neg", "-7")
        .with("scalars", "yes", "true")
        .with("scalars", "no", "false")
        .with("scalars", "str", "\"hi\"");
    let report = run_source_with_snapshots(SRC, &oracle).expect("scalar snapshots run");
    assert!(report.passed(), "all scalar snapshots match their goldens");
    assert_eq!(report.plain.checks.len(), 5);
    for check in &report.plain.checks {
        assert_eq!(snapshot_outcome(check), SnapshotOutcome::Matched);
    }
}

#[test]
fn snapshot_string_escaping_is_canonical_vix_not_debug() {
    // Tab, quote, backslash, newline escape canonically; a non-ASCII scalar is
    // emitted verbatim. This is a defined Vix rule, exercised end to end.
    const SRC: &str = r#"#[test]
fn esc() -> Stream<Check> {
    yield expect_snapshot("tab\tq\"bs\\nl\nend-é", "e");
}
"#;
    const GOLDEN: &str = "\"tab\\tq\\\"bs\\\\nl\\nend-é\"";
    let oracle = SnapshotExpectations::new().with("esc", "e", GOLDEN);
    let checks = snapshot_checks(SRC, &oracle);
    assert_eq!(checks.len(), 1);
    assert_eq!(snapshot_outcome(&checks[0]), SnapshotOutcome::Matched);
    assert_eq!(checks[0].snapshot.as_ref().unwrap().rendered, GOLDEN);
}

#[test]
fn duplicate_snapshot_name_is_a_typed_red_check() {
    // Two snapshots reusing one name in a test do not silently double-emit: the
    // second is a typed DuplicateName failure and the run does not pass.
    const SRC: &str = r#"#[test]
fn dup() -> Stream<Check> {
    yield expect_snapshot(1, "same");
    yield expect_snapshot(2, "same");
}
"#;
    let oracle = SnapshotExpectations::new().with("dup", "same", "1");
    let report = run_source_with_snapshots(SRC, &oracle).expect("dup source runs");
    assert!(report.agrees(), "lanes agree on the duplicate verdict");
    assert!(!report.passed(), "a duplicate snapshot name fails the run");
    let duplicates = report
        .plain
        .checks
        .iter()
        .filter(|c| matches!(snapshot_outcome(c), SnapshotOutcome::DuplicateName))
        .count();
    assert_eq!(duplicates, 1, "exactly one duplicate is flagged");
}

#[test]
fn snapshot_mismatch_is_a_red_check_with_expected_context() {
    const SRC: &str = r#"#[test]
fn m() -> Stream<Check> {
    yield expect_snapshot(1, "n");
}
"#;
    let oracle = SnapshotExpectations::new().with("m", "n", "2");
    let checks = snapshot_checks(SRC, &oracle);
    assert!(matches!(
        snapshot_outcome(&checks[0]),
        SnapshotOutcome::Mismatch { expected } if expected == "2"
    ));
    assert!(!checks[0].passed);
}

#[test]
fn snapshot_rendering_agrees_across_native_and_interpreter_lanes() {
    if !weavy::jit::task_lane::available() {
        return;
    }

    // Select both authorities per executable, without mutating process-global
    // environment state, and assert byte-identical oracle-backed renderings.
    let oracle = SnapshotExpectations::new().with("snapshot_record", "dep-mio", DEP_MIO_GOLDEN);
    let interpreter =
        run_source_with_snapshots_and_lane(RUNG_060, &oracle, LaneRequest::Interpreter)
            .expect("interpreter lane runs");
    let native = run_source_with_snapshots_and_lane(RUNG_060, &oracle, LaneRequest::Native)
        .expect("native lane runs");
    let interp_rendered = &interpreter.plain.checks[0]
        .snapshot
        .as_ref()
        .unwrap()
        .rendered;
    let native_rendered = &native.plain.checks[0].snapshot.as_ref().unwrap().rendered;
    assert_eq!(
        interp_rendered, native_rendered,
        "native and interpreter lanes render byte-identically"
    );
    assert_eq!(interp_rendered, DEP_MIO_GOLDEN);
    assert!(interpreter.passed() && native.passed());
}

#[test]
fn expect_snapshot_consumes_one_pair_of_value_and_string_name() {
    // The calling convention is one pair (T, String) — juxtaposition applied to a
    // grouped tuple, per the Calling chapter. Prove it type-checks with a varied
    // subject T and a String second component, and rejects a non-literal name.
    const OK: &str = r#"struct P { x: Int, y: Int }
#[test]
fn pair() -> Stream<Check> {
    yield expect_snapshot(P { x: 1, y: 2 }, "rec");
    yield expect_snapshot(7, "int");
}
"#;
    let oracle = SnapshotExpectations::new()
        .with("pair", "rec", "P {\n    x: 1,\n    y: 2,\n}")
        .with("pair", "int", "7");
    let report = run_source_with_snapshots(OK, &oracle).expect("pair form compiles and runs");
    assert!(report.passed(), "the tuple (value, name) form is accepted");

    // The name component must be a compile-time String literal, not a runtime
    // value: a bound identifier in name position is a typed compile error.
    const BAD: &str = r#"#[test]
fn bad() -> Stream<Check> {
    let n = "x";
    yield expect_snapshot(1, n);
}
"#;
    assert!(
        matches!(run_source(BAD), Err(RunError::Diagnostics(_))),
        "a non-literal snapshot name is rejected at compile time"
    );
}

// ---------------------------------------------------------------------------
// Rungs 071–077 (skipping the parallel exec band's 073/074): trees, glob,
// pinned fetch, fetch memoization, archive extraction. These certificates sit
// above the current consecutive red boundary (066/067+) — progress, not score.
// ---------------------------------------------------------------------------

/// Every rung of the tree/fetch band compiles through the production surface
/// grammar and checker into typed VIR: the machine-plane primitives
/// (`fixture_tree`, `/` projection, `.text()`, `.glob().collect()`,
/// `fixture_registry().url()`, `fetch`, `untar`, `.len()`) and the
/// `never_read` / `fetched` trace descriptors all resolve.
#[test]
fn tree_fetch_band_compiles_to_typed_vir() {
    for (rung, source) in [
        ("071", RUNG_071),
        ("072", RUNG_072),
        ("075", RUNG_075),
        ("076", RUNG_076),
        ("077", RUNG_077),
    ] {
        let module = Compiler::new()
            .compile(source)
            .unwrap_or_else(|error| panic!("rung {rung} compiles to VIR: {error:?}"));
        assert_eq!(module.tests.len(), 1, "rung {rung} declares one test");
    }
}

/// The rungs execute through the production runner: effect islands use the
/// store/memo/receipt plane while their consumers remain ordinary verified
/// Weavy islands. Each source is run in both plain and chaos lanes by
/// `run_source`; the in-language `never_read` and `fetched` checks are the
/// externally visible certificate.
#[test]
fn rung_071_runs_through_effect_plane() {
    assert_effect_rung("071", RUNG_071);
}

#[test]
fn rung_072_runs_through_effect_plane() {
    assert_effect_rung("072", RUNG_072);
}

#[test]
fn rung_075_runs_through_effect_plane() {
    assert_effect_rung("075", RUNG_075);
}

#[test]
fn rung_076_runs_through_effect_plane() {
    assert_effect_rung("076", RUNG_076);
}

#[test]
fn rung_077_runs_through_effect_plane() {
    assert_effect_rung("077", RUNG_077);
}

fn assert_effect_rung(rung: &str, source: &str) {
    let report = run_source(source)
        .unwrap_or_else(|error| panic!("rung {rung} runs through the effect plane: {error:?}"));
    assert!(
        report.passed(),
        "rung {rung} agrees across plain and chaos: {report:#?}"
    );
}

// ---------------------------------------------------------------------------
// Rungs 078-082: receipts, cross-run reuse, projection reuse, and rerun audit.
// ---------------------------------------------------------------------------

#[test]
fn rung_078_receipts_record_tree_and_decode_reads() {
    let report = run_source(RUNG_078).expect("rung 078 runs through production receipts");
    assert!(report.passed(), "rung 078 report: {report:#?}");
    for lane in [&report.plain, &report.chaos] {
        assert!(
            lane.receipt_count >= 2,
            "tree projection and typed TOML decode both record receipts: {lane:#?}",
        );
        assert!(lane.checks.iter().all(|check| check.passed));
    }
}

#[test]
fn rung_079_cross_run_reuses_without_recompute() {
    let audit = run_source_rerun_audit(RUNG_079).expect("rung 079 rerun audit executes");
    assert!(
        audit.second.checks.iter().all(|check| check.passed),
        "second run trace checks pass: {audit:#?}",
    );
    assert_eq!(
        audit.second.counters.memo_misses, 0,
        "second run recomputes nothing"
    );
    assert!(
        audit.second.counters.memo_hits_exact > 0,
        "second run is served by memo hits"
    );
    assert!(!audit.nondeterministic, "pure rerun stays deterministic");
}

#[test]
fn rung_080_early_cutoff_reuses_downstream_render() {
    let audit = run_source_rerun_audit(RUNG_080).expect("rung 080 rerun audit executes");
    assert!(
        audit.second.checks.iter().all(|check| check.passed),
        "second run trace checks pass: {audit:#?}",
    );
    assert!(
        audit.second.counters.memo_hits_exact + audit.second.counters.memo_hits_projection > 0,
        "rerun uses memo/projection hits: {:?}",
        audit.second.counters,
    );
}

#[test]
fn rung_081_projection_reuses_build_step_when_unread_path_changes() {
    let audit = run_source_rerun_audit(RUNG_081).expect("rung 081 rerun audit executes");
    assert!(
        audit.second.checks.iter().all(|check| check.passed),
        "second run trace checks pass: {audit:#?}",
    );
    assert!(
        audit.second.counters.memo_hits_projection > 0,
        "changed tree outside witnessed read projection is a projection hit: {:?}",
        audit.second.counters,
    );
}

#[test]
fn rung_082_rerun_audit_flags_nondeterministic_exec() {
    let audit = run_source_rerun_audit(RUNG_082).expect("rung 082 rerun audit executes");
    assert!(
        audit.nondeterministic,
        "authoritative rerun output/value families diverge: {audit:#?}",
    );
}

#[test]
fn incrementality_band_agrees_across_native_and_interpreter_lanes() {
    if !weavy::jit::task_lane::available() {
        return;
    }
    for (name, source) in [
        ("078", RUNG_078),
        ("079", RUNG_079),
        ("080", RUNG_080),
        ("081", RUNG_081),
    ] {
        let native = run_source_rerun_audit_with_lane(source, LaneRequest::Native)
            .unwrap_or_else(|error| panic!("rung {name} native rerun audit: {error:?}"));
        let interp = run_source_rerun_audit_with_lane(source, LaneRequest::Interpreter)
            .unwrap_or_else(|error| panic!("rung {name} interpreter rerun audit: {error:?}"));
        assert_eq!(
            native.second.check_family(),
            interp.second.check_family(),
            "rung {name} second-run check family diverges across lanes",
        );
        assert_eq!(
            native.second.value_family(),
            interp.second.value_family(),
            "rung {name} second-run value family diverges across lanes",
        );
    }
}

// ---------------------------------------------------------------------------
// Rungs 101-105: code-edit early cutoff and lookup-not-recompute discipline.
// ---------------------------------------------------------------------------

#[test]
fn rung_101_body_edit_early_cutoff_reuses_downstream_value() {
    let audit = run_source_revision_audit(RUNG_101, RUNG_101_V2)
        .expect("rung 101 source-revision audit executes");
    assert!(
        audit.second.checks.iter().all(|check| check.passed),
        "rung 101 second revision passes: {audit:#?}",
    );
    assert!(
        audit.load.claims_loaded > 0,
        "durable journal loads reusable claims: {audit:#?}",
    );
    assert!(
        audit.second.counters.memo_hits_exact
            + audit.second.counters.memo_hits_projection
            + audit.second.counters.memo_hits_semantic
            > 0,
        "second revision uses memo lookup/revalidation: {:?}",
        audit.second.counters,
    );
    assert!(
        audit.second.counters.memo_hits_semantic > 0,
        "source-revision location churn is bridged by suffix-nominated semantic hits: {:?}",
        audit.second.counters,
    );
    assert_eq!(
        audit.second.counters.memo_misses, 1,
        "101 recomputes exactly one changed source-revision demand, but not downstream render: {:?}",
        audit.second.counters,
    );
    assert_eq!(
        audit.second.counters.memo_hits_semantic, 1,
        "101 has exactly one suffix-nominated downstream hit: {:?}",
        audit.second.counters,
    );
    assert!(!audit.nondeterministic, "{audit:#?}");
}

#[test]
fn rung_102_body_edit_negative_control_recomputes_downstream_value() {
    let positive = run_source_revision_audit(RUNG_101, RUNG_101_V2)
        .expect("rung 101 source-revision audit executes");
    let control = run_source_revision_audit(RUNG_102, RUNG_102_V2)
        .expect("rung 102 source-revision audit executes");
    assert!(
        control.second.checks.iter().all(|check| check.passed),
        "rung 102 second revision passes: {control:#?}",
    );
    assert!(
        control.second.counters.memo_misses > positive.second.counters.memo_misses,
        "changed helper value forces more recomputation than 101: positive={:?} control={:?}",
        positive.second.counters,
        control.second.counters,
    );
    assert_eq!(
        control.second.counters.memo_misses, 2,
        "102 recomputes the changed source-revision demand and downstream render: {:?}",
        control.second.counters,
    );
    assert_eq!(
        control.second.counters.memo_hits_semantic, 0,
        "102's changed value leaves no accepted downstream suffix candidate: {:?}",
        control.second.counters,
    );
}

#[test]
fn rung_103_rename_location_churn_is_cold_but_correct() {
    let audit = run_source_revision_audit(RUNG_103, RUNG_103_V2)
        .expect("rung 103 source-revision audit executes");
    assert!(
        audit.second.checks.iter().all(|check| check.passed),
        "rung 103 second revision passes despite cold churn: {audit:#?}",
    );
    assert!(
        audit.second.counters.memo_misses > 0,
        "rename churn is deliberately cold: {:?}",
        audit.second.counters,
    );
}

#[test]
fn rung_104_wrapper_refactor_uses_suffix_nomination_without_leaf_demand() {
    let audit = run_source_revision_audit(RUNG_104, RUNG_104_V2)
        .expect("rung 104 source-revision audit executes");
    assert!(
        audit.second.checks.iter().all(|check| check.passed),
        "rung 104 second revision passes: {audit:#?}",
    );
    assert!(
        audit.second.counters.memo_hits_semantic > 0,
        "wrapper refactor uses suffix-nominated semantic memo hits: {:?}",
        audit.second.counters,
    );
}

#[test]
fn rung_105_warm_reuse_is_lookup_not_recompute_and_compare() {
    let audit =
        run_source_revision_audit(RUNG_105, RUNG_105).expect("rung 105 warm source audit executes");
    assert!(
        audit.second.checks.iter().all(|check| check.passed),
        "rung 105 trace checks pass on the warm run: {audit:#?}",
    );
    assert_eq!(
        audit.second.counters.memo_misses, 0,
        "warm source reuse performs no recomputation: {:?}",
        audit.second.counters,
    );
    assert!(
        audit.second.counters.memo_hits_exact
            + audit.second.counters.memo_hits_projection
            + audit.second.counters.memo_hits_semantic
            >= 1,
        "warm source reuse is proven by memo hit counters: {:?}",
        audit.second.counters,
    );
}

#[test]
fn source_revision_band_agrees_across_native_and_interpreter_lanes() {
    if !weavy::jit::task_lane::available() {
        return;
    }
    for (name, first, second) in [
        ("101", RUNG_101, RUNG_101_V2),
        ("102", RUNG_102, RUNG_102_V2),
        ("103", RUNG_103, RUNG_103_V2),
        ("104", RUNG_104, RUNG_104_V2),
        ("105", RUNG_105, RUNG_105),
    ] {
        let native = run_source_revision_audit_with_lane(first, second, LaneRequest::Native)
            .unwrap_or_else(|error| panic!("rung {name} native source audit: {error:?}"));
        let interp = run_source_revision_audit_with_lane(first, second, LaneRequest::Interpreter)
            .unwrap_or_else(|error| panic!("rung {name} interpreter source audit: {error:?}"));
        assert_eq!(
            native.second.check_family(),
            interp.second.check_family(),
            "rung {name} second-run check family diverges across lanes",
        );
        assert_eq!(
            native.second.value_family(),
            interp.second.value_family(),
            "rung {name} second-run value family diverges across lanes",
        );
    }
}
