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
pub const RESULT_OK_VARIANT: u32 = 0;
pub const RESULT_ERR_VARIANT: u32 = 1;

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

    /// The type a caught failure presents as: an opaque record carrying the
    /// failure value's identity. Its field is not a legal surface identifier,
    /// so programs can branch on `Err(_)` and pass the failure along, but not
    /// forge one.
    #[must_use]
    pub fn failure_value() -> Type {
        Type::Record(RecordType {
            name: "Failure".to_owned(),
            fields: vec![RecordField {
                name: "$failure".to_owned(),
                ty: Type::String,
            }],
        })
    }

    /// The `Result` a postfix `?` produces: `Ok` carries the operand's value,
    /// `Err` the caught typed failure.
    #[must_use]
    pub fn result(inner: Type) -> Self {
        Self {
            name: format!("Result<{}>", inner.name()),
            variants: vec![
                EnumVariant {
                    name: "Ok".to_owned(),
                    payload: VariantPayload::Tuple(vec![inner]),
                },
                EnumVariant {
                    name: "Err".to_owned(),
                    payload: VariantPayload::Tuple(vec![Self::failure_value()]),
                },
            ],
        }
    }

    /// The `Ok` payload when this enum is the `?` Result shape, `None` otherwise.
    #[must_use]
    pub fn result_inner(&self) -> Option<&Type> {
        let [ok, err] = self.variants.as_slice() else {
            return None;
        };
        let (VariantPayload::Tuple(ok_payload), VariantPayload::Tuple(err_payload)) =
            (&ok.payload, &err.payload)
        else {
            return None;
        };
        let ([inner], [failure]) = (ok_payload.as_slice(), err_payload.as_slice()) else {
            return None;
        };
        (ok.name == "Ok" && err.name == "Err" && *failure == Self::failure_value()).then_some(inner)
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

/// One static yield site: a check recipe together with the typed values it
/// captures. A site publishes a check descriptor only when its owning control
/// arm is taken at runtime; untaken arms publish nothing, so no phantom passing
/// checks exist.
///
/// The recipe is structurally a [`CheckRecipe::Value`] or a
/// [`CheckRecipe::Trace`]. A value recipe roots a pure `Op::Expect` recipe in
/// the enclosing function's node list, and that recipe's transitive inputs are
/// the captured values; recipe identity is span-insensitive (see
/// [`canonical_recipe`]). A trace recipe is a descriptor over the completed run
/// that lowers to no island and demands no value. `span` is source provenance
/// only and never enters identity.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct YieldSite {
    pub id: YieldSiteId,
    pub recipe: CheckRecipe,
    pub span: Span,
}

impl YieldSite {
    /// The value-check recipe root, present exactly when this site is a value
    /// check. Trace sites carry no demandable node.
    #[must_use]
    pub fn value_check(&self) -> Option<NodeId> {
        match &self.recipe {
            CheckRecipe::Value { check } => Some(*check),
            CheckRecipe::Snapshot { value, .. } => Some(*value),
            CheckRecipe::Trace(_) => None,
        }
    }
}

/// A yielded check is structurally one of two kinds. A **value check** is an
/// ordinary demanded pure island (`expect_eq`, `expect_some`, …). A **trace
/// check** is a descriptor evaluated only after every selected value check
/// completes, against a frozen completed-run counter/event/memo snapshot. Trace
/// checks never lower to Weavy boolean islands, never issue scheduler requests,
/// and never count their own result materialization or reporting.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CheckRecipe {
    /// Roots a pure `Op::Expect` recipe in the enclosing function's node list.
    Value { check: NodeId },
    /// Publishes the captured value structurally under a stable harness name.
    /// The recipe root is the value node itself (a publication, not a boolean),
    /// so the island realizes the value and the harness renders it type-directed.
    Snapshot { value: NodeId, name: String },
    /// A post-run assertion over the frozen completed-run snapshot.
    Trace(TraceCheck),
}

/// A first-class trace check: a claim about the finished run rather than a
/// demanded value. The three initial constructors bound machinery contacts and
/// carry only a scalar ceiling, so nothing about them is demanded.
///
/// The enum is deliberately open. `never_demanded(expr)` / `demanded(expr)` /
/// `demanded_once(expr)` each carry a *described wire* ([`DescribedWire`]) — a
/// recipe/location description of an operand the check pins WITHOUT demanding
/// it. Because a [`CheckRecipe::Trace`] is never lowered to an island and the
/// runner never evaluates a trace check's operands as demands, that wire is
/// held, not consumed: the intrinsic records the invocation's recipe/argument
/// selector, never its result. The design does not eagerly evaluate operands or
/// fabricate reflection.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TraceCheck {
    /// Scheduler machinery contacts during the test are at most `bound`.
    SchedulerRequestsAtMost {
        bound: i64,
    },
    /// Distinct memo entries standing at the end of the run are at most `bound`.
    MemoEntriesAtMost {
        bound: i64,
    },
    /// Store interns during the test are at most `bound`.
    StoreInternsAtMost {
        bound: i64,
    },
    ValueIslandSpawnsAtMost {
        bound: i64,
    },
    SuccessfulAggregateFreezesAtMost {
        bound: i64,
    },
    ActiveMoltenSelectionsAtMost {
        bound: i64,
    },
    ForcedCopySelectionsAtMost {
        bound: i64,
    },
    FramedBytesAtMost {
        bound: i64,
    },
    PeakMoltenBytesAtMost {
        bound: i64,
    },
    PeakMoltenNodesAtMost {
        bound: i64,
    },
    /// A callable's verified frame was entered exactly `times` times. This is
    /// function-identity provenance, independent of scheduler or memo layout.
    FunctionCallsExactly {
        function: FunctionId,
        times: i64,
    },
    /// The described invocation was demanded at least once during the run.
    Demanded {
        wire: DescribedWire,
    },
    /// The described invocation was never demanded during the run — the operand
    /// wire was held, not consumed, so no evaluation of it occurred.
    NeverDemanded {
        wire: DescribedWire,
    },
    /// The described invocation was demanded exactly once — one demand key, one
    /// computation. Repeated identical `recipe + argument` demands memoize to a
    /// single realization; distinct arguments stay distinct.
    DemandedOnce {
        wire: DescribedWire,
    },
    /// Exactly `count` effect processes were spawned over the whole run. A
    /// memoized re-demand of the same (plan × capability) key spawns nothing,
    /// so this is the whole-run process-count claim the testing chapter names.
    RanProcesses {
        count: i64,
    },
}

/// A held description of an unevaluated invocation: which user function is
/// invoked and, for a call-site selector, the exact scalar argument identities.
/// This is the "wire" a trace-check constructor pins — it is never demanded,
/// counted, or lowered to an island; it only names what to look for in the
/// frozen completed-run demand log. Arguments are carried as their surface
/// literals and resolved to canonical value identities where the check is
/// evaluated, so this VIR type stays free of any runtime-identity dependency.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct DescribedWire {
    /// The user function whose invocation this wire describes.
    pub function: FunctionId,
    /// Literal scalar arguments of the described invocation, in order. Empty for
    /// a zero-argument callee or a name-level selector.
    pub arguments: Vec<WireArg>,
    /// A name-level selector matches every argument demand of `function`
    /// (`demanded_once(costly())` — one distinct realization total); a call-site
    /// selector matches only the exact described argument identities
    /// (`demanded_once(costly(1))`).
    pub name_level: bool,
}

/// One scalar argument literal of a [`DescribedWire`]. Only closed scalar
/// literals participate in a described selector; the wire never evaluates a
/// sub-expression to obtain an argument.
#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum WireArg {
    Int(i64),
    Bool(bool),
}

