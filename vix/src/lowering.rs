//! VIR island lowering to architecture-neutral Weavy bytecode.

use std::collections::BTreeMap;
use std::fmt::Write;

use weavy::mem::Layout;
use weavy::task::{Fn as WeavyFn, Op as WeavyOp, Program as WeavyProgram};

use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticPayload, Diagnostics};
use crate::runtime::{DemandKey, DemandPreimage, FrameSlot, RecipeId, SchemaId};
use crate::support::Span;
use crate::vir::{Island, Node, NodeId, Op, Type};

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct SourceMapEntry {
    pub trace_id: u32,
    pub node: NodeId,
    pub span: Span,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct ValueConstant {
    pub node: NodeId,
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
                constant.node.0,
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
    island
        .nodes
        .iter()
        .filter(|node| node.op != Op::Yield)
        .map(|node| SourceMapEntry {
            trace_id: node.id.0,
            node: node.id,
            span: node.span,
        })
        .collect()
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
    let frame_size = FrameSlot::frame_size(island.nodes.len())
        .ok_or_else(|| lowering_diagnostic(output.span, "island frame size overflow"))?;
    let mut values = BTreeMap::new();
    let mut constants = Vec::new();
    let mut code = Vec::with_capacity(island.nodes.len().saturating_mul(2) + 1);
    for (index, node) in island.nodes.iter().enumerate() {
        if values.contains_key(&node.id) {
            return Err(lowering_diagnostic(node.span, "duplicate VIR node id"));
        }
        let dst = FrameSlot::for_word(index)
            .ok_or_else(|| lowering_diagnostic(node.span, "Weavy frame offset overflow"))?;
        code.push(WeavyOp::Trace { id: node.id.0 });
        let lowered = lower_node(node, dst, &values, &mut constants)?;
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
        .get(&island.output)
        .map(|value| value.slot.byte_offset())
        .ok_or_else(|| lowering_diagnostic(output.span, "island output has no frame slot"))?;
    code.push(WeavyOp::Ret {
        src: output_slot,
        size: 8,
    });
    let program = WeavyProgram {
        fns: vec![WeavyFn {
            frame: Layout {
                size: frame_size,
                align: 8,
            },
            code,
        }],
    };
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

fn lower_node(
    node: &Node,
    dst: FrameSlot,
    values: &BTreeMap<NodeId, LoweredSlot>,
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
            constants.push(ValueConstant {
                node: node.id,
                slot: dst_slot,
                schema: SchemaId::named("vix.String.v1"),
                bytes: value.as_bytes().to_vec(),
            });
            (None, ValueRepresentation::RealizedHandle)
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
