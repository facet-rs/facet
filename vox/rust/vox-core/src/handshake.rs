use vox_types::{
    ConnectionRole, ConnectionSettings, Decline, HandshakeMessage, HandshakeResult,
    IdentityResolutionContext, LinkRx, LinkTx, Metadata, PeerEvidence, PeerIdentity,
};

const INITIAL_CHANNEL_CREDIT_ZERO_ERROR: &str = "initial_channel_credit must be greater than zero";

#[derive(Debug)]
pub enum HandshakeError {
    Io(std::io::Error),
    Encode(String),
    Decode(String),
    PeerClosed,
    Protocol(String),
    Declined(Decline),
    Sorry(String),
}

impl std::fmt::Display for HandshakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "handshake io error: {e}"),
            Self::Encode(e) => write!(f, "handshake encode error: {e}"),
            Self::Decode(e) => write!(f, "handshake decode error: {e}"),
            Self::PeerClosed => write!(f, "peer closed during handshake"),
            Self::Protocol(msg) => write!(f, "handshake protocol error: {msg}"),
            Self::Declined(decline) => write!(f, "handshake declined: {}", decline.reason),
            Self::Sorry(reason) => write!(f, "handshake rejected: {reason}"),
        }
    }
}

impl std::error::Error for HandshakeError {}

// r[impl rpc.flow-control.credit.initial.zero]
fn validate_initial_channel_credit(settings: &ConnectionSettings) -> Result<(), HandshakeError> {
    if settings.initial_channel_credit == 0 {
        return Err(HandshakeError::Protocol(
            INITIAL_CHANNEL_CREDIT_ZERO_ERROR.into(),
        ));
    }
    Ok(())
}

/// The `Message` envelope schema as phon self-describing bytes — exchanged in the
/// handshake so each peer can build a compatibility decode program for the other's
/// `Message` (`r[connection.handshake.protocol-schema]`).
fn message_schema() -> Vec<u8> {
    vox_phon::schema_bytes::<vox_types::Message<'static>>()
        .expect("derive phon schema for Message envelope")
}

/// Content-derived root id of our own `Message` schema — the first eight bytes of
/// [`message_schema`], hoisted so it can be compared when the closure is absent.
fn message_root() -> u64 {
    vox_phon::schema_root_id::<vox_types::Message<'static>>()
        .expect("derive phon schema root for Message envelope")
}

/// Content-derived root id of our own `HandshakeMessage` schema — the closure
/// `to_self_describing` embeds in every handshake frame.
fn handshake_root() -> u64 {
    vox_phon::schema_root_id::<HandshakeMessage>()
        .expect("derive phon schema root for HandshakeMessage")
}

// r[impl connection.handshake.protocol-schema.connection-scoped]
/// Whether a peer that sent these two root ids can decode our compact form.
///
/// Both must match. The `HandshakeMessage` root gates the envelope (the peer must be
/// able to supply the closure `to_self_describing_by_root` leaves out), the `Message`
/// root gates the omitted `message_payload_schema`. Absent — which is what every peer
/// built before these fields existed sends — is a mismatch, so the full form is used,
/// which is the whole reason the fields are `Option` and not defaulted `u64`s.
fn peer_accepts_compact(peer_handshake_root: Option<u64>, peer_message_root: Option<u64>) -> bool {
    peer_handshake_root == Some(handshake_root()) && peer_message_root == Some(message_root())
}

/// The peer's `Message` schema closure, whether it sent one or named ours by id.
///
/// A compact sender leaves `message_payload_schema` empty and puts its root in
/// `compact_message_root`; equal content-derived ids mean its closure is byte-for-byte
/// the one we just derived, so we stand in our own. Everything downstream —
/// `validate_message_writer_schema`, `MessagePlan`, `HandshakeResult::peer_schema` —
/// then sees exactly what it would have seen from a peer that sent the bytes.
fn resolve_peer_message_schema(
    carried: Vec<u8>,
    claimed_root: Option<u64>,
    our_schema: &[u8],
) -> Result<Vec<u8>, String> {
    if !carried.is_empty() {
        return Ok(carried);
    }
    match claimed_root {
        Some(root) if root == message_root() => Ok(our_schema.to_vec()),
        Some(root) => Err(format!(
            "peer omitted its Message schema and named root {root:#x}, which is not ours \
             ({:#x}); a peer may only omit the closure when it has named our id",
            message_root()
        )),
        None => Err("peer sent an empty Message schema and named no root id".to_string()),
    }
}

/// Send a handshake message on a raw link.
///
/// `compact` names the schema by root id instead of carrying the whole closure — 4,215
/// bytes of `HandshakeMessage` descriptor that the peer, having published the same id,
/// already has. It is only ever passed `true` after the peer said so in the message
/// before this one, so the first frame in each direction is never compact.
async fn send_handshake<Tx: LinkTx>(
    tx: &Tx,
    msg: &HandshakeMessage,
    compact: bool,
) -> Result<(), HandshakeError> {
    let bytes = if compact {
        vox_phon::to_self_describing_by_root(msg)
    } else {
        vox_phon::to_self_describing(msg)
    }
    .map_err(|e| HandshakeError::Encode(e.to_string()))?;
    vox_types::dlog!(
        "[handshake] send {:?} ({} bytes{})",
        handshake_tag(msg),
        bytes.len(),
        if compact { ", compact" } else { "" }
    );
    tx.send(bytes).await.map_err(HandshakeError::Io)
}

