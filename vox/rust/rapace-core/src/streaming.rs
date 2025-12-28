//! Streaming types for server-streaming and client-streaming RPCs.
//!
//! # Server-Streaming Pattern
//!
//! For server-streaming RPCs, the server method returns a `Streaming<T>`:
//!
//! ```ignore
//! use rapace_core::Streaming;
//!
//! #[rapace::service]
//! trait RangeService {
//!     async fn range(&self, n: u32) -> Streaming<u32>;
//! }
//! ```
//!
//! The macro generates:
//! - Server: Calls the method, iterates the stream, sends DATA frames, then EOS
//! - Client: An `async fn` that returns `Result<Streaming<T>, RpcError>`
//!
//! # Client Usage
//!
//! ```ignore
//! use futures::StreamExt;
//!
//! let mut stream = client.range(5).await?;
//! while let Some(item) = stream.next().await {
//!     let value = item?;
//!     println!("{}", value);
//! }
//! ```
//!
//! # Server Implementation
//!
//! ```ignore
//! use rapace_core::Streaming;
//!
//! impl RangeService for MyImpl {
//!     async fn range(&self, n: u32) -> Streaming<u32> {
//!         let (tx, rx) = tokio::sync::mpsc::channel(16);
//!         tokio::spawn(async move {
//!             for i in 0..n {
//!                 if tx.send(Ok(i)).await.is_err() {
//!                     break;
//!                 }
//!             }
//!         });
//!         Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
//!     }
//! }
//! ```

use std::future::Future;
use std::pin::Pin;

use crate::RpcError;

/// Type alias for streaming RPC results.
///
/// Service traits should use this in their return types:
/// ```ignore
/// async fn range(&self, n: u32) -> Streaming<u32>;
/// ```
///
/// The outer `async fn` gives you the stream, and each item of the stream
/// is a `Result<T, RpcError>` representing either a value or an error.
pub type Streaming<T> = Pin<Box<dyn futures_core::Stream<Item = Result<T, RpcError>> + Send>>;

/// A sink for sending streaming items from server to client.
///
/// This is an internal building block. For service trait definitions,
/// use `Streaming<T>` as the return type instead.
pub trait StreamSink<T>: Send {
    /// Send an item to the client.
    ///
    /// Returns `Err` if the channel was cancelled or an error occurred.
    fn send(&mut self, item: T) -> Pin<Box<dyn Future<Output = Result<(), RpcError>> + Send + '_>>;

    /// Check if the stream has been cancelled by the client.
    fn is_cancelled(&self) -> bool;
}

/// A source for receiving streaming items (used in client-streaming).
///
/// This is an internal building block for future client-streaming support.
pub trait StreamSource<T> {
    /// Receive the next item, or `None` if the stream is complete.
    #[allow(clippy::type_complexity)]
    fn recv(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<T, RpcError>>> + Send + '_>>;
}

/// Marker trait for types that can be streamed.
///
/// Types must implement `Facet<'static>` for serialization and be `Send`.
pub trait Streamable: facet::Facet<'static> + Send + 'static {}

// Blanket implementation for all compatible types
impl<T: facet::Facet<'static> + Send + 'static> Streamable for T {}

#[cfg(test)]
mod tests {
    use super::*;

    // StreamSink and StreamSource are object-safe
    fn _assert_sink_object_safe(_: &dyn StreamSink<i32>) {}
    fn _assert_source_object_safe(_: &dyn StreamSource<i32>) {}

    #[test]
    fn test_streamable_impl() {
        fn _is_streamable<T: Streamable>() {}
        _is_streamable::<i32>();
        _is_streamable::<String>();
        _is_streamable::<Vec<u8>>();
    }
}
