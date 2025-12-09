//! TcpTunnel protocol definitions.
//!
//! The TcpTunnel service allows opening bidirectional byte tunnels over rapace.
//! After calling `open()`, both sides use the tunnel APIs on `RpcSession` to
//! send and receive chunks on the returned channel.

use std::sync::Arc;

use rapace_core::{RpcError, Transport};
use rapace_testkit::RpcSession;

// Required by the macro
#[allow(unused)]
use rapace_registry;

/// Response from opening a tunnel.
///
/// Contains the channel ID that should be used for bidirectional streaming.
/// Both sides register this channel as a tunnel and use `send_chunk()`/`recv()`.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct TunnelHandle {
    /// The channel ID to use for tunnel data.
    /// This is the same channel used for the `open()` RPC response,
    /// but after the response both sides switch to tunnel mode on this channel.
    pub channel_id: u32,
}

/// Service for opening bidirectional TCP tunnels.
///
/// The workflow:
/// 1. Client calls `open()` to get a `TunnelHandle` with a channel_id
/// 2. Both client and server register a tunnel on that channel_id
/// 3. Both sides use `session.send_chunk()` and the tunnel receiver for data
/// 4. When done, either side calls `session.close_tunnel()` to send EOS
#[allow(async_fn_in_trait)]
#[rapace_macros::service]
pub trait TcpTunnel {
    /// Open a new bidirectional tunnel.
    ///
    /// Returns a handle containing the channel_id to use for data transfer.
    /// After this returns, the channel transitions from RPC mode to tunnel mode.
    async fn open(&self) -> crate::protocol::TunnelHandle;
}

/// RpcSession-based client for TcpTunnel service.
///
/// This wraps an RpcSession and provides typed access to TcpTunnel methods.
/// Use this when you need access to the session for tunnel APIs.
pub struct TcpTunnelRpcClient<T: Transport + Send + Sync + 'static> {
    session: Arc<RpcSession<T>>,
}

impl<T: Transport + Send + Sync + 'static> TcpTunnelRpcClient<T> {
    /// Create a new client wrapping an RpcSession.
    pub fn new(session: Arc<RpcSession<T>>) -> Self {
        Self { session }
    }

    /// Get a reference to the underlying session.
    ///
    /// Use this to access tunnel APIs like `register_tunnel()` and `send_chunk()`.
    pub fn session(&self) -> &Arc<RpcSession<T>> {
        &self.session
    }

    /// Open a new bidirectional tunnel.
    ///
    /// Returns a handle containing the channel_id. After this returns,
    /// use `session().register_tunnel(channel_id)` to start receiving chunks.
    pub async fn open(&self) -> Result<TunnelHandle, RpcError> {
        let channel_id = self.session.next_channel_id();

        // Encode empty args (open takes no args)
        let payload = facet_postcard::to_vec(&()).map_err(|e| RpcError::Status {
            code: rapace_core::ErrorCode::Internal,
            message: format!("encode error: {:?}", e),
        })?;

        // method_id 1 = open (TcpTunnel's first method)
        let response = self.session.call(channel_id, 1, payload).await?;

        // Check for error
        if response.flags.contains(rapace_core::FrameFlags::ERROR) {
            return Err(rapace_testkit::parse_error_payload(&response.payload));
        }

        // Decode response
        let result: TunnelHandle =
            facet_postcard::from_bytes(&response.payload).map_err(|e| RpcError::Status {
                code: rapace_core::ErrorCode::Internal,
                message: format!("decode error: {:?}", e),
            })?;

        Ok(result)
    }
}
