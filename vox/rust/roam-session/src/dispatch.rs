//! Dispatch machinery for routing RPC requests to service handlers.
//!
//! This module contains:
//! - [`dispatch_call`] and [`dispatch_call_infallible`] - helpers for generated dispatchers
//! - [`Context`] - request context passed to handlers
//! - [`ServiceDispatcher`] trait - implemented by generated service dispatchers
//! - [`RoutedDispatcher`] - routes to different dispatchers by method ID

use std::sync::Arc;

use facet::Facet;
use facet_core::{PtrConst, PtrMut, PtrUninit, Shape};
use facet_format::{FormatDeserializer, MetaSource};
use facet_path::PathAccessError;
use facet_postcard::PostcardParser;
use facet_reflect::Partial;

use crate::{
    ChannelId, ChannelIdAllocator, ChannelRegistry, DriverMessage, Extensions, Middleware,
    Rejection, RpcPlan, SendPeek, runtime::Sender,
};

// ============================================================================
// Dispatch Context (task-local for response channel binding)
// ============================================================================

/// Context for binding response channels during dispatch.
///
/// When a service handler creates a channel with `roam::channel()` and returns
/// the Rx, the Tx needs to be bound to send Data over the wire. This context
/// provides the channel ID allocator and driver_tx needed for binding.
///
/// This is public for use by generated dispatchers with `DISPATCH_CONTEXT.scope()`.
#[derive(Clone)]
pub struct DispatchContext {
    pub(crate) conn_id: roam_wire::ConnectionId,
    pub(crate) channel_ids: Arc<ChannelIdAllocator>,
    pub(crate) driver_tx: Sender<DriverMessage>,
}

roam_task_local::task_local! {
    /// Task-local dispatch context. Using task_local instead of thread_local
    /// is critical: thread_local can leak across different async tasks that
    /// happen to run on the same worker thread, causing channel binding bugs.
    ///
    /// This is public for use by generated dispatchers.
    pub static DISPATCH_CONTEXT: DispatchContext;

    /// Task-local extensions from the current request context.
    ///
    /// This allows code running inside a handler (including `Caller` implementations
    /// like `TracingCaller`) to access extensions set by middleware, without needing
    /// direct access to the `Context`.
    ///
    /// Generated dispatchers scope this around the handler call.
    pub static CURRENT_EXTENSIONS: Extensions;

    /// Task-local metadata for the current inbound RPC call.
    ///
    /// Outgoing calls can use this as propagation source.
    pub static CURRENT_CALL_METADATA: roam_wire::Metadata;
}

/// Get the current dispatch context, if any.
pub(crate) fn get_dispatch_context() -> Option<DispatchContext> {
    DISPATCH_CONTEXT.try_with(|ctx| ctx.clone()).ok()
}

/// Get metadata from the current inbound RPC context, if any.
pub(crate) fn get_current_call_metadata() -> Option<roam_wire::Metadata> {
    CURRENT_CALL_METADATA.try_with(|m| m.clone()).ok()
}

// ============================================================================
// Request Context
// ============================================================================

