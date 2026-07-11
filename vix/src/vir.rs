//! Vix IR: typed demand wiring above Weavy's execution vocabulary.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use crate::diagnostic::{Diagnostic, Diagnostics};
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

/// Stable identifier for a static yield site within a test generator.
#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct YieldSiteId(pub u32);

/// One static yield site: a parameterized pure check recipe together with the
/// typed values it captures. A site publishes a check descriptor only when its
/// owning control arm is taken at runtime; untaken arms publish nothing, so no
/// phantom passing checks exist.
///
/// `check` roots a pure `Op::Expect` recipe in the enclosing function's node
/// list, and that recipe's transitive inputs are the captured values. Recipe
/// identity is span-insensitive (see [`canonical_recipe`]); `span` is source
/// provenance only and never enters identity.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct YieldSite {
    pub id: YieldSiteId,
    pub check: NodeId,
    pub span: Span,
}

/// One arm of a generator [`GeneratorStep::Match`]. The arm body is itself a
/// generator body; the sites inside it are owned by this arm and publish only
/// when the arm is taken.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct GeneratorArm {
    pub variant: u32,
    /// Nodes binding the arm pattern's payload projections.
    pub bindings: Vec<NodeId>,
    pub body: GeneratorBody,
}

/// One ordered step in a test generator: either a static yield site or real
/// control whose arms are themselves generator bodies.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum GeneratorStep {
    /// Publish one static yield site unconditionally at this position.
    Yield(YieldSite),
    /// Real variant dispatch on a scrutinee value; only the taken arm publishes
    /// its owned sites. Arms are exhaustive over the scrutinee enum.
    Match {
        scrutinee: NodeId,
        arms: Vec<GeneratorArm>,
    },
    /// Real two-way dispatch on a Bool condition value.
    If {
        condition: NodeId,
        consequent: GeneratorBody,
        alternative: GeneratorBody,
    },
}

/// The lowered body of a `#[test] -> Stream<Check>` generator: an ordered
/// sequence of control/yield steps over the enclosing function's pure nodes.
/// A flat generator (only `Yield` steps) is exactly the historical static
/// yield list the runner already executes; branch-dependent steps are the
/// dynamic codata boundary this checkpoint introduces.
#[derive(facet::Facet, Clone, Debug, Default, PartialEq, Eq)]
pub struct GeneratorBody {
    pub steps: Vec<GeneratorStep>,
}

/// One control edge on the path from the generator root to a yield site. An
/// empty owner path means the site publishes unconditionally.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GeneratorControl {
    MatchArm { scrutinee: NodeId, variant: u32 },
    IfConsequent { condition: NodeId },
    IfAlternative { condition: NodeId },
}

/// A yield site paired with the control path that owns it.
#[derive(Clone, Debug)]
pub struct OwnedYieldSite<'a> {
    pub owner: Vec<GeneratorControl>,
    pub site: &'a YieldSite,
}

