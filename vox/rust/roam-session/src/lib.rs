#![deny(unsafe_code)]

//! Session/state machine and RPC-level utilities.
//!
//! Canonical definitions live in `docs/content/spec/_index.md`,
//! `docs/content/rust-spec/_index.md`, and `docs/content/shm-spec/_index.md`.

#[macro_use]
mod macros;

pub mod diagnostic;
pub mod driver;
pub mod runtime;
pub mod transport;

pub use driver::{
    ConnectError, ConnectionError, Driver, FramedClient, HandshakeConfig, IncomingConnection,
    IncomingConnections, MessageConnector, Negotiated, NoDispatcher, RetryPolicy, accept_framed,
    connect_framed, connect_framed_with_policy, initiate_framed,
};
pub use transport::MessageTransport;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::runtime::Sender;
use facet::Facet;

pub use roam_frame::{Frame, MsgDesc, OwnedMessage, Payload};

mod connection_handle;
pub use connection_handle::*;

mod caller;
pub use caller::*;

mod errors;
pub use errors::*;

mod types;
pub use types::*;

mod channel;
pub use channel::*;

mod tunnel;
pub use tunnel::*;

mod flow_control;
pub use flow_control::*;

pub(crate) const CHANNEL_SIZE: usize = 1024;
pub(crate) const RX_STREAM_BUFFER_SIZE: usize = 1024;

// ============================================================================
// Dispatch Context (task-local for response channel binding)
// ============================================================================

/// Context for binding response channels during dispatch.
///
/// When a service handler creates a channel with `roam::channel()` and returns
/// the Rx, the Tx needs to be bound to send Data over the wire. This context
/// provides the channel ID allocator and driver_tx needed for binding.
#[derive(Clone)]
struct DispatchContext {
    conn_id: roam_wire::ConnectionId,
    channel_ids: Arc<ChannelIdAllocator>,
    driver_tx: Sender<DriverMessage>,
}

roam_task_local::task_local! {
    /// Task-local dispatch context. Using task_local instead of thread_local
    /// is critical: thread_local can leak across different async tasks that
    /// happen to run on the same worker thread, causing channel binding bugs.
    static DISPATCH_CONTEXT: DispatchContext;
}

/// Get the current dispatch context, if any.
fn get_dispatch_context() -> Option<DispatchContext> {
    DISPATCH_CONTEXT.try_with(|ctx| ctx.clone()).ok()
}

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
// Dispatch Helper
// ============================================================================

