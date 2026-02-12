use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use facet::Facet;

use facet_core::PtrMut;

use crate::{
    ChannelError, ChannelId, ChannelIdAllocator, ChannelRegistry, DriverMessage,
    IncomingChannelMessage, RX_STREAM_BUFFER_SIZE, ReceiverSlot, ResponseData, SenderSlot,
    ServiceDispatcher, TransportError, collect_channel_ids, patch_channel_ids, runtime::oneshot,
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
    pub(crate) channel_registry: std::sync::Mutex<ChannelRegistry>,
    /// Optional diagnostic state for SIGUSR1 dumps.
    pub(crate) diagnostic_state: Option<Arc<crate::diagnostic::DiagnosticState>>,
    /// Optional request concurrency limiter.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) request_semaphore: Option<Arc<tokio::sync::Semaphore>>,
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
        #[cfg(not(target_arch = "wasm32"))]
        let request_semaphore = if max_concurrent_requests == u32::MAX {
            None
        } else {
            Some(Arc::new(tokio::sync::Semaphore::new(
                max_concurrent_requests as usize,
            )))
        };
        Self {
            shared: Arc::new(HandleShared {
                conn_id,
                driver_tx,
                request_ids: RequestIdGenerator::new(),
                channel_ids: ChannelIdAllocator::new(role),
                channel_registry: std::sync::Mutex::new(channel_registry),
                diagnostic_state,
                #[cfg(not(target_arch = "wasm32"))]
                request_semaphore,
            }),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn acquire_request_slot(
        &self,
    ) -> Result<Option<tokio::sync::OwnedSemaphorePermit>, TransportError> {
        if let Some(semaphore) = &self.shared.request_semaphore {
            let permit = semaphore
                .clone()
                .acquire_owned()
                .await
                .map_err(|_| TransportError::DriverGone)?;
            Ok(Some(permit))
        } else {
            Ok(None)
        }
    }

    #[cfg(target_arch = "wasm32")]
    async fn acquire_request_slot(&self) -> Result<(), TransportError> {
        Ok(())
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
        // Precompute plan via OnceLock (one per monomorphized type, program lifetime).
        static ARGS_PLAN: OnceLock<Arc<crate::RpcPlan>> = OnceLock::new();
        let args_plan = ARGS_PLAN.get_or_init(|| Arc::new(crate::RpcPlan::for_type::<T>()));

        // Walk args and bind any channels (allocates channel IDs)
        // This collects receivers that need to be drained but does NOT spawn
        let mut drains = Vec::new();
        trace!("ConnectionHandle::call: binding channels");
        {
            let args_ptr = args as *mut T as *mut ();
            for loc in &args_plan.channel_locations {
                #[allow(unsafe_code)]
                let poke = unsafe {
                    facet::Poke::from_raw_parts(
                        PtrMut::new(args_ptr.cast::<u8>()),
                        args_plan.type_plan.root().shape,
                    )
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
                        warn!("call_with_metadata: unexpected path error: {_e}");
                    }
                }
            }
        }

        // Collect channel IDs for the Request message
        let channels = collect_channel_ids(args);
        trace!(
            channels = ?channels,
            drain_count = drains.len(),
            "ConnectionHandle::call: collected channels after bind_channels"
        );

        let payload = facet_postcard::to_vec(args).map_err(TransportError::Encode)?;

        // Generate args debug info for diagnostics when enabled
        let args_debug = if cfg!(feature = "diagnostics") {
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
            #[cfg(not(target_arch = "wasm32"))]
            let _request_permit = self.acquire_request_slot().await?;
            #[cfg(target_arch = "wasm32")]
            self.acquire_request_slot().await?;

            // Has Rx streams - spawn tasks to drain them
            // IMPORTANT: We must send Request BEFORE spawning drain tasks to ensure ordering.
            // We need to actually send the DriverMessage::Call to the driver's queue
            // before spawning drains, not just create the future.
            let request_id = self.shared.request_ids.next();
            let (response_tx, response_rx) = oneshot("call_with_rx_streams");

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
                            Some(IncomingChannelMessage::Data(payload)) => {
                                debug!(
                                    "drain task: received {} bytes on channel {}",
                                    payload.len(),
                                    channel_id
                                );
                                // Send data to driver
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
                                // Channel closed, send Close and exit
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

            // Just await the response - drain tasks run independently
            let result = response_rx
                .recv()
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

    /// Make an RPC call using reflection (non-generic).
    ///
    /// This is the non-generic core implementation that avoids monomorphization.
    /// The generic `call_with_metadata` delegates to this.
    ///
    /// # Safety
    ///
    /// - `args_ptr` must point to valid, initialized memory matching the plan's shape
    /// - The args type must be `Send`
    #[doc(hidden)]
    #[allow(unsafe_code)]
    pub unsafe fn call_with_metadata_by_plan(
        &self,
        method_id: u64,
        args_ptr: *mut (),
        args_plan: &crate::RpcPlan,
        metadata: roam_wire::Metadata,
    ) -> impl std::future::Future<Output = Result<ResponseData, TransportError>> + Send + '_ {
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

            if drains.is_empty() {
                // No Rx streams - simple call
                self.call_raw_with_channels_and_metadata(
                    method_id, channels, payload, args_debug, metadata,
                )
                .await
            } else {
                // Has Rx streams - spawn tasks to drain them
                // IMPORTANT: We must send Request BEFORE spawning drain tasks to ensure ordering.
                let request_id = self.shared.request_ids.next();
                let (response_tx, response_rx) = oneshot("call_raw_with_rx_streams");

                // Track outgoing request for diagnostics
                if let Some(diag) = &self.shared.diagnostic_state {
                    let args = args_debug.map(|s| {
                        let mut map = std::collections::HashMap::new();
                        map.insert("args".to_string(), s);
                        map
                    });
                    diag.record_outgoing_request(request_id, method_id, args);
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
                                            channel_id,
                                            "drain task failed to send DriverMessage::Data"
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

                // Just await the response - drain tasks run independently
                let result = response_rx
                    .recv()
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
        method_id: u64,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, TransportError> {
        self.call_raw_full(method_id, Vec::new(), Vec::new(), payload, None)
            .await
            .map(|r| r.payload)
    }

    /// Make a raw RPC call with pre-serialized payload and channel IDs.
    ///
    /// Used internally by `call()` after binding channels.
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
        metadata: roam_wire::Metadata,
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
        metadata: roam_wire::Metadata,
        channels: Vec<u64>,
        payload: Vec<u8>,
        args_debug: Option<String>,
    ) -> Result<ResponseData, TransportError> {
        #[cfg(not(target_arch = "wasm32"))]
        let _request_permit = self.acquire_request_slot().await?;
        #[cfg(target_arch = "wasm32")]
        self.acquire_request_slot().await?;

        let request_id = self.shared.request_ids.next();
        let (response_tx, response_rx) = oneshot("call_raw_with_channels");

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
            .recv()
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
            .unwrap()
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
            .unwrap()
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
            .unwrap()
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
        self.shared
            .channel_registry
            .lock()
            .unwrap()
            .close(channel_id);
    }

    /// Reset a channel.
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

    /// Check if a channel exists.
    pub fn contains_channel(&self, channel_id: ChannelId) -> bool {
        self.shared
            .channel_registry
            .lock()
            .unwrap()
            .contains(channel_id)
    }

    /// Receive credit for an outgoing channel.
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
    #[allow(unsafe_code)]
    pub fn bind_response_channels<T: Facet<'static>>(&self, response: &mut T, channels: &[u64]) {
        static PLAN: OnceLock<Arc<crate::RpcPlan>> = OnceLock::new();
        let plan = PLAN.get_or_init(|| Arc::new(crate::RpcPlan::for_type::<T>()));

        // Patch channel IDs from Response.channels into the deserialized response.
        // This is critical for ForwardingDispatcher where the payload contains upstream
        // channel IDs but channels[] contains the remapped downstream IDs.
        patch_channel_ids(response, channels);

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
    use std::time::Duration;

    #[tokio::test]
    async fn drain_task_exits_when_driver_data_send_fails() {
        let (driver_tx, mut driver_rx) = crate::runtime::channel("test_driver", 8);
        let handle = ConnectionHandle::new(driver_tx, Role::Initiator, u32::MAX);

        let (stream_tx, stream_rx) = crate::channel::<Vec<u8>>();
        let mut args = (stream_rx,);
        let call_task = tokio::spawn(async move { handle.call(42, &mut args).await });

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
            async move { handle.call_raw(1, vec![1]).await }
        });

        let first_msg = driver_rx.recv().await.expect("first call should be sent");
        let first_response_tx = match first_msg {
            DriverMessage::Call { response_tx, .. } => response_tx,
            _ => panic!("expected DriverMessage::Call for first request"),
        };

        let second = tokio::spawn({
            let handle = handle.clone();
            async move { handle.call_raw(2, vec![2]).await }
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
