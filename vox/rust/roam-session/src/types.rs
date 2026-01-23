use facet::Facet;

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
        metadata: Vec<(String, roam_wire::MetadataValue)>,
        /// Channel IDs used by this call (Tx/Rx), in declaration order.
        channels: Vec<u64>,
        payload: Vec<u8>,
        response_tx: OneshotSender<Result<ResponseData, TransportError>>,
    },
    /// Send a Data message on a stream.
    Data {
        conn_id: roam_wire::ConnectionId,
        channel_id: ChannelId,
        payload: Vec<u8>,
    },
    /// Send a Close message to end a stream.
    Close {
        conn_id: roam_wire::ConnectionId,
        channel_id: ChannelId,
    },
    /// Send a Response message (server-side call completed).
    Response {
        conn_id: roam_wire::ConnectionId,
        request_id: u64,
        /// Channel IDs for streams in the response (Tx/Rx returned by the method).
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
}

/// Registry of active streams for a connection.
///
/// Handles incoming streams (Data from wire → `Rx<T>` / `Tx<T>` handles).
/// For outgoing streams (server `Tx<T>` args), spawned tasks drain receivers
/// and send Data/Close messages via `driver_tx`.
///
/// r[impl channeling.unknown] - Unknown stream IDs cause Goodbye.
pub struct ChannelRegistry {
    /// Connection ID this registry belongs to.
    conn_id: roam_wire::ConnectionId,

    /// Streams where we receive Data messages (backing `Rx<T>` or `Tx<T>` handles on our side).
    /// Key: channel_id, Value: sender to route Data payloads to the handle.
    incoming: HashMap<ChannelId, Sender<Vec<u8>>>,

    /// Stream IDs that have been closed.
    /// Used to detect data-after-close violations.
    ///
    /// r[impl channeling.data-after-close] - Track closed streams.
    closed: HashSet<ChannelId>,

    // ========================================================================
    // Flow Control
    // ========================================================================
    /// r[impl flow.channel.credit-based] - Credit tracking for incoming streams.
    /// r[impl flow.channel.all-transports] - Flow control applies to all transports.
    /// This is the credit we've granted to the peer - bytes they can still send us.
    /// Decremented when we receive Data, incremented when we send Credit.
    incoming_credit: HashMap<ChannelId, u32>,

    /// r[impl flow.channel.credit-based] - Credit tracking for outgoing streams.
    /// r[impl flow.channel.all-transports] - Flow control applies to all transports.
    /// This is the credit peer granted us - bytes we can still send them.
    /// Decremented when we send Data, incremented when we receive Credit.
    outgoing_credit: HashMap<ChannelId, u32>,

    /// Initial credit to grant new streams.
    /// r[impl flow.channel.initial-credit] - Each stream starts with this credit.
    initial_credit: u32,

    /// Unified channel for all messages to the driver.
    /// The driver owns the receiving end and sends these on the wire.
    /// Using a single channel ensures FIFO ordering.
    driver_tx: Sender<DriverMessage>,

