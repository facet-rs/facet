//! Surface bindings: the projection from registered primitives (and vix-source
//! functions) onto vix-language names and their placement in name resolution.
//!
//! # Design contract
//!
//! One Rust primitive projects exactly **one** binding with **one** name.
//! Behavioural modes (`observe`/`refresh`, `json`/`toml` decode) are *request
//! fields the primitive reads* — never extra primitives, and never extra
//! compiler intrinsics. Ergonomic aliases are ordinary vix functions bound over
//! that single primitive (`refresh(x) = observe(x, refresh: true)`).
//!
//! This separates three concerns the codebase currently tangles:
//!   - **registry identity** — a primitive is a [`PrimitiveId`] matched by
//!     schema (`runtime::primitive`); it carries no surface name;
//!   - **surface name + placement** — this module: what a primitive (or vix
//!     function) is *called* in source, and whether it lives in the prelude or
//!     under a `::`-path;
//!   - **request construction** — how surface arguments fold into the
//!     primitive's request record ([`RequestShape`]).
//!
//! # Status
//!
//! **The projection lives on the primitive.** Each registered primitive
//! declares its own [`RawPrimitive::surface_name`](crate::runtime::RawPrimitive) and
//! [`RawPrimitive::request_shape`](crate::runtime::RawPrimitive); [`builtin_bindings`]
//! *harvests* the [`BindingRegistry`] from [`runtime::builtin_primitives`]
//! rather than maintaining a second table that names every primitive by hand.
//! `compiler::lower_value` recognizes built-in primitives through
//! [`prelude_primitive`] — the callee name maps to a [`PrimitiveId`] here, in
//! data, instead of scattered `== "decode"` / `effect_intrinsic` string
//! matches.
//!
//! **Request construction is data for the uniform primitives.** `fetch` and
//! `observe` lower through a [`RequestShape`] ([`request_shape`], keyed by
//! [`PrimitiveId`]): their arity, per-argument roles (a lowered
//! [`ArgRole::Value`] with its required type, or a [`Selector`] enum read at
//! lower time), request record, result type, and target primitive are all
//! data, consumed by one generic builder rather than a bespoke Rust arm each.
//!
//! Not yet on a shape: `decode`/`try_decode` (compile-time constant folding and
//! expected-type-derived targets don't reduce to a record shape) — both are
//! hand-registered onto the single [`decode_primitive_id`]; `request_shape`
//! returns `None` for it, and the compiler keeps a typed builder. The
//! `fixture_*`/`untar` dedicated VIR ops are not primitives at all — they are
//! [`Intrinsic`]s, matched here by name and dispatched to the compiler's
//! existing typed builder.
//!
//! Still on the compiler side: the binder consults [`is_prelude_name`] (a name
//! set derived from these bindings) but does not yet route full prelude/module
//! *resolution* through [`BindingRegistry::prelude`]/[`BindingRegistry::qualified`],
//! and the vix-fn `source` strings here are documentation — the actual injection
//! is `crate::stdlib`'s `PRELUDE_FUNCTIONS`. Language constructs (`Some`, `None`,
//! `by_key`, `range`, the `expect*`/trace checks) and the `.text()` method
//! surface are deliberately outside this registry.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::LazyLock;

use crate::runtime::{self, PrimitiveId};
use crate::vir::decode_primitive_id;

// Re-exported so existing call sites (`compiler.rs`) can keep spelling these
// `crate::binding::RequestShape` etc. — the types now live next to
// `RawPrimitive` in `runtime`, since it is the primitive that declares them.
pub use crate::runtime::{ArgRole, RequestShape, Selector, SelectorVariant};

/// A non-empty `::`-separated module path (`caps`, `some::ns::inner`).
///
/// The smart constructor rejects an empty path and empty segments, so a
/// [`Placement::Module`] can never name "nowhere" — an unresolvable placement
/// is not representable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModulePath(Vec<String>);

impl ModulePath {
    /// Build a module path from its segments, or `None` if the path is empty or
    /// any segment is empty.
    #[must_use]
    pub fn new(segments: impl IntoIterator<Item = impl Into<String>>) -> Option<Self> {
        let segments: Vec<String> = segments.into_iter().map(Into::into).collect();
        if segments.is_empty() || segments.iter().any(String::is_empty) {
            return None;
        }
        Some(Self(segments))
    }

    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.0
    }

    /// Render as a `::`-joined path for diagnostics.
    #[must_use]
    pub fn display(&self) -> String {
        self.0.join("::")
    }
}