/// Receive and decode a self-describing handshake message from a raw link. The
/// embedded writer schema feeds the compatibility decode program for the local
/// `HandshakeMessage`, so even the bootstrap message survives version skew.
async fn recv_handshake<Rx: LinkRx>(rx: &mut Rx) -> Result<HandshakeMessage, HandshakeError> {
    let backing = rx
        .recv()
        .await
        .map_err(|error| HandshakeError::Io(std::io::Error::other(error.to_string())))?
        .ok_or(HandshakeError::PeerClosed)?;
    vox_types::dlog!(
        "[handshake] recv raw frame ({} bytes)",
        backing.as_bytes().len()
    );
    let msg = vox_phon::from_self_describing::<HandshakeMessage>(backing.as_bytes())
        .map_err(|e| HandshakeError::Decode(e.to_string()))?;
    vox_types::dlog!("[handshake] recv {:?}", handshake_tag(&msg));
    Ok(msg)
}

fn handshake_tag(msg: &HandshakeMessage) -> &'static str {
    match msg {
        HandshakeMessage::Hello(_) => "Hello",
        HandshakeMessage::HelloYourself(_) => "HelloYourself",
        HandshakeMessage::LetsGo(_) => "LetsGo",
        HandshakeMessage::Decline(_) => "Decline",
        HandshakeMessage::Sorry(_) => "Sorry",
    }
}

// r[impl connection.identity.resolver]
// r[impl connection.policy.establishment]
fn resolve_peer_identity(
    role: ConnectionRole,
    peer_evidence: &PeerEvidence,
    peer_claims: &Metadata,
    identity_resolver: &dyn crate::IdentityResolver,
) -> Result<PeerIdentity, Decline> {
    identity_resolver.resolve(IdentityResolutionContext {
        role,
        evidence: peer_evidence,
        claims: peer_claims,
    })
}

// r[impl connection.handshake]
// r[impl connection.handshake.phon]
// r[impl connection.handshake.lane-settings]
// r[impl connection.handshake.protocol-schema.connection-scoped]
// r[impl connection.handshake.unversioned]
/// Perform the phon handshake as the initiator.
///
/// Three-step exchange:
/// 1. Send Hello
/// 2. Receive HelloYourself (or Decline/Sorry)
/// 3. Send LetsGo (or Decline/Sorry)
pub async fn handshake_as_initiator<Tx: LinkTx, Rx: LinkRx>(
    tx: &Tx,
    rx: &mut Rx,
    settings: ConnectionSettings,
    metadata: Metadata,
) -> Result<HandshakeResult, HandshakeError> {
    let identity_resolver = crate::AnonymousIdentityResolver;
    handshake_as_initiator_with_policy(
        tx,
        rx,
        settings,
        metadata,
        PeerEvidence::none(),
        &identity_resolver,
    )
    .await
}

// r[impl connection.handshake]
// r[impl connection.handshake.metadata]
// r[impl connection.handshake.decline]
// r[impl connection.identity.inputs]
// r[impl connection.identity.resolver]
// r[impl connection.identity]
// r[impl connection.policy.establishment]
// r[impl connection.policy.establishment.rejection]
/// Perform the phon handshake as the initiator with explicit connection policy.
pub async fn handshake_as_initiator_with_policy<Tx: LinkTx, Rx: LinkRx>(
    tx: &Tx,
    rx: &mut Rx,
    settings: ConnectionSettings,
    metadata: Metadata,
    peer_evidence: PeerEvidence,
    identity_resolver: &dyn crate::IdentityResolver,
) -> Result<HandshakeResult, HandshakeError> {
    handshake_as_initiator_pipelined(
        tx,
        rx,
        settings,
        metadata,
        peer_evidence,
        identity_resolver,
        async |_rx| Ok(()),
    )
    .await
}

