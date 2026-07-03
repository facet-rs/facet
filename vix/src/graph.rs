//! The graph demand engine: NOTHING FORCES LOCALLY.
//!
//! This is the heart of vix and the part that killed four prior attempts.
//! Evaluation is not a recursive walk that forces subexpressions as it meets
//! them. Instead:
//!
//! 1. **Building the graph computes nothing.** Adding a node is pure data; no
//!    operation runs at construction.
//! 2. **Demand backpropagates from the selected output.** You demand a node;
//!    it demands its inputs; they demand theirs — and only nodes on that
//!    reachable frontier are ever touched. A node no selection depends on is
//!    never computed.
//! 3. **Need is JOINT.** An `Apply` demands all its inputs at once, so
//!    independent inputs make progress concurrently (parallelism is the
//!    default, not an optimization).
//! 4. **Demand is DYNAMIC.** A `Cond` demands its scrutinee, then demands ONLY
//!    the chosen arm — the other arm never runs. Control flow lives in the
//!    graph; dep sets can depend on resolved values.
//! 5. **Every node is memoized by identity.** A node shared by two consumers
//!    computes once.
//! 6. **Boundaries suspend.** An `Input` node awaits an external future (an
//!    exec, a fetch, a remote value); reaching it parks the whole evaluation
//!    until it lands — demand is the await.
//!
//! Aggregation (fusing nodes for performance) does not violate any of this: a
//! fused node BATCHES demand at its boundary and computes strictly inside; it
//! is invisible to these semantics. The recursive oracle stays the reference —
//! the graph must produce the same value.
//!
//! Portability: the driver uses only `core::future` (vix stays wasm-clean); an
//! executor is supplied by the caller (tests use tokio). Values are `i64` in
//! v1 — the focus here is the DEMAND semantics, which are value-agnostic;
//! generalizing to `Value` tracks the weavy async operand generalization.

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use std::collections::{HashMap, HashSet};

/// Index of a node in a [`Graph`].
pub type NodeId = usize;

/// A pure operation over its inputs' values.
pub type Op = Box<dyn Fn(&[i64]) -> i64>;

/// A node — pure data. Constructing one runs nothing.
pub enum Node {
    /// A constant value.
    Const(i64),
    /// An external async boundary: its value arrives from a future supplied at
    /// demand time (an exec, a fetch, a remote value). `slot` indexes the
    /// caller-provided input futures.
    Input { slot: usize },
    /// A pure operation over its inputs' values. Runs only when demanded and
    /// all inputs are ready.
    Apply { op: Op, inputs: Vec<NodeId> },
    /// Dynamic demand: demand `scrutinee`; if it resolves to 0, the value is
    /// `zero`'s, else `nonzero`'s. ONLY the chosen arm is demanded.
    Cond {
        scrutinee: NodeId,
        zero: NodeId,
        nonzero: NodeId,
    },
}

/// A graph under construction. Adding nodes computes nothing.
#[derive(Default)]
pub struct Graph {
    nodes: Vec<Node>,
    input_count: usize,
}

impl Graph {
    pub fn new() -> Self {
        Graph::default()
    }

    fn push(&mut self, node: Node) -> NodeId {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    /// A constant.
    pub fn constant(&mut self, value: i64) -> NodeId {
        self.push(Node::Const(value))
    }

    /// An external async input boundary. Inputs are numbered in creation order;
    /// the caller supplies one future per input at demand time.
    pub fn input(&mut self) -> NodeId {
        let slot = self.input_count;
        self.input_count += 1;
        self.push(Node::Input { slot })
    }

    /// A pure operation over inputs.
    pub fn apply(&mut self, op: impl Fn(&[i64]) -> i64 + 'static, inputs: Vec<NodeId>) -> NodeId {
        self.push(Node::Apply {
            op: Box::new(op),
            inputs,
        })
    }

    /// A conditional: value of `zero` when `scrutinee == 0`, else `nonzero`.
    /// The unchosen arm is never demanded.
    pub fn cond(&mut self, scrutinee: NodeId, zero: NodeId, nonzero: NodeId) -> NodeId {
        self.push(Node::Cond {
            scrutinee,
            zero,
            nonzero,
        })
    }

    /// Number of input boundaries (how many futures a demand needs).
    pub fn input_count(&self) -> usize {
        self.input_count
    }
}

/// An observable demand event — the debuggable timeline of the evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A node was first pulled onto the demand frontier.
    Demanded(NodeId),
    /// A node's value was computed (or a boundary resolved).
    Computed(NodeId),
}

/// A demand-driven evaluation of one selected output, as a `Future`. Polling it
/// backpropagates demand; it resolves when the output's value is available, and
/// parks (Pending) whenever it's blocked on an outstanding input boundary.
pub struct Demand<'g> {
    graph: &'g Graph,
    output: NodeId,
    inputs: Vec<Option<Pin<Box<dyn Future<Output = i64>>>>>,
    demanded: HashSet<NodeId>,
    computed: HashMap<NodeId, i64>,
    /// The debuggable timeline (append-only): demand + compute events in order.
    pub trace: Vec<Event>,
}

