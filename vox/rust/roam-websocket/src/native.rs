//! Native (tokio-tungstenite) WebSocket transport.

use std::io;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use roam_session::MessageTransport;
use roam_stream::{
    ConnectionError, ConnectionHandle, Driver, FramedClient, HandshakeConfig, Message,
    MessageConnector, RetryPolicy, ServiceDispatcher, accept_framed, connect_framed,
    connect_framed_with_policy,
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;

/// WebSocket transport for roam messages (native implementation).
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
        peeps::timeout!(timeout, self.recv(), "ws.recv")
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

/// Accept a WebSocket connection and perform handshake.
///
/// r[impl message.hello.timing] - Send Hello immediately after connection.
/// r[impl message.hello.ordering] - Hello sent before any other message.
pub async fn ws_accept<S, D>(
    transport: WsTransport<S>,
    config: HandshakeConfig,
    dispatcher: D,
) -> Result<
    (
        ConnectionHandle,
        roam_session::IncomingConnections,
        Driver<roam_session::DiagnosticTransport<WsTransport<S>>, D>,
    ),
    ConnectionError,
>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
    D: ServiceDispatcher,
{
    accept_framed(transport, config, dispatcher).await
}

/// Connect via WebSocket with automatic reconnection.
///
/// Returns a client that automatically reconnects on failure.
/// The connector must implement [`MessageConnector`] with a `WsTransport` transport.
pub fn ws_connect<C, D>(connector: C, config: HandshakeConfig, dispatcher: D) -> FramedClient<C, D>
where
    C: MessageConnector,
    D: ServiceDispatcher + Clone,
{
    connect_framed(connector, config, dispatcher)
}

/// Connect via WebSocket with a custom retry policy.
pub fn ws_connect_with_policy<C, D>(
    connector: C,
    config: HandshakeConfig,
    dispatcher: D,
    retry_policy: RetryPolicy,
) -> FramedClient<C, D>
where
    C: MessageConnector,
    D: ServiceDispatcher + Clone,
{
    connect_framed_with_policy(connector, config, dispatcher, retry_policy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use roam_stream::NoDispatcher;
    use tokio::net::TcpListener;
    use tokio_tungstenite::{accept_async, connect_async};

    #[tokio::test]
    async fn websocket_hello_exchange() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let ws_url = format!("ws://{}", addr);

        let config = HandshakeConfig::default();

        // Server task
        let server_config = config.clone();
        let server_handle = peeps::spawn_tracked!("roam_ws_test_server", async move {
            let (stream, _) = listener.accept().await.unwrap();
            let ws_stream = accept_async(stream).await.unwrap();
            let transport = WsTransport::new(ws_stream);
            ws_accept(transport, server_config, NoDispatcher).await
        });

        // Client - for now just connect raw and do handshake manually
        let (ws_stream, _) = connect_async(&ws_url).await.unwrap();
        let transport = WsTransport::new(ws_stream);
        let (client_handle, _client_incoming, client_driver) =
            accept_framed(transport, config, NoDispatcher)
                .await
                .unwrap();

        // Spawn client driver
        peeps::spawn_tracked!("roam_ws_test_client_driver", client_driver.run());

        // Server should also succeed
        let (server_handle_result, _server_incoming, server_driver) =
            server_handle.await.unwrap().unwrap();
        peeps::spawn_tracked!("roam_ws_test_server_driver", server_driver.run());

        // Both handles exist - just verify they were created
        let _ = client_handle;
        let _ = server_handle_result;
    }
}
