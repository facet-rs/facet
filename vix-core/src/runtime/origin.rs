//! The origin seam: how bytes outside the machine enter it.
//!
//! An origin backend is an installed authority, never scheduler code
//! (`machine.primitive.effect-backend-service`). This module owns the seam's
//! vocabulary — the [`OriginAdapter`] trait's two verbs, the declarations that
//! route requests to installed adapters, and the structured failure taxonomy —
//! and none of it names a particular backend: the harness's offline store and
//! the HTTP transport are two entries in the same declared set.
//!
//! Routing is a lookup over declarations, not a sniff: each installed adapter
//! states the coordinate schemes it serves, the capability schema it admits,
//! and the tree-handle namespace it owns, as data. Adapters therefore do not
//! re-check any of it, the machine holds no default backend, and an
//! unroutable request is a loud typed refusal naming what was asked and what
//! is installed — the silent-fallback conjuring failure mode is structurally
//! unavailable.
//!
//! r[impl machine.primitive.origin-routing]
//! r[impl machine.primitive.origin-verbs]

use std::sync::Arc;

use crate::schema::SchemaPattern;

use super::model::TreeEntryKind;
use super::{PrimitiveMachineError, ValueId};

/// Why a coordinate read could not be served. The three answers are the
/// origin taxonomy and they are deliberately different: a miss may fall
/// through to another origin, a refusal routes elsewhere (or surfaces as a
/// configuration error), a corruption stops the demand.
///
/// In the current adapter set only `Miss` has producers: refusal is produced
/// by declaration routing (adapters stop sniffing), and corruption is
/// detected at the seam's pin check (`EffectCtx::origin_candidate`) because
/// an adapter is untrusted transport. The variants exist so a backend that
/// CAN classify further (an integrity-checking store, say) has the
/// vocabulary.
///
/// r[impl machine.primitive.origin-verbs]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OriginReadError {
    /// Nothing at the coordinate. A multi-origin fetch may fall through.
    Miss { detail: String },
    /// This adapter does not serve the request; route elsewhere.
    Refusal { detail: String },
    /// The backend served something it knows is wrong. Stop.
    Corruption { detail: String },
}

/// Why a tree-projection verb could not be served. `Missing` and `WrongKind`
/// are distinguishable on purpose: an entry that exists with a different
/// shape is not a miss, and the rerun audit must be able to tell "the file
/// appeared" from "the file became a directory"
/// (`machine.primitive.witness-reverification`).
///
/// r[impl machine.primitive.origin-verbs]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OriginTreeError {
    /// Nothing exists at the path — including an absent parent directory.
    Missing,
    /// The entry exists with this kind, contradicting the request.
    WrongKind { found: TreeEntryKind },
    /// The backend cannot serve its own declared namespace coherently. Stop.
    Corruption { detail: String },
}

/// An origin backend service: the seam's two verbs.
///
/// **Coordinate read** (`read`): bytes by provenance coordinate. The adapter
/// is deliberately untrusted — verification against the pin happens at the
/// seam, so an origin cannot lie about what it served; it can only miss.
///
/// **Tree projection** (`tree_kind` / `tree_bytes` / `tree_directory`): entry
/// kind, file bytes, and directory listing for a lazily-backed tree the
/// adapter's declared namespace owns. Listings are `(name, TreeEntryKind)`
/// rows — kinds only, never full entries, so nothing is materialized that was
/// not asked for. An adapter that declares no tree namespace is never routed
/// a tree verb; the defaults refuse loudly in case one is reached anyway.
///
/// Adapters perform no scheme or capability checks: their declaration
/// ([`OriginAdapterDecl`]) already admitted the request before it was routed.
///
/// r[impl machine.primitive.origin-verbs]
pub trait OriginAdapter: Send + Sync {
    fn read(&self, capability: &ValueId, coordinate: &str) -> Result<Vec<u8>, OriginReadError>;

    fn tree_kind(&self, handle: &[u8], path: &str) -> Result<TreeEntryKind, OriginTreeError> {
        let _ = (handle, path);
        Err(OriginTreeError::Corruption {
            detail: "this origin adapter declares no tree namespace".to_owned(),
        })
    }

    fn tree_bytes(&self, handle: &[u8], path: &str) -> Result<Vec<u8>, OriginTreeError> {
        let _ = (handle, path);
        Err(OriginTreeError::Corruption {
            detail: "this origin adapter declares no tree namespace".to_owned(),
        })
    }

    fn tree_directory(
        &self,
        handle: &[u8],
        path: &str,
    ) -> Result<Vec<(String, TreeEntryKind)>, OriginTreeError> {
        let _ = (handle, path);
        Err(OriginTreeError::Corruption {
            detail: "this origin adapter declares no tree namespace".to_owned(),
        })
    }
}