/// Helper for dispatching RPC methods with minimal generated code.
///
/// This function handles the common dispatch pattern:
/// 1. Deserialize args from payload
/// 2. Bind any Tx/Rx streams via registry
/// 3. Call the handler closure
/// 4. Encode the result and send Response
///
/// The generated code just needs to provide a closure that calls the handler method.
///
/// # Type Parameters
///
/// - `A`: Args tuple type (must implement Facet for deserialization)
/// - `R`: Result ok type (must implement Facet for serialization)
/// - `E`: User error type (must implement Facet for serialization)
/// - `F`: Handler closure type
/// - `Fut`: Future returned by handler
///
/// # Example
///
/// ```ignore
/// fn dispatch_echo(&self, payload: Vec<u8>, request_id: u64, registry: &mut ChannelRegistry)
///     -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>
/// {
///     let handler = self.handler.clone();
///     dispatch_call(payload, request_id, registry, move |args: (String,)| async move {
///         handler.echo(args.0).await
///     })
/// }
/// ```
///
/// The handler returns `Result<R, E>` - user errors are automatically wrapped
/// in `RoamError::User(e)` for wire serialization.
///
/// The `channels` parameter contains channel IDs from the Request message framing.
/// These are patched into the deserialized args before binding streams.
pub fn dispatch_call<A, R, E, F, Fut>(
    cx: &Context,
    payload: Vec<u8>,
    registry: &mut ChannelRegistry,
    handler: F,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>
where
    A: Facet<'static> + Send,
    R: Facet<'static> + Send,
    E: Facet<'static> + Send,
    F: FnOnce(A) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<R, E>> + Send + 'static,
{
    let conn_id = cx.conn_id;
    let request_id = cx.request_id.raw();
    let channels = &cx.channels;

    // Deserialize args
    let mut args: A = match facet_postcard::from_slice(&payload) {
        Ok(args) => args,
        Err(_) => {
            let task_tx = registry.driver_tx();
            return Box::pin(async move {
                // InvalidPayload error: Result::Err(1) + RoamError::InvalidPayload(2)
                let _ = task_tx
                    .send(DriverMessage::Response {
                        conn_id,
                        request_id,
                        channels: Vec::new(),
                        payload: vec![1, 2],
                    })
                    .await;
            });
        }
    };

    // Patch channel IDs from Request framing into deserialized args
    debug!(channels = ?channels, "dispatch_call: patching channel IDs");
    patch_channel_ids(&mut args, channels);

    // Bind streams via reflection - THIS MUST HAPPEN SYNCHRONOUSLY
    debug!("dispatch_call: binding streams SYNC");
    registry.bind_streams(&mut args);
    debug!("dispatch_call: streams bound SYNC - channels should now be registered");

    let task_tx = registry.driver_tx();
    let dispatch_ctx = registry.dispatch_context();

    // Use task_local scope so roam::channel() creates bound channels.
    // This is critical: unlike thread_local, task_local won't leak to other
    // tasks that happen to run on the same worker thread.
    Box::pin(DISPATCH_CONTEXT.scope(dispatch_ctx, async move {
        debug!("dispatch_call: handler ASYNC starting");
        let result = handler(args).await;
        debug!("dispatch_call: handler ASYNC finished");
        let (payload, response_channels) = match result {
            Ok(ref ok_result) => {
                // Collect channel IDs from the result (e.g., Rx<T> in return type)
                let channels = collect_channel_ids(ok_result);
                // Result::Ok(0) + serialized value
                let mut out = vec![0u8];
                match facet_postcard::to_vec(ok_result) {
                    Ok(bytes) => out.extend(bytes),
                    Err(_) => return,
                }
                (out, channels)
            }
            Err(user_error) => {
                // Result::Err(1) + RoamError::User(0) + serialized user error
                let mut out = vec![1u8, 0u8];
                match facet_postcard::to_vec(&user_error) {
                    Ok(bytes) => out.extend(bytes),
                    Err(_) => return,
                }
                (out, Vec::new())
            }
        };

        // Send Response with channel IDs for any Rx<T> in the result.
        // ForwardingDispatcher uses these to set up Data forwarding.
        let _ = task_tx
            .send(DriverMessage::Response {
                conn_id,
                request_id,
                channels: response_channels,
                payload,
            })
            .await;
    }))
}

/// Dispatch helper for infallible methods (those that return `T` instead of `Result<T, E>`).
///
/// Same as `dispatch_call` but for handlers that cannot fail at the application level.
pub fn dispatch_call_infallible<A, R, F, Fut>(
    cx: &Context,
    payload: Vec<u8>,
    registry: &mut ChannelRegistry,
    handler: F,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>
where
    A: Facet<'static> + Send,
    R: Facet<'static> + Send,
    F: FnOnce(A) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = R> + Send + 'static,
{
    let conn_id = cx.conn_id;
    let request_id = cx.request_id.raw();
    let channels = &cx.channels;

    // Deserialize args
    let mut args: A = match facet_postcard::from_slice(&payload) {
        Ok(args) => args,
        Err(_) => {
            let task_tx = registry.driver_tx();
            return Box::pin(async move {
                // InvalidPayload error: Result::Err(1) + RoamError::InvalidPayload(2)
                let _ = task_tx
                    .send(DriverMessage::Response {
                        conn_id,
                        request_id,
                        channels: Vec::new(),
                        payload: vec![1, 2],
                    })
                    .await;
            });
        }
    };

    // Patch channel IDs from Request framing into deserialized args
    patch_channel_ids(&mut args, channels);

    // Bind streams via reflection
    registry.bind_streams(&mut args);

    let task_tx = registry.driver_tx();
    let dispatch_ctx = registry.dispatch_context();

    // Use task_local scope so roam::channel() creates bound channels.
    Box::pin(DISPATCH_CONTEXT.scope(dispatch_ctx, async move {
        let result = handler(args).await;

        // Collect channel IDs from the result (e.g., Rx<T> in return type)
        let response_channels = collect_channel_ids(&result);
        if !response_channels.is_empty() {
            debug!(
                channels = ?response_channels,
                "dispatch_call_infallible: collected response channels"
            );
        }

        // Result::Ok(0) + serialized value
        let mut payload = vec![0u8];
        match facet_postcard::to_vec(&result) {
            Ok(bytes) => payload.extend(bytes),
            Err(_) => return,
        }

        // Send Response with channel IDs for any Rx<T> in the result.
        // ForwardingDispatcher uses these to set up Data forwarding.
        let _ = task_tx
            .send(DriverMessage::Response {
                conn_id,
                request_id,
                channels: response_channels,
                payload,
            })
            .await;
    }))
}

/// Send an "unknown method" error response.
///
/// Used by dispatchers when the method_id doesn't match any known method.
pub fn dispatch_unknown_method(
    cx: &Context,
    registry: &mut ChannelRegistry,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
    let conn_id = cx.conn_id;
    let request_id = cx.request_id.raw();
    let task_tx = registry.driver_tx();
    Box::pin(async move {
        // UnknownMethod error
        let _ = task_tx
            .send(DriverMessage::Response {
                conn_id,
                request_id,
                channels: Vec::new(),
                payload: vec![1, 1],
            })
            .await;
    })
}

/// Collect channel IDs from args by walking with Peek.
///
/// Returns channel IDs in declaration order (depth-first traversal).
/// Used by the client to populate the `channels` vec in Request messages.
///
/// r[impl call.request.channels] - Collects channel IDs in declaration order for the Request.
pub fn collect_channel_ids<T: Facet<'static>>(args: &T) -> Vec<u64> {
    let mut ids = Vec::new();
    let poke = facet::Peek::new(args);
    collect_channel_ids_recursive(poke, &mut ids);
    ids
}

fn collect_channel_ids_recursive(peek: facet::Peek<'_, '_>, ids: &mut Vec<u64>) {
    let shape = peek.shape();

    // Check if this is an Rx or Tx type
    if shape.decl_id == Rx::<()>::SHAPE.decl_id || shape.decl_id == Tx::<()>::SHAPE.decl_id {
        // Read the channel_id field
        if let Ok(ps) = peek.into_struct()
            && let Ok(channel_id_field) = ps.field_by_name("channel_id")
            && let Ok(&channel_id) = channel_id_field.get::<ChannelId>()
        {
            ids.push(channel_id);
        }
        return;
    }

    // Recurse into struct/tuple fields
    if let Ok(ps) = peek.into_struct() {
        let field_count = ps.field_count();
        for i in 0..field_count {
            if let Ok(field_peek) = ps.field(i) {
                collect_channel_ids_recursive(field_peek, ids);
            }
        }
        return;
    }

    // Recurse into Option<T> (specialized handling)
    if let Ok(po) = peek.into_option() {
        if let Some(inner) = po.value() {
            collect_channel_ids_recursive(inner, ids);
        }
        return;
    }

    // Recurse into enum variants (for other enums with data)
    if let Ok(pe) = peek.into_enum() {
        // Try to get the first field of the active variant (e.g., Some(T) has one field)
        if let Ok(Some(variant_peek)) = pe.field(0) {
            collect_channel_ids_recursive(variant_peek, ids);
        }
        return;
    }

    // Recurse into sequences (e.g., Vec<Tx<T>>)
    if let Ok(pl) = peek.into_list() {
        for element in pl.iter() {
            collect_channel_ids_recursive(element, ids);
        }
    }
}

/// Patch channel IDs into deserialized args by walking with Poke.
///
/// Overwrites channel_id fields in Rx/Tx in declaration order.
/// Used by the server to apply the authoritative `channels` vec from Request.
pub fn patch_channel_ids<T: Facet<'static>>(args: &mut T, channels: &[u64]) {
    debug!(channels = ?channels, "patch_channel_ids: patching channels from wire");
    let mut idx = 0;
    let poke = facet::Poke::new(args);
    patch_channel_ids_recursive(poke, channels, &mut idx);
}