// r[impl connection.handshake]
// r[impl transport.prologue.first-payload]
/// The initiator handshake, with a hook that runs **after `Hello` is sent and before
/// `HelloYourself` is awaited**.
///
/// That gap is the only place a caller can put work that must follow the Hello onto
/// the wire but must precede reading the reply — which is exactly the shape of
/// `finish_transport`. Letting the builder pass it in here is what turns the transport
/// prologue and the phon handshake from two sequential round trips into one: `VOTH` and
/// `Hello` leave together, `VOTA` and `HelloYourself` come back together.
///
/// The hook takes `&mut Rx` because completing the prologue means reading a frame, and
/// the handshake owns the receiver for the duration.
pub async fn handshake_as_initiator_pipelined<Tx: LinkTx, Rx: LinkRx, F>(
    tx: &Tx,
    rx: &mut Rx,
    settings: ConnectionSettings,
    metadata: Metadata,
    peer_evidence: PeerEvidence,
    identity_resolver: &dyn crate::IdentityResolver,
    after_hello: F,
) -> Result<HandshakeResult, HandshakeError>
where
    F: AsyncFnOnce(&mut Rx) -> Result<(), HandshakeError>,
{
    validate_initial_channel_credit(&settings)?;

    let our_schema = message_schema();

    // The initiator's Hello is the one message that can never be compact: it is
    // written before anything has been heard back, and the transport prologue that
    // precedes it carries no room to ask — `TransportHello::version` is compared for
    // strict equality (`transport_prologue.rs:176`) and its `reserved` bytes must be
    // zero (`:182`), so neither can carry a capability without a flag day. So this
    // sends the full closure, forever, and only *publishes* the ids that let the
    // acceptor's reply be compact.
    let hello = vox_types::Hello {
        parity: settings.parity,
        connection_settings: settings.clone(),
        message_payload_schema: our_schema.clone(),
        metadata,
        compact_handshake_root: Some(handshake_root()),
        compact_message_root: Some(message_root()),
    };

    // Step 1: Send Hello
    send_handshake(tx, &HandshakeMessage::Hello(hello), false).await?;

    // Step 1b: whatever had to follow Hello onto the wire but precede the reply —
    // in practice, collecting the `TransportAccept` whose wait this Hello just
    // overlapped.
    after_hello(rx).await?;

    // Step 2: Receive HelloYourself or Sorry
    let response = recv_handshake(rx).await?;
    let hy = match response {
        HandshakeMessage::HelloYourself(hy) => hy,
        HandshakeMessage::Decline(decline) => return Err(HandshakeError::Declined(decline)),
        HandshakeMessage::Sorry(sorry) => return Err(HandshakeError::Sorry(sorry.reason)),
        _ => {
            return Err(HandshakeError::Protocol(
                "expected HelloYourself, Decline, or Sorry".into(),
            ));
        }
    };
    // Whether our LetsGo may name its schema instead of carrying it. Decided from what
    // the acceptor just published, never assumed.
    let compact_reply = peer_accepts_compact(hy.compact_handshake_root, hy.compact_message_root);
    if hy.connection_settings.initial_channel_credit == 0 {
        let reason = INITIAL_CHANNEL_CREDIT_ZERO_ERROR.to_string();
        send_handshake(
            tx,
            &HandshakeMessage::Sorry(vox_types::Sorry {
                reason: reason.clone(),
            }),
            compact_reply,
        )
        .await?;
        return Err(HandshakeError::Protocol(reason));
    }
    let peer_schema = match resolve_peer_message_schema(
        hy.message_payload_schema,
        hy.compact_message_root,
        &our_schema,
    ) {
        Ok(schema) => schema,
        Err(reason) => {
            send_handshake(
                tx,
                &HandshakeMessage::Sorry(vox_types::Sorry {
                    reason: reason.clone(),
                }),
                compact_reply,
            )
            .await?;
            return Err(HandshakeError::Protocol(reason));
        }
    };

    let peer_identity = match resolve_peer_identity(
        ConnectionRole::Initiator,
        &peer_evidence,
        &hy.metadata,
        identity_resolver,
    ) {
        Ok(identity) => identity,
        Err(decline) => {
            send_handshake(
                tx,
                &HandshakeMessage::Decline(decline.clone()),
                compact_reply,
            )
            .await?;
            return Err(HandshakeError::Declined(decline));
        }
    };

    if let Err(reason) = crate::validate_message_writer_schema(&peer_schema) {
        send_handshake(
            tx,
            &HandshakeMessage::Sorry(vox_types::Sorry {
                reason: reason.clone(),
            }),
            compact_reply,
        )
        .await?;
        return Err(HandshakeError::Protocol(reason));
    }

    // Step 3: Send LetsGo
    send_handshake(
        tx,
        &HandshakeMessage::LetsGo(vox_types::LetsGo {}),
        compact_reply,
    )
    .await?;

    Ok(HandshakeResult {
        role: ConnectionRole::Initiator,
        our_settings: settings,
        peer_settings: hy.connection_settings,
        our_schema,
        peer_schema,
        peer_metadata: hy.metadata,
        peer_evidence,
        peer_identity,
    })
}

// r[impl connection.handshake]
// r[impl connection.handshake.phon]
// r[impl connection.handshake.lane-settings]
// r[impl connection.handshake.protocol-schema.connection-scoped]
// r[impl connection.handshake.unversioned]
/// Perform the phon handshake as the acceptor.
///
/// Three-step exchange:
/// 1. Receive Hello
/// 2. Send HelloYourself (or Decline/Sorry)
/// 3. Receive LetsGo (or Decline/Sorry)
pub async fn handshake_as_acceptor<Tx: LinkTx, Rx: LinkRx>(
    tx: &Tx,
    rx: &mut Rx,
    settings: ConnectionSettings,
    metadata: Metadata,
) -> Result<HandshakeResult, HandshakeError> {
    let identity_resolver = crate::AnonymousIdentityResolver;
    handshake_as_acceptor_with_policy(
        tx,
        rx,
        settings,
        metadata,
        PeerEvidence::none(),
        &identity_resolver,
    )
    .await
}

