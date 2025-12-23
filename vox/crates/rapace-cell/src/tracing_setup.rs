//! Tracing integration for rapace cells.
//!
//! This module sets up tracing so that cell logs are forwarded to the host process
//! via rapace-tracing. The host can also push filter updates to cells.

use std::sync::Arc;

use rapace::RpcSession;
use rapace_tracing::{RapaceTracingLayer, SharedFilter, TracingConfigImpl, TracingConfigServer};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::ServiceDispatch;

/// Create a shared filter + TracingConfig service without installing the tracing layer.
///
/// This lets cells expose the TracingConfig RPC early (so the host can push filters)
/// without emitting any tracing events until the cell decides to install the forwarding layer.
pub fn create_tracing_config_service() -> (SharedFilter, TracingConfigService) {
    let filter = SharedFilter::new();
    let config_impl = TracingConfigImpl::new(filter.clone());
    let config_server = TracingConfigServer::new(config_impl);
    (filter, TracingConfigService::new(config_server))
}

/// Install the tracing layer that forwards spans/events to the host, using an existing filter.
pub fn install_tracing_layer(session: Arc<RpcSession>, filter: SharedFilter) {
    let rt = tokio::runtime::Handle::current();
    let layer = RapaceTracingLayer::with_filter(session, rt, filter);
    tracing_subscriber::registry().with(layer).init();
}

/// Initialize tracing for a cell, forwarding logs to the host.
///
/// Returns a TracingConfigService that should be added to the dispatcher
/// so the host can push filter updates to this cell.
pub fn init_cell_tracing(session: Arc<RpcSession>) -> TracingConfigService {
    let (filter, service) = create_tracing_config_service();
    install_tracing_layer(session, filter);
    service
}

/// Wrapper around TracingConfigServer that implements ServiceDispatch.
pub struct TracingConfigService(Arc<TracingConfigServer<TracingConfigImpl>>);

impl TracingConfigService {
    pub fn new(server: TracingConfigServer<TracingConfigImpl>) -> Self {
        Self(Arc::new(server))
    }
}

impl ServiceDispatch for TracingConfigService {
    fn dispatch(
        &self,
        method_id: u32,
        frame: &rapace::Frame,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<rapace::Frame, rapace::RpcError>>
                + Send
                + 'static,
        >,
    > {
        let server = self.0.clone();
        // Create a new frame with the payload copied - this is necessary because
        // the SHM guard cannot be cloned, and the async task needs to own the data
        // since it outlives the request scope.
        //
        // Performance note: Tracing payloads are typically small (log messages, span
        // metadata), so this copy is acceptable. For large payloads, consider using
        // a different tracing strategy.
        let desc = frame.desc;
        let payload = rapace::rapace_core::Payload::Owned(frame.payload_bytes().to_vec());
        let frame_owned = rapace::Frame { desc, payload };
        Box::pin(async move { server.dispatch(method_id, &frame_owned).await })
    }
}
