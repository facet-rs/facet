//! VIR island lowering to architecture-neutral Weavy bytecode.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use weavy::mem::Layout;
use weavy::task::{
    ArgCopy, Fn as WeavyFn, FnId as WeavyFnId, Op as WeavyOp, Program as WeavyProgram,
};

use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticPayload, Diagnostics};
use crate::runtime::{
    DemandKey, DemandPreimage, FrameRegion, FrameSlot, FrameWords, RecipeId, SchemaId,
};
use crate::support::Span;
use crate::vir::{
    EnumType, Function, FunctionId, Island, Node, NodeId, NodeRef, ORDERING_EQUAL_VARIANT,
    ORDERING_GREATER_VARIANT, ORDERING_LESS_VARIANT, Op, Type, VariantPayload,
};

/// Island outcome status words. Word 0 of a fault-ABI return region.
pub const OUTCOME_OK: i64 = 0;
pub const OUTCOME_INDEX_OUT_OF_BOUNDS: i64 = 1;
pub const OUTCOME_MALFORMED_ARRAY: i64 = 2;

/// Words reserved at the head of a fault-ABI return region: status, and the
/// two failure details (`index`, `length`).
const OUTCOME_HEADER_WORDS: usize = 3;

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct SourceMapEntry {
    pub trace_id: u32,
    pub function: FunctionId,
    pub node: NodeId,
    pub span: Span,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct LoweringAttribution {
    pub functions: Vec<FunctionId>,
    pub source_map: Vec<SourceMapEntry>,
}

impl LoweringAttribution {
    #[must_use]
    pub fn function_for_frame(&self, frame: u32) -> Option<FunctionId> {
        self.functions.get(frame as usize).copied()
    }

    #[must_use]
    pub fn source_for_trace(&self, trace_id: u32) -> Option<&SourceMapEntry> {
        self.source_map
            .get(trace_id as usize)
            .filter(|entry| entry.trace_id == trace_id)
    }
}

/// A value the island admits into the store before entering the task, bound to
/// one entry-frame slot.
///
/// The two byte runs are deliberately distinct: `identity_preimage` is the
/// framed, ABI-independent content that identity is computed from, while
/// `memory` is the machine-plane payload task code reads through.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct ValueConstant {
    pub node: NodeRef,
    pub slot: FrameSlot,
    pub schema: SchemaId,
    pub identity_preimage: Vec<u8>,
    pub memory: Vec<u8>,
}

/// Cached executable bytes for one VIR recipe. Per-compilation source spans
/// and per-demand memo locations deliberately live outside this artifact.
pub struct LoweringArtifact {
    pub recipe: RecipeId,
    pub demand_key: DemandKey,
    pub demand_preimage: DemandPreimage,
    pub program: WeavyProgram,
    pub constants: Vec<ValueConstant>,
    /// Whether every function in this island returns a fault-ABI outcome
    /// region rather than a bare value. Set when the island can fail.
    pub fault_abi: bool,
}

impl LoweringArtifact {
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        for constant in &self.constants {
            let _ = writeln!(
                out,
                "constant n{} frame[{}] schema={} preimage={} memory={}",
                constant.node.node.0,
                constant.slot.byte_offset(),
                constant.schema.0.hex(),
                constant.identity_preimage.len(),
                constant.memory.len()
            );
        }
        for (function_index, function) in self.program.fns.iter().enumerate() {
            let _ = writeln!(
                out,
                "weavy fn {function_index} frame(size={}, align={})",
                function.frame.size, function.frame.align
            );
            for (pc, op) in function.code.iter().enumerate() {
                let _ = writeln!(out, "  {pc:04} {op:?}");
            }
        }
        out
    }
}

#[derive(facet::Facet, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoweringCacheCounters {
    pub hits: u64,
    pub misses: u64,
}

/// Explicit non-semantic cache of lowering artifacts. This is not a value
/// memo: eviction can only cause recompilation.
///
/// r[impl machine.lowering.cache]
#[derive(Default)]
pub struct LoweringCache {
    entries: BTreeMap<RecipeId, LoweringArtifact>,
    counters: LoweringCacheCounters,
}

impl LoweringCache {
    pub fn get_or_lower(&mut self, island: &Island) -> Result<&LoweringArtifact, Diagnostics> {
        let recipe = RecipeId::from_canonical_vir(&island.canonical_recipe_bytes());
        if self.entries.contains_key(&recipe) {
            self.counters.hits += 1;
            return Ok(self.entries.get(&recipe).expect("entry was just observed"));
        }
        self.counters.misses += 1;
        let lowered = lower_island(island, recipe)?;
        self.entries.insert(recipe, lowered);
        Ok(self.entries.get(&recipe).expect("entry was just inserted"))
    }

    #[must_use]
    pub fn counters(&self) -> LoweringCacheCounters {
        self.counters
    }

    pub fn inspect(&self) -> impl Iterator<Item = (&RecipeId, &LoweringArtifact)> {
        self.entries.iter()
    }
}

/// Build the current compilation's attribution table separately from cached
/// bytecode. A span-only edit neither invalidates bytecode nor inherits a stale
/// source map.
#[must_use]
pub fn source_map_for(island: &Island) -> Vec<SourceMapEntry> {
    attribution_for(island).source_map
}

/// Build per-compilation frame and trace attribution separately from cached
/// bytecode. Closure-local Weavy ids remain stable while source spans and
/// module-local function ids may change between compilations.
#[must_use]
pub fn attribution_for(island: &Island) -> LoweringAttribution {
    let mut functions = Vec::with_capacity(1 + island.callees.len());
    functions.push(island.function);
    functions.extend(island.callees.iter().map(|function| function.id));

    let mut source_map = Vec::new();
    push_source_entries(&mut source_map, island.function, &island.nodes);
    for function in &island.callees {
        push_source_entries(&mut source_map, function.id, &function.nodes);
    }
    LoweringAttribution {
        functions,
        source_map,
    }
}

fn push_source_entries(entries: &mut Vec<SourceMapEntry>, function: FunctionId, nodes: &[Node]) {
    for node in nodes.iter().filter(|node| node.op != Op::Yield) {
        entries.push(SourceMapEntry {
            trace_id: u32::try_from(entries.len()).expect("trace count fits u32"),
            function,
            node: node.id,
            span: node.span,
        });
    }
}