#[allow(unsafe_code)]
fn patch_channel_ids_recursive(mut poke: facet::Poke<'_, '_>, channels: &[u64], idx: &mut usize) {
    use facet::Def;

    let shape = poke.shape();

    // Check if this is an Rx or Tx type
    if shape.decl_id == Rx::<()>::SHAPE.decl_id || shape.decl_id == Tx::<()>::SHAPE.decl_id {
        // Overwrite the channel_id field
        if let Ok(mut ps) = poke.into_struct()
            && let Ok(mut channel_id_field) = ps.field_by_name("channel_id")
            && let Ok(channel_id_ref) = channel_id_field.get_mut::<ChannelId>()
            && *idx < channels.len()
        {
            *channel_id_ref = channels[*idx];
            *idx += 1;
        }
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
                    patch_channel_ids_recursive(field_poke, channels, idx);
                }
            }
        }

        // Recurse into Option<T>
        Def::Option(_) => {
            // Option is represented as an enum, use into_enum to access its value
            if let Ok(mut pe) = poke.into_enum()
                && let Ok(Some(inner_poke)) = pe.field(0)
            {
                patch_channel_ids_recursive(inner_poke, channels, idx);
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
                        patch_channel_ids_recursive(element_poke, channels, idx);
                    }
                }
            }
        }

        // Other enum variants
        _ if poke.is_enum() => {
            if let Ok(mut pe) = poke.into_enum()
                && let Ok(Some(variant_poke)) = pe.field(0)
            {
                patch_channel_ids_recursive(variant_poke, channels, idx);
            }
        }

        _ => {}
    }
}

