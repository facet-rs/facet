use vox_types::{
    ConnectionSettings, HandshakeMessage, HandshakeResult, LinkRx, LinkTx, SessionRole,
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
/// `Message` (`r[session.handshake.protocol-schema]`).
fn message_schema() -> Vec<u8> {
    vox_phon::schema_bytes::<vox_types::Message<'static>>()
        .expect("derive phon schema for Message envelope")
}

/// Send a handshake message on a raw link, self-describing (it carries its own phon
/// schema closure, so the peer can decode it without a prior exchange).
async fn send_handshake<Tx: LinkTx>(tx: &Tx, msg: &HandshakeMessage) -> Result<(), HandshakeError> {
    let bytes =
        vox_phon::to_self_describing(msg).map_err(|e| HandshakeError::Encode(e.to_string()))?;
    vox_types::dlog!(
        "[handshake] send {:?} ({} bytes)",
        handshake_tag(msg),
        bytes.len()
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
        HandshakeMessage::Sorry(_) => "Sorry",
    }
}

// r[impl session.handshake]
// r[impl session.handshake.phon]
// r[impl session.connection-settings.hello]
// r[impl session.handshake.protocol-schema.session-scoped]
// r[impl session.handshake.unversioned]
/// Perform the phon handshake as the initiator.
///
/// Three-step exchange:
/// 1. Send Hello
/// 2. Receive HelloYourself (or Sorry)
/// 3. Send LetsGo (or Sorry)
pub async fn handshake_as_initiator<Tx: LinkTx, Rx: LinkRx>(
    tx: &Tx,
    rx: &mut Rx,
    settings: ConnectionSettings,
    metadata: vox_types::Metadata,
) -> Result<HandshakeResult, HandshakeError> {
    validate_initial_channel_credit(&settings)?;

    let our_schema = message_schema();

    let hello = vox_types::Hello {
        parity: settings.parity,
        connection_settings: settings.clone(),
        message_payload_schema: our_schema.clone(),
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
    if let Err(reason) = crate::validate_message_writer_schema(&hy.message_payload_schema) {
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
    send_handshake(tx, &HandshakeMessage::LetsGo(vox_types::LetsGo {})).await?;

    Ok(HandshakeResult {
        role: SessionRole::Initiator,
        our_settings: settings,
        peer_settings: hy.connection_settings,
        our_schema,
        peer_schema: hy.message_payload_schema,
        peer_metadata: hy.metadata,
    })
}

// r[impl session.handshake]
// r[impl session.handshake.phon]
// r[impl session.connection-settings.hello]
// r[impl session.handshake.protocol-schema.session-scoped]
// r[impl session.handshake.unversioned]
/// Perform the phon handshake as the acceptor.
///
/// Three-step exchange:
/// 1. Receive Hello
/// 2. Send HelloYourself (or Sorry)
/// 3. Receive LetsGo (or Sorry)
pub async fn handshake_as_acceptor<Tx: LinkTx, Rx: LinkRx>(
    tx: &Tx,
    rx: &mut Rx,
    settings: ConnectionSettings,
    metadata: vox_types::Metadata,
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
    if let Err(reason) = crate::validate_message_writer_schema(&hello.message_payload_schema) {
        send_handshake(
            tx,
            &HandshakeMessage::Sorry(vox_types::Sorry {
                reason: reason.clone(),
            }),
        )
        .await?;
        return Err(HandshakeError::Protocol(reason));
    }

    // Acceptor adopts opposite parity
    let our_settings = ConnectionSettings {
        parity: hello.parity.other(),
        ..settings
    };

    let our_schema = message_schema();

    // Step 2: Send HelloYourself
    let hy = vox_types::HelloYourself {
        connection_settings: our_settings.clone(),
        message_payload_schema: our_schema.clone(),
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

    Ok(HandshakeResult {
        role: SessionRole::Acceptor,
        our_settings,
        peer_settings: hello.connection_settings,
        our_schema,
        peer_schema: hello.message_payload_schema,
        peer_metadata: hello.metadata,
    })
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

    // r[verify session.handshake]
    // r[verify session.handshake.phon]
    // r[verify session.handshake.protocol-schema]
    // r[verify session.connection-settings.hello]
    // r[verify session.peer]
    #[tokio::test]
    async fn hello_and_hello_yourself_carry_root_connection_settings() {
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
            }),
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
    }

    // r[verify session.handshake.sorry]
    // r[verify session.handshake.unversioned]
    // r[verify session.handshake.protocol-schema.session-scoped]
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
            }),
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
    async fn acceptor_rejects_peer_zero_initial_credit_before_session_starts() {
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
