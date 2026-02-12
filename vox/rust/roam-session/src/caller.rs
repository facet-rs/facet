use std::marker::PhantomData;
use std::sync::Arc;

use facet::Facet;
use facet_core::PtrUninit;

use crate::{
    CallError, ConnectionHandle, DecodeError, ResponseData, RoamError, RpcPlan, TransportError,
};

/// A raw pointer wrapper that is `Send` and `Sync`.
///
/// This is used to pass pointers through async boundaries in `call_with_metadata_by_shape`.
/// The caller must ensure that the underlying data is actually `Send`.
///
/// # Safety
///
/// The caller must ensure that:
/// - The underlying data is `Send`
/// - The pointer remains valid for the entire duration it's used
#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct SendPtr(*mut ());

// SAFETY: The caller of `call_with_metadata_by_shape` ensures the data is `Send`.
// The trait bounds on `CallFuture::into_future` enforce `Args: Send`.
#[allow(unsafe_code)]
unsafe impl Send for SendPtr {}
#[allow(unsafe_code)]
unsafe impl Sync for SendPtr {}

impl SendPtr {
    /// Create a new SendPtr from a raw pointer.
    ///
    /// # Safety
    ///
    /// The underlying data must be `Send` and the pointer must be valid.
    #[allow(unsafe_code)]
    pub unsafe fn new(ptr: *mut ()) -> Self {
        Self(ptr)
    }

    /// Get the raw pointer.
    pub fn as_ptr(self) -> *mut () {
        self.0
    }
}