/// Context passed to service method implementations.
///
/// Contains information about the request that may be useful to the handler:
/// - `conn_id`: Which virtual connection the request came from
/// - `metadata`: Key-value pairs sent with the request
/// - `extensions`: Type-safe storage for values inserted by middleware
///
/// This enables services to identify callers and access per-request metadata.
#[derive(Debug)]
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

    /// Method descriptor from the active service definition.
    ///
    /// Populated by the connection driver when the selected dispatcher can
    /// resolve `method_id` against its static method descriptors.
    pub method_descriptor: Option<&'static MethodDescriptor>,

    /// Metadata sent with the request.
    ///
    /// This is the `metadata` field from the wire `Request` message.
    pub metadata: roam_wire::Metadata,

    /// Channel IDs from the request, in argument declaration order.
    ///
    /// Used for channel binding. Proxies can use this to remap channel IDs.
    pub channels: Vec<u64>,

    /// Type-safe extension storage.
    ///
    /// Middleware can insert values here (e.g., authenticated user info)
    /// that handlers can later retrieve.
    pub extensions: Extensions,

    /// Argument names for the method being called.
    ///
    /// Set by the generated dispatcher. Middleware can use this to create
    /// per-argument span attributes (e.g., `rpc.args.user_id`).
    pub arg_names: &'static [&'static str],
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
            method_descriptor: None,
            metadata,
            channels,
            extensions: Extensions::new(),
            arg_names: &[],
        }
    }

    /// Set the method descriptor from dispatcher descriptors.
    pub fn with_method_descriptor(
        mut self,
        method_descriptor: Option<&'static MethodDescriptor>,
    ) -> Self {
        self.method_descriptor = method_descriptor;
        self
    }

    /// Set the argument names for this context.
    ///
    /// Called by generated dispatchers before invoking middleware.
    pub fn with_arg_names(mut self, arg_names: &'static [&'static str]) -> Self {
        self.arg_names = arg_names;
        self
    }

    /// Get the argument names.
    pub fn arg_names(&self) -> &'static [&'static str] {
        self.arg_names
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

    /// Get the method descriptor for this request, if known.
    pub fn method_descriptor(&self) -> Option<&'static MethodDescriptor> {
        self.method_descriptor
    }

    /// Get the fully-qualified method name for this request, if known.
    pub fn method_name(&self) -> Option<String> {
        self.method_descriptor
            .map(|d| format!("{}.{}", d.service_name, d.method_name))
    }
}