// ============================================================================
// Service Dispatcher
// ============================================================================

/// Context passed to service method implementations.
///
/// Contains information about the request that may be useful to the handler:
/// - `conn_id`: Which virtual connection the request came from
/// - `metadata`: Key-value pairs sent with the request
///
/// This enables services to identify callers and access per-request metadata.
#[derive(Debug, Clone)]
pub struct Context {
    /// The connection ID this request arrived on.
    ///
    /// For virtual connections, this identifies which specific connection
    /// the request came from, enabling bidirectional communication.
    pub conn_id: roam_wire::ConnectionId,

    /// The request ID for this call.
    ///
    /// Unique within the connection; used for response routing and cancellation.
    pub request_id: roam_wire::RequestId,

    /// The method ID being called.
    pub method_id: roam_wire::MethodId,

    /// Metadata sent with the request.
    ///
    /// This is the `metadata` field from the wire `Request` message.
    pub metadata: roam_wire::Metadata,

    /// Channel IDs from the request, in argument declaration order.
    ///
    /// Used for stream binding. Proxies can use this to remap channel IDs.
    pub channels: Vec<u64>,
}

impl Context {
    /// Create a new context.
    pub fn new(
        conn_id: roam_wire::ConnectionId,
        request_id: roam_wire::RequestId,
        method_id: roam_wire::MethodId,
        metadata: roam_wire::Metadata,
        channels: Vec<u64>,
    ) -> Self {
        Self {
            conn_id,
            request_id,
            method_id,
            metadata,
            channels,
        }
    }

    /// Get the connection ID.
    pub fn conn_id(&self) -> roam_wire::ConnectionId {
        self.conn_id
    }

    /// Get the request ID.
    pub fn request_id(&self) -> roam_wire::RequestId {
        self.request_id
    }

    /// Get the method ID.
    pub fn method_id(&self) -> roam_wire::MethodId {
        self.method_id
    }

    /// Get the request metadata.
    pub fn metadata(&self) -> &roam_wire::Metadata {
        &self.metadata
    }

    /// Get the channel IDs.
    pub fn channels(&self) -> &[u64] {
        &self.channels
    }
}

/// Trait for dispatching requests to a service.
///
/// The dispatcher handles both simple and channeling methods uniformly.
/// Stream binding is done via reflection (Poke) on the deserialized args.
pub trait ServiceDispatcher: Send + Sync {
    /// Returns the method IDs this dispatcher handles.
    ///
    /// Used by [`RoutedDispatcher`] to determine which methods to route
    /// to which dispatcher.
    fn method_ids(&self) -> Vec<u64>;

