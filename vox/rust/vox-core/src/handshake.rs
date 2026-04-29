use vox_types::{
    ConnectionSettings, HandshakeMessage, HandshakeResult, LinkRx, LinkTx, ResumeKeyBytes, Schema,
    SessionResumeKey, SessionRole,
};

const INITIAL_CHANNEL_CREDIT_ZERO_ERROR: &str = "initial_channel_credit must be greater than zero";

#[derive(Debug)]
pub enum HandshakeError {
    Io(std::io::Error),
    Encode(String),
    Decode(String),
    PeerClosed,
    Protocol(String),
    Sorry(String),
    NotResumable,
}

impl std::fmt::Display for HandshakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "handshake io error: {e}"),
            Self::Encode(e) => write!(f, "handshake encode error: {e}"),
            Self::Decode(e) => write!(f, "handshake decode error: {e}"),
            Self::PeerClosed => write!(f, "peer closed during handshake"),
            Self::Protocol(msg) => write!(f, "handshake protocol error: {msg}"),
            Self::Sorry(reason) => write!(f, "handshake rejected: {reason}"),
            Self::NotResumable => write!(f, "session is not resumable"),
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

/// Extract the Message schema from the static shape.
fn message_schema() -> Vec<Schema> {
    vox_types::extract_schemas(<vox_types::Message<'static> as facet::Facet<'static>>::SHAPE)
        .expect("schema extraction")
        .schemas
        .clone()
}

/// Send a CBOR-encoded handshake message on a raw link.
async fn send_handshake<Tx: LinkTx>(tx: &Tx, msg: &HandshakeMessage) -> Result<(), HandshakeError> {
    let bytes = facet_cbor::to_vec(msg).map_err(|e| HandshakeError::Encode(e.to_string()))?;
    vox_types::dlog!(
        "[handshake] send {:?} ({} bytes)",
        handshake_tag(msg),
        bytes.len()
    );
    tx.send(bytes).await.map_err(HandshakeError::Io)
}

/// Receive and decode a CBOR handshake message from a raw link.
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
    let msg = facet_cbor::from_slice(backing.as_bytes())
        .map_err(|e| HandshakeError::Decode(e.to_string()))?;
    vox_types::dlog!("[handshake] recv {:?}", handshake_tag(&msg));
    Ok(msg)
}

fn handshake_tag(msg: &HandshakeMessage) -> &'static str {
    match msg {
        HandshakeMessage::Hello(_) => "Hello",
        HandshakeMessage::HelloYourself(_) => "HelloYourself",
        HandshakeMessage::LetsGo(_) => "LetsGo",
        HandshakeMessage::Sorry(_) => "Sorry",
    }
}

// r[impl session.handshake]
// r[impl session.handshake.cbor]
/// Perform the CBOR handshake as the initiator.
///
/// Three-step exchange:
/// 1. Send Hello
/// 2. Receive HelloYourself (or Sorry)
/// 3. Send LetsGo (or Sorry)
pub async fn handshake_as_initiator<Tx: LinkTx, Rx: LinkRx>(
    tx: &Tx,
    rx: &mut Rx,
    settings: ConnectionSettings,
    supports_retry: bool,
    resume_key: Option<&SessionResumeKey>,
    metadata: vox_types::Metadata<'static>,
) -> Result<HandshakeResult, HandshakeError> {
    validate_initial_channel_credit(&settings)?;

    let our_schema = message_schema();

    let hello = vox_types::Hello {
        parity: settings.parity,
        connection_settings: settings.clone(),
        message_payload_schema: our_schema.clone(),
        supports_retry,
        resume_key: resume_key.map(ResumeKeyBytes::from_key),
        metadata,
    };

    // Step 1: Send Hello
    send_handshake(tx, &HandshakeMessage::Hello(hello)).await?;

    // Step 2: Receive HelloYourself or Sorry
    let response = recv_handshake(rx).await?;
    let hy = match response {
        HandshakeMessage::HelloYourself(hy) => hy,
        HandshakeMessage::Sorry(sorry) => return Err(HandshakeError::Sorry(sorry.reason)),
        _ => {
            return Err(HandshakeError::Protocol(
                "expected HelloYourself or Sorry".into(),
            ));
        }
    };
    if hy.connection_settings.initial_channel_credit == 0 {
        let reason = INITIAL_CHANNEL_CREDIT_ZERO_ERROR.to_string();
        send_handshake(
            tx,
            &HandshakeMessage::Sorry(vox_types::Sorry {
                reason: reason.clone(),
            }),
        )
        .await?;
        return Err(HandshakeError::Protocol(reason));
    }

    // Step 3: Send LetsGo
    // TODO: Compare schemas and send Sorry if incompatible
    send_handshake(tx, &HandshakeMessage::LetsGo(vox_types::LetsGo {})).await?;

    let session_resume_key = hy.resume_key.as_ref().and_then(|k| k.to_key());

    Ok(HandshakeResult {
        role: SessionRole::Initiator,
        our_settings: settings,
        peer_settings: hy.connection_settings,
        peer_supports_retry: hy.supports_retry,
        session_resume_key,
        peer_resume_key: None, // initiator doesn't receive a peer resume key
        our_schema,
        peer_schema: hy.message_payload_schema,
        peer_metadata: hy.metadata,
    })
}

