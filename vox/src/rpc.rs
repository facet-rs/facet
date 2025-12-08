// src/rpc.rs

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{mpsc, oneshot};

use crate::error::{ErrorCode, RapaceError, Result};
use crate::types::MethodId;

// =============================================================================
// Service and Method Traits
// =============================================================================

/// Marker trait for RPC services.
///
/// A service is a collection of related RPC methods that operate on shared state.
/// Services are identified by a unique name for logging and debugging purposes.
pub trait Service: Send + Sync + 'static {
    /// Unique name for this service (e.g., "rapace.example.Calculator").
    const NAME: &'static str;
}

/// Marker trait for RPC methods.
///
/// Each method has a unique ID within its service, request/response types that
/// implement the facet::Facet trait for serialization, and defines the data flow
/// pattern (unary, streaming, etc.).
pub trait Method: Send + Sync + 'static {
    /// Human-readable method name (e.g., "Add", "Subscribe").
    const NAME: &'static str;

    /// Numeric method ID for wire protocol dispatch.
    const ID: u32;

    /// Request payload type.
    type Request: facet::Facet + Send;

    /// Response payload type.
    type Response: facet::Facet + Send;
}

// =============================================================================
// Streaming Method Traits
// =============================================================================

/// Unary RPC: single request -> single response.
///
/// The simplest RPC pattern. Client sends one request, server sends one response.
///
/// Example:
/// ```ignore
/// struct AddMethod;
/// impl Method for AddMethod {
///     const NAME: &'static str = "Add";
///     const ID: u32 = 1;
///     type Request = AddRequest;
///     type Response = AddResponse;
/// }
/// impl UnaryMethod for AddMethod {}
/// ```
pub trait UnaryMethod: Method {}

/// Client streaming RPC: stream of requests -> single response.
///
/// Client sends multiple request messages, server sends one response after
/// receiving all requests. Useful for uploading data or batching operations.
///
/// Example: uploading a file in chunks, batch insert operations.
pub trait ClientStreamingMethod: Method {}

/// Server streaming RPC: single request -> stream of responses.
///
/// Client sends one request, server sends multiple response messages.
/// Useful for subscriptions, real-time updates, or large result sets.
///
/// Example: subscribing to events, streaming query results, file downloads.
pub trait ServerStreamingMethod: Method {}

/// Bidirectional streaming RPC: stream of requests <-> stream of responses.
///
/// Both client and server can send multiple messages in any order.
/// Useful for interactive protocols, real-time communication.
///
/// Example: chat, collaborative editing, streaming data processing.
pub trait BidiStreamingMethod: Method {}

// =============================================================================
// Metadata
// =============================================================================

/// Request/response metadata (headers, tracing context, etc.).
///
/// Metadata carries out-of-band information like authentication tokens,
/// tracing IDs, request priorities, and custom application headers.
#[derive(Debug, Clone, Default)]
pub struct Metadata {
    /// Key-value pairs for metadata entries.
    entries: HashMap<String, Vec<u8>>,
}

impl Metadata {
    /// Create an empty metadata instance.
    pub fn new() -> Self {
        Metadata {
            entries: HashMap::new(),
        }
    }

    /// Insert a metadata entry.
    pub fn insert(&mut self, key: String, value: Vec<u8>) {
        self.entries.insert(key, value);
    }

    /// Get a metadata entry by key.
    pub fn get(&self, key: &str) -> Option<&[u8]> {
        self.entries.get(key).map(|v| v.as_slice())
    }

    /// Get a string value from metadata.
    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(|bytes| std::str::from_utf8(bytes).ok())
    }

    /// Insert a string value into metadata.
    pub fn insert_str(&mut self, key: String, value: String) {
        self.insert(key, value.into_bytes());
    }

    /// Remove a metadata entry.
    pub fn remove(&mut self, key: &str) -> Option<Vec<u8>> {
        self.entries.remove(key)
    }

    /// Check if metadata contains a key.
    pub fn contains_key(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// Iterate over all metadata entries.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v.as_slice()))
    }

    /// Get the number of metadata entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if metadata is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// =============================================================================
// Request and Response Wrappers
// =============================================================================

/// Type-safe request wrapper.
///
/// Wraps the method's request payload along with metadata and an optional deadline.
/// The generic parameter M ensures compile-time type safety - you can't accidentally
/// pass a request to the wrong handler.
pub struct Request<M: Method> {
    /// The actual request payload.
    pub data: M::Request,