    /// Dispatch a request and send the response via the task channel.
    ///
    /// The dispatcher is responsible for:
    /// - Looking up the method by `cx.method_id()`
    /// - Deserializing arguments from payload
    /// - Patching channel IDs from `cx.channels()` into deserialized args via `patch_channel_ids()`
    /// - Binding any Tx/Rx streams via the registry
    /// - Calling the service method
    /// - Sending Data/Close messages for any Tx streams
    /// - Sending the Response message via DriverMessage::Response
    ///
    /// By using a single channel for Data/Close/Response, correct ordering is guaranteed:
    /// all stream Data and Close messages are sent before the Response.
    ///
    /// The `cx.channels()` contains channel IDs from the Request message framing,
    /// in declaration order. For a ForwardingDispatcher, this enables transparent proxying
    /// without parsing the payload.
    ///
    /// Returns a boxed future with `'static` lifetime so it can be spawned.
    /// Implementations should clone their service into the future to achieve this.
    ///
    /// r[impl channeling.allocation.caller] - Stream IDs are from Request.channels (caller allocated).
    fn dispatch(
        &self,
        cx: &Context,
        payload: Vec<u8>,
        registry: &mut ChannelRegistry,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>;
}

/// A dispatcher that routes to one of two dispatchers based on method ID.
///
/// Methods handled by `primary` (via [`ServiceDispatcher::method_ids`]) are
/// routed to it; all other methods are routed to `fallback`.
pub struct RoutedDispatcher<A, B> {
    primary: A,
    fallback: B,
    primary_methods: Vec<u64>,
}

impl<A, B> RoutedDispatcher<A, B>
where
    A: ServiceDispatcher,
{
    /// Create a new routed dispatcher.
    ///
    /// Methods declared by `primary.method_ids()` are routed to `primary`,
    /// all others to `fallback`.
    pub fn new(primary: A, fallback: B) -> Self {
        let primary_methods = primary.method_ids();
        Self {
            primary,
            fallback,
            primary_methods,
        }
    }
}

impl<A, B> ServiceDispatcher for RoutedDispatcher<A, B>
where
    A: ServiceDispatcher,
    B: ServiceDispatcher,
{
    fn method_ids(&self) -> Vec<u64> {
        let mut ids = self.primary_methods.clone();
        ids.extend(self.fallback.method_ids());
        ids
    }

    fn dispatch(
        &self,
        cx: &Context,
        payload: Vec<u8>,
        registry: &mut ChannelRegistry,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        if self.primary_methods.contains(&cx.method_id().raw()) {
            self.primary.dispatch(cx, payload, registry)
        } else {
            self.fallback.dispatch(cx, payload, registry)
        }
    }
}

// ============================================================================
// ForwardingDispatcher - Transparent RPC Proxy
// ============================================================================

/// A dispatcher that forwards all requests to an upstream connection.
///
/// This enables transparent proxying without knowing the service schema.
/// Channel IDs are remapped automatically: the proxy allocates new channel IDs
/// for the upstream connection and maintains bidirectional forwarding.
///
/// # Example
///
/// ```ignore
/// use roam_session::{ForwardingDispatcher, ConnectionHandle};
///
/// // Upstream connection to the actual service
/// let upstream: ConnectionHandle = /* ... */;
///
/// // Create a forwarding dispatcher
/// let proxy = ForwardingDispatcher::new(upstream);
///
/// // Use with accept() - all calls will be forwarded to upstream
/// let (handle, driver) = accept(stream, config, proxy).await?;
/// ```
pub struct ForwardingDispatcher {
    upstream: ConnectionHandle,
}

impl ForwardingDispatcher {
    /// Create a new forwarding dispatcher that proxies to the upstream connection.
    pub fn new(upstream: ConnectionHandle) -> Self {
        Self { upstream }
    }
}

impl Clone for ForwardingDispatcher {
    fn clone(&self) -> Self {
        Self {
            upstream: self.upstream.clone(),
        }
    }
}

impl ServiceDispatcher for ForwardingDispatcher {
    /// Returns empty - this dispatcher accepts all method IDs.
    fn method_ids(&self) -> Vec<u64> {
        vec![]
    }

