//! The demand graph and its driver: nothing forces locally.
//!
//! Nodes are plain data in a per-evaluation working graph (positional
//! ids, dies with the evaluation — persistence happens at
//! content-addressed boundaries only, never here). Building the graph
//! computes nothing. Demand enters at [`Graph::demand`] and
//! backpropagates: a body node needs ALL its inputs jointly (parallel
//! fan-out is discovered, not annotated), a cond node needs its
//! scrutinee and then exactly one arm, and external inputs park the
//! evaluation — the pending frontier is enumerable at any moment
//! (constitution §6: that enumerability is what the fleet, the
//! debugger, and the weavy suspension story all stand on).
//!
//! Node bodies run on a lane that follows weavy::jit::async's
//! suspend/resume protocol exactly — explicit operand stack, resume
//! cursor, awaits numbered in program order, values arriving through a
//! readiness array, suspend by returning up, resume by re-entry. That
//! file's own docs call the protocol the reusable part; the op set
//! here speaks [`Slot`] instead of i64 and merges back into weavy as
//! the stencil lane lands (deferrable by ruling; the lane below is the
//! interpreter lane and remains the reference forever).
//!
//! The driver is written as a poll-shaped step function over a ready
//! worklist. Workers and work-stealing arrive by making that worklist
//! shared — legal because scheduling order is unobservable by
//! construction (canonical total order) — without changing any node's
//! semantics.

use super::value::Slot;

/// Identifies a node in one evaluation's working graph. Positional;
/// meaningless outside the graph that issued it; never persisted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u32);

/// Identifies an external input boundary (a run, in the full system:
/// an exec, a fetch, a solve). The frontier is reported in these.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputId(u32);

/// One op in a node body. Deliberately tiny: enough to prove the
/// protocol over [`Slot`]s; grows with the lowering, merges into
/// weavy's op vocabulary when the stencil lane lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeOp {
    /// Push an immediate.
    Push(Slot),
    /// Suspend point: consume the next input node's resolved value, in
    /// declaration order.
    Await,
    /// Pop two I64s, push their sum.
    AddI64,
    /// Pop two I64s, push their product.
    MulI64,
}

/// A node: plain data, no host closures (the weavy-lowering shape
/// constraint — a node must be shippable, hashable, resumable).
#[derive(Clone, Debug)]
enum Node {
    Const(Slot),
    /// An external boundary: resolved only by [`Graph::provide`].
    Input(InputId),
    /// A body over inputs; each `Await` consumes the next input.
    Body {
        ops: Box<[NodeOp]>,
        inputs: Box<[NodeId]>,
    },
    /// Dynamic demand: the scrutinee decides which arm exists for this
    /// evaluation; the other arm is never demanded (control flow lives
    /// in the graph).
    Cond {
        scrutinee: NodeId,
        if_true: NodeId,
        if_false: NodeId,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum MachineError {
    TypeMismatch { op: &'static str, got: Slot },
    StackUnderflow,
    NoResult,
    CondScrutineeNotBool { got: Slot },
}

impl core::fmt::Display for MachineError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            MachineError::TypeMismatch { op, got } => {
                write!(f, "{op} expects I64 operands, got {got:?}")
            }
            MachineError::StackUnderflow => write!(f, "operand stack underflow"),
            MachineError::NoResult => write!(f, "body finished with an empty stack"),
            MachineError::CondScrutineeNotBool { got } => {
                write!(f, "cond scrutinee must be Bool, got {got:?}")
            }
        }
    }
}

impl std::error::Error for MachineError {}

/// A lane running one body: owns its resume cursor and operand stack,
/// suspends by returning, resumes by re-entry — the weavy protocol.
#[derive(Debug)]
struct Lane {
    cursor: usize,
    next_await: usize,
    stack: Vec<Slot>,
}

enum LaneStep {
    Done(Slot),
    /// Parked on an await; the lane's own cursor state knows which
    /// (`next_await`), so the step needn't carry it.
    Suspended,
}

impl Lane {
    fn new() -> Self {
        Self {
            cursor: 0,
            next_await: 0,
            stack: Vec::new(),
        }
    }

