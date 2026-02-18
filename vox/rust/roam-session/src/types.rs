use facet::Facet;

use crate::peeps::prelude::*;
use crate::{
    ChannelError, ConnectionHandle, DispatchContext, DriverTxSlot, RX_STREAM_BUFFER_SIZE,
    ReceiverSlot, ServiceDispatcher, TransportError,
    runtime::{OneshotSender, Sender},
};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

// ============================================================================
// Channel types
// ============================================================================

/// Channel ID type.
pub type ChannelId = u64;

/// Event delivered to channel receivers.
///
/// This distinguishes explicit peer Close from channel teardown/drop.
#[derive(Debug, PartialEq, Eq)]
pub enum IncomingChannelMessage {
    Data(Vec<u8>),
    Close,
}

/// Connection role - determines channel ID parity.
///
/// The initiator is whoever opened the connection (e.g. connected to a TCP socket,
/// or opened an SHM channel). The acceptor is whoever accepted/received the connection.
///
/// r[impl channeling.id.parity]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Initiator uses odd channel IDs (1, 3, 5, ...).
    Initiator,
    /// Acceptor uses even channel IDs (2, 4, 6, ...).
    Acceptor,
}

/// Allocates unique channel IDs with correct parity.
///
/// r[impl channeling.id.uniqueness] - IDs are unique within a connection.
/// r[impl channeling.id.parity] - Initiator uses odd, Acceptor uses even.
pub struct ChannelIdAllocator {
    next: AtomicU64,
}

impl ChannelIdAllocator {
    /// Create a new allocator for the given role.
    pub fn new(role: Role) -> Self {
        let start = match role {
            Role::Initiator => 1, // odd: 1, 3, 5, ...
            Role::Acceptor => 2,  // even: 2, 4, 6, ...
        };
        Self {
            next: AtomicU64::new(start),
        }
    }

    /// Allocate the next channel ID.
    pub fn next(&self) -> ChannelId {
        self.next.fetch_add(2, Ordering::Relaxed)
    }
}

// ============================================================================
// Channel Registry
// ============================================================================

use std::collections::{HashMap, HashSet};

/// Response data returned from a call, including any response channels.
#[derive(Debug)]
pub struct ResponseData {
    /// The response payload bytes.
    pub payload: Vec<u8>,
    /// Channel IDs in the response (`Rx<T>` returned by the method).
    /// Client must register receivers for these channels.
    pub channels: Vec<u64>,
}

/// All messages to the connection driver go through a single channel.
///
/// This unified channel ensures FIFO ordering: a Call followed by Data
/// will always be processed in that order, preventing race conditions
/// where Data could arrive before the Request is sent.
pub enum DriverMessage {
    /// Send a Request and expect a Response (client-side call).
    Call {
        conn_id: roam_wire::ConnectionId,
        request_id: u64,
        method_id: u64,
        metadata: roam_wire::Metadata,
        /// Channel IDs used by this call (Tx/Rx), in declaration order.
        channels: Vec<u64>,
        payload: Vec<u8>,
        response_tx: OneshotSender<Result<ResponseData, TransportError>>,
    },
    /// Send a Data message on a channel.
    Data {
        conn_id: roam_wire::ConnectionId,
        channel_id: ChannelId,
        payload: Vec<u8>,
    },
    /// Send a Close message to end a channel.
    Close {
        conn_id: roam_wire::ConnectionId,
        channel_id: ChannelId,
    },
    /// Send a Response message (server-side call completed).
    Response {
        conn_id: roam_wire::ConnectionId,
        request_id: u64,
        /// Channel IDs for channels in the response (Tx/Rx returned by the method).
        channels: Vec<u64>,
        payload: Vec<u8>,
    },
    /// Request to open a new virtual connection.
    Connect {
        request_id: u64,
        metadata: roam_wire::Metadata,
        response_tx: OneshotSender<Result<ConnectionHandle, crate::ConnectError>>,
        /// Dispatcher for handling incoming requests on the virtual connection.
        /// If None, the connection can only make calls, not receive them.
        dispatcher: Option<Box<dyn ServiceDispatcher>>,
    },
    /// Internal watchdog signal to sweep stale pending responses.
    SweepPendingResponses,
}

