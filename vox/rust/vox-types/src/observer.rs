use std::sync::Arc;
use std::time::Duration;

use crate::{ChannelDirection, ChannelId, ConnectionId, MethodId, RequestId};

pub type VoxObserverHandle = Arc<dyn VoxObserver>;

// r[impl rpc.observability.runtime]
pub trait VoxObserver: Send + Sync + 'static {
    fn rpc_event(&self, _event: RpcEvent) {}
    fn channel_event(&self, _event: ChannelEvent) {}
    fn transport_event(&self, _event: TransportEvent) {}
    fn driver_event(&self, _event: DriverEvent) {}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RpcSide {
    Client,
    Server,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RpcOutcome {
    Ok,
    Error,
    Cancelled,
    Dropped,
    Closed,
    SendFailed,
    Indeterminate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RpcEvent {
    Started {
        side: RpcSide,
        service: Option<&'static str>,
        method: Option<&'static str>,
        method_id: MethodId,
    },
    Finished {
        side: RpcSide,
        service: Option<&'static str>,
        method: Option<&'static str>,
        method_id: MethodId,
        outcome: RpcOutcome,
        elapsed: Duration,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelTrySendOutcome {
    Sent,
    FullCredit,
    FullRuntimeQueue,
    Unbound,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelSendOutcome {
    Sent,
    Closed,
    TransportError,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelCloseReason {
    Local,
    Remote,
    Dropped,
    ConnectionClosed,
    ReceiverDropped,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelResetReason {
    Local,
    Remote,
    ReceiverDropped,
    Protocol,
    ConnectionClosed,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceLocation {
    pub file: &'static str,
    pub line: u32,
    pub column: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChannelDebugContext {
    pub label: Option<&'static str>,
    pub type_name: Option<&'static str>,
    pub source_location: Option<SourceLocation>,
    pub service: Option<&'static str>,
    pub method: Option<&'static str>,
}

impl ChannelDebugContext {
    pub const fn is_empty(&self) -> bool {
        self.label.is_none()
            && self.type_name.is_none()
            && self.source_location.is_none()
            && self.service.is_none()
            && self.method.is_none()
    }

    pub const fn into_option(self) -> Option<Self> {
        if self.is_empty() { None } else { Some(self) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelEventContext {
    pub connection_id: Option<ConnectionId>,
    pub channel_id: ChannelId,
    pub debug: Option<ChannelDebugContext>,
}

impl ChannelEventContext {
    pub const fn new(channel_id: ChannelId) -> Self {
        Self {
            connection_id: None,
            channel_id,
            debug: None,
        }
    }
}

// r[impl rpc.observability.channel]
// r[impl rpc.observability.channel.context]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelEvent {
    Opened {
        channel: ChannelEventContext,
        direction: ChannelDirection,
        initial_credit: u32,
    },
    SendStarted {
        channel: ChannelEventContext,
    },
    SendWaitingForCredit {
        channel: ChannelEventContext,
    },
    SendFinished {
        channel: ChannelEventContext,
        outcome: ChannelSendOutcome,
        elapsed: Duration,
    },
    TrySend {
        channel: ChannelEventContext,
        outcome: ChannelTrySendOutcome,
    },
    CreditGranted {
        channel: ChannelEventContext,
        amount: u32,
    },
    ItemReceived {
        channel: ChannelEventContext,
    },
    ItemConsumed {
        channel: ChannelEventContext,
    },
    Closed {
        channel: ChannelEventContext,
        reason: ChannelCloseReason,
    },
    Reset {
        channel: ChannelEventContext,
        reason: ChannelResetReason,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionCloseReason {
    Local,
    Remote,
    Protocol,
    Transport,
    SessionShutdown,
    CallerDropped,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeErrorKind {
    Schema,
    Payload,
    Protocol,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeErrorKind {
    Schema,
    Payload,
    Transport,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolErrorKind {
    InvalidConnection,
    InvalidRequest,
    InvalidChannel,
    Schema,
    FlowControl,
    Unknown,
}

// r[impl rpc.observability.driver]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriverEvent {
    ConnectionOpened {
        connection_id: ConnectionId,
    },
    ConnectionClosed {
        connection_id: ConnectionId,
        reason: ConnectionCloseReason,
    },
    RequestStarted {
        connection_id: ConnectionId,
        request_id: RequestId,
        method_id: MethodId,
    },
    RequestFinished {
        connection_id: ConnectionId,
        request_id: RequestId,
        outcome: RpcOutcome,
        elapsed: Duration,
    },
    OutboundQueueFull {
        connection_id: ConnectionId,
    },
    OutboundQueueClosed {
        connection_id: ConnectionId,
    },
    FrameRead {
        connection_id: ConnectionId,
        bytes: usize,
    },
    FrameWritten {
        connection_id: ConnectionId,
        bytes: usize,
    },
    DecodeError {
        connection_id: ConnectionId,
        kind: DecodeErrorKind,
    },
    EncodeError {
        connection_id: ConnectionId,
        kind: EncodeErrorKind,
    },
    ProtocolError {
        connection_id: ConnectionId,
        kind: ProtocolErrorKind,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportEvent {
    FrameRead {
        connection_id: Option<ConnectionId>,
        bytes: usize,
    },
    FrameWritten {
        connection_id: Option<ConnectionId>,
        bytes: usize,
    },
    Closed {
        connection_id: Option<ConnectionId>,
        reason: ConnectionCloseReason,
    },
}