/// Where a bound name lives in vix name resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Placement {
    /// Injected into every module's scope; callable with no `use` (today's
    /// `fetch`, `observe`). This is the prelude layer the binder currently
    /// defers rather than resolves.
    Prelude,
    /// Reached through a qualified path or `use module::name` — e.g.
    /// `some::ns::cool_function`.
    Module(ModulePath),
}

/// Which dedicated VIR op an intrinsic call lowers to. These are **not**
/// primitives — `fixture_tree`/`fixture_registry` never cross an authority
/// boundary and `untar` is a deterministic pure transform — so there is no
/// request record to shape; the compiler keeps a hand-written typed builder
/// (`lower_effect_intrinsic`) for each. This enum is only the *name → which
/// builder arm* map, so that map is data rather than a string match in
/// `compiler.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intrinsic {
    /// `fixture_tree(name)` — a named fixture tree (a dedicated machine op).
    FixtureTree,
    /// `fixture_registry()` — the fixture registry (a dedicated machine op).
    FixtureRegistry,
    /// `untar(blob)` — expand a blob to a tree (a dedicated machine op).
    Untar,
}

/// The vix type a method dispatches on — the kind of its receiver. Moved here
/// from the compiler so that method dispatch is data in the binding layer, not a
/// second hardcoded registry parallel to this one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiverType {
    Array,
    String,
    Map,
    Set,
    Stream,
    Int,
    Path,
    ByteStream,
    Tree,
    TreeEntry,
    Blob,
    Registry,
}

impl ReceiverType {
    /// Classify a lowered receiver's [`crate::vir::Type`] into the method
    /// dispatch key, or `None` if the type carries no builtin methods.
    #[must_use]
    pub fn from_vir_type(ty: &crate::vir::Type) -> Option<Self> {
        use crate::vir::{ExternKind, Type};
        match ty {
            Type::Array(_) => Some(Self::Array),
            Type::String => Some(Self::String),
            Type::Map { .. } => Some(Self::Map),
            Type::Set(_) => Some(Self::Set),
            Type::Stream { .. } => Some(Self::Stream),
            Type::Int => Some(Self::Int),
            Type::Path => Some(Self::Path),
            Type::Record(record) if record.name == "ByteStream" => Some(Self::ByteStream),
            Type::Extern(ExternKind::Tree) => Some(Self::Tree),
            Type::Extern(ExternKind::TreeEntry) => Some(Self::TreeEntry),
            Type::Extern(ExternKind::Blob) => Some(Self::Blob),
            Type::Extern(ExternKind::Registry) => Some(Self::Registry),
            _ => None,
        }
    }
}

/// Which dedicated VIR op a receiver method lowers to. The compiler owns the
/// bespoke per-op lowering (`compiler::lower_method_call`); this enum is only the
/// name → which-arm map, so that map is data in the binding layer rather than a
/// second closed registry in `compiler.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MethodOp {
    ArrayLen,
    ArrayMap,
    ArrayFold,
    ArraySplitLast,
    ArrayAll,
    ArrayAny,
    ArrayContains,
    StringTrim,
    StringContains,
    StringSplitOnce,
    StringParseInt,
    StringIsNumeric,
    StringLines,
    ArraySorted,
    ArrayStream,
    IntRem,
    MapGet,
    MapHas,
    MapLen,
    MapKeys,
    MapValues,
    MapWith,
    SetHas,
    SetLen,
    SetValues,
    StreamFilter,
    StreamFilterMap,
    StreamFlatMap,
    StreamCollect,
    StreamFindMin,
    StreamFindMax,
    StreamSplitMin,
    PathToString,
    IntToString,
    ByteStreamCollect,
    ByteStreamTrim,
    TreeGlob,
    TreeEntryText,
    BlobLen,
    RegistryUrl,
    RegistryCoordinate,
}