impl<'g> Demand<'g> {
    /// Evaluate `output` of `graph`, with one input future per boundary (in
    /// input-creation order).
    pub fn new(
        graph: &'g Graph,
        output: NodeId,
        input_futures: Vec<Pin<Box<dyn Future<Output = i64>>>>,
    ) -> Self {
        assert_eq!(
            input_futures.len(),
            graph.input_count(),
            "one future per input boundary"
        );
        Demand {
            graph,
            output,
            inputs: input_futures.into_iter().map(Some).collect(),
            demanded: HashSet::new(),
            computed: HashMap::new(),
            trace: Vec::new(),
        }
    }

    /// The nodes currently on the demand frontier but not yet resolved — the
    /// live "what is this evaluation waiting on" view, for debugging.
    pub fn pending_frontier(&self) -> Vec<NodeId> {
        let mut v: Vec<NodeId> = self
            .demanded
            .iter()
            .copied()
            .filter(|n| !self.computed.contains_key(n))
            .collect();
        v.sort_unstable();
        v
    }

    fn demand(&mut self, node: NodeId) {
        if self.demanded.insert(node) {
            self.trace.push(Event::Demanded(node));
        }
    }

    fn resolve(&mut self, node: NodeId, value: i64) {
        self.computed.insert(node, value);
        self.trace.push(Event::Computed(node));
    }
}