impl Clone for Context {
    fn clone(&self) -> Self {
        Self {
            conn_id: self.conn_id,
            request_id: self.request_id,
            method_id: self.method_id,
            method_descriptor: self.method_descriptor,
            metadata: self.metadata.clone(),
            channels: self.channels.clone(),
            // Extensions are NOT cloned - each clone gets fresh extensions.
            // This is intentional: middleware modifies extensions on its copy,
            // but the inner dispatch already captured what it needs.
            extensions: Extensions::new(),
            arg_names: self.arg_names,
        }
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
/// These are patched into the deserialized args before binding channels.
///
/// **IMPORTANT**: Create `ARGS_PLAN` and `RESPONSE_PLAN` statics in your generated
/// dispatch code (one per endpoint), NOT inside a generic function, then pass them here.
#[allow(unsafe_code)]
pub fn dispatch_call<A, R, E, F, Fut>(
    cx: &Context,
    payload: Vec<u8>,
    registry: &mut ChannelRegistry,
    args_plan: &RpcPlan,
    response_plan: &'static RpcPlan,
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

    // Use MaybeUninit to avoid heap allocation for args.
    // Deserialization happens via non-generic prepare_sync.
    let mut args_slot = std::mem::MaybeUninit::<A>::uninit();

    // SAFETY: args_slot is properly aligned and sized for A.
    // prepare_sync will initialize it on success.
    let prepare_result = unsafe {
        prepare_sync(
            args_slot.as_mut_ptr().cast(),
            args_plan,
            &payload,
            &cx.channels,
            registry,
        )
    };

    let task_tx = registry.driver_tx();

    // Handle prepare errors - this is non-generic
    if let Err(e) = prepare_result {
        return Box::pin(async move {
            send_prepare_error(e, &task_tx, conn_id, request_id).await;
        });
    }

    // SAFETY: prepare_sync succeeded, so args_slot is initialized
    let args = unsafe { args_slot.assume_init() };

    let dispatch_ctx = registry.dispatch_context();

    // Use task_local scope so roam::channel() creates bound channels.
    // This is critical: unlike thread_local, task_local won't leak to other
    // tasks that happen to run on the same worker thread.
    let propagated_metadata = cx.metadata.clone();
    Box::pin(DISPATCH_CONTEXT.scope(dispatch_ctx, async move {
        CURRENT_CALL_METADATA
            .scope(propagated_metadata, async move {
                trace!("dispatch_call: handler starting");
                let result = handler(args).await;
                trace!("dispatch_call: handler finished");

                match result {
                    Ok(ref ok_result) => {
                        // Use non-generic send_ok_response via SendPeek
                        // SAFETY: R is Send (from where clause), ok_result outlives this scope,
                        // and we don't mutate it while the Peek exists
                        let peek = facet::Peek::new(ok_result);
                        let send_peek = unsafe { SendPeek::new(peek) };
                        send_ok_response(send_peek, response_plan, &task_tx, conn_id, request_id)
                            .await;
                    }
                    Err(ref user_error) => {
                        // Use non-generic send_error_response via SendPeek
                        // SAFETY: E is Send (from where clause), user_error outlives this scope,
                        // and we don't mutate it while the Peek exists
                        let peek = facet::Peek::new(user_error);
                        let send_peek = unsafe { SendPeek::new(peek) };
                        send_error_response(send_peek, &task_tx, conn_id, request_id).await;
                    }
                }
            })
            .await
    }))
}

/// Dispatch helper for infallible methods (those that return `T` instead of `Result<T, E>`).
///
/// Same as `dispatch_call` but for handlers that cannot fail at the application level.
/// Requires `args_plan` and `response_plan` - create these as statics in non-generic code.
#[allow(unsafe_code)]
pub fn dispatch_call_infallible<A, R, F, Fut>(
    cx: &Context,
    payload: Vec<u8>,
    registry: &mut ChannelRegistry,
    args_plan: &RpcPlan,
    response_plan: &'static RpcPlan,
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

    // Use MaybeUninit to avoid heap allocation for args.
    // Deserialization happens via non-generic prepare_sync.
    let mut args_slot = std::mem::MaybeUninit::<A>::uninit();

    // SAFETY: args_slot is properly aligned and sized for A.
    // prepare_sync will initialize it on success.
    let prepare_result = unsafe {
        prepare_sync(
            args_slot.as_mut_ptr().cast(),
            args_plan,
            &payload,
            &cx.channels,
            registry,
        )
    };

    let task_tx = registry.driver_tx();

    // Handle prepare errors - this is non-generic
    if let Err(e) = prepare_result {
        return Box::pin(async move {
            send_prepare_error(e, &task_tx, conn_id, request_id).await;
        });
    }

    // SAFETY: prepare_sync succeeded, so args_slot is initialized
    let args = unsafe { args_slot.assume_init() };

    let dispatch_ctx = registry.dispatch_context();

    // Use task_local scope so roam::channel() creates bound channels.
    let propagated_metadata = cx.metadata.clone();
    Box::pin(DISPATCH_CONTEXT.scope(dispatch_ctx, async move {
        CURRENT_CALL_METADATA
            .scope(propagated_metadata, async move {
                let result = handler(args).await;

                // Use non-generic send_ok_response via SendPeek
                // SAFETY: R is Send (from where clause), result outlives this scope,
                // and we don't mutate it while the Peek exists
                let peek = facet::Peek::new(&result);
                let send_peek = unsafe { SendPeek::new(peek) };
                send_ok_response(send_peek, response_plan, &task_tx, conn_id, request_id).await;
            })
            .await
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
// Non-Generic Dispatch Infrastructure (roam-next)
// ============================================================================

/// Error during the prepare phase of dispatch.
#[derive(Debug)]
pub enum PrepareError {
    /// Failed to deserialize the request payload.
    Deserialize(String),
    /// Request has wrong number of channel IDs for the method's Tx/Rx arguments.
    ChannelCountMismatch { expected: usize, got: usize },
    /// Middleware rejected the request.
    Rejected(Rejection),
    /// Failed to serialize the response.
    SerializeFailed,
}

impl std::fmt::Display for PrepareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PrepareError::Deserialize(msg) => write!(f, "deserialization error: {}", msg),
            PrepareError::ChannelCountMismatch { expected, got } => {
                write!(
                    f,
                    "channel count mismatch: expected {}, got {}",
                    expected, got
                )
            }
            PrepareError::Rejected(r) => write!(f, "rejected: {}", r.message),
            PrepareError::SerializeFailed => write!(f, "response serialization failed"),
        }
    }
}

impl std::error::Error for PrepareError {}

/// Prepare args synchronously for dispatch (non-generic).
///
/// This function performs all the **synchronous** pre-handler work via reflection:
///
/// 1. Deserializes payload into `args_ptr` using the provided `args_shape`
/// 2. Counts Tx/Rx fields and validates against provided channel count
/// 3. Patches channel IDs from the request into deserialized args
/// 4. Binds Tx/Rx streams via the registry
///
/// After this returns `Ok(())`, the caller can safely read from `args_ptr`,
/// then run middleware and call the handler.
///
/// # Why Sync?
///
/// Channel binding requires `&mut ChannelRegistry`, which cannot be held across
/// await points. This function must complete before the async block starts.
///
/// # Safety
///
/// - `args_ptr` must point to valid, aligned, properly-sized uninitialized memory
///   for the type described by `args_shape`
/// - The args type must be `Send` (enforced by the `#[service]` macro)
/// - On success, caller MUST read from `args_ptr` (to take ownership of the
///   initialized value) - failing to do so will leak memory
///
/// # Example
///
/// ```ignore
/// let mut args_slot = MaybeUninit::<(String, Rx<i32>)>::uninit();
///
/// // SYNC: prepare args
/// unsafe {
///     prepare_sync(
///         args_slot.as_mut_ptr().cast(),
///         <(String, Rx<i32>)>::SHAPE,
///         &payload,
///         &channels,
///         registry,
///     )?;
/// }
/// let args = unsafe { args_slot.assume_init_read() };
///
/// // ASYNC: middleware + handler
/// Box::pin(async move {
///     run_middleware(SendPeek::from(&args), &mut ctx, &middleware).await?;
///     handler.method(&ctx, args).await
/// })
/// ```
#[allow(unsafe_code)]
pub unsafe fn prepare_sync(
    args_ptr: *mut (),
    plan: &RpcPlan,
    payload: &[u8],
    channels: &[u64],
    registry: &mut ChannelRegistry,
) -> Result<(), PrepareError> {
    // 1. Deserialize into args_ptr using the precomputed type plan
    // SAFETY: caller guarantees args_ptr is valid and properly sized
    unsafe { deserialize_into(args_ptr, &plan.type_plan, payload) }?;

    // 2. Validate channel count against precomputed plan.
    // The number of channel locations that are reachable (not behind None) must match.
    let shape = plan.type_plan.root().shape;
    let expected_channels = {
        let peek =
            unsafe { facet::Peek::unchecked_new(PtrConst::new(args_ptr.cast::<u8>()), shape) };
        collect_channel_ids_with_plan(peek, plan).len()
    };
    if channels.len() != expected_channels {
        return Err(PrepareError::ChannelCountMismatch {
            expected: expected_channels,
            got: channels.len(),
        });
    }

    // 3. Patch channel IDs from Request framing into deserialized args
    trace!(channels = ?channels, "prepare_sync: patching channel IDs");
    // SAFETY: args_ptr is valid and initialized, channel count validated
    unsafe {
        patch_channel_ids_with_plan(args_ptr, plan, channels);
    }

    // 4. Bind channels via reflection
    trace!("prepare_sync: binding channels");
    // SAFETY: args_ptr is valid and initialized
    unsafe {
        registry.bind_channels_with_plan(args_ptr, plan);
    }

    Ok(())
}

/// Deserialize payload into a type-erased pointer using a precomputed TypePlanCore.
///
/// This is the non-generic deserialization function used by generated dispatchers.
/// It deserializes directly into caller-provided memory (typically stack-allocated
/// via `MaybeUninit`) to avoid heap allocation.
///
/// # Safety
///
/// - `ptr` must point to valid, properly aligned memory for the type described by `shape`
/// - The memory must have at least `shape.layout.size()` bytes available
/// - `type_plan` must have been built from `shape`
/// - On success, the memory at `ptr` will be initialized with the deserialized value
/// - On error, the memory at `ptr` may be partially initialized and MUST NOT be read
#[allow(unsafe_code)]
pub unsafe fn deserialize_into(
    ptr: *mut (),
    type_plan: &Arc<facet_reflect::TypePlanCore>,
    payload: &[u8],
) -> Result<(), PrepareError> {
    // Create a Partial that writes directly into caller-provided memory.
    // This avoids heap allocation - the value is constructed in-place.
    let ptr_uninit = PtrUninit::new(ptr.cast::<u8>());

    let root_id = type_plan.root_id();

    // SAFETY: Caller guarantees ptr is valid, aligned, and properly sized
    let partial: Partial<'_, false> =
        unsafe { Partial::from_raw(ptr_uninit, type_plan.clone(), root_id) }
            .map_err(|e| PrepareError::Deserialize(e.to_string()))?;

    // Use facet-format's FormatDeserializer with PostcardParser to deserialize.
    // This is non-generic - it uses the Shape for all type information.
    let mut parser = PostcardParser::new(payload);
    let mut deserializer: FormatDeserializer<'_, '_, false> =
        FormatDeserializer::new_owned(&mut parser);
    let partial = deserializer
        .deserialize_into(partial, MetaSource::FromEvents)
        .map_err(|e| PrepareError::Deserialize(e.to_string()))?;

    // Validate the value is fully initialized and leave it in place.
    // After this succeeds, the caller can safely read from ptr.
    partial
        .finish_in_place()
        .map_err(|e| PrepareError::Deserialize(e.to_string()))?;

    Ok(())
}

/// Patch channel IDs using a precomputed RpcPlan.
///
/// # Safety
///
/// - `args_ptr` must point to valid, initialized memory matching the plan's shape
#[allow(unsafe_code)]
pub unsafe fn patch_channel_ids_with_plan(args_ptr: *mut (), plan: &RpcPlan, channels: &[u64]) {
    let shape = plan.type_plan.root().shape;
    trace!(channels = ?channels, "patch_channel_ids_with_plan: patching channels");
    let mut idx = 0;
    for loc in &plan.channel_locations {
        // SAFETY: args_ptr is valid and initialized
        let poke =
            unsafe { facet::Poke::from_raw_parts(PtrMut::new(args_ptr.cast::<u8>()), shape) };
        match poke.at_path_mut(&loc.path) {
            Ok(channel_poke) => {
                if let Ok(mut ps) = channel_poke.into_struct()
                    && let Ok(mut channel_id_field) = ps.field_by_name("channel_id")
                    && let Ok(channel_id_ref) = channel_id_field.get_mut::<ChannelId>()
                    && idx < channels.len()
                {
                    *channel_id_ref = channels[idx];
                    idx += 1;
                }
            }
            Err(PathAccessError::OptionIsNone { .. }) => {
                // Option<Rx/Tx> is None — skip this channel location
            }
            Err(_e) => {
                warn!("patch_channel_ids_with_plan: unexpected path error: {_e}");
            }
        }
    }
}

// ============================================================================
// Non-Generic Response Helpers
// ============================================================================

/// Serialize and send an OK response using non-generic operations.
///
/// This function handles the response serialization and sending via reflection,
/// avoiding monomorphization:
///
/// 1. Collects channel IDs from the result (for `Rx<T>` in return types)
/// 2. Serializes the result using Shape reflection
/// 3. Sends the Response message via the driver channel
///
/// If serialization fails, sends an Internal error response instead.
///
/// Takes `SendPeek` instead of a raw pointer because `SendPeek` is Send,
/// allowing this async function's Future to be Send.
pub async fn send_ok_response(
    result: SendPeek<'_>,
    response_plan: &RpcPlan,
    driver_tx: &Sender<DriverMessage>,
    conn_id: roam_wire::ConnectionId,
    request_id: u64,
) {
    let peek = result.peek();

    // Collect channel IDs from the result using precomputed paths
    let response_channels = collect_channel_ids_with_plan(peek, response_plan);

    // Result::Ok(0) + serialized value
    let mut payload = vec![0u8];
    match facet_postcard::peek_to_vec(peek) {
        Ok(bytes) => payload.extend(bytes),
        Err(_) => {
            // Serialization failed - send Internal error
            send_prepare_error(
                PrepareError::SerializeFailed,
                driver_tx,
                conn_id,
                request_id,
            )
            .await;
            return;
        }
    }

    // Send Response with channel IDs
    let _ = driver_tx
        .send(DriverMessage::Response {
            conn_id,
            request_id,
            channels: response_channels,
            payload,
        })
        .await;
}

/// Serialize and send a user error response using non-generic operations.
///
/// This function handles error serialization and sending via reflection:
///
/// 1. Serializes the user error using Shape reflection
/// 2. Sends the Response message with error encoding
///
/// If serialization fails, sends an Internal error response instead.
///
/// Takes `SendPeek` instead of a raw pointer because `SendPeek` is Send,
/// allowing this async function's Future to be Send.
pub async fn send_error_response(
    error: SendPeek<'_>,
    driver_tx: &Sender<DriverMessage>,
    conn_id: roam_wire::ConnectionId,
    request_id: u64,
) {
    let peek = error.peek();

    // Result::Err(1) + RoamError::User(0) + serialized user error
    let mut payload = vec![1u8, 0u8];
    match facet_postcard::peek_to_vec(peek) {
        Ok(bytes) => payload.extend(bytes),
        Err(_) => {
            // Serialization failed - send Internal error
            send_prepare_error(
                PrepareError::SerializeFailed,
                driver_tx,
                conn_id,
                request_id,
            )
            .await;
            return;
        }
    }

    // Send Response (no channels for error responses)
    let _ = driver_tx
        .send(DriverMessage::Response {
            conn_id,
            request_id,
            channels: Vec::new(),
            payload,
        })
        .await;
}

/// Run pre-middleware on args via SendPeek.
///
/// This is called from the async block in generated dispatchers, after channel
/// binding has completed synchronously. The caller creates a `SendPeek` from
/// the owned args and passes it here.
///
/// Taking `SendPeek` instead of a raw pointer is critical: `SendPeek` is Send,
/// so capturing it in an async Future is safe. Raw pointers are not Send, so
/// passing them to an async function would make the Future not Send.
///
/// Pre-middleware runs first-to-last. If any middleware rejects, we return
/// early with the rejection (the caller should still call `run_post_middleware`
/// so middleware can clean up).
pub async fn run_pre_middleware(
    send_peek: SendPeek<'_>,
    ctx: &mut Context,
    middleware: &[Arc<dyn Middleware>],
) -> Result<(), Rejection> {
    for mw in middleware {
        mw.pre(ctx, send_peek).await?;
    }

    Ok(())
}

/// Run post-middleware after the handler completes.
///
/// Post-middleware runs last-to-first (reverse order), mirroring standard
/// "wrap" semantics where the first middleware added is the outermost wrapper.
///
/// This is called after the handler returns (or after a rejection). Middleware
/// can observe the outcome and clean up resources (e.g., end tracing spans).
pub async fn run_post_middleware(
    ctx: &Context,
    outcome: crate::MethodOutcome<'_>,
    middleware: &[Arc<dyn Middleware>],
) {
    // Post runs last-to-first
    for mw in middleware.iter().rev() {
        mw.post(ctx, outcome.clone()).await;
    }
}

/// Send a prepare error (deserialization, channel mismatch, rejection, etc.) as a response.
///
/// Maps each error type to the appropriate RoamError variant.
pub async fn send_prepare_error(
    error: PrepareError,
    driver_tx: &Sender<DriverMessage>,
    conn_id: roam_wire::ConnectionId,
    request_id: u64,
) {
    let payload = match error {
        PrepareError::Deserialize(_) => {
            // Result::Err(1) + RoamError::InvalidPayload(2)
            vec![1, 2]
        }
        PrepareError::ChannelCountMismatch { .. } => {
            // Channel count mismatch is a protocol error - treat as InvalidPayload
            // Result::Err(1) + RoamError::InvalidPayload(2)
            vec![1, 2]
        }
        PrepareError::Rejected(_) => {
            // Middleware rejection - map to Internal for now
            // Result::Err(1) + RoamError::Internal(3)
            vec![1, 3]
        }
        PrepareError::SerializeFailed => {
            // Serialization failure is an internal error
            // Result::Err(1) + RoamError::Internal(3)
            vec![1, 3]
        }
    };

    let _ = driver_tx
        .send(DriverMessage::Response {
            conn_id,
            request_id,
            channels: Vec::new(),
            payload,
        })
        .await;
}

/// Collect channel IDs from a Peek value using a precomputed RpcPlan.
pub fn collect_channel_ids_with_plan(peek: facet::Peek<'_, '_>, plan: &RpcPlan) -> Vec<u64> {
    let mut ids = Vec::new();
    for loc in &plan.channel_locations {
        match peek.at_path(&loc.path) {
            Ok(channel_peek) => {
                if let Ok(ps) = channel_peek.into_struct()
                    && let Ok(channel_id_field) = ps.field_by_name("channel_id")
                    && let Ok(&channel_id) = channel_id_field.get::<ChannelId>()
                {
                    ids.push(channel_id);
                }
            }
            Err(PathAccessError::OptionIsNone { .. }) => {
                // Option<Rx/Tx> is None — skip this channel location
            }
            Err(_e) => {
                warn!("collect_channel_ids_with_plan: unexpected path error: {_e}");
            }
        }
    }
    ids
}

// ============================================================================
// Channel ID Collection and Patching
// ============================================================================

/// Collect channel IDs from args by walking with Peek.
///
/// Returns channel IDs in declaration order (depth-first traversal).
/// Used by the client to populate the `channels` vec in Request messages.
///
/// The `plan` should be created once per type as a static in non-generic code.
///
/// r[impl call.request.channels] - Collects channel IDs in declaration order for the Request.
pub fn collect_channel_ids<T: Facet<'static>>(args: &T, plan: &RpcPlan) -> Vec<u64> {
    let peek = facet::Peek::new(args);
    collect_channel_ids_with_plan(peek, plan)
}

/// Patch channel IDs into deserialized args.
///
/// Overwrites channel_id fields in Rx/Tx in declaration order.
/// Used by the server to apply the authoritative `channels` vec from Request.
///
/// The `plan` should be created once per type as a static in non-generic code.
#[allow(unsafe_code)]
pub fn patch_channel_ids<T: Facet<'static>>(args: &mut T, plan: &RpcPlan, channels: &[u64]) {
    trace!(channels = ?channels, "patch_channel_ids: patching channels");
    let args_ptr = args as *mut T as *mut ();
    unsafe { patch_channel_ids_with_plan(args_ptr, plan, channels) };
}

// ============================================================================
// Service Dispatcher Trait
// ============================================================================

/// Trait for dispatching requests to a service.
///
/// The dispatcher handles both simple and channeling methods uniformly.
/// Channel binding is done via reflection (Poke) on the deserialized args.
pub trait ServiceDispatcher: Send + Sync {
    /// Returns the static service descriptor for this dispatcher.
    fn service_descriptor(&self) -> &'static ServiceDescriptor;