/// What an installed origin adapter serves, as data. Selection is a lookup
/// over these declarations; the adapter itself never re-checks any of it.
///
/// r[impl machine.primitive.origin-routing]
#[derive(Clone, Debug)]
pub struct OriginAdapterDecl {
    /// Diagnostic name, spoken by refusals ("offline", "http-blob").
    pub name: String,
    /// The coordinate schemes served — the part of a coordinate before
    /// `://`. Each scheme may be claimed by at most one installed adapter.
    pub schemes: Vec<String>,
    /// The capability schema this adapter admits. A routed read whose
    /// capability does not match is a refusal, produced here, not by the
    /// adapter.
    pub capability: SchemaPattern,
    /// The tree-handle namespace this adapter owns: a byte prefix of the
    /// handle residents of the lazily-backed trees it serves (a harness
    /// adapter might declare `sample-tree\0`). `None` for adapters that serve
    /// no trees. Namespaces of installed adapters may not be prefix-related.
    pub tree_namespace: Option<Vec<u8>>,
}

/// One installed origin adapter with its declaration.
#[derive(Clone)]
pub struct OriginInstallation {
    pub decl: OriginAdapterDecl,
    pub adapter: Arc<dyn OriginAdapter>,
}

impl core::fmt::Debug for OriginInstallation {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("OriginInstallation")
            .field("decl", &self.decl)
            .finish_non_exhaustive()
    }
}

/// Why an origin adapter could not be installed: its declaration overlaps an
/// installed one, so routing would stop being a function of the declaration
/// set. Rejected at install time — "exactly as the original read did" is a
/// property of the declarations, not a hope.
///
/// r[impl machine.primitive.origin-routing]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OriginInstallError {
    SchemeClaimedTwice {
        scheme: String,
        first: String,
        second: String,
    },
    /// Two adapters declared prefix-related tree namespaces (equal namespaces
    /// included): a handle in the longer namespace would be claimed by both.
    NamespaceOverlap { first: String, second: String },
}

impl core::fmt::Display for OriginInstallError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::SchemeClaimedTwice {
                scheme,
                first,
                second,
            } => write!(
                formatter,
                "origin adapter \"{second}\" claims coordinate scheme \"{scheme}\" \
                 already claimed by \"{first}\""
            ),
            Self::NamespaceOverlap { first, second } => write!(
                formatter,
                "origin adapters \"{first}\" and \"{second}\" declare prefix-related \
                 tree-handle namespaces"
            ),
        }
    }
}

impl std::error::Error for OriginInstallError {}

/// How a tree source routes: recognized content-identified residents never
/// reach an adapter, a namespaced handle routes to the adapter whose declared
/// namespace owns it, and an unclaimed handle is the loud refusal.
pub enum TreeRouting<'set> {
    /// The resident bytes are a carrier, canonical, or ustar tree — the value
    /// carries its own members and no backend is consulted.
    ContentIdentified,
    Origin(&'set OriginInstallation),
    Unclaimed(PrimitiveMachineError),
}

/// The declared set of installed origin adapters
/// (`machine.primitive.origin-routing`): every routing decision the machine
/// makes about origins is a lookup over this set's declarations.
#[derive(Clone, Default)]
pub struct OriginAdapterSet {
    entries: Vec<OriginInstallation>,
}

