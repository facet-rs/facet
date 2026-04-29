use crate::{ChannelEvent, DriverEvent, RpcEvent, TransportEvent, VoxObserver};

#[derive(Debug, Clone, Copy, Default)]
pub struct TracingObserver;

impl TracingObserver {
    pub const fn new() -> Self {
        Self
    }
}

// r[impl rpc.observability.runtime]
impl VoxObserver for TracingObserver {
    fn rpc_event(&self, event: RpcEvent) {
        tracing::debug!(target: "vox::observer::rpc", ?event, "vox rpc event");
    }

    fn channel_event(&self, event: ChannelEvent) {
        tracing::debug!(target: "vox::observer::channel", ?event, "vox channel event");
    }

    fn transport_event(&self, event: TransportEvent) {
        tracing::debug!(target: "vox::observer::transport", ?event, "vox transport event");
    }

    fn driver_event(&self, event: DriverEvent) {
        tracing::debug!(target: "vox::observer::driver", ?event, "vox driver event");
    }
}
