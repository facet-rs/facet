use facet::Facet;

use crate::{ConnectionSettings, Metadata, Parity};

// r[impl session.handshake]
// r[impl session.handshake.phon]
/// Phon self-describing handshake message exchanged before compact session traffic begins.
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum HandshakeMessage {
    Hello(Hello),
    HelloYourself(HelloYourself),
    LetsGo(LetsGo),
    Sorry(Sorry),
}

// r[impl session.handshake]
// r[impl session.handshake.unversioned]
/// Sent by the initiator as the first handshake message.
#[derive(Debug, Clone, Facet)]
pub struct Hello {
    /// The identifier partition desired by the initiator.
    pub parity: Parity,
    /// Connection limits advertised by the initiator for the root connection.
    // r[impl session.connection-settings.hello]
    pub connection_settings: ConnectionSettings,
    // r[impl session.handshake.protocol-schema]
    // r[impl session.handshake.protocol-schema.session-scoped]
    /// The initiator's schema for MessagePayload — the compact enum used
    /// for all subsequent communication.
    pub message_payload_schema: Vec<u8>,
    /// Metadata sent by the initiator (e.g. `vox-service` for service routing).
    #[facet(default)]
    pub metadata: Metadata,
}

// r[impl session.handshake]
// r[impl session.handshake.unversioned]
/// Sent by the acceptor in response to Hello.
#[derive(Debug, Clone, Facet)]
pub struct HelloYourself {
    /// Connection limits advertised by the acceptor for the root connection.
    // r[impl session.connection-settings.hello]
    pub connection_settings: ConnectionSettings,
    // r[impl session.handshake.protocol-schema]
    // r[impl session.handshake.protocol-schema.session-scoped]
    /// The acceptor's schema for MessagePayload.
    pub message_payload_schema: Vec<u8>,
    /// Metadata sent by the acceptor.
    #[facet(default)]
    pub metadata: Metadata,
}

// r[impl session.handshake]
/// Sent by the initiator to confirm schema compatibility and establish the session.
#[derive(Debug, Clone, Facet)]
pub struct LetsGo {}

// r[impl session.handshake.sorry]
/// Sent by either peer to reject the session due to schema incompatibility.
#[derive(Debug, Clone, Facet)]
pub struct Sorry {
    pub reason: String,
}

/// Result of a completed phon handshake.
#[derive(Debug, Clone)]
pub struct HandshakeResult {
    pub role: crate::SessionRole,
    pub our_settings: ConnectionSettings,
    pub peer_settings: ConnectionSettings,
    pub our_schema: Vec<u8>,
    pub peer_schema: Vec<u8>,
    /// Metadata received from the peer during handshake.
    pub peer_metadata: Metadata,
}