/// What a surface name resolves to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingTarget {
    /// A registered primitive, one-to-one with this name — except `decode`/
    /// `try_decode`, which are two surface names over the *same*
    /// [`PrimitiveId`] ([`decode_primitive_id`]) until the const-fold-through-
    /// wrappers work lands and their request construction becomes uniform.
    Primitive(PrimitiveId),
    /// A compiler-known dedicated VIR op (`fixture_tree`, `fixture_registry`,
    /// `untar`) — not a primitive at all.
    Intrinsic(Intrinsic),
    /// A vix-source function bound under a placement. This is the sanctioned way
    /// to add an alias or convenience wrapper (`refresh` over `observe`) and to
    /// "nicely add a pure vix function" to the prelude or a namespace. The
    /// function is effectful when its body invokes an effectful primitive; vix
    /// effect tracking flows through the call as it does for any wrapper.
    VixFunction { source: String },
    /// A receiver method that lowers to a dedicated VIR op. The compiler keeps
    /// the bespoke per-op lowering; this only records *which* op arm the
    /// `(receiver, name)` selects, so the method table is data here rather than a
    /// second registry in `compiler.rs`. Always paired with `Some(receiver)` on
    /// the [`Binding`].
    DedicatedOp(MethodOp),
}

/// One projected name: placement + leaf name + what it resolves to.
///
/// `receiver`/`arity` are `Some` for receiver-method bindings (dispatched by
/// receiver type + name) and `None` for free-function bindings (dispatched by
/// name + placement, arity carried by their [`RequestShape`] or signature).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub placement: Placement,
    pub name: String,
    pub target: BindingTarget,
    pub receiver: Option<ReceiverType>,
    pub arity: Option<usize>,
}

impl Binding {
    /// Bind a registered primitive to a surface name. One primitive, one name
    /// — except `decode`/`try_decode`, both hand-bound onto the same id.
    #[must_use]
    pub fn primitive(placement: Placement, name: impl Into<String>, id: PrimitiveId) -> Self {
        Self {
            placement,
            name: name.into(),
            target: BindingTarget::Primitive(id),
            receiver: None,
            arity: None,
        }
    }

    /// Bind a dedicated-op intrinsic to a surface name.
    #[must_use]
    pub fn intrinsic(placement: Placement, name: impl Into<String>, kind: Intrinsic) -> Self {
        Self {
            placement,
            name: name.into(),
            target: BindingTarget::Intrinsic(kind),
            receiver: None,
            arity: None,
        }
    }

    /// Bind a vix-source function to a surface name under a placement.
    #[must_use]
    pub fn vix_fn(
        placement: Placement,
        name: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            placement,
            name: name.into(),
            target: BindingTarget::VixFunction {
                source: source.into(),
            },
            receiver: None,
            arity: None,
        }
    }

    /// Bind a receiver method to a dedicated VIR op. Method bindings are placed
    /// in the prelude (globally available like the builtin methods they replace)
    /// and dispatched by `receiver` + `name`, never by placement.
    #[must_use]
    pub fn method(
        receiver: ReceiverType,
        name: impl Into<String>,
        arity: usize,
        op: MethodOp,
    ) -> Self {
        Self {
            placement: Placement::Prelude,
            name: name.into(),
            target: BindingTarget::DedicatedOp(op),
            receiver: Some(receiver),
            arity: Some(arity),
        }
    }
}

/// The set of surface bindings a runtime exposes. Lookups are placement-aware:
/// prelude names resolve unqualified; module names resolve by full path.
#[derive(Clone, Debug, Default)]
pub struct BindingRegistry {
    bindings: Vec<Binding>,
}

impl BindingRegistry {
    pub fn insert(&mut self, binding: Binding) {
        self.bindings.push(binding);
    }

    #[must_use]
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// Resolve an unqualified name against the prelude. Free-function bindings
    /// only — method bindings (`receiver.is_some()`) share the same registry but
    /// resolve through [`method`](Self::method), so a free function and a
    /// same-named method never collide.
    #[must_use]
    pub fn prelude(&self, name: &str) -> Option<&Binding> {
        self.bindings.iter().find(|b| {
            b.name == name && b.placement == Placement::Prelude && b.receiver.is_none()
        })
    }

    /// Resolve a fully-qualified `module::name` (free-function bindings only).
    #[must_use]
    pub fn qualified(&self, path: &ModulePath, name: &str) -> Option<&Binding> {
        self.bindings.iter().find(|b| {
            b.name == name
                && b.receiver.is_none()
                && matches!(&b.placement, Placement::Module(p) if p == path)
        })
    }

