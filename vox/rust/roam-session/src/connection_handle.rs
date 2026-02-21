use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use facet::Facet;

#[cfg(feature = "diagnostics")]
use crate::request_response_spy::RequestResponseSpy;
use facet_core::PtrMut;

use crate::{
    ChannelError, ChannelId, ChannelIdAllocator, ChannelRegistry, DriverMessage,
    IncomingChannelMessage, RX_STREAM_BUFFER_SIZE, ReceiverSlot, ResponseData, SenderSlot,
    ServiceDispatcher, TransportError, patch_channel_ids, runtime::oneshot,
};
use crate::{
    Role,
    runtime::{Receiver, Sender},
};

// ============================================================================
// Request ID generation
// ============================================================================

/// Generates unique request IDs for a connection.
///
/// r[impl call.request-id.uniqueness] - monotonically increasing counter starting at 1
pub struct RequestIdGenerator {
    next: AtomicU64,
}

impl RequestIdGenerator {
    /// Create a new generator starting at 1.
    pub fn new() -> Self {
        Self {
            next: AtomicU64::new(1),
        }
    }

    /// Generate the next unique request ID.
    pub fn next(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }
}

impl Default for RequestIdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Shared state between ConnectionHandle and Driver
// ============================================================================

/// Shared state between ConnectionHandle and Driver.
pub(crate) struct HandleShared {
    /// Connection ID for this handle (0 = root connection).
    pub(crate) conn_id: roam_wire::ConnectionId,
    /// Unified channel to send all messages to the driver.
    pub(crate) driver_tx: Sender<DriverMessage>,
    /// Request ID generator.
    pub(crate) request_ids: RequestIdGenerator,
    /// Channel ID allocator.
    pub(crate) channel_ids: ChannelIdAllocator,
    /// Channel registry for routing incoming data.
    /// Protected by a mutex since handles may create channels concurrently.
    pub(crate) channel_registry: crate::runtime::Mutex<ChannelRegistry>,
    /// Optional diagnostic state for SIGUSR1 dumps.
    pub(crate) diagnostic_state: Option<Arc<crate::diagnostic::DiagnosticState>>,
    /// Optional request concurrency limiter.
    pub(crate) request_semaphore: Option<moire::sync::Semaphore>,
}

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
/// let response = handle.call_raw(descriptor, payload).await?;
/// ```
#[derive(Clone)]
pub struct ConnectionHandle {
    shared: Arc<HandleShared>,
}

impl ConnectionHandle {
    fn upsert_metadata_entry(
        metadata: &mut roam_wire::Metadata,
        key: &str,
        value: roam_wire::MetadataValue,
        flags: u64,
    ) {
        if let Some((_, existing_value, existing_flags)) = metadata
            .iter_mut()
            .find(|(entry_key, _, _)| entry_key == key)
        {
            *existing_value = value;
            *existing_flags = flags;
            return;
        }
        metadata.push((key.to_string(), value, flags));
    }

    fn span_id_for_request(&self, _request_id: u64) -> String {
        ulid::Ulid::new().to_string()
    }

