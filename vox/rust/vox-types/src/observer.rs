use std::sync::Arc;
use std::time::Duration;

use crate::{ChannelDirection, ChannelId, ConnectionRole, LaneId, MethodId, RequestId};

pub type VoxObserverHandle = Arc<dyn VoxObserver>;

// r[impl rpc.observability.runtime]
pub trait VoxObserver: Send + Sync + 'static {
    fn rpc_event(&self, _event: RpcEvent) {}
    fn channel_event(&self, _event: ChannelEvent) {}
    fn transport_event(&self, _event: TransportEvent) {}
    fn establishment_event(&self, _event: EstablishmentEvent) {}
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
    TimedOut,
    Indeterminate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EstablishmentPhase {
    EndpointResolution,
    LinkCreation,
    TcpConnection,
    UnixSocketConnection,
    NamedPipeConnection,
    InProcessLink,
    TlsHandshake,
    PlatformSecurityHandshake,
    WebSocketUpgrade,
    VoxTransportPrologue,
    ConnectionHandshake,
    SchemaDecodePlan,
    ServiceLaneOpen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EstablishmentOutcome {
    Ok,
    Error,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EstablishmentContext {
    pub role: ConnectionRole,
    pub phase: EstablishmentPhase,
    pub lane_id: Option<LaneId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EstablishmentEvent {
    Started {
        context: EstablishmentContext,
    },
    Finished {
        context: EstablishmentContext,
        outcome: EstablishmentOutcome,
        elapsed: Duration,
    },
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
    RequestTerminated,
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
    pub connection_id: Option<LaneId>,
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
        connection_id: LaneId,
    },
    ConnectionClosed {
        connection_id: LaneId,
        reason: ConnectionCloseReason,
    },
    RequestStarted {
        connection_id: LaneId,
        request_id: RequestId,
        method_id: MethodId,
    },
    RequestFinished {
        connection_id: LaneId,
        request_id: RequestId,
        outcome: RpcOutcome,
        elapsed: Duration,
    },
    OutboundQueueFull {
        connection_id: LaneId,
    },
    OutboundQueueClosed {
        connection_id: LaneId,
    },
    FrameRead {
        connection_id: LaneId,
        bytes: usize,
    },
    FrameWritten {
        connection_id: LaneId,
        bytes: usize,
    },
    DecodeError {
        connection_id: LaneId,
        kind: DecodeErrorKind,
    },
    EncodeError {
        connection_id: LaneId,
        kind: EncodeErrorKind,
    },
    ProtocolError {
        connection_id: LaneId,
        kind: ProtocolErrorKind,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportEvent {
    FrameRead {
        connection_id: Option<LaneId>,
        bytes: usize,
    },
    FrameWritten {
        connection_id: Option<LaneId>,
        bytes: usize,
    },
    Closed {
        connection_id: Option<LaneId>,
        reason: ConnectionCloseReason,
    },
}

// r[impl rpc.observability.low-cardinality]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObserverMetricKind {
    RpcStarted,
    RpcFinished,
    ChannelOpened,
    ChannelSendStarted,
    ChannelSendWaitingForCredit,
    ChannelSendFinished,
    ChannelTrySend,
    ChannelCreditGranted,
    ChannelItemReceived,
    ChannelItemConsumed,
    ChannelClosed,
    ChannelReset,
    DriverConnectionOpened,
    DriverConnectionClosed,
    DriverRequestStarted,
    DriverRequestFinished,
    DriverOutboundQueueFull,
    DriverOutboundQueueClosed,
    DriverFrameRead,
    DriverFrameWritten,
    DriverDecodeError,
    DriverEncodeError,
    DriverProtocolError,
    TransportFrameRead,
    TransportFrameWritten,
    TransportClosed,
    EstablishmentStarted,
    EstablishmentFinished,
}

// r[impl rpc.observability.low-cardinality]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObserverMetricLabels {
    pub kind: ObserverMetricKind,
    pub service: Option<&'static str>,
    pub method: Option<&'static str>,
    pub side: Option<RpcSide>,
    pub outcome: Option<&'static str>,
    pub error_kind: Option<&'static str>,
    pub channel_direction: Option<ChannelDirection>,
    pub establishment_phase: Option<&'static str>,
}

impl ObserverMetricLabels {
    pub const fn new(kind: ObserverMetricKind) -> Self {
        Self {
            kind,
            service: None,
            method: None,
            side: None,
            outcome: None,
            error_kind: None,
            channel_direction: None,
            establishment_phase: None,
        }
    }

    fn with_channel_debug(mut self, debug: Option<ChannelDebugContext>) -> Self {
        if let Some(debug) = debug {
            self.service = debug.service;
            self.method = debug.method;
        }
        self
    }
}

impl RpcEvent {
    // r[impl rpc.observability.low-cardinality]
    pub fn metric_labels(&self) -> ObserverMetricLabels {
        match *self {
            Self::Started {
                side,
                service,
                method,
                method_id: _,
            } => {
                let mut labels = ObserverMetricLabels::new(ObserverMetricKind::RpcStarted);
                labels.side = Some(side);
                labels.service = service;
                labels.method = method;
                labels
            }
            Self::Finished {
                side,
                service,
                method,
                method_id: _,
                outcome,
                elapsed: _,
            } => {
                let mut labels = ObserverMetricLabels::new(ObserverMetricKind::RpcFinished);
                labels.side = Some(side);
                labels.service = service;
                labels.method = method;
                labels.outcome = Some(rpc_outcome_label(outcome));
                labels
            }
        }
    }
}

impl ChannelEvent {
    // r[impl rpc.observability.low-cardinality]
    pub fn metric_labels(&self) -> ObserverMetricLabels {
        match *self {
            Self::Opened {
                channel,
                direction,
                initial_credit: _,
            } => {
                let mut labels = ObserverMetricLabels::new(ObserverMetricKind::ChannelOpened)
                    .with_channel_debug(channel.debug);
                labels.channel_direction = Some(direction);
                labels
            }
            Self::SendStarted { channel } => {
                ObserverMetricLabels::new(ObserverMetricKind::ChannelSendStarted)
                    .with_channel_debug(channel.debug)
            }
            Self::SendWaitingForCredit { channel } => {
                ObserverMetricLabels::new(ObserverMetricKind::ChannelSendWaitingForCredit)
                    .with_channel_debug(channel.debug)
            }
            Self::SendFinished {
                channel,
                outcome,
                elapsed: _,
            } => {
                let mut labels = ObserverMetricLabels::new(ObserverMetricKind::ChannelSendFinished)
                    .with_channel_debug(channel.debug);
                labels.outcome = Some(channel_send_outcome_label(outcome));
                labels
            }
            Self::TrySend { channel, outcome } => {
                let mut labels = ObserverMetricLabels::new(ObserverMetricKind::ChannelTrySend)
                    .with_channel_debug(channel.debug);
                labels.outcome = Some(channel_try_send_outcome_label(outcome));
                labels
            }
            Self::CreditGranted { channel, amount: _ } => {
                ObserverMetricLabels::new(ObserverMetricKind::ChannelCreditGranted)
                    .with_channel_debug(channel.debug)
            }
            Self::ItemReceived { channel } => {
                ObserverMetricLabels::new(ObserverMetricKind::ChannelItemReceived)
                    .with_channel_debug(channel.debug)
            }
            Self::ItemConsumed { channel } => {
                ObserverMetricLabels::new(ObserverMetricKind::ChannelItemConsumed)
                    .with_channel_debug(channel.debug)
            }
            Self::Closed { channel, reason } => {
                let mut labels = ObserverMetricLabels::new(ObserverMetricKind::ChannelClosed)
                    .with_channel_debug(channel.debug);
                labels.outcome = Some(channel_close_reason_label(reason));
                labels
            }
            Self::Reset { channel, reason } => {
                let mut labels = ObserverMetricLabels::new(ObserverMetricKind::ChannelReset)
                    .with_channel_debug(channel.debug);
                labels.outcome = Some(channel_reset_reason_label(reason));
                labels
            }
        }
    }
}

impl DriverEvent {
    // r[impl rpc.observability.low-cardinality]
    pub fn metric_labels(&self) -> ObserverMetricLabels {
        match *self {
            Self::ConnectionOpened { connection_id: _ } => {
                ObserverMetricLabels::new(ObserverMetricKind::DriverConnectionOpened)
            }
            Self::ConnectionClosed {
                connection_id: _,
                reason,
            } => {
                let mut labels =
                    ObserverMetricLabels::new(ObserverMetricKind::DriverConnectionClosed);
                labels.outcome = Some(connection_close_reason_label(reason));
                labels
            }
            Self::RequestStarted {
                connection_id: _,
                request_id: _,
                method_id: _,
            } => ObserverMetricLabels::new(ObserverMetricKind::DriverRequestStarted),
            Self::RequestFinished {
                connection_id: _,
                request_id: _,
                outcome,
                elapsed: _,
            } => {
                let mut labels =
                    ObserverMetricLabels::new(ObserverMetricKind::DriverRequestFinished);
                labels.outcome = Some(rpc_outcome_label(outcome));
                labels
            }
            Self::OutboundQueueFull { connection_id: _ } => {
                ObserverMetricLabels::new(ObserverMetricKind::DriverOutboundQueueFull)
            }
            Self::OutboundQueueClosed { connection_id: _ } => {
                ObserverMetricLabels::new(ObserverMetricKind::DriverOutboundQueueClosed)
            }
            Self::FrameRead {
                connection_id: _,
                bytes: _,
            } => ObserverMetricLabels::new(ObserverMetricKind::DriverFrameRead),
            Self::FrameWritten {
                connection_id: _,
                bytes: _,
            } => ObserverMetricLabels::new(ObserverMetricKind::DriverFrameWritten),
            Self::DecodeError {
                connection_id: _,
                kind,
            } => {
                let mut labels = ObserverMetricLabels::new(ObserverMetricKind::DriverDecodeError);
                labels.error_kind = Some(decode_error_kind_label(kind));
                labels
            }
            Self::EncodeError {
                connection_id: _,
                kind,
            } => {
                let mut labels = ObserverMetricLabels::new(ObserverMetricKind::DriverEncodeError);
                labels.error_kind = Some(encode_error_kind_label(kind));
                labels
            }
            Self::ProtocolError {
                connection_id: _,
                kind,
            } => {
                let mut labels = ObserverMetricLabels::new(ObserverMetricKind::DriverProtocolError);
                labels.error_kind = Some(protocol_error_kind_label(kind));
                labels
            }
        }
    }
}

impl TransportEvent {
    // r[impl rpc.observability.low-cardinality]
    pub fn metric_labels(&self) -> ObserverMetricLabels {
        match *self {
            Self::FrameRead {
                connection_id: _,
                bytes: _,
            } => ObserverMetricLabels::new(ObserverMetricKind::TransportFrameRead),
            Self::FrameWritten {
                connection_id: _,
                bytes: _,
            } => ObserverMetricLabels::new(ObserverMetricKind::TransportFrameWritten),
            Self::Closed {
                connection_id: _,
                reason,
            } => {
                let mut labels = ObserverMetricLabels::new(ObserverMetricKind::TransportClosed);
                labels.outcome = Some(connection_close_reason_label(reason));
                labels
            }
        }
    }
}

impl EstablishmentEvent {
    // r[impl rpc.observability.establishment]
    // r[impl rpc.observability.low-cardinality]
    pub fn metric_labels(&self) -> ObserverMetricLabels {
        match *self {
            Self::Started { context } => {
                let mut labels =
                    ObserverMetricLabels::new(ObserverMetricKind::EstablishmentStarted);
                labels.establishment_phase = Some(establishment_phase_label(context.phase));
                labels
            }
            Self::Finished {
                context,
                outcome,
                elapsed: _,
            } => {
                let mut labels =
                    ObserverMetricLabels::new(ObserverMetricKind::EstablishmentFinished);
                labels.establishment_phase = Some(establishment_phase_label(context.phase));
                labels.outcome = Some(establishment_outcome_label(outcome));
                labels
            }
        }
    }
}

const fn rpc_outcome_label(outcome: RpcOutcome) -> &'static str {
    match outcome {
        RpcOutcome::Ok => "ok",
        RpcOutcome::Error => "error",
        RpcOutcome::Cancelled => "cancelled",
        RpcOutcome::Dropped => "dropped",
        RpcOutcome::Closed => "closed",
        RpcOutcome::SendFailed => "send-failed",
        RpcOutcome::TimedOut => "timed-out",
        RpcOutcome::Indeterminate => "indeterminate",
    }
}

const fn establishment_phase_label(phase: EstablishmentPhase) -> &'static str {
    match phase {
        EstablishmentPhase::EndpointResolution => "endpoint-resolution",
        EstablishmentPhase::LinkCreation => "link-creation",
        EstablishmentPhase::TcpConnection => "tcp-connection",
        EstablishmentPhase::UnixSocketConnection => "unix-socket-connection",
        EstablishmentPhase::NamedPipeConnection => "named-pipe-connection",
        EstablishmentPhase::InProcessLink => "in-process-link",
        EstablishmentPhase::TlsHandshake => "tls-handshake",
        EstablishmentPhase::PlatformSecurityHandshake => "platform-security-handshake",
        EstablishmentPhase::WebSocketUpgrade => "websocket-upgrade",
        EstablishmentPhase::VoxTransportPrologue => "vox-transport-prologue",
        EstablishmentPhase::ConnectionHandshake => "connection-handshake",
        EstablishmentPhase::SchemaDecodePlan => "schema-decode-plan",
        EstablishmentPhase::ServiceLaneOpen => "service-lane-open",
    }
}