    /// Resolve a receiver method by its receiver type and name.
    #[must_use]
    pub fn method(&self, receiver: ReceiverType, name: &str) -> Option<&Binding> {
        self.bindings
            .iter()
            .find(|b| b.receiver == Some(receiver) && b.name == name)
    }
}

/// The built-in prelude bindings, encoded as data.
///
/// Every registered primitive that projects a surface name (`RawPrimitive::
/// surface_name` returns `Some`) is harvested straight from
/// [`runtime::builtin_primitives`] — `fetch` and `observe` today. `decode`/
/// `try_decode` share one primitive under two names and are not yet uniform
/// (see the module docs), so they stay hand-registered onto
/// [`decode_primitive_id`]; the `fixture_*`/`untar` dedicated ops are hand-
/// registered as [`Intrinsic`]s. The behavioural modes (`refresh` over
/// `observe`; `json_decode`/`toml_decode`/`try_json_decode`/`try_toml_decode`
/// over `decode`/`try_decode`) are vix functions over the single primitive
/// rather than extra primitives or intrinsics. The compiler consumes this in
/// place of hardcoded name matches: [`prelude_primitive`]/[`prelude_intrinsic`]
/// map a name to its target for lowering, and [`is_prelude_name`] gates binder
/// resolution. The vix-fn `source` strings mirror `crate::stdlib`'s
/// `PRELUDE_FUNCTIONS`, which is what actually injects them.
///
/// Tree text reads (`.text()`) are a *method* binding surface, orthogonal to
/// free-function placement, and are intentionally omitted here — this is why
/// `TreeReadPrimitive` (also in `runtime::builtin_primitives`, but with no
/// `surface_name`) contributes no binding.
#[must_use]
pub fn builtin_bindings() -> BindingRegistry {
    let mut reg = BindingRegistry::default();

    // Harvest one binding per registered primitive that projects a surface
    // name — no second table naming `fetch`/`observe` by hand.
    for primitive in runtime::builtin_primitives::<()>() {
        if let Some(name) = primitive.surface_name() {
            reg.insert(Binding::primitive(
                Placement::Prelude,
                name,
                primitive.descriptor().id.clone(),
            ));
        }
    }

    // decode/try_decode: one registered primitive, two surface names — not yet
    // uniform (compile-time constant folding, expected-type-derived target), so
    // hand-registered onto the shared id rather than harvested.
    for name in ["decode", "try_decode"] {
        reg.insert(Binding::primitive(
            Placement::Prelude,
            name,
            decode_primitive_id(),
        ));
    }

    // fixture_tree/fixture_registry/untar: dedicated VIR ops, not primitives.
    for (name, kind) in [
        ("fixture_tree", Intrinsic::FixtureTree),
        ("fixture_registry", Intrinsic::FixtureRegistry),
        ("untar", Intrinsic::Untar),
    ] {
        reg.insert(Binding::intrinsic(Placement::Prelude, name, kind));
    }

    // Modes-as-aliases: vix functions over the single primitive, not new
    // primitives and not new compiler intrinsics (mirroring `stdlib`).
    for (name, source) in [
        (
            "refresh",
            "fn refresh<Origin>(origin: Origin) -> Blob { observe(origin, Mode::Refresh) }",
        ),
        (
            "json_decode",
            "fn json_decode<T>(text: String) -> T { decode(text, Format::Json) }",
        ),
        (
            "toml_decode",
            "fn toml_decode<T>(text: String) -> T { decode(text, Format::Toml) }",
        ),
        (
            "try_json_decode",
            "fn try_json_decode<T>(text: String) -> Result<T, DecodeError> { try_decode(text, Format::Json) }",
        ),
        (
            "try_toml_decode",
            "fn try_toml_decode<T>(text: String) -> Result<T, DecodeError> { try_decode(text, Format::Toml) }",
        ),
    ] {
        reg.insert(Binding::vix_fn(Placement::Prelude, name, source));
    }

    // Receiver methods: `(receiver, name)` → dedicated VIR op. These were a
    // second hardcoded registry in `compiler.rs` (`PreludeMethodRegistry`); they
    // now live in the same registry as the free-function bindings, dispatched by
    // receiver type. The bespoke per-op lowering stays in `lower_method_call`.
    for &(receiver, name, arity, op) in METHOD_BINDINGS {
        reg.insert(Binding::method(receiver, name, arity, op));
    }

    reg
}

