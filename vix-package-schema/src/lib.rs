//! The vix package manifest (`package.styx`) document types.
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

use std::collections::BTreeMap;

/// A whole `package.styx` document. Sections are `BTreeMap`s: styx spells
/// them as maps keyed by public name, and the sorted map keeps every
/// enumeration of the manifest deterministic without a canonicalization
/// pass.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PackageManifest {
    pub package: PackageMeta,
    /// Provider pins, keyed by input name. NOT a requirement list —
    /// requirements are the capability parameters of the fns the other
    /// sections name; an input answers *which package satisfies them, at
    /// which identity*.
    #[facet(default)]
    pub input: BTreeMap<String, Input>,
    /// `vx r <pkg> <name>` entry points: bare fn refs.
    #[facet(default)]
    pub command: BTreeMap<String, Command>,
    /// `vx build <pkg>#<name>` outputs: bare fn refs whose fns must return
    /// Tree or Blob (the materializability claim, checked at binding).
    #[facet(default)]
    pub artifact: BTreeMap<String, Artifact>,
    /// Exposed executables: a path into a tree, plus the capability-package
    /// contracts.
    #[facet(default)]
    pub program: BTreeMap<String, Program>,
}

/// The `package { … }` header: the package's own word about itself.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PackageMeta {
    pub name: String,
    pub version: String,
}

/// One arm per way a value can enter a package from outside. Deliberately no
/// `Path` arm: a local directory is an unpinned word, and overrides belong
/// to policy (`vx r --override-input`), never to the manifest.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
#[facet(rename_all = "kebab-case")]
pub enum Input {
    /// Another vix package, pinned by its source-tree identity. That tree
    /// contains the provider's own manifest, so its pins are covered
    /// transitively — one hash pins the closure.
    Package { version: String, hash: String },
    /// A pinned blob fetch — decodes toward the wire shape `PinnedBlobRef`:
    /// the coordinate's scheme names the origin adapter, the hash is the
    /// upstream digest in the pins.md spelling (`"<alg>:<hex>"`, algorithm
    /// inside the value), and `unpack` is the blob→tree step whose result
    /// carries the identity.
    Fetch {
        origin: String,
        hash: String,
        unpack: Unpack,
    },
}

/// How a fetched archive becomes a Tree. The resulting tree's identity is
/// the input's identity contribution; the archive bytes are transport.
#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
#[facet(rename_all = "kebab-case")]
pub enum Unpack {
    TarGz,
    TarXz,
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

/// Where a program's tree comes from. Invisible to consumers: a capability
/// resolved from a fetched toolchain and one carved from a built artifact
/// bind identically.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
#[facet(rename_all = "kebab-case")]
pub enum ProgramSource {
    /// A path into a declared input's tree (`input` names the entry in the
    /// `input` section).
    Input { name: String, path: String },
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
    /// The target is carried by role-named environment variables.
    EnvRoles {
        #[facet(rename = "os-role")]
        os_role: String,
        #[facet(rename = "arch-role")]
        arch_role: String,
    },
    /// The program only ever produces one target.
    FixedTarget { target: String },
}