// r[impl connection.handshake]
// r[impl connection.handshake.metadata]
// r[impl connection.handshake.decline]
// r[impl connection.identity.inputs]
// r[impl connection.identity.resolver]
// r[impl connection.identity]
// r[impl connection.policy.establishment]
// r[impl connection.policy.establishment.rejection]
/// Perform the phon handshake as the acceptor with explicit connection policy.
pub async fn handshake_as_acceptor_with_policy<Tx: LinkTx, Rx: LinkRx>(
    tx: &Tx,
    rx: &mut Rx,
    settings: ConnectionSettings,
    metadata: Metadata,
    peer_evidence: PeerEvidence,
    identity_resolver: &dyn crate::IdentityResolver,
) -> Result<HandshakeResult, HandshakeError> {
    validate_initial_channel_credit(&settings)?;

    // Step 1: Receive Hello
    let hello = match recv_handshake(rx).await? {
        HandshakeMessage::Hello(h) => h,
        _ => return Err(HandshakeError::Protocol("expected Hello".into())),
    };
    // The acceptor decides compaction for the whole rest of the exchange, and it
    // decides it here: after reading the initiator's Hello and before writing a single
    // byte back. That ordering is what makes the change deployable in one release —
    // every reply is conditioned on what the peer published, so an initiator that
    // published nothing keeps getting the form it was built to read.
    let compact_reply =
        peer_accepts_compact(hello.compact_handshake_root, hello.compact_message_root);
    if hello.connection_settings.initial_channel_credit == 0 {
        let reason = INITIAL_CHANNEL_CREDIT_ZERO_ERROR.to_string();
        send_handshake(
            tx,
            &HandshakeMessage::Sorry(vox_types::Sorry {
                reason: reason.clone(),
            }),
            compact_reply,
        )
        .await?;
        return Err(HandshakeError::Protocol(reason));
    }

    let peer_identity = match resolve_peer_identity(
        ConnectionRole::Acceptor,
        &peer_evidence,
        &hello.metadata,
        identity_resolver,
    ) {
        Ok(identity) => identity,
        Err(decline) => {
            send_handshake(
                tx,
                &HandshakeMessage::Decline(decline.clone()),
                compact_reply,
            )
            .await?;
            return Err(HandshakeError::Declined(decline));
        }
    };

    let our_schema = message_schema();

    let peer_schema = match resolve_peer_message_schema(
        hello.message_payload_schema,
        hello.compact_message_root,
        &our_schema,
    ) {
        Ok(schema) => schema,
        Err(reason) => {
            send_handshake(
                tx,
                &HandshakeMessage::Sorry(vox_types::Sorry {
                    reason: reason.clone(),
                }),
                compact_reply,
            )
            .await?;
            return Err(HandshakeError::Protocol(reason));
        }
    };

    if let Err(reason) = crate::validate_message_writer_schema(&peer_schema) {
        send_handshake(
            tx,
            &HandshakeMessage::Sorry(vox_types::Sorry {
                reason: reason.clone(),
            }),
            compact_reply,
        )
        .await?;
        return Err(HandshakeError::Protocol(reason));
    }

    // Acceptor adopts opposite parity
    let our_settings = ConnectionSettings {
        parity: hello.parity.other(),
        ..settings
    };

    // Step 2: Send HelloYourself. When the initiator published our ids, this drops
    // 14,193 bytes to a couple of hundred: the 4,215-byte `HandshakeMessage` closure
    // becomes an 8-byte root id, and the 9,957-byte `Message` closure is left out
    // entirely in favour of `compact_message_root`.
    let hy = vox_types::HelloYourself {
        connection_settings: our_settings.clone(),
        message_payload_schema: if compact_reply {
            Vec::new()
        } else {
            our_schema.clone()
        },
        metadata,
        compact_handshake_root: Some(handshake_root()),
        compact_message_root: Some(message_root()),
    };
    send_handshake(tx, &HandshakeMessage::HelloYourself(hy), compact_reply).await?;

    // Step 3: Receive LetsGo or Sorry
    let response = recv_handshake(rx).await?;
    match response {
        HandshakeMessage::LetsGo(_) => {}
        HandshakeMessage::Decline(decline) => return Err(HandshakeError::Declined(decline)),
        HandshakeMessage::Sorry(sorry) => return Err(HandshakeError::Sorry(sorry.reason)),
        _ => {
            return Err(HandshakeError::Protocol(
                "expected LetsGo, Decline, or Sorry".into(),
            ));
        }
    }

    Ok(HandshakeResult {
        role: ConnectionRole::Acceptor,
        our_settings,
        peer_settings: hello.connection_settings,
        our_schema,
        peer_schema,
        peer_metadata: hello.metadata,
        peer_evidence,
        peer_identity,
    })
}

#[cfg(test)]
mod tests {
    use vox_types::{
        EstablishmentRejectReason, IdentityBasis, IdentityBasisProvenance, Link, Parity,
        PeerEvidence, PeerEvidenceItem, PeerIdentity, PeerIdentityForm,
    };

    use super::*;

    fn settings(parity: Parity, initial_channel_credit: u32) -> ConnectionSettings {
        ConnectionSettings {
            parity,
            max_concurrent_requests: 64,
            initial_channel_credit,
        }
    }

    fn settings_with_request_limit(
        parity: Parity,
        max_concurrent_requests: u32,
        initial_channel_credit: u32,
    ) -> ConnectionSettings {
        ConnectionSettings {
            parity,
            max_concurrent_requests,
            initial_channel_credit,
        }
    }

