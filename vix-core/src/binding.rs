//! Surface bindings: the projection from registered primitives (and vix-source
//! functions) onto vix-language names and their placement in name resolution.
//!
//! # Design contract
//!
//! One Rust primitive projects exactly **one** binding with **one** name.
//! Behavioural modes (`json`/`toml` decode) are *request fields the primitive
//! reads* — never extra primitives, and never extra compiler intrinsics.
//! Ergonomic aliases are ordinary vix functions bound over that single
//! primitive (`json_decode(x) = decode(x, Format::Json)`).
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
//! *harvests* the [`BindingRegistry`] from [`runtime::builtin_primitive_surfaces`]
//! rather than maintaining a second table that names every primitive by hand.
//! `compiler::lower_value` recognizes built-in primitives through
//! [`surface_primitive`] — the unqualified compatibility spelling and the
//! canonical `std::` spelling map to a [`PrimitiveId`] here, in data, instead
//! of scattered string matches.
//!
//! **Request construction is data for the uniform primitives.** `fetch` lowers
//! through a [`RequestShape`] ([`request_shape`], keyed by [`PrimitiveId`]): its
//! arity, per-argument roles (a lowered [`ArgRole::Value`] with its required
//! type), request record, result type, and target primitive are all data,
//! consumed by one generic builder rather than a bespoke Rust arm each.
//!
//! Not yet on a shape: `decode`/`try_decode` (compile-time constant folding and
//! expected-type-derived targets don't reduce to a record shape) — both are
//! hand-registered onto the single [`decode_primitive_id`]; `request_shape`
//! returns `None` for it, and the compiler keeps a typed builder. The
//! `fixture_*`/`untar` dedicated VIR ops are not primitives at all — they are
//! [`Intrinsic`]s, matched here by name and dispatched to the compiler's
//! existing typed builder.
//!
//! Name resolution consults [`is_prelude_name`] for compatibility names and
//! [`BindingRegistry::qualified`] for canonical module paths. The vix-fn
//! `source` strings here document the matching sources injected by
//! `vixen-primitives`. Language constructs (`Some`, `None`, `by_key`, `range`,
//! the `expect*`/trace checks) and the `.text()` method surface are deliberately
//! outside this registry.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::LazyLock;

use crate::runtime::{self, PrimitiveId};
use crate::vir::decode_primitive_id;

// Re-exported so existing call sites (`compiler.rs`) can keep spelling these
// `crate::binding::RequestShape` etc. — the types now live next to
// `RawPrimitive` in `runtime`, since it is the primitive that declares them.
pub use crate::runtime::{ArgRole, RequestShape};

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
    /// `fetch`). This is the prelude layer the binder currently defers rather
    /// than resolves.
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
    Blob,
    Registry,
    /// A declared host type ([`crate::vir::ExternKind::Host`]), dispatched by its
    /// name. `Tree`/`TreeEntry` are the two the embedder declares today; they are
    /// no longer core `ReceiverType` variants, so a host type's methods are
    /// injected declarations ([`MethodDecl`]) rather than compiler-hardcoded.
    Host(&'static str),
}

/// The nominal name of the `Tree` host type — its identity string, hashed as
/// `builtin_schema("Tree")` / `b"extern" + "Tree"`. The machine engine (the tree
/// ops) still names it here; the *type declaration* is injected by the embedder.
pub const TREE: &str = "Tree";
/// The nominal name of the `TreeEntry` host type. See [`TREE`].
pub const TREE_ENTRY: &str = "TreeEntry";

