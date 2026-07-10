//! Vix IR: typed demand wiring above Weavy's execution vocabulary.

use std::collections::BTreeSet;
use std::fmt::Write;

use crate::support::Span;

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FunctionId(pub u32);

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u32);

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IslandId(pub u32);

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Type {
    Bool,
    Int,
    Check,
    StreamCheck,
    String,
}

impl Type {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bool => "Bool",
            Self::Int => "Int",
            Self::Check => "Check",
            Self::StreamCheck => "Stream<Check>",
            Self::String => "String",
        }
    }
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum EffectKind {
    Pure,
    Codata,
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectFacts {
    pub kind: EffectKind,
    pub fallible: bool,
    pub placed: bool,
}

impl EffectFacts {
    pub const PURE: Self = Self {
        kind: EffectKind::Pure,
        fallible: false,
        placed: false,
    };

    pub const CODATA: Self = Self {
        kind: EffectKind::Codata,
        fallible: false,
        placed: false,
    };
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Op {
    Bool(bool),
    Int(i64),
    Add,
    Sub,
    Mul,
    Eq,
    Ne,
    Expect,
    Yield,
    String(String),
}

/// One SSA-like operation. Dependencies are explicit node ids; no Rust
/// callback identity or host pointer can enter VIR.
///
/// r[impl machine.ir.vix-level]
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub id: NodeId,
    pub span: Span,
    pub ty: Type,
    pub effect: EffectFacts,
    pub inputs: Vec<NodeId>,
    pub op: Op,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub id: FunctionId,
    pub name: String,
    pub span: Span,
    pub nodes: Vec<Node>,
    pub yielded_checks: Vec<NodeId>,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct Test {
    pub name: String,
    pub function: FunctionId,
}

#[derive(facet::Facet, Clone, Debug, Default, PartialEq, Eq)]
pub struct Module {
    pub functions: Vec<Function>,
    pub tests: Vec<Test>,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct Island {
    pub id: IslandId,
    pub function: FunctionId,
    pub function_name: String,
    pub nodes: Vec<Node>,
    pub output: NodeId,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PartitionedTest {
    pub name: String,
    pub islands: Vec<Island>,
}

impl Module {
    /// Deterministic human/tool inspection form. Spans remain visible here but
    /// are excluded from recipe identity.
    ///
    /// r[impl machine.ir.inspectable]
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        for function in &self.functions {
            let _ = writeln!(
                out,
                "fn f{} {} @{}..{}",
                function.id.0, function.name, function.span.start, function.span.end
            );
            for node in &function.nodes {
                let _ = writeln!(
                    out,
                    "  n{} {:?} {:?} <- {:?} @{}..{}",
                    node.id.0, node.ty, node.op, node.inputs, node.span.start, node.span.end
                );
            }
        }
        for test in &self.tests {
            let _ = writeln!(out, "test {} = f{}", test.name, test.function.0);
        }
        out
    }

    /// Cut each yielded check into its own eager pure island. `yield` remains
    /// the codata edge and is intentionally outside the island interior.
    ///
    /// r[impl machine.island.partition]
    #[must_use]
    pub fn partition_test(&self, test: &Test) -> PartitionedTest {
        let function = &self.functions[test.function.0 as usize];
        let islands = function
            .yielded_checks
            .iter()
            .enumerate()
            .map(|(index, &output)| {
                let mut needed = BTreeSet::new();
                collect_dependencies(function, output, &mut needed);
                let nodes = function
                    .nodes
                    .iter()
                    .filter(|node| needed.contains(&node.id))
                    .cloned()
                    .collect();
                Island {
                    id: IslandId(index as u32),
                    function: function.id,
                    function_name: function.name.clone(),
                    nodes,
                    output,
                }
            })
            .collect();
        PartitionedTest {
            name: test.name.clone(),
            islands,
        }
    }
}

fn collect_dependencies(function: &Function, node: NodeId, needed: &mut BTreeSet<NodeId>) {
    if !needed.insert(node) {
        return;
    }
    let node = &function.nodes[node.0 as usize];
    for &input in &node.inputs {
        collect_dependencies(function, input, needed);
    }
}

impl Island {
    /// Canonical recipe preimage for the first compiler epoch. It is framed by
    /// explicit tags and node ids and deliberately excludes source spans and
    /// island ids.
    #[must_use]
    pub fn canonical_recipe_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        frame(&mut bytes, b"vix.vir.recipe.v1");
        frame(&mut bytes, self.function_name.as_bytes());
        for node in &self.nodes {
            frame(&mut bytes, &node.id.0.to_le_bytes());
            frame(
                &mut bytes,
                match node.ty {
                    Type::Bool => b"bool",
                    Type::Int => b"int",
                    Type::Check => b"check",
                    Type::StreamCheck => b"stream-check",
                    Type::String => b"string",
                },
            );
            match &node.op {
                Op::Bool(value) => frame(&mut bytes, &[0, u8::from(*value)]),
                Op::Expect => frame(&mut bytes, &[1]),
                Op::Yield => frame(&mut bytes, &[2]),
                Op::Int(value) => {
                    let mut encoded = Vec::with_capacity(9);
                    encoded.push(3);
                    encoded.extend_from_slice(&value.to_le_bytes());
                    frame(&mut bytes, &encoded);
                }
                Op::Add => frame(&mut bytes, &[4]),
                Op::Sub => frame(&mut bytes, &[5]),
                Op::Mul => frame(&mut bytes, &[6]),
                Op::Eq => frame(&mut bytes, &[7]),
                Op::Ne => frame(&mut bytes, &[8]),
                Op::String(value) => {
                    let mut encoded = Vec::with_capacity(1 + value.len());
                    encoded.push(9);
                    encoded.extend_from_slice(value.as_bytes());
                    frame(&mut bytes, &encoded);
                }
            }
            for input in &node.inputs {
                frame(&mut bytes, &input.0.to_le_bytes());
            }
        }
        frame(&mut bytes, &self.output.0.to_le_bytes());
        bytes
    }
}

fn frame(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    out.extend_from_slice(bytes);
}
