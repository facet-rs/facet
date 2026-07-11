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

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct RecordField {
    pub name: String,
    pub ty: Type,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct RecordType {
    pub name: String,
    pub fields: Vec<RecordField>,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum VariantPayload {
    Unit,
    Tuple(Vec<Type>),
    Record(Vec<RecordField>),
}

impl VariantPayload {
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Unit => 0,
            Self::Tuple(elements) => elements.len(),
            Self::Record(fields) => fields.len(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[must_use]
    pub fn field_type(&self, index: usize) -> Option<&Type> {
        match self {
            Self::Unit => None,
            Self::Tuple(elements) => elements.get(index),
            Self::Record(fields) => fields.get(index).map(|field| &field.ty),
        }
    }

    fn word_width(&self) -> Option<usize> {
        match self {
            Self::Unit => Some(0),
            Self::Tuple(elements) => elements.iter().try_fold(0usize, |width, element| {
                width.checked_add(element.word_width()?)
            }),
            Self::Record(fields) => fields.iter().try_fold(0usize, |width, field| {
                width.checked_add(field.ty.word_width()?)
            }),
        }
    }

    fn equality_is_structural(&self) -> bool {
        match self {
            Self::Unit => true,
            Self::Tuple(elements) => elements.iter().all(Type::equality_is_structural),
            Self::Record(fields) => fields.iter().all(|field| field.ty.equality_is_structural()),
        }
    }
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct EnumVariant {
    pub name: String,
    pub payload: VariantPayload,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct EnumType {
    pub name: String,
    pub variants: Vec<EnumVariant>,
}

pub const ORDERING_LESS_VARIANT: u32 = 0;
pub const ORDERING_EQUAL_VARIANT: u32 = 1;
pub const ORDERING_GREATER_VARIANT: u32 = 2;
pub const OPTION_SOME_VARIANT: u32 = 0;
pub const OPTION_NONE_VARIANT: u32 = 1;

impl EnumType {
    /// r[impl lang.value.ordering-is-enum]
    #[must_use]
    pub fn ordering() -> Self {
        Self {
            name: "Ordering".to_owned(),
            variants: ["Less", "Equal", "Greater"]
                .into_iter()
                .map(|name| EnumVariant {
                    name: name.to_owned(),
                    payload: VariantPayload::Unit,
                })
                .collect(),
        }
    }

    /// r[impl machine.value.option-no-store-alloc]
    #[must_use]
    pub fn option(inner: Type) -> Self {
        Self {
            name: format!("Option<{}>", inner.name()),
            variants: vec![
                EnumVariant {
                    name: "Some".to_owned(),
                    payload: VariantPayload::Tuple(vec![inner]),
                },
                EnumVariant {
                    name: "None".to_owned(),
                    payload: VariantPayload::Unit,
                },
            ],
        }
    }

    #[must_use]
    pub fn option_inner(&self) -> Option<&Type> {
        let [some, none] = self.variants.as_slice() else {
            return None;
        };
        let VariantPayload::Tuple(payload) = &some.payload else {
            return None;
        };
        let [inner] = payload.as_slice() else {
            return None;
        };
        (some.name == "Some"
            && none.name == "None"
            && matches!(none.payload, VariantPayload::Unit)
            && self.name == format!("Option<{}>", inner.name()))
        .then_some(inner)
    }
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct MatchArm {
    pub variant: u32,
    pub nodes: Vec<NodeId>,
    pub output: NodeId,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct ControlRegion {
    pub nodes: Vec<NodeId>,
    pub output: NodeId,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct OrderedMatchArm {
    pub condition: ControlRegion,
    pub body: ControlRegion,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Type {
    Bool,
    Int,
    Check,
    StreamCheck,
    String,
    Function {
        parameter: Box<Type>,
        result: Box<Type>,
    },
    Tuple(Vec<Type>),
    Record(RecordType),
    Enum(EnumType),
    /// A dense array whose authored positions are values, not type parameters.
    /// Its length is carried by each value rather than the type.
    Array(Box<Type>),
    /// A canonically ordered keyed value represented independently of insertion history.
    Map {
        key: Box<Type>,
        value: Box<Type>,
    },
    /// A canonically ordered set value with no observable payload column.
    Set(Box<Type>),
}

impl Type {
    #[must_use]
    pub fn ordering() -> Self {
        Self::Enum(EnumType::ordering())
    }

    #[must_use]
    pub fn option(inner: Type) -> Self {
        Self::Enum(EnumType::option(inner))
    }

    #[must_use]
    pub fn array(element: Type) -> Self {
        Self::Array(Box::new(element))
    }

    #[must_use]
    pub fn map(key: Type, value: Type) -> Self {
        Self::Map {
            key: Box::new(key),
            value: Box::new(value),
        }
    }

    #[must_use]
    pub fn set(element: Type) -> Self {
        Self::Set(Box::new(element))
    }

    #[must_use]
    pub fn array_element(&self) -> Option<&Type> {
        match self {
            Self::Array(element) => Some(element),
            _ => None,
        }
    }

    #[must_use]
    pub fn map_types(&self) -> Option<(&Type, &Type)> {
        match self {
            Self::Map { key, value } => Some((key, value)),
            _ => None,
        }
    }

    #[must_use]
    pub fn set_element(&self) -> Option<&Type> {
        match self {
            Self::Set(element) => Some(element),
            _ => None,
        }
    }

    #[must_use]
    pub fn option_inner(&self) -> Option<&Type> {
        let Self::Enum(enumeration) = self else {
            return None;
        };
        enumeration.option_inner()
    }

    #[must_use]
    pub fn name(&self) -> String {
        match self {
            Self::Bool => "Bool".to_owned(),
            Self::Int => "Int".to_owned(),
            Self::Check => "Check".to_owned(),
            Self::StreamCheck => "Stream<Check>".to_owned(),
            Self::String => "String".to_owned(),
            Self::Function { parameter, result } => {
                format!("fn({}) -> {}", parameter.name(), result.name())
            }
            Self::Tuple(elements) => {
                let trailing_comma = if elements.len() == 1 { "," } else { "" };
                let elements = elements
                    .iter()
                    .map(Self::name)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("({elements}{trailing_comma})")
            }
            Self::Record(record) => record.name.clone(),
            Self::Enum(enumeration) => enumeration.name.clone(),
            Self::Array(element) => format!("[{}]", element.name()),
            Self::Map { key, value } => format!("Map<{}, {}>", key.name(), value.name()),
            Self::Set(element) => format!("Set<{}>", element.name()),
        }
    }

    /// Number of inline Weavy frame words occupied by a realized value.
    /// Codata streams do not have an island-interior value representation.
    #[must_use]
    pub fn word_width(&self) -> Option<usize> {
        match self {
            Self::Bool
            | Self::Int
            | Self::Check
            | Self::String
            | Self::Array(_)
            | Self::Map { .. }
            | Self::Set(_) => Some(1),
            Self::Function { .. } => Some(2),
            Self::StreamCheck => None,
            Self::Tuple(elements) => elements.iter().try_fold(0usize, |width, element| {
                width.checked_add(element.word_width()?)
            }),
            Self::Record(record) => record.fields.iter().try_fold(0usize, |width, field| {
                width.checked_add(field.ty.word_width()?)
            }),
            Self::Enum(enumeration) => enumeration
                .variants
                .iter()
                .try_fold(0usize, |widest, variant| {
                    Some(widest.max(variant.payload.word_width()?))
                })?
                .checked_add(1),
        }
    }

    #[must_use]
    pub fn equality_is_structural(&self) -> bool {
        match self {
            Self::Bool | Self::Int | Self::String => true,
            Self::Array(_) | Self::Map { .. } | Self::Set(_) => false,
            Self::Function { .. } => false,
            Self::Tuple(elements) => elements.iter().all(Self::equality_is_structural),
            Self::Record(record) => record
                .fields
                .iter()
                .all(|field| field.ty.equality_is_structural()),
            Self::Enum(enumeration) => enumeration
                .variants
                .iter()
                .all(|variant| variant.payload.equality_is_structural()),
            Self::Check | Self::StreamCheck => false,
        }
    }

    /// r[related lang.value.structural-order]
    #[must_use]
    pub fn structural_order_is_defined(&self) -> bool {
        match self {
            Self::Bool | Self::Int | Self::String => true,
            Self::Array(_) | Self::Map { .. } | Self::Set(_) => false,
            Self::Function { .. } => false,
            Self::Tuple(elements) => elements.iter().all(Self::structural_order_is_defined),
            Self::Record(record) => record
                .fields
                .iter()
                .all(|field| field.ty.structural_order_is_defined()),
            Self::Enum(enumeration) => {
                enumeration
                    .variants
                    .iter()
                    .all(|variant| match &variant.payload {
                        VariantPayload::Unit => true,
                        VariantPayload::Tuple(elements) => {
                            elements.iter().all(Self::structural_order_is_defined)
                        }
                        VariantPayload::Record(fields) => fields
                            .iter()
                            .all(|field| field.ty.structural_order_is_defined()),
                    })
            }
            Self::Check | Self::StreamCheck => false,
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
    Closure(FunctionId),
    CallValue,
    Tuple,
    Record,
    Project {
        index: u32,
    },
    Variant {
        variant: u32,
    },
    VariantProject {
        variant: u32,
        field: u32,
    },
    Match {
        arms: Vec<MatchArm>,
    },
    Compare,
    If {
        consequent: ControlRegion,
        alternative: ControlRegion,
    },
    OrderedMatch {
        arms: Vec<OrderedMatchArm>,
        fallback: ControlRegion,
    },
    Div,
    IsVariant {
        variant: u32,
    },
    /// Build a dense array from its authored positions.
    Array,
    /// Read a dynamic position from a dense array.
    ArrayIndex,
    /// Read the dense array's value-level arity.
    ArrayLen,
    /// Add one element to a dense array, producing a fresh value.
    ArrayAppend,
    /// Concatenate two dense arrays, producing a fresh value.
    ArrayConcat,
    /// Construct a canonical map from alternating key/value inputs.
    Map,
    /// Construct a canonical set from element inputs.
    Set,
    /// Add one new map row, failing on a duplicate key.
    MapAdd,
    /// Combine two disjoint maps, failing on an overlapping key.
    MapConcat,
    /// Insert or replace one map row deliberately.
    MapWith,
    /// Address one map value by key, failing on absence.
    MapGet,
    /// Test map membership without demanding the value.
    MapHas,
    MapLen,
    MapKeys,
    /// Add one element to a set.
    SetAdd,
    /// Combine two sets.
    SetConcat,
    SetHas,
    SetLen,
    SetValues,
    /// Concatenate two immutable strings.
    StringConcat,
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
    pub records: Vec<RecordType>,
    pub enums: Vec<EnumType>,
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
        for record in &self.records {
            let fields = record
                .fields
                .iter()
                .map(|field| format!("{}: {}", field.name, field.ty.name()))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(out, "struct {} {{ {fields} }}", record.name);
        }
        for enumeration in &self.enums {
            let variants = enumeration
                .variants
                .iter()
                .map(|variant| match &variant.payload {
                    VariantPayload::Unit => variant.name.clone(),
                    VariantPayload::Tuple(elements) => format!(
                        "{}({})",
                        variant.name,
                        elements
                            .iter()
                            .map(Type::name)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    VariantPayload::Record(fields) => format!(
                        "{} {{ {} }}",
                        variant.name,
                        fields
                            .iter()
                            .map(|field| format!("{}: {}", field.name, field.ty.name()))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                })
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(out, "enum {} {{ {variants} }}", enumeration.name);
        }
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
                let mut nodes = function
                    .nodes
                    .iter()
                    .filter(|node| needed.contains(&node.id))
                    .cloned()
                    .collect::<Vec<_>>();
                prune_control_regions(&mut nodes, &needed);
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
        let callee = match &node.op {
            Op::Call(callee) | Op::Closure(callee) => *callee,
            _ => continue,
        };
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
        prune_control_regions(&mut sliced.nodes, &needed);
        order.push(sliced.clone());
        collect_callees(module, &sliced.nodes, seen, order);
    }
}

fn prune_control_regions(nodes: &mut [Node], needed: &BTreeSet<NodeId>) {
    for node in nodes {
        match &mut node.op {
            Op::Match { arms } => {
                for arm in arms {
                    arm.nodes.retain(|node| needed.contains(node));
                }
            }
            Op::If {
                consequent,
                alternative,
            } => {
                consequent.nodes.retain(|node| needed.contains(node));
                alternative.nodes.retain(|node| needed.contains(node));
            }
            Op::OrderedMatch { arms, fallback } => {
                for arm in arms {
                    arm.condition.nodes.retain(|node| needed.contains(node));
                    arm.body.nodes.retain(|node| needed.contains(node));
                }
                fallback.nodes.retain(|node| needed.contains(node));
            }
            _ => {}
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
    match &node.op {
        Op::Match { arms } => {
            for arm in arms {
                collect_dependencies(function, arm.output, needed);
            }
        }
        Op::If {
            consequent,
            alternative,
        } => {
            collect_dependencies(function, consequent.output, needed);
            collect_dependencies(function, alternative.output, needed);
        }
        Op::OrderedMatch { arms, fallback } => {
            for arm in arms {
                collect_dependencies(function, arm.condition.output, needed);
                collect_dependencies(function, arm.body.output, needed);
            }
            collect_dependencies(function, fallback.output, needed);
        }
        _ => {}
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
    frame(&mut bytes, &canonical_type(&function.return_type));
    frame(
        &mut bytes,
        &(function.parameters.len() as u64).to_le_bytes(),
    );
    for parameter in &function.parameters {
        let mut encoded = Vec::new();
        frame(&mut encoded, &parameter.id.0.to_le_bytes());
        frame(&mut encoded, &parameter.node.0.to_le_bytes());
        frame(&mut encoded, parameter.name.as_bytes());
        frame(&mut encoded, &canonical_type(&parameter.ty));
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
    frame(&mut bytes, &canonical_type(&node.ty));
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
        Op::Tuple => op.push(12),
        Op::Project { index } => {
            op.push(13);
            op.extend_from_slice(&index.to_le_bytes());
        }
        Op::Record => op.push(14),
        Op::Variant { variant } => {
            op.push(15);
            op.extend_from_slice(&variant.to_le_bytes());
        }
        Op::VariantProject { variant, field } => {
            op.push(16);
            op.extend_from_slice(&variant.to_le_bytes());
            op.extend_from_slice(&field.to_le_bytes());
        }
        Op::Match { arms } => {
            op.push(17);
            frame(&mut op, &(arms.len() as u64).to_le_bytes());
            for arm in arms {
                let mut encoded = Vec::new();
                frame(&mut encoded, &arm.variant.to_le_bytes());
                frame(&mut encoded, &(arm.nodes.len() as u64).to_le_bytes());
                for node in &arm.nodes {
                    frame(&mut encoded, &node.0.to_le_bytes());
                }
                frame(&mut encoded, &arm.output.0.to_le_bytes());
                frame(&mut op, &encoded);
            }
        }
        Op::Compare => op.push(18),
        Op::If {
            consequent,
            alternative,
        } => {
            op.push(19);
            for region in [consequent, alternative] {
                frame(&mut op, &canonical_control_region(region));
            }
        }
        Op::OrderedMatch { arms, fallback } => {
            op.push(20);
            frame(&mut op, &(arms.len() as u64).to_le_bytes());
            for arm in arms {
                let mut encoded = Vec::new();
                frame(&mut encoded, &canonical_control_region(&arm.condition));
                frame(&mut encoded, &canonical_control_region(&arm.body));
                frame(&mut op, &encoded);
            }
            frame(&mut op, &canonical_control_region(fallback));
        }
        Op::Closure(function) => {
            op.push(21);
            op.extend_from_slice(
                &function_ids
                    .get(function)
                    .expect("closure function belongs to the island closure")
                    .to_le_bytes(),
            );
        }
        Op::CallValue => op.push(22),
        Op::Div => op.push(23),
        Op::IsVariant { variant } => {
            op.push(24);
            op.extend_from_slice(&variant.to_le_bytes());
        }
        Op::Array => op.push(25),
        Op::ArrayIndex => op.push(26),
        Op::ArrayLen => op.push(27),
        Op::ArrayAppend => op.push(28),
        Op::ArrayConcat => op.push(29),
        Op::Map => op.push(30),
        Op::Set => op.push(31),
        Op::MapAdd => op.push(32),
        Op::MapConcat => op.push(33),
        Op::MapWith => op.push(34),
        Op::MapGet => op.push(35),
        Op::MapHas => op.push(36),
        Op::MapLen => op.push(37),
        Op::MapKeys => op.push(38),
        Op::SetAdd => op.push(39),
        Op::SetConcat => op.push(40),
        Op::SetHas => op.push(41),
        Op::SetLen => op.push(42),
        Op::SetValues => op.push(43),
        Op::StringConcat => op.push(44),
    }
    frame(&mut bytes, &op);
    frame(&mut bytes, &(node.inputs.len() as u64).to_le_bytes());
    for input in &node.inputs {
        frame(&mut bytes, &input.0.to_le_bytes());
    }
    bytes
}

fn canonical_control_region(region: &ControlRegion) -> Vec<u8> {
    let mut encoded = Vec::new();
    frame(&mut encoded, &(region.nodes.len() as u64).to_le_bytes());
    for node in &region.nodes {
        frame(&mut encoded, &node.0.to_le_bytes());
    }
    frame(&mut encoded, &region.output.0.to_le_bytes());
    encoded
}

pub(crate) fn canonical_type(ty: &Type) -> Vec<u8> {
    match ty {
        Type::Bool => b"bool".to_vec(),
        Type::Int => b"int".to_vec(),
        Type::Check => b"check".to_vec(),
        Type::StreamCheck => b"stream-check".to_vec(),
        Type::String => b"string".to_vec(),
        Type::Function { parameter, result } => {
            let mut bytes = b"function".to_vec();
            frame(&mut bytes, &canonical_type(parameter));
            frame(&mut bytes, &canonical_type(result));
            bytes
        }
        Type::Tuple(elements) => {
            let mut bytes = b"tuple".to_vec();
            frame(&mut bytes, &(elements.len() as u64).to_le_bytes());
            for element in elements {
                frame(&mut bytes, &canonical_type(element));
            }
            bytes
        }
        Type::Record(record) => {
            let mut bytes = b"nominal-record".to_vec();
            frame(&mut bytes, record.name.as_bytes());
            frame(&mut bytes, &(record.fields.len() as u64).to_le_bytes());
            for field in &record.fields {
                let mut encoded = Vec::new();
                frame(&mut encoded, field.name.as_bytes());
                frame(&mut encoded, &canonical_type(&field.ty));
                frame(&mut bytes, &encoded);
            }
            bytes
        }
        Type::Array(element) => {
            let mut bytes = b"array".to_vec();
            frame(&mut bytes, &canonical_type(element));
            bytes
        }
        Type::Map { key, value } => {
            let mut bytes = b"map".to_vec();
            frame(&mut bytes, &canonical_type(key));
            frame(&mut bytes, &canonical_type(value));
            bytes
        }
        Type::Set(element) => {
            let mut bytes = b"set".to_vec();
            frame(&mut bytes, &canonical_type(element));
            bytes
        }
        Type::Enum(enumeration) => {
            let mut bytes = b"nominal-enum".to_vec();
            frame(&mut bytes, enumeration.name.as_bytes());
            frame(
                &mut bytes,
                &(enumeration.variants.len() as u64).to_le_bytes(),
            );
            for variant in &enumeration.variants {
                let mut encoded = Vec::new();
                frame(&mut encoded, variant.name.as_bytes());
                match &variant.payload {
                    VariantPayload::Unit => frame(&mut encoded, b"unit"),
                    VariantPayload::Tuple(elements) => {
                        frame(&mut encoded, b"tuple");
                        frame(&mut encoded, &(elements.len() as u64).to_le_bytes());
                        for element in elements {
                            frame(&mut encoded, &canonical_type(element));
                        }
                    }
                    VariantPayload::Record(fields) => {
                        frame(&mut encoded, b"record");
                        frame(&mut encoded, &(fields.len() as u64).to_le_bytes());
                        for field in fields {
                            let mut field_bytes = Vec::new();
                            frame(&mut field_bytes, field.name.as_bytes());
                            frame(&mut field_bytes, &canonical_type(&field.ty));
                            frame(&mut encoded, &field_bytes);
                        }
                    }
                }
                frame(&mut bytes, &encoded);
            }
            bytes
        }
    }
}

fn frame(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    out.extend_from_slice(bytes);
}