/// A host-type declaration the embedder injects into the compiler
/// ([`crate::compiler::CompilerConfig::host_types`]), so a domain type like
/// `Tree` is declared by `vixen-primitives` rather than hardcoded as a variant
/// of the language core's `ExternKind`. `name` is the type's nominal identity.
///
/// `name` must be a builtin schema name the core reserves (see
/// `schema::SchemaBatch::vix_builtins`) — the schema *registration* is the one
/// piece of a host type that still lives in `vix-core`, since it anchors
/// byte-stable identity; the compiler rejects an unregistered name with a
/// diagnostic rather than panicking. A `name` that collides with a core type
/// spelling (`Blob`, `Registry`, …) is also rejected: the core always wins its
/// own spelling, so such a declaration could never be reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostTypeDecl {
    pub name: &'static str,
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
            Type::Extern(ExternKind::Blob) => Some(Self::Blob),
            Type::Extern(ExternKind::Registry) => Some(Self::Registry),
            // A declared host type dispatches by its name.
            Type::Extern(ExternKind::Host(name)) => Some(Self::Host(name)),
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
    StreamSplitMin,
    PathToString,
    IntToString,
    ByteStreamCollect,
    ByteStreamTrim,
    TreeGlob,
    BlobLen,
    // `RegistryUrl` is retired from this enum: `Registry.url` is a
    // primitive-backed method ([`MethodLowering::Primitive`]) whose whole
    // contract lives in `vixen-primitives` — the fully generic rail needs no
    // dedicated-op name in core.
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
    /// to add an alias or convenience wrapper (`json_decode` over `decode`) and
    /// to "nicely add a pure vix function" to the prelude or a namespace. The
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

/// The built-in surface bindings, encoded as data.
///
/// Every registered primitive that projects a surface name (`RawPrimitive::
/// surface_name` returns `Some`) is harvested straight from
/// [`runtime::builtin_primitive_surfaces`] — `fetch` today. `decode`/
/// `try_decode` share one primitive under two names and are not yet uniform
/// (see the module docs), so they stay hand-registered onto
/// [`decode_primitive_id`]; the `fixture_*`/`untar` dedicated ops are hand-
/// registered as [`Intrinsic`]s. The decode aliases
/// (`json_decode`/`toml_decode`/`try_json_decode`/`try_toml_decode` over
/// `decode`/`try_decode`) are vix functions over the single primitive rather
/// than extra primitives or intrinsics. The compiler consumes this in
/// place of hardcoded name matches: [`surface_primitive`]/[`surface_intrinsic`]
/// map a name to its target for lowering, and [`is_prelude_name`] gates legacy
/// unqualified resolution. The vix-fn `source` strings mirror the
/// `vixen-primitives` stdlib sources, which are what actually inject them.
///
/// Tree text reads (`.text()`) are a *method* binding surface, orthogonal to
/// free-function placement, and are intentionally omitted here — this is why
/// `TreeReadPrimitive` (a vixen builtin, but with no
/// `surface_name`) contributes no binding.
#[must_use]
pub fn builtin_bindings() -> BindingRegistry {
    let mut reg = BindingRegistry::default();
    let std = ModulePath::new(["std"]).expect("std is a valid module path");

    // Harvest one binding per builtin primitive that projects a surface name —
    // no second table naming `fetch`/`observe` by hand. The surface contracts
    // are language data (`runtime::builtin_primitive_surfaces`); the matching
    // implementations live in `vixen-primitives`.
    for primitive in runtime::builtin_primitive_surfaces() {
        for placement in [Placement::Prelude, Placement::Module(std.clone())] {
            reg.insert(Binding::primitive(
                placement,
                primitive.surface_name,
                primitive.id.clone(),
            ));
        }
    }

    // decode/try_decode: one registered primitive, two surface names — not yet
    // uniform (compile-time constant folding, expected-type-derived target), so
    // hand-registered onto the shared id rather than harvested.
    for name in ["decode", "try_decode"] {
        for placement in [Placement::Prelude, Placement::Module(std.clone())] {
            reg.insert(Binding::primitive(placement, name, decode_primitive_id()));
        }
    }

    // fixture_tree/fixture_registry/untar: dedicated VIR ops, not primitives.
    for (name, kind) in [
        ("fixture_tree", Intrinsic::FixtureTree),
        ("fixture_registry", Intrinsic::FixtureRegistry),
        ("untar", Intrinsic::Untar),
    ] {
        reg.insert(Binding::intrinsic(Placement::Prelude, name, kind));
        reg.insert(Binding::intrinsic(
            Placement::Module(std.clone()),
            name,
            kind,
        ));
    }

    // Modes-as-aliases: vix functions over the single primitive, not new
    // primitives and not new compiler intrinsics (mirroring `stdlib`).
    for (name, source) in [
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
        reg.insert(Binding::vix_fn(
            Placement::Module(std.clone()),
            name,
            source,
        ));
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
    (ReceiverType::Stream, "split_min", 0, MethodOp::StreamSplitMin),
    (ReceiverType::Path, "to_string", 0, MethodOp::PathToString),
    (ReceiverType::Int, "to_string", 0, MethodOp::IntToString),
    // The domain methods (`Tree.glob`, `TreeEntry.text`, `Blob.len`,
    // `Registry.url`) are no longer builtin: they are declared by
    // `vixen-primitives` and injected through [`CompilerConfig::methods`]
    // (issue 2520). `vix-core` ships only the axiom methods above.
];

/// A receiver-method declaration the embedder injects into the compiler
/// ([`CompilerConfig::methods`]), so a host type's methods are declared by
/// `vixen-primitives` rather than hardcoded in `vix-core`. Its `lowering` names
/// what the call becomes — a dedicated VIR op whose bespoke lowering
/// `compiler::lower_method_call` still owns, or a registered primitive the
/// compiler reaches through the fully generic request-record rail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MethodDecl {
    pub receiver: ReceiverType,
    pub name: &'static str,
    pub arity: usize,
    pub lowering: MethodLowering,
}