    /// Request metadata (headers, tracing context, etc.).
    pub metadata: Metadata,

    /// Optional deadline for this request. If specified, the request should
    /// be cancelled if not completed by this time.
    pub deadline: Option<Instant>,
}

impl<M: Method> Request<M> {
    /// Create a new request with the given data and default metadata.
    pub fn new(data: M::Request) -> Self {
        Request {
            data,
            metadata: Metadata::new(),
            deadline: None,
        }
    }

    /// Create a request with data and metadata.
    pub fn with_metadata(data: M::Request, metadata: Metadata) -> Self {
        Request {
            data,
            metadata,
            deadline: None,
        }
    }

    /// Set the deadline for this request.
    pub fn with_deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// Check if the request deadline has passed.
    pub fn is_expired(&self) -> bool {
        self.deadline.map_or(false, |d| Instant::now() > d)
    }

    /// Get the remaining time until deadline, if any.
    pub fn time_remaining(&self) -> Option<std::time::Duration> {
        self.deadline.and_then(|d| d.checked_duration_since(Instant::now()))
    }

    /// Consume this request and return just the data payload.
    pub fn into_data(self) -> M::Request {
        self.data
    }
}

/// Type-safe response wrapper.
///
/// Wraps the method's response payload along with metadata.
/// The generic parameter M ensures compile-time type safety.
pub struct Response<M: Method> {
    /// The actual response payload.
    pub data: M::Response,

    /// Response metadata (headers, trailers, etc.).
    pub metadata: Metadata,
}

impl<M: Method> Response<M> {
    /// Create a new response with the given data and default metadata.
    pub fn new(data: M::Response) -> Self {
        Response {
            data,
            metadata: Metadata::new(),
        }
    }

    /// Create a response with data and metadata.
    pub fn with_metadata(data: M::Response, metadata: Metadata) -> Self {
        Response {
            data,
            metadata,
        }
    }

    /// Consume this response and return just the data payload.
    pub fn into_data(self) -> M::Response {
        self.data
    }
}

// =============================================================================
// Streaming Types
// =============================================================================

/// A stream of incoming requests for client streaming and bidirectional streaming RPCs.
///
/// This wraps a channel receiver and provides a convenient async iterator interface.
pub struct RequestStream<M: Method> {
    /// Internal channel for receiving request messages.
    rx: mpsc::UnboundedReceiver<Result<M::Request>>,
}

impl<M: Method> RequestStream<M> {
    /// Create a new request stream from a channel receiver.
    pub fn new(rx: mpsc::UnboundedReceiver<Result<M::Request>>) -> Self {
        RequestStream { rx }
    }

    /// Receive the next request from the stream.
    ///
    /// Returns None when the stream is closed, or Some(Err) if an error occurred.
    pub async fn next(&mut self) -> Option<Result<M::Request>> {
        self.rx.recv().await
    }

    /// Try to receive a request without blocking.
    pub fn try_next(&mut self) -> Option<Result<M::Request>> {
        self.rx.try_recv().ok()
    }

    /// Close the stream, rejecting any further messages.
    pub fn close(&mut self) {
        self.rx.close();
    }
}

/// A stream of outgoing responses for server streaming and bidirectional streaming RPCs.
///
/// This wraps a channel sender and provides methods for sending response messages
/// and closing the stream.
pub struct ResponseStream<M: Method> {
    /// Internal channel for sending response messages.
    tx: mpsc::UnboundedSender<Result<M::Response>>,
}

impl<M: Method> ResponseStream<M> {
    /// Create a new response stream from a channel sender.
    pub fn new(tx: mpsc::UnboundedSender<Result<M::Response>>) -> Self {
        ResponseStream { tx }
    }

    /// Send a response message on the stream.
    ///
    /// Returns an error if the stream is closed.
    pub fn send(&self, response: M::Response) -> Result<()> {
        self.tx
            .send(Ok(response))
            .map_err(|_| RapaceError::new(ErrorCode::Aborted, "response stream closed"))
    }

    /// Send an error on the stream and close it.
    pub fn send_error(&self, error: RapaceError) -> Result<()> {
        self.tx
            .send(Err(error))
            .map_err(|_| RapaceError::new(ErrorCode::Aborted, "response stream closed"))
    }