fn lower_island(island: &Island, recipe: RecipeId) -> Result<LoweringArtifact, Diagnostics> {
    let output = island
        .nodes
        .iter()
        .find(|node| node.id == island.output)
        .ok_or_else(|| lowering_diagnostic(Span { start: 0, end: 0 }, "missing island output"))?;
    if output.ty != Type::Check {
        return Err(lowering_diagnostic(
            output.span,
            "island output is not a Check",
        ));
    }

    let function_ids = island.local_function_ids();
    let functions = island
        .callees
        .iter()
        .map(|function| (function.id, function))
        .collect::<BTreeMap<_, _>>();
    let attribution = attribution_for(island);
    let trace_ids = attribution
        .source_map
        .iter()
        .map(|entry| {
            (
                NodeRef {
                    function: entry.function,
                    node: entry.node,
                },
                entry.trace_id,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let constant_bodies = constant_bodies(island)?;
    let constant_closures = constant_closures(island, &constant_bodies);
    let fault_abi = island_can_fault(island);
    let mut layouts = BTreeMap::new();
    layouts.insert(
        island.function,
        FunctionLayout::build(
            island.function,
            &island.nodes,
            constant_closures.get(&island.function).ok_or_else(|| {
                lowering_diagnostic(output.span, "island function has no constant closure")
            })?,
            fault_abi.then_some(&output.ty),
            output.span,
        )?,
    );
    for function in &island.callees {
        let function_output = function.output.ok_or_else(|| {
            lowering_diagnostic(function.span, "called VIR function has no return node")
        })?;
        let _ = function_output;
        layouts.insert(
            function.id,
            FunctionLayout::build(
                function.id,
                &function.nodes,
                constant_closures.get(&function.id).ok_or_else(|| {
                    lowering_diagnostic(function.span, "called function has no constant closure")
                })?,
                fault_abi.then_some(&function.return_type),
                function.span,
            )?,
        );
    }
    let context = LoweringContext {
        root_function: island.function,
        function_ids: &function_ids,
        functions: &functions,
        trace_ids: &trace_ids,
        layouts: &layouts,
        constant_bodies: &constant_bodies,
    };

    let mut constants = Vec::new();
    let mut functions_out = Vec::with_capacity(1 + island.callees.len());
    functions_out.push(lower_vir_function(
        island.function,
        &island.nodes,
        &[],
        island.output,
        &context,
        &mut constants,
    )?);
    for function in &island.callees {
        let output = function.output.ok_or_else(|| {
            lowering_diagnostic(function.span, "called VIR function has no return node")
        })?;
        functions_out.push(lower_vir_function(
            function.id,
            &function.nodes,
            &function.parameters,
            output,
            &context,
            &mut constants,
        )?);
    }
    let program = WeavyProgram { fns: functions_out };
    let demand_preimage = DemandPreimage {
        closure: recipe,
        arguments: Vec::new(),
    };
    Ok(LoweringArtifact {
        recipe,
        demand_key: DemandKey::from_preimage(&demand_preimage),
        demand_preimage,
        program,
        constants,
        fault_abi,
    })
}

/// An island carries the fault ABI exactly when one of its functions can fail.
/// Today the only fallible operations are the store-backed array reads.
fn island_can_fault(island: &Island) -> bool {
    let fallible = |nodes: &[Node]| {
        nodes
            .iter()
            .any(|node| matches!(node.op, Op::ArrayIndex | Op::ArrayLen))
    };
    fallible(&island.nodes)
        || island
            .callees
            .iter()
            .any(|function| fallible(&function.nodes))
}

/// The framed content and machine payload of one admitted constant.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ConstantBody {
    schema: SchemaId,
    identity_preimage: Vec<u8>,
    memory: Vec<u8>,
}

/// Materialize every literal the island admits into the store before entry.
///
/// Only strings qualify today: a string is an immutable published value whose
/// bytes are already its framed content. Arrays are NOT here — interior array
/// construction is task-private molten vocabulary that never interns, never
/// hashes, and never publishes an identity (`machine.island.molten-private`).
fn constant_bodies(island: &Island) -> Result<BTreeMap<NodeRef, ConstantBody>, Diagnostics> {
    let mut bodies = BTreeMap::new();
    let mut collect = |function: FunctionId, nodes: &[Node]| {
        for node in nodes {
            let Op::String(value) = &node.op else {
                continue;
            };
            bodies.insert(
                NodeRef {
                    function,
                    node: node.id,
                },
                ConstantBody {
                    schema: SchemaId::named("vix.String.v1"),
                    identity_preimage: value.as_bytes().to_vec(),
                    memory: value.as_bytes().to_vec(),
                },
            );
        }
    };
    collect(island.function, &island.nodes);
    for function in &island.callees {
        collect(function.id, &function.nodes);
    }
    Ok(bodies)
}

/// The schema tag an array-words payload carries for its elements. Only
/// word-shaped elements have one: a payload of handles would make a container's
/// identity depend on process-local integers at freeze, which
/// `machine.identity.handle-by-referent` forbids.
fn array_element_schema(element: &Type, span: Span) -> Result<SchemaId, Diagnostics> {
    match element {
        Type::Int | Type::Bool => Ok(SchemaId::named(&element.name())),
        _ => Err(lowering_diagnostic(
            span,
            &format!(
                "an array of {} needs a referent-hashing element payload",
                element.name()
            ),
        )),
    }
}

fn constant_closures(
    island: &Island,
    bodies: &BTreeMap<NodeRef, ConstantBody>,
) -> BTreeMap<FunctionId, BTreeSet<NodeRef>> {
    let mut closures = BTreeMap::new();
    let mut calls = BTreeMap::new();
    let mut register = |function: FunctionId, nodes: &[Node]| {
        closures.insert(
            function,
            nodes
                .iter()
                .map(|node| NodeRef {
                    function,
                    node: node.id,
                })
                .filter(|constant| bodies.contains_key(constant))
                .collect::<BTreeSet<_>>(),
        );
        calls.insert(
            function,
            nodes
                .iter()
                .filter_map(|node| match &node.op {
                    Op::Call(callee) | Op::Closure(callee) => Some(*callee),
                    _ => None,
                })
                .collect::<BTreeSet<_>>(),
        );
    };
    register(island.function, &island.nodes);
    for function in &island.callees {
        register(function.id, &function.nodes);
    }

    loop {
        let previous = closures.clone();
        let mut changed = false;
        for (&function, callees) in &calls {
            let inherited = callees
                .iter()
                .filter_map(|callee| previous.get(callee))
                .flat_map(|constants| constants.iter().copied())
                .collect::<Vec<_>>();
            let constants = closures
                .get_mut(&function)
                .expect("every call-graph function has a closure");
            let before = constants.len();
            constants.extend(inherited);
            changed |= constants.len() != before;
        }
        if !changed {
            break;
        }
    }
    closures
}

struct LoweringContext<'a> {
    root_function: FunctionId,
    function_ids: &'a BTreeMap<FunctionId, u32>,
    functions: &'a BTreeMap<FunctionId, &'a Function>,
    trace_ids: &'a BTreeMap<NodeRef, u32>,
    layouts: &'a BTreeMap<FunctionId, FunctionLayout>,
    constant_bodies: &'a BTreeMap<NodeRef, ConstantBody>,
}

struct FunctionLayout {
    regions: BTreeMap<NodeId, FrameRegion>,
    constant_slots: BTreeMap<NodeRef, FrameSlot>,
    scratch: Option<FrameSlot>,
    control_scratch: Option<ControlScratch>,
    /// Holds the position word while an array literal fills itself.
    array_scratch: Option<FrameSlot>,
    fault: Option<FaultLayout>,
    frame_size: usize,
}

#[derive(Clone, Copy)]
struct ControlScratch {
    expected: FrameSlot,
    condition: FrameSlot,
}

/// The frame regions a fault-ABI function needs: the outcome region it returns,
/// one probe word per checked store read, and one landing region per call site
/// so a callee's outcome can be inspected before its value is consumed.
#[derive(Clone, Debug)]
struct FaultLayout {
    outcome: FrameRegion,
    present: FrameSlot,
    length: FrameSlot,
    call_outcomes: BTreeMap<NodeId, FrameRegion>,
}

impl FaultLayout {
    fn status(&self, span: Span) -> Result<FrameSlot, Diagnostics> {
        self.outcome
            .word(0)
            .ok_or_else(|| lowering_diagnostic(span, "outcome status lies outside its frame"))
    }

    fn detail(&self, index: usize, span: Span) -> Result<FrameSlot, Diagnostics> {
        self.outcome
            .word(1 + index)
            .ok_or_else(|| lowering_diagnostic(span, "outcome detail lies outside its frame"))
    }

    fn value(&self, words: FrameWords, span: Span) -> Result<FrameRegion, Diagnostics> {
        self.outcome
            .subregion(OUTCOME_HEADER_WORDS, words)
            .ok_or_else(|| lowering_diagnostic(span, "outcome value lies outside its frame"))
    }

    fn call_outcome(&self, node: NodeId, span: Span) -> Result<FrameRegion, Diagnostics> {
        self.call_outcomes
            .get(&node)
            .copied()
            .ok_or_else(|| lowering_diagnostic(span, "call site has no outcome landing region"))
    }
}

/// Header words of a call site's landing region, mirroring the callee's
/// outcome region.
fn outcome_region_words(value_words: FrameWords, span: Span) -> Result<FrameWords, Diagnostics> {
    value_words
        .as_usize()
        .checked_add(OUTCOME_HEADER_WORDS)
        .and_then(FrameWords::from_usize)
        .ok_or_else(|| lowering_diagnostic(span, "outcome region width overflow"))
}

impl FunctionLayout {
    fn build(
        function: FunctionId,
        nodes: &[Node],
        constants: &BTreeSet<NodeRef>,
        fault_return: Option<&Type>,
        span: Span,
    ) -> Result<Self, Diagnostics> {
        let mut regions = BTreeMap::new();
        let mut next_word = 0usize;
        for node in nodes {
            let width = node.ty.word_width().ok_or_else(|| {
                lowering_diagnostic(
                    node.span,
                    &format!("{} has no island-interior representation", node.ty.name()),
                )
            })?;
            let words = FrameWords::from_usize(width)
                .ok_or_else(|| lowering_diagnostic(node.span, "VIR value width overflow"))?;
            let region = FrameRegion::for_words(next_word, words)
                .ok_or_else(|| lowering_diagnostic(node.span, "Weavy frame region overflow"))?;
            if regions.insert(node.id, region).is_some() {
                return Err(lowering_diagnostic(node.span, "duplicate VIR node id"));
            }
            next_word = next_word
                .checked_add(width)
                .ok_or_else(|| lowering_diagnostic(node.span, "function frame size overflow"))?;
        }

        let needs_scratch = nodes.iter().any(|node| {
            matches!(node.op, Op::Compare)
                || (matches!(node.op, Op::Eq | Op::Ne)
                    && node
                        .inputs
                        .first()
                        .and_then(|input| regions.get(input))
                        .is_some_and(|region| region.words().as_usize() > 1))
        });
        let scratch = if needs_scratch {
            let slot = FrameSlot::for_word(next_word)
                .ok_or_else(|| lowering_diagnostic(span, "Weavy scratch offset overflow"))?;
            next_word = next_word
                .checked_add(FrameWords::ONE.as_usize())
                .ok_or_else(|| lowering_diagnostic(span, "function frame size overflow"))?;
            Some(slot)
        } else {
            None
        };
        let control_scratch = if nodes.iter().any(|node| matches!(node.op, Op::Match { .. })) {
            let expected = FrameSlot::for_word(next_word)
                .ok_or_else(|| lowering_diagnostic(span, "Weavy control offset overflow"))?;
            next_word = next_word
                .checked_add(FrameWords::ONE.as_usize())
                .ok_or_else(|| lowering_diagnostic(span, "function frame size overflow"))?;
            let condition = FrameSlot::for_word(next_word)
                .ok_or_else(|| lowering_diagnostic(span, "Weavy control offset overflow"))?;
            next_word = next_word
                .checked_add(FrameWords::ONE.as_usize())
                .ok_or_else(|| lowering_diagnostic(span, "function frame size overflow"))?;
            Some(ControlScratch {
                expected,
                condition,
            })
        } else {
            None
        };
        let array_scratch = if nodes.iter().any(|node| matches!(node.op, Op::Array)) {
            let slot = FrameSlot::for_word(next_word)
                .ok_or_else(|| lowering_diagnostic(span, "Weavy array offset overflow"))?;
            next_word = next_word
                .checked_add(FrameWords::ONE.as_usize())
                .ok_or_else(|| lowering_diagnostic(span, "function frame size overflow"))?;
            Some(slot)
        } else {
            None
        };
        let mut constant_slots = BTreeMap::new();
        for &constant in constants {
            let slot = if constant.function == function {
                let node = nodes
                    .iter()
                    .find(|node| node.id == constant.node)
                    .ok_or_else(|| {
                        lowering_diagnostic(span, "closure names a missing local constant")
                    })?;
                if !matches!(node.op, Op::String(_)) {
                    return Err(lowering_diagnostic(
                        node.span,
                        "closure constant is not an admitted literal node",
                    ));
                }
                regions
                    .get(&constant.node)
                    .copied()
                    .ok_or_else(|| {
                        lowering_diagnostic(node.span, "admitted constant has no frame region")
                    })?
                    .start()
            } else {
                let slot = FrameSlot::for_word(next_word)
                    .ok_or_else(|| lowering_diagnostic(span, "closure constant offset overflow"))?;
                next_word = next_word
                    .checked_add(1)
                    .ok_or_else(|| lowering_diagnostic(span, "function frame size overflow"))?;
                slot
            };
            constant_slots.insert(constant, slot);
        }

        let fault = match fault_return {
            None => None,
            Some(return_type) => {
                let word = |next_word: &mut usize| -> Result<FrameSlot, Diagnostics> {
                    let slot = FrameSlot::for_word(*next_word)
                        .ok_or_else(|| lowering_diagnostic(span, "fault offset overflow"))?;
                    *next_word = next_word
                        .checked_add(1)
                        .ok_or_else(|| lowering_diagnostic(span, "function frame size overflow"))?;
                    Ok(slot)
                };
                let present = word(&mut next_word)?;
                let length = word(&mut next_word)?;

                let outcome_words = outcome_region_words(type_words(return_type, span)?, span)?;
                let outcome = FrameRegion::for_words(next_word, outcome_words)
                    .ok_or_else(|| lowering_diagnostic(span, "outcome region overflow"))?;
                next_word = next_word
                    .checked_add(outcome_words.as_usize())
                    .ok_or_else(|| lowering_diagnostic(span, "function frame size overflow"))?;

                let mut call_outcomes = BTreeMap::new();
                for node in nodes
                    .iter()
                    .filter(|node| matches!(node.op, Op::Call(_) | Op::CallValue))
                {
                    let words = outcome_region_words(type_words(&node.ty, node.span)?, node.span)?;
                    let region = FrameRegion::for_words(next_word, words).ok_or_else(|| {
                        lowering_diagnostic(node.span, "call outcome region overflow")
                    })?;
                    next_word = next_word.checked_add(words.as_usize()).ok_or_else(|| {
                        lowering_diagnostic(node.span, "function frame size overflow")
                    })?;
                    call_outcomes.insert(node.id, region);
                }
                Some(FaultLayout {
                    outcome,
                    present,
                    length,
                    call_outcomes,
                })
            }
        };

        let frame_size = FrameSlot::frame_size(next_word)
            .ok_or_else(|| lowering_diagnostic(span, "function frame size overflow"))?;
        Ok(Self {
            regions,
            constant_slots,
            scratch,
            control_scratch,
            array_scratch,
            fault,
            frame_size,
        })
    }

    fn region(&self, node: NodeId, span: Span) -> Result<FrameRegion, Diagnostics> {
        self.regions
            .get(&node)
            .copied()
            .ok_or_else(|| lowering_diagnostic(span, "VIR node has no frame region"))
    }

    fn constant_slot(&self, constant: NodeRef, span: Span) -> Result<FrameSlot, Diagnostics> {
        self.constant_slots
            .get(&constant)
            .copied()
            .ok_or_else(|| lowering_diagnostic(span, "function closure is missing a constant"))
    }
}

#[derive(Clone, Copy)]
struct CodeLabel(usize);

enum PendingOp {
    Concrete(WeavyOp),
    Jump(CodeLabel),
    JumpIfZero { value: u32, target: CodeLabel },
}

struct CodeBuilder {
    ops: Vec<PendingOp>,
    labels: Vec<Option<usize>>,
}

impl CodeBuilder {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            ops: Vec::with_capacity(capacity),
            labels: Vec::new(),
        }
    }

    fn push(&mut self, op: WeavyOp) {
        self.ops.push(PendingOp::Concrete(op));
    }

    fn extend(&mut self, ops: impl IntoIterator<Item = WeavyOp>) {
        self.ops.extend(ops.into_iter().map(PendingOp::Concrete));
    }

    fn label(&mut self) -> CodeLabel {
        let label = CodeLabel(self.labels.len());
        self.labels.push(None);
        label
    }

    fn bind(&mut self, label: CodeLabel, span: Span) -> Result<(), Diagnostics> {
        let instruction = self.ops.len();
        let target = self
            .labels
            .get_mut(label.0)
            .ok_or_else(|| lowering_diagnostic(span, "unknown Weavy code label"))?;
        if target.replace(instruction).is_some() {
            return Err(lowering_diagnostic(span, "duplicate Weavy code label"));
        }
        Ok(())
    }

    fn jump(&mut self, target: CodeLabel) {
        self.ops.push(PendingOp::Jump(target));
    }

    fn jump_if_zero(&mut self, value: FrameSlot, target: CodeLabel) {
        self.ops.push(PendingOp::JumpIfZero {
            value: value.byte_offset(),
            target,
        });
    }

    fn finish(self, span: Span) -> Result<Vec<WeavyOp>, Diagnostics> {
        let labels = self.labels;
        self.ops
            .into_iter()
            .map(|op| match op {
                PendingOp::Concrete(op) => Ok(op),
                PendingOp::Jump(label) => Ok(WeavyOp::Jump {
                    target: Self::resolve(&labels, label, span)?,
                }),
                PendingOp::JumpIfZero { value, target } => Ok(WeavyOp::JumpIfZero {
                    value,
                    target: Self::resolve(&labels, target, span)?,
                }),
            })
            .collect()
    }

    fn resolve(labels: &[Option<usize>], label: CodeLabel, span: Span) -> Result<u32, Diagnostics> {
        let target = labels
            .get(label.0)
            .and_then(|target| *target)
            .ok_or_else(|| lowering_diagnostic(span, "unbound Weavy code label"))?;
        u32::try_from(target).map_err(|_| lowering_diagnostic(span, "Weavy jump target overflow"))
    }
}