/// The builtin receiver methods, as data: `(receiver, name, arity, op)`. One row
/// per [`MethodOp`]. Lookup is by `(receiver, name)`; arity is validated by the
/// compiler after resolution.
const METHOD_BINDINGS: &[(ReceiverType, &str, usize, MethodOp)] = &[
    (ReceiverType::Array, "len", 0, MethodOp::ArrayLen),
    (ReceiverType::Array, "map", 1, MethodOp::ArrayMap),
    (ReceiverType::Array, "fold", 2, MethodOp::ArrayFold),
    (ReceiverType::Array, "split_last", 0, MethodOp::ArraySplitLast),
    (ReceiverType::Array, "all", 1, MethodOp::ArrayAll),
    (ReceiverType::Array, "any", 1, MethodOp::ArrayAny),
    (ReceiverType::Array, "contains", 1, MethodOp::ArrayContains),
    (ReceiverType::String, "trim", 0, MethodOp::StringTrim),
    (ReceiverType::String, "contains", 1, MethodOp::StringContains),
    (ReceiverType::String, "split_once", 1, MethodOp::StringSplitOnce),
    (ReceiverType::String, "parse_int", 0, MethodOp::StringParseInt),
    (ReceiverType::String, "is_numeric", 0, MethodOp::StringIsNumeric),
    (ReceiverType::String, "lines", 0, MethodOp::StringLines),
    (ReceiverType::Array, "sorted", 0, MethodOp::ArraySorted),
    (ReceiverType::Array, "stream", 0, MethodOp::ArrayStream),
    (ReceiverType::Int, "rem", 1, MethodOp::IntRem),
    (ReceiverType::Map, "get", 1, MethodOp::MapGet),
    (ReceiverType::Map, "has", 1, MethodOp::MapHas),
    (ReceiverType::Map, "len", 0, MethodOp::MapLen),
    (ReceiverType::Map, "keys", 0, MethodOp::MapKeys),
    (ReceiverType::Map, "values", 0, MethodOp::MapValues),
    (ReceiverType::Map, "with", 2, MethodOp::MapWith),
    (ReceiverType::Set, "has", 1, MethodOp::SetHas),
    (ReceiverType::Set, "len", 0, MethodOp::SetLen),
    (ReceiverType::Set, "values", 0, MethodOp::SetValues),
    (ReceiverType::Stream, "filter", 1, MethodOp::StreamFilter),
    (ReceiverType::Stream, "filter_map", 1, MethodOp::StreamFilterMap),
    (ReceiverType::Stream, "flat_map", 1, MethodOp::StreamFlatMap),
    (ReceiverType::Stream, "collect", 0, MethodOp::StreamCollect),
    (ReceiverType::ByteStream, "collect", 0, MethodOp::ByteStreamCollect),
    (ReceiverType::ByteStream, "trim", 0, MethodOp::ByteStreamTrim),
    (ReceiverType::Stream, "find_min", 1, MethodOp::StreamFindMin),
    (ReceiverType::Stream, "find_max", 1, MethodOp::StreamFindMax),
    (ReceiverType::Stream, "split_min", 0, MethodOp::StreamSplitMin),
    (ReceiverType::Path, "to_string", 0, MethodOp::PathToString),
    (ReceiverType::Int, "to_string", 0, MethodOp::IntToString),
    (ReceiverType::Tree, "glob", 1, MethodOp::TreeGlob),
    (ReceiverType::TreeEntry, "text", 0, MethodOp::TreeEntryText),
    (ReceiverType::Blob, "len", 0, MethodOp::BlobLen),
    (ReceiverType::Registry, "url", 1, MethodOp::RegistryUrl),
    (ReceiverType::Registry, "coordinate", 1, MethodOp::RegistryCoordinate),
];

/// The dedicated op and arity a receiver method resolves to, or `None` if the
/// receiver type carries no builtin method of that name. The compiler dispatches
/// method lowering off this instead of a second hardcoded registry — mirroring
/// [`prelude_primitive`] for the free-function rail.
#[must_use]
pub fn prelude_method(receiver_ty: &crate::vir::Type, name: &str) -> Option<MethodResolution> {
    let receiver = ReceiverType::from_vir_type(receiver_ty)?;
    let binding = BUILTIN_BINDINGS.method(receiver, name)?;
    let BindingTarget::DedicatedOp(method) = binding.target else {
        return None;
    };
    Some(MethodResolution {
        method,
        arity: binding.arity?,
    })
}