    /// Check if the stream is closed (receiver dropped).
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }
}

impl<M: Method> Clone for ResponseStream<M> {
    fn clone(&self) -> Self {
        ResponseStream {
            tx: self.tx.clone(),
        }
    }
}

// =============================================================================
// Callback Support
// =============================================================================

/// A callback that can be invoked by the server during request processing.
///
/// Callbacks enable nested RPC calls where the server can call back to the client
/// (or another service) as part of handling a request. This is useful for:
/// - Progress notifications
/// - Interactive protocols where server needs client input
/// - Delegation patterns where server invokes client-provided handlers
///
/// Example:
/// ```ignore
/// struct ProgressCallback;
/// impl Callback for ProgressCallback {
///     type Request = ProgressUpdate;
///     type Response = Ack;
///
///     async fn invoke(&self, req: ProgressUpdate) -> Ack {
///         // Handle progress update
///         Ack { received: true }
///     }
/// }
/// ```
pub trait Callback: Send + Sync + 'static {
    /// Request type for the callback.
    type Request: facet::Facet + Send;

    /// Response type for the callback.
    type Response: facet::Facet + Send;

    /// Invoke the callback with a request, returning a response asynchronously.
    fn invoke(&self, req: Self::Request) -> impl Future<Output = Self::Response> + Send;
}

/// Type-erased callback handle that can be stored and invoked dynamically.
///
/// This allows callbacks to be registered and looked up by ID at runtime,
/// while maintaining type safety through the `Callback` trait.
pub struct CallbackHandle<C: Callback> {
    /// Unique identifier for this callback instance.
    pub id: u32,

    /// The actual callback implementation.
    callback: Arc<C>,
}

impl<C: Callback> CallbackHandle<C> {
    /// Create a new callback handle.
    pub fn new(id: u32, callback: Arc<C>) -> Self {
        CallbackHandle { id, callback }
    }

    /// Invoke the callback.
    pub async fn invoke(&self, req: C::Request) -> C::Response {
        self.callback.invoke(req).await
    }
}

impl<C: Callback> Clone for CallbackHandle<C> {
    fn clone(&self) -> Self {
        CallbackHandle {
            id: self.id,
            callback: Arc::clone(&self.callback),
        }
    }
}

// =============================================================================
// Handler Traits
// =============================================================================

/// Handler for unary RPCs.
///
/// Implement this trait to handle a unary method. The handler receives a Request
/// and returns a Future that resolves to a Response.
pub trait UnaryHandler<M: UnaryMethod>: Send + Sync + 'static {
    /// Handle a unary RPC call.
    fn call(&self, req: Request<M>) -> impl Future<Output = Result<Response<M>>> + Send;
}

/// Handler for client streaming RPCs.
///
/// The handler receives a stream of requests and returns a single response.
pub trait ClientStreamingHandler<M: ClientStreamingMethod>: Send + Sync + 'static {
    /// Handle a client streaming RPC call.
    fn call(
        &self,
        stream: RequestStream<M>,
        metadata: Metadata,
    ) -> impl Future<Output = Result<Response<M>>> + Send;
}

/// Handler for server streaming RPCs.
///
/// The handler receives a single request and sends responses via a stream.
pub trait ServerStreamingHandler<M: ServerStreamingMethod>: Send + Sync + 'static {
    /// Handle a server streaming RPC call.
    fn call(
        &self,
        req: Request<M>,
        stream: ResponseStream<M>,
    ) -> impl Future<Output = Result<()>> + Send;
}

/// Handler for bidirectional streaming RPCs.
///
/// The handler receives a request stream and sends responses via a response stream.
pub trait BidiStreamingHandler<M: BidiStreamingMethod>: Send + Sync + 'static {
    /// Handle a bidirectional streaming RPC call.
    fn call(
        &self,
        stream: RequestStream<M>,
        response: ResponseStream<M>,
        metadata: Metadata,
    ) -> impl Future<Output = Result<()>> + Send;
}

// =============================================================================
// Type-erased Handler for Dynamic Dispatch
// =============================================================================

/// Type-erased handler that can be stored in a registry.
///
/// This allows the dispatch system to look up and invoke handlers by method ID
/// at runtime, while preserving type safety through the handler traits.
pub struct DynamicHandler {
    /// Method ID for routing.
    pub method_id: MethodId,