impl GeneratorBody {
    /// Every yield site in control/source order, each paired with the control
    /// path that owns it.
    #[must_use]
    pub fn owned_sites(&self) -> Vec<OwnedYieldSite<'_>> {
        let mut out = Vec::new();
        self.collect_owned_sites(&mut Vec::new(), &mut out);
        out
    }

    fn collect_owned_sites<'a>(
        &'a self,
        path: &mut Vec<GeneratorControl>,
        out: &mut Vec<OwnedYieldSite<'a>>,
    ) {
        for step in &self.steps {
            match step {
                GeneratorStep::Yield(site) => out.push(OwnedYieldSite {
                    owner: path.clone(),
                    site,
                }),
                GeneratorStep::Match { scrutinee, arms } => {
                    for arm in arms {
                        path.push(GeneratorControl::MatchArm {
                            scrutinee: *scrutinee,
                            variant: arm.variant,
                        });
                        arm.body.collect_owned_sites(path, out);
                        path.pop();
                    }
                }
                GeneratorStep::If {
                    condition,
                    consequent,
                    alternative,
                } => {
                    path.push(GeneratorControl::IfConsequent {
                        condition: *condition,
                    });
                    consequent.collect_owned_sites(path, out);
                    path.pop();
                    path.push(GeneratorControl::IfAlternative {
                        condition: *condition,
                    });
                    alternative.collect_owned_sites(path, out);
                    path.pop();
                }
            }
        }
    }

    /// Whether any yield site is owned by a branch rather than published
    /// unconditionally. The static runner can only execute flat generators; a
    /// conditional generator is the explicit runtime seam.
    #[must_use]
    pub fn has_conditional_sites(&self) -> bool {
        self.owned_sites()
            .iter()
            .any(|owned| !owned.owner.is_empty())
    }

    /// The unconditional top-level sites, in order — the flat static-runner view.
    #[must_use]
    pub fn unconditional_sites(&self) -> Vec<&YieldSite> {
        self.steps
            .iter()
            .filter_map(|step| match step {
                GeneratorStep::Yield(site) => Some(site),
                _ => None,
            })
            .collect()
    }
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ArrayMapGrainKey {
    InputPosition,
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArrayMapGrain {
    pub key: ArrayMapGrainKey,
    pub origin: ArrayMapGrainKey,
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
    /// Keyed codata recipe. A stream node has no realized payload in a Weavy
    /// frame; a terminal operation such as `collect` lowers the recipe.
    Stream {
        key: Box<Type>,
        value: Box<Type>,
    },
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
    pub fn stream(key: Type, value: Type) -> Self {
        Self::Stream {
            key: Box::new(key),
            value: Box::new(value),
        }
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
    pub fn stream_types(&self) -> Option<(&Type, &Type)> {
        match self {
            Self::Stream { key, value } => Some((key, value)),
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
            Self::Stream { key, value } => {
                format!("Stream<{}, {}>", key.name(), value.name())
            }
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
            Self::Stream { .. } => Some(0),
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
            Self::Array(element) => element.equality_is_structural(),
            Self::Map { .. } | Self::Set(_) => false,
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
            Self::Check | Self::StreamCheck | Self::Stream { .. } => false,
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
            Self::Check | Self::StreamCheck | Self::Stream { .. } => false,
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
    /// Transform each independently demandable input position through one
    /// typed callable while preserving that position as the output origin.
    ///
    /// r[impl lang.collection.array-map]
    ArrayMap {
        grain: ArrayMapGrain,
    },
    /// Fold authored array positions left-to-right through one typed callable.
    ArrayFold,
    /// Partition the final authored position from the remaining prefix.
    ArraySplitLast,
    /// Test whether every authored array position satisfies one typed predicate.
    ArrayAll,
    /// Test whether any authored array position satisfies one typed predicate.
    ArrayAny,
    /// Test whether an array holds an element structurally equal to a given value.
    ArrayContains,
    /// Permute a dense array into ascending structural-semantic order,
    /// preserving every duplicate element.
    ArraySorted,
    /// Publish one taken generator yield site to the task's append-only codata
    /// log. This is a control-only construction effect: it emits the site's
    /// stable provenance selector and never lowers or evaluates the site's
    /// `Op::Expect` check operands. It is synthesised only by the generator-task
    /// builder, never by source lowering.
    PublishSite(YieldSiteId),
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
    MapValues,
    /// Add one element to a set.
    SetAdd,
    /// Combine two sets.
    SetConcat,
    SetHas,
    SetLen,
    SetValues,
    /// View authored array positions as stable stream keys.
    ArrayStream,
    /// Keep rows whose values satisfy a typed predicate without renumbering
    /// their stable keys.
    StreamFilter,
    /// Materialize a keyed codata recipe as its canonical Map value.
    StreamCollect,
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
    /// The lowered generator/codata body of this test.
    pub generator: GeneratorBody,
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
    pub array_map_partitions: Vec<ArrayMapPartition>,
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ArrayMapExecutionShape {
    FusedProjection,
    MaterializedLoop,
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArrayMapPartition {
    pub node: NodeRef,
    pub shape: ArrayMapExecutionShape,
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
                let mut array_map_partitions =
                    collect_array_map_partitions(function.id, &nodes, output);
                for callee in &callees {
                    if let Some(output) = callee.output {
                        array_map_partitions.extend(collect_array_map_partitions(
                            callee.id,
                            &callee.nodes,
                            output,
                        ));
                    }
                }
                Island {
                    id: IslandId(index as u32),
                    function: function.id,
                    function_name: function.name.clone(),
                    nodes,
                    output,
                    callees,
                    array_map_partitions,
                }
            })
            .collect();
        PartitionedTest {
            name: test.name.clone(),
            islands,
        }
    }

    /// Build the verified-lowering island for a test's generator task: one
    /// program that runs only the generator's real `Match`/`If` control over its
    /// scrutinees and publishes the taken yield sites through the append-only
    /// codata log. It never lowers a site's `Op::Expect` check operands; those
    /// stay ordinary pure demand work drained through the check islands. This is
    /// the zero-dynamic-key base case of the general provenance-keyed protocol:
    /// control chooses which sites publish, and each published descriptor is the
    /// site's stable [`YieldSiteId`] selector.
    ///
    /// Returns a typed diagnostic — never a panic — when a scrutinee or condition
    /// embeds VIR control flow, which the general (non zero-dynamic-key) protocol
    /// will remap. Pure helper calls in scrutinees/conditions are supported: their
    /// synthetic transitive callees and array-map partitions are collected exactly
    /// as [`Module::partition_test`] collects a check island's.
    pub fn generator_task_island(&self, test: &Test) -> Result<Island, Diagnostics> {
        let source = &self.functions[test.function.0 as usize];
        let mut builder = GeneratorTaskBuilder {
            source,
            nodes: Vec::new(),
        };
        let output = builder.lower_body(&test.generator, source.span)?;
        let nodes = builder.nodes;
        let mut seen = BTreeSet::from([test.function]);
        let mut callees = Vec::new();
        collect_callees(self, &nodes, &mut seen, &mut callees);
        let mut array_map_partitions = collect_array_map_partitions(test.function, &nodes, output);
        for callee in &callees {
            if let Some(output) = callee.output {
                array_map_partitions.extend(collect_array_map_partitions(
                    callee.id,
                    &callee.nodes,
                    output,
                ));
            }
        }
        Ok(Island {
            id: IslandId(0),
            function: test.function,
            function_name: format!("{}$generator", test.name),
            nodes,
            output,
            callees,
            array_map_partitions,
        })
    }
}

/// Assembles the synthetic VIR function that backs a test's generator task. It
/// copies each scrutinee/condition value's transitive closure from the source
/// test function and threads the generator's `Match`/`If` control, emitting an
/// [`Op::PublishSite`] for every yield site in control order. Check operands are
/// never reached — only scrutinee/condition closures are copied.
struct GeneratorTaskBuilder<'a> {
    source: &'a Function,
    nodes: Vec<Node>,
}

impl GeneratorTaskBuilder<'_> {
    fn push(
        &mut self,
        span: Span,
        ty: Type,
        effect: EffectFacts,
        inputs: Vec<NodeId>,
        op: Op,
    ) -> NodeId {
        let id = NodeId(u32::try_from(self.nodes.len()).expect("generator node index fits u32"));
        self.nodes.push(Node {
            id,
            span,
            ty,
            effect,
            inputs,
            op,
        });
        id
    }

    fn scalar_zero(&mut self, span: Span) -> NodeId {
        self.push(span, Type::Int, EffectFacts::PURE, Vec::new(), Op::Int(0))
    }

    fn range_from(&self, start: usize) -> Vec<NodeId> {
        (start..self.nodes.len())
            .map(|index| NodeId(u32::try_from(index).expect("generator node index fits u32")))
            .collect()
    }

    /// Copy a scrutinee/condition value's transitive closure into the generator
    /// function, returning its remapped id. Values are copied fresh each time so
    /// the generator frame is a self-contained, verifier-admitted program; the
    /// runtime memo/dedup collapses any repeated pure computation by identity.
    ///
    /// A scrutinee/condition that embeds a VIR control region (`If`/`Match`/
    /// `OrderedMatch`) is not remapped in this zero-dynamic-key checkpoint; it
    /// yields a typed diagnostic rather than a panic, so no valid source can
    /// crash the builder.
    fn copy_value(&mut self, id: NodeId) -> Result<NodeId, Diagnostics> {
        let node = &self.source.nodes[id.0 as usize];
        if matches!(
            node.op,
            Op::If { .. } | Op::Match { .. } | Op::OrderedMatch { .. }
        ) {
            return Err(Diagnostics::one(Diagnostic::unsupported(
                node.span,
                "generator scrutinee or condition embeds control flow",
            )));
        }
        let inputs = node
            .inputs
            .iter()
            .map(|&input| self.copy_value(input))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(self.push(
            node.span,
            node.ty.clone(),
            node.effect,
            inputs,
            node.op.clone(),
        ))
    }

    fn lower_body(&mut self, body: &GeneratorBody, span: Span) -> Result<NodeId, Diagnostics> {
        for step in &body.steps {
            self.lower_step(step)?;
        }
        Ok(self.scalar_zero(span))
    }

    fn lower_step(&mut self, step: &GeneratorStep) -> Result<(), Diagnostics> {
        match step {
            GeneratorStep::Yield(site) => {
                self.push(
                    site.span,
                    Type::Int,
                    EffectFacts::PURE,
                    Vec::new(),
                    Op::PublishSite(site.id),
                );
            }
            GeneratorStep::Match { scrutinee, arms } => {
                let span = self.source.nodes[scrutinee.0 as usize].span;
                let scrutinee = self.copy_value(*scrutinee)?;
                let mut match_arms = Vec::with_capacity(arms.len());
                for arm in arms {
                    let start = self.nodes.len();
                    for step in &arm.body.steps {
                        self.lower_step(step)?;
                    }
                    let output = self.scalar_zero(span);
                    match_arms.push(MatchArm {
                        variant: arm.variant,
                        nodes: self.range_from(start),
                        output,
                    });
                }
                self.push(
                    span,
                    Type::Int,
                    EffectFacts::PURE,
                    vec![scrutinee],
                    Op::Match { arms: match_arms },
                );
            }
            GeneratorStep::If {
                condition,
                consequent,
                alternative,
            } => {
                let span = self.source.nodes[condition.0 as usize].span;
                let condition = self.copy_value(*condition)?;
                let consequent = self.lower_control_region(consequent, span)?;
                let alternative = self.lower_control_region(alternative, span)?;
                self.push(
                    span,
                    Type::Int,
                    EffectFacts::PURE,
                    vec![condition],
                    Op::If {
                        consequent,
                        alternative,
                    },
                );
            }
        }
        Ok(())
    }

    fn lower_control_region(
        &mut self,
        body: &GeneratorBody,
        span: Span,
    ) -> Result<ControlRegion, Diagnostics> {
        let start = self.nodes.len();
        for step in &body.steps {
            self.lower_step(step)?;
        }
        let output = self.scalar_zero(span);
        Ok(ControlRegion {
            nodes: self.range_from(start),
            output,
        })
    }
}

fn collect_array_map_partitions(
    function: FunctionId,
    nodes: &[Node],
    output: NodeId,
) -> Vec<ArrayMapPartition> {
    nodes
        .iter()
        .filter(|node| matches!(node.op, Op::ArrayMap { .. }))
        .map(|map| {
            let consumers = nodes
                .iter()
                .filter(|candidate| candidate.inputs.contains(&map.id))
                .collect::<Vec<_>>();
            let value_consumers = consumers
                .iter()
                .filter(|consumer| matches!(consumer.op, Op::ArrayIndex))
                .count();
            let shape = if map.id != output
                && !consumers.is_empty()
                && value_consumers <= 1
                && consumers
                    .iter()
                    .all(|consumer| matches!(consumer.op, Op::ArrayIndex | Op::ArrayLen))
            {
                ArrayMapExecutionShape::FusedProjection
            } else {
                ArrayMapExecutionShape::MaterializedLoop
            };
            ArrayMapPartition {
                node: NodeRef {
                    function,
                    node: map.id,
                },
                shape,
            }
        })
        .collect()
}

impl PartitionedTest {
    /// Deterministic partition inspection. These choices are deliberately not
    /// part of [`Island::canonical_recipe_bytes`].
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = format!("partition {}\n", self.name);
        for island in &self.islands {
            let _ = writeln!(out, "island {} {}", island.id.0, island.function_name);
            for decision in &island.array_map_partitions {
                let _ = writeln!(
                    out,
                    "  array-map f{} n{} {:?}",
                    decision.node.function.0, decision.node.node.0, decision.shape
                );
            }
        }
        out
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
        Op::ArrayMap { grain } => {
            op.push(45);
            op.push(match grain.key {
                ArrayMapGrainKey::InputPosition => 0,
            });
            op.push(match grain.origin {
                ArrayMapGrainKey::InputPosition => 0,
            });
        }
        Op::ArrayStream => op.push(46),
        Op::StreamCollect => op.push(47),
        Op::ArrayFold => op.push(48),
        Op::StreamFilter => op.push(49),
        Op::MapValues => op.push(50),
        Op::ArraySplitLast => op.push(51),
        Op::ArrayAll => op.push(52),
        Op::ArrayAny => op.push(53),
        Op::ArrayContains => op.push(54),
        Op::ArraySorted => op.push(55),
        Op::PublishSite(site) => {
            op.push(56);
            op.extend_from_slice(&site.0.to_le_bytes());
        }
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

/// Span-insensitive identity of the pure check recipe rooted at `root` in
/// `function`. The recipe's transitive nodes are renumbered into local
/// dependency order, so the identity depends only on the recipe's structure and
/// its captured value shapes — never on source spans or the root's absolute
/// position in the function.
#[must_use]
pub fn canonical_recipe(function: &Function, root: NodeId) -> Vec<u8> {
    let mut needed = BTreeSet::new();
    collect_dependencies(function, root, &mut needed);
    // Ascending `NodeId` is topological (SSA append order), so a plain index
    // gives a deterministic local numbering independent of absolute position.
    let local: BTreeMap<NodeId, u32> = needed
        .iter()
        .enumerate()
        .map(|(index, id)| {
            (
                *id,
                u32::try_from(index).expect("recipe node count fits u32"),
            )
        })
        .collect();
    let mut bytes = Vec::new();
    frame(&mut bytes, b"vix.vir.recipe.site.v1");
    for id in &needed {
        let node = localize_node(&function.nodes[id.0 as usize], &local);
        frame(&mut bytes, &canonical_node(&node, &BTreeMap::new()));
    }
    frame(
        &mut bytes,
        &local
            .get(&root)
            .expect("recipe root is one of its own dependencies")
            .to_le_bytes(),
    );
    bytes
}

fn localize_node(node: &Node, local: &BTreeMap<NodeId, u32>) -> Node {
    let map = |id: NodeId| NodeId(local.get(&id).copied().unwrap_or(id.0));
    let mut localized = node.clone();
    localized.id = map(node.id);
    localized.inputs = node.inputs.iter().map(|&input| map(input)).collect();
    localize_control_regions(&mut localized.op, &map);
    localized
}

fn localize_control_regions(op: &mut Op, map: &impl Fn(NodeId) -> NodeId) {
    let localize_region = |region: &mut ControlRegion, map: &dyn Fn(NodeId) -> NodeId| {
        region.nodes = region.nodes.iter().map(|&node| map(node)).collect();
        region.output = map(region.output);
    };
    match op {
        Op::Match { arms } => {
            for arm in arms {
                arm.nodes = arm.nodes.iter().map(|&node| map(node)).collect();
                arm.output = map(arm.output);
            }
        }
        Op::If {
            consequent,
            alternative,
        } => {
            localize_region(consequent, map);
            localize_region(alternative, map);
        }
        Op::OrderedMatch { arms, fallback } => {
            for arm in arms {
                localize_region(&mut arm.condition, map);
                localize_region(&mut arm.body, map);
            }
            localize_region(fallback, map);
        }
        _ => {}
    }
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
        Type::Stream { key, value } => {
            let mut bytes = b"stream".to_vec();
            frame(&mut bytes, &canonical_type(key));
            frame(&mut bytes, &canonical_type(value));
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
