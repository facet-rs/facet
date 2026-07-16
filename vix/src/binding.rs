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
//! This is the representable model plus the intended projection of the
//! built-ins as data. It is **not yet wired**: today the binder defers prelude
//! names (see `binder.rs` module docs, which name `fetch` as the example) and
//! `compiler::lower_value` dispatches intrinsics by hardcoded callee strings
//! (`effect_intrinsic` / `decode_format`). Routing the binder's prelude/module
//! resolution and `lower_value` through a [`BindingRegistry`] — replacing those
//! string matches — is the next step, and is what closes the three gaps named
//! in `docs/content/registered-primitives.md` ("Current registration
//! boundary"). Until then this module must not be presented as the live binding
//! path.

use crate::runtime::PrimitiveId;

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

/// How a bound primitive builds its request from surface arguments.
///
/// This is the piece the compiler hardcodes today (the `Op::Bool(refresh)` and
/// `format => 0 | 1` discriminators). Modelling it as data is what lets an
/// arbitrarily registered primitive be compiled without compiler-side surgery
/// for its request. It is deliberately minimal here: the mode-carrying cases
/// are expressed as [`BindingTarget::VixFunction`] aliases, not as extra
/// shapes, keeping one primitive to one binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestShape {
    /// The surface arguments become the primitive's request record fields, in
    /// declaration order.
    Direct,
}

/// What a surface name resolves to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingTarget {
    /// A registered Rust primitive, one-to-one with this name.
    Primitive {
        id: PrimitiveId,
        request: RequestShape,
    },
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
    /// Bind a registered primitive to a surface name. One primitive, one name.
    #[must_use]
    pub fn primitive(
        placement: Placement,
        name: impl Into<String>,
        id: PrimitiveId,
        request: RequestShape,
    ) -> Self {
        Self {
            placement,
            name: name.into(),
            target: BindingTarget::Primitive { id, request },
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

/// The intended projection of the built-ins, encoded as data.
///
/// Each built-in primitive projects **one** prelude name; the behavioural modes
/// (`refresh`, `json_decode`, `toml_decode`) are vix functions over the single
/// primitive rather than extra primitives or intrinsics. This is the target
/// shape the binder and compiler are meant to consume in place of the hardcoded
/// intrinsic string matches — see this module's status note. It intentionally
/// describes the *target* signatures (e.g. `observe` taking a `refresh` arg,
/// `decode` taking a `format` arg), which is the projection the design calls
/// for, not a mirror of today's not-yet-refactored compiler.
///
/// Tree text reads (`.text()`) are a *method* binding surface, orthogonal to
/// free-function placement, and are intentionally omitted here.
#[must_use]
pub fn builtin_bindings() -> BindingRegistry {
    use crate::runtime::{observe_primitive_id, pinned_fetch_primitive_id};
    use crate::vir::decode_primitive_id;

    let mut reg = BindingRegistry::default();

    // One primitive : one binding : one name.
    reg.insert(Binding::primitive(
        Placement::Prelude,
        "fetch",
        pinned_fetch_primitive_id(),
        RequestShape::Direct,
    ));
    reg.insert(Binding::primitive(
        Placement::Prelude,
        "observe",
        observe_primitive_id(),
        RequestShape::Direct,
    ));
    reg.insert(Binding::primitive(
        Placement::Prelude,
        "decode",
        decode_primitive_id(),
        RequestShape::Direct,
    ));

    // Modes-as-aliases: vix functions over the single primitive, not new
    // primitives and not new compiler intrinsics.
    reg.insert(Binding::vix_fn(
        Placement::Prelude,
        "refresh",
        "fn refresh(x) = observe(x, refresh: true)",
    ));
    reg.insert(Binding::vix_fn(
        Placement::Prelude,
        "json_decode",
        "fn json_decode<T>(text) = decode<T>(text, format: Json)",
    ));
    reg.insert(Binding::vix_fn(
        Placement::Prelude,
        "toml_decode",
        "fn toml_decode<T>(text) = decode<T>(text, format: Toml)",
    ));

    reg
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

        // The three free-function primitives are in the prelude, one name each.
        for name in ["fetch", "observe", "decode"] {
            let binding = reg.prelude(name).expect("prelude primitive");
            assert!(matches!(binding.target, BindingTarget::Primitive { .. }));
        }

        // The mode aliases are vix functions, not primitives.
        for alias in ["refresh", "json_decode", "toml_decode"] {
            let binding = reg.prelude(alias).expect("prelude alias");
            assert!(matches!(binding.target, BindingTarget::VixFunction { .. }));
        }
    }

    #[test]
    fn a_namespaced_vix_function_resolves_by_path() {
        let path = ModulePath::new(["some", "ns"]).expect("path");
        let mut reg = builtin_bindings();
        reg.insert(Binding::vix_fn(
            Placement::Module(path.clone()),
            "cool_function",
            "fn cool_function(x) = observe(x, refresh: false)",
        ));

        // Reachable by its qualified path...
        assert!(reg.qualified(&path, "cool_function").is_some());
        // ...but not from the prelude.
        assert!(reg.prelude("cool_function").is_none());
    }
}