    /// Look up a method descriptor by ID.
    ///
    /// Default implementation searches `service_descriptor().methods`.
    /// [`RoutedDispatcher`] overrides this to check primary then fallback.
    fn method_descriptor(&self, method_id: u64) -> Option<&'static MethodDescriptor> {
        self.service_descriptor().by_id(method_id)
    }

    /// Dispatch a request and send the response via the task channel.
    ///
    /// The dispatcher is responsible for:
    /// - Looking up the method by `cx.method_id()`
    /// - Deserializing arguments from payload
    /// - Patching channel IDs from `cx.channels()` into deserialized args via `patch_channel_ids()`
    /// - Binding any Tx/Rx channels via the registry
    /// - Calling the service method
    /// - Sending Data/Close messages for any Tx channels
    /// - Sending the Response message via DriverMessage::Response
    ///
    /// By using a single channel for Data/Close/Response, correct ordering is guaranteed:
    /// all channel Data and Close messages are sent before the Response.
    ///
    /// The `cx.channels()` contains channel IDs from the Request message framing,
    /// in declaration order. For a ForwardingDispatcher, this enables transparent proxying
    /// without parsing the payload.
    ///
    /// Returns a boxed future with `'static` lifetime so it can be spawned.
    /// Implementations should clone their service into the future to achieve this.
    ///
    /// r[impl channeling.allocation.caller] - Channel IDs are from Request.channels (caller allocated).
    fn dispatch(
        &self,
        cx: Context,
        payload: Vec<u8>,
        registry: &mut ChannelRegistry,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>;
}

/// Static descriptor for a single RPC method.
///
/// Contains all metadata and precomputed plans needed for dispatching
/// and calling this method, eliminating the need for per-method OnceLock statics.
pub struct MethodDescriptor {
    /// Method ID (hash of service name, method name, arg shapes, return shape).
    pub id: u64,
    /// Service name (e.g., "Calculator").
    pub service_name: &'static str,
    /// Method name (e.g., "add").
    pub method_name: &'static str,
    /// Argument names in declaration order.
    pub arg_names: &'static [&'static str],
    /// Argument shapes in declaration order.
    pub arg_shapes: &'static [&'static Shape],
    /// Return type shape.
    pub return_shape: &'static Shape,
    /// Precomputed plan for the args tuple type.
    pub args_plan: &'static RpcPlan,
    /// Precomputed plan for the Ok/return type.
    pub ok_plan: &'static RpcPlan,
    /// Precomputed plan for the Err type (Infallible if infallible).
    pub err_plan: &'static RpcPlan,
}