    /// Method name for debugging.
    pub method_name: &'static str,

    /// The actual handler function, boxed for dynamic dispatch.
    handler: Box<dyn Fn(DynamicRequest) -> DynamicResponseFuture + Send + Sync>,
}

/// Type-erased request for dynamic dispatch.
pub struct DynamicRequest {
    /// Serialized request data.
    pub data: Vec<u8>,

    /// Request metadata.
    pub metadata: Metadata,

    /// Optional deadline.
    pub deadline: Option<Instant>,
}

/// Type-erased response future for dynamic dispatch.
pub type DynamicResponseFuture = Pin<Box<dyn Future<Output = Result<DynamicResponse>> + Send>>;

/// Type-erased response for dynamic dispatch.
pub struct DynamicResponse {
    /// Serialized response data.
    pub data: Vec<u8>,

    /// Response metadata.
    pub metadata: Metadata,
}

impl DynamicHandler {
    /// Create a new dynamic handler for a unary method.
    pub fn from_unary<M, H>(handler: Arc<H>) -> Self
    where
        M: UnaryMethod,
        H: UnaryHandler<M>,
    {
        let method_id = MethodId::new(M::ID);
        let method_name = M::NAME;

        let handler_fn = move |req: DynamicRequest| -> DynamicResponseFuture {
            let handler = Arc::clone(&handler);

            Box::pin(async move {
                // Deserialize request
                let data: M::Request = facet::Facet::from_bytes(&req.data)
                    .map_err(|e| RapaceError::invalid_argument(format!("failed to deserialize request: {}", e)))?;

                let typed_req = Request {
                    data,
                    metadata: req.metadata,
                    deadline: req.deadline,
                };

                // Invoke handler
                let response = handler.call(typed_req).await?;

                // Serialize response
                let serialized = facet::Facet::to_bytes(&response.data)
                    .map_err(|e| RapaceError::internal(format!("failed to serialize response: {}", e)))?;

                Ok(DynamicResponse {
                    data: serialized,
                    metadata: response.metadata,
                })
            })
        };

        DynamicHandler {
            method_id,
            method_name,
            handler: Box::new(handler_fn),
        }
    }

    /// Invoke this handler with a dynamic request.
    pub fn invoke(&self, req: DynamicRequest) -> DynamicResponseFuture {
        (self.handler)(req)
    }
}

// =============================================================================
// Context for RPC Handlers
// =============================================================================

/// Context provided to RPC handlers.
///
/// Contains information about the current RPC call, including peer information,
/// request metadata, and utilities for cancellation and timeouts.
pub struct Context {
    /// Request metadata.
    pub metadata: Metadata,

    /// Deadline for this RPC, if set.
    pub deadline: Option<Instant>,

    /// Peer address or identifier (if available).
    pub peer: Option<String>,
}

impl Context {
    /// Create a new context with the given metadata.
    pub fn new(metadata: Metadata) -> Self {
        Context {
            metadata,
            deadline: None,
            peer: None,
        }
    }

    /// Set the deadline for this context.
    pub fn with_deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// Set the peer identifier for this context.
    pub fn with_peer(mut self, peer: String) -> Self {
        self.peer = Some(peer);
        self
    }

    /// Check if the deadline has been exceeded.
    pub fn is_expired(&self) -> bool {
        self.deadline.map_or(false, |d| Instant::now() > d)
    }