/// Registry of active channels for a connection.
///
/// Handles incoming channels (Data from wire → `Rx<T>` / `Tx<T>` handles).
/// For outgoing channels (server `Tx<T>` args), spawned tasks drain receivers
/// and send Data/Close messages via `driver_tx`.
///
/// r[impl channeling.unknown] - Unknown channel IDs cause Goodbye.
pub struct ChannelRegistry {
    /// Connection ID this registry belongs to.
    conn_id: roam_wire::ConnectionId,

    /// Channels where we receive Data messages (backing `Rx<T>` or `Tx<T>` handles on our side).
    /// Key: channel_id, Value: sender to route Data payloads to the handle.
    incoming: HashMap<ChannelId, Sender<IncomingChannelMessage>>,

    /// Channel IDs that have been closed.
    /// Used to detect data-after-close violations.
    ///
    /// r[impl channeling.data-after-close] - Track closed channels.
    closed: HashSet<ChannelId>,

    // ========================================================================
    // Flow Control
    // ========================================================================
    /// r[impl flow.channel.credit-based] - Credit tracking for incoming channels.
    /// r[impl flow.channel.all-transports] - Flow control applies to all transports.
    /// This is the credit we've granted to the peer - bytes they can still send us.
    /// Decremented when we receive Data, incremented when we send Credit.
    incoming_credit: HashMap<ChannelId, u32>,

    /// r[impl flow.channel.credit-based] - Credit tracking for outgoing channels.
    /// r[impl flow.channel.all-transports] - Flow control applies to all transports.
    /// This is the credit peer granted us - bytes we can still send them.
    /// Decremented when we send Data, incremented when we receive Credit.
    outgoing_credit: HashMap<ChannelId, u32>,

    /// Initial credit to grant new channels.
    /// r[impl flow.channel.initial-credit] - Each channel starts with this credit.
    initial_credit: u32,

    /// Unified channel for all messages to the driver.
    /// The driver owns the receiving end and sends these on the wire.
    /// Using a single channel ensures FIFO ordering.
    driver_tx: Sender<DriverMessage>,

    /// Channel ID allocator for response channels created during dispatch.
    /// These are channels returned by service methods (e.g., `subscribe() -> Rx<Event>`).
    response_channel_ids: Arc<ChannelIdAllocator>,
    /// Optional diagnostics sink used for channel lifecycle tracking.
    diagnostic_state: Option<Arc<crate::diagnostic::DiagnosticState>>,
    /// Request currently being bound on this registry (server-side dispatch path).
    current_request_id: Option<u64>,
}

impl ChannelRegistry {
    /// Create a new registry with the given conn_id, initial credit, driver channel, and role.
    ///
    /// The `driver_tx` is used to send all messages (Call/Data/Close/Response)
    /// to the driver for transmission on the wire.
    ///
    /// The `role` determines channel ID parity for response channels:
    /// - Acceptor (server) uses even IDs
    /// - Initiator (client) uses odd IDs
    ///
    /// r[impl flow.channel.initial-credit] - Each channel starts with this credit.
    pub fn new_with_credit_and_role(
        conn_id: roam_wire::ConnectionId,
        initial_credit: u32,
        driver_tx: Sender<DriverMessage>,
        role: Role,
    ) -> Self {
        Self {
            conn_id,
            incoming: HashMap::new(),
            closed: HashSet::new(),
            incoming_credit: HashMap::new(),
            outgoing_credit: HashMap::new(),
            initial_credit,
            driver_tx,
            response_channel_ids: Arc::new(ChannelIdAllocator::new(role)),
            diagnostic_state: None,
            current_request_id: None,
        }
    }