/// Trait for making RPC calls.
///
/// This abstracts over different connection types (e.g., `ConnectionHandle`,
/// `ReconnectingClient`) so generated clients can work with any of them.
///
/// All callers return `TransportError` for transport-level failures.
/// Generated clients convert this to `CallError<E>` which also includes
/// response-level errors like `RoamError::User(E)`.
#[allow(async_fn_in_trait)]
#[allow(unsafe_code)]
pub trait Caller: Clone + Send + Sync + 'static {
    /// Make an RPC call with the given method ID and arguments.
    ///
    /// The arguments are mutable because channel bindings (Tx/Rx) need to be
    /// assigned channel IDs before serialization.
    ///
    /// Returns ResponseData containing the payload and any response channel IDs.
    ///
    /// The `args_plan` should be created once per type as a static in non-generic code.
    #[cfg(not(target_arch = "wasm32"))]
    fn call<T: Facet<'static> + Send>(
        &self,
        method_id: u64,
        args: &mut T,
        args_plan: &RpcPlan,
    ) -> impl std::future::Future<Output = Result<ResponseData, TransportError>> + Send {
        self.call_with_metadata(method_id, args, args_plan, roam_wire::Metadata::default())
    }

    /// Make an RPC call with the given method ID and arguments.
    ///
    /// The arguments are mutable because channel bindings (Tx/Rx) need to be
    /// assigned channel IDs before serialization.
    ///
    /// Returns ResponseData containing the payload and any response channel IDs.
    ///
    /// The `args_plan` should be created once per type as a static in non-generic code.
    #[cfg(target_arch = "wasm32")]
    fn call<T: Facet<'static> + Send>(
        &self,
        method_id: u64,
        args: &mut T,
        args_plan: &RpcPlan,
    ) -> impl std::future::Future<Output = Result<ResponseData, TransportError>> {
        self.call_with_metadata(method_id, args, args_plan, roam_wire::Metadata::default())
    }

    /// Make an RPC call with the given method ID, arguments, and metadata.
    ///
    /// The arguments are mutable because channel bindings (Tx/Rx) need to be
    /// assigned channel IDs before serialization.
    ///
    /// Returns ResponseData containing the payload and any response channel IDs.
    ///
    /// The `args_plan` should be created once per type as a static in non-generic code.
    #[cfg(not(target_arch = "wasm32"))]
    fn call_with_metadata<T: Facet<'static> + Send>(
        &self,
        method_id: u64,
        args: &mut T,
        args_plan: &RpcPlan,
        metadata: roam_wire::Metadata,
    ) -> impl std::future::Future<Output = Result<ResponseData, TransportError>> + Send;

    /// Make an RPC call with the given method ID, arguments, and metadata.
    ///
    /// The arguments are mutable because channel bindings (Tx/Rx) need to be
    /// assigned channel IDs before serialization.
    ///
    /// Returns ResponseData containing the payload and any response channel IDs.
    ///
    /// The `args_plan` should be created once per type as a static in non-generic code.
    #[cfg(target_arch = "wasm32")]
    fn call_with_metadata<T: Facet<'static> + Send>(
        &self,
        method_id: u64,
        args: &mut T,
        args_plan: &RpcPlan,
        metadata: roam_wire::Metadata,
    ) -> impl std::future::Future<Output = Result<ResponseData, TransportError>>;

    /// Bind receivers for `Rx<T>` channels in the response.
    ///
    /// After deserializing a response, any `Rx<T>` values in it are "hollow" -
    /// they have channel IDs but no actual receiver. This method walks the
    /// response and binds receivers for each Rx using the channel IDs from
    /// the Response message.
    ///
    /// The `plan` should be created once per type as a static in non-generic code.
    fn bind_response_channels<T: Facet<'static>>(
        &self,
        response: &mut T,
        plan: &RpcPlan,
        channels: &[u64],
    );

    // ========================================================================
    // Non-generic methods (reduce monomorphization)
    // ========================================================================

    /// Make an RPC call using reflection (non-generic).
    ///
    /// This is the non-generic core implementation that avoids monomorphization.
    /// The generic `call_with_metadata` can delegate to this.
    ///
    /// # Safety
    ///
    /// - `args_ptr` must have been created from a valid, initialized pointer matching the plan's shape
    /// - The underlying args type must be `Send`
    #[doc(hidden)]
    #[cfg(not(target_arch = "wasm32"))]
    fn call_with_metadata_by_plan(
        &self,
        method_id: u64,
        args_ptr: SendPtr,
        args_plan: &'static Arc<RpcPlan>,
        metadata: roam_wire::Metadata,
    ) -> impl std::future::Future<Output = Result<ResponseData, TransportError>> + Send;

    /// Make an RPC call using reflection (non-generic).
    ///
    /// This is the non-generic core implementation that avoids monomorphization.
    /// The generic `call_with_metadata` can delegate to this.
    ///
    /// # Safety
    ///
    /// - `args_ptr` must have been created from a valid, initialized pointer matching the plan's shape
    /// - The underlying args type must be `Send`
    #[doc(hidden)]
    #[cfg(target_arch = "wasm32")]
    fn call_with_metadata_by_plan(
        &self,
        method_id: u64,
        args_ptr: SendPtr,
        args_plan: &'static Arc<RpcPlan>,
        metadata: roam_wire::Metadata,
    ) -> impl std::future::Future<Output = Result<ResponseData, TransportError>>;

    /// Bind receivers for `Rx<T>` channels in the response using reflection (non-generic).
    ///
    /// # Safety
    ///
    /// - `response_ptr` must point to valid, initialized memory matching the plan's shape
    #[doc(hidden)]
    unsafe fn bind_response_channels_by_plan(
        &self,
        response_ptr: *mut (),
        response_plan: &RpcPlan,
        channels: &[u64],
    );
}