    /// Run from the resume point. `awaited[i]` must hold the value for
    /// await point `i` once known; `ready` marks which are known.
    fn run(
        &mut self,
        ops: &[NodeOp],
        ready: &[bool],
        awaited: &[Slot],
    ) -> Result<LaneStep, MachineError> {
        while self.cursor < ops.len() {
            match ops[self.cursor] {
                NodeOp::Push(slot) => self.stack.push(slot),
                NodeOp::Await => {
                    let ix = self.next_await;
                    if !ready[ix] {
                        // Suspend: state stays right here in the lane;
                        // re-entry resumes at this op.
                        return Ok(LaneStep::Suspended);
                    }
                    self.stack.push(awaited[ix]);
                    self.next_await += 1;
                }
                NodeOp::AddI64 => self.binary_i64("AddI64", |a, b| a.wrapping_add(b))?,
                NodeOp::MulI64 => self.binary_i64("MulI64", |a, b| a.wrapping_mul(b))?,
            }
            self.cursor += 1;
        }
        self.stack.pop().map(LaneStep::Done).ok_or(MachineError::NoResult)
    }

    fn binary_i64(
        &mut self,
        op: &'static str,
        f: impl Fn(i64, i64) -> i64,
    ) -> Result<(), MachineError> {
        let rhs = self.stack.pop().ok_or(MachineError::StackUnderflow)?;
        let lhs = self.stack.pop().ok_or(MachineError::StackUnderflow)?;
        match (lhs, rhs) {
            (Slot::I64(a), Slot::I64(b)) => {
                self.stack.push(Slot::I64(f(a, b)));
                Ok(())
            }
            (Slot::I64(_), other) | (other, _) => {
                Err(MachineError::TypeMismatch { op, got: other })
            }
        }
    }
}

/// Per-node evaluation state. Absent until demanded — a node no
/// selection depends on never gets one (and never runs).
#[derive(Debug)]
enum NodeState {
    /// Demanded, waiting on dependencies (body lane parked or cond
    /// scrutinee unresolved).
    Pending(Option<Lane>),
    Resolved(Slot),
}

/// One evaluation's working graph plus its demand state.
#[derive(Debug, Default)]
pub struct Graph {
    nodes: Vec<Node>,
    states: Vec<Option<NodeState>>,
    inputs: Vec<Option<Slot>>,
    /// How many body lanes ran to completion — the "work happened"
    /// counter the build-computes-nothing and computes-once tests pin.
    executions: usize,
}

impl Graph {
    pub fn new() -> Self {
        Self::default()
    }

    fn add(&mut self, node: Node) -> NodeId {
        let id = NodeId(u32::try_from(self.nodes.len()).expect("node count fits u32"));
        self.nodes.push(node);
        self.states.push(None);
        id
    }

    pub fn constant(&mut self, slot: Slot) -> NodeId {
        self.add(Node::Const(slot))
    }

    pub fn input(&mut self) -> (NodeId, InputId) {
        let input = InputId(u32::try_from(self.inputs.len()).expect("input count fits u32"));
        self.inputs.push(None);
        (self.add(Node::Input(input)), input)
    }

    pub fn body(&mut self, ops: impl Into<Box<[NodeOp]>>, inputs: impl Into<Box<[NodeId]>>) -> NodeId {
        self.add(Node::Body {
            ops: ops.into(),
            inputs: inputs.into(),
        })
    }

    pub fn cond(&mut self, scrutinee: NodeId, if_true: NodeId, if_false: NodeId) -> NodeId {
        self.add(Node::Cond {
            scrutinee,
            if_true,
            if_false,
        })
    }

    /// An external input's value arrived (a run finished). The next
    /// [`Graph::demand`] step picks it up; in the driver-as-runtime
    /// this is where a waker fires.
    pub fn provide(&mut self, input: InputId, slot: Slot) {
        self.inputs[input.0 as usize] = Some(slot);
    }

    /// Body-lane completions so far (the work-happened counter).
    pub fn executions(&self) -> usize {
        self.executions
    }

    /// Everything evaluation is currently blocked on, in stable order.
    /// Enumerable at any moment — the suspension story's foundation.
    pub fn pending_frontier(&self) -> Vec<InputId> {
        let mut frontier: Vec<InputId> = self
            .nodes
            .iter()
            .zip(&self.states)
            .filter_map(|(node, state)| match (node, state) {
                (Node::Input(input), Some(NodeState::Pending(_)))
                    if self.inputs[input.0 as usize].is_none() =>
                {
                    Some(*input)
                }
                _ => None,
            })
            .collect();
        frontier.sort_unstable();
        frontier
    }

