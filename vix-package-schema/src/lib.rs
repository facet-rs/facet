//! The vix package manifest document types.
//!
//! The manifest is frontmatter in a package's `main.vix`
//! (`r[vixen.package.frontmatter]`) — a `manifest` item carrying a
//! quote-family block literal whose content is the styx document these types
//! describe. There is exactly one carrier; a separate `package.styx` does not
//! exist.
//!
//! The manifest declares a package's **surface, not its provenance**. It does
//! not say where anything comes from: fetching is something vix code does, as
//! an ordinary demand, whenever it likes. An earlier draft carried a
//! declarative `input` section of pinned sources; it was struck
//! (`r[vixen.package.*]`, the Retracted section) and these types followed.
//!
//! One Rust source of truth for the manifest genre sketched in
//! `vix-core/corpus-next/package/`: these types ARE the schema, and their
//! facet `Shape` is the seam that exposes it to every consumer — Rust code
//! deserializes with `facet_styx::from_str::<PackageManifest>`, styx tooling
//! generates a validating schema from the same shape, and the vix rail reads
//! the shape through `vix::vir::Type::from_facet` when the manifest crosses
//! into the language. No consumer restates the shape.
//!
//! Deliberately absent: any parser or handler. The manifest is dead data —
//! reading and *acting on* one is vix-side code (`package.vix`), not a Rust
//! library. The only Rust code here is the type definitions and the tests
//! that pin the corpus manifests to them.
//!
//! Spelling contract with styx (learned from the real parser, recorded in
//! `corpus-next/package/README.md`): an entry is at most two atoms, so every
//! section is a map keyed by public name; repeated keys are errors; a tag's
//! payload is adjacent (`@package{…}`); sequences use parentheses. Enum
//! values wear kebab-case tags — `rename_all = "kebab-case"` here, and the
//! vix decoder applies the same kebab rule to variant identifiers, so the
//! two sides agree without a rename surface in vix.

pub mod frontmatter;

pub use frontmatter::{Frontmatter, FrontmatterError};

use std::collections::BTreeMap;

/// A whole manifest document. Sections are `BTreeMap`s: styx spells them as
/// maps keyed by public name, and the sorted map keeps every enumeration of
/// the manifest deterministic without a canonicalization pass.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PackageManifest {
    pub package: PackageMeta,
    /// `vx r <pkg> <name>` entry points: bare fn refs.
    #[facet(default)]
    pub command: BTreeMap<String, Command>,
    /// `vx build <pkg>#<name>` outputs: bare fn refs whose fns must return
    /// Tree or Blob (the claim `vixen.delivery.result` requires, checked at
    /// binding).
    #[facet(default)]
    pub artifact: BTreeMap<String, Artifact>,
    /// Exposed executables: a path into an artifact this package builds, plus
    /// the capability-package contracts.
    #[facet(default)]
    pub program: BTreeMap<String, Program>,
}

/// The `package { … }` header: the package's own word about itself.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PackageMeta {
    pub name: String,
    pub version: String,
}

/// A `vx r` entry point: nothing but a public name (the map key) bound to a
/// vix fn. The fn is the whole recipe — its parameters declare its
/// requirements, its return type decides the presentation.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct Command {
    /// `module::fn` path into the package's own vix code.
    pub r#fn: String,
}

/// A `vx build` output: a public name bound to a vix fn returning Tree or
/// Blob. Purity makes an artifact nothing but its fn's memoized value.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct Artifact {
    /// `module::fn` path; the fn must return Tree or Blob
    /// (vixen.delivery.result — anything else cannot be materialized).
    pub r#fn: String,
}

/// An exposed executable: where its bytes live and how to run it — the
/// on-disk spelling of a capability package (vixen.capability.package-is-
/// data). The program's key provides the capability type.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub source: ProgramSource,
    pub protocol: Protocol,
    pub target: Target,
}

/// Where a program's bytes come from. Which side of the seam a capability
/// came from is invisible to consumers: one carved from a just-built artifact
/// and one that arrived some other way bind identically.
///
/// One arm today. It stays an enum rather than collapsing to a struct because
/// the styx spelling is a tagged payload (`source @artifact{…}`) and because
/// the second arm is a named gap, not an absence: exposing a program sourced
/// from *outside* this package has no spelling yet. The retracted `@input`
/// arm was that spelling, and it rested on a declarative input section that
/// did not survive.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
#[facet(rename_all = "kebab-case")]
pub enum ProgramSource {
    /// A path into one of this package's own artifacts.
    Artifact { name: String, path: String },
}

/// The exec output protocol contract, mirroring
/// `vix::runtime::exec_backend::ExecOutputProtocol`.
#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
#[facet(rename_all = "kebab-case")]
pub enum Protocol {
    ExitOnly,
    ProgressiveLinesV1,
}

/// The target discipline contract, mirroring the variants of
/// `vixen_primitives::capability_package::TargetDiscipline` (data only — the
/// normalize hooks are code and stay in the capability package).
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
#[facet(rename_all = "kebab-case")]
pub enum Target {
    /// The program produces host-neutral output; no target enters identity.
    Neutral,
    /// The target is an argv flag (`--target <triple>`).
    ArgvFlag { flag: String },
    /// The target is carried by role-named environment variables. Field
    /// names are single words on purpose: the vix mirror of this type has no
    /// rename surface, so a field name must be spellable identically in a
    /// styx document, a Rust struct, and a vix struct.
    EnvRoles { os: String, arch: String },
    /// The program only ever produces one target.
    FixedTarget { target: String },
}