impl Caller for ConnectionHandle {
    async fn call_with_metadata<T: Facet<'static> + Send>(
        &self,
        method_id: u64,
        args: &mut T,
        args_plan: &RpcPlan,
        metadata: roam_wire::Metadata,
    ) -> Result<ResponseData, TransportError> {
        ConnectionHandle::call_with_metadata(self, method_id, args, args_plan, metadata).await
    }

    fn bind_response_channels<T: Facet<'static>>(
        &self,
        response: &mut T,
        plan: &RpcPlan,
        channels: &[u64],
    ) {
        ConnectionHandle::bind_response_channels(self, response, plan, channels)
    }

    #[allow(unsafe_code)]
    #[cfg(not(target_arch = "wasm32"))]
    fn call_with_metadata_by_plan(
        &self,
        method_id: u64,
        args_ptr: SendPtr,
        args_plan: &'static Arc<RpcPlan>,
        metadata: roam_wire::Metadata,
    ) -> impl std::future::Future<Output = Result<ResponseData, TransportError>> + Send {
        // SAFETY: Caller guarantees args_ptr is valid, initialized, and Send
        unsafe {
            ConnectionHandle::call_with_metadata_by_plan(
                self,
                method_id,
                args_ptr.as_ptr(),
                args_plan,
                metadata,
            )
        }
    }

    #[allow(unsafe_code)]
    #[cfg(target_arch = "wasm32")]
    fn call_with_metadata_by_plan(
        &self,
        method_id: u64,
        args_ptr: SendPtr,
        args_plan: &'static Arc<RpcPlan>,
        metadata: roam_wire::Metadata,
    ) -> impl std::future::Future<Output = Result<ResponseData, TransportError>> {
        // SAFETY: Caller guarantees args_ptr is valid, initialized, and Send
        unsafe {
            ConnectionHandle::call_with_metadata_by_plan(
                self,
                method_id,
                args_ptr.as_ptr(),
                args_plan,
                metadata,
            )
        }
    }

    #[allow(unsafe_code)]
    unsafe fn bind_response_channels_by_plan(
        &self,
        response_ptr: *mut (),
        response_plan: &RpcPlan,
        channels: &[u64],
    ) {
        // SAFETY: Caller guarantees response_ptr is valid and initialized
        unsafe {
            ConnectionHandle::bind_response_channels_by_plan(
                self,
                response_ptr,
                response_plan,
                channels,
            )
        }
    }
}

// ============================================================================
// CallFuture - Builder pattern for RPC calls with optional metadata
// ============================================================================

/// A future representing an RPC call that can be configured with metadata.
///
/// This provides a builder pattern for RPC calls:
/// - `client.method(args).await` - Simple call with default (empty) metadata
/// - `client.method(args).with_metadata(meta).await` - Call with custom metadata
///
/// The future is lazy - the RPC call is not made until `.await` is called.
///
/// # Example
///
/// ```ignore
/// // Simple call
/// let result = client.subscribe(route).await?;
///
/// // With metadata (key, value, flags)
/// let result = client.subscribe(route)
///     .with_metadata(vec![("trace-id".into(), MetadataValue::String("abc".into()), 0)])
///     .await?;
/// ```
pub struct CallFuture<C, Args, Ok, Err>
where
    C: Caller,
    Args: Facet<'static>,
{
    caller: C,
    method_id: u64,
    args: Args,
    metadata: roam_wire::Metadata,
    /// Precomputed plan for the Args type.
    args_plan: &'static Arc<RpcPlan>,
    /// Precomputed plan for the Ok (response) type.
    ok_plan: &'static Arc<RpcPlan>,
    /// Precomputed plan for the Err type.
    err_plan: &'static Arc<RpcPlan>,
    _phantom: PhantomData<fn() -> (Ok, Err)>,
}

impl<C, Args, Ok, Err> CallFuture<C, Args, Ok, Err>
where
    C: Caller,
    Args: Facet<'static>,
{
    /// Create a new CallFuture with precomputed plans.
    ///
    /// Plans should be obtained from `OnceLock` statics at the call site
    /// (e.g., in macro-generated client methods) to ensure one plan per type.
    pub fn new(
        caller: C,
        method_id: u64,
        args: Args,
        args_plan: &'static Arc<RpcPlan>,
        ok_plan: &'static Arc<RpcPlan>,
        err_plan: &'static Arc<RpcPlan>,
    ) -> Self {
        Self {
            caller,
            method_id,
            args,
            metadata: roam_wire::Metadata::default(),
            args_plan,
            ok_plan,
            err_plan,
            _phantom: PhantomData,
        }
    }

    /// Set metadata for this call.
    ///
    /// Metadata is a list of key-value pairs that will be sent with the request.
    /// The server can access this via `Context::metadata()`.
    pub fn with_metadata(mut self, metadata: roam_wire::Metadata) -> Self {
        self.metadata = metadata;
        self
    }
}

