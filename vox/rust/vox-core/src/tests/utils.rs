use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use facet::Facet;
use vox_types::{
    Conduit, ConduitRx, ConnectionRole, ConnectionSettings, Handler, HandshakeResult, Message,
    MessageFamily, MessagePayload, Parity, Payload, ReplySink, RequestCall, RequestResponse,
    SelfRef, Tx,
};

use crate::{BareConduit, DriverReplySink, memory_link_pair};

#[derive(Clone)]
pub(crate) struct TestLaneClient {
    pub(crate) caller: crate::Caller,
    pub(crate) connection: Option<crate::connection::ConnectionHandle>,
}

impl crate::FromVoxLane for TestLaneClient {
    const SERVICE_NAME: &'static str = "Noop";

    fn from_vox_lane(
        caller: crate::Caller,
        connection: Option<crate::connection::ConnectionHandle>,
    ) -> Self {
        Self { caller, connection }
    }
}

pub(crate) fn test_acceptor_handshake() -> HandshakeResult {
    HandshakeResult {
        role: ConnectionRole::Acceptor,
        our_settings: ConnectionSettings {
            parity: Parity::Even,
            max_concurrent_requests: 64,
            initial_channel_credit: 16,
        },
        peer_settings: ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
            initial_channel_credit: 16,
        },
        our_schema: vec![],
        peer_schema: vec![],
        peer_metadata: vox_types::metadata().str("vox-service", "Noop").build(),
    }
}

pub(crate) fn test_initiator_handshake() -> HandshakeResult {
    HandshakeResult {
        role: ConnectionRole::Initiator,
        our_settings: ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
            initial_channel_credit: 16,
        },
        peer_settings: ConnectionSettings {
            parity: Parity::Even,
            max_concurrent_requests: 64,
            initial_channel_credit: 16,
        },
        our_schema: vec![],
        peer_schema: vec![],
        peer_metadata: vox_types::metadata().str("vox-service", "Noop").build(),
    }
}

pub(crate) type MessageConduit = BareConduit<MessageFamily, crate::MemoryLink>;

pub(crate) fn message_conduit_pair() -> (MessageConduit, MessageConduit) {
    let (a, b) = memory_link_pair(64);
    (BareConduit::new(a), BareConduit::new(b))
}

/// Conduit wrapper used by keepalive tests: drops inbound Pong messages.
pub(crate) struct DropPongConduit<C> {
    inner: C,
}

impl<C> DropPongConduit<C> {
    pub(crate) fn new(inner: C) -> Self {
        Self { inner }
    }
}

impl<C> Conduit for DropPongConduit<C>
where
    C: Conduit<Msg = MessageFamily>,
    C::Rx: Send,
{
    type Msg = MessageFamily;
    type Tx = C::Tx;
    type Rx = DropPongRx<C::Rx>;

    fn split(self) -> (Self::Tx, Self::Rx) {
        let (tx, rx) = self.inner.split();
        (tx, DropPongRx { inner: rx })
    }
}

impl<C> crate::IntoConduit for DropPongConduit<C>
where
    C: Conduit<Msg = MessageFamily>,
    C::Rx: Send,
{
    type Conduit = Self;

    fn into_conduit(self) -> Self {
        self
    }
}

pub(crate) struct DropPongRx<Rx> {
    inner: Rx,
}

impl<Rx> ConduitRx for DropPongRx<Rx>
where
    Rx: ConduitRx<Msg = MessageFamily> + Send,
{
    type Msg = MessageFamily;
    type Error = Rx::Error;

    async fn recv(&mut self) -> Result<Option<SelfRef<Message<'static>>>, Self::Error> {
        loop {
            let Some(msg) = self.inner.recv().await? else {
                return Ok(None);
            };
            let msg_ref = msg.get();
            if matches!(&msg_ref.payload, MessagePayload::Pong(_)) {
                continue;
            }
            return Ok(Some(msg));
        }
    }
}

/// A handler that echoes back the raw args payload as the response.
#[derive(Clone, Copy)]
pub(crate) struct EchoHandler;

impl Handler<DriverReplySink> for EchoHandler {
    async fn handle(
        &self,
        call: SelfRef<RequestCall<'static>>,
        reply: DriverReplySink,
        _schemas: std::sync::Arc<vox_types::SchemaRecvTracker>,
    ) {
        let call = call.get();
        let args_bytes = match &call.args {
            Payload::Encoded(bytes) => bytes,
            _ => panic!("expected incoming payload"),
        };

        let result: u32 = vox_phon::from_slice(args_bytes).expect("deserialize args");
        reply
            .send_reply(RequestResponse {
                ret: Payload::outgoing(&result),
                schemas: Default::default(),
                metadata: Default::default(),
            })
            .await;
    }
}

/// A handler that blocks forever until its task is cancelled.
/// Tracks whether cancellation occurred via a drop guard.
#[derive(Clone)]
pub(crate) struct BlockingHandler {
    pub(crate) was_cancelled: Arc<AtomicBool>,
}

impl Handler<DriverReplySink> for BlockingHandler {
    async fn handle(
        &self,
        _call: SelfRef<RequestCall<'static>>,
        reply: DriverReplySink,
        _schemas: std::sync::Arc<vox_types::SchemaRecvTracker>,
    ) {
        let was_cancelled = self.was_cancelled.clone();
        // Hold the reply to prevent premature DriverReplySink::drop
        let _reply = reply;
        // Create a drop guard that records cancellation
        struct DropGuard(Arc<AtomicBool>);
        impl Drop for DropGuard {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let _guard = DropGuard(was_cancelled);
        // Block forever — only cancellation (abort) will stop this
        std::future::pending::<()>().await;
    }
}

use crate::connection::{LaneAcceptor, LaneRequest, PendingLane};

pub(crate) struct EchoAcceptor;

impl LaneAcceptor for EchoAcceptor {
    fn accept(
        &self,
        _request: &LaneRequest,
        connection: PendingLane,
    ) -> Result<(), vox_types::Metadata> {
        connection.handle_with(EchoHandler);
        Ok(())
    }
}

#[derive(Facet)]
pub(crate) struct SubscribeArgs {
    pub(crate) updates: Tx<u32>,
}
