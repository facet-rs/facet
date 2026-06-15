use std::time::Duration;

use crate::time::Instant;
use crate::{
    ChannelCloseReason, ChannelDebugContext, ChannelDirection, ChannelId, ChannelResetReason,
    ConnectionCloseReason, LaneId, MethodId, RequestId,
};

// r[impl rpc.debug.snapshot]
#[derive(Clone, Debug, Default)]
pub struct VoxDebugSnapshot {
    pub connections: Vec<ConnectionDebugSnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionDebugState {
    Open,
    Closing,
    Closed,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriverTaskStatus {
    Alive,
    Dead,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct ConnectionDebugSnapshot {
    pub connection_id: LaneId,
    pub endpoint: Option<String>,
    pub surface: Option<String>,
    pub component: Option<String>,
    pub state: ConnectionDebugState,
    pub outstanding_requests: usize,
    pub requests: Vec<RequestDebugSnapshot>,
    pub open_channels: Vec<ChannelDebugSnapshot>,
    pub outbound_queue_depth: Option<usize>,
    pub outbound_queue_capacity: Option<usize>,
    pub local_control_queue_depth: Option<usize>,
    pub local_control_queue_capacity: Option<usize>,
    pub last_inbound_message_at: Option<Instant>,
    pub last_outbound_message_at: Option<Instant>,
    pub last_progress_at: Option<Instant>,
    pub close_reason: Option<ConnectionCloseReason>,
    pub driver_task_status: DriverTaskStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestDebugState {
    Dispatching,
    WaitingForResponse,
    Finished,
    Failed,
}

#[derive(Clone, Debug)]
pub struct RequestDebugSnapshot {
    pub request_id: RequestId,
    pub service: Option<&'static str>,
    pub method: Option<&'static str>,
    pub method_id: MethodId,
    pub age: Duration,
    pub state: RequestDebugState,
    pub response_sender_blocked: Option<bool>,
    pub associated_channels: Vec<ChannelId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelReceiverState {
    Present,
    Dropped,
    Closed,
    Reset,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct ChannelDebugSnapshot {
    pub connection_id: LaneId,
    pub channel_id: ChannelId,
    pub direction: ChannelDirection,
    pub debug: Option<ChannelDebugContext>,
    pub initial_credit: u32,
    pub available_send_credit: Option<u32>,
    pub inbound_queue_len: Option<usize>,
    pub inbound_queue_capacity: Option<usize>,
    pub outbound_runtime_queue_len: Option<usize>,
    pub outbound_runtime_queue_capacity: Option<usize>,
    pub send_waiters_count: Option<usize>,
    pub receiver_state: ChannelReceiverState,
    pub last_item_sent_at: Option<Instant>,
    pub last_item_received_at: Option<Instant>,
    pub last_item_consumed_at: Option<Instant>,
    pub last_credit_granted_at: Option<Instant>,
    pub last_credit_received_at: Option<Instant>,
    pub last_credit_granted_amount: Option<u32>,
    pub last_credit_received_amount: Option<u32>,
    pub pending_local_grant_credit: u32,
    pub total_credit_granted: u64,
    pub total_credit_received: u64,
    pub current_permit_count: Option<u32>,
    pub zero_credit_with_blocked_senders: bool,
    pub sent: u64,
    pub sends_started: u64,
    pub sends_completed: u64,
    pub sends_waited_for_credit: u64,
    pub try_send_full_credit: u64,
    pub try_send_full_runtime_queue: u64,
    pub closed: u64,
    pub reset: u64,
    pub dropped: u64,
    pub items_received: u64,
    pub items_consumed: u64,
    pub credit_granted: u64,
    pub credit_received: u64,
    pub close_reason: Option<ChannelCloseReason>,
    pub reset_reason: Option<ChannelResetReason>,
}