// On native, the future must be Send so it can be spawned on tokio.
// On WASM, futures don't need Send since everything is single-threaded.
#[cfg(not(target_arch = "wasm32"))]
impl<C, Args, Ok, Err> std::future::IntoFuture for CallFuture<C, Args, Ok, Err>
where
    C: Caller,
    Args: Facet<'static> + Send + 'static,
    Ok: Facet<'static> + Send + 'static,
    Err: Facet<'static> + Send + 'static,
{
    type Output = Result<Ok, CallError<Err>>;
    type IntoFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Self::Output> + Send>>;

    #[allow(unsafe_code)]
    fn into_future(self) -> Self::IntoFuture {
        let CallFuture {
            caller,
            method_id,
            mut args,
            metadata,
            args_plan,
            ok_plan,
            err_plan,
            _phantom,
        } = self;

        Box::pin(async move {
            // SAFETY: args is valid, initialized, and Send (enforced by trait bounds).
            // We create the pointer INSIDE the async block after the move to ensure
            // it points to the correct memory location.
            let args_ptr = unsafe { SendPtr::new((&raw mut args).cast::<()>()) };

            let response = caller
                .call_with_metadata_by_plan(method_id, args_ptr, args_plan, metadata)
                .await
                .map_err(CallError::from)?;

            // Use non-generic decode to reduce monomorphization.
            // SAFETY: MaybeUninit is properly aligned and sized for Ok/Err types
            let mut ok_slot = std::mem::MaybeUninit::<Ok>::uninit();
            let mut err_slot = std::mem::MaybeUninit::<Err>::uninit();

            let outcome = unsafe {
                decode_response_into(
                    &response.payload,
                    ok_slot.as_mut_ptr().cast::<()>(),
                    ok_plan,
                    err_slot.as_mut_ptr().cast::<()>(),
                    err_plan,
                )
            };

            match outcome {
                DecodeOutcome::Ok => {
                    // SAFETY: decode_response_into initialized ok_slot
                    let mut result = unsafe { ok_slot.assume_init() };
                    // SAFETY: result is valid and initialized
                    unsafe {
                        caller.bind_response_channels_by_plan(
                            (&raw mut result).cast::<()>(),
                            ok_plan,
                            &response.channels,
                        );
                    }
                    std::result::Result::Ok(result)
                }
                DecodeOutcome::UserError => {
                    // SAFETY: decode_response_into initialized err_slot
                    let user_error = unsafe { err_slot.assume_init() };
                    std::result::Result::Err(CallError::Roam(RoamError::User(user_error)))
                }
                DecodeOutcome::SystemError(e) => {
                    // Map RoamError<()> to RoamError<Err>
                    let mapped = match e {
                        RoamError::User(()) => unreachable!("SystemError never has User variant"),
                        RoamError::UnknownMethod => RoamError::UnknownMethod,
                        RoamError::InvalidPayload => RoamError::InvalidPayload,
                        RoamError::Cancelled => RoamError::Cancelled,
                    };
                    std::result::Result::Err(CallError::Roam(mapped))
                }
                DecodeOutcome::DeserializeFailed(msg) => std::result::Result::Err(
                    CallError::Protocol(DecodeError::DeserializeFailed(msg)),
                ),
            }
        })
    }
}

