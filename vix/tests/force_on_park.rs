//! Force-on-park protocol certificate.
//!
//! A verified task that reaches an `AwaitWire` on an unresolved canonical wire
//! parks and returns control fully to the scheduler (its frame arena is the
//! owned suspended state, so every task/store/value-memory borrow is released).
//! The scheduler resolves the wire's argument `DemandPreimage` through the
//! existing memo state machine, then resumes the SAME interpreter or native
//! task with the realized typed value. This exercises the seam directly by
//! constructing the verified islands — no host callback, no pre-resolution, no
//! second memo, no lane fallback.

use vix::lowering::{LoweringArtifact, LoweringAttribution, LoweringCache, attribution_for};
use vix::runtime::{
    ChaosPolicy, EventKind, EventLog, FailureValue, IslandInputs, Location, MachineCause, Runtime,
    RuntimeFault, WireDemand,
};
use vix::support::Span;
use vix::vir::{
    EffectFacts, FunctionId, Island, IslandId, IslandPurpose, Node, NodeId, Op, Type, ValueIslandId,
};

fn node(id: u32, ty: Type, inputs: Vec<NodeId>, op: Op) -> Node {
    Node {
        id: NodeId(id),
        span: Span { start: 0, end: 0 },
        ty,
        effect: EffectFacts::PURE,
        inputs,
        op,
    }
}

fn island(id: u32, nodes: Vec<Node>, output: u32, wire_inputs: Vec<ValueIslandId>) -> Island {
    Island {
        id: IslandId(id),
        purpose: IslandPurpose::Value,
        function: FunctionId(id),
        function_name: format!("island{id}"),
        parameters: Vec::new(),
        value_inputs: Vec::new(),
        wire_inputs,
        effect_inputs: Vec::new(),
        forced_copy_value: false,
        nodes,
        output: NodeId(output),
        callees: Vec::new(),
        array_map_partitions: Vec::new(),
    }
}

fn const_island(id: u32, value: i64) -> Island {
    island(
        id,
        vec![node(0, Type::Int, vec![], Op::Int(value))],
        0,
        Vec::new(),
    )
}

/// `1 / 0` — a checked division that fails on the language plane.
fn division_by_zero_island(id: u32) -> Island {
    island(
        id,
        vec![
            node(0, Type::Int, vec![], Op::Int(1)),
            node(1, Type::Int, vec![], Op::Int(0)),
            node(2, Type::Int, vec![NodeId(0), NodeId(1)], Op::Div),
        ],
        2,
        Vec::new(),
    )
}

/// The dummy value-island id filling a consumer's `wire_inputs`; the scheduler
/// resolves through the `WireDemand` table, so only the count matters here.
fn dummy_wire(index: u32) -> ValueIslandId {
    ValueIslandId {
        function: FunctionId(1000),
        node: NodeId(index),
    }
}

fn wire<'a>(
    arg: &Island,
    lowered: &'a LoweringArtifact,
    attribution: &'a LoweringAttribution,
    location: &'a Location,
) -> WireDemand<'a> {
    WireDemand {
        island: arg.id,
        location,
        lowered,
        attribution,
        arguments: &[],
        wires: &[],
        function: arg.function,
        demand_arguments: &[],
    }
}

/// A single unresolved wire parks the task; the scheduler resolves it through
/// the memo path and resumes the same task with the realized value.
#[test]
fn await_wire_parks_resolves_and_resumes() {
    let arg = const_island(1, 999);
    let consumer = island(
        2,
        vec![node(0, Type::Int, vec![], Op::AwaitWire { input: 0 })],
        0,
        vec![dummy_wire(0)],
    );

    let mut arg_cache = LoweringCache::default();
    let mut consumer_cache = LoweringCache::default();
    let arg_lowered = arg_cache.get_or_lower(&arg).expect("arg lowers");
    let consumer_lowered = consumer_cache
        .get_or_lower(&consumer)
        .expect("consumer lowers");
    let arg_attr = attribution_for(&arg);
    let consumer_attr = attribution_for(&consumer);
    let arg_loc = Location::for_test_value("force", "arg");
    let consumer_loc = Location::for_test_value("force", "consumer");

    let mut runtime = Runtime::new(EventLog::default());
    let wires = [wire(&arg, arg_lowered, &arg_attr, &arg_loc)];
    let result = runtime
        .evaluate(
            consumer.id,
            &consumer_loc,
            consumer_lowered,
            &consumer_attr,
            IslandInputs {
                arguments: &[],
                wires: &wires,
            },
            ChaosPolicy::default(),
        )
        .expect("the parked task resumes with the resolved value");
    assert!(result.failure.is_none());
    assert_eq!(
        runtime.scalar_word(result.handle),
        Some(999),
        "the resumed task returns the realized wire value",
    );
    let parked = runtime
        .sink()
        .events()
        .iter()
        .any(|event| matches!(event.kind, EventKind::WeavyParked { .. }));
    let resumed = runtime
        .sink()
        .events()
        .iter()
        .any(|event| matches!(event.kind, EventKind::WeavyResumed { .. }));
    assert!(parked, "the task actually parked");
    assert!(resumed, "the scheduler resumed the same task");
}

