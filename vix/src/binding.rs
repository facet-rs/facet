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
//! **Primitive dispatch is wired.** `compiler::lower_value` recognizes built-in
//! primitives through [`prelude_primitive`] — the callee name maps to a
//! [`PrimitiveKind`] here, in data, instead of scattered `== "decode"` /
//! `effect_intrinsic` string matches. The genuinely per-primitive request
//! construction (the `Format`/`Mode` selector reads, expected-type-derived
//! targets, constant folding) stays in the compiler's typed builders, dispatched
//! from that one kind; only the *name → primitive* map lives here.
//!
//! Still on the compiler side: the binder consults [`is_prelude_name`] (a name
//! set derived from these bindings) but does not yet route full prelude/module
//! *resolution* through [`BindingRegistry::prelude`]/[`BindingRegistry::qualified`],
//! and the vix-fn `source` strings here are documentation — the actual injection
//! is `crate::stdlib`'s `PRELUDE_FUNCTIONS`. Language constructs (`Some`, `None`,
//! `by_key`, `range`, the `expect*`/trace checks) and the `.text()` method
//! surface are deliberately outside this registry.

use std::collections::BTreeSet;
use std::sync::LazyLock;

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

/// Which built-in primitive a prelude name lowers to.
///
/// The compiler dispatches the (genuinely per-primitive) request construction on
/// this: selector reads (`Format`/`Mode`), expected-type-derived targets, and
/// constant folding do not reduce to a single data shape. What the registry owns
/// is the mapping from surface name to primitive — the set of primitive names
/// lives here as data, not as scattered string matches in `lower_value`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveKind {
    /// `fetch(pin)` — pinned-blob fetch.
    Fetch,
    /// `observe(origin, Mode)` — observe/refresh over one primitive.
    Observe,
    /// `decode(document, Format)` — infallible typed decode to `T`.
    Decode,
    /// `try_decode(document, Format)` — fallible decode to `Result<T, DecodeError>`.
    TryDecode,
    /// `fixture_tree(name)` — a named fixture tree (a dedicated machine op).
    FixtureTree,
    /// `fixture_registry()` — the fixture registry (a dedicated machine op).
    FixtureRegistry,
    /// `untar(blob)` — expand a blob to a tree (a dedicated machine op).
    Untar,
}

/// What a surface name resolves to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingTarget {
    /// A built-in primitive, one-to-one with this name.
    Primitive(PrimitiveKind),
    /// A vix-source function bound under a placement. This is the sanctioned way
    /// to add an alias or convenience wrapper (`refresh` over `observe`) and to
    /// "nicely add a pure vix function" to the prelude or a namespace. The
    /// function is effectful when its body invokes an effectful primitive; vix
    /// effect tracking flows through the call as it does for any wrapper.
    VixFunction { source: String },
}

/// One projected name: placement + leaf name + what it resolves to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub placement: Placement,
    pub name: String,
    pub target: BindingTarget,
}

impl Binding {
    /// Bind a built-in primitive to a surface name. One primitive, one name.
    #[must_use]
    pub fn primitive(
        placement: Placement,
        name: impl Into<String>,
        kind: PrimitiveKind,
    ) -> Self {
        Self {
            placement,
            name: name.into(),
            target: BindingTarget::Primitive(kind),
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

    /// Resolve an unqualified name against the prelude.
    #[must_use]
    pub fn prelude(&self, name: &str) -> Option<&Binding> {
        self.bindings
            .iter()
            .find(|b| b.name == name && b.placement == Placement::Prelude)
    }

    /// Resolve a fully-qualified `module::name`.
    #[must_use]
    pub fn qualified(&self, path: &ModulePath, name: &str) -> Option<&Binding> {
        self.bindings
            .iter()
            .find(|b| b.name == name && matches!(&b.placement, Placement::Module(p) if p == path))
    }
}

/// The built-in prelude bindings, encoded as data.
///
/// Each built-in primitive projects **one** prelude name; the behavioural modes
/// (`refresh` over `observe`; `json_decode`/`toml_decode`/`try_json_decode`/
/// `try_toml_decode` over `decode`/`try_decode`) are vix functions over the
/// single primitive rather than extra primitives or intrinsics. The compiler
/// consumes this in place of hardcoded name matches: [`prelude_primitive`] maps
/// a name to its [`PrimitiveKind`] for lowering, and [`is_prelude_name`] gates
/// binder resolution. The vix-fn `source` strings mirror `crate::stdlib`'s
/// `PRELUDE_FUNCTIONS`, which is what actually injects them.
///
/// Tree text reads (`.text()`) are a *method* binding surface, orthogonal to
/// free-function placement, and are intentionally omitted here.
#[must_use]
pub fn builtin_bindings() -> BindingRegistry {
    let mut reg = BindingRegistry::default();

    // One primitive : one binding : one name.
    for (name, kind) in [
        ("fetch", PrimitiveKind::Fetch),
        ("observe", PrimitiveKind::Observe),
        ("decode", PrimitiveKind::Decode),
        ("try_decode", PrimitiveKind::TryDecode),
        ("fixture_tree", PrimitiveKind::FixtureTree),
        ("fixture_registry", PrimitiveKind::FixtureRegistry),
        ("untar", PrimitiveKind::Untar),
    ] {
        reg.insert(Binding::primitive(Placement::Prelude, name, kind));
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

    reg
}

/// The built-in bindings, built once. The single source of truth for which
/// surface names are prelude primitives / vix-fn aliases.
static BUILTIN_BINDINGS: LazyLock<BindingRegistry> = LazyLock::new(builtin_bindings);

/// The [`PrimitiveKind`] a prelude name lowers to, or `None` if the name is not
/// a built-in primitive (a vix-fn alias, a user name, or unknown). The compiler
/// calls this to dispatch primitive lowering instead of matching callee strings.
#[must_use]
pub fn prelude_primitive(name: &str) -> Option<PrimitiveKind> {
    match BUILTIN_BINDINGS.prelude(name)?.target {
        BindingTarget::Primitive(kind) => Some(kind),
        BindingTarget::VixFunction { .. } => None,
    }
}

/// The set of prelude-placed binding names, derived once from
/// [`builtin_bindings`] so the binding set stays the single source of truth.
static PRELUDE_NAMES: LazyLock<BTreeSet<String>> = LazyLock::new(|| {
    BUILTIN_BINDINGS
        .bindings()
        .iter()
        .filter(|binding| binding.placement == Placement::Prelude)
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

        // The built-in primitives are in the prelude, one name each, and
        // `prelude_primitive` maps each to its lowering kind.
        for (name, kind) in [
            ("fetch", PrimitiveKind::Fetch),
            ("observe", PrimitiveKind::Observe),
            ("decode", PrimitiveKind::Decode),
            ("try_decode", PrimitiveKind::TryDecode),
            ("fixture_tree", PrimitiveKind::FixtureTree),
            ("fixture_registry", PrimitiveKind::FixtureRegistry),
            ("untar", PrimitiveKind::Untar),
        ] {
            let binding = reg.prelude(name).expect("prelude primitive");
            assert!(matches!(binding.target, BindingTarget::Primitive(_)));
            assert_eq!(prelude_primitive(name), Some(kind));
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
}