/// What a resolved receiver method lowers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MethodLowering {
    /// A dedicated VIR op with bespoke lowering in `compiler::lower_method_call`
    /// (`TreeGlob`'s codata stream, `BlobLen`). The declaration is injectable;
    /// the machine engine stays in core.
    DedicatedOp(MethodOp),
    /// A registered primitive: the compiler builds the declared request record
    /// (`Op::Record`) and invokes the primitive (`Op::InvokePrimitive`) — no
    /// per-method compiler code at all. This is the fully open rail: a method
    /// declared this way needs nothing from `vix-core` beyond the generic
    /// lowering, so its contract (request/result types, primitive id) lives in
    /// the embedder alongside its implementation.
    Primitive(PrimitiveMethodDecl),
}

/// The contract of a primitive-backed receiver method: the request record the
/// call assembles (its first field is the receiver, the rest bind the call's
/// positional arguments in order), the result type of the invocation, and the
/// primitive that serves it. Carried as `fn() -> _` thunks so declarations stay
/// `const` data while the types are built on demand.
///
/// The invocation node is `PURE` — an ordinary in-frame primitive demand; any
/// effect is witnessed *inside* the primitive via its `EffectCtx` reads, as
/// `registry-url` reads the manifest.
// Equality on the thunks is pointer identity — good enough for its one use
// (`CompilerConfig` equality, where "same declaration table" is the question);
// two configs sharing one `DOMAIN_METHODS` compare equal, which is the case
// that matters.
#[allow(unpredictable_function_pointer_comparisons)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrimitiveMethodDecl {
    pub request: fn() -> crate::vir::Type,
    pub result: fn() -> crate::vir::Type,
    pub id: fn() -> PrimitiveId,
}

/// Resolve a receiver method against an embedder-injected declaration set, or
/// `None` if none matches. The compiler consults this alongside
/// [`prelude_method`] (the axiom methods), the way `primitive_shape` consults
/// injected primitive surfaces alongside the builtin ones.
#[must_use]
pub fn injected_method(
    methods: &[MethodDecl],
    receiver_ty: &crate::vir::Type,
    name: &str,
) -> Option<MethodResolution> {
    let receiver = ReceiverType::from_vir_type(receiver_ty)?;
    let decl = methods
        .iter()
        .find(|decl| decl.receiver == receiver && decl.name == name)?;
    Some(MethodResolution {
        lowering: decl.lowering,
        arity: decl.arity,
    })
}

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
        lowering: MethodLowering::DedicatedOp(method),
        arity: binding.arity?,
    })
}