#[cfg(target_arch = "wasm32")]
impl<C, Args, Ok, Err> std::future::IntoFuture for CallFuture<C, Args, Ok, Err>
where
    C: Caller,
    Args: Facet<'static> + Send + 'static,
    Ok: Facet<'static> + Send + 'static,
    Err: Facet<'static> + Send + 'static,
{
    type Output = Result<Ok, CallError<Err>>;
    type IntoFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Self::Output>>>;

    #[allow(unsafe_code)]
    fn into_future(self) -> Self::IntoFuture {
        let CallFuture {
            caller,
            method_id,
            mut args,
            metadata,
            args_plan,
            ok_plan,
            err_plan,
            _phantom,
        } = self;

        Box::pin(async move {
            // SAFETY: args is valid, initialized, and Send (enforced by trait bounds).
            // We create the pointer INSIDE the async block after the move to ensure
            // it points to the correct memory location.
            let args_ptr = unsafe { SendPtr::new((&raw mut args).cast::<()>()) };

            let response = caller
                .call_with_metadata_by_plan(method_id, args_ptr, args_plan, metadata)
                .await
                .map_err(CallError::from)?;

            // Use non-generic decode to reduce monomorphization.
            // SAFETY: MaybeUninit is properly aligned and sized for Ok/Err types
            let mut ok_slot = std::mem::MaybeUninit::<Ok>::uninit();
            let mut err_slot = std::mem::MaybeUninit::<Err>::uninit();

            let outcome = unsafe {
                decode_response_into(
                    &response.payload,
                    ok_slot.as_mut_ptr().cast::<()>(),
                    ok_plan,
                    err_slot.as_mut_ptr().cast::<()>(),
                    err_plan,
                )
            };

            match outcome {
                DecodeOutcome::Ok => {
                    // SAFETY: decode_response_into initialized ok_slot
                    let mut result = unsafe { ok_slot.assume_init() };
                    // SAFETY: result is valid and initialized
                    unsafe {
                        caller.bind_response_channels_by_plan(
                            (&raw mut result).cast::<()>(),
                            ok_plan,
                            &response.channels,
                        );
                    }
                    std::result::Result::Ok(result)
                }
                DecodeOutcome::UserError => {
                    // SAFETY: decode_response_into initialized err_slot
                    let user_error = unsafe { err_slot.assume_init() };
                    std::result::Result::Err(CallError::Roam(RoamError::User(user_error)))
                }
                DecodeOutcome::SystemError(e) => {
                    // Map RoamError<()> to RoamError<Err>
                    let mapped = match e {
                        RoamError::User(()) => unreachable!("SystemError never has User variant"),
                        RoamError::UnknownMethod => RoamError::UnknownMethod,
                        RoamError::InvalidPayload => RoamError::InvalidPayload,
                        RoamError::Cancelled => RoamError::Cancelled,
                    };
                    std::result::Result::Err(CallError::Roam(mapped))
                }
                DecodeOutcome::DeserializeFailed(msg) => std::result::Result::Err(
                    CallError::Protocol(DecodeError::DeserializeFailed(msg)),
                ),
            }
        })
    }
}

// ============================================================================
// Response Decoding
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

/// Result of non-generic response decoding.
///
/// Used by `decode_response_into` to indicate the outcome without being generic
/// over the user error type.
#[doc(hidden)]
#[derive(Debug)]
pub enum DecodeOutcome {
    /// Ok variant was deserialized successfully into the provided pointer.
    Ok,
    /// User error was deserialized successfully into the provided error pointer.
    UserError,
    /// A system RoamError occurred (no user error to deserialize).
    SystemError(RoamError<()>),
    /// Deserialization failed.
    DeserializeFailed(String),
}

