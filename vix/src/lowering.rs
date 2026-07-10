//! VIR island lowering to architecture-neutral Weavy bytecode.

use std::collections::BTreeMap;
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
use crate::vir::{Function, FunctionId, Island, Node, NodeId, NodeRef, Op, Type};

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

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct ValueConstant {
    pub node: NodeRef,
    pub slot: FrameSlot,
    pub schema: SchemaId,
    pub bytes: Vec<u8>,
}

/// Cached executable bytes for one VIR recipe. Per-compilation source spans
/// and per-demand memo locations deliberately live outside this artifact.
pub struct LoweringArtifact {
    pub recipe: RecipeId,
    pub demand_key: DemandKey,
    pub demand_preimage: DemandPreimage,
    pub program: WeavyProgram,
    pub constants: Vec<ValueConstant>,
}

impl LoweringArtifact {
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        for constant in &self.constants {
            let _ = writeln!(
                out,
                "constant n{} frame[{}] schema={} bytes={}",
                constant.node.node.0,
                constant.slot.byte_offset(),
                constant.schema.0.hex(),
                constant.bytes.len()
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
    let mut layouts = BTreeMap::new();
    layouts.insert(
        island.function,
        FunctionLayout::build(&island.nodes, output.span)?,
    );
    for function in &island.callees {
        layouts.insert(
            function.id,
            FunctionLayout::build(&function.nodes, function.span)?,
        );
    }
    let context = LoweringContext {
        function_ids: &function_ids,
        functions: &functions,
        trace_ids: &trace_ids,
        layouts: &layouts,
    };

    let mut constants = Vec::new();
    let mut functions_out = Vec::with_capacity(1 + island.callees.len());
    functions_out.push(lower_vir_function(
        island.function,
        &island.nodes,
        &[],
        island.output,
        true,
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
            false,
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
    })
}

struct LoweringContext<'a> {
    function_ids: &'a BTreeMap<FunctionId, u32>,
    functions: &'a BTreeMap<FunctionId, &'a Function>,
    trace_ids: &'a BTreeMap<NodeRef, u32>,
    layouts: &'a BTreeMap<FunctionId, FunctionLayout>,
}

struct FunctionLayout {
    regions: BTreeMap<NodeId, FrameRegion>,
    scratch: Option<FrameSlot>,
    frame_size: usize,
}

impl FunctionLayout {
    fn build(nodes: &[Node], span: Span) -> Result<Self, Diagnostics> {
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
            matches!(node.op, Op::Eq | Op::Ne)
                && node
                    .inputs
                    .first()
                    .and_then(|input| regions.get(input))
                    .is_some_and(|region| region.words().as_usize() > 1)
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
        let frame_size = FrameSlot::frame_size(next_word)
            .ok_or_else(|| lowering_diagnostic(span, "function frame size overflow"))?;
        Ok(Self {
            regions,
            scratch,
            frame_size,
        })
    }

    fn region(&self, node: NodeId, span: Span) -> Result<FrameRegion, Diagnostics> {
        self.regions
            .get(&node)
            .copied()
            .ok_or_else(|| lowering_diagnostic(span, "VIR node has no frame region"))
    }
}

fn lower_vir_function(
    function: FunctionId,
    nodes: &[Node],
    parameters: &[crate::vir::Parameter],
    output: NodeId,
    allow_constants: bool,
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
        allow_constants,
        layout,
    };
    let output_node = nodes
        .iter()
        .find(|node| node.id == output)
        .ok_or_else(|| lowering_diagnostic(Span { start: 0, end: 0 }, "missing function output"))?;
    let mut values = BTreeMap::new();
    let mut code = Vec::with_capacity(nodes.len().saturating_mul(2) + 1);
    for node in nodes {
        if values.contains_key(&node.id) {
            return Err(lowering_diagnostic(node.span, "duplicate VIR node id"));
        }
        let dst = layout.region(node.id, node.span)?;
        let trace_id = context
            .trace_ids
            .get(&NodeRef {
                function,
                node: node.id,
            })
            .copied()
            .ok_or_else(|| lowering_diagnostic(node.span, "VIR node has no trace attribution"))?;
        code.push(WeavyOp::Trace { id: trace_id });
        let lowered = lower_node(node, dst, &values, &function_context, context, constants)?;
        code.extend(lowered.ops);
        values.insert(
            node.id,
            LoweredSlot {
                region: dst,
                ty: node.ty.clone(),
                representation: lowered.representation,
            },
        );
    }
    let output_region = values
        .get(&output)
        .map(|value| value.region)
        .ok_or_else(|| {
            lowering_diagnostic(output_node.span, "function output has no frame region")
        })?;
    code.push(WeavyOp::Ret {
        src: output_region.start().byte_offset(),
        size: output_region
            .byte_size()
            .ok_or_else(|| lowering_diagnostic(output_node.span, "return size overflow"))?,
    });
    Ok(WeavyFn {
        frame: Layout {
            size: layout.frame_size,
            align: FrameSlot::word_align(),
        },
        code,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValueRepresentation {
    Word,
    RealizedHandle,
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
    allow_constants: bool,
    layout: &'a FunctionLayout,
}

fn lower_node(
    node: &Node,
    dst: FrameRegion,
    values: &BTreeMap<NodeId, LoweredSlot>,
    function: &FunctionLoweringContext<'_>,
    context: &LoweringContext<'_>,
    constants: &mut Vec<ValueConstant>,
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
        Op::String(value) => {
            require_input_count(node, 0)?;
            require_node_type(node, Type::String)?;
            if !function.allow_constants {
                return Err(lowering_diagnostic(
                    node.span,
                    "callee String constant requires a closure constant slot",
                ));
            }
            constants.push(ValueConstant {
                node: NodeRef {
                    function: function.id,
                    node: node.id,
                },
                slot: dst_slot,
                schema: SchemaId::named("vix.String.v1"),
                bytes: value.as_bytes().to_vec(),
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
            let op = lower_call_node(node, dst_region, values, *callee, context)?;
            (vec![op], representation_for_type(&node.ty, node.span)?)
        }
        Op::Add | Op::Sub | Op::Mul => {
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
                _ => unreachable!("matched arithmetic VIR op"),
            };
            (vec![op], ValueRepresentation::Word)
        }
        Op::Tuple => lower_aggregate_node(node, dst_region, values, AggregateKind::Tuple)?,
        Op::Record => lower_aggregate_node(node, dst_region, values, AggregateKind::Record)?,
        Op::Project { index } => lower_project_node(node, dst_region, values, *index)?,
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
    values: &BTreeMap<NodeId, LoweredSlot>,
    callee: FunctionId,
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

    let mut args = Vec::with_capacity(target.parameters.len());
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

    let callee = context
        .function_ids
        .get(&callee)
        .copied()
        .ok_or_else(|| lowering_diagnostic(node.span, "called function has no local ABI id"))?;
    Ok(WeavyOp::Call {
        callee: WeavyFnId(callee),
        args,
        ret: dst.start().byte_offset(),
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
        Type::Tuple(_) | Type::Record(_) => Ok(ValueRepresentation::InlineComposite),
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