/// A resolved receiver method: what it lowers to, and its arity.
#[derive(Clone, Copy, Debug)]
pub struct MethodResolution {
    pub lowering: MethodLowering,
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

/// Whether `module::name` is a registered built-in surface binding.
#[must_use]
pub fn is_qualified_binding(module: &str, name: &str) -> bool {
    let Some(path) = ModulePath::new([module]) else {
        return false;
    };
    BUILTIN_BINDINGS.qualified(&path, name).is_some()
}

/// The module an embedder-injected primitive surface is reachable under,
/// mirroring the `Placement::Module(std)` half of [`builtin_bindings`]'s static
/// harvest (each surface is placed at both the prelude and `std`). Injected and
/// bundled surfaces share one placement rule, kept in this single spot.
fn injected_surface_module() -> ModulePath {
    ModulePath::new(["std"]).expect("std is a valid module path")
}

/// Whether `module::name` is a registered built-in surface binding **or** is
/// projected by one of the embedder-injected primitive `surfaces`.
///
/// This is the qualified-name analogue of the call-lowering seam in
/// `ModuleContext::primitive_shape`, which already consults injected surfaces
/// before the static fallback. Module resolution (`resolve_imports` and the
/// qualified-path rewriter) calls this so that a primitive whose surface is
/// *injected* from the runtime — rather than bundled into
/// [`runtime::builtin_primitive_surfaces`] — still resolves as `std::name` and
/// through `use std::{name}`, exactly as a bundled one does.
#[must_use]
pub fn is_qualified_binding_with(
    surfaces: &[runtime::PrimitiveSurface],
    module: &str,
    name: &str,
) -> bool {
    if is_qualified_binding(module, name) {
        return true;
    }
    ModulePath::new([module]).is_some_and(|path| {
        path == injected_surface_module()
            && surfaces.iter().any(|surface| surface.surface_name == name)
    })
}

/// Resolve a possibly-qualified surface `name` against the embedder-injected
/// primitive `surfaces`. Mirrors [`surface_primitive`]'s dual spelling: an
/// injected surface answers to both its bare prelude name (`grab`) and its
/// canonical qualified name (`std::grab`). Returns `None` for a qualified
/// spelling under any module other than `std`.
#[must_use]
pub fn injected_surface<'a>(
    surfaces: &'a [runtime::PrimitiveSurface],
    name: &str,
) -> Option<&'a runtime::PrimitiveSurface> {
    let leaf = match name.rsplit_once("::") {
        Some((module, leaf)) => {
            let path = ModulePath::new(module.split("::"))?;
            if path != injected_surface_module() {
                return None;
            }
            leaf
        }
        None => name,
    };
    surfaces.iter().find(|surface| surface.surface_name == leaf)
}

/// Resolve a primitive by either its compatibility prelude name or its
/// canonical qualified name (for example `std::fetch`).
#[must_use]
pub fn surface_primitive(name: &str) -> Option<PrimitiveId> {
    if let Some(primitive) = prelude_primitive(name) {
        return Some(primitive);
    }
    let (module, leaf) = name.rsplit_once("::")?;
    let path = ModulePath::new(module.split("::"))?;
    match &BUILTIN_BINDINGS.qualified(&path, leaf)?.target {
        BindingTarget::Primitive(id) => Some(id.clone()),
        BindingTarget::Intrinsic(_)
        | BindingTarget::VixFunction { .. }
        | BindingTarget::DedicatedOp(_) => None,
    }
}