    // r[verify connection.handshake]
    // r[verify connection.handshake.phon]
    // r[verify connection.handshake.protocol-schema]
    // r[verify connection.handshake.lane-settings]
    // r[verify connection.peer]
    // r[verify rpc.metadata.records]
    #[tokio::test]
    async fn hello_and_hello_yourself_carry_connection_settings() {
        let (client_link, server_link) = crate::memory_link_pair(4);
        let (client_tx, mut client_rx) = client_link.split();
        let (server_tx, mut server_rx) = server_link.split();

        let initiator_settings = settings_with_request_limit(Parity::Odd, 37, 23);
        let initiator_expected = initiator_settings.clone();
        let acceptor_settings = settings_with_request_limit(Parity::Even, 41, 29);
        let acceptor_expected = acceptor_settings.clone();
        let acceptor_metadata = vox_types::metadata()
            .str("vox-service", "AcceptorService")
            .build();
        let acceptor_schema = message_schema();

        let initiator = tokio::spawn(async move {
            handshake_as_initiator(
                &client_tx,
                &mut client_rx,
                initiator_settings,
                vox_types::Metadata::default(),
            )
            .await
        });

        let hello = recv_handshake(&mut server_rx).await.expect("recv hello");
        let HandshakeMessage::Hello(hello) = hello else {
            panic!("expected Hello");
        };
        assert_eq!(hello.connection_settings, initiator_expected);

        send_handshake(
            &server_tx,
            &HandshakeMessage::HelloYourself(vox_types::HelloYourself {
                connection_settings: acceptor_settings,
                message_payload_schema: acceptor_schema.clone(),
                metadata: acceptor_metadata.clone(),
                compact_handshake_root: None,
                compact_message_root: None,
            }),
            false,
        )
        .await
        .expect("send hello-yourself");

        let lets_go = recv_handshake(&mut server_rx).await.expect("recv lets-go");
        assert!(matches!(lets_go, HandshakeMessage::LetsGo(_)));

        let result = initiator
            .await
            .expect("initiator task")
            .expect("initiator handshake");
        assert_eq!(result.our_settings, initiator_expected);
        assert_eq!(result.peer_settings, acceptor_expected);
        assert_eq!(result.peer_schema, acceptor_schema);
        assert_eq!(result.peer_metadata, acceptor_metadata);
        assert!(result.peer_evidence.is_empty());
        assert!(result.peer_identity.is_anonymous());
    }

    // r[verify connection.handshake.metadata]
    // r[verify connection.evidence]
    // r[verify connection.identity]
    // r[verify connection.identity.forms]
    // r[verify connection.identity.inputs]
    // r[verify connection.identity.local]
    // r[verify connection.identity.redaction]
    // r[verify connection.identity.scope]
    // r[verify connection.identity.use-cases]
    // r[verify connection.policy.establishment]
    #[tokio::test]
    async fn identity_resolver_builds_identity_from_local_evidence_and_verified_claims() {
        let (client_link, server_link) = crate::memory_link_pair(4);
        let (client_tx, mut client_rx) = client_link.split();
        let (server_tx, mut server_rx) = server_link.split();

        let peer_evidence = unsafe {
            PeerEvidence::from_runtime_asserted(vec![PeerEvidenceItem::Tls {
                verified_subject: Some("CN=server".into()),
                alpn: Some("vox".into()),
            }])
        };
        let resolver = crate::identity_resolver_fn(|context| {
            assert_eq!(context.role, ConnectionRole::Initiator);
            assert_eq!(
                vox_types::metadata_get_str(context.claims, "server-auth"),
                Some("token-ok")
            );
            let [
                PeerEvidenceItem::Tls {
                    verified_subject,
                    alpn,
                },
            ] = context.evidence.items()
            else {
                panic!("expected TLS evidence, got {:?}", context.evidence.items());
            };
            assert_eq!(verified_subject.as_deref(), Some("CN=server"));
            assert_eq!(alpn.as_deref(), Some("vox"));

            Ok(PeerIdentity::composite(vec![
                IdentityBasis::new(
                    PeerIdentityForm::CertificateBacked,
                    IdentityBasisProvenance::EvidenceBacked,
                    "tls:server",
                ),
                IdentityBasis::new(
                    PeerIdentityForm::ApplicationUser,
                    IdentityBasisProvenance::VerifiedClaimBacked,
                    "user:7",
                ),
            ]))
        });

        let initiator = tokio::spawn(async move {
            handshake_as_initiator_with_policy(
                &client_tx,
                &mut client_rx,
                settings(Parity::Odd, 16),
                vox_types::Metadata::default(),
                peer_evidence,
                &resolver,
            )
            .await
        });

        let hello = recv_handshake(&mut server_rx).await.expect("recv hello");
        assert!(matches!(hello, HandshakeMessage::Hello(_)));

        let peer_metadata = vox_types::metadata()
            .str("server-auth", "token-ok")
            .str("traceparent", "redacted-by-policy")
            .build();
        send_handshake(
            &server_tx,
            &HandshakeMessage::HelloYourself(vox_types::HelloYourself {
                connection_settings: settings(Parity::Even, 16),
                message_payload_schema: message_schema(),
                metadata: peer_metadata.clone(),
                compact_handshake_root: None,
                compact_message_root: None,
            }),
            false,
        )
        .await
        .expect("send hello-yourself");

        let lets_go = recv_handshake(&mut server_rx).await.expect("recv lets-go");
        assert!(matches!(lets_go, HandshakeMessage::LetsGo(_)));

        let result = initiator
            .await
            .expect("initiator task")
            .expect("initiator handshake");
        assert_eq!(result.peer_metadata, peer_metadata);
        assert_eq!(result.peer_evidence.items().len(), 1);
        assert_eq!(result.peer_identity.form(), PeerIdentityForm::Composite);
        assert_eq!(result.peer_identity.bases().len(), 2);
        assert_eq!(
            result.peer_identity.bases()[0].provenance,
            IdentityBasisProvenance::EvidenceBacked
        );
        assert_eq!(result.peer_identity.bases()[0].redacted, "tls:server");
        assert_eq!(
            result.peer_identity.bases()[1].provenance,
            IdentityBasisProvenance::VerifiedClaimBacked
        );
        assert_eq!(result.peer_identity.bases()[1].redacted, "user:7");
    }