fn lower_vir_function(
    function: FunctionId,
    nodes: &[Node],
    parameters: &[crate::vir::Parameter],
    output: NodeId,
    context: &LoweringContext<'_>,
    constants: &mut Vec<ValueConstant>,
) -> Result<WeavyFn, Diagnostics> {
    let layout = context
        .layouts
        .get(&function)
        .ok_or_else(|| lowering_diagnostic(Span { start: 0, end: 0 }, "missing function layout"))?;
    let function_context = FunctionLoweringContext {
        id: function,
        parameters,
        layout,
    };
    let output_node = nodes
        .iter()
        .find(|node| node.id == output)
        .ok_or_else(|| lowering_diagnostic(Span { start: 0, end: 0 }, "missing function output"))?;
    let nodes_by_id = nodes
        .iter()
        .map(|node| (node.id, node))
        .collect::<BTreeMap<_, _>>();
    let node_ids = nodes.iter().map(|node| node.id).collect::<Vec<_>>();
    let mut values = BTreeMap::new();
    let mut code = CodeBuilder::with_capacity(nodes.len().saturating_mul(2) + 1);
    let exit = code.label();
    {
        let sequence = SequenceContext {
            nodes: &nodes_by_id,
            function: &function_context,
            lowering: context,
        };
        let mut outputs = SequenceOutputs {
            constants,
            code: &mut code,
            exit,
        };
        lower_node_sequence(&node_ids, &mut values, &sequence, &mut outputs, None)?;
    }
    let output_region = values
        .get(&output)
        .map(|value| value.region)
        .ok_or_else(|| {
            lowering_diagnostic(output_node.span, "function output has no frame region")
        })?;
    let return_region = match &layout.fault {
        None => output_region,
        Some(fault) => {
            // Success epilogue: publish an OK status beside the value, then
            // share the single Ret with every fault landing.
            let value = fault.value(output_region.words(), output_node.span)?;
            code.push(WeavyOp::ConstI64 {
                dst: fault.status(output_node.span)?.byte_offset(),
                value: OUTCOME_OK,
            });
            code.extend(copy_region(output_node, output_region, value)?);
            code.bind(exit, output_node.span)?;
            fault.outcome
        }
    };
    code.push(WeavyOp::Ret {
        src: return_region.start().byte_offset(),
        size: return_region
            .byte_size()
            .ok_or_else(|| lowering_diagnostic(output_node.span, "return size overflow"))?,
    });
    let code = code.finish(output_node.span)?;
    Ok(WeavyFn {
        frame: Layout {
            size: layout.frame_size,
            align: FrameSlot::word_align(),
        },
        code,
    })
}

struct SequenceContext<'nodes, 'function, 'lowering> {
    nodes: &'nodes BTreeMap<NodeId, &'nodes Node>,
    function: &'function FunctionLoweringContext<'function>,
    lowering: &'lowering LoweringContext<'lowering>,
}

struct SequenceOutputs<'constants, 'code> {
    constants: &'constants mut Vec<ValueConstant>,
    code: &'code mut CodeBuilder,
    /// The function's shared return landing. Faulting sites jump here after
    /// writing their outcome header.
    exit: CodeLabel,
}

fn lower_node_sequence(
    node_ids: &[NodeId],
    values: &mut BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
    active_variant: Option<u32>,
) -> Result<(), Diagnostics> {
    let mut controlled = BTreeSet::new();
    for node_id in node_ids {
        let node = sequence.nodes.get(node_id).copied().ok_or_else(|| {
            lowering_diagnostic(
                Span { start: 0, end: 0 },
                "control region names a missing node",
            )
        })?;
        match &node.op {
            Op::Match { arms } => {
                for arm in arms {
                    controlled.extend(arm.nodes.iter().copied());
                }
            }
            Op::If {
                consequent,
                alternative,
            } => {
                controlled.extend(consequent.nodes.iter().copied());
                controlled.extend(alternative.nodes.iter().copied());
            }
            Op::OrderedMatch { arms, fallback } => {
                for arm in arms {
                    controlled.extend(arm.condition.nodes.iter().copied());
                    controlled.extend(arm.body.nodes.iter().copied());
                }
                controlled.extend(fallback.nodes.iter().copied());
            }
            _ => {}
        }
    }

    for node_id in node_ids {
        if controlled.contains(node_id) {
            continue;
        }
        let node =
            sequence.nodes.get(node_id).copied().ok_or_else(|| {
                lowering_diagnostic(Span { start: 0, end: 0 }, "missing VIR node")
            })?;
        if values.contains_key(&node.id) {
            return Err(lowering_diagnostic(node.span, "duplicate VIR node id"));
        }
        let dst = sequence.function.layout.region(node.id, node.span)?;
        let trace_id = sequence
            .lowering
            .trace_ids
            .get(&NodeRef {
                function: sequence.function.id,
                node: node.id,
            })
            .copied()
            .ok_or_else(|| lowering_diagnostic(node.span, "VIR node has no trace attribution"))?;
        outputs.code.push(WeavyOp::Trace { id: trace_id });
        let representation = match &node.op {
            Op::ArrayIndex => lower_array_index_node(node, dst, values, sequence, outputs)?,
            Op::ArrayLen => lower_array_len_node(node, dst, values, sequence, outputs)?,
            Op::Call(_) | Op::CallValue if sequence.function.layout.fault.is_some() => {
                lower_faulting_call_node(node, dst, values, sequence, outputs)?
            }
            Op::Match { .. } => lower_match_node(node, dst, values, sequence, outputs)?,
            Op::If { .. } => lower_if_node(node, dst, values, sequence, outputs, active_variant)?,
            Op::OrderedMatch { .. } => {
                lower_ordered_match_node(node, dst, values, sequence, outputs, active_variant)?
            }
            Op::Compare => lower_compare_node(node, dst, values, sequence, outputs)?,
            _ => {
                let lowered = lower_node(
                    node,
                    dst,
                    values,
                    sequence.function,
                    sequence.lowering,
                    outputs.constants,
                    active_variant,
                )?;
                outputs.code.extend(lowered.ops);
                lowered.representation
            }
        };
        values.insert(
            node.id,
            LoweredSlot {
                region: dst,
                ty: node.ty.clone(),
                representation,
            },
        );
    }
    Ok(())
}

