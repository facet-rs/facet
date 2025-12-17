//! Tracing integration for rapace cells.
//!
//! This module sets up tracing so that cell logs are forwarded to the host process
//! via rapace-tracing. The host can also push filter updates to cells.

use std::sync::Arc;

use rapace::RpcSession;
use rapace_tracing::{RapaceTracingLayer, TracingConfigImpl, TracingConfigServer};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::ServiceDispatch;

/// Initialize tracing for a cell, forwarding logs to the host.
///
/// Returns a TracingConfigService that should be added to the dispatcher
/// so the host can push filter updates to this cell.
pub fn init_cell_tracing(session: Arc<RpcSession>) -> TracingConfigService {
    let rt = tokio::runtime::Handle::current();

    // Create the tracing layer that forwards logs to host
    let (layer, filter) = RapaceTracingLayer::new(session, rt);

    // Initialize the tracing subscriber with our layer
    tracing_subscriber::registry().with(layer).init();

    // Create the TracingConfig server so host can push filter updates
    let config_impl = TracingConfigImpl::new(filter);
    let config_server = TracingConfigServer::new(config_impl);

    TracingConfigService::new(config_server)
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
        payload: &[u8],
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<rapace::Frame, rapace::RpcError>>
                + Send
                + 'static,
        >,
    > {
        let server = self.0.clone();
        let payload_owned = payload.to_vec();
        Box::pin(async move { server.dispatch(method_id, &payload_owned).await })
    }
}