/// A resolved receiver method: which dedicated op to lower to, and its arity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MethodResolution {
    pub method: MethodOp,
    pub arity: usize,
}

/// The built-in bindings, built once. The single source of truth for which
/// surface names are prelude primitives / vix-fn aliases.
static BUILTIN_BINDINGS: LazyLock<BindingRegistry> = LazyLock::new(builtin_bindings);

/// The [`PrimitiveId`] a prelude name lowers to, or `None` if the name is not a
/// built-in primitive (an intrinsic, a vix-fn alias, a user name, or unknown).
/// The compiler calls this to dispatch primitive lowering instead of matching
/// callee strings.
#[must_use]
pub fn prelude_primitive(name: &str) -> Option<PrimitiveId> {
    match &BUILTIN_BINDINGS.prelude(name)?.target {
        BindingTarget::Primitive(id) => Some(id.clone()),
        BindingTarget::Intrinsic(_)
        | BindingTarget::VixFunction { .. }
        | BindingTarget::DedicatedOp(_) => None,
    }
}

/// The [`Intrinsic`] a prelude name lowers to, or `None` if the name is not one
/// of the dedicated-op intrinsics.
#[must_use]
pub fn prelude_intrinsic(name: &str) -> Option<Intrinsic> {
    match &BUILTIN_BINDINGS.prelude(name)?.target {
        BindingTarget::Intrinsic(kind) => Some(*kind),
        BindingTarget::Primitive(_)
        | BindingTarget::VixFunction { .. }
        | BindingTarget::DedicatedOp(_) => None,
    }
}

/// The [`RequestShape`] a registered primitive's call lowers through, or `None`
/// for primitives whose request construction is not yet data (`decode`, the
/// shared `decode`/`try_decode` id — stays in the compiler's typed builders).
/// Returning `Some` is the contract that a primitive is fully described by
/// data: the compiler builds its request generically from the shape. Built by
/// asking every [`runtime::builtin_primitives`] entry for its own
/// [`RawPrimitive::request_shape`](crate::runtime::RawPrimitive) — never a `match`
/// over a closed enum.
#[must_use]
pub fn request_shape(id: &PrimitiveId) -> Option<RequestShape> {
    REQUEST_SHAPES.get(id).cloned()
}

static REQUEST_SHAPES: LazyLock<BTreeMap<PrimitiveId, RequestShape>> = LazyLock::new(|| {
    runtime::builtin_primitives::<()>()
        .into_iter()
        .filter_map(|primitive| {
            let shape = primitive.request_shape()?;
            Some((primitive.descriptor().id.clone(), shape))
        })
        .collect()
});

/// The set of prelude-placed binding names, derived once from
/// [`builtin_bindings`] so the binding set stays the single source of truth.
static PRELUDE_NAMES: LazyLock<BTreeSet<String>> = LazyLock::new(|| {
    BUILTIN_BINDINGS
        .bindings()
        .iter()
        .filter(|binding| {
            binding.placement == Placement::Prelude && binding.receiver.is_none()
        })
        .map(|binding| binding.name.clone())
        .collect()
});