    /// Get the remaining time until deadline, if any.
    pub fn time_remaining(&self) -> Option<std::time::Duration> {
        self.deadline.and_then(|d| d.checked_duration_since(Instant::now()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test service and methods

    struct TestService;
    impl Service for TestService {
        const NAME: &'static str = "test.TestService";
    }

    #[derive(Debug, Clone, PartialEq)]
    struct AddRequest {
        a: i32,
        b: i32,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct AddResponse {
        result: i32,
    }

    // Mock Facet implementations for testing
    impl facet::Facet for AddRequest {
        fn to_bytes(&self) -> std::result::Result<Vec<u8>, facet::Error> {
            Ok(format!("{},{}", self.a, self.b).into_bytes())
        }

        fn from_bytes(bytes: &[u8]) -> std::result::Result<Self, facet::Error> {
            let s = std::str::from_utf8(bytes).map_err(|_| facet::Error::InvalidData)?;
            let parts: Vec<&str> = s.split(',').collect();
            if parts.len() != 2 {
                return Err(facet::Error::InvalidData);
            }
            Ok(AddRequest {
                a: parts[0].parse().map_err(|_| facet::Error::InvalidData)?,
                b: parts[1].parse().map_err(|_| facet::Error::InvalidData)?,
            })
        }
    }

    impl facet::Facet for AddResponse {
        fn to_bytes(&self) -> std::result::Result<Vec<u8>, facet::Error> {
            Ok(self.result.to_string().into_bytes())
        }

        fn from_bytes(bytes: &[u8]) -> std::result::Result<Self, facet::Error> {
            let s = std::str::from_utf8(bytes).map_err(|_| facet::Error::InvalidData)?;
            Ok(AddResponse {
                result: s.parse().map_err(|_| facet::Error::InvalidData)?,
            })
        }
    }

    struct AddMethod;
    impl Method for AddMethod {
        const NAME: &'static str = "Add";
        const ID: u32 = 1;
        type Request = AddRequest;
        type Response = AddResponse;
    }
    impl UnaryMethod for AddMethod {}

    #[test]
    fn test_metadata() {
        let mut meta = Metadata::new();
        assert!(meta.is_empty());
        assert_eq!(meta.len(), 0);

        meta.insert_str("key1".into(), "value1".into());
        assert!(!meta.is_empty());
        assert_eq!(meta.len(), 1);
        assert_eq!(meta.get_str("key1"), Some("value1"));

        meta.insert("key2".into(), vec![1, 2, 3]);
        assert_eq!(meta.get("key2"), Some(&[1, 2, 3][..]));

        assert!(meta.contains_key("key1"));
        assert!(meta.contains_key("key2"));
        assert!(!meta.contains_key("key3"));

        meta.remove("key1");
        assert_eq!(meta.len(), 1);
        assert!(!meta.contains_key("key1"));
    }

    #[test]
    fn test_request() {
        let req = Request::<AddMethod>::new(AddRequest { a: 1, b: 2 });
        assert_eq!(req.data.a, 1);
        assert_eq!(req.data.b, 2);
        assert!(req.deadline.is_none());
        assert!(!req.is_expired());

        let future = Instant::now() + std::time::Duration::from_secs(60);
        let req = req.with_deadline(future);
        assert!(!req.is_expired());
        assert!(req.time_remaining().is_some());
    }

    #[test]
    fn test_response() {
        let resp = Response::<AddMethod>::new(AddResponse { result: 3 });
        assert_eq!(resp.data.result, 3);

        let data = resp.into_data();
        assert_eq!(data.result, 3);
    }

    #[test]
    fn test_context() {
        let mut meta = Metadata::new();
        meta.insert_str("trace-id".into(), "abc123".into());

        let ctx = Context::new(meta);
        assert_eq!(ctx.metadata.get_str("trace-id"), Some("abc123"));
        assert!(ctx.deadline.is_none());
        assert!(ctx.peer.is_none());
        assert!(!ctx.is_expired());

        let ctx = ctx
            .with_deadline(Instant::now() + std::time::Duration::from_secs(60))
            .with_peer("127.0.0.1:8080".into());

        assert!(ctx.peer.is_some());
        assert!(!ctx.is_expired());
    }

    #[tokio::test]
    async fn test_request_stream() {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut stream = RequestStream::<AddMethod>::new(rx);

        tx.send(Ok(AddRequest { a: 1, b: 2 })).unwrap();
        tx.send(Ok(AddRequest { a: 3, b: 4 })).unwrap();
        drop(tx);

        let req1 = stream.next().await.unwrap().unwrap();
        assert_eq!(req1.a, 1);

        let req2 = stream.next().await.unwrap().unwrap();
        assert_eq!(req2.a, 3);

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn test_response_stream() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let stream = ResponseStream::<AddMethod>::new(tx);

        stream.send(AddResponse { result: 10 }).unwrap();
        stream.send(AddResponse { result: 20 }).unwrap();
        drop(stream);

        let resp1 = rx.recv().await.unwrap().unwrap();
        assert_eq!(resp1.result, 10);

        let resp2 = rx.recv().await.unwrap().unwrap();
        assert_eq!(resp2.result, 20);

        assert!(rx.recv().await.is_none());
    }
}
