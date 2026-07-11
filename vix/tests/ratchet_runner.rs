use vix::compiler::Compiler;
use vix::diagnostic::{DiagnosticCode, DiagnosticSeverity};
use vix::lowering::{LoweringCache, attribution_for, source_map_for};
use vix::ratchet::run_source;
use vix::runtime::{DemandState, EventKind, MemoVerdict, SchemaId, TaskState};
use vix::surface::{SurfaceParser, ast};
use vix::vir::{EffectKind, FunctionId, NodeRef, Op as VirOp, Type as VirType, VariantPayload};
use weavy::task::Op as WeavyOp;
use weavy::{PayloadKind, RegionShape, ValueShapeKind, WordKind};

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
const RUNG_032: &str = include_str!("ratchet/032-pop.reject.vix");
const RUNG_041: &str = include_str!("ratchet/041-maps.vix");
const RUNG_042: &str = include_str!("ratchet/042-map-overwrite.vix");
const RUNG_043: &str = include_str!("ratchet/043-map-keys-canonical.vix");
const RUNG_044: &str = include_str!("ratchet/044-sets.vix");
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
                assert_eq!(constant.root.slot.byte_offset(), 0);
                assert_eq!(constant.owner.slot.byte_offset(), 0);
            } else {
                assert_eq!(constant.owner.function, constant.node.function);
                assert_ne!(constant.owner.function, root_function);
                assert_eq!(constant.root.entry, root_entry);
                assert_eq!(constant.owner.entry, 1);
                assert_eq!(constant.root.slot.byte_offset(), 24);
                assert_eq!(constant.owner.slot.byte_offset(), 8);
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
        assert_eq!(root_call[1].dst, 8);
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
        RUNG_021, RUNG_022, RUNG_023, RUNG_024, RUNG_025,
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
        let shape = lowered.contract().functions[0].frame.regions[region]
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
        assert!(lane.events.iter().all(|event| match event.kind {
            EventKind::StoreAlloc { identity, .. } => {
                identity.schema == SchemaId::named("vix.Check.v1")
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
        assert!(lane.events.iter().all(|event| match event.kind {
            EventKind::StoreAlloc { identity, .. } => {
                identity.schema == SchemaId::named("vix.Check.v1")
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
fn unused_collection_results(
    xs: [Int],
    map: Map<String, Int>,
    set: Set<Int>,
) -> Int {
    let array_all = xs ++ xs;
    let map_one = map + ("x", 1);
    let map_all = map ++ map;
    let rebound = map.with ("x", 1);
    let set_one = set + 1;
    let set_all = set ++ set;
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

    let partitioned = map_compilation.partition_test(&map_compilation.tests[0]);
    let mut lowering_cache = LoweringCache::default();
    let error = match lowering_cache.get_or_lower(&partitioned.islands[0]) {
        Ok(_) => panic!("map lowering remains the next runtime boundary"),
        Err(vix::lowering::LoweringError::Diagnostics(diagnostics)) => diagnostics,
        Err(vix::lowering::LoweringError::Machine(error)) => {
            panic!("unexpected verifier failure: {error:?}")
        }
    };
    assert_eq!(error.entries.len(), 1);
    assert_eq!(error.entries[0].code, DiagnosticCode::LoweringUnsupported);
    assert_eq!(
        error.entries[0].message(),
        "map/set lowering is not implemented"
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
