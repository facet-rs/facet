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
use roam_session::{IncomingChannelMessage, ResponseData, TransportError};
use tokio::sync::{mpsc, oneshot};

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
                        error!("Failed to serialize outgoing message: {}", e);
                        continue;
                    }
                };
                // r[bridge.ws.text-frames]
                if ws_sink.send(Message::Text(json.into())).await.is_err() {
                    debug!("WebSocket send failed, closing");
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
                debug!("WebSocket receive error: {e}");
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
                            warn!("Error handling client message: {}", e);
                            // Send goodbye on protocol error
                            let _ = outgoing_tx
                                .send(ServerMessage::goodbye(format!("error: {}", e)))
                                .await;
                            break;
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse client message: {}", e);
                        let _ = outgoing_tx
                            .send(ServerMessage::goodbye("bridge.ws.message-format"))
                            .await;
                        break;
                    }
                }
            }
            Message::Binary(_) => {
                // r[bridge.ws.text-frames] - Binary frames not allowed
                warn!("Received binary frame, protocol violation");
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
                debug!("Client closed WebSocket");
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

    // For streaming calls, set up channels first before spawning
    // This ensures channels are registered before any data messages arrive
    if has_channels {
        let streaming_state = setup_streaming_call(
            Arc::clone(&session),
            request_id,
            Arc::clone(&service),
            &method_detail,
            method_id,
            args,
        )
        .await?;

        // Spawn a task to run the streaming call (channels are already registered)
        let session_clone = Arc::clone(&session);
        tokio::spawn(async move {
            let result = run_streaming_call(session_clone.clone(), streaming_state).await;

            // Complete the call
            {
                let mut session_guard = session_clone.lock().await;
                session_guard.complete_call(request_id);

                if let Err(e) = result {
                    warn!("Streaming call {} failed: {}", request_id, e);
                }
            }
        });
    } else {
        // Simple calls can be spawned directly
        let session_clone = Arc::clone(&session);
        tokio::spawn(async move {
            let result = handle_simple_call(
                session_clone.clone(),
                request_id,
                service,
                &method_detail,
                method_id,
                args,
            )
            .await;

            // Complete the call and send response
            {
                let mut session_guard = session_clone.lock().await;
                session_guard.complete_call(request_id);

                if let Err(e) = result {
                    warn!("Call {} failed: {}", request_id, e);
                }
            }
        });
    }

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

/// State needed to run a streaming call after setup.
struct StreamingCallState {
    #[allow(dead_code)]
    session: Arc<tokio::sync::Mutex<WsSession>>,
    request_id: u64,
    ws_to_roam_rx_map: HashMap<u64, (u64, &'static Shape)>,
    roam_to_ws_tx_map: HashMap<u64, (u64, &'static Shape)>,
    roam_receivers: Vec<(u64, mpsc::Receiver<IncomingChannelMessage>)>,
    /// Response receiver - the call has already been sent when this is set
    response_rx: oneshot::Receiver<Result<ResponseData, TransportError>>,
    return_shape: &'static Shape,
    error_shape: Option<&'static Shape>,
}

/// Set up a streaming call by registering channels.
///
/// This must be called synchronously before the task is spawned,
/// so that channels are registered before any data messages arrive.
///
/// r[bridge.ws.streaming]
async fn setup_streaming_call(
    session: Arc<tokio::sync::Mutex<WsSession>>,
    request_id: u64,
    service: Arc<dyn BridgeService>,
    method: &MethodDetail,
    method_id: u64,
    args: serde_json::Value,
) -> Result<StreamingCallState, BridgeError> {
    // Get the connection handle for streaming support
    let handle = service.connection_handle().clone();

    // Extract channel info from method signature
    let mut rx_channels: Vec<(usize, &'static Shape)> = Vec::new();
    let mut tx_channels: Vec<(usize, &'static Shape)> = Vec::new();

    for (i, arg) in method.args.iter().enumerate() {
        if is_rx(arg.ty) {
            if let Some(elem_shape) = get_channel_element_type(arg.ty) {
                rx_channels.push((i, elem_shape));
            }
        } else if is_tx(arg.ty)
            && let Some(elem_shape) = get_channel_element_type(arg.ty)
        {
            tx_channels.push((i, elem_shape));
        }
    }

    // Extract WebSocket channel IDs from args
    let args_array = args
        .as_array()
        .ok_or_else(|| BridgeError::bad_request("Args must be a JSON array"))?;

    // Build channel mappings
    let mut roam_channel_ids: Vec<u64> = Vec::new();
    let mut ws_to_roam_rx_map: HashMap<u64, (u64, &'static Shape)> = HashMap::new();
    let mut roam_to_ws_tx_map: HashMap<u64, (u64, &'static Shape)> = HashMap::new();

    // Process Rx channels (client sends to server)
    for (arg_idx, elem_shape) in &rx_channels {
        if let Some(ws_channel_id) = args_array.get(*arg_idx).and_then(|v| v.as_u64()) {
            let roam_channel_id = handle.alloc_channel_id();
            roam_channel_ids.push(roam_channel_id);
            ws_to_roam_rx_map.insert(ws_channel_id, (roam_channel_id, elem_shape));
        }
    }

    // Process Tx channels (server sends to client)
    for (arg_idx, elem_shape) in &tx_channels {
        if let Some(ws_channel_id) = args_array.get(*arg_idx).and_then(|v| v.as_u64()) {
            let roam_channel_id = handle.alloc_channel_id();
            roam_channel_ids.push(roam_channel_id);
            roam_to_ws_tx_map.insert(roam_channel_id, (ws_channel_id, elem_shape));
        }
    }

    // Build args with roam channel IDs substituted
    let mut modified_args = args_array.clone();
    let mut roam_channel_idx = 0;
    for (i, arg) in method.args.iter().enumerate() {
        if (is_rx(arg.ty) || is_tx(arg.ty)) && roam_channel_idx < roam_channel_ids.len() {
            modified_args[i] = serde_json::Value::Number(serde_json::Number::from(
                roam_channel_ids[roam_channel_idx],
            ));
            roam_channel_idx += 1;
        }
    }

    // Transcode the modified args to postcard
    let arg_shapes: Vec<&'static Shape> = method.args.iter().map(|a| a.ty).collect();
    let args_json = serde_json::to_vec(&modified_args)
        .map_err(|e| BridgeError::bad_request(format!("Failed to serialize args: {}", e)))?;
    let postcard_payload = crate::transcode::json_args_to_postcard(&args_json, &arg_shapes)?;

    // Set up channels for receiving data from roam (Tx channels)
    let mut roam_receivers: Vec<(u64, mpsc::Receiver<IncomingChannelMessage>)> = Vec::new();
    for &roam_channel_id in roam_to_ws_tx_map.keys() {
        let (tx, rx) = mpsc::channel::<IncomingChannelMessage>(256);
        handle.register_incoming(roam_channel_id, tx);
        roam_receivers.push((roam_channel_id, rx));
    }

    // Get the driver_tx for sending Data/Close messages to roam
    let driver_tx = handle.driver_tx();

    // Register WebSocket channels in the session BEFORE sending the call
    // This is critical - channels must be registered before any data messages arrive
    {
        let mut session_guard = session.lock().await;

        // Store driver_tx for handle_data to use
        session_guard.set_driver_tx(driver_tx.clone());

        for (&ws_channel_id, &(roam_channel_id, elem_shape)) in &ws_to_roam_rx_map {
            let (ws_data_tx, _ws_data_rx) = mpsc::channel::<Vec<u8>>(256);
            session_guard.register_channel(
                ws_channel_id,
                request_id,
                ChannelDirection::ClientToServer,
                elem_shape,
                Some(ws_data_tx),
            );
            session_guard.set_roam_channel_id(ws_channel_id, roam_channel_id);
        }
        for (ws_channel_id, elem_shape) in roam_to_ws_tx_map.values() {
            session_guard.register_channel(
                *ws_channel_id,
                request_id,
                ChannelDirection::ServerToClient,
                elem_shape,
                None,
            );
        }
    }

    // Create the response channel and send the Call message IMMEDIATELY
    // This ensures the Request is queued before any Data messages can be forwarded
    let (response_tx, response_rx) = oneshot::channel();
    let roam_request_id = handle.alloc_request_id();

    let call_msg = roam_session::DriverMessage::Call {
        conn_id: roam_wire::ConnectionId::ROOT,
        request_id: roam_request_id,
        method_id,
        metadata: Vec::new(),
        channels: roam_channel_ids,
        payload: postcard_payload,
        response_tx,
    };

    driver_tx
        .send(call_msg)
        .await
        .map_err(|_| BridgeError::internal("Failed to send call to roam driver"))?;

    let (return_shape, error_shape) = extract_result_types(method);

    Ok(StreamingCallState {
        session,
        request_id,
        ws_to_roam_rx_map,
        roam_to_ws_tx_map,
        roam_receivers,
        response_rx,
        return_shape,
        error_shape,
    })
}

/// Run a streaming call after setup.
async fn run_streaming_call(
    session: Arc<tokio::sync::Mutex<WsSession>>,
    state: StreamingCallState,
) -> Result<(), BridgeError> {
    let StreamingCallState {
        request_id,
        ws_to_roam_rx_map,
        roam_to_ws_tx_map,
        roam_receivers,
        response_rx,
        return_shape,
        error_shape,
        ..
    } = state;

    let outgoing_tx = {
        let session_guard = session.lock().await;
        session_guard.outgoing_tx().clone()
    };

    // Spawn tasks to forward data from roam to WebSocket (Tx channels)
    for (roam_channel_id, mut rx) in roam_receivers {
        let (ws_channel_id, elem_shape) = roam_to_ws_tx_map[&roam_channel_id];
        let outgoing_tx = outgoing_tx.clone();
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                match msg {
                    IncomingChannelMessage::Data(postcard_data) => {
                        match crate::transcode::postcard_to_json_with_shape(
                            &postcard_data,
                            elem_shape,
                        ) {
                            Ok(json_bytes) => {
                                if let Ok(value) =
                                    serde_json::from_slice::<serde_json::Value>(&json_bytes)
                                {
                                    let _ = outgoing_tx
                                        .send(ServerMessage::data(ws_channel_id, value))
                                        .await;
                                }
                            }
                            Err(e) => {
                                warn!("Failed to transcode channel data: {}", e);
                            }
                        }
                    }
                    IncomingChannelMessage::Close => {
                        break;
                    }
                }
            }
        });
    }

    // Wait for the response - the call was already sent in setup_streaming_call
    let response = response_rx
        .await
        .map_err(|_| BridgeError::internal("Response channel closed"))?;

    // Clean up channels in session
    {
        let mut session_guard = session.lock().await;
        for &ws_channel_id in ws_to_roam_rx_map.keys() {
            session_guard.remove_channel(ws_channel_id);
        }
        for (ws_channel_id, _) in roam_to_ws_tx_map.values() {
            session_guard.remove_channel(*ws_channel_id);
        }
    }

    // Send the response
    let session_guard = session.lock().await;
    match response {
        Ok(response_data) => {
            let response_bytes = &response_data.payload;
            if response_bytes.is_empty() {
                return session_guard
                    .send(ServerMessage::protocol_error(request_id, "empty_response"))
                    .await;
            }

            match response_bytes[0] {
                0x00 => {
                    let value_bytes = &response_bytes[1..];
                    match crate::transcode::postcard_to_json_with_shape(value_bytes, return_shape) {
                        Ok(json_bytes) => {
                            let value: serde_json::Value = serde_json::from_slice(&json_bytes)
                                .unwrap_or(serde_json::Value::Null);
                            session_guard
                                .send(ServerMessage::success(request_id, value))
                                .await
                        }
                        Err(e) => {
                            warn!("Failed to transcode response: {}", e);
                            session_guard
                                .send(ServerMessage::protocol_error(request_id, "transcode_error"))
                                .await
                        }
                    }
                }
                0x01 => {
                    if response_bytes.len() < 2 {
                        return session_guard
                            .send(ServerMessage::protocol_error(request_id, "truncated_error"))
                            .await;
                    }
                    match response_bytes[1] {
                        0x00 => {
                            if let Some(err_shape) = error_shape {
                                let error_bytes = &response_bytes[2..];
                                match crate::transcode::postcard_to_json_with_shape(
                                    error_bytes,
                                    err_shape,
                                ) {
                                    Ok(json_bytes) => {
                                        let value: serde_json::Value =
                                            serde_json::from_slice(&json_bytes)
                                                .unwrap_or(serde_json::Value::Null);
                                        session_guard
                                            .send(ServerMessage::user_error(request_id, value))
                                            .await
                                    }
                                    Err(_) => {
                                        session_guard
                                            .send(ServerMessage::protocol_error(
                                                request_id,
                                                "transcode_error",
                                            ))
                                            .await
                                    }
                                }
                            } else {
                                session_guard
                                    .send(ServerMessage::user_error(
                                        request_id,
                                        serde_json::Value::Null,
                                    ))
                                    .await
                            }
                        }
                        0x01 => {
                            session_guard
                                .send(ServerMessage::protocol_error(request_id, "unknown_method"))
                                .await
                        }
                        0x02 => {
                            session_guard
                                .send(ServerMessage::protocol_error(request_id, "invalid_payload"))
                                .await
                        }
                        0x03 => {
                            session_guard
                                .send(ServerMessage::protocol_error(request_id, "cancelled"))
                                .await
                        }
                        _ => {
                            session_guard
                                .send(ServerMessage::protocol_error(request_id, "unknown_error"))
                                .await
                        }
                    }
                }
                _ => {
                    session_guard
                        .send(ServerMessage::protocol_error(
                            request_id,
                            "invalid_response",
                        ))
                        .await
                }
            }
        }
        Err(e) => {
            error!("Streaming call {} failed: {:?}", request_id, e);
            session_guard
                .send(ServerMessage::protocol_error(request_id, "call_failed"))
                .await
        }
    }
}

/// Handle incoming data on a channel.
///
/// r[bridge.ws.data]
async fn handle_data(
    session: Arc<tokio::sync::Mutex<WsSession>>,
    channel_id: u64,
    value: serde_json::Value,
) -> Result<(), BridgeError> {
    use roam_session::DriverMessage;

    trace!("handle_data: channel={}, value={}", channel_id, value);

    let session_guard = session.lock().await;

    let channel = session_guard.get_channel(channel_id);
    trace!(
        "handle_data: channel state = {:?}",
        channel.map(|c| c.direction)
    );

    let channel = channel
        .ok_or_else(|| BridgeError::bad_request(format!("Unknown channel: {}", channel_id)))?;

    // Verify direction (only client->server channels accept data from client)
    if channel.direction != ChannelDirection::ClientToServer {
        return Err(BridgeError::bad_request(format!(
            "Channel {} is not a client-to-server channel",
            channel_id
        )));
    }

    // Get the roam channel ID
    let roam_channel_id = channel.roam_channel_id.ok_or_else(|| {
        BridgeError::internal(format!("No roam channel ID for channel {}", channel_id))
    })?;

    // Get the element shape for transcoding
    let element_shape = channel.element_shape;

    // Get the driver_tx for sending to roam
    let driver_tx = session_guard
        .driver_tx()
        .cloned()
        .ok_or_else(|| BridgeError::internal("No driver_tx available"))?;

    // Drop the session guard before async operations
    drop(session_guard);

    // Convert JSON value to postcard using the element shape
    // Wrap value in an array for transcode (it expects array of args)
    let json_bytes = serde_json::to_vec(&[&value])
        .map_err(|e| BridgeError::bad_request(format!("Invalid value: {}", e)))?;

    let postcard_bytes = crate::transcode::json_args_to_postcard(&json_bytes, &[element_shape])?;

    // Forward to the roam connection via DriverMessage::Data
    driver_tx
        .send(DriverMessage::Data {
            conn_id: roam_wire::ConnectionId::ROOT,
            channel_id: roam_channel_id,
            payload: postcard_bytes,
        })
        .await
        .map_err(|_| BridgeError::internal("Failed to send data to roam"))?;

    Ok(())
}

/// Handle channel close.
///
/// r[bridge.ws.close]
async fn handle_close(
    session: Arc<tokio::sync::Mutex<WsSession>>,
    channel_id: u64,
) -> Result<(), BridgeError> {
    use roam_session::DriverMessage;

    let mut session_guard = session.lock().await;

    let channel = session_guard.get_channel(channel_id);

    if let Some(channel) = channel {
        // Only client->server channels can be closed by the client
        if channel.direction != ChannelDirection::ClientToServer {
            return Err(BridgeError::bad_request(format!(
                "Channel {} cannot be closed by client",
                channel_id
            )));
        }

        // Get the roam channel ID and driver_tx
        if let (Some(roam_channel_id), Some(driver_tx)) =
            (channel.roam_channel_id, session_guard.driver_tx().cloned())
        {
            // Remove the channel from tracking
            session_guard.remove_channel(channel_id);
            drop(session_guard);

            // Send Close to roam
            let _ = driver_tx
                .send(DriverMessage::Close {
                    conn_id: roam_wire::ConnectionId::ROOT,
                    channel_id: roam_channel_id,
                })
                .await;
        } else {
            session_guard.remove_channel(channel_id);
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
        debug!("Cancelled request {}", request_id);
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