    // r[verify connection.handshake.decline]
    // r[verify connection.policy.establishment.rejection]
    // r[verify connection.identity.resolver]
    // r[verify rejection.reason.taxonomy]
    #[tokio::test]
    async fn acceptor_declines_when_identity_resolver_rejects_initiator_claims() {
        let (client_link, server_link) = crate::memory_link_pair(4);
        let (client_tx, mut client_rx) = client_link.split();
        let (server_tx, mut server_rx) = server_link.split();

        let acceptor = tokio::spawn(async move {
            let resolver = crate::identity_resolver_fn(|context| {
                assert_eq!(context.role, ConnectionRole::Acceptor);
                assert_eq!(
                    vox_types::metadata_get_str(context.claims, "auth"),
                    Some("nope")
                );
                Err(Decline::new(EstablishmentRejectReason::Forbidden))
            });
            handshake_as_acceptor_with_policy(
                &server_tx,
                &mut server_rx,
                settings(Parity::Even, 16),
                vox_types::Metadata::default(),
                unsafe { PeerEvidence::synthetic("memory-link") },
                &resolver,
            )
            .await
        });

        send_handshake(
            &client_tx,
            &HandshakeMessage::Hello(vox_types::Hello {
                parity: Parity::Odd,
                connection_settings: settings(Parity::Odd, 16),
                message_payload_schema: message_schema(),
                metadata: vox_types::metadata().str("auth", "nope").build(),
                compact_handshake_root: None,
                compact_message_root: None,
            }),
            false,
        )
        .await
        .expect("send hello");

        let response = recv_handshake(&mut client_rx).await.expect("recv decline");
        assert!(
            matches!(
                response,
                HandshakeMessage::Decline(vox_types::Decline {
                    reason: EstablishmentRejectReason::Forbidden,
                    ..
                })
            ),
            "expected Decline::Forbidden, got: {response:?}"
        );

        let result = acceptor.await.expect("acceptor task");
        assert!(
            matches!(
                result,
                Err(HandshakeError::Declined(vox_types::Decline {
                    reason: EstablishmentRejectReason::Forbidden,
                    ..
                }))
            ),
            "expected acceptor Declined::Forbidden, got: {result:?}"
        );
    }

    // r[verify connection.handshake.decline]
    // r[verify connection.policy.establishment.rejection]
    // r[verify connection.identity.resolver]
    #[tokio::test]
    async fn initiator_declines_when_identity_resolver_rejects_acceptor_claims() {
        let (client_link, server_link) = crate::memory_link_pair(4);
        let (client_tx, mut client_rx) = client_link.split();
        let (server_tx, mut server_rx) = server_link.split();

        let initiator = tokio::spawn(async move {
            let resolver = crate::identity_resolver_fn(|context| {
                assert_eq!(context.role, ConnectionRole::Initiator);
                assert_eq!(
                    vox_types::metadata_get_str(context.claims, "server-auth"),
                    Some("bad")
                );
                Err(Decline::new(EstablishmentRejectReason::Forbidden))
            });
            handshake_as_initiator_with_policy(
                &client_tx,
                &mut client_rx,
                settings(Parity::Odd, 16),
                vox_types::Metadata::default(),
                unsafe { PeerEvidence::synthetic("memory-link") },
                &resolver,
            )
            .await
        });

        let hello = recv_handshake(&mut server_rx).await.expect("recv hello");
        assert!(matches!(hello, HandshakeMessage::Hello(_)));

        send_handshake(
            &server_tx,
            &HandshakeMessage::HelloYourself(vox_types::HelloYourself {
                connection_settings: settings(Parity::Even, 16),
                message_payload_schema: message_schema(),
                metadata: vox_types::metadata().str("server-auth", "bad").build(),
                compact_handshake_root: None,
                compact_message_root: None,
            }),
            false,
        )
        .await
        .expect("send hello-yourself");

        let response = recv_handshake(&mut server_rx).await.expect("recv decline");
        assert!(
            matches!(
                response,
                HandshakeMessage::Decline(vox_types::Decline {
                    reason: EstablishmentRejectReason::Forbidden,
                    ..
                })
            ),
            "expected Decline::Forbidden, got: {response:?}"
        );

        let result = initiator.await.expect("initiator task");
        assert!(
            matches!(
                result,
                Err(HandshakeError::Declined(vox_types::Decline {
                    reason: EstablishmentRejectReason::Forbidden,
                    ..
                }))
            ),
            "expected initiator Declined::Forbidden, got: {result:?}"
        );
    }