/// Read one array position through the checked store vocabulary.
///
/// The `present` flag is the machine's answer, not a folded constant: an
/// out-of-range read publishes an `IndexOutOfBounds` outcome carrying the
/// demanded index and the array's own length, and abandons the island.
///
/// r[impl lang.collection.array-index]
fn lower_array_index_node(
    node: &Node,
    dst: FrameRegion,
    values: &BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
) -> Result<ValueRepresentation, Diagnostics> {
    let (array, index) = binary_values(node, values)?;
    let element = array_read_element(node, &array)?;
    if node.ty != element {
        return Err(lowering_diagnostic(
            node.span,
            "array index result is not the element type",
        ));
    }
    require_value(node, &index, &Type::Int, ValueRepresentation::Word)?;
    let representation = representation_for_type(&element, node.span)?;
    if representation != ValueRepresentation::Word || dst.words() != FrameWords::ONE {
        return Err(lowering_diagnostic(
            node.span,
            "store-backed array elements occupy one frame word",
        ));
    }
    let element_schema_ref = array_element_schema(&element, node.span)?.ref_word();
    let fault = fault_layout(node, sequence)?;

    outputs.code.push(WeavyOp::LoadArrayWord {
        dst: dst.start().byte_offset(),
        present: fault.present.byte_offset(),
        array: array.region.start().byte_offset(),
        index: index.region.start().byte_offset(),
        elem_schema_ref: element_schema_ref,
    });
    let out_of_range = outputs.code.label();
    let in_range = outputs.code.label();
    outputs.code.jump_if_zero(fault.present, out_of_range);
    outputs.code.jump(in_range);

    outputs.code.bind(out_of_range, node.span)?;
    outputs.code.push(WeavyOp::LoadArrayLen {
        dst: fault.length.byte_offset(),
        present: fault.present.byte_offset(),
        array: array.region.start().byte_offset(),
        elem_schema_ref: element_schema_ref,
    });
    outputs.code.push(WeavyOp::ConstI64 {
        dst: fault.status(node.span)?.byte_offset(),
        value: OUTCOME_INDEX_OUT_OF_BOUNDS,
    });
    outputs.code.push(WeavyOp::CopyI64 {
        dst: fault.detail(0, node.span)?.byte_offset(),
        src: index.region.start().byte_offset(),
    });
    outputs.code.push(WeavyOp::CopyI64 {
        dst: fault.detail(1, node.span)?.byte_offset(),
        src: fault.length.byte_offset(),
    });
    outputs.code.jump(outputs.exit);

    outputs.code.bind(in_range, node.span)?;
    Ok(ValueRepresentation::Word)
}

/// The element count carried by the array value itself. A payload whose header
/// does not answer is a machine invariant break, not a user error.
fn lower_array_len_node(
    node: &Node,
    dst: FrameRegion,
    values: &BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
) -> Result<ValueRepresentation, Diagnostics> {
    require_node_type(node, Type::Int)?;
    require_input_count(node, 1)?;
    let array = input_value(node, values, 0)?;
    let element = array_read_element(node, &array)?;
    let element_schema_ref = array_element_schema(&element, node.span)?.ref_word();
    if dst.words() != FrameWords::ONE {
        return Err(lowering_diagnostic(
            node.span,
            "array length does not occupy one frame word",
        ));
    }
    let fault = fault_layout(node, sequence)?;

    outputs.code.push(WeavyOp::LoadArrayLen {
        dst: dst.start().byte_offset(),
        present: fault.present.byte_offset(),
        array: array.region.start().byte_offset(),
        elem_schema_ref: element_schema_ref,
    });
    let malformed = outputs.code.label();
    let well_formed = outputs.code.label();
    outputs.code.jump_if_zero(fault.present, malformed);
    outputs.code.jump(well_formed);

    outputs.code.bind(malformed, node.span)?;
    outputs.code.push(WeavyOp::ConstI64 {
        dst: fault.status(node.span)?.byte_offset(),
        value: OUTCOME_MALFORMED_ARRAY,
    });
    for detail in 0..2 {
        outputs.code.push(WeavyOp::ConstI64 {
            dst: fault.detail(detail, node.span)?.byte_offset(),
            value: 0,
        });
    }
    outputs.code.jump(outputs.exit);

    outputs.code.bind(well_formed, node.span)?;
    Ok(ValueRepresentation::Word)
}

/// Under the fault ABI a callee returns an outcome region. The caller inspects
/// the status before it consumes the value, and propagates a fault unchanged.
fn lower_faulting_call_node(
    node: &Node,
    dst: FrameRegion,
    values: &BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
) -> Result<ValueRepresentation, Diagnostics> {
    let fault = fault_layout(node, sequence)?;
    let landing = fault.call_outcome(node.id, node.span)?;
    let call = match &node.op {
        Op::Call(callee) => lower_call_node(
            node,
            dst,
            landing.start(),
            values,
            *callee,
            sequence.function.layout,
            sequence.lowering,
        )?,
        Op::CallValue => lower_call_value_node(node, landing.start(), values)?,
        _ => {
            return Err(lowering_diagnostic(
                node.span,
                "non-call node reached faulting call lowering",
            ));
        }
    };
    outputs.code.push(call);

    let status = landing
        .word(0)
        .ok_or_else(|| lowering_diagnostic(node.span, "call outcome status lies outside its frame"))?;
    let returned = outputs.code.label();
    outputs.code.jump_if_zero(status, returned);
    for word in 0..OUTCOME_HEADER_WORDS {
        let src = landing.word(word).ok_or_else(|| {
            lowering_diagnostic(node.span, "call outcome header lies outside its frame")
        })?;
        let dst = fault.outcome.word(word).ok_or_else(|| {
            lowering_diagnostic(node.span, "outcome header lies outside its frame")
        })?;
        outputs.code.push(WeavyOp::CopyI64 {
            dst: dst.byte_offset(),
            src: src.byte_offset(),
        });
    }
    outputs.code.jump(outputs.exit);

    outputs.code.bind(returned, node.span)?;
    let value = landing
        .subregion(OUTCOME_HEADER_WORDS, dst.words())
        .ok_or_else(|| lowering_diagnostic(node.span, "call outcome value lies outside its frame"))?;
    outputs.code.extend(copy_region(node, value, dst)?);
    representation_for_type(&node.ty, node.span)
}

fn array_read_element(node: &Node, array: &LoweredSlot) -> Result<Type, Diagnostics> {
    let element = array
        .ty
        .array_element()
        .ok_or_else(|| lowering_diagnostic(node.span, "checked array read on a non-array value"))?
        .clone();
    require_value(
        node,
        array,
        &array.ty.clone(),
        ValueRepresentation::MoltenHandle,
    )?;
    Ok(element)
}

fn fault_layout<'a>(
    node: &Node,
    sequence: &'a SequenceContext<'_, '_, '_>,
) -> Result<&'a FaultLayout, Diagnostics> {
    sequence
        .function
        .layout
        .fault
        .as_ref()
        .ok_or_else(|| lowering_diagnostic(node.span, "fallible node in a non-faulting island"))
}

fn lower_ordered_match_node(
    node: &Node,
    dst: FrameRegion,
    values: &BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
    active_variant: Option<u32>,
) -> Result<ValueRepresentation, Diagnostics> {
    let Op::OrderedMatch { arms, fallback } = &node.op else {
        return Err(lowering_diagnostic(
            node.span,
            "non-OrderedMatch node reached structured ordered-match lowering",
        ));
    };
    require_input_count(node, 1)?;
    let _ = input_value(node, values, 0)?;
    let result_representation = representation_for_type(&node.ty, node.span)?;
    let end = outputs.code.label();

    for arm in arms {
        let mut condition_values = values.clone();
        lower_node_sequence(
            &arm.condition.nodes,
            &mut condition_values,
            sequence,
            outputs,
            active_variant,
        )?;
        let condition = condition_values.get(&arm.condition.output).ok_or_else(|| {
            lowering_diagnostic(node.span, "ordered-match condition has no lowered value")
        })?;
        require_value(node, condition, &Type::Bool, ValueRepresentation::Word)?;
        let next = outputs.code.label();
        outputs.code.jump_if_zero(condition.region.start(), next);

        let mut body_values = condition_values;
        lower_node_sequence(
            &arm.body.nodes,
            &mut body_values,
            sequence,
            outputs,
            active_variant,
        )?;
        let body = body_values.get(&arm.body.output).ok_or_else(|| {
            lowering_diagnostic(node.span, "ordered-match body has no lowered value")
        })?;
        require_value(node, body, &node.ty, result_representation)?;
        outputs.code.extend(copy_region(node, body.region, dst)?);
        outputs.code.jump(end);
        outputs.code.bind(next, node.span)?;
    }

    let mut fallback_values = values.clone();
    lower_node_sequence(
        &fallback.nodes,
        &mut fallback_values,
        sequence,
        outputs,
        active_variant,
    )?;
    let fallback = fallback_values.get(&fallback.output).ok_or_else(|| {
        lowering_diagnostic(node.span, "ordered-match fallback has no lowered value")
    })?;
    require_value(node, fallback, &node.ty, result_representation)?;
    outputs
        .code
        .extend(copy_region(node, fallback.region, dst)?);
    outputs.code.bind(end, node.span)?;

    Ok(result_representation)
}

fn lower_if_node(
    node: &Node,
    dst: FrameRegion,
    values: &BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
    active_variant: Option<u32>,
) -> Result<ValueRepresentation, Diagnostics> {
    let Op::If {
        consequent,
        alternative,
    } = &node.op
    else {
        return Err(lowering_diagnostic(
            node.span,
            "non-If node reached structured conditional lowering",
        ));
    };
    require_input_count(node, 1)?;
    let condition = input_value(node, values, 0)?;
    require_value(node, &condition, &Type::Bool, ValueRepresentation::Word)?;
    let result_representation = representation_for_type(&node.ty, node.span)?;

    let alternative_label = outputs.code.label();
    let end = outputs.code.label();
    outputs
        .code
        .jump_if_zero(condition.region.start(), alternative_label);

    let mut consequent_values = values.clone();
    lower_node_sequence(
        &consequent.nodes,
        &mut consequent_values,
        sequence,
        outputs,
        active_variant,
    )?;
    let consequent_output = consequent_values.get(&consequent.output).ok_or_else(|| {
        lowering_diagnostic(node.span, "if consequent output has no lowered value")
    })?;
    require_value(node, consequent_output, &node.ty, result_representation)?;
    outputs
        .code
        .extend(copy_region(node, consequent_output.region, dst)?);
    outputs.code.jump(end);

    outputs.code.bind(alternative_label, node.span)?;
    let mut alternative_values = values.clone();
    lower_node_sequence(
        &alternative.nodes,
        &mut alternative_values,
        sequence,
        outputs,
        active_variant,
    )?;
    let alternative_output = alternative_values.get(&alternative.output).ok_or_else(|| {
        lowering_diagnostic(node.span, "if alternative output has no lowered value")
    })?;
    require_value(node, alternative_output, &node.ty, result_representation)?;
    outputs
        .code
        .extend(copy_region(node, alternative_output.region, dst)?);
    outputs.code.bind(end, node.span)?;

    Ok(result_representation)
}

