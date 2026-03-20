use roam_types::{
    ConnectionSettings, HandshakeMessage, HandshakeResult, LinkRx, LinkTx, LinkTxPermit,
    ResumeKeyBytes, Schema, SessionResumeKey, SessionRole, WriteSlot,
};

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

/// Extract the Message schema from the static shape.
fn message_schema() -> Vec<Schema> {
    roam_types::extract_schemas(<roam_types::Message<'static> as facet::Facet<'static>>::SHAPE)
        .expect("schema extraction")
        .schemas
}

/// Send a CBOR-encoded handshake message on a raw link.
async fn send_handshake<Tx: LinkTx>(tx: &Tx, msg: &HandshakeMessage) -> Result<(), HandshakeError> {
    let bytes = facet_cbor::to_vec(msg).map_err(|e| HandshakeError::Encode(e.to_string()))?;
    let permit = tx.reserve().await.map_err(HandshakeError::Io)?;
    let mut slot = permit.alloc(bytes.len()).map_err(HandshakeError::Io)?;
    slot.as_mut_slice().copy_from_slice(&bytes);
    slot.commit();
    Ok(())
}

/// Receive and decode a CBOR handshake message from a raw link.
async fn recv_handshake<Rx: LinkRx>(rx: &mut Rx) -> Result<HandshakeMessage, HandshakeError> {
    let backing = rx
        .recv()
        .await
        .map_err(|error| HandshakeError::Io(std::io::Error::other(error.to_string())))?
        .ok_or(HandshakeError::PeerClosed)?;
    facet_cbor::from_slice(backing.as_bytes()).map_err(|e| HandshakeError::Decode(e.to_string()))
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
) -> Result<HandshakeResult, HandshakeError> {
    let our_schema = message_schema();

    let hello = roam_types::Hello {
        parity: settings.parity,
        connection_settings: settings.clone(),
        message_payload_schema: our_schema.clone(),
        supports_retry,
        resume_key: resume_key.map(ResumeKeyBytes::from_key),
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

    // Step 3: Send LetsGo
    // TODO: Compare schemas and send Sorry if incompatible
    send_handshake(tx, &HandshakeMessage::LetsGo(roam_types::LetsGo {})).await?;

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
) -> Result<HandshakeResult, HandshakeError> {
    // Step 1: Receive Hello
    let hello = match recv_handshake(rx).await? {
        HandshakeMessage::Hello(h) => h,
        _ => return Err(HandshakeError::Protocol("expected Hello".into())),
    };

    // Validate resume key if this is a resumption attempt
    if let Some(expected) = expected_resume_key {
        let actual = hello.resume_key.as_ref().and_then(|k| k.to_key());
        match actual {
            Some(actual) if actual == *expected => {} // OK
            _ => {
                let reason = "session resume key mismatch".to_string();
                send_handshake(
                    tx,
                    &HandshakeMessage::Sorry(roam_types::Sorry {
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
    let hy = roam_types::HelloYourself {
        connection_settings: our_settings.clone(),
        message_payload_schema: our_schema.clone(),
        supports_retry,
        resume_key: our_resume_key.as_ref().map(ResumeKeyBytes::from_key),
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
    })
}

fn fresh_resume_key() -> Result<SessionResumeKey, HandshakeError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| {
        HandshakeError::Protocol(format!("failed to generate session key: {error}"))
    })?;
    Ok(SessionResumeKey(bytes))
}