    /// Channel ID allocator for response channels created during dispatch.
    /// These are channels returned by service methods (e.g., `subscribe() -> Rx<Event>`).
    response_channel_ids: Arc<ChannelIdAllocator>,
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
    /// r[impl flow.channel.initial-credit] - Each stream starts with this credit.
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
        }
    }

    /// Create a new registry with the given initial credit and driver channel.
    /// Uses ROOT conn_id and Acceptor role for backward compatibility (server-side usage).
    ///
    /// r[impl flow.channel.initial-credit] - Each stream starts with this credit.
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

    /// Get the dispatch context for response channel binding.
    ///
    /// Used by `dispatch_call` and `dispatch_call_infallible` to set up
    /// thread-local context so `roam::channel()` can create bound channels.
    pub(crate) fn dispatch_context(&self) -> DispatchContext {
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

    /// Register an incoming stream.
    ///
    /// The connection layer will route Data messages for this channel_id to the sender.
    /// Used for both `Rx<T>` (caller receives from callee) and `Tx<T>` (callee sends to caller).
    ///
    /// r[impl flow.channel.initial-credit] - Stream starts with initial credit.
    pub fn register_incoming(&mut self, channel_id: ChannelId, tx: Sender<Vec<u8>>) {
        self.incoming.insert(channel_id, tx);
        // Grant initial credit - peer can send us this many bytes
        self.incoming_credit.insert(channel_id, self.initial_credit);
    }

    /// Register credit tracking for an outgoing stream.
    ///
    /// The actual receiver is NOT stored here - the driver owns it directly.
    /// This only sets up credit tracking for the stream.
    ///
    /// r[impl flow.channel.initial-credit] - Stream starts with initial credit.
    pub fn register_outgoing_credit(&mut self, channel_id: ChannelId) {
        // Assume peer grants us initial credit - we can send them this many bytes
        self.outgoing_credit.insert(channel_id, self.initial_credit);
    }

    /// Route a Data message payload to the appropriate incoming stream.
    ///
    /// Returns Ok(()) if routed successfully, Err(ChannelError) otherwise.
    ///
    /// r[impl channeling.data] - Data messages routed by channel_id.
    /// r[impl channeling.data-after-close] - Reject data on closed streams.
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
    ) -> Result<(Sender<Vec<u8>>, Vec<u8>), ChannelError> {
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
        // Note: if no credit entry exists, the stream may not be registered yet
        // (e.g., Rx stream created by callee). In that case, skip credit check.

        if let Some(tx) = self.incoming.get(&channel_id) {
            Ok((tx.clone(), payload))
        } else {
            Err(ChannelError::Unknown)
        }
    }

    /// Route a Data message payload to the appropriate incoming stream.
    ///
    /// Returns Ok(()) if routed successfully, Err(ChannelError) otherwise.
    ///
    /// r[impl channeling.data] - Data messages routed by channel_id.
    /// r[impl channeling.data-after-close] - Reject data on closed streams.
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
        let _ = tx.send(payload).await;
        Ok(())
    }

    /// Close an incoming stream (remove from registry).
    ///
    /// Dropping the sender will cause the `Rx<T>`'s recv() to return None.
    ///
    /// r[impl channeling.close] - Close terminates the stream.
    /// r[impl flow.channel.close-exempt] - Close doesn't consume credit.
    pub fn close(&mut self, channel_id: ChannelId) {
        self.incoming.remove(&channel_id);
        self.incoming_credit.remove(&channel_id);
        self.outgoing_credit.remove(&channel_id);
        self.closed.insert(channel_id);
    }

    /// Reset a stream (remove from registry, discard credit).
    ///
    /// r[impl channeling.reset] - Reset terminates the stream abruptly.
    /// r[impl channeling.reset.credit] - Outstanding credit is lost on reset.
    pub fn reset(&mut self, channel_id: ChannelId) {
        self.incoming.remove(&channel_id);
        self.incoming_credit.remove(&channel_id);
        self.outgoing_credit.remove(&channel_id);
        self.closed.insert(channel_id);
    }

    /// Receive a Credit message - add credit for an outgoing stream.
    ///
    /// r[impl flow.channel.credit-grant] - Credit message adds to available credit.
    /// r[impl flow.channel.credit-additive] - Credit accumulates additively.
    pub fn receive_credit(&mut self, channel_id: ChannelId, bytes: u32) {
        if let Some(credit) = self.outgoing_credit.get_mut(&channel_id) {
            // r[impl flow.channel.credit-additive] - Add to existing credit
            *credit = credit.saturating_add(bytes);
        }
        // If no entry, stream may be closed or unknown - ignore
    }

    /// Check if a stream ID is registered (either incoming or outgoing credit).
    pub fn contains(&self, channel_id: ChannelId) -> bool {
        self.incoming.contains_key(&channel_id) || self.outgoing_credit.contains_key(&channel_id)
    }

    /// Check if a stream ID is registered as incoming.
    pub fn contains_incoming(&self, channel_id: ChannelId) -> bool {
        self.incoming.contains_key(&channel_id)
    }

    /// Check if a stream ID has outgoing credit registered.
    pub fn contains_outgoing(&self, channel_id: ChannelId) -> bool {
        self.outgoing_credit.contains_key(&channel_id)
    }

    /// Check if a stream has been closed.
    pub fn is_closed(&self, channel_id: ChannelId) -> bool {
        self.closed.contains(&channel_id)
    }

    /// Get the number of active outgoing streams (by credit tracking).
    pub fn outgoing_count(&self) -> usize {
        self.outgoing_credit.len()
    }

    /// Get remaining credit for an outgoing stream.
    ///
    /// Returns None if stream is not registered.
    pub fn outgoing_credit(&self, channel_id: ChannelId) -> Option<u32> {
        self.outgoing_credit.get(&channel_id).copied()
    }

    /// Get remaining credit we've granted for an incoming stream.
    ///
    /// Returns None if stream is not registered.
    pub fn incoming_credit(&self, channel_id: ChannelId) -> Option<u32> {
        self.incoming_credit.get(&channel_id).copied()
    }

    /// Bind streams in deserialized args for server-side dispatch.
    ///
    /// Walks the args using Poke reflection to find any `Rx<T>` or `Tx<T>` fields.
    /// For each stream found:
    /// - For `Rx<T>`: creates a channel, sets the receiver slot, registers for incoming data
    /// - For `Tx<T>`: sets the task_tx so send() writes directly to the wire
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut args = facet_postcard::from_slice::<(Rx<i32>, Tx<String>)>(&payload)?;
    /// registry.bind_streams(&mut args);
    /// let (input, output) = args;
    /// // ... call handler with input, output ...
    /// // When handler returns and Tx is dropped, Close is sent automatically
    /// ```
    pub fn bind_streams<T: Facet<'static>>(&mut self, args: &mut T) {
        let poke = facet::Poke::new(args);
        self.bind_streams_recursive(poke);
    }

    /// Recursively walk a Poke value looking for Rx/Tx streams to bind.
    #[allow(unsafe_code)]
    fn bind_streams_recursive(&mut self, mut poke: facet::Poke<'_, '_>) {
        use facet::Def;

        let shape = poke.shape();

        // Check if this is an Rx or Tx type
        if shape.decl_id == crate::Rx::<()>::SHAPE.decl_id {
            debug!("bind_streams_recursive: found Rx, binding");
            self.bind_rx_stream(poke);
            return;
        } else if shape.decl_id == crate::Tx::<()>::SHAPE.decl_id {
            debug!("bind_streams_recursive: found Tx, binding");
            self.bind_tx_stream(poke);
            return;
        }

        // Dispatch based on the shape's definition
        match shape.def {
            Def::Scalar => {}

            // Recurse into struct/tuple fields
            _ if poke.is_struct() => {
                let mut ps = poke.into_struct().expect("is_struct was true");
                let field_count = ps.field_count();
                trace!(field_count, "bind_streams_recursive: recursing into struct");
                for i in 0..field_count {
                    if let Ok(field_poke) = ps.field(i) {
                        self.bind_streams_recursive(field_poke);
                    }
                }
            }

            // Recurse into Option<T>
            Def::Option(_) => {
                // Option is represented as an enum, use into_enum to access its value
                if let Ok(mut pe) = poke.into_enum()
                    && let Ok(Some(inner_poke)) = pe.field(0)
                {
                    self.bind_streams_recursive(inner_poke);
                }
            }

            // Recurse into list elements (e.g., Vec<Tx<T>>)
            Def::List(list_def) => {
                let len = {
                    let peek = poke.as_peek();
                    peek.into_list().map(|pl| pl.len()).unwrap_or(0)
                };
                // Get mutable access to elements via VTable (no PokeList exists)
                if let Some(get_mut_fn) = list_def.vtable.get_mut {
                    let element_shape = list_def.t;
                    let data_ptr = poke.data_mut();
                    for i in 0..len {
                        // SAFETY: We have exclusive mutable access via poke, index < len, shape is correct
                        let element_ptr = unsafe { (get_mut_fn)(data_ptr, i, element_shape) };
                        if let Some(ptr) = element_ptr {
                            // SAFETY: ptr points to a valid element with the correct shape
                            let element_poke =
                                unsafe { facet::Poke::from_raw_parts(ptr, element_shape) };
                            self.bind_streams_recursive(element_poke);
                        }
                    }
                }
            }

            // Other enum variants
            _ if poke.is_enum() => {
                if let Ok(mut pe) = poke.into_enum()
                    && let Ok(Some(variant_poke)) = pe.field(0)
                {
                    self.bind_streams_recursive(variant_poke);
                }
            }

            _ => {}
        }
    }

    /// Bind an Rx<T> stream for server-side dispatch.
    ///
    /// Server receives data from client on this stream.
    /// Creates a channel, sets the receiver slot, registers the sender for routing.
    fn bind_rx_stream(&mut self, poke: facet::Poke<'_, '_>) {
        if let Ok(mut ps) = poke.into_struct() {
            // Get the channel_id that was deserialized from the wire
            let channel_id = if let Ok(channel_id_field) = ps.field_by_name("channel_id")
                && let Ok(id_ref) = channel_id_field.get::<ChannelId>()
            {
                *id_ref
            } else {
                warn!("bind_rx_stream: could not get channel_id field");
                return;
            };

            debug!(channel_id, "bind_rx_stream: registering incoming channel");

            // Create channel and set receiver slot
            let (tx, rx) = crate::runtime::channel(RX_STREAM_BUFFER_SIZE);

            if let Ok(mut receiver_field) = ps.field_by_name("receiver")
                && let Ok(slot) = receiver_field.get_mut::<ReceiverSlot>()
            {
                slot.set(rx);
            }

            // Register for incoming data routing
            self.register_incoming(channel_id, tx);
            debug!(channel_id, "bind_rx_stream: channel registered");
        } else {
            warn!("bind_rx_stream: could not convert poke to struct");
        }
    }

    /// Bind a Tx<T> stream for server-side dispatch.
    ///
    /// Server sends data to client on this stream.
    /// Sets the conn_id and driver_tx so Tx::send() writes DriverMessage::Data to the wire.
    /// When the Tx is dropped, it sends DriverMessage::Close automatically.
    fn bind_tx_stream(&mut self, poke: facet::Poke<'_, '_>) {
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
}