    // r[verify connection.handshake.sorry]
    // r[verify connection.handshake.unversioned]
    // r[verify connection.handshake.protocol-schema.connection-scoped]
    #[tokio::test]
    async fn acceptor_rejects_incompatible_peer_message_schema_with_sorry() {
        let (client_link, server_link) = crate::memory_link_pair(4);
        let (client_tx, mut client_rx) = client_link.split();
        let (server_tx, mut server_rx) = server_link.split();

        let acceptor = tokio::spawn(async move {
            handshake_as_acceptor(
                &server_tx,
                &mut server_rx,
                settings(Parity::Even, 16),
                vox_types::Metadata::default(),
            )
            .await
        });

        let incompatible_schema = vox_phon::schema_bytes::<u32>().expect("u32 schema");
        send_handshake(
            &client_tx,
            &HandshakeMessage::Hello(vox_types::Hello {
                parity: Parity::Odd,
                connection_settings: settings(Parity::Odd, 16),
                message_payload_schema: incompatible_schema,
                metadata: vox_types::Metadata::default(),
                compact_handshake_root: None,
                compact_message_root: None,
            }),
            false,
        )
        .await
        .expect("send hello");

        let response = recv_handshake(&mut client_rx).await.expect("recv sorry");
        assert!(
            matches!(
                response,
                HandshakeMessage::Sorry(vox_types::Sorry { ref reason })
                    if reason.contains("peer Message schema is incompatible")
            ),
            "expected Sorry for incompatible peer schema, got: {response:?}"
        );

        let result = acceptor.await.expect("acceptor task");
        assert!(
            matches!(result, Err(HandshakeError::Protocol(ref reason)) if reason.contains("peer Message schema is incompatible")),
            "expected acceptor protocol error for incompatible peer schema, got: {result:?}"
        );
    }

    // r[verify connection.handshake.protocol-schema.connection-scoped]
    // r[verify connection.handshake.unversioned]
    /// The old-peer cell of the wire table: an initiator that publishes no root ids
    /// is a peer built before they existed, and it must keep receiving the closure it
    /// was built to read.
    ///
    /// This is the cell that cannot be recovered from in production. A `HelloYourself`
    /// compacted at an initiator that cannot expand it is not a degraded connection,
    /// it is a connection that never establishes — so the assertion is on the frame
    /// SIZE, which is the thing an old peer actually experiences, not on a flag.
    #[tokio::test]
    async fn acceptor_sends_the_full_closure_to_an_initiator_that_published_no_roots() {
        let (client_link, server_link) = crate::memory_link_pair(4);
        let (client_tx, mut client_rx) = client_link.split();
        let (server_tx, mut server_rx) = server_link.split();

        let acceptor = tokio::spawn(async move {
            handshake_as_acceptor(
                &server_tx,
                &mut server_rx,
                settings(Parity::Even, 16),
                vox_types::Metadata::default(),
            )
            .await
        });

        send_handshake(
            &client_tx,
            &HandshakeMessage::Hello(vox_types::Hello {
                parity: Parity::Odd,
                connection_settings: settings(Parity::Odd, 16),
                message_payload_schema: message_schema(),
                metadata: vox_types::Metadata::default(),
                // Exactly what a peer from before this change writes.
                compact_handshake_root: None,
                compact_message_root: None,
            }),
            false,
        )
        .await
        .expect("send hello");

        let frame = client_rx
            .recv()
            .await
            .expect("recv hello-yourself")
            .expect("hello-yourself frame");
        let bytes = frame.as_bytes();
        assert_ne!(
            u32::from_le_bytes(bytes[..4].try_into().unwrap()),
            vox_phon::COMPACT_SCHEMA_REF,
            "an initiator that published no roots must not be sent a compact envelope"
        );
        assert!(
            bytes.len() > 10_000,
            "expected the full closure ({} bytes is too small to carry one)",
            bytes.len()
        );
        let hy = vox_phon::from_self_describing::<HandshakeMessage>(bytes).expect("decode");
        let HandshakeMessage::HelloYourself(hy) = hy else {
            panic!("expected HelloYourself");
        };
        assert!(
            !hy.message_payload_schema.is_empty(),
            "the Message closure must be carried, not named, for a peer that cannot resolve a name"
        );

        send_handshake(
            &client_tx,
            &HandshakeMessage::LetsGo(vox_types::LetsGo {}),
            false,
        )
        .await
        .expect("send lets-go");
        let result = acceptor.await.expect("acceptor task").expect("handshake");
        assert!(
            !result.peer_schema.is_empty(),
            "the acceptor still ends up holding the initiator's Message schema"
        );
    }

