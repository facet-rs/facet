use crate::{ConnectionRole, Metadata, MetadataExt};

/// One identity epoch exists in the v1 connection policy model.
// r[impl connection.identity.late-claims]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentityEpoch(pub u64);

impl IdentityEpoch {
    pub const V1: Self = Self(0);
}

impl Default for IdentityEpoch {
    fn default() -> Self {
        Self::V1
    }
}

/// Locally asserted information about the counterpart or accepted link.
///
/// Application payloads and metadata are claims, not evidence. Built-in
/// transports and embeddings should be the only producers of these values.
// r[impl connection.evidence]
#[derive(Debug, Clone, Default)]
pub struct PeerEvidence {
    items: Vec<PeerEvidenceItem>,
}

impl PeerEvidence {
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Build non-empty evidence from local transport/runtime assertions.
    ///
    /// # Safety
    ///
    /// The caller must be trusted transport, platform, embedding, or test
    /// harness code asserting facts learned locally. Peer-authored metadata,
    /// payloads, service handlers, and policy callbacks must not be laundered
    /// into evidence through this constructor.
    #[must_use]
    pub unsafe fn from_runtime_asserted(items: Vec<PeerEvidenceItem>) -> Self {
        Self { items }
    }

    /// Build synthetic evidence for memory transports and tests.
    ///
    /// # Safety
    ///
    /// The caller must ensure the synthetic identity is a local test/runtime
    /// assertion, not a peer-authored claim.
    #[must_use]
    pub unsafe fn synthetic(label: impl Into<String>) -> Self {
        Self {
            items: vec![PeerEvidenceItem::Synthetic {
                label: label.into(),
            }],
        }
    }

    #[must_use]
    pub fn items(&self) -> &[PeerEvidenceItem] {
        &self.items
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Evidence kinds intentionally remain coarse. Platform integrations can carry
/// redactable details without requiring core Vox to understand every OS API.
// r[impl connection.identity.use-cases]
// r[impl connection.identity.redaction]
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum PeerEvidenceItem {
    Synthetic {
        label: String,
    },
    Tls {
        verified_subject: Option<String>,
        alpn: Option<String>,
    },
    UnixPeerCredentials {
        uid: Option<u32>,
        gid: Option<u32>,
        pid: Option<u32>,
    },
    PlatformProcess {
        description: String,
    },
    Xpc {
        code_signing_identity: String,
    },
    InProcess {
        component: String,
    },
}

// r[impl connection.identity.forms]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PeerIdentityForm {
    Anonymous,
    Synthetic,
    LocalProcess,
    CertificateBacked,
    ApplicationUser,
    Composite,
}

// r[impl connection.identity.forms]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum IdentityBasisProvenance {
    EvidenceBacked,
    VerifiedClaimBacked,
    Synthetic,
}

// r[impl connection.identity.forms]
// r[impl connection.identity.redaction]
#[derive(Debug, Clone)]
pub struct IdentityBasis {
    pub form: PeerIdentityForm,
    pub provenance: IdentityBasisProvenance,
    pub redacted: String,
}

impl IdentityBasis {
    #[must_use]
    pub fn new(
        form: PeerIdentityForm,
        provenance: IdentityBasisProvenance,
        redacted: impl Into<String>,
    ) -> Self {
        Self {
            form,
            provenance,
            redacted: redacted.into(),
        }
    }
}

/// The local peer's policy-resolved view of the counterpart.
// r[impl connection.identity]
// r[impl connection.identity.scope]
#[derive(Debug, Clone)]
pub struct PeerIdentity {
    epoch: IdentityEpoch,
    form: PeerIdentityForm,
    bases: Vec<IdentityBasis>,
}

impl PeerIdentity {
    #[must_use]
    pub fn anonymous() -> Self {
        Self {
            epoch: IdentityEpoch::V1,
            form: PeerIdentityForm::Anonymous,
            bases: Vec::new(),
        }
    }

    #[must_use]
    pub fn from_basis(basis: IdentityBasis) -> Self {
        Self {
            epoch: IdentityEpoch::V1,
            form: basis.form,
            bases: vec![basis],
        }
    }

    #[must_use]
    pub fn composite(bases: Vec<IdentityBasis>) -> Self {
        let form = if bases.len() <= 1 {
            bases
                .first()
                .map_or(PeerIdentityForm::Anonymous, |basis| basis.form)
        } else {
            PeerIdentityForm::Composite
        };
        Self {
            epoch: IdentityEpoch::V1,
            form,
            bases,
        }
    }

    #[must_use]
    pub fn epoch(&self) -> IdentityEpoch {
        self.epoch
    }

    #[must_use]
    pub fn form(&self) -> PeerIdentityForm {
        self.form
    }

    #[must_use]
    pub fn bases(&self) -> &[IdentityBasis] {
        &self.bases
    }

    #[must_use]
    pub fn is_anonymous(&self) -> bool {
        matches!(self.form, PeerIdentityForm::Anonymous)
    }
}

impl Default for PeerIdentity {
    fn default() -> Self {
        Self::anonymous()
    }
}

/// Local authorization output attached to an accepted lane.
///
/// A grant is produced by local lane policy after it evaluates the connection
/// identity, locally asserted evidence, and lane-open metadata. It is not a
/// peer-authored credential; public grant detail may be mirrored into
/// lane-accept metadata, but the runtime treats the local grant object as the
/// authoritative authorization context for requests on that lane.
// r[impl lane.authorization.context]
// r[impl request.authorization]
#[derive(Debug, Clone, Default)]
pub struct LaneGrant {
    metadata: Metadata,
}

impl LaneGrant {
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn from_metadata(metadata: Metadata) -> Self {
        Self { metadata }
    }

    #[must_use]
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.metadata.meta_is_empty()
    }
}

/// Authorization facts made available to one dispatched request.
///
/// This context is owned so generated dispatchers can place it in request
/// extensions without borrowing the driver task. It preserves the distinction
/// between connection-scoped identity/evidence and lane-scoped policy output.
// r[impl request.authorization]
#[derive(Debug, Clone)]
pub struct RequestAuthorizationContext {
    peer_identity: PeerIdentity,
    peer_evidence: PeerEvidence,
    lane_grant: LaneGrant,
}

impl RequestAuthorizationContext {
    #[must_use]
    pub fn new(
        peer_identity: PeerIdentity,
        peer_evidence: PeerEvidence,
        lane_grant: LaneGrant,
    ) -> Self {
        Self {
            peer_identity,
            peer_evidence,
            lane_grant,
        }
    }

    #[must_use]
    pub fn anonymous() -> Self {
        Self::new(
            PeerIdentity::anonymous(),
            PeerEvidence::none(),
            LaneGrant::empty(),
        )
    }

    #[must_use]
    pub fn peer_identity(&self) -> &PeerIdentity {
        &self.peer_identity
    }

    #[must_use]
    pub fn peer_evidence(&self) -> &PeerEvidence {
        &self.peer_evidence
    }

    #[must_use]
    pub fn lane_grant(&self) -> &LaneGrant {
        &self.lane_grant
    }
}

/// Inputs available to connection identity resolution.
// r[impl connection.identity.inputs]
// r[impl connection.identity.local]
// r[impl connection.identity.resolver]
pub struct IdentityResolutionContext<'a> {
    pub role: ConnectionRole,
    pub evidence: &'a PeerEvidence,
    pub claims: &'a Metadata,
}