    /// Create a new registry with the given initial credit and driver channel.
    /// Uses ROOT conn_id and Acceptor role for backward compatibility (server-side usage).
    ///
    /// r[impl flow.channel.initial-credit] - Each channel starts with this credit.
    pub fn new_with_credit(initial_credit: u32, driver_tx: Sender<DriverMessage>) -> Self {
        Self::new_with_credit_and_role(
            roam_wire::ConnectionId::ROOT,
            initial_credit,
            driver_tx,
            Role::Acceptor,
        )
    }

    /// Create a new registry with default infinite credit.
    ///
    /// r[impl flow.channel.infinite-credit] - Implementations MAY use very large credit.
    /// r[impl flow.channel.zero-credit] - With infinite credit, zero-credit never occurs.
    /// This disables backpressure but simplifies implementation.
    pub fn new(driver_tx: Sender<DriverMessage>) -> Self {
        Self::new_with_credit(u32::MAX, driver_tx)
    }

    /// Get the connection ID for this registry.
    pub fn conn_id(&self) -> roam_wire::ConnectionId {
        self.conn_id
    }

    /// Attach diagnostics state for channel open/close lifecycle recording.
    pub fn set_diagnostic_state(
        &mut self,
        diagnostic_state: Option<Arc<crate::diagnostic::DiagnosticState>>,
    ) {
        self.diagnostic_state = diagnostic_state;
    }

    /// Set request context used while binding channels during server dispatch.
    pub fn set_current_request_id(&mut self, request_id: Option<u64>) {
        self.current_request_id = request_id;
    }

    /// Get the dispatch context for response channel binding.
    ///
    /// Used by dispatch methods to set up task-local context so
    /// `roam::channel()` can create bound channels. The context should
    /// be passed to `DISPATCH_CONTEXT.scope()` in the async block.
    pub fn dispatch_context(&self) -> DispatchContext {
        DispatchContext {
            conn_id: self.conn_id,
            channel_ids: self.response_channel_ids.clone(),
            driver_tx: self.driver_tx.clone(),
        }
    }

    /// Get a clone of the driver message sender.
    ///
    /// Used by codegen to spawn tasks that send Data/Close/Response messages.
    pub fn driver_tx(&self) -> Sender<DriverMessage> {
        self.driver_tx.clone()
    }

    /// Get the response channel ID allocator.
    /// Used by ForwardingDispatcher to allocate downstream channel IDs for response channels.
    pub fn response_channel_ids(&self) -> Arc<ChannelIdAllocator> {
        self.response_channel_ids.clone()
    }

    /// Register an incoming channel.
    ///
    /// The connection layer will route Data messages for this channel_id to the sender.
    /// Used for both `Rx<T>` (caller receives from callee) and `Tx<T>` (callee sends to caller).
    ///
    /// r[impl flow.channel.initial-credit] - Channel starts with initial credit.
    pub fn register_incoming(&mut self, channel_id: ChannelId, tx: Sender<IncomingChannelMessage>) {
        if let Some(diag) = &self.diagnostic_state {
            diag.record_channel_open(
                channel_id,
                crate::diagnostic::ChannelDirection::Rx,
                self.current_request_id,
            );
        }
        self.incoming.insert(channel_id, tx);
        // Grant initial credit - peer can send us this many bytes
        self.incoming_credit.insert(channel_id, self.initial_credit);
    }

    /// Register credit tracking for an outgoing channel.
    ///
    /// The actual receiver is NOT stored here - the driver owns it directly.
    /// This only sets up credit tracking for the channel.
    ///
    /// r[impl flow.channel.initial-credit] - Channel starts with initial credit.
    pub fn register_outgoing_credit(&mut self, channel_id: ChannelId) {
        if let Some(diag) = &self.diagnostic_state {
            diag.record_channel_open(
                channel_id,
                crate::diagnostic::ChannelDirection::Tx,
                self.current_request_id,
            );
        }
        // Assume peer grants us initial credit - we can send them this many bytes
        self.outgoing_credit.insert(channel_id, self.initial_credit);
    }

