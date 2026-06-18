use facet::Facet;

use crate::{ConnectionSettings, Metadata, Parity};

// r[impl connection.handshake]
// r[impl connection.handshake.phon]
/// Phon self-describing handshake message exchanged before compact connection traffic begins.
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum HandshakeMessage {
    Hello(Hello),
    HelloYourself(HelloYourself),
    LetsGo(LetsGo),
    Decline(Decline),
    Sorry(Sorry),
}

// r[impl connection.handshake]
// r[impl connection.handshake.unversioned]
/// Sent by the initiator as the first handshake message.
#[derive(Debug, Clone, Facet)]
pub struct Hello {
    /// The identifier partition desired by the initiator.
    pub parity: Parity,
    /// Connection-default and control-lane limits advertised by the initiator.
    // r[impl connection.handshake.lane-settings]
    pub connection_settings: ConnectionSettings,
    // r[impl connection.handshake.protocol-schema]
    // r[impl connection.handshake.protocol-schema.connection-scoped]
    /// The initiator's schema for MessagePayload — the compact enum used
    /// for all subsequent communication.
    pub message_payload_schema: Vec<u8>,
    /// Metadata sent by the initiator (e.g. `vox-service` for service routing).
    // r[impl connection.handshake.metadata]
    // r[impl rpc.metadata.records]
    #[facet(default)]
    pub metadata: Metadata,
}

// r[impl connection.handshake]
// r[impl connection.handshake.unversioned]
/// Sent by the acceptor in response to Hello.
#[derive(Debug, Clone, Facet)]
pub struct HelloYourself {
    /// Connection-default and control-lane limits advertised by the acceptor.
    // r[impl connection.handshake.lane-settings]
    pub connection_settings: ConnectionSettings,
    // r[impl connection.handshake.protocol-schema]
    // r[impl connection.handshake.protocol-schema.connection-scoped]
    /// The acceptor's schema for MessagePayload.
    pub message_payload_schema: Vec<u8>,
    /// Metadata sent by the acceptor.
    // r[impl connection.handshake.metadata]
    // r[impl rpc.metadata.records]
    #[facet(default)]
    pub metadata: Metadata,
}

// r[impl connection.handshake]
/// Sent by the initiator to confirm schema compatibility and establish the connection.
#[derive(Debug, Clone, Facet)]
pub struct LetsGo {}

// r[impl rejection.reason.taxonomy]
// r[impl connection.policy.establishment.rejection]
/// Typed connection-establishment policy rejection reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum EstablishmentRejectReason {
    Unauthenticated,
    Forbidden,
    NotReady,
    Draining,
    Unsupported,
    PolicyRejected,
}

impl EstablishmentRejectReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unauthenticated => "unauthenticated",
            Self::Forbidden => "forbidden",
            Self::NotReady => "not-ready",
            Self::Draining => "draining",
            Self::Unsupported => "unsupported",
            Self::PolicyRejected => "policy-rejected",
        }
    }
}

impl std::fmt::Display for EstablishmentRejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// r[impl connection.handshake.decline]
// r[impl connection.policy.establishment.rejection]
/// Sent by either peer to reject connection establishment due to policy.
#[derive(Debug, Clone, Facet)]
pub struct Decline {
    pub reason: EstablishmentRejectReason,
    // r[impl rpc.metadata.records]
    #[facet(default)]
    pub metadata: Metadata,
}

impl Decline {
    #[must_use]
    pub fn new(reason: EstablishmentRejectReason) -> Self {
        Self {
            reason,
            metadata: Metadata::default(),
        }
    }

    #[must_use]
    pub fn with_metadata(reason: EstablishmentRejectReason, metadata: Metadata) -> Self {
        Self { reason, metadata }
    }
}

// r[impl connection.handshake.sorry]
/// Sent by either peer to reject the connection due to schema incompatibility.
#[derive(Debug, Clone, Facet)]
pub struct Sorry {
    pub reason: String,
}

/// Result of a completed phon handshake.
#[derive(Debug, Clone)]
pub struct HandshakeResult {
    pub role: crate::ConnectionRole,
    pub our_settings: ConnectionSettings,
    pub peer_settings: ConnectionSettings,
    pub our_schema: Vec<u8>,
    pub peer_schema: Vec<u8>,
    /// Metadata received from the peer during handshake.
    pub peer_metadata: Metadata,
    /// Locally asserted evidence used while resolving the peer identity.
    // r[impl connection.evidence]
    pub peer_evidence: crate::PeerEvidence,
    /// Local policy-resolved identity for the peer.
    // r[impl connection.identity]
    pub peer_identity: crate::PeerIdentity,
}
