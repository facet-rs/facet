//! WebSocket session state management.
//!
//! Manages multiplexed RPC calls and channels over a single WebSocket connection.

use std::collections::HashMap;
use std::sync::Arc;

use facet_core::Shape;
use roam_session::DriverMessage;

use crate::{BridgeError, BridgeService};

use super::messages::ServerMessage;

/// State for a single WebSocket connection.
///
/// r[bridge.ws.multiplexing] - Supports multiple concurrent calls.
pub struct WsSession {
    /// Registered services.
    services: Arc<HashMap<String, Arc<dyn BridgeService>>>,
    /// In-flight calls, keyed by client request ID.
    calls: HashMap<u64, CallState>,
    /// Active channels for streaming, keyed by channel ID.
    channels: HashMap<u64, ChannelState>,
    /// Sender for outgoing messages to the WebSocket.
    outgoing_tx: peeps_sync::Sender<ServerMessage>,
    /// Sender for messages to the roam connection (for streaming).
    driver_tx: Option<peeps_sync::Sender<DriverMessage>>,
    /// Initial credit for new channels (bytes).
    initial_credit: u64,
}

/// State for an in-flight RPC call.
struct CallState {
    /// Service being called.
    #[allow(dead_code)]
    service_name: String,
    /// Method being called.
    #[allow(dead_code)]
    method_name: String,
    /// Cancellation token (for future use).
    #[allow(dead_code)]
    cancelled: bool,
}

/// Direction of a channel from the bridge's perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelDirection {
    /// Client sends to bridge (Tx<T> from client's POV).
    ClientToServer,
    /// Bridge sends to client (Rx<T> from client's POV).
    ServerToClient,
}

/// State for an active channel.
#[allow(dead_code)]
pub struct ChannelState {
    /// Associated request ID.
    pub request_id: u64,
    /// Direction of the channel.
    pub direction: ChannelDirection,
    /// Element type shape for transcoding.
    pub element_shape: &'static Shape,
    /// Sender for Data messages to the roam connection (ClientToServer channels).
    pub roam_tx: Option<peeps_sync::Sender<Vec<u8>>>,
    /// The corresponding roam channel ID (for forwarding).
    pub roam_channel_id: Option<u64>,
    /// Outstanding credit (bytes) for this channel.
    pub credit: u64,
}

#[allow(dead_code)]
impl WsSession {
    /// Create a new session.
    pub fn new(
        services: Arc<HashMap<String, Arc<dyn BridgeService>>>,
        outgoing_tx: peeps_sync::Sender<ServerMessage>,
    ) -> Self {
        Self {
            services,
            calls: HashMap::new(),
            channels: HashMap::new(),
            outgoing_tx,
            driver_tx: None,
            initial_credit: 65536, // Default: 64KB initial credit
        }
    }

    /// Set the driver_tx for sending messages to the roam connection.
    pub fn set_driver_tx(&mut self, driver_tx: peeps_sync::Sender<DriverMessage>) {
        self.driver_tx = Some(driver_tx);
    }

    /// Get the driver_tx for sending messages to the roam connection.
    pub fn driver_tx(&self) -> Option<&peeps_sync::Sender<DriverMessage>> {
        self.driver_tx.as_ref()
    }

    /// Get the services map.
    pub fn services(&self) -> &Arc<HashMap<String, Arc<dyn BridgeService>>> {
        &self.services
    }

    /// Get the outgoing message sender.
    pub fn outgoing_tx(&self) -> &peeps_sync::Sender<ServerMessage> {
        &self.outgoing_tx
    }

    /// Register an in-flight call.
    pub fn register_call(&mut self, request_id: u64, service_name: String, method_name: String) {
        self.calls.insert(
            request_id,
            CallState {
                service_name,
                method_name,
                cancelled: false,
            },
        );
    }

    /// Complete a call (remove from tracking).
    pub fn complete_call(&mut self, request_id: u64) {
        self.calls.remove(&request_id);
    }

    /// Check if a call exists.
    pub fn has_call(&self, request_id: u64) -> bool {
        self.calls.contains_key(&request_id)
    }

    /// Cancel a call.
    pub fn cancel_call(&mut self, request_id: u64) -> bool {
        if let Some(call) = self.calls.get_mut(&request_id) {
            call.cancelled = true;
            true
        } else {
            false
        }
    }

    /// Register a channel for streaming.
    pub fn register_channel(
        &mut self,
        channel_id: u64,
        request_id: u64,
        direction: ChannelDirection,
        element_shape: &'static Shape,
        roam_tx: Option<peeps_sync::Sender<Vec<u8>>>,
    ) {
        self.channels.insert(
            channel_id,
            ChannelState {
                request_id,
                direction,
                element_shape,
                roam_tx,
                roam_channel_id: None,
                credit: self.initial_credit,
            },
        );
    }

    /// Set the roam channel ID for a WebSocket channel.
    pub fn set_roam_channel_id(&mut self, ws_channel_id: u64, roam_channel_id: u64) {
        if let Some(channel) = self.channels.get_mut(&ws_channel_id) {
            channel.roam_channel_id = Some(roam_channel_id);
        }
    }

    /// Get the roam channel ID for a WebSocket channel.
    pub fn get_roam_channel_id(&self, ws_channel_id: u64) -> Option<u64> {
        self.channels
            .get(&ws_channel_id)
            .and_then(|c| c.roam_channel_id)
    }

    /// Get a channel state.
    pub fn get_channel(&self, channel_id: u64) -> Option<&ChannelState> {
        self.channels.get(&channel_id)
    }

    /// Get a mutable channel state.
    pub fn get_channel_mut(&mut self, channel_id: u64) -> Option<&mut ChannelState> {
        self.channels.get_mut(&channel_id)
    }

    /// Remove a channel.
    pub fn remove_channel(&mut self, channel_id: u64) -> Option<ChannelState> {
        self.channels.remove(&channel_id)
    }

    /// Check if a channel exists.
    pub fn has_channel(&self, channel_id: u64) -> bool {
        self.channels.contains_key(&channel_id)
    }

    /// Add credit to a channel.
    pub fn add_credit(&mut self, channel_id: u64, bytes: u64) {
        if let Some(channel) = self.channels.get_mut(&channel_id) {
            channel.credit = channel.credit.saturating_add(bytes);
        }
    }

    /// Consume credit from a channel, returns true if successful.
    pub fn consume_credit(&mut self, channel_id: u64, bytes: u64) -> bool {
        if let Some(channel) = self.channels.get_mut(&channel_id) {
            if channel.credit >= bytes {
                channel.credit -= bytes;
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Look up a service by name.
    pub fn get_service(&self, name: &str) -> Result<Arc<dyn BridgeService>, BridgeError> {
        self.services
            .get(name)
            .cloned()
            .ok_or_else(|| BridgeError::bad_request(format!("Unknown service: {}", name)))
    }

    /// Send a message to the client.
    pub async fn send(&self, msg: ServerMessage) -> Result<(), BridgeError> {
        self.outgoing_tx
            .send(msg)
            .await
            .map_err(|_| BridgeError::internal("WebSocket send channel closed"))
    }
}
