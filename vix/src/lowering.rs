//! VIR island lowering to architecture-neutral Weavy bytecode.

use std::collections::BTreeMap;
use std::fmt::Write;

use weavy::mem::Layout;
use weavy::task::{
    ArgCopy, Fn as WeavyFn, FnId as WeavyFnId, Op as WeavyOp, Program as WeavyProgram,
};

use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticPayload, Diagnostics};
use crate::runtime::{DemandKey, DemandPreimage, FrameSlot, RecipeId, SchemaId};
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
    let context = LoweringContext {
        function_ids: &function_ids,
        functions: &functions,
        trace_ids: &trace_ids,
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
    let function_context = FunctionLoweringContext {
        id: function,
        parameters,
        allow_constants,
    };
    let output_node = nodes
        .iter()
        .find(|node| node.id == output)
        .ok_or_else(|| lowering_diagnostic(Span { start: 0, end: 0 }, "missing function output"))?;
    let frame_size = FrameSlot::frame_size(nodes.len())
        .ok_or_else(|| lowering_diagnostic(output_node.span, "function frame size overflow"))?;
    let mut values = BTreeMap::new();
    let mut code = Vec::with_capacity(nodes.len().saturating_mul(2) + 1);
    for (index, node) in nodes.iter().enumerate() {
        if values.contains_key(&node.id) {
            return Err(lowering_diagnostic(node.span, "duplicate VIR node id"));
        }
        let dst = FrameSlot::for_word(index)
            .ok_or_else(|| lowering_diagnostic(node.span, "Weavy frame offset overflow"))?;
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
        if let Some(op) = lowered.op {
            code.push(op);
        }
        values.insert(
            node.id,
            LoweredSlot {
                slot: dst,
                ty: node.ty,
                representation: lowered.representation,
            },
        );
    }
    let output_slot = values
        .get(&output)
        .map(|value| value.slot.byte_offset())
        .ok_or_else(|| {
            lowering_diagnostic(output_node.span, "function output has no frame slot")
        })?;
    code.push(WeavyOp::Ret {
        src: output_slot,
        size: FrameSlot::word_size(),
    });
    Ok(WeavyFn {
        frame: Layout {
            size: frame_size,
            align: FrameSlot::word_align(),
        },
        code,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValueRepresentation {
    Word,
    RealizedHandle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LoweredSlot {
    slot: FrameSlot,
    ty: Type,
    representation: ValueRepresentation,
}

struct LoweredNode {
    op: Option<WeavyOp>,
    representation: ValueRepresentation,
}

struct FunctionLoweringContext<'a> {
    id: FunctionId,
    parameters: &'a [crate::vir::Parameter],
    allow_constants: bool,
}

fn lower_node(
    node: &Node,
    dst: FrameSlot,
    values: &BTreeMap<NodeId, LoweredSlot>,
    function: &FunctionLoweringContext<'_>,
    context: &LoweringContext<'_>,
    constants: &mut Vec<ValueConstant>,
) -> Result<LoweredNode, Diagnostics> {
    let dst_slot = dst;
    let dst = dst.byte_offset();
    let (op, representation) = match &node.op {
        Op::Bool(value) => {
            require_input_count(node, 0)?;
            require_node_type(node, Type::Bool)?;
            (
                Some(WeavyOp::ConstI64 {
                    dst,
                    value: i64::from(*value),
                }),
                ValueRepresentation::Word,
            )
        }
        Op::Int(value) => {
            require_input_count(node, 0)?;
            require_node_type(node, Type::Int)?;
            (
                Some(WeavyOp::ConstI64 { dst, value: *value }),
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
            (None, ValueRepresentation::RealizedHandle)
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
            (None, representation_for_type(node.ty, node.span)?)
        }
        Op::Call(callee) => {
            let op = lower_call_node(node, dst_slot, values, *callee, context)?;
            (Some(op), representation_for_type(node.ty, node.span)?)
        }
        Op::Add | Op::Sub | Op::Mul => {
            require_node_type(node, Type::Int)?;
            let (a, b) = binary_values(node, values)?;
            require_value(node, a, Type::Int, ValueRepresentation::Word)?;
            require_value(node, b, Type::Int, ValueRepresentation::Word)?;
            let op = match &node.op {
                Op::Add => WeavyOp::AddI64 {
                    dst,
                    a: a.slot.byte_offset(),
                    b: b.slot.byte_offset(),
                },
                Op::Sub => WeavyOp::SubI64 {
                    dst,
                    a: a.slot.byte_offset(),
                    b: b.slot.byte_offset(),
                },
                Op::Mul => WeavyOp::MulI64 {
                    dst,
                    a: a.slot.byte_offset(),
                    b: b.slot.byte_offset(),
                },
                _ => unreachable!("matched arithmetic VIR op"),
            };
            (Some(op), ValueRepresentation::Word)
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
            let operand_representation = match a.ty {
                Type::Bool | Type::Int => ValueRepresentation::Word,
                Type::String => ValueRepresentation::RealizedHandle,
                Type::Check | Type::StreamCheck => {
                    return Err(lowering_diagnostic(
                        node.span,
                        "equality is not defined for this VIR type",
                    ));
                }
            };
            require_value(node, a, a.ty, operand_representation)?;
            require_value(node, b, a.ty, operand_representation)?;
            let op = if matches!(&node.op, Op::Eq) {
                WeavyOp::EqI64 {
                    dst,
                    a: a.slot.byte_offset(),
                    b: b.slot.byte_offset(),
                }
            } else {
                WeavyOp::NeI64 {
                    dst,
                    a: a.slot.byte_offset(),
                    b: b.slot.byte_offset(),
                }
            };
            (Some(op), ValueRepresentation::Word)
        }
        Op::Expect => {
            require_node_type(node, Type::Check)?;
            require_input_count(node, 1)?;
            let condition = input_value(node, values, 0)?;
            require_value(node, condition, Type::Bool, ValueRepresentation::Word)?;
            (
                Some(WeavyOp::CopyI64 {
                    dst,
                    src: condition.slot.byte_offset(),
                }),
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
    Ok(LoweredNode { op, representation })
}

fn lower_call_node(
    node: &Node,
    dst: FrameSlot,
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
    if target.return_type != node.ty || target.output.is_none() {
        return Err(lowering_diagnostic(
            node.span,
            "call result does not match the called function",
        ));
    }
    require_input_count(node, target.parameters.len())?;

    let mut args = Vec::with_capacity(target.parameters.len());
    for (index, parameter) in target.parameters.iter().enumerate() {
        let source = input_value(node, values, index)?;
        require_value(
            node,
            source,
            parameter.ty,
            representation_for_type(parameter.ty, node.span)?,
        )?;
        let parameter_index = target
            .nodes
            .iter()
            .position(|candidate| candidate.id == parameter.node)
            .ok_or_else(|| lowering_diagnostic(node.span, "called parameter has no frame node"))?;
        let parameter_slot = FrameSlot::for_word(parameter_index).ok_or_else(|| {
            lowering_diagnostic(node.span, "called parameter frame offset overflow")
        })?;
        args.push(ArgCopy {
            src: source.slot.byte_offset(),
            dst: parameter_slot.byte_offset(),
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
        ret: dst.byte_offset(),
    })
}

fn representation_for_type(ty: Type, span: Span) -> Result<ValueRepresentation, Diagnostics> {
    match ty {
        Type::Bool | Type::Int | Type::Check => Ok(ValueRepresentation::Word),
        Type::String => Ok(ValueRepresentation::RealizedHandle),
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
        .copied()
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
    value: LoweredSlot,
    expected_type: Type,
    expected_representation: ValueRepresentation,
) -> Result<(), Diagnostics> {
    if value.ty != expected_type || value.representation != expected_representation {
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