const fn establishment_outcome_label(outcome: EstablishmentOutcome) -> &'static str {
    match outcome {
        EstablishmentOutcome::Ok => "ok",
        EstablishmentOutcome::Error => "error",
        EstablishmentOutcome::Rejected => "rejected",
    }
}

const fn channel_try_send_outcome_label(outcome: ChannelTrySendOutcome) -> &'static str {
    match outcome {
        ChannelTrySendOutcome::Sent => "sent",
        ChannelTrySendOutcome::FullCredit => "full-credit",
        ChannelTrySendOutcome::FullRuntimeQueue => "full-runtime-queue",
        ChannelTrySendOutcome::Unbound => "unbound",
        ChannelTrySendOutcome::Closed => "closed",
    }
}

const fn channel_send_outcome_label(outcome: ChannelSendOutcome) -> &'static str {
    match outcome {
        ChannelSendOutcome::Sent => "sent",
        ChannelSendOutcome::Closed => "closed",
        ChannelSendOutcome::TransportError => "transport-error",
    }
}

const fn channel_close_reason_label(reason: ChannelCloseReason) -> &'static str {
    match reason {
        ChannelCloseReason::Local => "local",
        ChannelCloseReason::Remote => "remote",
        ChannelCloseReason::Dropped => "dropped",
        ChannelCloseReason::ConnectionClosed => "connection-closed",
        ChannelCloseReason::RequestTerminated => "request-terminated",
        ChannelCloseReason::ReceiverDropped => "receiver-dropped",
        ChannelCloseReason::Unknown => "unknown",
    }
}

