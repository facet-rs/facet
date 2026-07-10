//! Vix IR: typed demand wiring above Weavy's execution vocabulary.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use crate::support::Span;

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FunctionId(pub u32);

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParameterId(pub u32);

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u32);

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeRef {
    pub function: FunctionId,
    pub node: NodeId,
}

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
    Parameter(ParameterId),
    Call(FunctionId),
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

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ParameterKind {
    Positional,
    Named,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub id: ParameterId,
    pub node: NodeId,
    pub name: String,
    pub ty: Type,
    pub kind: ParameterKind,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub id: FunctionId,
    pub name: String,
    pub span: Span,
    pub parameters: Vec<Parameter>,
    pub return_type: Type,
    pub nodes: Vec<Node>,
    pub output: Option<NodeId>,
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
    pub callees: Vec<Function>,
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
                "fn f{} {} -> {} @{}..{}",
                function.id.0,
                function.name,
                function.return_type.name(),
                function.span.start,
                function.span.end
            );
            for parameter in &function.parameters {
                let _ = writeln!(
                    out,
                    "  param p{} n{} {:?} {}: {}",
                    parameter.id.0,
                    parameter.node.0,
                    parameter.kind,
                    parameter.name,
                    parameter.ty.name()
                );
            }
            for node in &function.nodes {
                let _ = writeln!(
                    out,
                    "  n{} {:?} {:?} <- {:?} @{}..{}",
                    node.id.0, node.ty, node.op, node.inputs, node.span.start, node.span.end
                );
            }
            if let Some(output) = function.output {
                let _ = writeln!(out, "  return n{}", output.0);
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
                    .collect::<Vec<_>>();
                let mut seen = BTreeSet::from([function.id]);
                let mut callees = Vec::new();
                collect_callees(self, &nodes, &mut seen, &mut callees);
                Island {
                    id: IslandId(index as u32),
                    function: function.id,
                    function_name: function.name.clone(),
                    nodes,
                    output,
                    callees,
                }
            })
            .collect();
        PartitionedTest {
            name: test.name.clone(),
            islands,
        }
    }
}

fn collect_callees(
    module: &Module,
    nodes: &[Node],
    seen: &mut BTreeSet<FunctionId>,
    order: &mut Vec<Function>,
) {
    for node in nodes {
        let Op::Call(callee) = &node.op else {
            continue;
        };
        let callee = *callee;
        if !seen.insert(callee) {
            continue;
        }
        let function = &module.functions[callee.0 as usize];
        let mut needed = function
            .parameters
            .iter()
            .map(|parameter| parameter.node)
            .collect::<BTreeSet<_>>();
        if let Some(output) = function.output {
            collect_dependencies(function, output, &mut needed);
        }
        let mut sliced = function.clone();
        sliced.nodes.retain(|node| needed.contains(&node.id));
        order.push(sliced.clone());
        collect_callees(module, &sliced.nodes, seen, order);
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
    pub(crate) fn local_function_ids(&self) -> BTreeMap<FunctionId, u32> {
        let mut ids = BTreeMap::from([(self.function, 0)]);
        for (index, function) in self.callees.iter().enumerate() {
            let local = u32::try_from(index + 1).expect("function closure fits u32");
            ids.insert(function.id, local);
        }
        ids
    }

    /// Canonical recipe preimage for this island's transitive closure. Local
    /// function indices make unrelated module declaration order irrelevant;
    /// source spans and island ids remain attribution, not identity.
    #[must_use]
    pub fn canonical_recipe_bytes(&self) -> Vec<u8> {
        let function_ids = self.local_function_ids();
        let mut bytes = Vec::new();
        frame(&mut bytes, b"vix.vir.recipe.v2");

        let mut entry = Vec::new();
        frame(&mut entry, b"entry");
        frame(&mut entry, self.function_name.as_bytes());
        for node in &self.nodes {
            frame(&mut entry, &canonical_node(node, &function_ids));
        }
        frame(&mut entry, &self.output.0.to_le_bytes());
        frame(&mut bytes, &entry);

        for function in &self.callees {
            frame(
                &mut bytes,
                &canonical_function(
                    function,
                    *function_ids
                        .get(&function.id)
                        .expect("callee has a closure-local id"),
                    &function_ids,
                ),
            );
        }
        bytes
    }
}

fn canonical_function(
    function: &Function,
    local_id: u32,
    function_ids: &BTreeMap<FunctionId, u32>,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    frame(&mut bytes, b"function");
    frame(&mut bytes, &local_id.to_le_bytes());
    frame(&mut bytes, function.name.as_bytes());
    frame(&mut bytes, type_name(function.return_type));
    frame(
        &mut bytes,
        &(function.parameters.len() as u64).to_le_bytes(),
    );
    for parameter in &function.parameters {
        let mut encoded = Vec::new();
        frame(&mut encoded, &parameter.id.0.to_le_bytes());
        frame(&mut encoded, &parameter.node.0.to_le_bytes());
        frame(&mut encoded, parameter.name.as_bytes());
        frame(&mut encoded, type_name(parameter.ty));
        frame(
            &mut encoded,
            match parameter.kind {
                ParameterKind::Positional => b"positional",
                ParameterKind::Named => b"named",
            },
        );
        frame(&mut bytes, &encoded);
    }
    for node in &function.nodes {
        frame(&mut bytes, &canonical_node(node, function_ids));
    }
    match function.output {
        Some(output) => frame(&mut bytes, &output.0.to_le_bytes()),
        None => frame(&mut bytes, b"no-output"),
    }
    bytes
}

fn canonical_node(node: &Node, function_ids: &BTreeMap<FunctionId, u32>) -> Vec<u8> {
    let mut bytes = Vec::new();
    frame(&mut bytes, &node.id.0.to_le_bytes());
    frame(&mut bytes, type_name(node.ty));
    frame(
        &mut bytes,
        &[
            match node.effect.kind {
                EffectKind::Pure => 0,
                EffectKind::Codata => 1,
            },
            u8::from(node.effect.fallible),
            u8::from(node.effect.placed),
        ],
    );
    let mut op = Vec::new();
    match &node.op {
        Op::Bool(value) => op.extend_from_slice(&[0, u8::from(*value)]),
        Op::Expect => op.push(1),
        Op::Yield => op.push(2),
        Op::Int(value) => {
            op.push(3);
            op.extend_from_slice(&value.to_le_bytes());
        }
        Op::Add => op.push(4),
        Op::Sub => op.push(5),
        Op::Mul => op.push(6),
        Op::Eq => op.push(7),
        Op::Ne => op.push(8),
        Op::String(value) => {
            op.push(9);
            op.extend_from_slice(value.as_bytes());
        }
        Op::Parameter(parameter) => {
            op.push(10);
            op.extend_from_slice(&parameter.0.to_le_bytes());
        }
        Op::Call(function) => {
            op.push(11);
            op.extend_from_slice(
                &function_ids
                    .get(function)
                    .expect("called function belongs to the island closure")
                    .to_le_bytes(),
            );
        }
    }
    frame(&mut bytes, &op);
    frame(&mut bytes, &(node.inputs.len() as u64).to_le_bytes());
    for input in &node.inputs {
        frame(&mut bytes, &input.0.to_le_bytes());
    }
    bytes
}

const fn type_name(ty: Type) -> &'static [u8] {
    match ty {
        Type::Bool => b"bool",
        Type::Int => b"int",
        Type::Check => b"check",
        Type::StreamCheck => b"stream-check",
        Type::String => b"string",
    }
}

fn frame(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    out.extend_from_slice(bytes);
}
