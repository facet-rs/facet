//! WebSocket connection handler.
//!
//! r[bridge.ws.subprotocol] - Validates `roam-bridge.v1` subprotocol.
//! r[bridge.ws.text-frames] - All messages are JSON text frames.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use facet_core::{Def, Shape};
use futures_util::{SinkExt, StreamExt};
#[allow(unused_imports)]
use roam_schema::{MethodDetail, contains_stream, is_rx, is_tx};
use tokio::sync::mpsc;

use crate::{BridgeError, BridgeService, ProtocolErrorKind};

use super::messages::{ClientMessage, ServerMessage};
use super::session::{ChannelDirection, WsSession};

/// Handle a WebSocket connection.
///
/// r[bridge.ws.subprotocol] - Connection validated before this is called.
pub async fn handle_websocket(
    ws: WebSocket,
    services: Arc<HashMap<String, Arc<dyn BridgeService>>>,
) {
    let (mut ws_sink, mut ws_stream) = ws.split();

    // Channel for outgoing messages
    let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<ServerMessage>(256);

    // Create session state
    let session = Arc::new(tokio::sync::Mutex::new(WsSession::new(
        services,
        outgoing_tx.clone(),
    )));

    // Spawn task to send outgoing messages
    let send_task = {
        let session = Arc::clone(&session);
        tokio::spawn(async move {
            while let Some(msg) = outgoing_rx.recv().await {
                let json = match serde_json::to_string(&msg) {
                    Ok(j) => j,
                    Err(e) => {
                        tracing::error!("Failed to serialize outgoing message: {}", e);
                        continue;
                    }
                };
                // r[bridge.ws.text-frames]
                if ws_sink.send(Message::Text(json.into())).await.is_err() {
                    tracing::debug!("WebSocket send failed, closing");
                    break;
                }
            }
            // Try to send goodbye before closing
            let goodbye = ServerMessage::goodbye("connection.closed");
            if let Ok(json) = serde_json::to_string(&goodbye) {
                let _ = ws_sink.send(Message::Text(json.into())).await;
            }
            let _ = ws_sink.close().await;
            drop(session); // Keep session alive until send task ends
        })
    };

    // Process incoming messages
    while let Some(msg_result) = ws_stream.next().await {
        let msg = match msg_result {
            Ok(m) => m,
            Err(e) => {
                tracing::debug!("WebSocket receive error: {}", e);
                break;
            }
        };

        match msg {
            // r[bridge.ws.text-frames]
            Message::Text(text) => {
                let text_str: &str = text.as_ref();
                match serde_json::from_str::<ClientMessage>(text_str) {
                    Ok(client_msg) => {
                        if let Err(e) =
                            handle_client_message(client_msg, Arc::clone(&session)).await
                        {
                            tracing::warn!("Error handling client message: {}", e);
                            // Send goodbye on protocol error
                            let _ = outgoing_tx
                                .send(ServerMessage::goodbye(format!("error: {}", e)))
                                .await;
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse client message: {}", e);
                        let _ = outgoing_tx
                            .send(ServerMessage::goodbye("bridge.ws.message-format"))
                            .await;
                        break;
                    }
                }
            }
            Message::Binary(_) => {
                // r[bridge.ws.text-frames] - Binary frames not allowed
                tracing::warn!("Received binary frame, protocol violation");
                let _ = outgoing_tx
                    .send(ServerMessage::goodbye("bridge.ws.text-frames"))
                    .await;
                break;
            }
            Message::Ping(data) => {
                let _ = outgoing_tx
                    .send(ServerMessage::Goodbye {
                        reason: "pong".into(),
                    })
                    .await;
                // Actually send pong through the sink directly
                // Note: This is handled automatically by axum in most cases
                let _ = data; // Acknowledge we received it
            }
            Message::Pong(_) => {
                // Ignore pong
            }
            Message::Close(_) => {
                tracing::debug!("Client closed WebSocket");
                break;
            }
        }
    }

    // Close the outgoing channel to stop the send task
    drop(outgoing_tx);

    // Wait for send task to complete
    let _ = send_task.await;
}

/// Handle a parsed client message.
async fn handle_client_message(
    msg: ClientMessage,
    session: Arc<tokio::sync::Mutex<WsSession>>,
) -> Result<(), BridgeError> {
    match msg {
        ClientMessage::Request {
            id,
            service,
            method,
            args,
            metadata,
        } => handle_request(session, id, service, method, args, metadata).await,
        ClientMessage::Data { channel, value } => handle_data(session, channel, value).await,
        ClientMessage::Close { channel } => handle_close(session, channel).await,
        ClientMessage::Reset { channel } => handle_reset(session, channel).await,
        ClientMessage::Credit { channel, bytes } => handle_credit(session, channel, bytes).await,
        ClientMessage::Cancel { id } => handle_cancel(session, id).await,
    }
}

/// Handle a request message.
///
/// r[bridge.ws.request]
async fn handle_request(
    session: Arc<tokio::sync::Mutex<WsSession>>,
    request_id: u64,
    service_name: String,
    method_name: String,
    args: serde_json::Value,
    _metadata: HashMap<String, serde_json::Value>,
) -> Result<(), BridgeError> {
    // Look up service and method
    let lookup_result = {
        let session_guard = session.lock().await;
        let service = match session_guard.get_service(&service_name) {
            Ok(s) => s,
            Err(_) => {
                // Unknown service - send error response, don't close connection
                let _ = session_guard
                    .send(ServerMessage::protocol_error(request_id, "unknown_service"))
                    .await;
                return Ok(());
            }
        };
        let detail = service.service_detail();

        let method = match detail.methods.iter().find(|m| m.method_name == method_name) {
            Some(m) => m,
            None => {
                // Unknown method - send error response, don't close connection
                let _ = session_guard
                    .send(ServerMessage::protocol_error(request_id, "unknown_method"))
                    .await;
                return Ok(());
            }
        };

        let method_id = roam_hash::method_id_from_detail(method);
        let has_channels = method.args.iter().any(|a| contains_stream(a.ty))
            || contains_stream(method.return_type);

        (service, method.clone(), method_id, has_channels)
    };

    let (service, method_detail, method_id, has_channels) = lookup_result;

    // Register the call
    {
        let mut session_guard = session.lock().await;
        session_guard.register_call(request_id, service_name.clone(), method_name.clone());
    }

    // Spawn a task to handle the call
    let session_clone = Arc::clone(&session);
    tokio::spawn(async move {
        let result = if has_channels {
            handle_streaming_call(
                session_clone.clone(),
                request_id,
                service,
                &method_detail,
                method_id,
                args,
            )
            .await
        } else {
            handle_simple_call(
                session_clone.clone(),
                request_id,
                service,
                &method_detail,
                method_id,
                args,
            )
            .await
        };

        // Complete the call and send response
        {
            let mut session_guard = session_clone.lock().await;
            session_guard.complete_call(request_id);

            if let Err(e) = result {
                tracing::warn!("Call {} failed: {}", request_id, e);
            }
        }
    });

    Ok(())
}

/// Handle a simple (non-streaming) RPC call.
async fn handle_simple_call(
    session: Arc<tokio::sync::Mutex<WsSession>>,
    request_id: u64,
    service: Arc<dyn BridgeService>,
    method: &MethodDetail,
    _method_id: u64,
    args: serde_json::Value,
) -> Result<(), BridgeError> {
    // Convert JSON args to postcard
    let args_json = serde_json::to_vec(&args)
        .map_err(|e| BridgeError::bad_request(format!("Invalid args: {}", e)))?;

    // Get the connection handle from the service and make the call
    // We need to call through the BridgeService trait
    let metadata = crate::BridgeMetadata::default();
    let response = service
        .call_json(&method.method_name, &args_json, metadata)
        .await;

    // Send the response
    let session_guard = session.lock().await;
    match response {
        Ok(bridge_response) => {
            let msg = bridge_response_to_ws(request_id, bridge_response)?;
            session_guard.send(msg).await
        }
        Err(_e) => {
            session_guard
                .send(ServerMessage::protocol_error(request_id, "bridge_error"))
                .await
        }
    }
}

/// Handle a streaming RPC call with channels.
async fn handle_streaming_call(
    session: Arc<tokio::sync::Mutex<WsSession>>,
    request_id: u64,
    _service: Arc<dyn BridgeService>,
    method: &MethodDetail,
    _method_id: u64,
    args: serde_json::Value,
) -> Result<(), BridgeError> {
    // For streaming calls, we need to:
    // 1. Parse the args to find channel IDs
    // 2. Set up channel routing for Tx channels (client->server)
    // 3. Set up channel routing for Rx channels (server->client)
    // 4. Make the roam call with channel binding
    // 5. Forward data messages bidirectionally

    // For now, we'll use a simplified approach that works with the existing
    // BridgeService trait. A full implementation would require direct access
    // to the ConnectionHandle.

    // Extract channel info from method signature
    let mut tx_channels: Vec<(usize, &'static Shape)> = Vec::new(); // Client sends to server
    let mut rx_channels: Vec<(usize, &'static Shape)> = Vec::new(); // Server sends to client

    for (i, arg) in method.args.iter().enumerate() {
        if is_tx(arg.ty) {
            // Tx<T> from caller's POV means client sends, server receives
            if let Some(elem_shape) = get_channel_element_type(arg.ty) {
                tx_channels.push((i, elem_shape));
            }
        } else if is_rx(arg.ty) {
            // Rx<T> from caller's POV means client receives, server sends
            if let Some(elem_shape) = get_channel_element_type(arg.ty) {
                rx_channels.push((i, elem_shape));
            }
        }
    }

    // Extract channel IDs from args
    let args_array = args
        .as_array()
        .ok_or_else(|| BridgeError::bad_request("Args must be a JSON array"))?;

    // Register channels for each Tx argument (client -> server)
    for (arg_idx, elem_shape) in &tx_channels {
        if let Some(channel_id) = args_array.get(*arg_idx).and_then(|v| v.as_u64()) {
            // Create a channel for forwarding data to the roam connection
            let (roam_tx, _roam_rx) = mpsc::channel::<Vec<u8>>(256);

            let mut session_guard = session.lock().await;
            session_guard.register_channel(
                channel_id,
                request_id,
                ChannelDirection::ClientToServer,
                elem_shape,
                Some(roam_tx),
            );
        }
    }

    // Register channels for each Rx argument (server -> client)
    for (arg_idx, elem_shape) in &rx_channels {
        if let Some(channel_id) = args_array.get(*arg_idx).and_then(|v| v.as_u64()) {
            let mut session_guard = session.lock().await;
            session_guard.register_channel(
                channel_id,
                request_id,
                ChannelDirection::ServerToClient,
                elem_shape,
                None,
            );
        }
    }

    // For now, return an error indicating streaming is not yet fully implemented
    // TODO: Implement full streaming support with ConnectionHandle::call
    let session_guard = session.lock().await;
    session_guard
        .send(ServerMessage::protocol_error(
            request_id,
            "streaming_not_implemented",
        ))
        .await
}

/// Handle incoming data on a channel.
///
/// r[bridge.ws.data]
async fn handle_data(
    session: Arc<tokio::sync::Mutex<WsSession>>,
    channel_id: u64,
    value: serde_json::Value,
) -> Result<(), BridgeError> {
    let mut session_guard = session.lock().await;

    let channel = session_guard
        .get_channel_mut(channel_id)
        .ok_or_else(|| BridgeError::bad_request(format!("Unknown channel: {}", channel_id)))?;

    // Verify direction (only client->server channels accept data from client)
    if channel.direction != ChannelDirection::ClientToServer {
        return Err(BridgeError::bad_request(format!(
            "Channel {} is not a Tx channel",
            channel_id
        )));
    }

    // Convert JSON value to postcard using the element shape
    let json_bytes = serde_json::to_vec(&value)
        .map_err(|e| BridgeError::bad_request(format!("Invalid value: {}", e)))?;

    let postcard_bytes =
        crate::transcode::json_args_to_postcard(&json_bytes, &[channel.element_shape])?;

    // Forward to the roam connection
    if let Some(roam_tx) = &channel.roam_tx {
        roam_tx
            .send(postcard_bytes)
            .await
            .map_err(|_| BridgeError::internal("Channel send failed"))?;
    }

    Ok(())
}

/// Handle channel close.
///
/// r[bridge.ws.close]
async fn handle_close(
    session: Arc<tokio::sync::Mutex<WsSession>>,
    channel_id: u64,
) -> Result<(), BridgeError> {
    let mut session_guard = session.lock().await;

    let channel = session_guard.remove_channel(channel_id);

    if let Some(channel) = channel {
        // Dropping the roam_tx will signal close to the roam connection
        if channel.direction != ChannelDirection::ClientToServer {
            return Err(BridgeError::bad_request(format!(
                "Channel {} cannot be closed by client",
                channel_id
            )));
        }
    }

    Ok(())
}

/// Handle channel reset.
///
/// r[bridge.ws.reset]
async fn handle_reset(
    session: Arc<tokio::sync::Mutex<WsSession>>,
    channel_id: u64,
) -> Result<(), BridgeError> {
    let mut session_guard = session.lock().await;

    // Just remove the channel - reset is forceful
    session_guard.remove_channel(channel_id);

    Ok(())
}

/// Handle credit grant.
///
/// r[bridge.ws.credit]
async fn handle_credit(
    session: Arc<tokio::sync::Mutex<WsSession>>,
    channel_id: u64,
    bytes: u64,
) -> Result<(), BridgeError> {
    let mut session_guard = session.lock().await;
    session_guard.add_credit(channel_id, bytes);
    Ok(())
}

/// Handle request cancellation.
///
/// r[bridge.ws.cancel]
async fn handle_cancel(
    session: Arc<tokio::sync::Mutex<WsSession>>,
    request_id: u64,
) -> Result<(), BridgeError> {
    let mut session_guard = session.lock().await;

    if session_guard.cancel_call(request_id) {
        // TODO: Actually propagate cancellation to the roam call
        tracing::debug!("Cancelled request {}", request_id);
    }

    Ok(())
}

/// Convert a BridgeResponse to a WebSocket ServerMessage.
fn bridge_response_to_ws(
    request_id: u64,
    response: crate::BridgeResponse,
) -> Result<ServerMessage, BridgeError> {
    match response {
        crate::BridgeResponse::Success(json_bytes) => {
            let value: serde_json::Value = serde_json::from_slice(&json_bytes)
                .map_err(|e| BridgeError::internal(format!("Invalid JSON in response: {}", e)))?;
            Ok(ServerMessage::success(request_id, value))
        }
        crate::BridgeResponse::UserError(json_bytes) => {
            let value: serde_json::Value = serde_json::from_slice(&json_bytes)
                .map_err(|e| BridgeError::internal(format!("Invalid JSON in error: {}", e)))?;
            Ok(ServerMessage::user_error(request_id, value))
        }
        crate::BridgeResponse::ProtocolError(kind) => {
            let error_str = match kind {
                ProtocolErrorKind::UnknownMethod => "unknown_method",
                ProtocolErrorKind::InvalidPayload => "invalid_payload",
                ProtocolErrorKind::Cancelled => "cancelled",
            };
            Ok(ServerMessage::protocol_error(request_id, error_str))
        }
    }
}

/// Extract the element type from a Tx<T> or Rx<T> shape.
#[allow(dead_code)]
fn get_channel_element_type(shape: &'static Shape) -> Option<&'static Shape> {
    // The shape should have a generic parameter for the element type
    // type_params is a slice of TypeParam
    if let Some(first) = shape.type_params.first() {
        return Some(first.shape);
    }
    None
}

/// Extract the success and error types from a method's return type.
#[allow(dead_code)]
fn extract_result_types(method: &MethodDetail) -> (&'static Shape, Option<&'static Shape>) {
    let return_shape = method.return_type;

    // Check if the return type is Result<T, E>
    if let Def::Result(result_def) = return_shape.def {
        let success_shape = result_def.t();
        let error_shape = result_def.e();
        return (success_shape, Some(error_shape));
    }

    // Infallible method: return type is T directly
    (return_shape, None)
}
