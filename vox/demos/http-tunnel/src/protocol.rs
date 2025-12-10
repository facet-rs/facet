//! TcpTunnel protocol definitions.
//!
//! The TcpTunnel service allows opening bidirectional byte tunnels over rapace.
//! After calling `open()`, both sides use the tunnel APIs on `RpcSession` to
//! send and receive chunks on the returned channel.

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
#[rapace::service]
pub trait TcpTunnel {
    /// Open a new bidirectional tunnel.
    ///
    /// Returns a handle containing the channel_id to use for data transfer.
    /// After this returns, the channel transitions from RPC mode to tunnel mode.
    async fn open(&self) -> crate::protocol::TunnelHandle;
}