    fn dispatch(
        &self,
        cx: &Context,
        payload: Vec<u8>,
        registry: &mut ChannelRegistry,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        let task_tx = registry.driver_tx();
        let upstream = self.upstream.clone();
        let conn_id = cx.conn_id;
        let method_id = cx.method_id.raw();
        let request_id = cx.request_id.raw();
        let channels = cx.channels.clone();

        if channels.is_empty() {
            // Unary call - but response may contain Rx<T> channels
            // We need to set up forwarding for any response channels.
            //
            // IMPORTANT: Upstream and downstream use different channel ID spaces.
            // The upstream channel IDs must be remapped to downstream channel IDs.
            let downstream_channel_ids = registry.response_channel_ids();

            Box::pin(async move {
                let response = upstream
                    .call_raw_with_channels(method_id, vec![], payload, None)
                    .await;

                let (response_payload, upstream_response_channels) = match response {
                    Ok(data) => (data.payload, data.channels),
                    Err(TransportError::Encode(_)) => {
                        // Should not happen for raw call
                        (vec![1, 2], Vec::new()) // Err(InvalidPayload)
                    }
                    Err(TransportError::ConnectionClosed) | Err(TransportError::DriverGone) => {
                        // Connection to upstream failed - return Cancelled
                        (vec![1, 3], Vec::new()) // Err(Cancelled)
                    }
                };

                // If response has channels (e.g., method returns Rx<T>),
                // set up forwarding for Data from upstream to downstream.
                // We allocate new downstream channel IDs and remap when forwarding.
                let mut downstream_channels = Vec::new();
                if !upstream_response_channels.is_empty() {
                    debug!(
                        upstream_channels = ?upstream_response_channels,
                        "ForwardingDispatcher: setting up response channel forwarding"
                    );
                    for &upstream_id in &upstream_response_channels {
                        // Allocate a downstream channel ID
                        let downstream_id = downstream_channel_ids.next();
                        downstream_channels.push(downstream_id);

                        debug!(
                            upstream_id,
                            downstream_id, "ForwardingDispatcher: mapping channel IDs"
                        );

                        // Set up forwarding: upstream → downstream
                        let (tx, mut rx) = crate::runtime::channel::<Vec<u8>>(64);
                        upstream.register_incoming(upstream_id, tx);

                        let task_tx_clone = task_tx.clone();
                        crate::runtime::spawn(async move {
                            debug!(
                                upstream_id,
                                downstream_id, "ForwardingDispatcher: forwarding task started"
                            );
                            while let Some(data) = rx.recv().await {
                                debug!(
                                    upstream_id,
                                    downstream_id,
                                    data_len = data.len(),
                                    "ForwardingDispatcher: forwarding data"
                                );
                                let _ = task_tx_clone
                                    .send(DriverMessage::Data {
                                        conn_id,
                                        channel_id: downstream_id,
                                        payload: data,
                                    })
                                    .await;
                            }
                            debug!(
                                upstream_id,
                                downstream_id,
                                "ForwardingDispatcher: forwarding task ended, sending Close"
                            );
                            // Channel closed
                            let _ = task_tx_clone
                                .send(DriverMessage::Close {
                                    conn_id,
                                    channel_id: downstream_id,
                                })
                                .await;
                        });
                    }
                }

                let _ = task_tx
                    .send(DriverMessage::Response {
                        conn_id,
                        request_id,
                        channels: downstream_channels,
                        payload: response_payload,
                    })
                    .await;
            })
        } else {
            // Streaming call - set up bidirectional channel forwarding
            //
            // IMPORTANT: We must send the upstream Request BEFORE any Data is
            // forwarded, otherwise the backend will reject Data for unknown channels.
            //
            // Strategy:
            // 1. Register incoming handlers synchronously (buffers Data in mpsc channels)
            // 2. In the async block: send Request first, then spawn forwarding tasks
            //    (spawning AFTER Request is sent is safe - ordering is established)

            // Allocate upstream channel IDs and set up buffering channels
            let mut upstream_channels = Vec::with_capacity(channels.len());
            let mut ds_to_us_rxs = Vec::with_capacity(channels.len());
            let mut us_to_ds_rxs = Vec::with_capacity(channels.len());
            let mut channel_map = Vec::with_capacity(channels.len());

            let upstream_task_tx = upstream.driver_tx();

            for &downstream_id in &channels {
                let upstream_id = upstream.alloc_channel_id();
                upstream_channels.push(upstream_id);
                channel_map.push((downstream_id, upstream_id));

                // Buffer for downstream → upstream (client sends Data)
                let (ds_to_us_tx, ds_to_us_rx) = crate::runtime::channel(64);
                registry.register_incoming(downstream_id, ds_to_us_tx);
                ds_to_us_rxs.push(ds_to_us_rx);

                // Buffer for upstream → downstream (server sends Data)
                let (us_to_ds_tx, us_to_ds_rx) = crate::runtime::channel(64);
                upstream.register_incoming(upstream_id, us_to_ds_tx);
                us_to_ds_rxs.push(us_to_ds_rx);
            }

            // Everything below runs in the async block
            Box::pin(async move {
                // Send the upstream Request - this queues the Request command
                // which will be sent before any Data we forward
                let response_future =
                    upstream.call_raw_with_channels(method_id, upstream_channels, payload, None);

                // Now spawn forwarding tasks - safe because Request is queued first
                // and command_tx/task_tx are processed in order by the driver
                let upstream_conn_id = upstream.conn_id();
                for (i, mut rx) in ds_to_us_rxs.into_iter().enumerate() {
                    let upstream_id = channel_map[i].1;
                    let upstream_task_tx = upstream_task_tx.clone();
                    crate::runtime::spawn(async move {
                        while let Some(data) = rx.recv().await {
                            let _ = upstream_task_tx
                                .send(DriverMessage::Data {
                                    conn_id: upstream_conn_id,
                                    channel_id: upstream_id,
                                    payload: data,
                                })
                                .await;
                        }
                        // Channel closed
                        let _ = upstream_task_tx
                            .send(DriverMessage::Close {
                                conn_id: upstream_conn_id,
                                channel_id: upstream_id,
                            })
                            .await;
                    });
                }

                for (i, mut rx) in us_to_ds_rxs.into_iter().enumerate() {
                    let downstream_id = channel_map[i].0;
                    let task_tx = task_tx.clone();
                    crate::runtime::spawn(async move {
                        while let Some(data) = rx.recv().await {
                            let _ = task_tx
                                .send(DriverMessage::Data {
                                    conn_id,
                                    channel_id: downstream_id,
                                    payload: data,
                                })
                                .await;
                        }
                        // Channel closed
                        let _ = task_tx
                            .send(DriverMessage::Close {
                                conn_id,
                                channel_id: downstream_id,
                            })
                            .await;
                    });
                }

                // Wait for upstream response
                let response = response_future.await;

                let (response_payload, upstream_response_channels) = match response {
                    Ok(data) => (data.payload, data.channels),
                    Err(TransportError::Encode(_)) => {
                        (vec![1, 2], Vec::new()) // Err(InvalidPayload)
                    }
                    Err(TransportError::ConnectionClosed) | Err(TransportError::DriverGone) => {
                        (vec![1, 3], Vec::new()) // Err(Cancelled)
                    }
                };

                // Map upstream response channels back to downstream channel IDs.
                // The downstream client allocated the original IDs and expects them
                // in the Response, not the upstream IDs we allocated for forwarding.
                let downstream_response_channels: Vec<u64> = upstream_response_channels
                    .iter()
                    .filter_map(|&upstream_id| {
                        channel_map
                            .iter()
                            .find(|(_, us)| *us == upstream_id)
                            .map(|(ds, _)| *ds)
                    })
                    .collect();

                let _ = task_tx
                    .send(DriverMessage::Response {
                        conn_id,
                        request_id,
                        channels: downstream_response_channels,
                        payload: response_payload,
                    })
                    .await;
            })
        }
    }
}

// ============================================================================
// LateBoundForwarder - Forwarding with Deferred Handle Binding
// ============================================================================

/// A handle that can be set once after creation.
///
/// This solves the chicken-and-egg problem in bidirectional proxying where:
/// 1. You need to pass a dispatcher to `connect()` for reverse-direction calls
/// 2. But the dispatcher needs a handle that's only available after `accept_framed()`
///
/// # Example
///
/// ```ignore
/// // Create the late-bound handle (empty initially)
/// let late_bound = LateBoundHandle::new();
///
/// // Pass a forwarder using this handle to connect()
/// let virtual_conn = handle.connect(
///     metadata,
///     Some(Box::new(LateBoundForwarder::new(late_bound.clone()))),
/// ).await?;
///
/// // Accept the other connection to get its handle
/// let (browser_handle, driver) = accept_framed(transport, config, dispatcher).await?;
///
/// // NOW bind the handle - any incoming calls will be forwarded
/// late_bound.set(browser_handle);
/// ```
#[derive(Clone)]
pub struct LateBoundHandle {
    inner: Arc<std::sync::OnceLock<ConnectionHandle>>,
}

impl LateBoundHandle {
    /// Create a new unbound handle.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(std::sync::OnceLock::new()),
        }
    }

    /// Bind the handle to a connection. Can only be called once.
    ///
    /// # Panics
    ///
    /// Panics if called more than once.
    pub fn set(&self, handle: ConnectionHandle) {
        if self.inner.set(handle).is_err() {
            panic!("LateBoundHandle::set called more than once");
        }
    }

    /// Try to get the bound handle, if set.
    pub fn get(&self) -> Option<&ConnectionHandle> {
        self.inner.get()
    }
}

