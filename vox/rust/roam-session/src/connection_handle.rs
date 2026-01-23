use std::sync::Arc;

use facet::Facet;

use crate::{
    ChannelError, ChannelId, ChannelIdAllocator, ChannelRegistry, DriverMessage, HandleShared,
    RX_STREAM_BUFFER_SIZE, ReceiverSlot, RequestIdGenerator, ResponseData, SenderSlot,
    ServiceDispatcher, TransportError, collect_channel_ids, diagnostic, patch_channel_ids,
    runtime::oneshot,
};
use crate::{
    Role,
    runtime::{Receiver, Sender},
};

/// Handle for making outgoing RPC calls.
///
/// This is the client-side API. It can be cloned and used from multiple tasks.
/// The actual I/O is driven by the `Driver` future which must be spawned.
///
/// # Example
///
/// ```ignore
/// let (handle, driver) = establish_connection(transport, dispatcher).await?;
/// tokio::spawn(driver);
///
/// // Use handle to make calls
/// let response = handle.call_raw(method_id, payload).await?;
/// ```
#[derive(Clone)]
pub struct ConnectionHandle {
    shared: Arc<HandleShared>,
}

impl ConnectionHandle {
    /// Create a new handle for the root connection (conn_id = 0).
    ///
    /// All messages (Call/Data/Close/Response) go through a single unified channel
    /// to ensure FIFO ordering.
    pub fn new(driver_tx: Sender<DriverMessage>, role: Role, initial_credit: u32) -> Self {
        Self::new_with_diagnostics(
            roam_wire::ConnectionId::ROOT,
            driver_tx,
            role,
            initial_credit,
            None,
        )
    }

    /// Create a new handle with a specific connection ID and optional diagnostic state.
    ///
    /// If `diagnostic_state` is provided, all RPC calls and channels will be tracked
    /// for debugging purposes.
    pub fn new_with_diagnostics(
        conn_id: roam_wire::ConnectionId,
        driver_tx: Sender<DriverMessage>,
        role: Role,
        initial_credit: u32,
        diagnostic_state: Option<Arc<crate::diagnostic::DiagnosticState>>,
    ) -> Self {
        let channel_registry = ChannelRegistry::new_with_credit(initial_credit, driver_tx.clone());
        Self {
            shared: Arc::new(HandleShared {
                conn_id,
                driver_tx,
                request_ids: RequestIdGenerator::new(),
                channel_ids: ChannelIdAllocator::new(role),
                channel_registry: std::sync::Mutex::new(channel_registry),
                diagnostic_state,
            }),
        }
    }

    /// Get the connection ID for this handle.
    pub fn conn_id(&self) -> roam_wire::ConnectionId {
        self.shared.conn_id
    }

    /// Get the diagnostic state, if any.
    pub fn diagnostic_state(&self) -> Option<&Arc<crate::diagnostic::DiagnosticState>> {
        self.shared.diagnostic_state.as_ref()
    }