/// Is `name` a prelude-placed surface binding? The binder consults this to
/// resolve prelude names (`fetch`, `observe`, …) rather than leaving them in
/// its `unresolved` bucket — the "prelude layer" its module docs describe.
#[must_use]
pub fn is_prelude_name(name: &str) -> bool {
    PRELUDE_NAMES.contains(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{observe_primitive_id, pinned_fetch_primitive_id};
    use crate::vir::{ExternKind, Type};

    #[test]
    fn empty_module_path_is_unrepresentable() {
        assert!(ModulePath::new(Vec::<String>::new()).is_none());
        assert!(ModulePath::new(["ok", ""]).is_none());
        let path = ModulePath::new(["some", "ns"]).expect("non-empty path");
        assert_eq!(path.display(), "some::ns");
    }

    #[test]
    fn builtins_project_one_prelude_name_per_primitive() {
        let reg = builtin_bindings();

        // fetch/observe are harvested from the registered primitives, one name
        // each, and `prelude_primitive` maps each to its `PrimitiveId`.
        for (name, id) in [
            ("fetch", pinned_fetch_primitive_id()),
            ("observe", observe_primitive_id()),
        ] {
            let binding = reg.prelude(name).expect("prelude primitive");
            assert!(matches!(binding.target, BindingTarget::Primitive(_)));
            assert_eq!(prelude_primitive(name), Some(id));
        }

        // decode/try_decode are hand-registered onto the same shared id.
        for name in ["decode", "try_decode"] {
            let binding = reg.prelude(name).expect("prelude primitive");
            assert!(matches!(binding.target, BindingTarget::Primitive(_)));
            assert_eq!(prelude_primitive(name), Some(decode_primitive_id()));
        }

        // The dedicated-op intrinsics are not primitives.
        for (name, kind) in [
            ("fixture_tree", Intrinsic::FixtureTree),
            ("fixture_registry", Intrinsic::FixtureRegistry),
            ("untar", Intrinsic::Untar),
        ] {
            let binding = reg.prelude(name).expect("prelude intrinsic");
            assert!(matches!(binding.target, BindingTarget::Intrinsic(_)));
            assert_eq!(prelude_intrinsic(name), Some(kind));
            assert_eq!(prelude_primitive(name), None);
        }

        // The mode aliases are vix functions, not primitives.
        for alias in [
            "refresh",
            "json_decode",
            "toml_decode",
            "try_json_decode",
            "try_toml_decode",
        ] {
            let binding = reg.prelude(alias).expect("prelude alias");
            assert!(matches!(binding.target, BindingTarget::VixFunction { .. }));
            assert_eq!(prelude_primitive(alias), None);
            assert_eq!(prelude_intrinsic(alias), None);
        }
    }

    #[test]
    fn a_namespaced_vix_function_resolves_by_path() {
        let path = ModulePath::new(["some", "ns"]).expect("path");
        let mut reg = builtin_bindings();
        reg.insert(Binding::vix_fn(
            Placement::Module(path.clone()),
            "cool_function",
            "fn cool_function<Origin>(origin: Origin) -> Blob { observe(origin, Mode::Observe) }",
        ));

        // Reachable by its qualified path...
        assert!(reg.qualified(&path, "cool_function").is_some());
        // ...but not from the prelude.
        assert!(reg.prelude("cool_function").is_none());
    }

    #[test]
    fn only_the_uniform_primitives_have_a_request_shape() {
        // fetch/observe are fully data — the compiler builds their request from
        // the shape. decode/try_decode share an id whose shape is still `None`.
        assert!(request_shape(&pinned_fetch_primitive_id()).is_some());
        assert!(request_shape(&observe_primitive_id()).is_some());
        assert!(
            request_shape(&decode_primitive_id()).is_none(),
            "decode should have no shape yet"
        );
    }

    #[test]
    fn fetch_shape_is_one_value_arg() {
        let shape = request_shape(&pinned_fetch_primitive_id()).expect("fetch shape");
        assert_eq!(shape.args.len(), 1);
        assert!(matches!(shape.args[0], ArgRole::Value { .. }));
        assert_eq!(shape.result, Type::Extern(ExternKind::Blob));
        assert_eq!(shape.primitive, pinned_fetch_primitive_id());
    }

    #[test]
    fn observe_shape_is_a_value_then_a_mode_selector() {
        let shape = request_shape(&observe_primitive_id()).expect("observe shape");
        assert_eq!(shape.args.len(), 2);
        assert!(matches!(shape.args[0], ArgRole::Value { .. }));
        let ArgRole::Selector(selector) = &shape.args[1] else {
            panic!("observe's second argument is the Mode selector");
        };
        assert_eq!(selector.enum_name, "Mode");
        // The selector carries its own accepted variants and folded flags.
        assert_eq!(
            selector
                .variants
                .iter()
                .find(|v| v.variant == "Refresh")
                .map(|v| v.flag),
            Some(true),
        );
    }

    #[test]
    fn selector_builds_its_own_diagnostic_wording() {
        let shape = request_shape(&observe_primitive_id()).expect("observe shape");
        let ArgRole::Selector(selector) = &shape.args[1] else {
            panic!("expected the Mode selector");
        };
        assert_eq!(
            selector.expected(),
            "an observe mode `Mode::Observe` or `Mode::Refresh`",
        );
        assert_eq!(
            selector.unknown("Spin"),
            "an unknown observe mode `Mode::Spin`"
        );
    }
}