impl Default for LateBoundHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// A dispatcher that forwards all requests to a late-bound upstream connection.
///
/// Like [`ForwardingDispatcher`], but the upstream handle is provided after creation
/// via [`LateBoundHandle::set`]. This enables bidirectional proxying scenarios.
///
/// If a request arrives before the handle is bound, it returns `Cancelled`.
///
/// # Example
///
/// ```ignore
/// // Create late-bound handle and forwarder
/// let late_bound = LateBoundHandle::new();
/// let forwarder = LateBoundForwarder::new(late_bound.clone());
///
/// // Use forwarder with connect() for reverse-direction calls
/// let virtual_conn = handle.connect(metadata, Some(Box::new(forwarder))).await?;
///
/// // Later, bind the actual handle
/// let (browser_handle, driver) = accept_framed(...).await?;
/// late_bound.set(browser_handle);
/// ```
pub struct LateBoundForwarder {
    upstream: LateBoundHandle,
}

impl LateBoundForwarder {
    /// Create a new late-bound forwarding dispatcher.
    pub fn new(upstream: LateBoundHandle) -> Self {
        Self { upstream }
    }
}

impl Clone for LateBoundForwarder {
    fn clone(&self) -> Self {
        Self {
            upstream: self.upstream.clone(),
        }
    }
}