fn lower_match_node(
    node: &Node,
    dst: FrameRegion,
    values: &BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
) -> Result<ValueRepresentation, Diagnostics> {
    let Op::Match { arms } = &node.op else {
        return Err(lowering_diagnostic(
            node.span,
            "non-Match node reached structured match lowering",
        ));
    };
    require_input_count(node, 1)?;
    let scrutinee = input_value(node, values, 0)?;
    require_value(
        node,
        &scrutinee,
        &scrutinee.ty,
        ValueRepresentation::InlineComposite,
    )?;
    let enum_layout = EnumLayout::for_type(&scrutinee.ty, node.span)?;
    if enum_layout.words != scrutinee.region.words() {
        return Err(lowering_diagnostic(
            node.span,
            "match scrutinee has the wrong enum frame width",
        ));
    }
    if arms.is_empty() {
        return Err(lowering_diagnostic(node.span, "Match has no arms"));
    }
    let mut seen = BTreeSet::new();
    for arm in arms {
        enum_layout.variant(arm.variant, node.span)?;
        if !seen.insert(arm.variant) {
            return Err(lowering_diagnostic(
                node.span,
                "Match repeats an enum variant",
            ));
        }
    }
    if seen.len() != enum_layout.enumeration.variants.len() {
        return Err(lowering_diagnostic(
            node.span,
            "Match is not exhaustive over its enum type",
        ));
    }

    let result_representation = representation_for_type(&node.ty, node.span)?;
    let tag = scrutinee
        .region
        .word(0)
        .ok_or_else(|| lowering_diagnostic(node.span, "enum tag lies outside its frame"))?;
    let end = outputs.code.label();
    for (arm_index, arm) in arms.iter().enumerate() {
        let is_last = arm_index + 1 == arms.len();
        let next = if is_last {
            None
        } else {
            let scratch = sequence.function.layout.control_scratch.ok_or_else(|| {
                lowering_diagnostic(node.span, "Match has no control scratch region")
            })?;
            outputs.code.push(WeavyOp::ConstI64 {
                dst: scratch.expected.byte_offset(),
                value: i64::from(arm.variant),
            });
            outputs.code.push(WeavyOp::EqI64 {
                dst: scratch.condition.byte_offset(),
                a: tag.byte_offset(),
                b: scratch.expected.byte_offset(),
            });
            let next = outputs.code.label();
            outputs.code.jump_if_zero(scratch.condition, next);
            Some(next)
        };

        let mut arm_values = values.clone();
        lower_node_sequence(
            &arm.nodes,
            &mut arm_values,
            sequence,
            outputs,
            Some(arm.variant),
        )?;
        let output = arm_values.get(&arm.output).ok_or_else(|| {
            lowering_diagnostic(node.span, "match arm output has no lowered value")
        })?;
        require_value(node, output, &node.ty, result_representation)?;
        outputs.code.extend(copy_region(node, output.region, dst)?);

        if let Some(next) = next {
            outputs.code.jump(end);
            outputs.code.bind(next, node.span)?;
        }
    }
    outputs.code.bind(end, node.span)?;
    Ok(result_representation)
}

#[derive(Clone, Copy)]
enum CompareLeafKind {
    SignedWord,
    ValueBytes,
}

#[derive(Clone, Copy)]
struct CompareLeaf {
    kind: CompareLeafKind,
    a: FrameSlot,
    b: FrameSlot,
}

// r[related machine.value.structural-order]
fn lower_compare_node(
    node: &Node,
    dst: FrameRegion,
    values: &BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
) -> Result<ValueRepresentation, Diagnostics> {
    require_node_type(node, Type::ordering())?;
    let (a, b) = binary_values(node, values)?;
    if a.ty != b.ty {
        return Err(lowering_diagnostic(
            node.span,
            "comparison operands have different VIR types",
        ));
    }
    if !a.ty.structural_order_is_defined() {
        return Err(lowering_diagnostic(
            node.span,
            "structural order is not defined for this VIR type",
        ));
    }
    let representation = representation_for_type(&a.ty, node.span)?;
    require_value(node, &a, &a.ty, representation)?;
    require_value(node, &b, &a.ty, representation)?;
    if dst.words() != FrameWords::ONE {
        return Err(lowering_diagnostic(
            node.span,
            "Ordering result does not occupy one frame word",
        ));
    }

    let mut leaves = Vec::new();
    collect_compare_leaves(&a.ty, a.region, b.region, node.span, &mut leaves)?;
    let dst = dst.start();
    let end = outputs.code.label();
    if leaves.is_empty() {
        outputs.code.push(WeavyOp::ConstI64 {
            dst: dst.byte_offset(),
            value: i64::from(ORDERING_EQUAL_VARIANT),
        });
    }
    for (index, leaf) in leaves.iter().enumerate() {
        let is_last = index + 1 == leaves.len();
        match leaf.kind {
            CompareLeafKind::SignedWord => {
                let not_less = outputs.code.label();
                outputs.code.push(WeavyOp::LtI64 {
                    dst: dst.byte_offset(),
                    a: leaf.a.byte_offset(),
                    b: leaf.b.byte_offset(),
                });
                outputs.code.jump_if_zero(dst, not_less);
                outputs.code.push(WeavyOp::ConstI64 {
                    dst: dst.byte_offset(),
                    value: i64::from(ORDERING_LESS_VARIANT),
                });
                outputs.code.jump(end);
                outputs.code.bind(not_less, node.span)?;

                let equal = outputs.code.label();
                outputs.code.push(WeavyOp::GtI64 {
                    dst: dst.byte_offset(),
                    a: leaf.a.byte_offset(),
                    b: leaf.b.byte_offset(),
                });
                outputs.code.jump_if_zero(dst, equal);
                outputs.code.push(WeavyOp::ConstI64 {
                    dst: dst.byte_offset(),
                    value: i64::from(ORDERING_GREATER_VARIANT),
                });
                outputs.code.jump(end);
                outputs.code.bind(equal, node.span)?;
                if is_last {
                    outputs.code.push(WeavyOp::ConstI64 {
                        dst: dst.byte_offset(),
                        value: i64::from(ORDERING_EQUAL_VARIANT),
                    });
                }
            }
            CompareLeafKind::ValueBytes => {
                outputs.code.push(WeavyOp::CompareValueBytes {
                    dst: dst.byte_offset(),
                    a: leaf.a.byte_offset(),
                    b: leaf.b.byte_offset(),
                });
                if !is_last {
                    let scratch = sequence.function.layout.scratch.ok_or_else(|| {
                        lowering_diagnostic(node.span, "comparison has no scratch word")
                    })?;
                    outputs.code.push(WeavyOp::ConstI64 {
                        dst: scratch.byte_offset(),
                        value: i64::from(ORDERING_EQUAL_VARIANT),
                    });
                    outputs.code.push(WeavyOp::EqI64 {
                        dst: scratch.byte_offset(),
                        a: dst.byte_offset(),
                        b: scratch.byte_offset(),
                    });
                    outputs.code.jump_if_zero(scratch, end);
                }
            }
        }
    }
    outputs.code.bind(end, node.span)?;
    Ok(ValueRepresentation::InlineComposite)
}

fn collect_compare_leaves(
    ty: &Type,
    a: FrameRegion,
    b: FrameRegion,
    span: Span,
    leaves: &mut Vec<CompareLeaf>,
) -> Result<(), Diagnostics> {
    if a.words() != b.words() || a.words() != type_words(ty, span)? {
        return Err(lowering_diagnostic(
            span,
            "comparison operands have incompatible frame regions",
        ));
    }
    match ty {
        Type::Bool | Type::Int => {
            leaves.push(CompareLeaf {
                kind: CompareLeafKind::SignedWord,
                a: a.start(),
                b: b.start(),
            });
        }
        Type::String => {
            leaves.push(CompareLeaf {
                kind: CompareLeafKind::ValueBytes,
                a: a.start(),
                b: b.start(),
            });
        }
        Type::Function { .. } => {
            return Err(lowering_diagnostic(
                span,
                "function comparison requires stable closure identity",
            ));
        }
        Type::Tuple(elements) => {
            collect_compare_fields(elements.iter(), a, b, span, leaves)?;
        }
        Type::Record(record) => {
            collect_compare_fields(
                record.fields.iter().map(|field| &field.ty),
                a,
                b,
                span,
                leaves,
            )?;
        }
        Type::Enum(enumeration)
            if enumeration
                .variants
                .iter()
                .all(|variant| variant.payload.is_empty()) =>
        {
            leaves.push(CompareLeaf {
                kind: CompareLeafKind::SignedWord,
                a: a.start(),
                b: b.start(),
            });
        }
        Type::Enum(_) => {
            return Err(lowering_diagnostic(
                span,
                "payload enum comparison needs variant-directed lowering",
            ));
        }
        Type::Array(_) => {
            return Err(lowering_diagnostic(
                span,
                "array comparison needs element-directed lowering",
            ));
        }
        Type::Check | Type::StreamCheck => {
            return Err(lowering_diagnostic(
                span,
                "comparison reached a non-orderable VIR type",
            ));
        }
    }
    Ok(())
}