const fn channel_reset_reason_label(reason: ChannelResetReason) -> &'static str {
    match reason {
        ChannelResetReason::Local => "local",
        ChannelResetReason::Remote => "remote",
        ChannelResetReason::ReceiverDropped => "receiver-dropped",
        ChannelResetReason::Protocol => "protocol",
        ChannelResetReason::ConnectionClosed => "connection-closed",
        ChannelResetReason::Unknown => "unknown",
    }
}

const fn connection_close_reason_label(reason: ConnectionCloseReason) -> &'static str {
    match reason {
        ConnectionCloseReason::Local => "local",
        ConnectionCloseReason::Remote => "remote",
        ConnectionCloseReason::Protocol => "protocol",
        ConnectionCloseReason::Transport => "transport",
        ConnectionCloseReason::SessionShutdown => "connection-shutdown",
        ConnectionCloseReason::CallerDropped => "caller-dropped",
        ConnectionCloseReason::Unknown => "unknown",
    }
}

const fn decode_error_kind_label(kind: DecodeErrorKind) -> &'static str {
    match kind {
        DecodeErrorKind::Schema => "schema",
        DecodeErrorKind::Payload => "payload",
        DecodeErrorKind::Protocol => "protocol",
        DecodeErrorKind::Unknown => "unknown",
    }
}