impl std::fmt::Debug for MethodDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MethodDescriptor")
            .field("id", &self.id)
            .field("service_name", &self.service_name)
            .field("method_name", &self.method_name)
            .finish()
    }
}

/// Static descriptor for a roam RPC service.
///
/// Contains the service name and all method descriptors. Built once per service
/// via OnceLock in macro-generated code.
pub struct ServiceDescriptor {
    /// Service name (e.g., "Calculator").
    pub service_name: &'static str,
    /// All methods in this service.
    pub methods: &'static [&'static MethodDescriptor],
}

impl ServiceDescriptor {
    /// Look up a method descriptor by method ID.
    pub fn by_id(&self, method_id: u64) -> Option<&'static MethodDescriptor> {
        self.methods.iter().find(|m| m.id == method_id).copied()
    }
}

/// An empty service descriptor for dispatchers that don't serve any methods.
pub static EMPTY_DESCRIPTOR: ServiceDescriptor = ServiceDescriptor {
    service_name: "",
    methods: &[],
};

// ============================================================================
// Routed Dispatcher
// ============================================================================

/// A dispatcher that routes to one of two dispatchers based on method ID.
///
/// Methods handled by `primary` (via [`ServiceDispatcher::service_descriptor`]) are
/// routed to it; all other methods are routed to `fallback`.
#[derive(Clone)]
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
    /// Methods declared by `primary.service_descriptor()` are routed to `primary`,
    /// all others to `fallback`.
    pub fn new(primary: A, fallback: B) -> Self {
        let primary_methods = primary
            .service_descriptor()
            .methods
            .iter()
            .map(|m| m.id)
            .collect();
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
    fn service_descriptor(&self) -> &'static ServiceDescriptor {
        self.primary.service_descriptor()
    }

    fn method_descriptor(&self, method_id: u64) -> Option<&'static MethodDescriptor> {
        self.primary
            .method_descriptor(method_id)
            .or_else(|| self.fallback.method_descriptor(method_id))
    }

    fn dispatch(
        &self,
        cx: Context,
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