fn collect_compare_fields<'a>(
    fields: impl IntoIterator<Item = &'a Type>,
    a: FrameRegion,
    b: FrameRegion,
    span: Span,
    leaves: &mut Vec<CompareLeaf>,
) -> Result<(), Diagnostics> {
    let mut offset = 0usize;
    for ty in fields {
        let words = type_words(ty, span)?;
        let a = a
            .subregion(offset, words)
            .ok_or_else(|| lowering_diagnostic(span, "comparison field lies outside operand"))?;
        let b = b
            .subregion(offset, words)
            .ok_or_else(|| lowering_diagnostic(span, "comparison field lies outside operand"))?;
        collect_compare_leaves(ty, a, b, span, leaves)?;
        offset = offset
            .checked_add(words.as_usize())
            .ok_or_else(|| lowering_diagnostic(span, "comparison field offset overflow"))?;
    }
    if offset != a.words().as_usize() {
        return Err(lowering_diagnostic(
            span,
            "comparison fields do not cover the operand",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValueRepresentation {
    Word,
    RealizedHandle,
    /// A task-private molten handle. It has no identity and may not cross an
    /// island edge until a freeze/publish service exists.
    ///
    /// r[impl machine.island.molten-private]
    MoltenHandle,
    InlineComposite,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LoweredSlot {
    region: FrameRegion,
    ty: Type,
    representation: ValueRepresentation,
}

struct LoweredNode {
    ops: Vec<WeavyOp>,
    representation: ValueRepresentation,
}

struct FunctionLoweringContext<'a> {
    id: FunctionId,
    parameters: &'a [crate::vir::Parameter],
    layout: &'a FunctionLayout,
}

fn lower_node(
    node: &Node,
    dst: FrameRegion,
    values: &BTreeMap<NodeId, LoweredSlot>,
    function: &FunctionLoweringContext<'_>,
    context: &LoweringContext<'_>,
    constants: &mut Vec<ValueConstant>,
    active_variant: Option<u32>,
) -> Result<LoweredNode, Diagnostics> {
    let dst_region = dst;
    let dst_slot = dst.start();
    let dst = dst_slot.byte_offset();
    let (ops, representation) = match &node.op {
        Op::Bool(value) => {
            require_input_count(node, 0)?;
            require_node_type(node, Type::Bool)?;
            (
                vec![WeavyOp::ConstI64 {
                    dst,
                    value: i64::from(*value),
                }],
                ValueRepresentation::Word,
            )
        }
        Op::Int(value) => {
            require_input_count(node, 0)?;
            require_node_type(node, Type::Int)?;
            (
                vec![WeavyOp::ConstI64 { dst, value: *value }],
                ValueRepresentation::Word,
            )
        }
        Op::String(_) => {
            require_input_count(node, 0)?;
            require_node_type(node, Type::String)?;
            let constant = NodeRef {
                function: function.id,
                node: node.id,
            };
            if function.layout.constant_slot(constant, node.span)? != dst_slot {
                return Err(lowering_diagnostic(
                    node.span,
                    "String node does not occupy its local closure slot",
                ));
            }
            let root_layout = context
                .layouts
                .get(&context.root_function)
                .ok_or_else(|| lowering_diagnostic(node.span, "missing island root layout"))?;
            let body = context.constant_bodies.get(&constant).ok_or_else(|| {
                lowering_diagnostic(node.span, "String literal has no admitted constant body")
            })?;
            constants.push(ValueConstant {
                node: constant,
                slot: root_layout.constant_slot(constant, node.span)?,
                schema: body.schema,
                identity_preimage: body.identity_preimage.clone(),
                memory: body.memory.clone(),
            });
            (Vec::new(), ValueRepresentation::RealizedHandle)
        }
        Op::Parameter(parameter_id) => {
            require_input_count(node, 0)?;
            let parameter = function
                .parameters
                .iter()
                .find(|parameter| parameter.id == *parameter_id)
                .ok_or_else(|| {
                    lowering_diagnostic(node.span, "VIR parameter node has no declaration")
                })?;
            if parameter.node != node.id || parameter.ty != node.ty {
                return Err(lowering_diagnostic(
                    node.span,
                    "VIR parameter declaration does not match its node",
                ));
            }
            (Vec::new(), representation_for_type(&node.ty, node.span)?)
        }
        Op::Call(callee) => {
            let op = lower_call_node(
                node,
                dst_region,
                dst_slot,
                values,
                *callee,
                function.layout,
                context,
            )?;
            (vec![op], representation_for_type(&node.ty, node.span)?)
        }
        Op::Closure(callee) => {
            require_input_count(node, 0)?;
            if !matches!(node.ty, Type::Function { .. }) || dst_region.words().as_usize() != 2 {
                return Err(lowering_diagnostic(
                    node.span,
                    "closure value does not occupy its two-word ABI",
                ));
            }
            let target = context.functions.get(callee).copied().ok_or_else(|| {
                lowering_diagnostic(node.span, "closure function is absent from the island")
            })?;
            let target_layout = context.layouts.get(callee).ok_or_else(|| {
                lowering_diagnostic(node.span, "closure function has no frame layout")
            })?;
            let Type::Function { parameter, result } = &node.ty else {
                unreachable!("closure type was checked above")
            };
            let [target_parameter] = target.parameters.as_slice() else {
                return Err(lowering_diagnostic(
                    node.span,
                    "closure function does not have exactly one parameter",
                ));
            };
            if &target_parameter.ty != parameter.as_ref()
                || &target.return_type != result.as_ref()
                || target_layout
                    .region(target_parameter.node, node.span)?
                    .start()
                    != FrameSlot::for_word(0).expect("word zero is a frame slot")
            {
                return Err(lowering_diagnostic(
                    node.span,
                    "closure function does not satisfy the callable ABI",
                ));
            }
            if !target_layout.constant_slots.is_empty() {
                return Err(lowering_diagnostic(
                    node.span,
                    "indirect closure constants require an environment",
                ));
            }
            let callee = context.function_ids.get(callee).copied().ok_or_else(|| {
                lowering_diagnostic(node.span, "closure function has no local ABI id")
            })?;
            let environment = dst_region.word(1).ok_or_else(|| {
                lowering_diagnostic(node.span, "closure environment offset overflow")
            })?;
            (
                vec![
                    WeavyOp::ConstI64 {
                        dst,
                        value: i64::from(callee),
                    },
                    WeavyOp::ConstI64 {
                        dst: environment.byte_offset(),
                        value: 0,
                    },
                ],
                ValueRepresentation::InlineComposite,
            )
        }
        Op::CallValue => {
            let op = lower_call_value_node(node, dst_slot, values)?;
            (vec![op], representation_for_type(&node.ty, node.span)?)
        }
        Op::Add | Op::Sub | Op::Mul | Op::Div => {
            require_node_type(node, Type::Int)?;
            let (a, b) = binary_values(node, values)?;
            require_value(node, &a, &Type::Int, ValueRepresentation::Word)?;
            require_value(node, &b, &Type::Int, ValueRepresentation::Word)?;
            let op = match &node.op {
                Op::Add => WeavyOp::AddI64 {
                    dst,
                    a: a.region.start().byte_offset(),
                    b: b.region.start().byte_offset(),
                },
                Op::Sub => WeavyOp::SubI64 {
                    dst,
                    a: a.region.start().byte_offset(),
                    b: b.region.start().byte_offset(),
                },
                Op::Mul => WeavyOp::MulI64 {
                    dst,
                    a: a.region.start().byte_offset(),
                    b: b.region.start().byte_offset(),
                },
                Op::Div => WeavyOp::DivI64 {
                    dst,
                    a: a.region.start().byte_offset(),
                    b: b.region.start().byte_offset(),
                },
                _ => unreachable!("matched arithmetic VIR op"),
            };
            (vec![op], ValueRepresentation::Word)
        }
        Op::Tuple => lower_aggregate_node(node, dst_region, values, AggregateKind::Tuple)?,
        Op::Record => lower_aggregate_node(node, dst_region, values, AggregateKind::Record)?,
        Op::Project { index } => lower_project_node(node, dst_region, values, *index)?,
        Op::Variant { variant } => lower_variant_node(node, dst_region, values, *variant)?,
        Op::VariantProject { variant, field } => {
            if active_variant != Some(*variant) {
                return Err(lowering_diagnostic(
                    node.span,
                    "variant payload projection lies outside its matching arm",
                ));
            }
            lower_variant_project_node(node, dst_region, values, *variant, *field)?
        }
        Op::IsVariant { variant } => {
            require_node_type(node, Type::Bool)?;
            require_input_count(node, 1)?;
            let value = input_value(node, values, 0)?;
            let Type::Enum(enumeration) = &value.ty else {
                return Err(lowering_diagnostic(
                    node.span,
                    "variant predicate input is not an enum",
                ));
            };
            if usize::try_from(*variant)
                .ok()
                .is_none_or(|variant| variant >= enumeration.variants.len())
            {
                return Err(lowering_diagnostic(
                    node.span,
                    "variant predicate index is out of range",
                ));
            }
            require_value(
                node,
                &value,
                &value.ty,
                ValueRepresentation::InlineComposite,
            )?;
            (
                vec![
                    WeavyOp::ConstI64 {
                        dst,
                        value: i64::from(*variant),
                    },
                    WeavyOp::EqI64 {
                        dst,
                        a: value.region.start().byte_offset(),
                        b: dst,
                    },
                ],
                ValueRepresentation::Word,
            )
        }
        Op::Match { .. } => {
            return Err(lowering_diagnostic(
                node.span,
                "structured Match reached scalar node lowering",
            ));
        }
        Op::If { .. } => {
            return Err(lowering_diagnostic(
                node.span,
                "structured If reached scalar node lowering",
            ));
        }
        Op::OrderedMatch { .. } => {
            return Err(lowering_diagnostic(
                node.span,
                "structured OrderedMatch reached scalar node lowering",
            ));
        }
        Op::Compare => {
            return Err(lowering_diagnostic(
                node.span,
                "structured Compare reached scalar node lowering",
            ));
        }
        Op::Eq | Op::Ne => {
            require_node_type(node, Type::Bool)?;
            let (a, b) = binary_values(node, values)?;
            if a.ty != b.ty {
                return Err(lowering_diagnostic(
                    node.span,
                    "equality operands have different VIR types",
                ));
            }
            if !a.ty.equality_is_structural() {
                return Err(lowering_diagnostic(
                    node.span,
                    "equality is not defined for this VIR type",
                ));
            }
            let operand_representation = representation_for_type(&a.ty, node.span)?;
            require_value(node, &a, &a.ty, operand_representation)?;
            require_value(node, &b, &a.ty, operand_representation)?;
            (
                lower_equality(
                    node,
                    dst_region,
                    a.region,
                    b.region,
                    matches!(&node.op, Op::Eq),
                    function.layout.scratch,
                )?,
                ValueRepresentation::Word,
            )
        }
        Op::Expect => {
            require_node_type(node, Type::Check)?;
            require_input_count(node, 1)?;
            let condition = input_value(node, values, 0)?;
            require_value(node, &condition, &Type::Bool, ValueRepresentation::Word)?;
            (
                vec![WeavyOp::CopyI64 {
                    dst,
                    src: condition.region.start().byte_offset(),
                }],
                ValueRepresentation::Word,
            )
        }
        Op::Array => {
            // Interior construction: reserve a task-private molten payload and
            // fill its positions from already-lowered element words. No
            // scheduler request, no intern, no identity, no host call.
            //
            // r[impl machine.island.molten-private]
            // r[impl lang.collection.array-positions-are-data]
            let element = node.ty.array_element().ok_or_else(|| {
                lowering_diagnostic(node.span, "array construction has a non-array VIR type")
            })?;
            let element_schema_ref = array_element_schema(element, node.span)?.ref_word();
            if dst_region.words() != FrameWords::ONE {
                return Err(lowering_diagnostic(
                    node.span,
                    "an array handle occupies one frame word",
                ));
            }
            let count = u32::try_from(node.inputs.len())
                .map_err(|_| lowering_diagnostic(node.span, "array length overflow"))?;
            let position = function.layout.array_scratch.ok_or_else(|| {
                lowering_diagnostic(node.span, "array construction has no position scratch word")
            })?;

            let mut ops = vec![WeavyOp::ArrayNew {
                dst,
                count,
                elem_schema_ref: element_schema_ref,
            }];
            for index in 0..node.inputs.len() {
                let value = input_value(node, values, index)?;
                require_value(node, &value, element, ValueRepresentation::Word)?;
                ops.push(WeavyOp::ConstI64 {
                    dst: position.byte_offset(),
                    value: i64::try_from(index)
                        .map_err(|_| lowering_diagnostic(node.span, "array index overflow"))?,
                });
                ops.push(WeavyOp::ArrayStoreWord {
                    array: dst,
                    index: position.byte_offset(),
                    src: value.region.start().byte_offset(),
                });
            }
            (ops, ValueRepresentation::MoltenHandle)
        }
        Op::ArrayIndex | Op::ArrayLen => {
            return Err(lowering_diagnostic(
                node.span,
                "checked array read reached scalar node lowering",
            ));
        }
        Op::Yield => {
            return Err(lowering_diagnostic(
                node.span,
                "codata Yield appeared inside an island",
            ));
        }
    };
    Ok(LoweredNode {
        ops,
        representation,
    })
}

fn lower_call_node(
    node: &Node,
    dst: FrameRegion,
    ret: FrameSlot,
    values: &BTreeMap<NodeId, LoweredSlot>,
    callee: FunctionId,
    caller_layout: &FunctionLayout,
    context: &LoweringContext<'_>,
) -> Result<WeavyOp, Diagnostics> {
    let target = context.functions.get(&callee).copied().ok_or_else(|| {
        lowering_diagnostic(
            node.span,
            "called function is absent from the island closure",
        )
    })?;
    let target_output = target
        .output
        .ok_or_else(|| lowering_diagnostic(node.span, "called function has no return value"))?;
    if target.return_type != node.ty {
        return Err(lowering_diagnostic(
            node.span,
            "call result does not match the called function",
        ));
    }
    require_input_count(node, target.parameters.len())?;
    let target_layout = context
        .layouts
        .get(&callee)
        .ok_or_else(|| lowering_diagnostic(node.span, "called function has no frame layout"))?;
    if target_layout.region(target_output, node.span)?.words() != dst.words() {
        return Err(lowering_diagnostic(
            node.span,
            "call result region does not match the called function",
        ));
    }

    let mut args = Vec::with_capacity(
        target
            .parameters
            .len()
            .saturating_add(target_layout.constant_slots.len()),
    );
    for (index, parameter) in target.parameters.iter().enumerate() {
        let source = input_value(node, values, index)?;
        require_value(
            node,
            &source,
            &parameter.ty,
            representation_for_type(&parameter.ty, node.span)?,
        )?;
        let parameter_region = target_layout.region(parameter.node, node.span)?;
        if source.region.words() != parameter_region.words() {
            return Err(lowering_diagnostic(
                node.span,
                "call argument region does not match its parameter",
            ));
        }
        args.push(ArgCopy {
            src: source.region.start().byte_offset(),
            dst: parameter_region.start().byte_offset(),
            size: source
                .region
                .byte_size()
                .ok_or_else(|| lowering_diagnostic(node.span, "argument size overflow"))?,
        });
    }
    for (&constant, &target_slot) in &target_layout.constant_slots {
        let source_slot = caller_layout.constant_slot(constant, node.span)?;
        args.push(ArgCopy {
            src: source_slot.byte_offset(),
            dst: target_slot.byte_offset(),
            size: FrameSlot::word_size(),
        });
    }

    let callee = context
        .function_ids
        .get(&callee)
        .copied()
        .ok_or_else(|| lowering_diagnostic(node.span, "called function has no local ABI id"))?;
    Ok(WeavyOp::Call {
        callee: WeavyFnId(callee),
        args,
        ret: ret.byte_offset(),
    })
}

fn lower_call_value_node(
    node: &Node,
    ret: FrameSlot,
    values: &BTreeMap<NodeId, LoweredSlot>,
) -> Result<WeavyOp, Diagnostics> {
    require_input_count(node, 2)?;
    let callee = input_value(node, values, 0)?;
    let Type::Function { parameter, result } = &callee.ty else {
        return Err(lowering_diagnostic(
            node.span,
            "indirect call input is not a function value",
        ));
    };
    require_value(
        node,
        &callee,
        &callee.ty,
        ValueRepresentation::InlineComposite,
    )?;
    if callee.region.words().as_usize() != 2 || result.as_ref() != &node.ty {
        return Err(lowering_diagnostic(
            node.span,
            "indirect call does not match the closure ABI",
        ));
    }
    let argument = input_value(node, values, 1)?;
    require_value(
        node,
        &argument,
        parameter,
        representation_for_type(parameter, node.span)?,
    )?;
    Ok(WeavyOp::CallIndirect {
        callee: callee.region.start().byte_offset(),
        args: vec![ArgCopy {
            src: argument.region.start().byte_offset(),
            dst: 0,
            size: argument
                .region
                .byte_size()
                .ok_or_else(|| lowering_diagnostic(node.span, "argument size overflow"))?,
        }],
        ret: ret.byte_offset(),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AggregateKind {
    Tuple,
    Record,
}

struct AggregateElementLayout<'a> {
    ty: &'a Type,
    offset_words: usize,
    words: FrameWords,
}

struct AggregateLayout<'a> {
    kind: AggregateKind,
    elements: Vec<AggregateElementLayout<'a>>,
    words: FrameWords,
}

impl<'a> AggregateLayout<'a> {
    fn for_type(ty: &'a Type, span: Span) -> Result<Self, Diagnostics> {
        let (kind, element_types): (AggregateKind, Vec<&Type>) = match ty {
            Type::Tuple(elements) => (AggregateKind::Tuple, elements.iter().collect()),
            Type::Record(record) => (
                AggregateKind::Record,
                record.fields.iter().map(|field| &field.ty).collect(),
            ),
            _ => {
                return Err(lowering_diagnostic(
                    span,
                    &format!("{} is not an aggregate type", ty.name()),
                ));
            }
        };

        let mut offset_words = 0usize;
        let mut elements = Vec::with_capacity(element_types.len());
        for element_type in element_types {
            let words = type_words(element_type, span)?;
            elements.push(AggregateElementLayout {
                ty: element_type,
                offset_words,
                words,
            });
            offset_words = offset_words
                .checked_add(words.as_usize())
                .ok_or_else(|| lowering_diagnostic(span, "aggregate layout width overflow"))?;
        }
        let words = FrameWords::from_usize(offset_words)
            .ok_or_else(|| lowering_diagnostic(span, "aggregate layout width overflow"))?;
        Ok(Self {
            kind,
            elements,
            words,
        })
    }
}

struct EnumVariantLayout<'a> {
    elements: Vec<AggregateElementLayout<'a>>,
    words: FrameWords,
}

struct EnumLayout<'a> {
    enumeration: &'a EnumType,
    variants: Vec<EnumVariantLayout<'a>>,
    words: FrameWords,
}

impl<'a> EnumLayout<'a> {
    fn for_type(ty: &'a Type, span: Span) -> Result<Self, Diagnostics> {
        let Type::Enum(enumeration) = ty else {
            return Err(lowering_diagnostic(
                span,
                &format!("{} is not an enum type", ty.name()),
            ));
        };
        let mut widest_payload = 0usize;
        let mut variants = Vec::with_capacity(enumeration.variants.len());
        for variant in &enumeration.variants {
            let element_types = match &variant.payload {
                VariantPayload::Unit => Vec::new(),
                VariantPayload::Tuple(elements) => elements.iter().collect(),
                VariantPayload::Record(fields) => fields.iter().map(|field| &field.ty).collect(),
            };
            let mut offset_words = 0usize;
            let mut elements = Vec::with_capacity(element_types.len());
            for element_type in element_types {
                let words = type_words(element_type, span)?;
                elements.push(AggregateElementLayout {
                    ty: element_type,
                    offset_words,
                    words,
                });
                offset_words = offset_words.checked_add(words.as_usize()).ok_or_else(|| {
                    lowering_diagnostic(span, "enum payload layout width overflow")
                })?;
            }
            widest_payload = widest_payload.max(offset_words);
            variants.push(EnumVariantLayout {
                elements,
                words: FrameWords::from_usize(offset_words).ok_or_else(|| {
                    lowering_diagnostic(span, "enum payload layout width overflow")
                })?,
            });
        }
        let total_words = widest_payload
            .checked_add(1)
            .ok_or_else(|| lowering_diagnostic(span, "enum layout width overflow"))?;
        Ok(Self {
            enumeration,
            variants,
            words: FrameWords::from_usize(total_words)
                .ok_or_else(|| lowering_diagnostic(span, "enum layout width overflow"))?,
        })
    }

    fn variant(&self, index: u32, span: Span) -> Result<&EnumVariantLayout<'a>, Diagnostics> {
        let index = usize::try_from(index)
            .map_err(|_| lowering_diagnostic(span, "enum variant index overflow"))?;
        self.variants
            .get(index)
            .ok_or_else(|| lowering_diagnostic(span, "enum variant index is out of bounds"))
    }
}

fn lower_variant_node(
    node: &Node,
    dst: FrameRegion,
    values: &BTreeMap<NodeId, LoweredSlot>,
    variant: u32,
) -> Result<(Vec<WeavyOp>, ValueRepresentation), Diagnostics> {
    let layout = EnumLayout::for_type(&node.ty, node.span)?;
    let variant_layout = layout.variant(variant, node.span)?;
    require_input_count(node, variant_layout.elements.len())?;
    if layout.words != dst.words() {
        return Err(lowering_diagnostic(
            node.span,
            "enum construction destination has the wrong frame width",
        ));
    }
    if variant_layout
        .words
        .as_usize()
        .checked_add(1)
        .is_none_or(|occupied| occupied > layout.words.as_usize())
    {
        return Err(lowering_diagnostic(
            node.span,
            "enum variant payload exceeds its frame layout",
        ));
    }

    let mut ops = Vec::new();
    for word in 0..dst.words().as_usize() {
        let slot = dst
            .word(word)
            .ok_or_else(|| lowering_diagnostic(node.span, "enum zeroing offset overflow"))?;
        ops.push(WeavyOp::ConstI64 {
            dst: slot.byte_offset(),
            value: 0,
        });
    }
    ops.push(WeavyOp::ConstI64 {
        dst: dst.start().byte_offset(),
        value: i64::from(variant),
    });
    for (index, element) in variant_layout.elements.iter().enumerate() {
        let value = input_value(node, values, index)?;
        require_value(
            node,
            &value,
            element.ty,
            representation_for_type(element.ty, node.span)?,
        )?;
        let payload_offset = element
            .offset_words
            .checked_add(1)
            .ok_or_else(|| lowering_diagnostic(node.span, "enum payload offset overflow"))?;
        let target = dst
            .subregion(payload_offset, element.words)
            .ok_or_else(|| lowering_diagnostic(node.span, "enum payload lies outside its frame"))?;
        ops.extend(copy_region(node, value.region, target)?);
    }
    Ok((ops, ValueRepresentation::InlineComposite))
}

fn lower_variant_project_node(
    node: &Node,
    dst: FrameRegion,
    values: &BTreeMap<NodeId, LoweredSlot>,
    variant: u32,
    field: u32,
) -> Result<(Vec<WeavyOp>, ValueRepresentation), Diagnostics> {
    require_input_count(node, 1)?;
    let receiver = input_value(node, values, 0)?;
    require_value(
        node,
        &receiver,
        &receiver.ty,
        ValueRepresentation::InlineComposite,
    )?;
    let layout = EnumLayout::for_type(&receiver.ty, node.span)?;
    let variant_layout = layout.variant(variant, node.span)?;
    let field = usize::try_from(field)
        .map_err(|_| lowering_diagnostic(node.span, "variant field index overflow"))?;
    let element = variant_layout
        .elements
        .get(field)
        .ok_or_else(|| lowering_diagnostic(node.span, "variant field index is out of bounds"))?;
    if &node.ty != element.ty {
        return Err(lowering_diagnostic(
            node.span,
            "variant projection result has the wrong VIR type",
        ));
    }
    let payload_offset = element
        .offset_words
        .checked_add(1)
        .ok_or_else(|| lowering_diagnostic(node.span, "enum payload offset overflow"))?;
    let source = receiver
        .region
        .subregion(payload_offset, element.words)
        .ok_or_else(|| lowering_diagnostic(node.span, "enum payload lies outside its frame"))?;
    if source.words() != dst.words() {
        return Err(lowering_diagnostic(
            node.span,
            "variant projection destination has the wrong frame width",
        ));
    }
    Ok((
        copy_region(node, source, dst)?,
        representation_for_type(element.ty, node.span)?,
    ))
}

fn lower_aggregate_node(
    node: &Node,
    dst: FrameRegion,
    values: &BTreeMap<NodeId, LoweredSlot>,
    expected_kind: AggregateKind,
) -> Result<(Vec<WeavyOp>, ValueRepresentation), Diagnostics> {
    let layout = AggregateLayout::for_type(&node.ty, node.span)?;
    if layout.kind != expected_kind {
        return Err(lowering_diagnostic(
            node.span,
            "aggregate construction op does not match its VIR type",
        ));
    }
    require_input_count(node, layout.elements.len())?;
    let mut ops = Vec::new();
    for (index, element) in layout.elements.iter().enumerate() {
        let value = input_value(node, values, index)?;
        require_value(
            node,
            &value,
            element.ty,
            representation_for_type(element.ty, node.span)?,
        )?;
        let target = dst
            .subregion(element.offset_words, element.words)
            .ok_or_else(|| {
                lowering_diagnostic(node.span, "aggregate field lies outside its frame region")
            })?;
        ops.extend(copy_region(node, value.region, target)?);
    }
    if layout.words != dst.words() {
        return Err(lowering_diagnostic(
            node.span,
            "aggregate fields do not fill their frame region",
        ));
    }
    Ok((ops, ValueRepresentation::InlineComposite))
}

fn lower_project_node(
    node: &Node,
    dst: FrameRegion,
    values: &BTreeMap<NodeId, LoweredSlot>,
    index: u32,
) -> Result<(Vec<WeavyOp>, ValueRepresentation), Diagnostics> {
    require_input_count(node, 1)?;
    let receiver = input_value(node, values, 0)?;
    let layout = AggregateLayout::for_type(&receiver.ty, node.span)?;
    require_value(
        node,
        &receiver,
        &receiver.ty,
        ValueRepresentation::InlineComposite,
    )?;
    let index = usize::try_from(index)
        .map_err(|_| lowering_diagnostic(node.span, "aggregate projection index overflow"))?;
    let element = layout
        .elements
        .get(index)
        .ok_or_else(|| lowering_diagnostic(node.span, "aggregate projection is out of bounds"))?;
    if &node.ty != element.ty {
        return Err(lowering_diagnostic(
            node.span,
            "aggregate projection result has the wrong VIR type",
        ));
    }
    let source = receiver
        .region
        .subregion(element.offset_words, element.words)
        .ok_or_else(|| {
            lowering_diagnostic(
                node.span,
                "aggregate projection lies outside its frame region",
            )
        })?;
    if source.words() != dst.words() {
        return Err(lowering_diagnostic(
            node.span,
            "aggregate projection destination has the wrong frame width",
        ));
    }
    Ok((
        copy_region(node, source, dst)?,
        representation_for_type(element.ty, node.span)?,
    ))
}

fn lower_equality(
    node: &Node,
    dst: FrameRegion,
    a: FrameRegion,
    b: FrameRegion,
    equal: bool,
    scratch: Option<FrameSlot>,
) -> Result<Vec<WeavyOp>, Diagnostics> {
    if a.words() != b.words() || dst.words() != FrameWords::ONE {
        return Err(lowering_diagnostic(
            node.span,
            "equality operands have incompatible frame regions",
        ));
    }
    let width = a.words().as_usize();
    let dst = dst.start().byte_offset();
    if width == 0 {
        return Ok(vec![WeavyOp::ConstI64 {
            dst,
            value: i64::from(equal),
        }]);
    }
    let a0 = a
        .word(0)
        .ok_or_else(|| lowering_diagnostic(node.span, "equality word offset overflow"))?
        .byte_offset();
    let b0 = b
        .word(0)
        .ok_or_else(|| lowering_diagnostic(node.span, "equality operand is empty"))?
        .byte_offset();
    if width == 1 {
        return Ok(vec![if equal {
            WeavyOp::EqI64 { dst, a: a0, b: b0 }
        } else {
            WeavyOp::NeI64 { dst, a: a0, b: b0 }
        }]);
    }

    let scratch = scratch
        .ok_or_else(|| lowering_diagnostic(node.span, "composite equality has no scratch word"))?
        .byte_offset();
    let mut ops = vec![WeavyOp::EqI64 { dst, a: a0, b: b0 }];
    for index in 1..width {
        let a = a
            .word(index)
            .ok_or_else(|| lowering_diagnostic(node.span, "equality word offset overflow"))?
            .byte_offset();
        let b = b
            .word(index)
            .ok_or_else(|| lowering_diagnostic(node.span, "equality word offset overflow"))?
            .byte_offset();
        ops.push(WeavyOp::EqI64 { dst: scratch, a, b });
        ops.push(WeavyOp::MulI64 {
            dst,
            a: dst,
            b: scratch,
        });
    }
    if !equal {
        ops.push(WeavyOp::ConstI64 {
            dst: scratch,
            value: 0,
        });
        ops.push(WeavyOp::EqI64 {
            dst,
            a: dst,
            b: scratch,
        });
    }
    Ok(ops)
}

fn copy_region(
    node: &Node,
    source: FrameRegion,
    target: FrameRegion,
) -> Result<Vec<WeavyOp>, Diagnostics> {
    if source.words() != target.words() {
        return Err(lowering_diagnostic(
            node.span,
            "value copy has incompatible frame regions",
        ));
    }
    (0..source.words().as_usize())
        .map(|index| {
            let src = source
                .word(index)
                .ok_or_else(|| lowering_diagnostic(node.span, "source word offset overflow"))?;
            let dst = target
                .word(index)
                .ok_or_else(|| lowering_diagnostic(node.span, "target word offset overflow"))?;
            Ok(WeavyOp::CopyI64 {
                dst: dst.byte_offset(),
                src: src.byte_offset(),
            })
        })
        .collect()
}

fn type_words(ty: &Type, span: Span) -> Result<FrameWords, Diagnostics> {
    ty.word_width()
        .and_then(FrameWords::from_usize)
        .ok_or_else(|| lowering_diagnostic(span, "type has no finite frame width"))
}

fn representation_for_type(ty: &Type, span: Span) -> Result<ValueRepresentation, Diagnostics> {
    match ty {
        Type::Bool | Type::Int | Type::Check => Ok(ValueRepresentation::Word),
        Type::String => Ok(ValueRepresentation::RealizedHandle),
        Type::Array(_) => Ok(ValueRepresentation::MoltenHandle),
        Type::Function { .. } | Type::Tuple(_) | Type::Record(_) | Type::Enum(_) => {
            Ok(ValueRepresentation::InlineComposite)
        }
        Type::StreamCheck => Err(lowering_diagnostic(
            span,
            "Stream<Check> has no island-interior word representation",
        )),
    }
}

fn binary_values(
    node: &Node,
    values: &BTreeMap<NodeId, LoweredSlot>,
) -> Result<(LoweredSlot, LoweredSlot), Diagnostics> {
    require_input_count(node, 2)?;
    Ok((input_value(node, values, 0)?, input_value(node, values, 1)?))
}

fn input_value(
    node: &Node,
    values: &BTreeMap<NodeId, LoweredSlot>,
    index: usize,
) -> Result<LoweredSlot, Diagnostics> {
    let input = node
        .inputs
        .get(index)
        .ok_or_else(|| lowering_diagnostic(node.span, "missing VIR input"))?;
    values
        .get(input)
        .cloned()
        .ok_or_else(|| lowering_diagnostic(node.span, "VIR input is not topologically prior"))
}

fn require_node_type(node: &Node, expected: Type) -> Result<(), Diagnostics> {
    if node.ty != expected {
        return Err(lowering_diagnostic(
            node.span,
            &format!(
                "VIR op produces {}, expected {}",
                node.ty.name(),
                expected.name()
            ),
        ));
    }
    Ok(())
}

fn require_value(
    node: &Node,
    value: &LoweredSlot,
    expected_type: &Type,
    expected_representation: ValueRepresentation,
) -> Result<(), Diagnostics> {
    if &value.ty != expected_type || value.representation != expected_representation {
        return Err(lowering_diagnostic(
            node.span,
            &format!(
                "VIR input has {} {:?}, expected {} {:?}",
                value.ty.name(),
                value.representation,
                expected_type.name(),
                expected_representation
            ),
        ));
    }
    Ok(())
}

fn require_input_count(node: &Node, expected: usize) -> Result<(), Diagnostics> {
    if node.inputs.len() != expected {
        return Err(lowering_diagnostic(
            node.span,
            &format!(
                "VIR op has {} inputs, expected {expected}",
                node.inputs.len()
            ),
        ));
    }
    Ok(())
}

fn lowering_diagnostic(span: Span, construct: &str) -> Diagnostics {
    Diagnostics::one(Diagnostic {
        code: DiagnosticCode::LoweringUnsupported,
        primary: span,
        labels: Vec::new(),
        payload: DiagnosticPayload::Unsupported {
            construct: construct.to_owned(),
        },
    })
}