/// One arm of a generator [`GeneratorStep::Match`]. The arm body is itself a
/// generator body; the sites inside it are owned by this arm and publish only
/// when the arm is taken.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct GeneratorArm {
    pub variant: u32,
    /// A pure Bool node that is true exactly when this arm is selected.
    pub condition: NodeId,
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
    Path,
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
    /// A total order over `T` supplied by the caller. Physically an `Order<T>`
    /// value is its key-extraction recipe — a closure `fn(T) -> (K, T)` that
    /// pairs the extracted key with the source value so structural comparison of
    /// the pair sorts by key and breaks ties by the source. The recipe is an
    /// ordinary Vix closure; a consuming operation (`sorted`) reads it and never
    /// materializes an `Order<T>` value in a Weavy frame.
    Order(Box<Type>),
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
    pub fn order(subject: Type) -> Self {
        Self::Order(Box::new(subject))
    }

    #[must_use]
    pub fn order_subject(&self) -> Option<&Type> {
        match self {
            Self::Order(subject) => Some(subject),
            _ => None,
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
            Self::Path => "Path".to_owned(),
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
            Self::Order(subject) => format!("Order<{}>", subject.name()),
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
            | Self::Path
            | Self::Array(_)
            | Self::Map { .. }
            | Self::Set(_) => Some(1),
            Self::Stream { .. } => Some(0),
            // An `Order<T>` recipe is never materialized in a Weavy frame; a
            // consuming operation reads its closure recipe directly.
            Self::Order(_) => Some(0),
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
            Self::Bool | Self::Int | Self::String | Self::Path => true,
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
            Self::Check | Self::StreamCheck | Self::Stream { .. } | Self::Order(_) => false,
        }
    }

    /// r[related lang.value.structural-order]
    #[must_use]
    pub fn structural_order_is_defined(&self) -> bool {
        match self {
            Self::Bool | Self::Int | Self::String | Self::Path => true,
            Self::Array(element) => element.structural_order_is_defined(),
            Self::Map { .. } | Self::Set(_) => false,
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
            Self::Check | Self::StreamCheck | Self::Stream { .. } | Self::Order(_) => false,
        }
    }
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum EffectKind {
    Pure,
    Codata,
    /// A registered runtime-primitive demand (exec, fetch): the node's value is
    /// produced by the scheduler-owned effect path, never by in-frame Weavy
    /// code. Demanding it may spawn external work; not demanding it runs
    /// nothing.
    Effect,
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

    /// An effect-primitive demand: fallible (the termination grammar may map a
    /// run to a typed failure) and never placed here.
    pub const EFFECT: Self = Self {
        kind: EffectKind::Effect,
        fallible: true,
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
    Path(String),
    Parameter(ParameterId),
    Call(FunctionId),
    Closure(FunctionId),
    CallValue,
    /// Demand a wire input: request an unevaluated pure value from the scheduler,
    /// park until it is evaluated through the DemandPreimage + Location memo
    /// path, and resume with the typed scalar result. `input` indexes the
    /// enclosing island's [`Island::wire_inputs`]. A callee demands a wire only
    /// where it is consumed, so an unconsumed wire issues no demand.
    AwaitWire {
        input: u32,
    },
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
    /// Build the dense half-open `[from, to)` integer array in one in-frame
    /// fill. Inputs are `[from, to]`. Empty (`from == to`) is the empty array;
    /// reversed bounds (`from > to`) and a width that overflows the element
    /// count are a typed machine fault — the deliberately unspecified edge is a
    /// red seam, never a silent clamp.
    Range,
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
    /// Map each row through a typed `V -> Option<W>`, keeping the source key for
    /// every `Some` output and dropping every `None` without renumbering.
    StreamFilterMap,
    /// Expand each row through a typed `V -> Stream<K2, W>`, composing the outer
    /// key with each inner key into a deterministic tuple key.
    StreamFlatMap,
    /// Materialize a keyed codata recipe as its canonical Map value.
    StreamCollect,
    /// Select the structurally-least value whose row satisfies a typed
    /// predicate, breaking ties by the stable stream key, retaining the stream.
    StreamFindMin,
    /// Select the structurally-greatest value whose row satisfies a typed
    /// predicate, breaking ties by the stable stream key, retaining the stream.
    StreamFindMax,
    /// Remove exactly the structurally-least row (stable key tie-break) from a
    /// keyed codata recipe, realizing the remaining values as a fresh dense
    /// array in canonical structural key order with the selected row omitted.
    StreamSplitMin,
    /// Concatenate two immutable strings.
    StringConcat,
    /// Test whether a string contains a byte-identical substring.
    StringContains,
    /// Partition a string at its first delimiter occurrence.
    StringSplitOnce,
    /// Parse a canonical decimal string as an Int.
    StringParseInt,
    /// Test whether a string is a non-empty run of ASCII decimal digits.
    StringIsNumeric,
    /// Join a compiler-validated segment suffix onto a relative Path.
    PathJoin,
    /// Render a relative Path as its String spelling.
    PathToString,
    /// Render a signed Int as its canonical decimal String spelling.
    /// The operand is an inline scalar; the result is a fresh resident
    /// molten byte run, identical in byte semantics to a string literal.
    IntToString,
    /// Run a command through the exec effect primitive. The single input is the
    /// capability value (referenced by identity — its `ValueId` enters the
    /// demand preimage); `argv` is the command grammar's parse of the template.
    /// The node's value is the `ExecOutcome` the termination grammar produces;
    /// a nonzero exit is a typed language failure, never a status integer.
    ///
    /// r[impl machine.primitive.exec-outcome]
    /// r[impl machine.primitive.capabilities-by-identity]
    Exec {
        argv: Vec<String>,
    },
    /// Postfix `?`: catch the operand's demand edge. The operand becomes its
    /// own demanded island; this node's value is `Result::Ok(value)` when that
    /// demand publishes and `Result::Err(failure)` when it language-fails —
    /// the failure participates as an ordinary value, address intact.
    Try,
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

impl Function {
    /// The parameters that are demand wires: those NOT proven to be consumed on
    /// every reachable path from the function's output. A parameter absent from
    /// this set is strict — its consumption is proven on every path — and the
    /// compiler may evaluate its argument eagerly as a strictness optimization of
    /// call-by-need semantics. This is a pure, conservative property of the
    /// function body, independent of any trace descriptor: a parameter is a wire
    /// unless every reachable return path consumes it.
    ///
    /// `pick(flag) where { a, b } -> if flag { a } else { b }` classifies `a` and
    /// `b` as wires (each consumed on only one branch) and `flag` as strict;
    /// `add(a) where { b } -> a + b` classifies both as strict.
    #[must_use]
    pub fn wire_parameters(&self) -> BTreeSet<ParameterId> {
        let Some(output) = self.output else {
            return BTreeSet::new();
        };
        let mut cache = BTreeMap::new();
        let always = self.always_consumed(output, &mut cache);
        self.parameters
            .iter()
            .filter(|parameter| !always.contains(&parameter.node))
            .map(|parameter| parameter.id)
            .collect()
    }

    /// The parameter nodes consumed on every path when evaluating `node`. A
    /// node's own inputs are always evaluated before it; a branch contributes
    /// only what all of its arms share, because only one arm is taken.
    fn always_consumed(
        &self,
        node: NodeId,
        cache: &mut BTreeMap<NodeId, BTreeSet<NodeId>>,
    ) -> BTreeSet<NodeId> {
        if let Some(cached) = cache.get(&node) {
            return cached.clone();
        }
        // A VIR function body is acyclic; the placeholder only guards against a
        // malformed cycle and is overwritten with the real set below.
        cache.insert(node, BTreeSet::new());
        let this = &self.nodes[node.0 as usize];
        let mut consumed = BTreeSet::new();
        if matches!(this.op, Op::Parameter(_)) {
            consumed.insert(node);
        }
        for &input in &this.inputs {
            consumed.extend(self.always_consumed(input, cache));
        }
        match &this.op {
            Op::If {
                consequent,
                alternative,
            } => {
                let taken = self.always_consumed(consequent.output, cache);
                let untaken = self.always_consumed(alternative.output, cache);
                consumed.extend(taken.intersection(&untaken).copied());
            }
            Op::Match { arms } => {
                if let Some((first, rest)) = arms.split_first() {
                    let mut shared = self.always_consumed(first.output, cache);
                    for arm in rest {
                        let arm_consumed = self.always_consumed(arm.output, cache);
                        shared = shared.intersection(&arm_consumed).copied().collect();
                    }
                    consumed.extend(shared);
                }
            }
            Op::OrderedMatch { arms, fallback } => {
                let mut shared = self.always_consumed(fallback.output, cache);
                for arm in arms {
                    let body = self.always_consumed(arm.body.output, cache);
                    shared = shared.intersection(&body).copied().collect();
                }
                consumed.extend(shared);
            }
            _ => {}
        }
        cache.insert(node, consumed.clone());
        consumed
    }
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct Test {
    pub name: String,
    pub function: FunctionId,
    /// The lowered generator/codata body of this test.
    pub generator: GeneratorBody,
    /// Typed metadata parsed from the `#[test { … }]` attribute arguments.
    pub metadata: TestMetadata,
}

/// Typed, in-language `#[test]` metadata. Parsed once at compile time from the
/// attribute arguments; the outer enforcing runner reads it before execution.
#[derive(facet::Facet, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TestMetadata {
    pub budget: Budget,
}

/// A test execution budget. Each dimension is independently optional; a ceiling
/// present here is enforced by the outer runner, which terminates a run that
/// exceeds it. Stored as scalar units (facet-friendly); [`Budget::wall`]
/// reconstructs a [`std::time::Duration`].
#[derive(facet::Facet, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Budget {
    /// Wall-clock ceiling in nanoseconds, when a `budget_wall` was declared.
    pub wall_ns: Option<u64>,
    /// Resident-set ceiling in bytes, when a `budget_rss` was declared.
    pub rss_bytes: Option<u64>,
}

impl Budget {
    /// Whether any budget dimension is present.
    #[must_use]
    pub fn is_present(&self) -> bool {
        self.wall_ns.is_some() || self.rss_bytes.is_some()
    }

    /// The wall-clock ceiling as a [`std::time::Duration`], when declared.
    #[must_use]
    pub fn wall(&self) -> Option<std::time::Duration> {
        self.wall_ns.map(std::time::Duration::from_nanos)
    }
}

#[derive(facet::Facet, Clone, Debug, Default, PartialEq, Eq)]
pub struct Module {
    pub records: Vec<RecordType>,
    pub enums: Vec<EnumType>,
    pub functions: Vec<Function>,
    pub tests: Vec<Test>,
    pub force_molten_copy: bool,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct Island {
    pub id: IslandId,
    pub purpose: IslandPurpose,
    pub function: FunctionId,
    pub function_name: String,
    pub parameters: Vec<Parameter>,
    pub value_inputs: Vec<ValueIslandId>,
    /// Demanded wire inputs, one per [`Op::AwaitWire`] in this island's nodes and
    /// indexed by that op's `input`. Each names the value island to demand
    /// through the scheduler when the awaiting task parks: an unevaluated pure
    /// value the task requests, parks on, and resumes with. Unlike
    /// `value_inputs`, a wire input is resolved lazily — a wire under an untaken
    /// branch is never awaited, so it is never demanded.
    pub wire_inputs: Vec<ValueIslandId>,
    pub forced_copy_value: bool,
    pub nodes: Vec<Node>,
    pub output: NodeId,
    pub callees: Vec<Function>,
    pub array_map_partitions: Vec<ArrayMapPartition>,
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum IslandPurpose {
    Check,
    Generator,
    Value,
    /// Publishes a value solely so a snapshot check can render it structurally.
    /// Like [`Value`](IslandPurpose::Value) it realizes the value, but the outcome
    /// envelope is forced for every type — including scalars and strings — so the
    /// runtime always freezes a renderable structure.
    Snapshot,
}

/// Content-free canonical graph provenance for a shared value producer. This
/// names the VIR producer, never a source binding, vector/arrival ordinal,
/// content digest, or runtime handle.
#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValueIslandId {
    pub function: FunctionId,
    pub node: NodeId,
}

impl ValueIslandId {
    #[must_use]
    pub fn stable_segment(self) -> String {
        format!("f{}-n{}", self.function.0, self.node.0)
    }
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PartitionedValue {
    pub id: ValueIslandId,
    pub island: Island,
    /// Present when this value island is a hoisted user-function invocation
    /// demanded as a wire: its callee identity and exact scalar arguments. The
    /// runner records one realized demand per computed (non-memoized) evaluation
    /// so a described-wire trace check can observe the invocation. Absent for an
    /// ordinary shared-publication island.
    pub wire: Option<WireProvenance>,
}

/// The described invocation a hoisted wire value island realizes: the callee
/// user function and its exact scalar argument literals. Recorded so a
/// described-wire trace check can select this realization by callee identity and
/// argument identities.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct WireProvenance {
    pub function: FunctionId,
    pub arguments: Vec<WireArg>,
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

/// One capability parameter of a test: a value the demand root (the harness)
/// supplies by identity before any island of the test runs. It is named by the
/// same [`ValueIslandId`] space as shared publications, so consuming islands
/// list it in `value_inputs` and the runner resolves it from the same
/// published-values map.
///
/// r[impl machine.primitive.capabilities-by-identity]
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PartitionedCapability {
    pub id: ValueIslandId,
    pub parameter: ParameterId,
    pub name: String,
    pub ty: Type,
}

/// One `?` catch: after the operand island publishes (value or typed failure),
/// the runner constructs the `Result` value under this id, so consuming check
/// islands receive it as an ordinary pre-published enum input.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PartitionedCatch {
    /// The Try node's published identity.
    pub id: ValueIslandId,
    /// The caught operand island (an effect island or an ordinary value island).
    pub operand: ValueIslandId,
    /// The `Result<T>` enum type the catch constructs.
    pub result_type: Type,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PartitionedTest {
    pub name: String,
    /// Harness-supplied capability inputs, published before `values`.
    pub capabilities: Vec<PartitionedCapability>,
    pub values: Vec<PartitionedValue>,
    /// `?` catches, constructed after `values` publish and before any site runs.
    pub catches: Vec<PartitionedCatch>,
    /// Argument islands demanded lazily through force-on-park, keyed by their
    /// [`ValueIslandId`]. A consuming island's [`Island::wire_inputs`] names one
    /// of these; the runner builds a `WireDemand` from it and drives it through
    /// the memo path only when a task actually parks on the wire. Structurally
    /// equal invocations collapse to one entry, so an awaited wire memoizes to a
    /// single realization.
    pub wire_islands: Vec<PartitionedValue>,
    pub generator: Option<Island>,
    /// Value-check islands, in site order. A [`PartitionedRecipe::Value`]
    /// indexes into this vector. Trace sites contribute no island.
    pub islands: Vec<Island>,
    /// Every yield site, in [`YieldSiteId`] order, mapped to its recipe. This
    /// is the provenance-keyed site → recipe map the runner drives. It never
    /// relies on an island-vector ordinal, which shifts once trace sites are
    /// filtered out of the island vector.
    pub sites: Vec<PartitionedSite>,
}

/// One yield site paired with the partitioned recipe it resolves to. The site's
/// stable [`YieldSiteId`] is its provenance selector.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PartitionedSite {
    pub id: YieldSiteId,
    pub recipe: PartitionedRecipe,
}

/// A partitioned check recipe: an index into [`PartitionedTest::islands`] for a
/// value check, or a self-contained trace descriptor evaluated post-run.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PartitionedRecipe {
    Value { island: usize },
    Snapshot { island: usize, name: String },
    Trace(TraceCheck),
}

/// The inter-island edges an island partition stops at: shared aggregate
/// publications (`Op::Parameter` value inputs), shared scalar-invocation wires
/// (`Op::AwaitWire` consumers), and lazy call arguments (demanded inside a
/// specialized callee). Each maps a source node to the representative island the
/// runner drives for it.
#[derive(Clone, Copy)]
struct IslandBoundary<'a> {
    shared: &'a BTreeMap<NodeId, ValueIslandId>,
    wires: &'a BTreeMap<NodeId, ValueIslandId>,
    lazy_arg_reps: &'a BTreeMap<NodeId, ValueIslandId>,
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

    /// Cut each yielded value check into its own eager pure island, and resolve
    /// each yield site to a provenance-keyed recipe. `yield` remains the codata
    /// edge and is intentionally outside the island interior. A trace site
    /// resolves to a self-contained descriptor and contributes no island: it is
    /// evaluated post-run against the frozen snapshot, never lowered or demanded.
    ///
    /// r[impl machine.island.partition]
    #[must_use]
    pub fn partition_test(&self, test: &Test) -> PartitionedTest {
        self.try_partition_test(test)
            .expect("unsupported shared publication in unchecked partition inspection")
    }

    pub fn try_partition_test(&self, test: &Test) -> Result<PartitionedTest, Diagnostics> {
        let function = &self.functions[test.function.0 as usize];
        // Every site, in stable YieldSiteId order (dense across value and trace
        // sites). Island ordinals compact over value sites only, so a site is
        // keyed by its YieldSiteId, never by its position after filtering.
        let mut ordered: Vec<&YieldSite> = test
            .generator
            .owned_sites()
            .into_iter()
            .map(|owned| owned.site)
            .collect();
        ordered.sort_by_key(|site| site.id.0);
        let mut consumer_sets = BTreeMap::<NodeId, BTreeSet<PublicationConsumer>>::new();
        for site in &ordered {
            let Some(check) = site.value_check() else {
                continue;
            };
            let mut visited = BTreeSet::new();
            let mut materializers = BTreeSet::new();
            collect_publication_materializers(function, check, &mut visited, &mut materializers);
            for dependency in materializers {
                consumer_sets
                    .entry(dependency)
                    .or_default()
                    .insert(PublicationConsumer::ValueCheck(site.id));
            }
        }
        let mut generator_controls = Vec::new();
        collect_generator_control_nodes(&test.generator, &mut generator_controls);
        for control in generator_controls {
            let mut visited = BTreeSet::new();
            let mut materializers = BTreeSet::new();
            collect_publication_materializers(function, control, &mut visited, &mut materializers);
            for dependency in materializers {
                consumer_sets
                    .entry(dependency)
                    .or_default()
                    .insert(PublicationConsumer::GeneratorControl);
            }
        }
        // Capability parameters: values the demand root supplies by identity
        // (no ambient acquire). Each becomes a pre-published input every island
        // may name; the harness interns the capability value before any island
        // of this test runs.
        //
        // r[impl machine.primitive.capabilities-by-identity]
        let capabilities = function
            .parameters
            .iter()
            .map(|parameter| PartitionedCapability {
                id: self.value_island_id(function.id, parameter.node),
                parameter: parameter.id,
                name: parameter.name.clone(),
                ty: parameter.ty.clone(),
            })
            .collect::<Vec<_>>();
        let capability_ids = function
            .parameters
            .iter()
            .map(|parameter| {
                (
                    parameter.node,
                    self.value_island_id(function.id, parameter.node),
                )
            })
            .collect::<BTreeMap<_, _>>();
        // Every consumed exec node is its own effect island: an effect demand
        // never inlines into a consumer's Weavy frame, and structurally equal
        // exec expressions stay distinct demand sites whose second evaluation
        // memoizes on the (plan × capability) demand key (rung 069).
        let effect_nodes = function
            .nodes
            .iter()
            .filter(|node| {
                matches!(node.op, Op::Exec { .. }) && consumer_sets.contains_key(&node.id)
            })
            .collect::<Vec<_>>();
        let mut shared = function
            .nodes
            .iter()
            .filter(|node| {
                consumer_sets
                    .get(&node.id)
                    .is_some_and(|sites| sites.len() >= 2)
            })
            .filter(|node| is_shared_publication_candidate(node))
            .filter(|node| !matches!(node.op, Op::Exec { .. }))
            .collect::<Vec<_>>();
        let candidate_ids = shared.iter().map(|node| node.id).collect::<BTreeSet<_>>();
        shared.retain(|candidate| {
            !candidate_ids.iter().copied().any(|other| {
                if other == candidate.id {
                    return false;
                }
                let mut dependencies = BTreeSet::new();
                collect_dependencies(function, other, &mut dependencies);
                dependencies.contains(&candidate.id)
                    && consumer_sets.get(&other) == consumer_sets.get(&candidate.id)
            })
        });
        shared.sort_by_key(|node| ValueIslandId {
            function: function.id,
            node: node.id,
        });
        // A shared scalar user-invocation is a demanded WIRE, not an eagerly
        // published aggregate: it is consumed through `AwaitWire`/force-on-park so
        // its typed scalar result flows through the same memo path an ordinary
        // argument demand uses. Structurally equal invocations collapse to one wire
        // identity — two `costly(7)` calls share a single memoized realization (rung
        // 058) — while distinct arguments keep distinct wire identities (rung 059).
        // Aggregate publications remain eager shared value islands.
        let mut wire_nodes = Vec::new();
        shared.retain(|node| {
            if is_scalar_call_candidate(node) {
                wire_nodes.push(*node);
                false
            } else {
                true
            }
        });
        let shared_ids = shared
            .iter()
            .map(|node| (node.id, self.value_island_id(function.id, node.id)))
            .collect::<BTreeMap<_, _>>();
        let effect_ids = effect_nodes
            .iter()
            .map(|node| (node.id, self.value_island_id(function.id, node.id)))
            .collect::<BTreeMap<_, _>>();
        // `?` catches: each consumed Try node publishes a constructed Result
        // value; its operand becomes its own demanded island — the expression
        // edge the catch holds — unless it is already an effect island.
        let catch_nodes = function
            .nodes
            .iter()
            .filter(|node| matches!(node.op, Op::Try) && consumer_sets.contains_key(&node.id))
            .collect::<Vec<_>>();
        let mut catch_operand_nodes: Vec<&Node> = Vec::new();
        for catch in &catch_nodes {
            let operand = catch.inputs[0];
            if effect_ids.contains_key(&operand)
                || shared_ids.contains_key(&operand)
                || catch_operand_nodes.iter().any(|node| node.id == operand)
            {
                continue;
            }
            catch_operand_nodes.push(&function.nodes[operand.0 as usize]);
        }
        let catch_operand_ids = catch_operand_nodes
            .iter()
            .map(|node| (node.id, self.value_island_id(function.id, node.id)))
            .collect::<BTreeMap<_, _>>();
        let catches = catch_nodes
            .iter()
            .map(|node| PartitionedCatch {
                id: self.value_island_id(function.id, node.id),
                operand: self.value_island_id(function.id, node.inputs[0]),
                result_type: node.ty.clone(),
            })
            .collect::<Vec<_>>();
        // The pre-published inputs every island may name: ordinary shared
        // publications, harness-supplied capability values, effect
        // publications, and constructed catches all arrive through the same
        // value-input seam.
        let published_ids = {
            let mut map = shared_ids.clone();
            map.extend(capability_ids.iter().map(|(node, value)| (*node, *value)));
            map.extend(effect_ids.iter().map(|(node, value)| (*node, *value)));
            map.extend(
                catch_operand_ids
                    .iter()
                    .map(|(node, value)| (*node, *value)),
            );
            for node in &catch_nodes {
                map.insert(node.id, self.value_island_id(function.id, node.id));
            }
            map
        };
        // The non-strict argument nodes of every lazy call site: a call whose
        // callee has wire parameters demands each such argument inside the callee
        // through force-on-park, so the argument becomes its own wire island. Both
        // the shared-consumer wires (rung 058) and these lazy arguments (rung 053)
        // collapse through one structural-fingerprint map, so equal invocations
        // share one realization and distinct ones stay distinct.
        let mut lazy_arg_nodes = Vec::new();
        for node in &function.nodes {
            if let Op::Call(callee) = node.op {
                for index in wire_param_indices(&self.functions[callee.0 as usize]) {
                    lazy_arg_nodes.push(node.inputs[index]);
                }
            }
        }
        // Independently-demandable aggregate fields: a projection of a
        // projection-only local record becomes an `AwaitWire` of its own field's
        // initializer, so demanding `p.x` never computes `p.y` (rung 054).
        let projection_wires = projection_field_wires(function);
        // Collapse structurally equal wire invocations to one representative demand
        // identity. `wire_ids` maps every shared-consumer node and every fused field
        // projection to the representative wire island it awaits (rungs 058/054);
        // `lazy_arg_reps` maps every lazy call argument node to its representative
        // wire island (rung 053). `wire_islands` are the argument islands built once
        // per representative and driven lazily through force-on-park.
        let mut wire_representatives = Vec::new();
        let (wire_ids, lazy_arg_reps) = {
            let mut fingerprints = BTreeMap::new();
            let mut representative_of = BTreeMap::<String, NodeId>::new();
            let mut register = |node_id: NodeId| -> ValueIslandId {
                let fingerprint = structural_fingerprint(function, node_id, &mut fingerprints);
                let representative = *representative_of.entry(fingerprint).or_insert(node_id);
                if representative == node_id {
                    wire_representatives.push(node_id);
                }
                self.value_island_id(function.id, representative)
            };
            let mut wire_ids = wire_nodes
                .iter()
                .map(|node| (node.id, register(node.id)))
                .collect::<BTreeMap<_, _>>();
            for &(project, field_init) in &projection_wires {
                let representative = register(field_init);
                wire_ids.insert(project, representative);
            }
            let lazy_arg_reps = lazy_arg_nodes
                .iter()
                .map(|&arg| (arg, register(arg)))
                .collect::<BTreeMap<_, _>>();
            (wire_ids, lazy_arg_reps)
        };
        let values = shared
            .iter()
            .map(|node| (node, IslandPurpose::Value))
            .chain(effect_nodes.iter().map(|node| (node, IslandPurpose::Value)))
            .chain(
                catch_operand_nodes
                    .iter()
                    .map(|node| (node, IslandPurpose::Value)),
            )
            .enumerate()
            .map(|(ordinal, (node, purpose))| {
                // A value island computes itself; only OTHER pre-published
                // values (shared aggregates, capabilities, effect outcomes) are
                // inputs.
                let representative_id = self.value_island_id(function.id, node.id);
                let shared_here = published_ids
                    .iter()
                    .filter(|(candidate, _)| **candidate != node.id)
                    .map(|(candidate, value)| (*candidate, *value))
                    .collect();
                let lazy_here = lazy_reps_excluding(&lazy_arg_reps, representative_id);
                PartitionedValue {
                    id: representative_id,
                    island: self.partition_function_output_with_shared(
                        function,
                        node.id,
                        IslandId(u32::try_from(ordinal).expect("value island index fits u32")),
                        purpose,
                        &IslandBoundary {
                            shared: &shared_here,
                            wires: &BTreeMap::new(),
                            lazy_arg_reps: &lazy_here,
                        },
                    ),
                    wire: None,
                }
            })
            .collect::<Vec<_>>();
        // The argument islands demanded through force-on-park. Each computes its
        // invocation from ordinary shared inputs; a nested wire consumes another
        // wire island. The island's own representative node is not one of its
        // inputs, so it is excluded from every input map.
        let wire_islands = wire_representatives
            .iter()
            .enumerate()
            .map(|(ordinal, &node_id)| {
                let representative_id = self.value_island_id(function.id, node_id);
                let wires_here = wire_ids
                    .iter()
                    .filter(|(_, value)| **value != representative_id)
                    .map(|(candidate, value)| (*candidate, *value))
                    .collect();
                let lazy_here = lazy_reps_excluding(&lazy_arg_reps, representative_id);
                let node = &function.nodes[node_id.0 as usize];
                PartitionedValue {
                    id: representative_id,
                    island: self.partition_function_output_with_shared(
                        function,
                        node_id,
                        IslandId(u32::try_from(ordinal).expect("wire island index fits u32")),
                        IslandPurpose::Value,
                        &IslandBoundary {
                            shared: &published_ids,
                            wires: &wires_here,
                            lazy_arg_reps: &lazy_here,
                        },
                    ),
                    wire: scalar_call_provenance(function, node),
                }
            })
            .collect::<Vec<_>>();
        let generator = test
            .generator
            .has_conditional_sites()
            .then(|| self.generator_task_island_with_shared(test, &published_ids))
            .transpose()?;
        let mut islands = Vec::new();
        let mut sites = Vec::with_capacity(ordered.len());
        for site in ordered {
            let recipe = match &site.recipe {
                CheckRecipe::Value { check } => {
                    let island = islands.len();
                    islands.push(self.partition_function_output_with_shared(
                        function,
                        *check,
                        IslandId(u32::try_from(island).expect("island index fits u32")),
                        IslandPurpose::Check,
                        &IslandBoundary {
                            shared: &published_ids,
                            wires: &wire_ids,
                            lazy_arg_reps: &lazy_arg_reps,
                        },
                    ));
                    PartitionedRecipe::Value { island }
                }
                CheckRecipe::Snapshot { value, name } => {
                    // A snapshot publishes the value itself, so its island is a
                    // value publication whose output is the value node. Its
                    // dedicated purpose forces the outcome envelope for every
                    // type so scalars and strings freeze renderably too.
                    let island = islands.len();
                    islands.push(self.partition_function_output_with_shared(
                        function,
                        *value,
                        IslandId(u32::try_from(island).expect("island index fits u32")),
                        IslandPurpose::Snapshot,
                        &IslandBoundary {
                            shared: &published_ids,
                            wires: &wire_ids,
                            lazy_arg_reps: &lazy_arg_reps,
                        },
                    ));
                    PartitionedRecipe::Snapshot {
                        island,
                        name: name.clone(),
                    }
                }
                CheckRecipe::Trace(trace) => PartitionedRecipe::Trace(trace.clone()),
            };
            sites.push(PartitionedSite {
                id: site.id,
                recipe,
            });
        }
        Ok(PartitionedTest {
            name: test.name.clone(),
            capabilities,
            values,
            catches,
            wire_islands,
            generator,
            islands,
            sites,
        })
    }

    fn value_island_id(&self, function: FunctionId, node: NodeId) -> ValueIslandId {
        ValueIslandId { function, node }
    }

    fn partition_function_output_with_shared(
        &self,
        function: &Function,
        output: NodeId,
        id: IslandId,
        purpose: IslandPurpose,
        boundary: &IslandBoundary<'_>,
    ) -> Island {
        let IslandBoundary {
            shared,
            wires,
            lazy_arg_reps,
        } = *boundary;
        let stop = shared
            .keys()
            .chain(wires.keys())
            .chain(lazy_arg_reps.keys())
            .copied()
            .collect();
        let mut needed = BTreeSet::new();
        collect_dependencies_stopping_at(function, output, &stop, &mut needed);
        let mut nodes = function
            .nodes
            .iter()
            .filter(|node| needed.contains(&node.id))
            .cloned()
            .collect::<Vec<_>>();
        let mut parameters = Vec::new();
        let mut value_inputs = Vec::new();
        let mut wire_inputs = Vec::new();
        for node in &mut nodes {
            if let Some(&value) = shared.get(&node.id) {
                let id = ParameterId(
                    u32::try_from(parameters.len()).expect("shared value parameter count fits u32"),
                );
                node.op = Op::Parameter(id);
                node.inputs.clear();
                parameters.push(Parameter {
                    id,
                    node: node.id,
                    name: format!("$value_{}", value.stable_segment()),
                    ty: node.ty.clone(),
                    kind: ParameterKind::Positional,
                });
                value_inputs.push(value);
            } else if let Some(&value) = wires.get(&node.id) {
                // A demanded wire: the node becomes an await point that requests
                // this value island from the scheduler and parks until it is
                // evaluated through the memo path. It is reached — and therefore
                // demanded — only when control actually consumes it.
                let input = u32::try_from(wire_inputs.len()).expect("wire input count fits u32");
                node.op = Op::AwaitWire { input };
                node.inputs.clear();
                wire_inputs.push(value);
            }
        }
        prune_control_regions(&mut nodes, &needed);
        // A fused field projection or awaited wire consumer no longer references
        // the aggregate it was cut from, so its now-dead subtree — an unmaterialized
        // record and its unprojected field initializers — must be dropped before
        // collecting callees, or an undemanded field's invocation would execute.
        if !wires.is_empty() {
            let live = island_reachable(&nodes, output);
            nodes.retain(|node| live.contains(&node.id));
        }
        let mut seen = BTreeSet::from([function.id]);
        let mut callees = Vec::new();
        collect_callees(self, &nodes, &mut seen, &mut callees);
        // Lazy call sites: a bundled call whose callee has wire parameters keeps
        // its strict arguments as `ArgCopy` frame bindings, but each non-strict
        // argument is demanded inside the callee through force-on-park. The wire
        // argument islands are appended to this island's `wire_inputs`, giving
        // each a global index the callee's specialized `AwaitWire` nodes reference.
        // The typed call-site mapping is this island's `wire_inputs` — supplied at
        // evaluation time, never baked into the shared lowered callee — so two call
        // sites cannot cross-wire even when they share a lowering.
        let mut specialized = Vec::new();
        let mut next_specialized = u32::try_from(self.functions.len())
            .expect("module function count fits u32")
            .max(
                callees
                    .iter()
                    .map(|callee| callee.id.0 + 1)
                    .max()
                    .unwrap_or(0),
            );
        for node in &mut nodes {
            let Op::Call(callee_id) = node.op else {
                continue;
            };
            let callee = &self.functions[callee_id.0 as usize];
            let wire_positions = wire_param_indices(callee);
            if wire_positions.is_empty() {
                continue;
            }
            let wire_set: BTreeSet<usize> = wire_positions.iter().copied().collect();
            let mut strict_inputs = Vec::new();
            let mut await_map = BTreeMap::new();
            for (index, parameter) in callee.parameters.iter().enumerate() {
                let arg = node.inputs[index];
                if wire_set.contains(&index) {
                    let value = *lazy_arg_reps
                        .get(&arg)
                        .expect("lazy wire argument has a partitioned island");
                    let global =
                        u32::try_from(wire_inputs.len()).expect("wire input count fits u32");
                    wire_inputs.push(value);
                    await_map.insert(parameter.node, global);
                } else {
                    strict_inputs.push(arg);
                }
            }
            let new_id = FunctionId(next_specialized);
            next_specialized += 1;
            specialized.push(specialize_lazy_callee(callee, &await_map, new_id));
            node.op = Op::Call(new_id);
            node.inputs = strict_inputs;
        }
        if !specialized.is_empty() {
            // A lazy argument is consumed only inside its callee now, so its
            // island node is dead in this island; drop everything no longer
            // reachable from the output before collecting the callee closure.
            let live = island_reachable(&nodes, output);
            nodes.retain(|node| live.contains(&node.id));
            callees.extend(specialized);
            let referenced = referenced_function_ids(&nodes, &callees);
            callees.retain(|callee| referenced.contains(&callee.id));
        }
        let mut array_map_partitions = collect_array_map_partitions(function.id, &nodes, output);
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
            id,
            purpose,
            function: function.id,
            function_name: function.name.clone(),
            parameters,
            value_inputs,
            wire_inputs,
            forced_copy_value: matches!(purpose, IslandPurpose::Value | IslandPurpose::Snapshot)
                && self.force_molten_copy,
            nodes,
            output,
            callees,
            array_map_partitions,
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
        self.generator_task_island_with_shared(test, &BTreeMap::new())
    }

    fn generator_task_island_with_shared(
        &self,
        test: &Test,
        shared: &BTreeMap<NodeId, ValueIslandId>,
    ) -> Result<Island, Diagnostics> {
        let source = &self.functions[test.function.0 as usize];
        let mut builder = GeneratorTaskBuilder {
            source,
            nodes: Vec::new(),
            shared,
            shared_parameters: BTreeMap::new(),
            parameters: Vec::new(),
            value_inputs: Vec::new(),
        };
        let output = builder.lower_body(&test.generator, source.span)?;
        let GeneratorTaskBuilder {
            nodes,
            parameters,
            value_inputs,
            ..
        } = builder;
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
            purpose: IslandPurpose::Generator,
            function: test.function,
            function_name: format!("{}$generator", test.name),
            parameters,
            value_inputs,
            wire_inputs: Vec::new(),
            forced_copy_value: false,
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
    shared: &'a BTreeMap<NodeId, ValueIslandId>,
    shared_parameters: BTreeMap<NodeId, NodeId>,
    parameters: Vec<Parameter>,
    value_inputs: Vec<ValueIslandId>,
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
        if let Some(&parameter) = self.shared_parameters.get(&id) {
            return Ok(parameter);
        }
        let node = &self.source.nodes[id.0 as usize];
        if let Some(&value) = self.shared.get(&id) {
            let parameter = ParameterId(
                u32::try_from(self.parameters.len())
                    .expect("generator shared parameter count fits u32"),
            );
            let remapped = self.push(
                node.span,
                node.ty.clone(),
                EffectFacts::PURE,
                Vec::new(),
                Op::Parameter(parameter),
            );
            self.parameters.push(Parameter {
                id: parameter,
                node: remapped,
                name: format!("$value_{}", value.stable_segment()),
                ty: node.ty.clone(),
                kind: ParameterKind::Positional,
            });
            self.value_inputs.push(value);
            self.shared_parameters.insert(id, remapped);
            return Ok(remapped);
        }
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
        for island in self
            .values
            .iter()
            .map(|value| &value.island)
            .chain(self.islands.iter())
        {
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

/// The parameter positions of `callee` that are demand wires, in declaration
/// order. A call's argument inputs align with the callee's parameters by index
/// (positional then named), so a wire parameter at index `i` is fed by the call
/// node's input at index `i`.
/// Locally constructed records/tuples consumed only by field projections of
/// inline-word fields: each such projection is an independently-demandable
/// aggregate field. Returns `(projection node, field-initializer node)` pairs, so
/// the projection can demand only its own field's initializer through
/// force-on-park while the aggregate is never materialized and the other fields
/// are never computed. A record consumed whole (passed, returned, or a control
/// output) stays materialized and is excluded.
fn projection_field_wires(function: &Function) -> Vec<(NodeId, NodeId)> {
    let mut region_outputs = BTreeSet::new();
    for node in &function.nodes {
        match &node.op {
            Op::If {
                consequent,
                alternative,
            } => {
                region_outputs.insert(consequent.output);
                region_outputs.insert(alternative.output);
            }
            Op::Match { arms } => {
                region_outputs.extend(arms.iter().map(|arm| arm.output));
            }
            Op::OrderedMatch { arms, fallback } => {
                for arm in arms {
                    region_outputs.insert(arm.condition.output);
                    region_outputs.insert(arm.body.output);
                }
                region_outputs.insert(fallback.output);
            }
            _ => {}
        }
    }
    let mut result = Vec::new();
    for record in &function.nodes {
        if !matches!(record.op, Op::Record | Op::Tuple) {
            continue;
        }
        let mut projections = Vec::new();
        let mut whole = function.output == Some(record.id) || region_outputs.contains(&record.id);
        for other in &function.nodes {
            if other.id == record.id {
                continue;
            }
            match &other.op {
                Op::Project { index } if other.inputs == [record.id] => {
                    projections.push((other.id, *index, other.ty.clone()));
                }
                _ if other.inputs.contains(&record.id) => whole = true,
                _ => {}
            }
        }
        if whole || projections.is_empty() {
            continue;
        }
        // Only fuse when every projected field is an inline word the force-on-park
        // scalar resume supports; a composite field stays materialized.
        let projected_word = projections
            .iter()
            .all(|(_, _, ty)| matches!(ty, Type::Int | Type::Bool));
        // Only fuse when materializing the aggregate would execute a genuinely
        // undemanded field invocation — an unprojected field whose initializer is a
        // user call (rung 054's `expensive()`). A record of literals or fully
        // projected fields is materialized unchanged, so aggregate contract shapes
        // are preserved where no partial-demand laziness is observable.
        let projected: BTreeSet<u32> = projections.iter().map(|(_, index, _)| *index).collect();
        let saves_an_invocation = (0..record.inputs.len()).any(|index| {
            !projected.contains(&(index as u32))
                && matches!(
                    function.nodes[record.inputs[index].0 as usize].op,
                    Op::Call(_)
                )
        });
        if projected_word && saves_an_invocation {
            for (project, index, _) in projections {
                result.push((project, record.inputs[index as usize]));
            }
        }
    }
    result
}

/// The lazy-argument representative map for building one island, with the
/// island's own representative excluded so its own invocation is computed rather
/// than stopped at as a wire boundary.
fn lazy_reps_excluding(
    lazy_arg_reps: &BTreeMap<NodeId, ValueIslandId>,
    representative_id: ValueIslandId,
) -> BTreeMap<NodeId, ValueIslandId> {
    lazy_arg_reps
        .iter()
        .filter(|(_, value)| **value != representative_id)
        .map(|(node, value)| (*node, *value))
        .collect()
}

fn wire_param_indices(callee: &Function) -> Vec<usize> {
    let wires = callee.wire_parameters();
    callee
        .parameters
        .iter()
        .enumerate()
        .filter(|(_, parameter)| {
            // Force-on-park resumes an awaiting task with a typed scalar word, so
            // only inline-word (`Int`/`Bool`) wire parameters are demanded lazily.
            // A composite wire parameter stays a bundled `ArgCopy` binding until
            // the handle-resume seam lands.
            wires.contains(&parameter.id) && matches!(parameter.ty, Type::Int | Type::Bool)
        })
        .map(|(index, _)| index)
        .collect()
}

/// Specialize a bundled callee for one lazy call site: each wire parameter's
/// declaration node becomes an [`Op::AwaitWire`] at the call-site-specific global
/// wire-input index, and the wire parameters are removed from the frame's
/// argument list so only strict parameters remain `ArgCopy` bindings. The
/// awaited value is demanded inside the callee's own taken control region, so an
/// untaken arm never parks and never demands. `new_id` is the specialized frame's
/// island-local identity; different call sites get different `await_map`s, so a
/// shared source callee cannot hardcode the wrong task input across call sites.
fn specialize_lazy_callee(
    callee: &Function,
    await_map: &BTreeMap<NodeId, u32>,
    new_id: FunctionId,
) -> Function {
    let mut needed = callee
        .parameters
        .iter()
        .map(|parameter| parameter.node)
        .collect::<BTreeSet<_>>();
    if let Some(output) = callee.output {
        collect_dependencies(callee, output, &mut needed);
    }
    let mut sliced = callee.clone();
    sliced.id = new_id;
    sliced.nodes.retain(|node| needed.contains(&node.id));
    for node in &mut sliced.nodes {
        if let Some(&input) = await_map.get(&node.id) {
            node.op = Op::AwaitWire { input };
            node.inputs.clear();
        }
    }
    sliced
        .parameters
        .retain(|parameter| !await_map.contains_key(&parameter.node));
    // A former wire parameter was a top-level node evaluated unconditionally. As
    // an `AwaitWire` it must park only where control actually consumes it, so sink
    // each into the control regions that reference it; an untaken arm then never
    // parks and never demands.
    let await_ids: BTreeSet<NodeId> = await_map.keys().copied().collect();
    sink_await_nodes(&mut sliced.nodes, &await_ids);
    prune_control_regions(&mut sliced.nodes, &needed);
    sliced
}

/// Move each awaited wire node into the control regions that reference it, so it
/// is lowered inside that region (evaluated only when the region is taken) rather
/// than at the frame's unconditional top level.
fn sink_await_nodes(nodes: &mut [Node], await_ids: &BTreeSet<NodeId>) {
    let inputs_of: BTreeMap<NodeId, Vec<NodeId>> = nodes
        .iter()
        .map(|node| (node.id, node.inputs.clone()))
        .collect();
    let sink = |region_nodes: &mut Vec<NodeId>, output: NodeId| {
        for &wire in await_ids {
            let referenced = output == wire
                || region_nodes.iter().any(|id| {
                    inputs_of
                        .get(id)
                        .is_some_and(|inputs| inputs.contains(&wire))
                });
            if referenced && !region_nodes.contains(&wire) {
                // An `AwaitWire` has no inputs, so it is topologically first in the
                // region; inserting at the front keeps every consumer prior-ordered.
                region_nodes.insert(0, wire);
            }
        }
    };
    for node in nodes {
        match &mut node.op {
            Op::If {
                consequent,
                alternative,
            } => {
                sink(&mut consequent.nodes, consequent.output);
                sink(&mut alternative.nodes, alternative.output);
            }
            Op::Match { arms } => {
                for arm in arms {
                    sink(&mut arm.nodes, arm.output);
                }
            }
            Op::OrderedMatch { arms, fallback } => {
                for arm in arms {
                    sink(&mut arm.condition.nodes, arm.condition.output);
                    sink(&mut arm.body.nodes, arm.body.output);
                }
                sink(&mut fallback.nodes, fallback.output);
            }
            _ => {}
        }
    }
}

/// The function ids reachable from `nodes` through `Op::Call`/`Op::Closure`,
/// transitively over `functions`. Used to drop a lazy call's original bundled
/// callee once every reference has been rewritten to its specialization.
fn referenced_function_ids(nodes: &[Node], functions: &[Function]) -> BTreeSet<FunctionId> {
    let by_id: BTreeMap<FunctionId, &Function> = functions
        .iter()
        .map(|function| (function.id, function))
        .collect();
    let mut reachable = BTreeSet::new();
    let mut stack: Vec<FunctionId> = nodes.iter().filter_map(direct_callee).collect();
    while let Some(id) = stack.pop() {
        if !reachable.insert(id) {
            continue;
        }
        if let Some(function) = by_id.get(&id) {
            stack.extend(function.nodes.iter().filter_map(direct_callee));
        }
    }
    reachable
}

/// The node ids reachable from `output` within one island's own node list,
/// following data inputs and taken/untaken control-region outputs. Used to drop
/// lazy-argument nodes once a lazy call no longer references them.
fn island_reachable(nodes: &[Node], output: NodeId) -> BTreeSet<NodeId> {
    let by_id: BTreeMap<NodeId, &Node> = nodes.iter().map(|node| (node.id, node)).collect();
    let mut reachable = BTreeSet::new();
    let mut stack = vec![output];
    while let Some(id) = stack.pop() {
        if !reachable.insert(id) {
            continue;
        }
        let Some(node) = by_id.get(&id) else {
            continue;
        };
        stack.extend(node.inputs.iter().copied());
        match &node.op {
            Op::Match { arms } => {
                for arm in arms {
                    stack.push(arm.output);
                }
            }
            Op::If {
                consequent,
                alternative,
            } => {
                stack.push(consequent.output);
                stack.push(alternative.output);
            }
            Op::OrderedMatch { arms, fallback } => {
                for arm in arms {
                    stack.push(arm.condition.output);
                    stack.push(arm.body.output);
                }
                stack.push(fallback.output);
            }
            _ => {}
        }
    }
    reachable
}

fn direct_callee(node: &Node) -> Option<FunctionId> {
    match node.op {
        Op::Call(callee) | Op::Closure(callee) => Some(callee),
        _ => None,
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

/// Shared publication owns aggregate construction, never an aggregate-typed
/// view. A call, record projection, or enum payload projection may carry an
/// ordered handle, but extracting that node would launder the handle without
/// owning the producer's ordered freeze. Dense arrays have a complete freeze
/// capability; ordered Map/Set materializers remain nominated so the explicit
/// rung-138 diagnostic fires instead of silently recomputing them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum PublicationConsumer {
    GeneratorControl,
    ValueCheck(YieldSiteId),
}

/// A scalar user-function invocation whose result is a shared, memoizable
/// demand: a word-typed `Op::Call`. Two structurally equal such calls collapse
/// to one demand identity (rung 058); distinct ones stay distinct (rung 059).
/// Single-consumer scalar calls stay bundled inside their island (rung 004's
/// direct `WeavyOp::Call`); the shared cut only applies at two or more consumers.
fn is_scalar_call_candidate(node: &Node) -> bool {
    // Only genuine inline scalars (never a handle-backed aggregate, string, or
    // path) flow through the `AwaitWire`/`scalar_word` resume path.
    matches!(node.op, Op::Call(_)) && matches!(node.ty, Type::Int | Type::Bool)
}

/// The described invocation a shared scalar call realizes, when every argument is
/// a closed scalar literal. A call with a non-literal argument shares its
/// computation but carries no described-wire provenance (nothing selects it).
fn scalar_call_provenance(function: &Function, node: &Node) -> Option<WireProvenance> {
    let Op::Call(callee) = node.op else {
        return None;
    };
    let mut arguments = Vec::with_capacity(node.inputs.len());
    for &input in &node.inputs {
        match function.nodes[input.0 as usize].op {
            Op::Int(value) => arguments.push(WireArg::Int(value)),
            Op::Bool(value) => arguments.push(WireArg::Bool(value)),
            _ => return None,
        }
    }
    Some(WireProvenance {
        function: callee,
        arguments,
    })
}

/// A canonical, node-id-independent fingerprint of the pure subtree rooted at
/// `node`: its operation and the fingerprints of its inputs. Two subtrees with
/// the same fingerprint compute the same value, so they may share one demand
/// identity. This never inspects a runtime handle, arrival ordinal, or trace
/// descriptor — only the authored graph.
fn structural_fingerprint(
    function: &Function,
    node: NodeId,
    cache: &mut BTreeMap<NodeId, String>,
) -> String {
    if let Some(cached) = cache.get(&node) {
        return cached.clone();
    }
    // Guard against a malformed cycle; a well-formed VIR body is acyclic.
    cache.insert(node, format!("cycle:{}", node.0));
    let this = &function.nodes[node.0 as usize];
    let inputs = this
        .inputs
        .iter()
        .map(|&input| structural_fingerprint(function, input, cache))
        .collect::<Vec<_>>()
        .join(",");
    let fingerprint = format!("{:?}[{}]:{}", this.op, inputs, this.ty.name());
    cache.insert(node, fingerprint.clone());
    fingerprint
}

fn is_shared_publication_candidate(node: &Node) -> bool {
    if is_scalar_call_candidate(node) {
        return true;
    }
    match &node.ty {
        Type::Array(_) => true,
        Type::Map { .. } => matches!(
            node.op,
            Op::Map | Op::MapAdd | Op::MapConcat | Op::MapWith | Op::StreamCollect
        ),
        Type::Set(_) => matches!(node.op, Op::Set | Op::SetAdd | Op::SetConcat),
        Type::Tuple(_) | Type::Record(_) | Type::Enum(_)
            if type_contains_publication_aggregate(&node.ty) =>
        {
            matches!(
                node.op,
                Op::Call(_) | Op::CallValue | Op::If { .. } | Op::Match { .. } | Op::Exec { .. }
            )
        }
        _ => false,
    }
}

fn type_contains_publication_aggregate(ty: &Type) -> bool {
    match ty {
        Type::Array(_) | Type::Map { .. } | Type::Set(_) => true,
        Type::Tuple(elements) => elements.iter().any(type_contains_publication_aggregate),
        Type::Record(record) => record
            .fields
            .iter()
            .any(|field| type_contains_publication_aggregate(&field.ty)),
        Type::Enum(enumeration) => {
            enumeration
                .variants
                .iter()
                .any(|variant| match &variant.payload {
                    VariantPayload::Unit => false,
                    VariantPayload::Tuple(elements) => {
                        elements.iter().any(type_contains_publication_aggregate)
                    }
                    VariantPayload::Record(fields) => fields
                        .iter()
                        .any(|field| type_contains_publication_aggregate(&field.ty)),
                })
        }
        _ => false,
    }
}

fn collect_generator_control_nodes(body: &GeneratorBody, out: &mut Vec<NodeId>) {
    for step in &body.steps {
        match step {
            GeneratorStep::Yield(_) => {}
            GeneratorStep::Match { scrutinee, arms } => {
                out.push(*scrutinee);
                for arm in arms {
                    collect_generator_control_nodes(&arm.body, out);
                }
            }
            GeneratorStep::If {
                condition,
                consequent,
                alternative,
            } => {
                out.push(*condition);
                collect_generator_control_nodes(consequent, out);
                collect_generator_control_nodes(alternative, out);
            }
        }
    }
}

fn collect_publication_materializers(
    function: &Function,
    node: NodeId,
    visited: &mut BTreeSet<NodeId>,
    materializers: &mut BTreeSet<NodeId>,
) {
    if !visited.insert(node) {
        return;
    }
    let node = &function.nodes[node.0 as usize];
    let aggregate_view = matches!(node.ty, Type::Map { .. } | Type::Set(_))
        && !is_shared_publication_candidate(node)
        && !matches!(node.op, Op::Project { .. } | Op::VariantProject { .. });
    if aggregate_view {
        return;
    }
    if is_shared_publication_candidate(node) {
        materializers.insert(node.id);
    }
    // A `?` catch is consumed as a pre-published value like a shared aggregate:
    // registering it here makes its consumption visible so the partitioner can
    // island its operand and publish the constructed Result.
    if matches!(node.op, Op::Try) {
        materializers.insert(node.id);
    }
    for &input in &node.inputs {
        collect_publication_materializers(function, input, visited, materializers);
    }
    match &node.op {
        Op::Match { arms } => {
            for arm in arms {
                collect_publication_materializers(function, arm.output, visited, materializers);
            }
        }
        Op::If {
            consequent,
            alternative,
        } => {
            collect_publication_materializers(function, consequent.output, visited, materializers);
            collect_publication_materializers(function, alternative.output, visited, materializers);
        }
        Op::OrderedMatch { arms, fallback } => {
            for arm in arms {
                collect_publication_materializers(
                    function,
                    arm.condition.output,
                    visited,
                    materializers,
                );
                collect_publication_materializers(
                    function,
                    arm.body.output,
                    visited,
                    materializers,
                );
            }
            collect_publication_materializers(function, fallback.output, visited, materializers);
        }
        _ => {}
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

fn collect_dependencies_stopping_at(
    function: &Function,
    node: NodeId,
    stop: &BTreeSet<NodeId>,
    needed: &mut BTreeSet<NodeId>,
) {
    if !needed.insert(node) || stop.contains(&node) {
        return;
    }
    let node = &function.nodes[node.0 as usize];
    for &input in &node.inputs {
        collect_dependencies_stopping_at(function, input, stop, needed);
    }
    match &node.op {
        Op::Match { arms } => {
            for arm in arms {
                collect_dependencies_stopping_at(function, arm.output, stop, needed);
            }
        }
        Op::If {
            consequent,
            alternative,
        } => {
            collect_dependencies_stopping_at(function, consequent.output, stop, needed);
            collect_dependencies_stopping_at(function, alternative.output, stop, needed);
        }
        Op::OrderedMatch { arms, fallback } => {
            for arm in arms {
                collect_dependencies_stopping_at(function, arm.condition.output, stop, needed);
                collect_dependencies_stopping_at(function, arm.body.output, stop, needed);
            }
            collect_dependencies_stopping_at(function, fallback.output, stop, needed);
        }
        _ => {}
    }
}

impl Island {
    /// The effect node this island publishes, when its output is an effect
    /// demand. An effect island is evaluated by the registered runtime
    /// primitive, never lowered to a Weavy frame.
    #[must_use]
    pub fn effect_output(&self) -> Option<&Node> {
        let output = self.nodes.iter().find(|node| node.id == self.output)?;
        matches!(output.op, Op::Exec { .. }).then_some(output)
    }

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
        frame(&mut entry, &(self.parameters.len() as u64).to_le_bytes());
        for parameter in &self.parameters {
            let mut encoded = Vec::new();
            frame(&mut encoded, &parameter.id.0.to_le_bytes());
            frame(&mut encoded, &parameter.node.0.to_le_bytes());
            frame(&mut encoded, &canonical_type(&parameter.ty));
            frame(
                &mut encoded,
                match parameter.kind {
                    ParameterKind::Positional => b"positional",
                    ParameterKind::Named => b"named",
                },
            );
            frame(&mut entry, &encoded);
        }
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
                EffectKind::Effect => 2,
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
        Op::Path(value) => {
            op.push(80);
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
        Op::AwaitWire { input } => {
            op.push(84);
            op.extend_from_slice(&input.to_le_bytes());
        }
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
        Op::StreamFilterMap => op.push(56),
        Op::StreamFlatMap => op.push(57),
        Op::PublishSite(site) => {
            op.push(58);
            op.extend_from_slice(&site.0.to_le_bytes());
        }
        Op::StreamFindMin => op.push(59),
        Op::StreamFindMax => op.push(60),
        Op::StreamSplitMin => op.push(61),
        Op::StringContains => op.push(62),
        Op::StringSplitOnce => op.push(63),
        Op::StringParseInt => op.push(64),
        Op::StringIsNumeric => op.push(65),
        Op::PathJoin => op.push(81),
        Op::PathToString => op.push(82),
        Op::IntToString => op.push(84),
        Op::Range => op.push(83),
        Op::Exec { argv } => {
            op.push(85);
            frame(&mut op, &(argv.len() as u64).to_le_bytes());
            for argument in argv {
                frame(&mut op, argument.as_bytes());
            }
        }
        Op::Try => op.push(86),
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
        Type::Path => b"path".to_vec(),
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
        Type::Order(subject) => {
            let mut bytes = b"order".to_vec();
            frame(&mut bytes, &canonical_type(subject));
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