/// A selected division-by-zero argument propagates `DivisionByZero` with its
/// authored source site to the parent demand.
#[test]
fn forced_division_by_zero_wire_propagates_the_typed_failure() {
    let arg = division_by_zero_island(1);
    let consumer = island(
        2,
        vec![node(0, Type::Int, vec![], Op::AwaitWire { input: 0 })],
        0,
        vec![dummy_wire(0)],
    );

    let mut arg_cache = LoweringCache::default();
    let mut consumer_cache = LoweringCache::default();
    let arg_lowered = arg_cache.get_or_lower(&arg).expect("arg lowers");
    let consumer_lowered = consumer_cache
        .get_or_lower(&consumer)
        .expect("consumer lowers");
    let arg_attr = attribution_for(&arg);
    let consumer_attr = attribution_for(&consumer);
    let arg_loc = Location::for_test_value("force", "arg");
    let consumer_loc = Location::for_test_value("force", "consumer");

    let mut runtime = Runtime::new(EventLog::default());
    let wires = [wire(&arg, arg_lowered, &arg_attr, &arg_loc)];
    let result = runtime
        .evaluate(
            consumer.id,
            &consumer_loc,
            consumer_lowered,
            &consumer_attr,
            IslandInputs {
                arguments: &[],
                wires: &wires,
            },
            ChaosPolicy::default(),
        )
        .expect("a failing wire is a language failure, never a machine crash");
    assert!(
        matches!(result.failure, Some(FailureValue::DivisionByZero { .. })),
        "the forced wire propagated its typed failure, got {:?}",
        result.failure,
    );
    assert!(
        result.failure_context.is_some(),
        "the propagated failure keeps its authored source site",
    );
}

/// Two awaits of the same wire evaluate the argument once (shared memo identity)
/// and resume both consumers with the same realized value.
#[test]
fn repeated_force_of_one_wire_evaluates_once() {
    let arg = const_island(1, 21);
    let consumer = island(
        2,
        vec![
            node(0, Type::Int, vec![], Op::AwaitWire { input: 0 }),
            node(1, Type::Int, vec![], Op::AwaitWire { input: 1 }),
            node(2, Type::Int, vec![NodeId(0), NodeId(1)], Op::Add),
        ],
        2,
        vec![dummy_wire(0), dummy_wire(1)],
    );

    let mut arg_cache = LoweringCache::default();
    let mut consumer_cache = LoweringCache::default();
    let arg_lowered = arg_cache.get_or_lower(&arg).expect("arg lowers");
    let consumer_lowered = consumer_cache
        .get_or_lower(&consumer)
        .expect("consumer lowers");
    let arg_attr = attribution_for(&arg);
    let consumer_attr = attribution_for(&consumer);
    let arg_loc = Location::for_test_value("force", "arg");
    let consumer_loc = Location::for_test_value("force", "consumer");

    let mut runtime = Runtime::new(EventLog::default());
    // Both wire inputs resolve the SAME argument island (same location/preimage).
    let wires = [
        wire(&arg, arg_lowered, &arg_attr, &arg_loc),
        wire(&arg, arg_lowered, &arg_attr, &arg_loc),
    ];
    let result = runtime
        .evaluate(
            consumer.id,
            &consumer_loc,
            consumer_lowered,
            &consumer_attr,
            IslandInputs {
                arguments: &[],
                wires: &wires,
            },
            ChaosPolicy::default(),
        )
        .expect("both awaits resume from the same memo identity");
    assert_eq!(
        runtime.scalar_word(result.handle),
        Some(42),
        "both forces observe the one realized value (21 + 21)",
    );
    // The argument island's own frame is entered exactly once; the second force
    // is a memo hit, not a recomputation.
    let arg_frames = runtime
        .sink()
        .events()
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                EventKind::WeavyFrameEntered { function, .. } if function == arg.function
            )
        })
        .count();
    assert_eq!(arg_frames, 1, "the shared argument computed exactly once");
}

/// A wire that forces the demand already being evaluated on the stack is a
/// cyclic demand: the demand state machine detects the re-entrant `Running`
/// demand and returns a typed fault instead of recursing forever.
#[test]
fn reentrant_wire_demand_is_a_typed_fault() {
    let consumer = island(
        2,
        vec![node(0, Type::Int, vec![], Op::AwaitWire { input: 0 })],
        0,
        vec![dummy_wire(0)],
    );
    let mut cache = LoweringCache::default();
    let lowered = cache.get_or_lower(&consumer).expect("consumer lowers");
    let attribution = attribution_for(&consumer);
    let consumer_loc = Location::for_test_value("force", "cycle");
    let wire_loc = Location::for_test_value("force", "cycle-wire");

    let mut runtime = Runtime::new(EventLog::default());
    // The wire resolves the SAME island — the consumer's own demand key — so
    // forcing it re-enters a demand already `Running` on the stack.
    let wires = [WireDemand {
        island: consumer.id,
        location: &wire_loc,
        lowered,
        attribution: &attribution,
        arguments: &[],
        wires: &[],
        function: consumer.function,
        demand_arguments: &[],
    }];
    let error = runtime
        .evaluate(
            consumer.id,
            &consumer_loc,
            lowered,
            &attribution,
            IslandInputs {
                arguments: &[],
                wires: &wires,
            },
            ChaosPolicy::default(),
        )
        .expect_err("a cyclic wire demand is a typed fault, not an infinite loop");
    assert!(
        matches!(
            error.cause,
            MachineCause::Runtime(RuntimeFault::ReentrantDemand { .. })
        ),
        "the cycle is a typed re-entrant demand fault, got {:?}",
        error.cause,
    );
}