// r[impl session.handshake]
// r[impl session.handshake.cbor]
/// Perform the CBOR handshake as the acceptor.
///
/// Three-step exchange:
/// 1. Receive Hello
/// 2. Send HelloYourself (or Sorry)
/// 3. Receive LetsGo (or Sorry)
pub async fn handshake_as_acceptor<Tx: LinkTx, Rx: LinkRx>(
    tx: &Tx,
    rx: &mut Rx,
    settings: ConnectionSettings,
    supports_retry: bool,
    resumable: bool,
    expected_resume_key: Option<&SessionResumeKey>,
    metadata: vox_types::Metadata<'static>,
) -> Result<HandshakeResult, HandshakeError> {
    validate_initial_channel_credit(&settings)?;

    // Step 1: Receive Hello
    let hello = match recv_handshake(rx).await? {
        HandshakeMessage::Hello(h) => h,
        _ => return Err(HandshakeError::Protocol("expected Hello".into())),
    };
    if hello.connection_settings.initial_channel_credit == 0 {
        let reason = INITIAL_CHANNEL_CREDIT_ZERO_ERROR.to_string();
        send_handshake(
            tx,
            &HandshakeMessage::Sorry(vox_types::Sorry {
                reason: reason.clone(),
            }),
        )
        .await?;
        return Err(HandshakeError::Protocol(reason));
    }

    // Validate resume key if this is a resumption attempt
    if let Some(expected) = expected_resume_key {
        let actual = hello.resume_key.as_ref().and_then(|k| k.to_key());
        match actual {
            Some(actual) if actual == *expected => {} // OK
            _ => {
                let reason = "session resume key mismatch".to_string();
                send_handshake(
                    tx,
                    &HandshakeMessage::Sorry(vox_types::Sorry {
                        reason: reason.clone(),
                    }),
                )
                .await?;
                return Err(HandshakeError::Protocol(reason));
            }
        }
    }

    // Acceptor adopts opposite parity
    let our_settings = ConnectionSettings {
        parity: hello.parity.other(),
        ..settings
    };

    // Generate resume key if we're resumable
    let our_resume_key = if resumable {
        Some(fresh_resume_key()?)
    } else {
        None
    };

    let our_schema = message_schema();

    // Step 2: Send HelloYourself
    let hy = vox_types::HelloYourself {
        connection_settings: our_settings.clone(),
        message_payload_schema: our_schema.clone(),
        supports_retry,
        resume_key: our_resume_key.as_ref().map(ResumeKeyBytes::from_key),
        metadata,
    };
    send_handshake(tx, &HandshakeMessage::HelloYourself(hy)).await?;

    // Step 3: Receive LetsGo or Sorry
    let response = recv_handshake(rx).await?;
    match response {
        HandshakeMessage::LetsGo(_) => {}
        HandshakeMessage::Sorry(sorry) => return Err(HandshakeError::Sorry(sorry.reason)),
        _ => return Err(HandshakeError::Protocol("expected LetsGo or Sorry".into())),
    }

    let peer_resume_key = hello.resume_key.as_ref().and_then(|k| k.to_key());

    Ok(HandshakeResult {
        role: SessionRole::Acceptor,
        our_settings,
        peer_settings: hello.connection_settings,
        peer_supports_retry: hello.supports_retry,
        session_resume_key: our_resume_key,
        peer_resume_key,
        our_schema,
        peer_schema: hello.message_payload_schema,
        peer_metadata: hello.metadata,
    })
}

fn fresh_resume_key() -> Result<SessionResumeKey, HandshakeError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| {
        HandshakeError::Protocol(format!("failed to generate session key: {error}"))
    })?;
    Ok(SessionResumeKey(bytes))
}

#[cfg(test)]
mod tests {
    use vox_types::{Link, Parity};

    use super::*;

    fn settings(parity: Parity, initial_channel_credit: u32) -> ConnectionSettings {
        ConnectionSettings {
            parity,
            max_concurrent_requests: 64,
            initial_channel_credit,
        }
    }

    // r[verify rpc.flow-control.credit.initial.zero]
    #[tokio::test]
    async fn initiator_rejects_local_zero_initial_credit_before_handshake() {
        let (link, _peer) = crate::memory_link_pair(1);
        let (tx, mut rx) = link.split();

        let result =
            handshake_as_initiator(&tx, &mut rx, settings(Parity::Odd, 0), true, None, vec![])
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
    async fn acceptor_rejects_peer_zero_initial_credit_before_session_starts() {
        let (client_link, server_link) = crate::memory_link_pair(4);
        let (client_tx, mut client_rx) = client_link.split();
        let (server_tx, mut server_rx) = server_link.split();

        let acceptor = tokio::spawn(async move {
            handshake_as_acceptor(
                &server_tx,
                &mut server_rx,
                settings(Parity::Even, 16),
                true,
                false,
                None,
                vec![],
            )
            .await
        });

        send_handshake(
            &client_tx,
            &HandshakeMessage::Hello(vox_types::Hello {
                parity: Parity::Odd,
                connection_settings: settings(Parity::Odd, 0),
                message_payload_schema: message_schema(),
                supports_retry: true,
                resume_key: None,
                metadata: vec![],
            }),
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