    /// Route a Data message payload to the appropriate incoming channel.
    ///
    /// Returns Ok(()) if routed successfully, Err(ChannelError) otherwise.
    ///
    /// r[impl channeling.data] - Data messages routed by channel_id.
    /// r[impl channeling.data-after-close] - Reject data on closed channels.
    /// r[impl flow.channel.credit-overrun] - Reject if data exceeds remaining credit.
    /// r[impl flow.channel.credit-consume] - Deduct bytes from remaining credit.
    /// r[impl flow.channel.byte-accounting] - Credit measured in payload bytes.
    ///
    /// Returns a sender and payload if routing is allowed, or an error.
    /// The actual send must be done by the caller to avoid holding locks across await.
    pub fn prepare_route_data(
        &mut self,
        channel_id: ChannelId,
        payload: Vec<u8>,
    ) -> Result<(Sender<IncomingChannelMessage>, Vec<u8>), ChannelError> {
        // Check for data-after-close
        if self.closed.contains(&channel_id) {
            return Err(ChannelError::DataAfterClose);
        }

        // Check credit before routing
        // r[impl flow.channel.credit-overrun] - Reject if exceeds credit
        let payload_len = payload.len() as u32;
        if let Some(credit) = self.incoming_credit.get_mut(&channel_id) {
            if payload_len > *credit {
                return Err(ChannelError::CreditOverrun);
            }
            // r[impl flow.channel.credit-consume] - Deduct from credit
            *credit -= payload_len;
        }
        // Note: if no credit entry exists, the channel may not be registered yet
        // (e.g., Rx channel created by callee). In that case, skip credit check.

        if let Some(tx) = self.incoming.get(&channel_id) {
            Ok((tx.clone(), payload))
        } else {
            Err(ChannelError::Unknown)
        }
    }

    /// Route a Data message payload to the appropriate incoming channel.
    ///
    /// Returns Ok(()) if routed successfully, Err(ChannelError) otherwise.
    ///
    /// r[impl channeling.data] - Data messages routed by channel_id.
    /// r[impl channeling.data-after-close] - Reject data on closed channels.
    /// r[impl flow.channel.credit-overrun] - Reject if data exceeds remaining credit.
    /// r[impl flow.channel.credit-consume] - Deduct bytes from remaining credit.
    /// r[impl flow.channel.byte-accounting] - Credit measured in payload bytes.
    pub async fn route_data(
        &mut self,
        channel_id: ChannelId,
        payload: Vec<u8>,
    ) -> Result<(), ChannelError> {
        let (tx, payload) = self.prepare_route_data(channel_id, payload)?;
        // If send fails, the Rx<T> was dropped - that's okay, just drop the data
        let _ = tx.send(IncomingChannelMessage::Data(payload)).await;
        Ok(())
    }

    /// Close an incoming channel (remove from registry).
    ///
    /// Dropping the sender will cause the `Rx<T>`'s recv() to return None.
    ///
    /// r[impl channeling.close] - Close terminates the channel.
    /// r[impl flow.channel.close-exempt] - Close doesn't consume credit.
    pub fn close(&mut self, channel_id: ChannelId) {
        if let Some(tx) = self.incoming.remove(&channel_id) {
            // Best-effort delivery of explicit Close to preserve channel semantics.
            match tx.try_send(IncomingChannelMessage::Close) {
                Ok(()) => {}
                Err(_e) => {
                    crate::runtime::spawn("roam_channel_close_fallback", async move {
                        let _ = tx.send(IncomingChannelMessage::Close).await;
                    });
                }
            }
        }
        self.incoming_credit.remove(&channel_id);
        self.outgoing_credit.remove(&channel_id);
        self.closed.insert(channel_id);
    }

    /// Reset a channel (remove from registry, discard credit).
    ///
    /// r[impl channeling.reset] - Reset terminates the channel abruptly.
    /// r[impl channeling.reset.credit] - Outstanding credit is lost on reset.
    pub fn reset(&mut self, channel_id: ChannelId) {
        self.incoming.remove(&channel_id);
        self.incoming_credit.remove(&channel_id);
        self.outgoing_credit.remove(&channel_id);
        self.closed.insert(channel_id);
    }

