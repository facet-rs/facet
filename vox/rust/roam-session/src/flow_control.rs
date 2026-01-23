// ============================================================================
// Flow Control
// ============================================================================

use crate::ChannelId;

/// Abstraction for stream flow control mechanism.
///
/// Different transports implement credit-based flow control differently:
/// - **Stream transports** (TCP, WebSocket): explicit `Message::Credit` on the wire
/// - **SHM**: shared atomic counters in the channel table (`ChannelEntry::granted_total`)
///
/// This trait abstracts the mechanism while `ChannelRegistry` remains the source
/// of truth for stream lifecycle (routing, ordering, existence).
///
/// r[impl flow.channel.credit-based]
/// r[impl flow.channel.all-transports]
pub trait FlowControl: Send {
    /// Called when we receive data on a channel (receiver side).
    ///
    /// The implementation may grant credit back to the sender:
    /// - Stream: queue a `Message::Credit` to send
    /// - SHM: increment `ChannelEntry::granted_total` atomically
    ///
    /// r[impl flow.channel.credit-grant]
    fn on_data_received(&mut self, channel_id: ChannelId, bytes: u32);

    /// Wait until we have enough credit to send `bytes` on a channel (sender side).
    ///
    /// - Stream: check `ChannelRegistry::outgoing_credit`, wait on notify if insufficient
    /// - SHM: poll/futex wait on `granted_total - sent_total >= bytes`
    ///
    /// Returns `Ok(())` when credit is available, `Err` if the channel is closed/invalid.
    ///
    /// r[impl flow.channel.zero-credit]
    fn wait_for_send_credit(
        &mut self,
        channel_id: ChannelId,
        bytes: u32,
    ) -> impl std::future::Future<Output = std::io::Result<()>> + Send;

    /// Consume credit after sending data (sender side).
    ///
    /// Called after successfully sending `bytes` on a channel.
    /// - Stream: decrement `ChannelRegistry::outgoing_credit`
    /// - SHM: increment local `sent_total`
    ///
    /// r[impl flow.channel.credit-consume]
    fn consume_send_credit(&mut self, channel_id: ChannelId, bytes: u32);
}

/// No-op flow control for infinite credit mode.
///
/// r[impl flow.channel.infinite-credit]
///
/// Used when flow control is disabled or not yet implemented.
/// All operations succeed immediately without tracking.
#[derive(Debug, Clone, Copy, Default)]
pub struct InfiniteCredit;

impl FlowControl for InfiniteCredit {
    fn on_data_received(&mut self, _channel_id: ChannelId, _bytes: u32) {
        // No credit tracking needed
    }

    async fn wait_for_send_credit(
        &mut self,
        _channel_id: ChannelId,
        _bytes: u32,
    ) -> std::io::Result<()> {
        // Infinite credit - always available
        Ok(())
    }

    fn consume_send_credit(&mut self, _channel_id: ChannelId, _bytes: u32) {
        // No credit tracking needed
    }
}
