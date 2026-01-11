#![deny(unsafe_code)]

//! WebSocket transport layer for roam RPC.
//!
//! This crate provides WebSocket support for roam services using the
//! [`MessageTransport`] trait from `roam-stream`.
//!
//! Unlike byte stream transports (TCP, Unix sockets), WebSocket provides
//! native message framing, so no COBS encoding is needed.
//!
//! r[impl transport.message.one-to-one] - Each WebSocket message = one roam message.
//! r[impl transport.message.binary] - Uses binary WebSocket frames.
//! r[impl transport.message.multiplexing] - channel_id field provides multiplexing.
//!
//! # Example
//!
//! ```ignore
//! use roam_websocket::{WsTransport, ws_accept, ws_connect};
//!
//! // Server: accept WebSocket connection
//! let ws_stream = accept_async(tcp_stream).await?;
//! let transport = WsTransport::new(ws_stream);
//! let conn = ws_accept(transport, hello).await?;
//! conn.run(&dispatcher).await?;
//!
//! // Client: connect to WebSocket server
//! let (ws_stream, _) = connect_async("ws://localhost:9000").await?;
//! let transport = WsTransport::new(ws_stream);
//! let conn = ws_connect(transport, hello).await?;
//! ```

use std::io;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use roam_stream::{
    Connection, ConnectionError, Hello, Message, MessageTransport, hello_exchange_acceptor,
    hello_exchange_initiator,
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;

/// WebSocket transport for roam messages.
///
/// Wraps a [`WebSocketStream`] and implements [`MessageTransport`].
/// Messages are postcard-encoded and sent as binary WebSocket frames.
pub struct WsTransport<S> {
    stream: WebSocketStream<S>,
    /// Last decoded bytes for error detection.
    last_decoded: Vec<u8>,
}

impl<S> WsTransport<S> {
    /// Create a new WebSocket transport from a stream.
    pub fn new(stream: WebSocketStream<S>) -> Self {
        Self {
            stream,
            last_decoded: Vec::new(),
        }
    }

    /// Get a reference to the underlying WebSocket stream.
    pub fn stream(&self) -> &WebSocketStream<S> {
        &self.stream
    }

    /// Get a mutable reference to the underlying WebSocket stream.
    pub fn stream_mut(&mut self) -> &mut WebSocketStream<S> {
        &mut self.stream
    }

    /// Consume the transport and return the underlying WebSocket stream.
    pub fn into_inner(self) -> WebSocketStream<S> {
        self.stream
    }
}

impl<S> MessageTransport for WsTransport<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    /// Send a message over WebSocket.
    ///
    /// r[impl transport.message.binary] - Send as binary frame.
    /// r[impl transport.message.one-to-one] - One message per frame.
    async fn send(&mut self, msg: &Message) -> io::Result<()> {
        let payload = facet_postcard::to_vec(msg)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        self.stream
            .send(WsMessage::Binary(payload.into()))
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(())
    }

    /// Receive a message with timeout.
    async fn recv_timeout(&mut self, timeout: Duration) -> io::Result<Option<Message>> {
        tokio::time::timeout(timeout, self.recv())
            .await
            .unwrap_or(Ok(None))
    }

    /// Receive a message (blocking until one arrives or connection closes).
    async fn recv(&mut self) -> io::Result<Option<Message>> {
        loop {
            match self.stream.next().await {
                Some(Ok(WsMessage::Binary(data))) => {
                    // r[impl transport.message.binary] - Process binary frames.
                    self.last_decoded = data.to_vec();
                    let msg: Message = facet_postcard::from_slice(&data).map_err(|e| {
                        io::Error::new(io::ErrorKind::InvalidData, format!("postcard: {e}"))
                    })?;
                    return Ok(Some(msg));
                }
                Some(Ok(WsMessage::Close(_))) => {
                    // Clean close
                    return Ok(None);
                }
                Some(Ok(WsMessage::Ping(data))) => {
                    // Respond to ping with pong
                    let _ = self.stream.send(WsMessage::Pong(data)).await;
                    continue;
                }
                Some(Ok(WsMessage::Pong(_))) => {
                    // Ignore pongs
                    continue;
                }
                Some(Ok(WsMessage::Text(_))) => {
                    // r[impl transport.message.binary] - Text frames are protocol violations.
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "text frames not allowed",
                    ));
                }
                Some(Ok(WsMessage::Frame(_))) => {
                    // Raw frames shouldn't appear here
                    continue;
                }
                Some(Err(e)) => {
                    return Err(io::Error::other(e.to_string()));
                }
                None => {
                    // Stream ended
                    return Ok(None);
                }
            }
        }
    }

    fn last_decoded(&self) -> &[u8] {
        &self.last_decoded
    }
}

/// Type alias for WebSocket-based connections over TCP.
pub type WsConnection<S> = Connection<WsTransport<S>>;

/// Perform Hello exchange as the acceptor over WebSocket.
///
/// r[impl message.hello.timing] - Send Hello immediately after connection.
/// r[impl message.hello.ordering] - Hello sent before any other message.
pub async fn ws_accept<S>(
    transport: WsTransport<S>,
    hello: Hello,
) -> Result<WsConnection<S>, ConnectionError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    hello_exchange_acceptor(transport, hello).await
}

/// Perform Hello exchange as the initiator over WebSocket.
///
/// r[impl message.hello.timing] - Send Hello immediately after connection.
/// r[impl message.hello.ordering] - Hello sent before any other message.
pub async fn ws_connect<S>(
    transport: WsTransport<S>,
    hello: Hello,
) -> Result<WsConnection<S>, ConnectionError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    hello_exchange_initiator(transport, hello).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use tokio_tungstenite::{accept_async, connect_async};

    #[tokio::test]
    async fn websocket_hello_exchange() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let ws_url = format!("ws://{}", addr);

        let hello = Hello::V1 {
            max_payload_size: 1024 * 1024,
            initial_channel_credit: 64 * 1024,
        };

        // Server task
        let server_hello = hello.clone();
        let server_handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let ws_stream = accept_async(stream).await.unwrap();
            let transport = WsTransport::new(ws_stream);
            ws_accept(transport, server_hello).await
        });

        // Client
        let (ws_stream, _) = connect_async(&ws_url).await.unwrap();
        let transport = WsTransport::new(ws_stream);
        let client_conn = ws_connect(transport, hello.clone()).await.unwrap();

        // Verify negotiation
        assert_eq!(client_conn.negotiated().max_payload_size, 1024 * 1024);

        // Server should also succeed
        let server_conn = server_handle.await.unwrap().unwrap();
        assert_eq!(server_conn.negotiated().max_payload_size, 1024 * 1024);
    }
}