    /// Make a typed RPC call with automatic serialization and stream binding.
    ///
    /// Walks the args using Poke reflection to find any `Rx<T>` or `Tx<T>` fields,
    /// binds stream IDs, and sets up the stream infrastructure before serialization.
    ///
    /// # Arguments
    ///
    /// * `method_id` - The method ID to call
    /// * `args` - Arguments to serialize (typically a tuple of all method args).
    ///   Must be mutable so stream IDs can be assigned.
    ///
    /// # Stream Binding
    ///
    /// For `Rx<T>` in args (caller passes receiver, keeps sender to push data):
    /// - Allocates a stream ID
    /// - Takes the receiver and spawns a task to drain it, sending Data messages
    /// - The caller keeps the `Tx<T>` from `roam::channel()` to send values
    ///
    /// For `Tx<T>` in args (caller passes sender, keeps receiver to pull data):
    /// - Allocates a stream ID
    /// - Takes the sender and registers for incoming Data routing
    /// - The caller keeps the `Rx<T>` from `roam::channel()` to receive values
    ///
    /// # Example
    ///
    /// ```ignore
    /// // For a streaming method sum(numbers: Rx<i32>) -> i64
    /// let (tx, rx) = roam::channel::<i32>();
    /// let response = handle.call(method_id::SUM, &mut (rx,)).await?;
    /// // tx.send(&42).await to push values
    /// ```
    /// Make an RPC call with default (empty) metadata.
    pub async fn call<T: Facet<'static>>(
        &self,
        method_id: u64,
        args: &mut T,
    ) -> Result<ResponseData, TransportError> {
        self.call_with_metadata(method_id, args, roam_wire::Metadata::default())
            .await
    }

    /// Make an RPC call with custom metadata.
    pub async fn call_with_metadata<T: Facet<'static>>(
        &self,
        method_id: u64,
        args: &mut T,
        metadata: roam_wire::Metadata,
    ) -> Result<ResponseData, TransportError> {
        // Walk args and bind any streams (allocates channel IDs)
        // This collects receivers that need to be drained but does NOT spawn
        let mut drains = Vec::new();
        trace!("ConnectionHandle::call: binding streams");
        self.bind_streams(args, &mut drains);

        // Collect channel IDs for the Request message
        let channels = collect_channel_ids(args);
        trace!(
            channels = ?channels,
            drain_count = drains.len(),
            "ConnectionHandle::call: collected channels after bind_streams"
        );

        let payload = facet_postcard::to_vec(args).map_err(TransportError::Encode)?;

        // Generate args debug info for diagnostics when enabled
        let args_debug = if diagnostic::debug_enabled() {
            Some(
                facet_pretty::PrettyPrinter::new()
                    .with_colors(facet_pretty::ColorMode::Never)
                    .with_max_content_len(64)
                    .format(args),
            )
        } else {
            None
        };

        if drains.is_empty() {
            // No Rx streams - simple call
            self.call_raw_with_channels_and_metadata(
                method_id, channels, payload, args_debug, metadata,
            )
            .await
        } else {
            // Has Rx streams - spawn tasks to drain them
            // IMPORTANT: We must send Request BEFORE spawning drain tasks to ensure ordering.
            // We need to actually send the DriverMessage::Call to the driver's queue
            // before spawning drains, not just create the future.
            let request_id = self.shared.request_ids.next();
            let (response_tx, response_rx) = oneshot();

            // Track outgoing request for diagnostics
            if let Some(diag) = &self.shared.diagnostic_state {
                let args = args_debug.map(|s| {
                    let mut map = std::collections::HashMap::new();
                    map.insert("args".to_string(), s);
                    map
                });
                diag.record_outgoing_request(request_id, method_id, args);
                // Associate channels with this request
                diag.associate_channels_with_request(&channels, request_id);
            }

            let msg = DriverMessage::Call {
                conn_id: self.shared.conn_id,
                request_id,
                method_id,
                metadata,
                channels,
                payload,
                response_tx,
            };

            // Send the Call message NOW, before spawning drain tasks
            if self.shared.driver_tx.send(msg).await.is_err() {
                return Err(TransportError::DriverGone);
            }

            let task_tx = self.shared.channel_registry.lock().unwrap().driver_tx();
            let conn_id = self.shared.conn_id;

            // Spawn a task for each drain to forward data to driver
            for (channel_id, mut rx) in drains {
                let task_tx = task_tx.clone();
                crate::runtime::spawn(async move {
                    loop {
                        match rx.recv().await {
                            Some(payload) => {
                                debug!(
                                    "drain task: received {} bytes on channel {}",
                                    payload.len(),
                                    channel_id
                                );
                                // Send data to driver
                                let _ = task_tx
                                    .send(DriverMessage::Data {
                                        conn_id,
                                        channel_id,
                                        payload,
                                    })
                                    .await;
                                debug!(
                                    "drain task: sent DriverMessage::Data for channel {}",
                                    channel_id
                                );
                            }
                            None => {
                                debug!("drain task: channel {} closed", channel_id);
                                // Channel closed, send Close and exit
                                let _ = task_tx
                                    .send(DriverMessage::Close {
                                        conn_id,
                                        channel_id,
                                    })
                                    .await;
                                debug!(
                                    "drain task: sent DriverMessage::Close for channel {}",
                                    channel_id
                                );
                                break;
                            }
                        }
                    }
                });
            }

            // Just await the response - drain tasks run independently
            let result = response_rx
                .await
                .map_err(|_| TransportError::DriverGone)?
                .map_err(|_| TransportError::ConnectionClosed);

            // Mark request as complete
            if let Some(diag) = &self.shared.diagnostic_state {
                diag.complete_request(request_id);
            }

            result
        }
    }

    /// Walk args and bind any Rx<T> or Tx<T> streams.
    /// Collects (channel_id, receiver) pairs for Rx streams that need draining.
    fn bind_streams<T: Facet<'static>>(
        &self,
        args: &mut T,
        drains: &mut Vec<(ChannelId, Receiver<Vec<u8>>)>,
    ) {
        let poke = facet::Poke::new(args);
        self.bind_streams_recursive(poke, drains);
    }

    /// Recursively walk a Poke value looking for Rx/Tx streams to bind.
    #[allow(unsafe_code)]
    fn bind_streams_recursive(
        &self,
        mut poke: facet::Poke<'_, '_>,
        drains: &mut Vec<(ChannelId, Receiver<Vec<u8>>)>,
    ) {
        use facet::Def;

        let shape = poke.shape();

        // Check if this is an Rx or Tx type
        if shape.decl_id == crate::Rx::<()>::SHAPE.decl_id {
            self.bind_rx_stream(poke, drains);
            return;
        } else if shape.decl_id == crate::Tx::<()>::SHAPE.decl_id {
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
                for i in 0..field_count {
                    if let Ok(field_poke) = ps.field(i) {
                        self.bind_streams_recursive(field_poke, drains);
                    }
                }
            }

            // Recurse into Option<T>
            Def::Option(_) => {
                // Option is represented as an enum, use into_enum to access its value
                if let Ok(mut pe) = poke.into_enum()
                    && let Ok(Some(inner_poke)) = pe.field(0)
                {
                    self.bind_streams_recursive(inner_poke, drains);
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
                            self.bind_streams_recursive(element_poke, drains);
                        }
                    }
                }
            }

            // Other enum variants
            _ if poke.is_enum() => {
                if let Ok(mut pe) = poke.into_enum()
                    && let Ok(Some(variant_poke)) = pe.field(0)
                {
                    self.bind_streams_recursive(variant_poke, drains);
                }
            }

            _ => {}
        }
    }

    /// Bind an Rx<T> stream - caller passes receiver, keeps sender.
    /// Collects the receiver for draining (no spawning).
    fn bind_rx_stream(
        &self,
        poke: facet::Poke<'_, '_>,
        drains: &mut Vec<(ChannelId, Receiver<Vec<u8>>)>,
    ) {
        let channel_id = self.alloc_channel_id();
        debug!(
            channel_id,
            "OutgoingBinder::bind_rx_stream: allocated channel_id for Rx"
        );

        if let Ok(mut ps) = poke.into_struct() {
            // Set channel_id field by getting mutable access to the u64
            if let Ok(mut channel_id_field) = ps.field_by_name("channel_id")
                && let Ok(id_ref) = channel_id_field.get_mut::<ChannelId>()
            {
                debug!(
                    old_id = *id_ref,
                    new_id = channel_id,
                    "OutgoingBinder::bind_rx_stream: overwriting channel_id"
                );
                *id_ref = channel_id;
            }

            // Take the receiver from ReceiverSlot - collect for draining later
            if let Ok(mut receiver_field) = ps.field_by_name("receiver")
                && let Ok(slot) = receiver_field.get_mut::<ReceiverSlot>()
                && let Some(rx) = slot.take()
            {
                debug!(
                    channel_id,
                    "OutgoingBinder::bind_rx_stream: took receiver, adding to drains"
                );
                drains.push((channel_id, rx));
            }
        }
    }

    /// Bind a Tx<T> stream - caller passes sender, keeps receiver.
    /// We take the sender and register for incoming Data routing.
    fn bind_tx_stream(&self, poke: facet::Poke<'_, '_>) {
        let channel_id = self.alloc_channel_id();
        debug!(
            channel_id,
            "OutgoingBinder::bind_tx_stream: allocated channel_id for Tx"
        );

        if let Ok(mut ps) = poke.into_struct() {
            // Set channel_id field by getting mutable access to the u64
            if let Ok(mut channel_id_field) = ps.field_by_name("channel_id")
                && let Ok(id_ref) = channel_id_field.get_mut::<ChannelId>()
            {
                debug!(
                    old_id = *id_ref,
                    new_id = channel_id,
                    "OutgoingBinder::bind_tx_stream: overwriting channel_id"
                );
                *id_ref = channel_id;
            }

            // Take the sender from SenderSlot
            if let Ok(mut sender_field) = ps.field_by_name("sender")
                && let Ok(slot) = sender_field.get_mut::<SenderSlot>()
                && let Some(tx) = slot.take()
            {
                debug!(
                    channel_id,
                    "OutgoingBinder::bind_tx_stream: took sender, registering for incoming"
                );
                // Register for incoming Data routing
                self.register_incoming(channel_id, tx);
            }
        }
    }

    /// Make a raw RPC call with pre-serialized payload.
    ///
    /// Returns the raw response payload bytes.
    /// Note: For streaming calls, use `call()` which handles channel binding.
    pub async fn call_raw(
        &self,
        method_id: u64,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, TransportError> {
        self.call_raw_full(method_id, Vec::new(), Vec::new(), payload, None)
            .await
            .map(|r| r.payload)
    }

    /// Make a raw RPC call with pre-serialized payload and channel IDs.
    ///
    /// Used internally by `call()` after binding streams.
    /// Returns ResponseData so caller can handle response channels.
    pub(crate) async fn call_raw_with_channels(
        &self,
        method_id: u64,
        channels: Vec<u64>,
        payload: Vec<u8>,
        args_debug: Option<String>,
    ) -> Result<ResponseData, TransportError> {
        self.call_raw_full(method_id, Vec::new(), channels, payload, args_debug)
            .await
    }

    pub(crate) async fn call_raw_with_channels_and_metadata(
        &self,
        method_id: u64,
        channels: Vec<u64>,
        payload: Vec<u8>,
        args_debug: Option<String>,
        metadata: roam_wire::Metadata,
    ) -> Result<ResponseData, TransportError> {
        self.call_raw_full(method_id, metadata, channels, payload, args_debug)
            .await
    }

    /// Make a raw RPC call with pre-serialized payload and metadata.
    ///
    /// Returns the raw response payload bytes.
    pub async fn call_raw_with_metadata(
        &self,
        method_id: u64,
        payload: Vec<u8>,
        metadata: Vec<(String, roam_wire::MetadataValue)>,
    ) -> Result<Vec<u8>, TransportError> {
        self.call_raw_full(method_id, metadata, Vec::new(), payload, None)
            .await
            .map(|r| r.payload)
    }

    /// Make a raw RPC call with all options.
    ///
    /// Returns ResponseData containing the payload and any response channel IDs.
    async fn call_raw_full(
        &self,
        method_id: u64,
        metadata: Vec<(String, roam_wire::MetadataValue)>,
        channels: Vec<u64>,
        payload: Vec<u8>,
        args_debug: Option<String>,
    ) -> Result<ResponseData, TransportError> {
        let request_id = self.shared.request_ids.next();
        let (response_tx, response_rx) = oneshot();

        // Track outgoing request for diagnostics
        if let Some(diag) = &self.shared.diagnostic_state {
            let args = args_debug.map(|s| {
                let mut map = std::collections::HashMap::new();
                map.insert("args".to_string(), s);
                map
            });
            diag.record_outgoing_request(request_id, method_id, args);
            // Associate channels with this request
            diag.associate_channels_with_request(&channels, request_id);
        }

        let msg = DriverMessage::Call {
            conn_id: self.shared.conn_id,
            request_id,
            method_id,
            metadata,
            channels,
            payload,
            response_tx,
        };

        self.shared
            .driver_tx
            .send(msg)
            .await
            .map_err(|_| TransportError::DriverGone)?;

        let result = response_rx
            .await
            .map_err(|_| TransportError::DriverGone)?
            .map_err(|_| TransportError::ConnectionClosed);

        // Mark request as complete
        if let Some(diag) = &self.shared.diagnostic_state {
            diag.complete_request(request_id);
        }

        result
    }

    /// Open a new virtual connection on the link.
    ///
    /// Sends a `Connect` message to the remote peer and waits for an
    /// `Accept` or `Reject` response. Returns a new `ConnectionHandle`
    /// for the virtual connection if accepted.
    ///
    /// r[impl core.conn.open]
    ///
    /// # Arguments
    ///
    /// * `metadata` - Optional metadata to send with the Connect request
    ///   (e.g., authentication tokens, routing hints).
    /// * `dispatcher` - Optional dispatcher for handling incoming requests on the
    ///   virtual connection. If None, the connection can only make calls, not receive them.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Open a new virtual connection that can receive calls
    /// let dispatcher = Box::new(MyDispatcher::new());
    /// let virtual_conn = handle.connect(vec![], Some(dispatcher)).await?;
    ///
    /// // Use the new connection for calls
    /// let response = virtual_conn.call_raw(method_id, payload).await?;
    /// ```
    pub async fn connect(
        &self,
        metadata: roam_wire::Metadata,
        dispatcher: Option<Box<dyn ServiceDispatcher>>,
    ) -> Result<ConnectionHandle, crate::ConnectError> {
        let request_id = self.shared.request_ids.next();
        let (response_tx, response_rx) = oneshot();

        let msg = DriverMessage::Connect {
            request_id,
            metadata,
            response_tx,
            dispatcher,
        };

        self.shared.driver_tx.send(msg).await.map_err(|_| {
            crate::ConnectError::ConnectFailed(std::io::Error::other("driver gone"))
        })?;

        response_rx
            .await
            .map_err(|_| crate::ConnectError::ConnectFailed(std::io::Error::other("driver gone")))?
    }

    /// Allocate a stream ID for an outgoing stream.
    ///
    /// Used internally when binding streams during call().
    pub fn alloc_channel_id(&self) -> ChannelId {
        self.shared.channel_ids.next()
    }

    /// Allocate a unique request ID for an outgoing call.
    ///
    /// Used when manually constructing DriverMessage::Call.
    pub fn alloc_request_id(&self) -> u64 {
        self.shared.request_ids.next()
    }

    /// Register an incoming stream (we receive data from peer).
    ///
    /// Used when schema has `Tx<T>` (callee sends to caller) - we receive that data.
    pub fn register_incoming(&self, channel_id: ChannelId, tx: Sender<Vec<u8>>) {
        // Track channel for diagnostics (request_id not available here)
        if let Some(diag) = &self.shared.diagnostic_state {
            diag.record_channel_open(channel_id, crate::diagnostic::ChannelDirection::Rx, None);
        }
        self.shared
            .channel_registry
            .lock()
            .unwrap()
            .register_incoming(channel_id, tx);
    }

    /// Register credit tracking for an outgoing stream.
    ///
    /// The actual receiver is owned by the driver, not the registry.
    pub fn register_outgoing_credit(&self, channel_id: ChannelId) {
        // Track channel for diagnostics (request_id not available here)
        if let Some(diag) = &self.shared.diagnostic_state {
            diag.record_channel_open(channel_id, crate::diagnostic::ChannelDirection::Tx, None);
        }
        self.shared
            .channel_registry
            .lock()
            .unwrap()
            .register_outgoing_credit(channel_id);
    }

    /// Route incoming stream data to the appropriate Rx.
    pub async fn route_data(
        &self,
        channel_id: ChannelId,
        payload: Vec<u8>,
    ) -> Result<(), ChannelError> {
        // Get the sender while holding the lock, then release before await
        let (tx, payload) = self
            .shared
            .channel_registry
            .lock()
            .unwrap()
            .prepare_route_data(channel_id, payload)?;
        // Send without holding the lock
        let _ = tx.send(payload).await;
        Ok(())
    }

    /// Close an incoming stream.
    pub fn close_channel(&self, channel_id: ChannelId) {
        // Track channel close for diagnostics
        if let Some(diag) = &self.shared.diagnostic_state {
            diag.record_channel_close(channel_id);
        }
        self.shared
            .channel_registry
            .lock()
            .unwrap()
            .close(channel_id);
    }

    /// Reset a stream.
    pub fn reset_channel(&self, channel_id: ChannelId) {
        // Track channel close for diagnostics
        if let Some(diag) = &self.shared.diagnostic_state {
            diag.record_channel_close(channel_id);
        }
        self.shared
            .channel_registry
            .lock()
            .unwrap()
            .reset(channel_id);
    }

    /// Check if a stream exists.
    pub fn contains_channel(&self, channel_id: ChannelId) -> bool {
        self.shared
            .channel_registry
            .lock()
            .unwrap()
            .contains(channel_id)
    }

    /// Receive credit for an outgoing stream.
    pub fn receive_credit(&self, channel_id: ChannelId, bytes: u32) {
        self.shared
            .channel_registry
            .lock()
            .unwrap()
            .receive_credit(channel_id, bytes);
    }

    /// Get a clone of the driver message sender.
    ///
    /// Used for forwarding/proxy scenarios where messages need to be sent
    /// on this connection's wire.
    pub fn driver_tx(&self) -> Sender<DriverMessage> {
        self.shared.channel_registry.lock().unwrap().driver_tx()
    }

    /// Bind receivers for `Rx<T>` streams in a deserialized response.
    ///
    /// After deserializing a response, any `Rx<T>` values are "hollow" - they have
    /// channel IDs but no actual receiver. This method walks the response using
    /// reflection and binds receivers for each `Rx<T>` so data can be received.
    ///
    /// # How it works
    ///
    /// For each `Rx<T>` found in the response:
    /// 1. Read the channel_id that was set during deserialization
    /// 2. Create a new channel (tx, rx)
    /// 3. Set the receiver slot on the Rx
    /// 4. Register the sender with the channel registry for incoming data routing
    ///
    /// This mirrors server-side `ChannelRegistry::bind_streams` but for responses.
    ///
    /// IMPORTANT: The `channels` parameter contains the authoritative channel IDs
    /// from the Response framing. For forwarded connections (via ForwardingDispatcher),
    /// these IDs may differ from the IDs serialized in the payload. We patch them first.
    pub fn bind_response_streams<T: Facet<'static>>(&self, response: &mut T, channels: &[u64]) {
        // Patch channel IDs from Response.channels into the deserialized response.
        // This is critical for ForwardingDispatcher where the payload contains upstream
        // channel IDs but channels[] contains the remapped downstream IDs.
        patch_channel_ids(response, channels);

        let poke = facet::Poke::new(response);
        self.bind_response_streams_recursive(poke);
    }

    /// Recursively walk a Poke value looking for Rx streams to bind in responses.
    #[allow(unsafe_code)]
    fn bind_response_streams_recursive(&self, mut poke: facet::Poke<'_, '_>) {
        use facet::Def;

        let shape = poke.shape();

        // Check if this is an Rx type - only Rx needs binding in responses
        // (Tx in responses would be outgoing, but that's uncommon for return types)
        if shape.decl_id == crate::Rx::<()>::SHAPE.decl_id {
            self.bind_rx_response_stream(poke);
            return;
        }

        // Dispatch based on the shape's definition
        match shape.def {
            Def::Scalar => {}

            // Recurse into struct/tuple fields
            _ if poke.is_struct() => {
                let mut ps = poke.into_struct().expect("is_struct was true");
                let field_count = ps.field_count();
                for i in 0..field_count {
                    if let Ok(field_poke) = ps.field(i) {
                        self.bind_response_streams_recursive(field_poke);
                    }
                }
            }

            // Recurse into Option<T>
            Def::Option(_) => {
                // Option is represented as an enum, use into_enum to access its value
                if let Ok(mut pe) = poke.into_enum()
                    && let Ok(Some(inner_poke)) = pe.field(0)
                {
                    self.bind_response_streams_recursive(inner_poke);
                }
            }

            // Recurse into list elements (e.g., Vec<Rx<T>>)
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
                            self.bind_response_streams_recursive(element_poke);
                        }
                    }
                }
            }

            // Other enum variants
            _ if poke.is_enum() => {
                if let Ok(mut pe) = poke.into_enum()
                    && let Ok(Some(variant_poke)) = pe.field(0)
                {
                    self.bind_response_streams_recursive(variant_poke);
                }
            }

            _ => {}
        }
    }

    /// Bind a single Rx<T> stream from a response.
    ///
    /// Creates a channel, sets the receiver slot, and registers for incoming data.
    fn bind_rx_response_stream(&self, poke: facet::Poke<'_, '_>) {
        if let Ok(mut ps) = poke.into_struct() {
            // Get the channel_id that was deserialized from the wire
            let channel_id = if let Ok(channel_id_field) = ps.field_by_name("channel_id")
                && let Ok(id_ref) = channel_id_field.get::<ChannelId>()
            {
                *id_ref
            } else {
                return;
            };

            // Create channel and set receiver slot
            let (tx, rx) = crate::runtime::channel(RX_STREAM_BUFFER_SIZE);

            if let Ok(mut receiver_field) = ps.field_by_name("receiver")
                && let Ok(slot) = receiver_field.get_mut::<ReceiverSlot>()
            {
                slot.set(rx);
            }

            // Register for incoming data routing
            self.register_incoming(channel_id, tx);
        }
    }
}