/// Decode a response payload into provided memory using reflection (non-generic).
///
/// This is the non-generic version of `decode_response()`. It deserializes directly
/// into caller-provided memory to avoid monomorphization.
///
/// # Arguments
///
/// * `payload` - The response payload bytes
/// * `ok_ptr` - Pointer to write the Ok value if successful
/// * `ok_plan` - Precomputed RpcPlan for the Ok type
/// * `err_ptr` - Pointer to write the user error if it's a User error
/// * `err_plan` - Precomputed RpcPlan for the Err type
///
/// # Returns
///
/// - `DecodeOutcome::Ok` - The Ok value was written to `ok_ptr`
/// - `DecodeOutcome::UserError` - A user error was written to `err_ptr`
/// - `DecodeOutcome::SystemError` - A system error occurred (UnknownMethod, InvalidPayload, etc.)
/// - `DecodeOutcome::DeserializeFailed` - Deserialization failed
///
/// # Safety
///
/// - `ok_ptr` must point to valid, aligned, properly-sized uninitialized memory for the Ok type
/// - `err_ptr` must point to valid, aligned, properly-sized uninitialized memory for the Err type
/// - On `DecodeOutcome::Ok`, `ok_ptr` is initialized and MUST be read
/// - On `DecodeOutcome::UserError`, `err_ptr` is initialized and MUST be read
/// - On other outcomes, neither pointer is initialized
#[doc(hidden)]
#[allow(unsafe_code)]
pub unsafe fn decode_response_into(
    payload: &[u8],
    ok_ptr: *mut (),
    ok_plan: &RpcPlan,
    err_ptr: *mut (),
    err_plan: &RpcPlan,
) -> DecodeOutcome {
    if payload.is_empty() {
        return DecodeOutcome::DeserializeFailed("empty payload".into());
    }

    match payload[0] {
        0 => {
            // Ok variant: deserialize the value into ok_ptr
            if let Err(e) = unsafe { deserialize_into_ptr(ok_ptr, ok_plan, &payload[1..]) } {
                return DecodeOutcome::DeserializeFailed(e);
            }
            DecodeOutcome::Ok
        }
        1 => {
            // Err variant: determine what kind of error
            if payload.len() < 2 {
                return DecodeOutcome::DeserializeFailed("truncated error".into());
            }
            match payload[1] {
                0 => {
                    // User error: deserialize into err_ptr
                    if let Err(e) =
                        unsafe { deserialize_into_ptr(err_ptr, err_plan, &payload[2..]) }
                    {
                        return DecodeOutcome::DeserializeFailed(e);
                    }
                    DecodeOutcome::UserError
                }
                1 => DecodeOutcome::SystemError(RoamError::UnknownMethod),
                2 => DecodeOutcome::SystemError(RoamError::InvalidPayload),
                3 => DecodeOutcome::SystemError(RoamError::Cancelled),
                d => DecodeOutcome::DeserializeFailed(format!(
                    "unknown RoamError discriminant: {}",
                    d
                )),
            }
        }
        d => DecodeOutcome::DeserializeFailed(format!("invalid result discriminant: {}", d)),
    }
}

/// Deserialize payload into a type-erased pointer using a precomputed RpcPlan.
///
/// # Safety
///
/// - `ptr` must point to valid, properly aligned memory for the type described by the plan's shape
/// - On success, the memory at `ptr` will be initialized
/// - On error, the memory may be partially initialized and MUST NOT be read
#[allow(unsafe_code)]
unsafe fn deserialize_into_ptr(ptr: *mut (), plan: &RpcPlan, payload: &[u8]) -> Result<(), String> {
    use facet_format::{FormatDeserializer, MetaSource};
    use facet_postcard::PostcardParser;
    use facet_reflect::Partial;

    let ptr_uninit = PtrUninit::new(ptr.cast::<u8>());

    let type_plan = &plan.type_plan;
    let root_id = type_plan.root_id();

    // SAFETY: Caller guarantees ptr is valid, aligned, and properly sized
    let partial: Partial<'_, false> =
        unsafe { Partial::from_raw(ptr_uninit, type_plan.clone(), root_id) }
            .map_err(|e| e.to_string())?;

    let mut parser = PostcardParser::new(payload);
    let mut deserializer: FormatDeserializer<'_, '_, false> =
        FormatDeserializer::new_owned(&mut parser);
    let partial = deserializer
        .deserialize_into(partial, MetaSource::FromEvents)
        .map_err(|e| e.to_string())?;

    partial.finish_in_place().map_err(|e| e.to_string())?;

    Ok(())
}