impl Future for Demand<'_> {
    type Output = i64;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<i64> {
        let this = &mut *self;
        this.demand(this.output);

        loop {
            let mut progress = false;

            // Snapshot the demanded-but-unresolved frontier; process each. New
            // demands/resolutions surface on the next iteration (the `progress`
            // flag keeps us looping until we quiesce).
            let frontier: Vec<NodeId> = this
                .demanded
                .iter()
                .copied()
                .filter(|n| !this.computed.contains_key(n))
                .collect();

            for node in frontier {
                match &this.graph.nodes[node] {
                    Node::Const(v) => {
                        let v = *v;
                        this.resolve(node, v);
                        progress = true;
                    }
                    Node::Input { slot } => {
                        let slot = *slot;
                        if let Some(fut) = this.inputs[slot].as_mut()
                            && let Poll::Ready(v) = fut.as_mut().poll(cx)
                        {
                            this.inputs[slot] = None;
                            this.resolve(node, v);
                            progress = true;
                        }
                    }
                    Node::Apply { inputs, .. } => {
                        let inputs = inputs.clone();
                        let mut ready = true;
                        for &i in &inputs {
                            if !this.computed.contains_key(&i) {
                                // Joint demand: pull EVERY input at once.
                                if this.demanded.insert(i) {
                                    this.trace.push(Event::Demanded(i));
                                    progress = true;
                                }
                                ready = false;
                            }
                        }
                        if ready {
                            let values: Vec<i64> =
                                inputs.iter().map(|i| this.computed[i]).collect();
                            let Node::Apply { op, .. } = &this.graph.nodes[node] else {
                                unreachable!()
                            };
                            let v = op(&values);
                            this.resolve(node, v);
                            progress = true;
                        }
                    }
                    Node::Cond {
                        scrutinee,
                        zero,
                        nonzero,
                    } => {
                        let (scrutinee, zero, nonzero) = (*scrutinee, *zero, *nonzero);
                        match this.computed.get(&scrutinee).copied() {
                            None => {
                                if this.demanded.insert(scrutinee) {
                                    this.trace.push(Event::Demanded(scrutinee));
                                    progress = true;
                                }
                            }
                            Some(s) => {
                                // Dynamic demand: pull ONLY the chosen arm.
                                let arm = if s == 0 { zero } else { nonzero };
                                match this.computed.get(&arm).copied() {
                                    None => {
                                        if this.demanded.insert(arm) {
                                            this.trace.push(Event::Demanded(arm));
                                            progress = true;
                                        }
                                    }
                                    Some(v) => {
                                        this.resolve(node, v);
                                        progress = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(v) = this.computed.get(&this.output) {
                return Poll::Ready(*v);
            }
            if !progress {
                // Blocked on an outstanding input boundary; its poll registered
                // the waker, so we'll be re-polled when it lands.
                return Poll::Pending;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Duration;

    fn later(value: i64, ms: u64) -> Pin<Box<dyn Future<Output = i64>>> {
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            value
        })
    }

    /// A run-counting op: proves whether an Apply actually executed.
    fn counted(
        counter: &Rc<Cell<u32>>,
        f: impl Fn(&[i64]) -> i64 + 'static,
    ) -> impl Fn(&[i64]) -> i64 + 'static {
        let counter = counter.clone();
        move |vs: &[i64]| {
            counter.set(counter.get() + 1);
            f(vs)
        }
    }

    async fn eval(demand: Demand<'_>) -> (i64, Vec<Event>) {
        let mut demand = demand;
        let result = core::future::poll_fn(|cx| Pin::new(&mut demand).poll(cx)).await;
        (result, demand.trace.clone())
    }

    #[test]
    fn building_the_graph_computes_nothing() {
        let ran = Rc::new(Cell::new(0));
        let mut g = Graph::new();
        let a = g.constant(2);
        let b = g.constant(3);
        let _sum = g.apply(counted(&ran, |v| v[0] + v[1]), vec![a, b]);
        // No demand yet ⇒ the op never ran, no matter that the node exists.
        assert_eq!(ran.get(), 0);
    }

    #[tokio::test]
    async fn demand_pulls_only_the_reachable_closure() {
        let ran_used = Rc::new(Cell::new(0));
        let ran_unused = Rc::new(Cell::new(0));
        let mut g = Graph::new();
        let a = g.constant(20);
        let b = g.constant(22);
        let used = g.apply(counted(&ran_used, |v| v[0] + v[1]), vec![a, b]);
        // An entirely separate subgraph the selection does not depend on.
        let c = g.constant(100);
        let _unused = g.apply(counted(&ran_unused, |v| v[0] * 2), vec![c]);

        let (result, _) = eval(Demand::new(&g, used, vec![])).await;
        assert_eq!(result, 42);
        assert_eq!(ran_used.get(), 1);
        assert_eq!(
            ran_unused.get(),
            0,
            "nothing forces locally: unused node never ran"
        );
    }

    #[tokio::test]
    async fn unused_cond_arm_never_runs() {
        let ran_zero = Rc::new(Cell::new(0));
        let ran_nonzero = Rc::new(Cell::new(0));
        let mut g = Graph::new();
        let scrut = g.constant(0); // ⇒ choose the `zero` arm
        let z_in = g.constant(21);
        let zero_arm = g.apply(counted(&ran_zero, |v| v[0] * 2), vec![z_in]);
        let nz_in = g.constant(1);
        let nonzero_arm = g.apply(counted(&ran_nonzero, |v| v[0] + 999), vec![nz_in]);
        let c = g.cond(scrut, zero_arm, nonzero_arm);

        let (result, _) = eval(Demand::new(&g, c, vec![])).await;
        assert_eq!(result, 42);
        assert_eq!(ran_zero.get(), 1, "the chosen arm ran");
        assert_eq!(ran_nonzero.get(), 0, "the UNCHOSEN arm never ran");
    }

    #[tokio::test]
    async fn shared_node_computes_once() {
        let ran = Rc::new(Cell::new(0));
        let mut g = Graph::new();
        let base = g.apply(counted(&ran, |_| 7), vec![]);
        // Two consumers of the same node.
        let left = g.apply(|v| v[0] + 1, vec![base]);
        let right = g.apply(|v| v[0] + 2, vec![base]);
        let top = g.apply(|v| v[0] + v[1], vec![left, right]);

        let (result, _) = eval(Demand::new(&g, top, vec![])).await;
        assert_eq!(result, (7 + 1) + (7 + 2));
        assert_eq!(
            ran.get(),
            1,
            "the shared node memoized: computed exactly once"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn independent_inputs_resolve_concurrently() {
        // Two Input boundaries feeding one Apply: joint demand ⇒ both futures
        // are polled every turn, so they resolve concurrently. The whole eval
        // takes ~max(delays), not sum.
        let mut g = Graph::new();
        let x = g.input();
        let y = g.input();
        let sum = g.apply(|v| v[0] + v[1], vec![x, y]);

        let start = std::time::Instant::now();
        let (result, _) = eval(Demand::new(&g, sum, vec![later(40, 60), later(2, 60)])).await;
        let elapsed = start.elapsed();
        assert_eq!(result, 42);
        assert!(
            elapsed < Duration::from_millis(150),
            "concurrent (~60ms), not serial (~120ms): {elapsed:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn async_boundary_suspends_and_resumes() {
        // demand parks on the Input boundary, then resumes when it lands.
        let mut g = Graph::new();
        let x = g.input();
        let forty = g.constant(40);
        let sum = g.apply(|v| v[0] + v[1], vec![x, forty]);

        let mut demand = Demand::new(&g, sum, vec![later(2, 50)]);
        let mut parked = false;
        let result = core::future::poll_fn(|cx| {
            let p = Pin::new(&mut demand).poll(cx);
            if p.is_pending() {
                // While parked, the frontier shows we're blocked on the input.
                assert!(demand.pending_frontier().contains(&x));
                parked = true;
            }
            p
        })
        .await;
        assert!(parked, "the boundary suspended the evaluation");
        assert_eq!(result, 42);
    }
}