impl ServiceDispatcher for LateBoundForwarder {
    fn method_ids(&self) -> Vec<u64> {
        vec![]
    }

    fn dispatch(
        &self,
        cx: &Context,
        payload: Vec<u8>,
        registry: &mut ChannelRegistry,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        let task_tx = registry.driver_tx();
        let conn_id = cx.conn_id;
        let request_id = cx.request_id.raw();

        // Try to get the upstream handle
        let Some(upstream) = self.upstream.get().cloned() else {
            // Handle not bound yet - return Cancelled
            debug!(
                method_id = cx.method_id.raw(),
                "LateBoundForwarder: upstream not bound, returning Cancelled"
            );
            return Box::pin(async move {
                let _ = task_tx
                    .send(DriverMessage::Response {
                        conn_id,
                        request_id,
                        channels: vec![],
                        payload: vec![1, 3], // Err(Cancelled)
                    })
                    .await;
            });
        };

        // Delegate to ForwardingDispatcher now that we have the handle
        ForwardingDispatcher::new(upstream).dispatch(cx, payload, registry)
    }
}

// TODO: Remove this shim once facet implements `Facet` for `core::convert::Infallible`
// and for the never type `!` (facet-rs/facet#1668), then use `Infallible`.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Never;

// ============================================================================
// Connection Handle (Client-side API)
// ============================================================================

/// Decode a response payload into the expected type.
///
/// This is the core response decoding logic used by generated clients.
/// It handles the wire format: `[0] + value_bytes` for Ok, `[1, discriminant] + error_bytes` for Err.
///
/// Returns `Result<T, CallError<E>>` with the decoded value or error.
pub fn decode_response<T: Facet<'static>, E: Facet<'static>>(
    payload: &[u8],
) -> Result<T, CallError<E>> {
    if payload.is_empty() {
        return Err(DecodeError::EmptyPayload.into());
    }

    match payload[0] {
        0 => {
            // Ok variant: deserialize the value
            facet_postcard::from_slice(&payload[1..]).map_err(CallError::Decode)
        }
        1 => {
            // Err variant: deserialize RoamError<E>
            if payload.len() < 2 {
                return Err(DecodeError::TruncatedError.into());
            }
            let roam_error = match payload[1] {
                0 => {
                    // User error
                    let user_error: E =
                        facet_postcard::from_slice(&payload[2..]).map_err(CallError::Decode)?;
                    RoamError::User(user_error)
                }
                1 => RoamError::UnknownMethod,
                2 => RoamError::InvalidPayload,
                3 => RoamError::Cancelled,
                d => return Err(DecodeError::UnknownRoamErrorDiscriminant(d).into()),
            };
            Err(CallError::Roam(roam_error))
        }
        d => Err(DecodeError::InvalidResultDiscriminant(d).into()),
    }
}

/// Shared state between ConnectionHandle and Driver.
struct HandleShared {
    /// Connection ID for this handle (0 = root connection).
    conn_id: roam_wire::ConnectionId,
    /// Unified channel to send all messages to the driver.
    driver_tx: Sender<DriverMessage>,
    /// Request ID generator.
    request_ids: RequestIdGenerator,
    /// Stream ID allocator.
    channel_ids: ChannelIdAllocator,
    /// Stream registry for routing incoming data.
    /// Protected by a mutex since handles may create streams concurrently.
    channel_registry: std::sync::Mutex<ChannelRegistry>,
    /// Optional diagnostic state for SIGUSR1 dumps.
    diagnostic_state: Option<Arc<crate::diagnostic::DiagnosticState>>,
}

#[cfg(test)]
mod tests;