const fn encode_error_kind_label(kind: EncodeErrorKind) -> &'static str {
    match kind {
        EncodeErrorKind::Schema => "schema",
        EncodeErrorKind::Payload => "payload",
        EncodeErrorKind::Transport => "transport",
        EncodeErrorKind::Unknown => "unknown",
    }
}

const fn protocol_error_kind_label(kind: ProtocolErrorKind) -> &'static str {
    match kind {
        ProtocolErrorKind::InvalidConnection => "invalid-connection",
        ProtocolErrorKind::InvalidRequest => "invalid-request",
        ProtocolErrorKind::InvalidChannel => "invalid-channel",
        ProtocolErrorKind::Schema => "schema",
        ProtocolErrorKind::FlowControl => "flow-control",
        ProtocolErrorKind::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingObserver {
        events: Mutex<Vec<&'static str>>,
    }

    impl VoxObserver for RecordingObserver {
        fn rpc_event(&self, _event: RpcEvent) {
            self.events
                .lock()
                .expect("observer events mutex poisoned")
                .push("rpc");
        }

        fn channel_event(&self, _event: ChannelEvent) {
            self.events
                .lock()
                .expect("observer events mutex poisoned")
                .push("channel");
        }

        fn transport_event(&self, _event: TransportEvent) {
            self.events
                .lock()
                .expect("observer events mutex poisoned")
                .push("transport");
        }

        fn establishment_event(&self, _event: EstablishmentEvent) {
            self.events
                .lock()
                .expect("observer events mutex poisoned")
                .push("establishment");
        }

        fn driver_event(&self, _event: DriverEvent) {
            self.events
                .lock()
                .expect("observer events mutex poisoned")
                .push("driver");
        }
    }

    fn sample_channel() -> ChannelEventContext {
        ChannelEventContext {
            connection_id: Some(LaneId(42)),
            channel_id: ChannelId(99),
            debug: Some(ChannelDebugContext {
                label: Some("per-request debug label"),
                type_name: Some("alloc::string::String"),
                source_location: Some(SourceLocation {
                    file: "src/lib.rs",
                    line: 10,
                    column: 20,
                }),
                service: Some("Catalog"),
                method: Some("stream"),
            }),
        }
    }

    fn assert_metric_labels_hide_ids(labels: ObserverMetricLabels) {
        let rendered = format!("{labels:?}");
        assert!(!rendered.contains("LaneId"));
        assert!(!rendered.contains("RequestId"));
        assert!(!rendered.contains("ChannelId"));
        assert!(!rendered.contains("MethodId"));
        assert!(!rendered.contains("per-request debug label"));
        assert!(!rendered.contains("alloc::string::String"));
        assert!(!rendered.contains("src/lib.rs"));
    }

    // r[verify rpc.observability.runtime]
    #[test]
    fn observer_interface_receives_local_event_categories_without_backend() {
        let observer = RecordingObserver::default();

        observer.rpc_event(RpcEvent::Started {
            side: RpcSide::Client,
            service: Some("Catalog"),
            method: Some("get"),
            method_id: MethodId(1),
        });
        observer.channel_event(ChannelEvent::Opened {
            channel: sample_channel(),
            direction: ChannelDirection::Tx,
            initial_credit: 16,
        });
        observer.transport_event(TransportEvent::FrameRead {
            connection_id: Some(LaneId(1)),
            bytes: 64,
        });
        observer.establishment_event(EstablishmentEvent::Started {
            context: EstablishmentContext {
                role: ConnectionRole::Initiator,
                phase: EstablishmentPhase::ConnectionHandshake,
                lane_id: None,
            },
        });
        observer.driver_event(DriverEvent::ConnectionOpened {
            connection_id: LaneId(1),
        });

        assert_eq!(
            *observer
                .events
                .lock()
                .expect("observer events mutex poisoned"),
            ["rpc", "channel", "transport", "establishment", "driver"]
        );
    }

    // r[verify rpc.observability.channel]
    #[test]
    fn channel_observer_events_cover_lifecycle_and_low_cardinality_labels() {
        let cases = [
            (
                ChannelEvent::Opened {
                    channel: sample_channel(),
                    direction: ChannelDirection::Rx,
                    initial_credit: 16,
                },
                ObserverMetricKind::ChannelOpened,
            ),
            (
                ChannelEvent::SendStarted {
                    channel: sample_channel(),
                },
                ObserverMetricKind::ChannelSendStarted,
            ),
            (
                ChannelEvent::SendWaitingForCredit {
                    channel: sample_channel(),
                },
                ObserverMetricKind::ChannelSendWaitingForCredit,
            ),
            (
                ChannelEvent::SendFinished {
                    channel: sample_channel(),
                    outcome: ChannelSendOutcome::TransportError,
                    elapsed: Duration::from_millis(1),
                },
                ObserverMetricKind::ChannelSendFinished,
            ),
            (
                ChannelEvent::TrySend {
                    channel: sample_channel(),
                    outcome: ChannelTrySendOutcome::FullRuntimeQueue,
                },
                ObserverMetricKind::ChannelTrySend,
            ),
            (
                ChannelEvent::CreditGranted {
                    channel: sample_channel(),
                    amount: 4,
                },
                ObserverMetricKind::ChannelCreditGranted,
            ),
            (
                ChannelEvent::ItemReceived {
                    channel: sample_channel(),
                },
                ObserverMetricKind::ChannelItemReceived,
            ),
            (
                ChannelEvent::ItemConsumed {
                    channel: sample_channel(),
                },
                ObserverMetricKind::ChannelItemConsumed,
            ),
            (
                ChannelEvent::Closed {
                    channel: sample_channel(),
                    reason: ChannelCloseReason::ConnectionClosed,
                },
                ObserverMetricKind::ChannelClosed,
            ),
            (
                ChannelEvent::Reset {
                    channel: sample_channel(),
                    reason: ChannelResetReason::Protocol,
                },
                ObserverMetricKind::ChannelReset,
            ),
        ];

        for (event, kind) in cases {
            assert!(
                format!("{event:?}").contains("ChannelId(99)"),
                "channel events should retain local channel IDs for logs/debug"
            );
            let labels = event.metric_labels();
            assert_eq!(labels.kind, kind);
            assert_eq!(labels.service, Some("Catalog"));
            assert_eq!(labels.method, Some("stream"));
            assert_metric_labels_hide_ids(labels);
        }
    }

    // r[verify rpc.observability.driver]
    #[test]
    fn driver_observer_events_cover_runtime_diagnostics_and_low_cardinality_labels() {
        let cases = [
            (
                DriverEvent::ConnectionOpened {
                    connection_id: LaneId(7),
                },
                ObserverMetricKind::DriverConnectionOpened,
            ),
            (
                DriverEvent::ConnectionClosed {
                    connection_id: LaneId(7),
                    reason: ConnectionCloseReason::Protocol,
                },
                ObserverMetricKind::DriverConnectionClosed,
            ),
            (
                DriverEvent::RequestStarted {
                    connection_id: LaneId(7),
                    request_id: RequestId(11),
                    method_id: MethodId(13),
                },
                ObserverMetricKind::DriverRequestStarted,
            ),
            (
                DriverEvent::RequestFinished {
                    connection_id: LaneId(7),
                    request_id: RequestId(11),
                    outcome: RpcOutcome::Indeterminate,
                    elapsed: Duration::from_millis(2),
                },
                ObserverMetricKind::DriverRequestFinished,
            ),
            (
                DriverEvent::OutboundQueueFull {
                    connection_id: LaneId(7),
                },
                ObserverMetricKind::DriverOutboundQueueFull,
            ),
            (
                DriverEvent::OutboundQueueClosed {
                    connection_id: LaneId(7),
                },
                ObserverMetricKind::DriverOutboundQueueClosed,
            ),
            (
                DriverEvent::FrameRead {
                    connection_id: LaneId(7),
                    bytes: 128,
                },
                ObserverMetricKind::DriverFrameRead,
            ),
            (
                DriverEvent::FrameWritten {
                    connection_id: LaneId(7),
                    bytes: 256,
                },
                ObserverMetricKind::DriverFrameWritten,
            ),
            (
                DriverEvent::DecodeError {
                    connection_id: LaneId(7),
                    kind: DecodeErrorKind::Payload,
                },
                ObserverMetricKind::DriverDecodeError,
            ),
            (
                DriverEvent::EncodeError {
                    connection_id: LaneId(7),
                    kind: EncodeErrorKind::Transport,
                },
                ObserverMetricKind::DriverEncodeError,
            ),
            (
                DriverEvent::ProtocolError {
                    connection_id: LaneId(7),
                    kind: ProtocolErrorKind::FlowControl,
                },
                ObserverMetricKind::DriverProtocolError,
            ),
        ];

        for (event, kind) in cases {
            assert!(
                format!("{event:?}").contains("LaneId(7)"),
                "driver events should retain connection IDs for logs/debug"
            );
            let labels = event.metric_labels();
            assert_eq!(labels.kind, kind);
            assert_metric_labels_hide_ids(labels);
        }
    }

    // r[verify rpc.observability.low-cardinality]
    #[test]
    fn metric_labels_project_rpc_and_channel_events_without_ids() {
        let rpc_labels = RpcEvent::Finished {
            side: RpcSide::Client,
            service: Some("Catalog"),
            method: Some("get"),
            method_id: MethodId(0xdead_beef),
            outcome: RpcOutcome::Ok,
            elapsed: Duration::from_millis(3),
        }
        .metric_labels();

        assert_eq!(rpc_labels.kind, ObserverMetricKind::RpcFinished);
        assert_eq!(rpc_labels.side, Some(RpcSide::Client));
        assert_eq!(rpc_labels.service, Some("Catalog"));
        assert_eq!(rpc_labels.method, Some("get"));
        assert_eq!(rpc_labels.outcome, Some("ok"));
        assert!(!format!("{rpc_labels:?}").contains("dead"));

        let channel_labels = ChannelEvent::TrySend {
            channel: ChannelEventContext {
                connection_id: Some(LaneId(42)),
                channel_id: ChannelId(99),
                debug: Some(ChannelDebugContext {
                    label: Some("per-request debug label"),
                    type_name: Some("alloc::string::String"),
                    source_location: Some(SourceLocation {
                        file: "src/lib.rs",
                        line: 10,
                        column: 20,
                    }),
                    service: Some("Catalog"),
                    method: Some("stream"),
                }),
            },
            outcome: ChannelTrySendOutcome::FullRuntimeQueue,
        }
        .metric_labels();

        assert_eq!(channel_labels.kind, ObserverMetricKind::ChannelTrySend);
        assert_eq!(channel_labels.service, Some("Catalog"));
        assert_eq!(channel_labels.method, Some("stream"));
        assert_eq!(channel_labels.outcome, Some("full-runtime-queue"));
        let rendered = format!("{channel_labels:?}");
        assert!(!rendered.contains("LaneId"));
        assert!(!rendered.contains("ChannelId"));
        assert!(!rendered.contains("per-request debug label"));
        assert!(!rendered.contains("alloc::string::String"));
        assert!(!rendered.contains("src/lib.rs"));
    }

    // r[verify rpc.observability.low-cardinality]
    #[test]
    fn metric_labels_project_driver_and_transport_events_without_ids() {
        let driver_labels = DriverEvent::DecodeError {
            connection_id: LaneId(77),
            kind: DecodeErrorKind::Payload,
        }
        .metric_labels();

        assert_eq!(driver_labels.kind, ObserverMetricKind::DriverDecodeError);
        assert_eq!(driver_labels.error_kind, Some("payload"));

        let transport_labels = TransportEvent::Closed {
            connection_id: Some(LaneId(88)),
            reason: ConnectionCloseReason::Transport,
        }
        .metric_labels();

        assert_eq!(transport_labels.kind, ObserverMetricKind::TransportClosed);
        assert_eq!(transport_labels.outcome, Some("transport"));
        assert!(!format!("{driver_labels:?}").contains("77"));
        assert!(!format!("{transport_labels:?}").contains("88"));
    }

    // r[verify rpc.observability.establishment]
    // r[verify rpc.observability.low-cardinality]
    #[test]
    fn establishment_events_keep_phase_and_hide_lane_ids_in_metric_labels() {
        let started = EstablishmentEvent::Started {
            context: EstablishmentContext {
                role: ConnectionRole::Initiator,
                phase: EstablishmentPhase::VoxTransportPrologue,
                lane_id: None,
            },
        };
        let finished = EstablishmentEvent::Finished {
            context: EstablishmentContext {
                role: ConnectionRole::Acceptor,
                phase: EstablishmentPhase::ServiceLaneOpen,
                lane_id: Some(LaneId(99)),
            },
            outcome: EstablishmentOutcome::Rejected,
            elapsed: Duration::from_millis(4),
        };

        let started_labels = started.metric_labels();
        assert_eq!(
            started_labels.kind,
            ObserverMetricKind::EstablishmentStarted
        );
        assert_eq!(
            started_labels.establishment_phase,
            Some("vox-transport-prologue")
        );

        let finished_labels = finished.metric_labels();
        assert_eq!(
            finished_labels.kind,
            ObserverMetricKind::EstablishmentFinished
        );
        assert_eq!(
            finished_labels.establishment_phase,
            Some("service-lane-open")
        );
        assert_eq!(finished_labels.outcome, Some("rejected"));
        assert!(
            format!("{finished:?}").contains("LaneId(99)"),
            "establishment events should retain local IDs for logs/debug"
        );
        assert!(!format!("{finished_labels:?}").contains("99"));
    }
}