    fn merged_outgoing_metadata(
        &self,
        mut metadata: roam_wire::Metadata,
        request_id: u64,
        method_name: &str,
    ) -> roam_wire::Metadata {
        if let Some(current_call_metadata) = crate::dispatch::get_current_call_metadata() {
            for (key, value, flags) in current_call_metadata {
                if flags & roam_wire::metadata_flags::NO_PROPAGATE != 0 {
                    continue;
                }
                if metadata
                    .iter()
                    .any(|(existing_key, _, _)| existing_key == &key)
                {
                    continue;
                }
                metadata.push((key, value, flags));
            }
        }

        let parent_span = roam_wire::metadata_string(&metadata, crate::MOIRE_SPAN_ID_METADATA_KEY);
        let span_id = self.span_id_for_request(request_id);
        let chain_id = roam_wire::metadata_string(&metadata, crate::MOIRE_CHAIN_ID_METADATA_KEY)
            .unwrap_or_else(|| span_id.clone());
        Self::upsert_metadata_entry(
            &mut metadata,
            crate::MOIRE_CHAIN_ID_METADATA_KEY,
            roam_wire::MetadataValue::String(chain_id),
            roam_wire::metadata_flags::NONE,
        );
        Self::upsert_metadata_entry(
            &mut metadata,
            crate::MOIRE_SPAN_ID_METADATA_KEY,
            roam_wire::MetadataValue::String(span_id.clone()),
            roam_wire::metadata_flags::NONE,
        );
        Self::upsert_metadata_entry(
            &mut metadata,
            crate::MOIRE_METHOD_NAME_METADATA_KEY,
            roam_wire::MetadataValue::String(method_name.to_owned()),
            roam_wire::metadata_flags::NONE,
        );
        if let Some(parent_span) = parent_span {
            Self::upsert_metadata_entry(
                &mut metadata,
                crate::MOIRE_PARENT_SPAN_ID_METADATA_KEY,
                roam_wire::MetadataValue::String(parent_span),
                roam_wire::metadata_flags::NONE,
            );
        }

        metadata
    }

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
        Self::new_with_diagnostics_and_limits(
            conn_id,
            driver_tx,
            role,
            initial_credit,
            u32::MAX,
            diagnostic_state,
        )
    }

    /// Create a new handle with explicit call concurrency limits.
    pub fn new_with_diagnostics_and_limits(
        conn_id: roam_wire::ConnectionId,
        driver_tx: Sender<DriverMessage>,
        role: Role,
        initial_credit: u32,
        max_concurrent_requests: u32,
        diagnostic_state: Option<Arc<crate::diagnostic::DiagnosticState>>,
    ) -> Self {
        let channel_registry = ChannelRegistry::new_with_credit(initial_credit, driver_tx.clone());
        let request_semaphore = if max_concurrent_requests == u32::MAX {
            None
        } else {
            Some(moire::sync::Semaphore::new(
                "request_semaphore",
                max_concurrent_requests as usize,
            ))
        };
        Self {
            shared: Arc::new(HandleShared {
                conn_id,
                driver_tx,
                request_ids: RequestIdGenerator::new(),
                channel_ids: ChannelIdAllocator::new(role),
                channel_registry: crate::runtime::Mutex::new(
                    "ConnectionHandle.channel_registry",
                    channel_registry,
                ),
                diagnostic_state,
                request_semaphore,
            }),
        }
    }

    async fn acquire_request_slot(
        &self,
    ) -> Result<Option<moire::sync::OwnedSemaphorePermit>, TransportError> {
        if let Some(semaphore) = &self.shared.request_semaphore {
            let permit = semaphore
                .acquire_owned()
                .await
                .map_err(|_| TransportError::DriverGone)?;
            Ok(Some(permit))
        } else {
            Ok(None)
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

    /// Make a typed RPC call with automatic serialization and channel binding.
    ///
    /// Walks the args using Poke reflection to find any `Rx<T>` or `Tx<T>` fields,
    /// binds channel IDs, and sets up the channel infrastructure before serialization.
    ///
    /// # Arguments
    ///
    /// * `method_id` - The method ID to call
    /// * `args` - Arguments to serialize (typically a tuple of all method args).
    ///   Must be mutable so channel IDs can be assigned.
    ///
    /// # Channel Binding
    ///
    /// For `Rx<T>` in args (caller passes receiver, keeps sender to push data):
    /// - Allocates a channel ID
    /// - Takes the receiver and spawns a task to drain it, sending Data messages
    /// - The caller keeps the `Tx<T>` from `roam::channel()` to send values
    ///
    /// For `Tx<T>` in args (caller passes sender, keeps receiver to pull data):
    /// - Allocates a channel ID
    /// - Takes the sender and registers for incoming Data routing
    /// - The caller keeps the `Rx<T>` from `roam::channel()` to receive values
    ///
    /// # Example
    ///
    /// ```ignore
    /// // For a channeled method sum(numbers: Rx<i32>) -> i64
    /// let (tx, rx) = roam::channel::<i32>();
    /// let response = handle.call(method_id::SUM, &mut (rx,)).await?;
    /// // tx.send(&42).await to push values
    /// ```
    /// Make a typed RPC call with default (empty) metadata.
    ///
    /// The descriptor contains all precomputed plans and method metadata.
    pub async fn call<T: Facet<'static>>(
        &self,
        descriptor: &'static crate::MethodDescriptor,
        args: &mut T,
    ) -> Result<ResponseData, TransportError> {
        let args_ptr = args as *mut T as *mut ();
        #[allow(unsafe_code)]
        unsafe {
            self.call_with_metadata_by_plan(descriptor, args_ptr, roam_wire::Metadata::default())
                .await
        }
    }

    /// Make an RPC call using reflection (non-generic).
    ///
    /// This is the non-generic core implementation that avoids monomorphization.
    ///
    /// # Safety
    ///
    /// - `args_ptr` must point to valid, initialized memory matching the descriptor's args_plan shape
    /// - The args type must be `Send`
    #[doc(hidden)]
    #[allow(unsafe_code)]
    #[track_caller]
    pub unsafe fn call_with_metadata_by_plan(
        &self,
        descriptor: &'static crate::MethodDescriptor,
        args_ptr: *mut (),
        metadata: roam_wire::Metadata,
    ) -> impl std::future::Future<Output = Result<ResponseData, TransportError>> + Send + '_ {
        let args_plan = descriptor.args_plan;
        let args_shape = args_plan.type_plan.root().shape;

        // Do all pointer work synchronously BEFORE creating the async block.
        // This ensures the raw pointer doesn't need to be captured by the future.

        // Walk args and bind any channels (allocates channel IDs)
        // This collects receivers that need to be drained but does NOT spawn
        let mut drains = Vec::new();
        trace!("ConnectionHandle::call_by_plan: binding channels");

        // SAFETY: Caller guarantees args_ptr is valid and initialized
        // Walk args and bind channels using precomputed paths
        for loc in &args_plan.channel_locations {
            let poke = unsafe {
                facet::Poke::from_raw_parts(PtrMut::new(args_ptr.cast::<u8>()), args_shape)
            };
            match poke.at_path_mut(&loc.path) {
                Ok(channel_poke) => match loc.kind {
                    crate::ChannelKind::Rx => {
                        self.bind_rx_channel(channel_poke, &mut drains);
                    }
                    crate::ChannelKind::Tx => {
                        self.bind_tx_channel(channel_poke);
                    }
                },
                Err(facet_path::PathAccessError::OptionIsNone { .. }) => {}
                Err(_e) => {
                    warn!("call_with_metadata_by_plan: unexpected path error: {_e}");
                }
            }
        }

        // Collect channel IDs for the Request message using precomputed paths
        // SAFETY: args_ptr is valid and initialized (was just walked by bind_channels)
        let peek = unsafe {
            facet::Peek::unchecked_new(facet_core::PtrConst::new(args_ptr.cast::<u8>()), args_shape)
        };
        let channels = crate::dispatch::collect_channel_ids_with_plan(peek, args_plan);
        trace!(
            channels = ?channels,
            drain_count = drains.len(),
            "ConnectionHandle::call_by_plan: collected channels after bind_channels"
        );

        // Serialize using non-generic peek_to_vec
        let peek = unsafe {
            facet::Peek::unchecked_new(facet_core::PtrConst::new(args_ptr.cast::<u8>()), args_shape)
        };
        let payload_result = facet_postcard::peek_to_vec(peek);

        // Generate args debug info for diagnostics when enabled
        let args_debug = if cfg!(feature = "diagnostics") {
            let peek = unsafe {
                facet::Peek::unchecked_new(
                    facet_core::PtrConst::new(args_ptr.cast::<u8>()),
                    args_shape,
                )
            };
            Some(
                facet_pretty::PrettyPrinter::new()
                    .with_colors(facet_pretty::ColorMode::Never)
                    .with_max_content_len(64)
                    .format_peek(peek),
            )
        } else {
            None
        };

        // Now return an async block that doesn't capture args_ptr
        async move {
            let payload = payload_result.map_err(TransportError::Encode)?;
            self.call_raw_full_with_drains(
                descriptor, metadata, channels, payload, args_debug, drains,
            )
            .await
        }
    }

    /// Bind an Rx<T> channel - caller passes receiver, keeps sender.
    /// Collects the receiver for draining (no spawning).
    fn bind_rx_channel(
        &self,
        poke: facet::Poke<'_, '_>,
        drains: &mut Vec<(ChannelId, Receiver<IncomingChannelMessage>)>,
    ) {
        let channel_id = self.alloc_channel_id();
        debug!(
            channel_id,
            "OutgoingBinder::bind_rx_channel: allocated channel_id for Rx"
        );

        if let Ok(mut ps) = poke.into_struct() {
            // Set channel_id field by getting mutable access to the u64
            if let Ok(mut channel_id_field) = ps.field_by_name("channel_id")
                && let Ok(id_ref) = channel_id_field.get_mut::<ChannelId>()
            {
                debug!(
                    old_id = *id_ref,
                    new_id = channel_id,
                    "OutgoingBinder::bind_rx_channel: overwriting channel_id"
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
                    "OutgoingBinder::bind_rx_channel: took receiver, adding to drains"
                );
                drains.push((channel_id, rx));
            }
        }
    }

    /// Bind a Tx<T> channel - caller passes sender, keeps receiver.
    /// We take the sender and register for incoming Data routing.
    fn bind_tx_channel(&self, poke: facet::Poke<'_, '_>) {
        let channel_id = self.alloc_channel_id();
        debug!(
            channel_id,
            "OutgoingBinder::bind_tx_channel: allocated channel_id for Tx"
        );

        if let Ok(mut ps) = poke.into_struct() {
            // Set channel_id field by getting mutable access to the u64
            if let Ok(mut channel_id_field) = ps.field_by_name("channel_id")
                && let Ok(id_ref) = channel_id_field.get_mut::<ChannelId>()
            {
                debug!(
                    old_id = *id_ref,
                    new_id = channel_id,
                    "OutgoingBinder::bind_tx_channel: overwriting channel_id"
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
                    "OutgoingBinder::bind_tx_channel: took sender, registering for incoming"
                );
                // Register for incoming Data routing
                self.register_incoming(channel_id, tx);
            }
        }
    }

    /// Make a raw RPC call with pre-serialized payload.
    ///
    /// Returns the raw response payload bytes.
    /// Note: For channeled calls, use `call()` which handles channel binding.
    pub async fn call_raw(
        &self,
        descriptor: &'static crate::MethodDescriptor,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, TransportError> {
        self.call_raw_full(descriptor, Vec::new(), Vec::new(), payload, None)
            .await
            .map(|r| r.payload)
    }

    /// Make a raw RPC call with pre-serialized payload and channel IDs.
    ///
    /// Used internally by forwarding after binding channels.
    /// Returns ResponseData so caller can handle response channels.
    pub(crate) async fn call_raw_with_channels(
        &self,
        descriptor: &'static crate::MethodDescriptor,
        channels: Vec<u64>,
        payload: Vec<u8>,
        args_debug: Option<String>,
    ) -> Result<ResponseData, TransportError> {
        self.call_raw_full(descriptor, Vec::new(), channels, payload, args_debug)
            .await
    }

    /// Make a raw RPC call with pre-serialized payload and metadata.
    ///
    /// Returns the raw response payload bytes.
    pub async fn call_raw_with_metadata(
        &self,
        descriptor: &'static crate::MethodDescriptor,
        payload: Vec<u8>,
        metadata: roam_wire::Metadata,
    ) -> Result<Vec<u8>, TransportError> {
        self.call_raw_full(descriptor, metadata, Vec::new(), payload, None)
            .await
            .map(|r| r.payload)
    }

    /// Make a raw RPC call with all options.
    ///
    /// Returns ResponseData containing the payload and any response channel IDs.
    async fn call_raw_full(
        &self,
        descriptor: &'static crate::MethodDescriptor,
        metadata: roam_wire::Metadata,
        channels: Vec<u64>,
        payload: Vec<u8>,
        args_debug: Option<String>,
    ) -> Result<ResponseData, TransportError> {
        self.call_raw_full_with_drains(
            descriptor,
            metadata,
            channels,
            payload,
            args_debug,
            Vec::new(),
        )
        .await
    }

    /// Core call implementation: sends a DriverMessage::Call, spawns drain tasks, waits for response.
    #[allow(clippy::too_many_arguments)]
    async fn call_raw_full_with_drains(
        &self,
        descriptor: &'static crate::MethodDescriptor,
        metadata: roam_wire::Metadata,
        channels: Vec<u64>,
        payload: Vec<u8>,
        args_debug: Option<String>,
        drains: Vec<(ChannelId, Receiver<IncomingChannelMessage>)>,
    ) -> Result<ResponseData, TransportError> {
        let _request_permit = self.acquire_request_slot().await?;

        let method_id = descriptor.id;
        let request_id = self.shared.request_ids.next();
        let method_name_full = format!("{}.{}", descriptor.service_name, descriptor.method_name);
        #[cfg(feature = "diagnostics")]
        let mut metadata = self.merged_outgoing_metadata(metadata, request_id, &method_name_full);
        #[cfg(not(feature = "diagnostics"))]
        let metadata = self.merged_outgoing_metadata(metadata, request_id, &method_name_full);

        #[cfg(feature = "diagnostics")]
        {
            let request_handle = if let Some(diag) = self.shared.diagnostic_state.as_deref() {
                let args_debug_str = args_debug.as_deref().unwrap_or("");
                let args_json = if args_debug_str.is_empty() {
                    String::from("[]")
                } else {
                    args_debug_str.to_string()
                };
                let request_body = moire_types::RequestEntity {
                    service_name: String::from(descriptor.service_name),
                    method_name: String::from(descriptor.method_name),
                    args_json: moire_types::Json::new(args_json),
                };
                Some(diag.emit_request_node(method_name_full.clone(), request_body))
            } else {
                None
            };

            if let Some(request_wire_id) = request_handle.as_ref().and_then(|h| h.id_for_wire()) {
                Self::upsert_metadata_entry(
                    &mut metadata,
                    crate::MOIRE_REQUEST_ENTITY_ID_METADATA_KEY,
                    roam_wire::MetadataValue::String(request_wire_id),
                    roam_wire::metadata_flags::NONE,
                );
            }
        }

        // Track outgoing request for diagnostics
        if let Some(diag) = &self.shared.diagnostic_state {
            let args = args_debug.as_ref().map(|s| {
                let mut map = std::collections::HashMap::new();
                map.insert("args".to_string(), s.clone());
                map
            });
            diag.record_outgoing_request(crate::diagnostic::RequestRecord {
                conn_id: self.shared.conn_id.raw(),
                request_id,
                method_id,
                metadata: Some(&metadata),
                args,
            });
            diag.associate_channels_with_request(&channels, request_id);
        }

        let (response_tx, response_rx) = oneshot("call");
        let msg = DriverMessage::Call {
            conn_id: self.shared.conn_id,
            request_id,
            method_id,
            metadata,
            channels,
            payload,
            response_tx,
        };

        self.send_and_drain(msg, drains, response_rx).await
    }

    /// Send a DriverMessage::Call, spawn drain tasks, and wait for the response.
    async fn send_and_drain(
        &self,
        msg: DriverMessage,
        drains: Vec<(ChannelId, Receiver<IncomingChannelMessage>)>,
        response_rx: crate::runtime::OneshotReceiver<Result<ResponseData, TransportError>>,
    ) -> Result<ResponseData, TransportError> {
        self.shared
            .driver_tx
            .send(msg)
            .await
            .map_err(|_| TransportError::DriverGone)?;

        let conn_id = self.shared.conn_id;
        if !drains.is_empty() {
            let task_tx = self.shared.channel_registry.lock().driver_tx();
            for (channel_id, mut rx) in drains {
                let task_tx = task_tx.clone();
                crate::runtime::spawn("roam_tx_drain", async move {
                    loop {
                        match rx.recv().await {
                            Some(IncomingChannelMessage::Data(payload)) => {
                                debug!(
                                    "drain task: received {} bytes on channel {}",
                                    payload.len(),
                                    channel_id
                                );
                                if task_tx
                                    .send(DriverMessage::Data {
                                        conn_id,
                                        channel_id,
                                        payload,
                                    })
                                    .await
                                    .is_err()
                                {
                                    warn!(
                                        conn_id = conn_id.raw(),
                                        channel_id, "drain task failed to send DriverMessage::Data"
                                    );
                                    break;
                                }
                                debug!(
                                    "drain task: sent DriverMessage::Data for channel {}",
                                    channel_id
                                );
                            }
                            Some(IncomingChannelMessage::Close) | None => {
                                debug!("drain task: channel {} closed", channel_id);
                                if task_tx
                                    .send(DriverMessage::Close {
                                        conn_id,
                                        channel_id,
                                    })
                                    .await
                                    .is_err()
                                {
                                    warn!(
                                        conn_id = conn_id.raw(),
                                        channel_id,
                                        "drain task failed to send DriverMessage::Close"
                                    );
                                }
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
        }

        response_rx
            .recv()
            .await
            .map_err(|_| TransportError::DriverGone)?
            .map_err(|_| TransportError::ConnectionClosed)
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
    /// let response = virtual_conn.call_raw(descriptor, payload).await?;
    /// ```
    pub async fn connect(
        &self,
        metadata: roam_wire::Metadata,
        dispatcher: Option<Box<dyn ServiceDispatcher>>,
    ) -> Result<ConnectionHandle, crate::ConnectError> {
        let request_id = self.shared.request_ids.next();
        let (response_tx, response_rx) = oneshot("connect_virtual");

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
            .recv()
            .await
            .map_err(|_| crate::ConnectError::ConnectFailed(std::io::Error::other("driver gone")))?
    }

    /// Allocate a channel ID for an outgoing channel.
    ///
    /// Used internally when binding channels during call().
    pub fn alloc_channel_id(&self) -> ChannelId {
        self.shared.channel_ids.next()
    }

    /// Allocate a unique request ID for an outgoing call.
    ///
    /// Used when manually constructing DriverMessage::Call.
    pub fn alloc_request_id(&self) -> u64 {
        self.shared.request_ids.next()
    }

    /// Register an incoming channel (we receive data from peer).
    ///
    /// Used when schema has `Tx<T>` (callee sends to caller) - we receive that data.
    pub fn register_incoming(&self, channel_id: ChannelId, tx: Sender<IncomingChannelMessage>) {
        // Track channel for diagnostics (request_id not available here)
        if let Some(diag) = &self.shared.diagnostic_state {
            diag.record_channel_open(channel_id, crate::diagnostic::ChannelDirection::Rx, None);
        }
        self.shared
            .channel_registry
            .lock()
            .register_incoming(channel_id, tx);
    }

    /// Register credit tracking for an outgoing channel.
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
            .register_outgoing_credit(channel_id);
    }

    /// Route incoming channel data to the appropriate Rx.
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
            .prepare_route_data(channel_id, payload)?;
        // Send without holding the lock
        let _ = tx.send(IncomingChannelMessage::Data(payload)).await;
        Ok(())
    }

    /// Close an incoming channel.
    pub fn close_channel(&self, channel_id: ChannelId) {
        // Track channel close for diagnostics
        if let Some(diag) = &self.shared.diagnostic_state {
            diag.record_channel_close(channel_id);
        }
        self.shared.channel_registry.lock().close(channel_id);
    }

    /// Reset a channel.
    pub fn reset_channel(&self, channel_id: ChannelId) {
        // Track channel close for diagnostics
        if let Some(diag) = &self.shared.diagnostic_state {
            diag.record_channel_close(channel_id);
        }
        self.shared.channel_registry.lock().reset(channel_id);
    }

    /// Check if a channel exists.
    pub fn contains_channel(&self, channel_id: ChannelId) -> bool {
        self.shared.channel_registry.lock().contains(channel_id)
    }

    /// Receive credit for an outgoing channel.
    pub fn receive_credit(&self, channel_id: ChannelId, bytes: u32) {
        self.shared
            .channel_registry
            .lock()
            .receive_credit(channel_id, bytes);
    }

    /// Get a clone of the driver message sender.
    ///
    /// Used for forwarding/proxy scenarios where messages need to be sent
    /// on this connection's wire.
    pub fn driver_tx(&self) -> Sender<DriverMessage> {
        self.shared.channel_registry.lock().driver_tx()
    }

    /// Bind receivers for `Rx<T>` channels in a deserialized response.
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
    /// This mirrors server-side channel binding but for responses.
    ///
    /// IMPORTANT: The `channels` parameter contains the authoritative channel IDs
    /// from the Response framing. For forwarded connections (via ForwardingDispatcher),
    /// these IDs may differ from the IDs serialized in the payload. We patch them first.
    ///
    /// The `plan` should be created once per type as a static in non-generic code.
    #[allow(unsafe_code)]
    pub fn bind_response_channels<T: Facet<'static>>(
        &self,
        response: &mut T,
        plan: &crate::RpcPlan,
        channels: &[u64],
    ) {
        // Patch channel IDs from Response.channels into the deserialized response.
        // This is critical for ForwardingDispatcher where the payload contains upstream
        // channel IDs but channels[] contains the remapped downstream IDs.
        patch_channel_ids(response, plan, channels);

        let response_ptr = response as *mut T as *mut ();
        unsafe { self.bind_response_channels_with_plan(response_ptr, plan) };
    }

    /// Bind Rx channels in a response using precomputed paths.
    ///
    /// # Safety
    ///
    /// - `response_ptr` must point to valid, initialized memory matching the plan's shape
    #[allow(unsafe_code)]
    unsafe fn bind_response_channels_with_plan(
        &self,
        response_ptr: *mut (),
        plan: &crate::RpcPlan,
    ) {
        let shape = plan.type_plan.root().shape;
        for loc in &plan.channel_locations {
            // Only Rx needs binding in responses
            if loc.kind != crate::ChannelKind::Rx {
                continue;
            }
            let poke = unsafe {
                facet::Poke::from_raw_parts(PtrMut::new(response_ptr.cast::<u8>()), shape)
            };
            match poke.at_path_mut(&loc.path) {
                Ok(channel_poke) => {
                    self.bind_rx_response_stream(channel_poke);
                }
                Err(facet_path::PathAccessError::OptionIsNone { .. }) => {}
                Err(_e) => {
                    warn!("bind_response_channels_with_plan: unexpected path error: {_e}");
                }
            }
        }
    }

    /// Bind a single Rx<T> channel from a response.
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
            let (tx, rx) = crate::runtime::channel("rx_stream_bind", RX_STREAM_BUFFER_SIZE);

            if let Ok(mut receiver_field) = ps.field_by_name("receiver")
                && let Ok(slot) = receiver_field.get_mut::<ReceiverSlot>()
            {
                slot.set(rx);
            }

            // Register for incoming data routing
            self.register_incoming(channel_id, tx);
        }
    }

    /// Bind receivers for `Rx<T>` channels in a deserialized response using reflection (non-generic).
    ///
    /// This is the non-generic version of `bind_response_channels`. It uses the precomputed
    /// RpcPlan from an OnceLock to avoid both monomorphization and redundant plan construction.
    ///
    /// # Safety
    ///
    /// - `response_ptr` must point to valid, initialized memory matching the plan's shape
    #[doc(hidden)]
    #[allow(unsafe_code)]
    pub unsafe fn bind_response_channels_by_plan(
        &self,
        response_ptr: *mut (),
        response_plan: &crate::RpcPlan,
        channels: &[u64],
    ) {
        // Patch channel IDs from Response.channels into the deserialized response.
        unsafe {
            crate::dispatch::patch_channel_ids_with_plan(response_ptr, response_plan, channels);
        }

        // Bind response channels using precomputed paths
        unsafe { self.bind_response_channels_with_plan(response_ptr, response_plan) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MethodDescriptor, RpcPlan};
    use facet::Facet;
    use std::sync::LazyLock;
    use std::time::Duration;

    static TEST_DESC: LazyLock<&'static MethodDescriptor> = LazyLock::new(|| {
        Box::leak(Box::new(MethodDescriptor {
            id: 42,
            service_name: "Test",
            method_name: "test",
            arg_names: &[],
            arg_shapes: &[],
            return_shape: <() as Facet>::SHAPE,
            args_plan: Box::leak(Box::new(RpcPlan::for_type::<(crate::Rx<Vec<u8>>,)>())),
            ok_plan: Box::leak(Box::new(RpcPlan::for_type::<()>())),
            err_plan: Box::leak(Box::new(RpcPlan::for_type::<std::convert::Infallible>())),
        }))
    });

    static RAW_DESC_1: LazyLock<&'static MethodDescriptor> = LazyLock::new(|| {
        Box::leak(Box::new(MethodDescriptor {
            id: 1,
            service_name: "Test",
            method_name: "method1",
            arg_names: &[],
            arg_shapes: &[],
            return_shape: <() as Facet>::SHAPE,
            args_plan: Box::leak(Box::new(RpcPlan::for_type::<()>())),
            ok_plan: Box::leak(Box::new(RpcPlan::for_type::<()>())),
            err_plan: Box::leak(Box::new(RpcPlan::for_type::<()>())),
        }))
    });

    static RAW_DESC_2: LazyLock<&'static MethodDescriptor> = LazyLock::new(|| {
        Box::leak(Box::new(MethodDescriptor {
            id: 2,
            service_name: "Test",
            method_name: "method2",
            arg_names: &[],
            arg_shapes: &[],
            return_shape: <() as Facet>::SHAPE,
            args_plan: Box::leak(Box::new(RpcPlan::for_type::<()>())),
            ok_plan: Box::leak(Box::new(RpcPlan::for_type::<()>())),
            err_plan: Box::leak(Box::new(RpcPlan::for_type::<()>())),
        }))
    });

    #[tokio::test]
    async fn drain_task_exits_when_driver_data_send_fails() {
        let (driver_tx, mut driver_rx) = crate::runtime::channel("test_driver", 8);
        let handle = ConnectionHandle::new(driver_tx, Role::Initiator, u32::MAX);

        let (stream_tx, stream_rx) = crate::channel::<Vec<u8>>();
        let mut args = (stream_rx,);
        let call_task = tokio::spawn(async move { handle.call(*TEST_DESC, &mut args).await });

        let call_msg = driver_rx
            .recv()
            .await
            .expect("expected DriverMessage::Call");
        assert!(
            matches!(call_msg, DriverMessage::Call { .. }),
            "first message must be DriverMessage::Call"
        );

        drop(driver_rx);

        stream_tx.send(&b"payload".to_vec()).await.unwrap();
        drop(call_msg);

        let result = tokio::time::timeout(Duration::from_secs(1), call_task)
            .await
            .expect("call should terminate once driver side is closed")
            .expect("call task should not panic");
        assert!(
            matches!(result, Err(TransportError::DriverGone)),
            "call should fail once driver side is gone"
        );
    }

    #[tokio::test]
    async fn call_respects_max_concurrent_requests_limit() {
        let (driver_tx, mut driver_rx) = crate::runtime::channel("test_driver", 8);
        let handle = ConnectionHandle::new_with_diagnostics_and_limits(
            roam_wire::ConnectionId::ROOT,
            driver_tx,
            Role::Initiator,
            u32::MAX,
            1,
            None,
        );

        let first = tokio::spawn({
            let handle = handle.clone();
            async move { handle.call_raw(*RAW_DESC_1, vec![1]).await }
        });

        let first_msg = driver_rx.recv().await.expect("first call should be sent");
        let first_response_tx = match first_msg {
            DriverMessage::Call { response_tx, .. } => response_tx,
            _ => panic!("expected DriverMessage::Call for first request"),
        };

        let second = tokio::spawn({
            let handle = handle.clone();
            async move { handle.call_raw(*RAW_DESC_2, vec![2]).await }
        });

        let blocked = tokio::time::timeout(Duration::from_millis(100), driver_rx.recv()).await;
        assert!(
            blocked.is_err(),
            "second call should wait for first response slot"
        );

        first_response_tx
            .send(Ok(ResponseData {
                payload: vec![10],
                channels: vec![],
            }))
            .expect("first response receiver should still exist");
        let first_result = first.await.expect("first task should not panic");
        assert_eq!(first_result.expect("first call should succeed"), vec![10]);

        let second_msg = driver_rx.recv().await.expect("second call should be sent");
        let second_response_tx = match second_msg {
            DriverMessage::Call { response_tx, .. } => response_tx,
            _ => panic!("expected DriverMessage::Call for second request"),
        };
        second_response_tx
            .send(Ok(ResponseData {
                payload: vec![20],
                channels: vec![],
            }))
            .expect("second response receiver should still exist");

        let second_result = second.await.expect("second task should not panic");
        assert_eq!(second_result.expect("second call should succeed"), vec![20]);
    }
}