/// Resolve an intrinsic by either its compatibility prelude name or its
/// canonical qualified name (for example `std::untar`).
#[must_use]
pub fn surface_intrinsic(name: &str) -> Option<Intrinsic> {
    if let Some(intrinsic) = prelude_intrinsic(name) {
        return Some(intrinsic);
    }
    let (module, leaf) = name.rsplit_once("::")?;
    let path = ModulePath::new(module.split("::"))?;
    match &BUILTIN_BINDINGS.qualified(&path, leaf)?.target {
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
/// asking every [`runtime::builtin_primitive_surfaces`] entry for its own
/// [`RawPrimitive::request_shape`](crate::runtime::RawPrimitive) — never a `match`
/// over a closed enum.
#[must_use]
pub fn request_shape(id: &PrimitiveId) -> Option<RequestShape> {
    REQUEST_SHAPES.get(id).cloned()
}

static REQUEST_SHAPES: LazyLock<BTreeMap<PrimitiveId, RequestShape>> = LazyLock::new(|| {
    runtime::builtin_primitive_surfaces()
        .into_iter()
        .map(|primitive| (primitive.id.clone(), primitive.shape))
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
/// resolve prelude names (`fetch`, …) rather than leaving them in its
/// `unresolved` bucket — the "prelude layer" its module docs describe.
#[must_use]
pub fn is_prelude_name(name: &str) -> bool {
    PRELUDE_NAMES.contains(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::pinned_fetch_primitive_id;
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
        let std = ModulePath::new(["std"]).expect("std module path");

        // fetch is harvested from the registered primitives, one name, and
        // `prelude_primitive` maps it to its `PrimitiveId`.
        for (name, id) in [("fetch", pinned_fetch_primitive_id())] {
            let binding = reg.prelude(name).expect("prelude primitive");
            assert!(matches!(binding.target, BindingTarget::Primitive(_)));
            assert_eq!(prelude_primitive(name), Some(id.clone()));
            assert!(reg.qualified(&std, name).is_some());
            assert_eq!(surface_primitive(&format!("std::{name}")), Some(id));
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

        // The decode aliases are vix functions, not primitives.
        for alias in [
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
            "fn cool_function(text: String) -> String { text }",
        ));

        // Reachable by its qualified path...
        assert!(reg.qualified(&path, "cool_function").is_some());
        // ...but not from the prelude.
        assert!(reg.prelude("cool_function").is_none());
    }

    #[test]
    fn only_the_uniform_primitives_have_a_request_shape() {
        // fetch is fully data — the compiler builds its request from the shape.
        // decode/try_decode share an id whose shape is still `None`.
        assert!(request_shape(&pinned_fetch_primitive_id()).is_some());
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

    /// A surface standing in for one the embedder injects rather than bundling
    /// into `builtin_primitive_surfaces` — its name (`grab`) is deliberately not
    /// in the static table. It reuses fetch's real shape so a lowered call is
    /// well-typed.
    fn injected_grab() -> runtime::PrimitiveSurface {
        let fetch = runtime::builtin_primitive_surfaces()
            .into_iter()
            .next()
            .expect("fetch surface");
        runtime::PrimitiveSurface {
            surface_name: "grab",
            ..fetch
        }
    }

    #[test]
    fn injected_surface_is_a_qualified_binding_only_under_std() {
        let surfaces = [injected_grab()];

        // `grab` is not in the static table, so plain `is_qualified_binding`
        // never sees it…
        assert!(!is_qualified_binding("std", "grab"));
        // …but the injected-aware seam resolves it under `std`, and only there.
        assert!(is_qualified_binding_with(&surfaces, "std", "grab"));
        assert!(!is_qualified_binding_with(&surfaces, "other", "grab"));
        assert!(!is_qualified_binding_with(&surfaces, "std", "not_injected"));

        // The static bundled names still resolve through the same seam.
        assert!(is_qualified_binding_with(&surfaces, "std", "fetch"));
    }

    #[test]
    fn injected_surface_answers_bare_and_std_spellings() {
        let surfaces = [injected_grab()];

        // Both the prelude spelling and the canonical `std::` spelling resolve,
        // mirroring `surface_primitive`'s dual spelling for bundled primitives.
        assert!(injected_surface(&surfaces, "grab").is_some());
        assert!(injected_surface(&surfaces, "std::grab").is_some());

        // A qualified spelling under any other module does not.
        assert!(injected_surface(&surfaces, "other::grab").is_none());
        // An unknown leaf does not.
        assert!(injected_surface(&surfaces, "std::nope").is_none());
    }
}
