//! VIR island lowering to architecture-neutral Weavy bytecode.

use std::collections::BTreeMap;
use std::fmt::Write;

use weavy::mem::Layout;
use weavy::task::{Fn as WeavyFn, Op as WeavyOp, Program as WeavyProgram};

use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticPayload, Diagnostics};
use crate::runtime::{DemandKey, DemandPreimage, RecipeId};
use crate::support::Span;
use crate::vir::{Island, Node, NodeId, Op, Type};

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct SourceMapEntry {
    pub trace_id: u32,
    pub node: NodeId,
    pub span: Span,
}

/// Cached executable bytes for one VIR recipe. Per-compilation source spans
/// and per-demand memo locations deliberately live outside this artifact.
pub struct LoweringArtifact {
    pub recipe: RecipeId,
    pub demand_key: DemandKey,
    pub demand_preimage: DemandPreimage,
    pub program: WeavyProgram,
}

impl LoweringArtifact {
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
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
    let frame_size = island
        .nodes
        .len()
        .checked_mul(8)
        .ok_or_else(|| lowering_diagnostic(output.span, "island frame size overflow"))?;
    let mut slots = BTreeMap::new();
    let mut code = Vec::with_capacity(island.nodes.len().saturating_mul(2) + 1);
    for (index, node) in island.nodes.iter().enumerate() {
        if slots.contains_key(&node.id) {
            return Err(lowering_diagnostic(node.span, "duplicate VIR node id"));
        }
        let dst = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_mul(8))
            .ok_or_else(|| lowering_diagnostic(node.span, "Weavy frame offset overflow"))?;
        code.push(WeavyOp::Trace { id: node.id.0 });
        code.push(lower_node(node, dst, &slots)?);
        slots.insert(node.id, dst);
    }
    let output_slot = slots
        .get(&island.output)
        .copied()
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
    })
}

fn lower_node(
    node: &Node,
    dst: u32,
    slots: &BTreeMap<NodeId, u32>,
) -> Result<WeavyOp, Diagnostics> {
    let op = match node.op {
        Op::Bool(value) => {
            require_input_count(node, 0)?;
            WeavyOp::ConstI64 {
                dst,
                value: i64::from(value),
            }
        }
        Op::Int(value) => {
            require_input_count(node, 0)?;
            WeavyOp::ConstI64 { dst, value }
        }
        Op::Add => {
            let (a, b) = binary_inputs(node, slots)?;
            WeavyOp::AddI64 { dst, a, b }
        }
        Op::Sub => {
            let (a, b) = binary_inputs(node, slots)?;
            WeavyOp::SubI64 { dst, a, b }
        }
        Op::Mul => {
            let (a, b) = binary_inputs(node, slots)?;
            WeavyOp::MulI64 { dst, a, b }
        }
        Op::Eq => {
            let (a, b) = binary_inputs(node, slots)?;
            WeavyOp::EqI64 { dst, a, b }
        }
        Op::Ne => {
            let (a, b) = binary_inputs(node, slots)?;
            WeavyOp::NeI64 { dst, a, b }
        }
        Op::Expect => {
            require_input_count(node, 1)?;
            WeavyOp::CopyI64 {
                dst,
                src: input_slot(node, slots, 0)?,
            }
        }
        Op::Yield => {
            return Err(lowering_diagnostic(
                node.span,
                "codata Yield appeared inside an island",
            ));
        }
    };
    Ok(op)
}

fn binary_inputs(node: &Node, slots: &BTreeMap<NodeId, u32>) -> Result<(u32, u32), Diagnostics> {
    require_input_count(node, 2)?;
    Ok((input_slot(node, slots, 0)?, input_slot(node, slots, 1)?))
}

fn input_slot(
    node: &Node,
    slots: &BTreeMap<NodeId, u32>,
    index: usize,
) -> Result<u32, Diagnostics> {
    let input = node
        .inputs
        .get(index)
        .ok_or_else(|| lowering_diagnostic(node.span, "missing VIR input"))?;
    slots
        .get(input)
        .copied()
        .ok_or_else(|| lowering_diagnostic(node.span, "VIR input is not topologically prior"))
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
