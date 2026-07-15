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

use std::rc::Rc;

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

fn wire(
    arg: &Island,
    lowered: Rc<LoweringArtifact>,
    attribution: LoweringAttribution,
    location: Location,
) -> WireDemand {
    WireDemand {
        island: arg.id,
        location,
        lowered,
        attribution: Rc::new(attribution),
        arguments: Vec::new(),
        wires: Vec::new(),
        function: arg.function,
        // Synthetic argument islands carry no invocation provenance: nothing
        // literal-selectable and no authored preimage to select on.
        demand_arguments: None,
        preimage: String::new(),
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
    let arg_lowered = arg_cache.get_or_lower_owned(&arg).expect("arg lowers");
    let consumer_lowered = consumer_cache
        .get_or_lower_owned(&consumer)
        .expect("consumer lowers");
    let arg_attr = attribution_for(&arg);
    let consumer_attr = attribution_for(&consumer);
    let arg_loc = Location::for_test_value("force", "arg");
    let consumer_loc = Location::for_test_value("force", "consumer");

    let mut runtime = Runtime::new(EventLog::default());
    let wires = vec![wire(&arg, arg_lowered, arg_attr, arg_loc)];
    let result = runtime
        .evaluate(
            consumer.id,
            &consumer_loc,
            consumer_lowered,
            &consumer_attr,
            IslandInputs {
                arguments: Vec::new(),
                wires,
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

/// Production-path certificate: a wire-parked frame resides in Runtime
/// scheduler state — off the recursive Rust stack — while its wire's argument
/// demand runs as a separate scheduler task, and the SAME frame resumes to the
/// realized identity.
///
/// This drives the ordinary `evaluate` loop (no manual stepping, no host
/// callback). It witnesses three facts that only hold when the parked frame is
/// scheduler-resident rather than a suspended Rust call frame:
///
/// 1. `peak_parked_frames` is non-zero — a frame was retained in the scheduler's
///    off-stack parked map during the run;
/// 2. the wire's argument island ran under a *different* `TaskId` than the
///    consumer while the consumer was parked;
/// 3. the same `TaskId` that emitted `WeavyParked` emitted `WeavyResumed`, and
///    the resumed frame produced the realized wire value.
#[test]
fn parked_frame_is_scheduler_resident_off_stack_and_resumes_to_same_identity() {
    let arg = const_island(1, 999);
    let consumer = island(
        2,
        vec![node(0, Type::Int, vec![], Op::AwaitWire { input: 0 })],
        0,
        vec![dummy_wire(0)],
    );

    let mut arg_cache = LoweringCache::default();
    let mut consumer_cache = LoweringCache::default();
    let arg_lowered = arg_cache.get_or_lower_owned(&arg).expect("arg lowers");
    let consumer_lowered = consumer_cache
        .get_or_lower_owned(&consumer)
        .expect("consumer lowers");
    let arg_attr = attribution_for(&arg);
    let consumer_attr = attribution_for(&consumer);
    let arg_loc = Location::for_test_value("force", "arg");
    let consumer_loc = Location::for_test_value("force", "consumer");

    let mut runtime = Runtime::new(EventLog::default());
    let wires = vec![wire(&arg, arg_lowered, arg_attr, arg_loc)];
    let result = runtime
        .evaluate(
            consumer.id,
            &consumer_loc,
            consumer_lowered,
            &consumer_attr,
            IslandInputs {
                arguments: Vec::new(),
                wires,
            },
            ChaosPolicy::default(),
        )
        .expect("the parked frame resumes with the resolved value");

    // (1) A frame was retained in the scheduler's off-stack parked map.
    assert!(
        runtime.counters().peak_parked_frames >= 1,
        "a wire-parked frame was retained in Runtime scheduler state off the recursive stack",
    );

    // (2) The wire's argument island ran under its own scheduler task while the
    // consumer frame was parked — the two frames are distinct scheduler tasks,
    // not one nested Rust stack frame.
    let parked_task = runtime
        .sink()
        .events()
        .iter()
        .find_map(|event| match event.kind {
            EventKind::WeavyParked { task, .. } => Some(task),
            _ => None,
        })
        .expect("the consumer frame parked");
    let arg_task = runtime
        .sink()
        .events()
        .iter()
        .find_map(|event| match event.kind {
            EventKind::IslandEntered { task, island } if island == arg.id => Some(task),
            _ => None,
        })
        .expect("the wire's argument island entered as its own task");
    assert_ne!(
        parked_task, arg_task,
        "the parked consumer frame and its wire's argument ran as distinct scheduler tasks",
    );

    // (3) The SAME frame that parked resumed, and it produced the realized value.
    let resumed_task = runtime
        .sink()
        .events()
        .iter()
        .find_map(|event| match event.kind {
            EventKind::WeavyResumed { task } => Some(task),
            _ => None,
        })
        .expect("the parked frame resumed");
    assert_eq!(
        parked_task, resumed_task,
        "the frame that parked is the frame that resumed",
    );
    assert!(result.failure.is_none());
    assert_eq!(
        runtime.scalar_word(result.handle),
        Some(999),
        "the resumed frame returns the realized wire value",
    );

    // The resumed frame's result identity is stable: a fresh runtime re-running
    // the same demand parks, resumes, and realizes the byte-identical value.
    let mut arg_cache = LoweringCache::default();
    let mut consumer_cache = LoweringCache::default();
    let arg_lowered = arg_cache.get_or_lower_owned(&arg).expect("arg lowers");
    let consumer_lowered = consumer_cache
        .get_or_lower_owned(&consumer)
        .expect("consumer lowers");
    let arg_attr = attribution_for(&arg);
    let consumer_attr = attribution_for(&consumer);
    let arg_loc = Location::for_test_value("force", "arg");
    let consumer_loc = Location::for_test_value("force", "consumer");
    let mut replay = Runtime::new(EventLog::default());
    let wires = vec![wire(&arg, arg_lowered, arg_attr, arg_loc)];
    let replayed = replay
        .evaluate(
            consumer.id,
            &consumer_loc,
            consumer_lowered,
            &consumer_attr,
            IslandInputs {
                arguments: Vec::new(),
                wires,
            },
            ChaosPolicy::default(),
        )
        .expect("the replay parks and resumes");
    assert_eq!(
        result.identity, replayed.identity,
        "the parked-then-resumed frame resolves to the same realized identity",
    );
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
    let arg_lowered = arg_cache.get_or_lower_owned(&arg).expect("arg lowers");
    let consumer_lowered = consumer_cache
        .get_or_lower_owned(&consumer)
        .expect("consumer lowers");
    let arg_attr = attribution_for(&arg);
    let consumer_attr = attribution_for(&consumer);
    let arg_loc = Location::for_test_value("force", "arg");
    let consumer_loc = Location::for_test_value("force", "consumer");

    let mut runtime = Runtime::new(EventLog::default());
    let wires = vec![wire(&arg, arg_lowered, arg_attr, arg_loc)];
    let result = runtime
        .evaluate(
            consumer.id,
            &consumer_loc,
            consumer_lowered,
            &consumer_attr,
            IslandInputs {
                arguments: Vec::new(),
                wires,
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
    let arg_lowered = arg_cache.get_or_lower_owned(&arg).expect("arg lowers");
    let consumer_lowered = consumer_cache
        .get_or_lower_owned(&consumer)
        .expect("consumer lowers");
    let arg_attr = attribution_for(&arg);
    let consumer_attr = attribution_for(&consumer);
    let arg_loc = Location::for_test_value("force", "arg");
    let consumer_loc = Location::for_test_value("force", "consumer");

    let mut runtime = Runtime::new(EventLog::default());
    // Both wire inputs resolve the SAME argument island (same location/preimage).
    let wires = vec![
        wire(
            &arg,
            Rc::clone(&arg_lowered),
            arg_attr.clone(),
            arg_loc.clone(),
        ),
        wire(&arg, arg_lowered, arg_attr, arg_loc),
    ];
    let result = runtime
        .evaluate(
            consumer.id,
            &consumer_loc,
            consumer_lowered,
            &consumer_attr,
            IslandInputs {
                arguments: Vec::new(),
                wires,
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
    let lowered = cache
        .get_or_lower_owned(&consumer)
        .expect("consumer lowers");
    let attribution = attribution_for(&consumer);
    let consumer_loc = Location::for_test_value("force", "cycle");
    let wire_loc = Location::for_test_value("force", "cycle-wire");

    let mut runtime = Runtime::new(EventLog::default());
    // The wire resolves the SAME island — the consumer's own demand key — so
    // forcing it re-enters a demand already `Running` on the stack.
    let wires = vec![WireDemand {
        island: consumer.id,
        location: wire_loc,
        lowered: Rc::clone(&lowered),
        attribution: Rc::new(attribution.clone()),
        arguments: Vec::new(),
        wires: Vec::new(),
        function: consumer.function,
        demand_arguments: None,
        preimage: String::new(),
    }];
    let error = runtime
        .evaluate(
            consumer.id,
            &consumer_loc,
            lowered,
            &attribution,
            IslandInputs {
                arguments: Vec::new(),
                wires,
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