    // r[verify connection.handshake.protocol-schema.connection-scoped]
    /// The new/new cell: when the initiator publishes ids the acceptor recognises as
    /// its own, the reply names the schemas instead of carrying them — and the
    /// handshake still produces the identical `peer_schema` it would have parsed off
    /// the wire, which is what makes this invisible to everything downstream.
    #[tokio::test]
    async fn acceptor_names_the_schemas_for_an_initiator_that_published_matching_roots() {
        let (client_link, server_link) = crate::memory_link_pair(4);
        let (client_tx, mut client_rx) = client_link.split();
        let (server_tx, mut server_rx) = server_link.split();

        let acceptor = tokio::spawn(async move {
            handshake_as_acceptor(
                &server_tx,
                &mut server_rx,
                settings(Parity::Even, 16),
                vox_types::Metadata::default(),
            )
            .await
        });

        send_handshake(
            &client_tx,
            &HandshakeMessage::Hello(vox_types::Hello {
                parity: Parity::Odd,
                connection_settings: settings(Parity::Odd, 16),
                message_payload_schema: message_schema(),
                metadata: vox_types::Metadata::default(),
                compact_handshake_root: Some(handshake_root()),
                compact_message_root: Some(message_root()),
            }),
            false,
        )
        .await
        .expect("send hello");

        let frame = client_rx
            .recv()
            .await
            .expect("recv hello-yourself")
            .expect("hello-yourself frame");
        let bytes = frame.as_bytes();
        assert_eq!(
            u32::from_le_bytes(bytes[..4].try_into().unwrap()),
            vox_phon::COMPACT_SCHEMA_REF,
            "expected a compact envelope"
        );
        assert!(
            bytes.len() < 1_200,
            "the point of the exercise is one datagram; got {} bytes",
            bytes.len()
        );

        let hy = vox_phon::from_self_describing::<HandshakeMessage>(bytes).expect("decode");
        let HandshakeMessage::HelloYourself(hy) = hy else {
            panic!("expected HelloYourself");
        };
        assert!(
            hy.message_payload_schema.is_empty(),
            "closure must be named, not sent"
        );
        assert_eq!(
            resolve_peer_message_schema(
                hy.message_payload_schema,
                hy.compact_message_root,
                &message_schema()
            )
            .expect("resolve named schema"),
            message_schema(),
            "a named schema must resolve to exactly the bytes a full peer would have sent"
        );

        send_handshake(
            &client_tx,
            &HandshakeMessage::LetsGo(vox_types::LetsGo {}),
            true,
        )
        .await
        .expect("send compact lets-go");
        acceptor
            .await
            .expect("acceptor task")
            .expect("acceptor accepts a compact LetsGo");
    }

    // r[verify connection.handshake.protocol-schema.connection-scoped]
    /// A root id the reader does not recognise is refused, not guessed at. This is the
    /// safety net under the whole scheme: compaction is only ever sound because equal
    /// content-derived ids imply equal closures, so an unequal id must never resolve.
    #[test]
    fn a_named_schema_that_is_not_ours_is_refused() {
        let error = resolve_peer_message_schema(Vec::new(), Some(message_root() ^ 1), &[])
            .expect_err("a foreign root must not resolve");
        assert!(error.contains("not ours"), "unhelpful error: {error}");

        let error = resolve_peer_message_schema(Vec::new(), None, &[])
            .expect_err("an empty schema with no root must not resolve");
        assert!(error.contains("named no root"), "unhelpful error: {error}");
    }

    // r[verify rpc.flow-control.credit.initial.zero]
    #[tokio::test]
    async fn initiator_rejects_local_zero_initial_credit_before_handshake() {
        let (link, _peer) = crate::memory_link_pair(1);
        let (tx, mut rx) = link.split();

        let result = handshake_as_initiator(
            &tx,
            &mut rx,
            settings(Parity::Odd, 0),
            vox_types::Metadata::default(),
        )
        .await;

        assert!(
            matches!(
                result,
                Err(HandshakeError::Protocol(ref message))
                    if message == INITIAL_CHANNEL_CREDIT_ZERO_ERROR
            ),
            "expected zero-credit protocol error, got: {result:?}"
        );
    }

    // r[verify rpc.flow-control.credit.initial.zero]
    #[tokio::test]
    async fn acceptor_rejects_peer_zero_initial_credit_before_connection_starts() {
        let (client_link, server_link) = crate::memory_link_pair(4);
        let (client_tx, mut client_rx) = client_link.split();
        let (server_tx, mut server_rx) = server_link.split();

        let acceptor = tokio::spawn(async move {
            handshake_as_acceptor(
                &server_tx,
                &mut server_rx,
                settings(Parity::Even, 16),
                vox_types::Metadata::default(),
            )
            .await
        });

        send_handshake(
            &client_tx,
            &HandshakeMessage::Hello(vox_types::Hello {
                parity: Parity::Odd,
                connection_settings: settings(Parity::Odd, 0),
                message_payload_schema: message_schema(),
                metadata: vox_types::Metadata::default(),
                compact_handshake_root: None,
                compact_message_root: None,
            }),
            false,
        )
        .await
        .expect("send hello");

        let response = recv_handshake(&mut client_rx).await.expect("recv sorry");
        assert!(
            matches!(
                response,
                HandshakeMessage::Sorry(vox_types::Sorry { ref reason })
                    if reason == INITIAL_CHANNEL_CREDIT_ZERO_ERROR
            ),
            "expected Sorry for zero credit, got: {response:?}"
        );

        let result = acceptor.await.expect("acceptor task");
        assert!(
            matches!(
                result,
                Err(HandshakeError::Protocol(ref message))
                    if message == INITIAL_CHANNEL_CREDIT_ZERO_ERROR
            ),
            "expected zero-credit protocol error, got: {result:?}"
        );
    }
}
