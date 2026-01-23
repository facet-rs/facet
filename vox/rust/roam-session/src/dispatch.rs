//! Dispatch machinery for routing RPC requests to service handlers.
//!
//! This module contains:
//! - [`dispatch_call`] and [`dispatch_call_infallible`] - helpers for generated dispatchers
//! - [`Context`] - request context passed to handlers
//! - [`ServiceDispatcher`] trait - implemented by generated service dispatchers
//! - [`RoutedDispatcher`] - routes to different dispatchers by method ID

use std::sync::Arc;

use facet::Facet;

use crate::{
    ChannelId, ChannelIdAllocator, ChannelRegistry, DriverMessage, Rx, Tx, runtime::Sender,
};

// ============================================================================
// Dispatch Context (task-local for response channel binding)
// ============================================================================

/// Context for binding response channels during dispatch.
///
/// When a service handler creates a channel with `roam::channel()` and returns
/// the Rx, the Tx needs to be bound to send Data over the wire. This context
/// provides the channel ID allocator and driver_tx needed for binding.
#[derive(Clone)]
pub(crate) struct DispatchContext {
    pub(crate) conn_id: roam_wire::ConnectionId,
    pub(crate) channel_ids: Arc<ChannelIdAllocator>,
    pub(crate) driver_tx: Sender<DriverMessage>,
}

roam_task_local::task_local! {
    /// Task-local dispatch context. Using task_local instead of thread_local
    /// is critical: thread_local can leak across different async tasks that
    /// happen to run on the same worker thread, causing channel binding bugs.
    pub(crate) static DISPATCH_CONTEXT: DispatchContext;
}

/// Get the current dispatch context, if any.
pub(crate) fn get_dispatch_context() -> Option<DispatchContext> {
    DISPATCH_CONTEXT.try_with(|ctx| ctx.clone()).ok()
}

// ============================================================================
// Request Context
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

// ============================================================================
// Dispatch Helpers
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

// ============================================================================
// Channel ID Collection and Patching
// ============================================================================

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
// Service Dispatcher Trait
// ============================================================================

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

// ============================================================================
// Routed Dispatcher
// ============================================================================

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