impl OriginAdapterSet {
    /// Install one adapter under its declaration, rejecting overlap with the
    /// already-installed set.
    pub fn install(
        &mut self,
        decl: OriginAdapterDecl,
        adapter: Arc<dyn OriginAdapter>,
    ) -> Result<(), OriginInstallError> {
        for installed in &self.entries {
            for scheme in &decl.schemes {
                if installed.decl.schemes.contains(scheme) {
                    return Err(OriginInstallError::SchemeClaimedTwice {
                        scheme: scheme.clone(),
                        first: installed.decl.name.clone(),
                        second: decl.name.clone(),
                    });
                }
            }
            if let (Some(installed_ns), Some(new_ns)) =
                (&installed.decl.tree_namespace, &decl.tree_namespace)
                && (installed_ns.starts_with(new_ns) || new_ns.starts_with(installed_ns))
            {
                return Err(OriginInstallError::NamespaceOverlap {
                    first: installed.decl.name.clone(),
                    second: decl.name.clone(),
                });
            }
        }
        self.entries.push(OriginInstallation { decl, adapter });
        Ok(())
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// What is installed, for refusal diagnostics: the refusal names the
    /// declaration set so an unconfigured origin reads as the configuration
    /// gap it is, never as a mysterious miss.
    #[must_use]
    fn installed_summary(&self) -> String {
        if self.entries.is_empty() {
            return "no origin adapter is installed".to_owned();
        }
        let entries = self
            .entries
            .iter()
            .map(|installation| {
                format!(
                    "\"{}\" (schemes: {})",
                    installation.decl.name,
                    installation.decl.schemes.join(", ")
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("installed origin adapters: {entries}")
    }

    /// Route a coordinate read by declared scheme and capability schema.
    ///
    /// r[impl machine.primitive.origin-routing]
    pub fn route_coordinate(
        &self,
        capability: &ValueId,
        coordinate: &str,
    ) -> Result<&OriginInstallation, PrimitiveMachineError> {
        let Some((scheme, _)) = coordinate.split_once("://") else {
            return Err(PrimitiveMachineError::OriginUnroutable {
                detail: format!(
                    "origin read {coordinate} refused: the coordinate names no scheme; {}",
                    self.installed_summary()
                ),
            });
        };
        let Some(installation) = self
            .entries
            .iter()
            .find(|installation| installation.decl.schemes.iter().any(|s| s == scheme))
        else {
            return Err(PrimitiveMachineError::OriginUnroutable {
                detail: format!(
                    "origin read {coordinate} refused: no installed origin adapter serves \
                     scheme \"{scheme}\"; {}",
                    self.installed_summary()
                ),
            });
        };
        if !installation.decl.capability.matches(&capability.schema) {
            return Err(PrimitiveMachineError::OriginUnroutable {
                detail: format!(
                    "origin read {coordinate} refused: adapter \"{}\" serves scheme \
                     \"{scheme}\" but does not admit capability schema {}; {}",
                    installation.decl.name,
                    capability.schema,
                    self.installed_summary()
                ),
            });
        }
        Ok(installation)
    }

    /// Route a tree source by its resident bytes: content-identified residents
    /// are recognized first and never route to an adapter; otherwise the
    /// declared namespaces are matched by byte prefix.
    ///
    /// r[impl machine.primitive.origin-routing]
    #[must_use]
    pub fn route_tree(&self, resident: &[u8]) -> TreeRouting<'_> {
        if super::tree_resident::is_content_identified_tree(resident) {
            return TreeRouting::ContentIdentified;
        }
        if let Some(installation) = self.entries.iter().find(|installation| {
            installation
                .decl
                .tree_namespace
                .as_ref()
                .is_some_and(|namespace| resident.starts_with(namespace))
        }) {
            return TreeRouting::Origin(installation);
        }
        TreeRouting::Unclaimed(PrimitiveMachineError::OriginUnroutable {
            detail: format!(
                "tree read refused: the source's resident bytes (leading {:?}) are neither \
                 a content-identified tree nor a handle in any declared namespace; {}",
                String::from_utf8_lossy(&resident[..resident.len().min(24)]),
                self.installed_summary()
            ),
        })
    }

    /// The adapter-relative name of a lazily-backed tree handle: its resident
    /// bytes with the owning adapter's declared namespace stripped. `None`
    /// for content-identified trees and unclaimed handles.
    ///
    /// This is what makes tree projections **name-relative** without any
    /// primitive knowing a backend: an origin-backed tree's projection paths
    /// are spelled `<name>/<path>` — the witness names its subject in the
    /// owning adapter's coordinate space — and the *seam* derives the name
    /// from the declaration set, so no primitive spells any namespace.
    ///
    /// r[impl machine.primitive.origin-verbs]
    #[must_use]
    pub fn tree_handle_name(&self, resident: &[u8]) -> Option<Vec<u8>> {
        match self.route_tree(resident) {
            TreeRouting::Origin(installation) => {
                let namespace = installation.decl.tree_namespace.as_ref()?;
                Some(resident[namespace.len()..].to_vec())
            }
            TreeRouting::ContentIdentified | TreeRouting::Unclaimed(_) => None,
        }
    }
}

/// Fold an adapter's coordinate-read answer into the machine error channel.
/// The mapping IS the taxonomy: a miss stays fall-through-able
/// (`ProjectionMissing`), a refusal stays a routing statement
/// (`OriginUnroutable`), a corruption stays a stop (`OriginCorrupt`).
pub(crate) fn origin_read_machine_error(
    error: OriginReadError,
    coordinate: &str,
) -> PrimitiveMachineError {
    match error {
        OriginReadError::Miss { detail } => PrimitiveMachineError::ProjectionMissing {
            detail: format!("origin read {coordinate} missed: {detail}"),
        },
        OriginReadError::Refusal { detail } => PrimitiveMachineError::OriginUnroutable {
            detail: format!("origin read {coordinate} refused by its adapter: {detail}"),
        },
        OriginReadError::Corruption { detail } => PrimitiveMachineError::OriginCorrupt {
            detail: format!("origin read {coordinate} served corrupt bytes: {detail}"),
        },
    }
}

/// Fold an adapter's tree-verb answer into the machine error channel,
/// preserving the miss/wrong-kind distinction the witness vocabulary needs.
pub(crate) fn origin_tree_machine_error(
    error: OriginTreeError,
    path: &str,
) -> PrimitiveMachineError {
    match error {
        OriginTreeError::Missing => PrimitiveMachineError::ProjectionMissing {
            detail: format!("tree path {path} is absent"),
        },
        OriginTreeError::WrongKind { found } => PrimitiveMachineError::ProjectionWrongKind {
            found,
            detail: format!("tree path {path} exists with a contradicting kind"),
        },
        OriginTreeError::Corruption { detail } => PrimitiveMachineError::OriginCorrupt {
            detail: format!("tree path {path} could not be served: {detail}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vir::Type;

    struct NullAdapter;

    impl OriginAdapter for NullAdapter {
        fn read(
            &self,
            _capability: &ValueId,
            _coordinate: &str,
        ) -> Result<Vec<u8>, OriginReadError> {
            Err(OriginReadError::Miss {
                detail: "null adapter serves nothing".to_owned(),
            })
        }
    }

    fn decl(name: &str, schemes: &[&str], namespace: Option<&[u8]>) -> OriginAdapterDecl {
        OriginAdapterDecl {
            name: name.to_owned(),
            schemes: schemes.iter().map(|s| (*s).to_owned()).collect(),
            capability: SchemaPattern::exact(&Type::String.schema_ref()),
            tree_namespace: namespace.map(<[u8]>::to_vec),
        }
    }

    /// r[verify machine.primitive.origin-routing]
    #[test]
    fn a_scheme_claimed_twice_is_rejected_at_install_time() {
        let mut set = OriginAdapterSet::default();
        set.install(decl("first", &["offline"], None), Arc::new(NullAdapter))
            .expect("first install succeeds");
        assert_eq!(
            set.install(
                decl("second", &["https", "offline"], None),
                Arc::new(NullAdapter)
            ),
            Err(OriginInstallError::SchemeClaimedTwice {
                scheme: "offline".to_owned(),
                first: "first".to_owned(),
                second: "second".to_owned(),
            })
        );
    }

    /// r[verify machine.primitive.origin-routing]
    #[test]
    fn prefix_related_namespaces_are_rejected_at_install_time() {
        let mut set = OriginAdapterSet::default();
        set.install(
            decl("first", &["a"], Some(b"sample-tree\0")),
            Arc::new(NullAdapter),
        )
        .expect("first install succeeds");
        // A strict prefix of an installed namespace: every handle the
        // installed adapter owns would also be claimed by this one.
        assert_eq!(
            set.install(decl("second", &["b"], Some(b"sample")), Arc::new(NullAdapter)),
            Err(OriginInstallError::NamespaceOverlap {
                first: "first".to_owned(),
                second: "second".to_owned(),
            })
        );
        // Disjoint namespaces coexist.
        set.install(
            decl("third", &["c"], Some(b"other-tree\0")),
            Arc::new(NullAdapter),
        )
        .expect("a disjoint namespace installs");
    }

    /// r[verify machine.primitive.origin-routing]
    #[test]
    fn an_unclaimed_scheme_is_a_loud_refusal_naming_the_installed_set() {
        let mut set = OriginAdapterSet::default();
        set.install(decl("offline", &["offline"], None), Arc::new(NullAdapter))
            .expect("install succeeds");
        let capability =
            super::super::FramedNode::leaf(Type::String.schema_ref(), b"cap".to_vec()).identity();
        let error = set
            .route_coordinate(&capability, "https://example.test/blob")
            .expect_err("unclaimed scheme refuses");
        let PrimitiveMachineError::OriginUnroutable { detail } = error else {
            panic!("expected a typed unroutable refusal, got {error:?}");
        };
        assert!(detail.contains("https://example.test/blob"), "{detail}");
        assert!(detail.contains("\"offline\""), "{detail}");
    }

    /// r[verify machine.primitive.origin-routing]
    #[test]
    fn an_inadmissible_capability_is_a_refusal_produced_by_routing() {
        let mut set = OriginAdapterSet::default();
        set.install(decl("offline", &["offline"], None), Arc::new(NullAdapter))
            .expect("install succeeds");
        // The declaration admits String; route with an Int-schema capability.
        let capability =
            super::super::FramedNode::leaf(Type::Int.schema_ref(), b"cap".to_vec()).identity();
        let error = set
            .route_coordinate(&capability, "offline://registry/x")
            .expect_err("inadmissible capability refuses");
        assert!(matches!(
            error,
            PrimitiveMachineError::OriginUnroutable { .. }
        ));
    }
}