    /// Drive demand from `root` as far as it can go without new
    /// external values. `Ok(Some(slot))` = resolved; `Ok(None)` =
    /// parked on the frontier. Poll-shaped on purpose: the async
    /// driver wraps this step; workers share the worklist later.
    pub fn demand(&mut self, root: NodeId) -> Result<Option<Slot>, MachineError> {
        loop {
            let mut progressed = false;
            let mut worklist = vec![root];
            while let Some(id) = worklist.pop() {
                progressed |= self.step(id, &mut worklist)?;
            }
            match &self.states[root.0 as usize] {
                Some(NodeState::Resolved(slot)) => return Ok(Some(*slot)),
                _ if progressed => continue,
                _ => return Ok(None),
            }
        }
    }

    fn resolved(&self, id: NodeId) -> Option<Slot> {
        match &self.states[id.0 as usize] {
            Some(NodeState::Resolved(slot)) => Some(*slot),
            _ => None,
        }
    }

    /// Advance one node; push unresolved dependencies onto the
    /// worklist. Returns whether anything changed.
    fn step(&mut self, id: NodeId, worklist: &mut Vec<NodeId>) -> Result<bool, MachineError> {
        if matches!(self.states[id.0 as usize], Some(NodeState::Resolved(_))) {
            return Ok(false);
        }
        let first_demand = self.states[id.0 as usize].is_none();
        match self.nodes[id.0 as usize].clone() {
            Node::Const(slot) => {
                self.states[id.0 as usize] = Some(NodeState::Resolved(slot));
                Ok(true)
            }
            Node::Input(input) => match self.inputs[input.0 as usize] {
                Some(slot) => {
                    self.states[id.0 as usize] = Some(NodeState::Resolved(slot));
                    Ok(true)
                }
                None => {
                    self.states[id.0 as usize] = Some(NodeState::Pending(None));
                    Ok(first_demand)
                }
            },
            Node::Body { ops, inputs } => {
                // JOINT need: every input is demanded at once — the
                // fan-out is discovered here, not annotated.
                let mut all_known = true;
                for &input in inputs.iter() {
                    if self.resolved(input).is_none() {
                        worklist.push(input);
                        all_known = false;
                    }
                }
                let ready: Vec<bool> = inputs
                    .iter()
                    .map(|&input| self.resolved(input).is_some())
                    .collect();
                let awaited: Vec<Slot> = inputs
                    .iter()
                    .map(|&input| self.resolved(input).unwrap_or(Slot::Unit))
                    .collect();
                let mut lane = match self.states[id.0 as usize].take() {
                    Some(NodeState::Pending(Some(lane))) => lane,
                    _ => Lane::new(),
                };
                match lane.run(&ops, &ready, &awaited)? {
                    LaneStep::Done(slot) => {
                        self.executions += 1;
                        self.states[id.0 as usize] = Some(NodeState::Resolved(slot));
                        Ok(true)
                    }
                    LaneStep::Suspended => {
                        self.states[id.0 as usize] = Some(NodeState::Pending(Some(lane)));
                        Ok(first_demand || all_known)
                    }
                }
            }
            Node::Cond {
                scrutinee,
                if_true,
                if_false,
            } => match self.resolved(scrutinee) {
                None => {
                    worklist.push(scrutinee);
                    self.states[id.0 as usize] = Some(NodeState::Pending(None));
                    Ok(first_demand)
                }
                Some(Slot::Bool(which)) => {
                    let arm = if which { if_true } else { if_false };
                    match self.resolved(arm) {
                        Some(slot) => {
                            self.states[id.0 as usize] = Some(NodeState::Resolved(slot));
                            Ok(true)
                        }
                        None => {
                            worklist.push(arm);
                            self.states[id.0 as usize] = Some(NodeState::Pending(None));
                            Ok(false)
                        }
                    }
                }
                Some(other) => Err(MachineError::CondScrutineeNotBool { got: other }),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn i(v: i64) -> Slot {
        Slot::I64(v)
    }

    #[test]
    fn building_the_graph_computes_nothing() {
        let mut g = Graph::new();
        let a = g.constant(i(2));
        let b = g.constant(i(3));
        let _sum = g.body([NodeOp::Await, NodeOp::Await, NodeOp::AddI64], [a, b]);
        assert_eq!(g.executions(), 0);
    }

    #[test]
    fn demand_pulls_only_the_reachable_closure() {
        let mut g = Graph::new();
        let a = g.constant(i(2));
        let b = g.constant(i(3));
        let wanted = g.body([NodeOp::Await, NodeOp::Await, NodeOp::AddI64], [a, b]);
        let _unwanted = g.body([NodeOp::Await, NodeOp::Push(i(100)), NodeOp::MulI64], [a]);
        let result = g.demand(wanted).expect("no machine error");
        assert_eq!(result, Some(i(5)));
        assert_eq!(g.executions(), 1, "the undemanded body must never run");
    }

    #[test]
    fn shared_node_computes_once() {
        let mut g = Graph::new();
        let two = g.constant(i(2));
        let shared = g.body([NodeOp::Await, NodeOp::Push(i(10)), NodeOp::MulI64], [two]);
        let left = g.body([NodeOp::Await, NodeOp::Push(i(1)), NodeOp::AddI64], [shared]);
        let right = g.body([NodeOp::Await, NodeOp::Push(i(2)), NodeOp::AddI64], [shared]);
        let top = g.body([NodeOp::Await, NodeOp::Await, NodeOp::AddI64], [left, right]);
        assert_eq!(g.demand(top).expect("ok"), Some(i(43)));
        assert_eq!(g.executions(), 4, "diamond: shared ran once, not twice");
    }

    #[test]
    fn unused_cond_arm_never_runs() {
        let mut g = Graph::new();
        let flag = g.constant(Slot::Bool(false));
        let five = g.constant(i(5));
        let taken = g.body([NodeOp::Await, NodeOp::Push(i(2)), NodeOp::MulI64], [five]);
        let not_taken = g.body([NodeOp::Await, NodeOp::Push(i(9)), NodeOp::AddI64], [five]);
        let cond = g.cond(flag, not_taken, taken);
        assert_eq!(g.demand(cond).expect("ok"), Some(i(10)));
        assert_eq!(g.executions(), 1, "only the selected arm ran");
    }

    #[test]
    fn joint_need_parks_all_inputs_at_once() {
        let mut g = Graph::new();
        let (left, left_input) = g.input();
        let (right, right_input) = g.input();
        let sum = g.body([NodeOp::Await, NodeOp::Await, NodeOp::AddI64], [left, right]);

        assert_eq!(g.demand(sum).expect("ok"), None, "parks on external inputs");
        assert_eq!(
            g.pending_frontier(),
            vec![left_input, right_input],
            "BOTH inputs demanded jointly on the first step — batched, not sequential"
        );
    }

    #[test]
    fn suspends_on_inputs_and_resumes_to_completion() {
        let mut g = Graph::new();
        let (a, a_input) = g.input();
        let (b, b_input) = g.input();
        let sum = g.body([NodeOp::Await, NodeOp::Await, NodeOp::AddI64], [a, b]);

        assert_eq!(g.demand(sum).expect("ok"), None);
        g.provide(b_input, i(2));
        assert_eq!(
            g.demand(sum).expect("ok"),
            None,
            "one of two inputs is not enough"
        );
        assert_eq!(g.pending_frontier(), vec![a_input], "frontier shrank to the missing input");
        g.provide(a_input, i(40));
        assert_eq!(g.demand(sum).expect("ok"), Some(i(42)));
        assert_eq!(g.pending_frontier(), Vec::<InputId>::new());
        assert_eq!(g.executions(), 1);
    }

    #[test]
    fn slots_beyond_i64_flow_through_bodies() {
        let mut g = Graph::new();
        let (h, h_input) = g.input();
        let pass_through = g.body([NodeOp::Await], [h]);
        assert_eq!(g.demand(pass_through).expect("ok"), None);
        g.provide(h_input, Slot::F64(super::super::value::TotalF64::new(1.5)));
        let out = g.demand(pass_through).expect("ok");
        assert_eq!(out, Some(Slot::F64(super::super::value::TotalF64::new(1.5))));
    }

    #[test]
    fn type_errors_surface_as_machine_errors() {
        let mut g = Graph::new();
        let t = g.constant(Slot::Bool(true));
        let bad = g.body([NodeOp::Await, NodeOp::Push(i(1)), NodeOp::AddI64], [t]);
        assert!(matches!(
            g.demand(bad),
            Err(MachineError::TypeMismatch { op: "AddI64", .. })
        ));
    }
}