    /// Receive a Credit message - add credit for an outgoing channel.
    ///
    /// r[impl flow.channel.credit-grant] - Credit message adds to available credit.
    /// r[impl flow.channel.credit-additive] - Credit accumulates additively.
    pub fn receive_credit(&mut self, channel_id: ChannelId, bytes: u32) {
        if let Some(credit) = self.outgoing_credit.get_mut(&channel_id) {
            // r[impl flow.channel.credit-additive] - Add to existing credit
            *credit = credit.saturating_add(bytes);
        }
        // If no entry, channel may be closed or unknown - ignore
    }

    /// Check if a channel ID is registered (either incoming or outgoing credit).
    pub fn contains(&self, channel_id: ChannelId) -> bool {
        self.incoming.contains_key(&channel_id) || self.outgoing_credit.contains_key(&channel_id)
    }

    /// Check if a channel ID is registered as incoming.
    pub fn contains_incoming(&self, channel_id: ChannelId) -> bool {
        self.incoming.contains_key(&channel_id)
    }

    /// Check if a channel ID has outgoing credit registered.
    pub fn contains_outgoing(&self, channel_id: ChannelId) -> bool {
        self.outgoing_credit.contains_key(&channel_id)
    }

    /// Check if a channel has been closed.
    pub fn is_closed(&self, channel_id: ChannelId) -> bool {
        self.closed.contains(&channel_id)
    }

    /// Get the number of active outgoing channels (by credit tracking).
    pub fn outgoing_count(&self) -> usize {
        self.outgoing_credit.len()
    }

    /// Get remaining credit for an outgoing channel.
    ///
    /// Returns None if channel is not registered.
    pub fn outgoing_credit(&self, channel_id: ChannelId) -> Option<u32> {
        self.outgoing_credit.get(&channel_id).copied()
    }

    /// Get remaining credit we've granted for an incoming channel.
    ///
    /// Returns None if channel is not registered.
    pub fn incoming_credit(&self, channel_id: ChannelId) -> Option<u32> {
        self.incoming_credit.get(&channel_id).copied()
    }

