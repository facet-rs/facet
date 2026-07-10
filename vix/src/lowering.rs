//! VIR island lowering to architecture-neutral Weavy bytecode.

use std::collections::BTreeMap;
use std::fmt::Write;

use weavy::mem::Layout;
use weavy::task::{Fn as WeavyFn, Op as WeavyOp, Program as WeavyProgram};

use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticPayload, Diagnostics};
use crate::runtime::{DemandKey, DemandPreimage, RecipeId};
use crate::support::Span;
use crate::vir::{Island, NodeId, Op};

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
    if output.op != Op::Expect || output.inputs.len() != 1 {
        return Err(lowering_diagnostic(
            output.span,
            "the first Weavy lowerer accepts Expect(Bool) islands",
        ));
    }
    let input = island
        .nodes
        .iter()
        .find(|node| node.id == output.inputs[0])
        .ok_or_else(|| lowering_diagnostic(output.span, "missing Expect input"))?;
    let Op::Bool(value) = input.op else {
        return Err(lowering_diagnostic(
            input.span,
            "the first Weavy lowerer accepts a Bool literal condition",
        ));
    };

    let program = WeavyProgram {
        fns: vec![WeavyFn {
            frame: Layout { size: 8, align: 8 },
            code: vec![
                WeavyOp::Trace { id: input.id.0 },
                WeavyOp::ConstI64 {
                    dst: 0,
                    value: i64::from(value),
                },
                WeavyOp::Trace { id: output.id.0 },
                WeavyOp::Ret { src: 0, size: 8 },
            ],
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
