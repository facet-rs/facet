use std::marker::PhantomData;

use facet::Facet;

use crate::{CallError, ConnectionHandle, DecodeError, ResponseData, RoamError, TransportError};

/// Trait for making RPC calls.
///
/// This abstracts over different connection types (e.g., `ConnectionHandle`,
/// `ReconnectingClient`) so generated clients can work with any of them.
///
/// All callers return `TransportError` for transport-level failures.
/// Generated clients convert this to `CallError<E>` which also includes
/// response-level errors like `RoamError::User(E)`.
#[allow(async_fn_in_trait)]
pub trait Caller: Clone + Send + Sync + 'static {
    /// Make an RPC call with the given method ID and arguments.
    ///
    /// The arguments are mutable because stream bindings (Tx/Rx) need to be
    /// assigned channel IDs before serialization.
    ///
    /// Returns ResponseData containing the payload and any response channel IDs.
    #[cfg(not(target_arch = "wasm32"))]
    fn call<T: Facet<'static> + Send>(
        &self,
        method_id: u64,
        args: &mut T,
    ) -> impl std::future::Future<Output = Result<ResponseData, TransportError>> + Send {
        self.call_with_metadata(method_id, args, roam_wire::Metadata::default())
    }

    /// Make an RPC call with the given method ID and arguments.
    ///
    /// The arguments are mutable because stream bindings (Tx/Rx) need to be
    /// assigned channel IDs before serialization.
    ///
    /// Returns ResponseData containing the payload and any response channel IDs.
    #[cfg(target_arch = "wasm32")]
    fn call<T: Facet<'static> + Send>(
        &self,
        method_id: u64,
        args: &mut T,
    ) -> impl std::future::Future<Output = Result<ResponseData, TransportError>> {
        self.call_with_metadata(method_id, args, roam_wire::Metadata::default())
    }

    /// Make an RPC call with the given method ID, arguments, and metadata.
    ///
    /// The arguments are mutable because stream bindings (Tx/Rx) need to be
    /// assigned channel IDs before serialization.
    ///
    /// Returns ResponseData containing the payload and any response channel IDs.
    #[cfg(not(target_arch = "wasm32"))]
    fn call_with_metadata<T: Facet<'static> + Send>(
        &self,
        method_id: u64,
        args: &mut T,
        metadata: roam_wire::Metadata,
    ) -> impl std::future::Future<Output = Result<ResponseData, TransportError>> + Send;

    /// Make an RPC call with the given method ID, arguments, and metadata.
    ///
    /// The arguments are mutable because stream bindings (Tx/Rx) need to be
    /// assigned channel IDs before serialization.
    ///
    /// Returns ResponseData containing the payload and any response channel IDs.
    #[cfg(target_arch = "wasm32")]
    fn call_with_metadata<T: Facet<'static> + Send>(
        &self,
        method_id: u64,
        args: &mut T,
        metadata: roam_wire::Metadata,
    ) -> impl std::future::Future<Output = Result<ResponseData, TransportError>>;

    /// Bind receivers for `Rx<T>` streams in the response.
    ///
    /// After deserializing a response, any `Rx<T>` values in it are "hollow" -
    /// they have channel IDs but no actual receiver. This method walks the
    /// response and binds receivers for each Rx using the channel IDs from
    /// the Response message.
    fn bind_response_streams<T: Facet<'static>>(&self, response: &mut T, channels: &[u64]);
}

impl Caller for ConnectionHandle {
    async fn call_with_metadata<T: Facet<'static> + Send>(
        &self,
        method_id: u64,
        args: &mut T,
        metadata: roam_wire::Metadata,
    ) -> Result<ResponseData, TransportError> {
        ConnectionHandle::call_with_metadata(self, method_id, args, metadata).await
    }

    fn bind_response_streams<T: Facet<'static>>(&self, response: &mut T, channels: &[u64]) {
        ConnectionHandle::bind_response_streams(self, response, channels)
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
/// // With metadata
/// let result = client.subscribe(route)
///     .with_metadata(vec![("trace-id".into(), MetadataValue::String("abc".into()))])
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
    _phantom: PhantomData<fn() -> (Ok, Err)>,
}

impl<C, Args, Ok, Err> CallFuture<C, Args, Ok, Err>
where
    C: Caller,
    Args: Facet<'static>,
{
    /// Create a new CallFuture.
    pub fn new(caller: C, method_id: u64, args: Args) -> Self {
        Self {
            caller,
            method_id,
            args,
            metadata: roam_wire::Metadata::default(),
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

    fn into_future(self) -> Self::IntoFuture {
        let CallFuture {
            caller,
            method_id,
            mut args,
            metadata,
            _phantom,
        } = self;

        Box::pin(async move {
            let response = caller
                .call_with_metadata(method_id, &mut args, metadata)
                .await
                .map_err(CallError::from)?;
            let mut result = decode_response::<Ok, Err>(&response.payload)?;
            caller.bind_response_streams(&mut result, &response.channels);
            Ok(result)
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

    fn into_future(self) -> Self::IntoFuture {
        let CallFuture {
            caller,
            method_id,
            mut args,
            metadata,
            _phantom,
        } = self;

        Box::pin(async move {
            let response = caller
                .call_with_metadata(method_id, &mut args, metadata)
                .await
                .map_err(CallError::from)?;
            let mut result = decode_response::<Ok, Err>(&response.payload)?;
            caller.bind_response_streams(&mut result, &response.channels);
            Ok(result)
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