    /// Bind channels in deserialized args for server-side dispatch.
    ///
    /// Walks the args using Poke reflection to find any `Rx<T>` or `Tx<T>` fields.
    /// For each channel found:
    /// - For `Rx<T>`: creates a channel, sets the receiver slot, registers for incoming data
    /// - For `Tx<T>`: sets the task_tx so send() writes directly to the wire
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut args = facet_postcard::from_slice::<(Rx<i32>, Tx<String>)>(&payload)?;
    /// registry.bind_channels(&mut args);
    /// let (input, output) = args;
    /// // ... call handler with input, output ...
    /// // When handler returns and Tx is dropped, Close is sent automatically
    /// ```
    ///
    /// The `plan` should be created once per type as a static in non-generic code.
    pub fn bind_channels<T: Facet<'static>>(&mut self, args: &mut T, plan: &crate::RpcPlan) {
        let args_ptr = args as *mut T as *mut ();
        // SAFETY: args is valid and initialized
        #[allow(unsafe_code)]
        unsafe {
            self.bind_channels_with_plan(args_ptr, plan);
        }
    }

    /// Bind channels using a precomputed RpcPlan.
    ///
    /// Iterates over precomputed channel locations and binds each Rx/Tx channel.
    ///
    /// # Safety
    ///
    /// - `args_ptr` must point to valid, initialized memory matching `args_shape`
    #[allow(unsafe_code)]
    pub(crate) unsafe fn bind_channels_with_plan(
        &mut self,
        args_ptr: *mut (),
        plan: &crate::RpcPlan,
    ) {
        let shape = plan.type_plan.root().shape;
        for loc in &plan.channel_locations {
            // SAFETY: args_ptr is valid and initialized
            let poke = unsafe {
                facet::Poke::from_raw_parts(facet_core::PtrMut::new(args_ptr.cast::<u8>()), shape)
            };
            match poke.at_path_mut(&loc.path) {
                Ok(channel_poke) => match loc.kind {
                    crate::ChannelKind::Rx => {
                        trace!("bind_channels_with_plan: found Rx");
                        self.bind_rx_channel(channel_poke);
                    }
                    crate::ChannelKind::Tx => {
                        trace!("bind_channels_with_plan: found Tx");
                        self.bind_tx_channel(channel_poke);
                    }
                },
                Err(facet_path::PathAccessError::OptionIsNone { .. }) => {
                    // Option<Rx/Tx> is None — skip this channel location
                }
                Err(_e) => {
                    warn!("bind_channels_with_plan: unexpected path error: {_e}");
                }
            }
        }
    }

    /// Bind an Rx<T> channel for server-side dispatch.
    ///
    /// Server receives data from client on this channel.
    /// Creates a channel, sets the receiver slot, registers the sender for routing.
    fn bind_rx_channel(&mut self, poke: facet::Poke<'_, '_>) {
        if let Ok(mut ps) = poke.into_struct() {
            // Get the channel_id that was deserialized from the wire
            let channel_id = if let Ok(channel_id_field) = ps.field_by_name("channel_id")
                && let Ok(id_ref) = channel_id_field.get::<ChannelId>()
            {
                *id_ref
            } else {
                warn!("bind_rx_channel: could not get channel_id field");
                return;
            };

            trace!(channel_id, "bind_rx_channel: registering incoming channel");

            // Create channel and set receiver slot
            let (tx, rx) = crate::runtime::channel("rx_stream_bind", RX_STREAM_BUFFER_SIZE);

            if let Ok(mut receiver_field) = ps.field_by_name("receiver")
                && let Ok(slot) = receiver_field.get_mut::<ReceiverSlot>()
            {
                slot.set(rx);
            }

            // Register for incoming data routing
            self.register_incoming(channel_id, tx);
            trace!(channel_id, "bind_rx_channel: channel registered");
        } else {
            warn!("bind_rx_channel: could not convert poke to struct");
        }
    }

    /// Bind a Tx<T> channel for server-side dispatch.
    ///
    /// Server sends data to client on this channel.
    /// Sets the conn_id and driver_tx so Tx::send() writes DriverMessage::Data to the wire.
    /// When the Tx is dropped, it sends DriverMessage::Close automatically.
    fn bind_tx_channel(&mut self, poke: facet::Poke<'_, '_>) {
        if let Ok(mut ps) = poke.into_struct() {
            // Set conn_id so Data/Close messages go to the correct virtual connection
            // r[impl core.conn.independence]
            if let Ok(mut conn_id_field) = ps.field_by_name("conn_id")
                && let Ok(id_ref) = conn_id_field.get_mut::<roam_wire::ConnectionId>()
            {
                *id_ref = self.conn_id;
            }

            // Set driver_tx so Tx::send() can write directly to the wire
            if let Ok(mut driver_tx_field) = ps.field_by_name("driver_tx")
                && let Ok(slot) = driver_tx_field.get_mut::<DriverTxSlot>()
            {
                slot.set(self.driver_tx.clone());
            }
        }
    }

    /// Snapshot current per-channel credit state for diagnostics.
    pub fn snapshot_credits(&self) -> Vec<crate::diagnostic::ChannelCreditInfo> {
        // Collect all channel IDs that have either incoming or outgoing credit
        let mut channel_ids: std::collections::HashSet<u64> = std::collections::HashSet::new();
        channel_ids.extend(self.incoming_credit.keys());
        channel_ids.extend(self.outgoing_credit.keys());

        let mut result: Vec<crate::diagnostic::ChannelCreditInfo> = channel_ids
            .into_iter()
            .map(|ch_id| crate::diagnostic::ChannelCreditInfo {
                channel_id: ch_id,
                incoming_credit: self.incoming_credit.get(&ch_id).copied().unwrap_or(0),
                outgoing_credit: self.outgoing_credit.get(&ch_id).copied().unwrap_or(0),
            })
            .collect();
        result.sort_by_key(|c| c.channel_id);
        result
    }
}
