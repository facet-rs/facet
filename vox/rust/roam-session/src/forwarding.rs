//! Transparent RPC proxying/forwarding.
//!
//! This module provides dispatchers that forward requests to an upstream connection
//! without needing to know the service schema.

use std::sync::Arc;

use crate::{
    ChannelRegistry, ConnectionHandle, Context, DriverMessage, IncomingChannelMessage,
    ServiceDispatcher, TransportError,
};

// ============================================================================
// ForwardingDispatcher - Transparent RPC Proxy
// ============================================================================

/// A dispatcher that forwards all requests to an upstream connection.
///
/// This enables transparent proxying without knowing the service schema.
/// Channel IDs are remapped automatically: the proxy allocates new channel IDs
/// for the upstream connection and maintains bidirectional forwarding.
///
/// # Example
///
/// ```ignore
/// use roam_session::{ForwardingDispatcher, ConnectionHandle};
///
/// // Upstream connection to the actual service
/// let upstream: ConnectionHandle = /* ... */;
///
/// // Create a forwarding dispatcher
/// let proxy = ForwardingDispatcher::new(upstream);
///
/// // Use with accept() - all calls will be forwarded to upstream
/// let (handle, driver) = accept(stream, config, proxy).await?;
/// ```
pub struct ForwardingDispatcher {
    upstream: ConnectionHandle,
}

impl ForwardingDispatcher {
    /// Create a new forwarding dispatcher that proxies to the upstream connection.
    pub fn new(upstream: ConnectionHandle) -> Self {
        Self { upstream }
    }
}

impl Clone for ForwardingDispatcher {
    fn clone(&self) -> Self {
        Self {
            upstream: self.upstream.clone(),
        }
    }
}

impl ServiceDispatcher for ForwardingDispatcher {
    /// Returns empty - this dispatcher accepts all method IDs.
    fn method_ids(&self) -> Vec<u64> {
        vec![]
    }

    fn dispatch(
        &self,
        cx: Context,
        payload: Vec<u8>,
        registry: &mut ChannelRegistry,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        let task_tx = registry.driver_tx();
        let upstream = self.upstream.clone();
        let conn_id = cx.conn_id;
        let method_id = cx.method_id.raw();
        let request_id = cx.request_id.raw();
        let channels = cx.channels.clone();

        if channels.is_empty() {
            // Unary call - but response may contain Rx<T> channels
            // We need to set up forwarding for any response channels.
            //
            // IMPORTANT: Upstream and downstream use different channel ID spaces.
            // The upstream channel IDs must be remapped to downstream channel IDs.
            let downstream_channel_ids = registry.response_channel_ids();

            Box::pin(async move {
                let response = upstream
                    .call_raw_with_channels(method_id, vec![], payload, None)
                    .await;

                let (response_payload, upstream_response_channels) = match response {
                    Ok(data) => (data.payload, data.channels),
                    Err(TransportError::Encode(_)) => {
                        // Should not happen for raw call
                        (vec![1, 2], Vec::new()) // Err(InvalidPayload)
                    }
                    Err(TransportError::ConnectionClosed) | Err(TransportError::DriverGone) => {
                        // Connection to upstream failed - return Cancelled
                        (vec![1, 3], Vec::new()) // Err(Cancelled)
                    }
                };

                // If response has channels (e.g., method returns Rx<T>),
                // set up forwarding for Data from upstream to downstream.
                // We allocate new downstream channel IDs and remap when forwarding.
                let mut downstream_channels = Vec::new();
                if !upstream_response_channels.is_empty() {
                    debug!(
                        upstream_channels = ?upstream_response_channels,
                        "ForwardingDispatcher: setting up response channel forwarding"
                    );
                    for &upstream_id in &upstream_response_channels {
                        // Allocate a downstream channel ID
                        let downstream_id = downstream_channel_ids.next();
                        downstream_channels.push(downstream_id);

                        debug!(
                            upstream_id,
                            downstream_id, "ForwardingDispatcher: mapping channel IDs"
                        );

                        // Set up forwarding: upstream → downstream
                        let (tx, mut rx) =
                            crate::runtime::channel::<IncomingChannelMessage>("fwd_resp_chan", 64);
                        upstream.register_incoming(upstream_id, tx);

                        let task_tx_clone = task_tx.clone();
                        crate::runtime::spawn("roam_fwd_response_relay", async move {
                            debug!(
                                upstream_id,
                                downstream_id, "ForwardingDispatcher: forwarding task started"
                            );
                            while let Some(msg) = rx.recv().await {
                                match msg {
                                    IncomingChannelMessage::Data(data) => {
                                        debug!(
                                            upstream_id,
                                            downstream_id,
                                            data_len = data.len(),
                                            "ForwardingDispatcher: forwarding data"
                                        );
                                        let _ = task_tx_clone
                                            .send(DriverMessage::Data {
                                                conn_id,
                                                channel_id: downstream_id,
                                                payload: data,
                                            })
                                            .await;
                                    }
                                    IncomingChannelMessage::Close => {
                                        let _ = task_tx_clone
                                            .send(DriverMessage::Close {
                                                conn_id,
                                                channel_id: downstream_id,
                                            })
                                            .await;
                                        break;
                                    }
                                }
                            }
                        });
                    }
                }

                let _ = task_tx
                    .send(DriverMessage::Response {
                        conn_id,
                        request_id,
                        channels: downstream_channels,
                        payload: response_payload,
                    })
                    .await;
            })
        } else {
            // Streaming call - set up bidirectional channel forwarding
            //
            // IMPORTANT: We must send the upstream Request BEFORE any Data is
            // forwarded, otherwise the backend will reject Data for unknown channels.
            //
            // Strategy:
            // 1. Register incoming handlers synchronously (buffers Data in mpsc channels)
            // 2. In the async block: send Request first, then spawn forwarding tasks
            //    (spawning AFTER Request is sent is safe - ordering is established)

            // Allocate upstream channel IDs and set up buffering channels
            let mut upstream_channels = Vec::with_capacity(channels.len());
            let mut ds_to_us_rxs = Vec::with_capacity(channels.len());
            let mut us_to_ds_rxs = Vec::with_capacity(channels.len());
            let mut channel_map = Vec::with_capacity(channels.len());

            let upstream_task_tx = upstream.driver_tx();

            for &downstream_id in &channels {
                let upstream_id = upstream.alloc_channel_id();
                upstream_channels.push(upstream_id);
                channel_map.push((downstream_id, upstream_id));

                // Buffer for downstream → upstream (client sends Data)
                let (ds_to_us_tx, ds_to_us_rx) = crate::runtime::channel("fwd_ds_to_us", 64);
                registry.register_incoming(downstream_id, ds_to_us_tx);
                ds_to_us_rxs.push(ds_to_us_rx);

                // Buffer for upstream → downstream (server sends Data)
                let (us_to_ds_tx, us_to_ds_rx) = crate::runtime::channel("fwd_us_to_ds", 64);
                upstream.register_incoming(upstream_id, us_to_ds_tx);
                us_to_ds_rxs.push(us_to_ds_rx);
            }

            // Everything below runs in the async block
            Box::pin(async move {
                // Send the upstream Request - this queues the Request command
                // which will be sent before any Data we forward
                let response_future =
                    upstream.call_raw_with_channels(method_id, upstream_channels, payload, None);

                // Now spawn forwarding tasks - safe because Request is queued first
                // and command_tx/task_tx are processed in order by the driver
                let upstream_conn_id = upstream.conn_id();
                for (i, mut rx) in ds_to_us_rxs.into_iter().enumerate() {
                    let upstream_id = channel_map[i].1;
                    let upstream_task_tx = upstream_task_tx.clone();
                    crate::runtime::spawn("roam_fwd_ds_to_us_relay", async move {
                        while let Some(msg) = rx.recv().await {
                            match msg {
                                IncomingChannelMessage::Data(data) => {
                                    let _ = upstream_task_tx
                                        .send(DriverMessage::Data {
                                            conn_id: upstream_conn_id,
                                            channel_id: upstream_id,
                                            payload: data,
                                        })
                                        .await;
                                }
                                IncomingChannelMessage::Close => {
                                    let _ = upstream_task_tx
                                        .send(DriverMessage::Close {
                                            conn_id: upstream_conn_id,
                                            channel_id: upstream_id,
                                        })
                                        .await;
                                    break;
                                }
                            }
                        }
                    });
                }

                for (i, mut rx) in us_to_ds_rxs.into_iter().enumerate() {
                    let downstream_id = channel_map[i].0;
                    let task_tx = task_tx.clone();
                    crate::runtime::spawn("roam_fwd_us_to_ds_relay", async move {
                        while let Some(msg) = rx.recv().await {
                            match msg {
                                IncomingChannelMessage::Data(data) => {
                                    let _ = task_tx
                                        .send(DriverMessage::Data {
                                            conn_id,
                                            channel_id: downstream_id,
                                            payload: data,
                                        })
                                        .await;
                                }
                                IncomingChannelMessage::Close => {
                                    let _ = task_tx
                                        .send(DriverMessage::Close {
                                            conn_id,
                                            channel_id: downstream_id,
                                        })
                                        .await;
                                    break;
                                }
                            }
                        }
                    });
                }

                // Wait for upstream response
                let response = response_future.await;

                let (response_payload, upstream_response_channels) = match response {
                    Ok(data) => (data.payload, data.channels),
                    Err(TransportError::Encode(_)) => {
                        (vec![1, 2], Vec::new()) // Err(InvalidPayload)
                    }
                    Err(TransportError::ConnectionClosed) | Err(TransportError::DriverGone) => {
                        (vec![1, 3], Vec::new()) // Err(Cancelled)
                    }
                };

                // Map upstream response channels back to downstream channel IDs.
                // The downstream client allocated the original IDs and expects them
                // in the Response, not the upstream IDs we allocated for forwarding.
                let downstream_response_channels: Vec<u64> = upstream_response_channels
                    .iter()
                    .filter_map(|&upstream_id| {
                        channel_map
                            .iter()
                            .find(|(_, us)| *us == upstream_id)
                            .map(|(ds, _)| *ds)
                    })
                    .collect();

                let _ = task_tx
                    .send(DriverMessage::Response {
                        conn_id,
                        request_id,
                        channels: downstream_response_channels,
                        payload: response_payload,
                    })
                    .await;
            })
        }
    }
}

// ============================================================================
// LateBoundForwarder - Forwarding with Deferred Handle Binding
// ============================================================================

/// A handle that can be set once after creation.
///
/// This solves the chicken-and-egg problem in bidirectional proxying where:
/// 1. You need to pass a dispatcher to `connect()` for reverse-direction calls
/// 2. But the dispatcher needs a handle that's only available after `accept_framed()`
///
/// # Example
///
/// ```ignore
/// // Create the late-bound handle (empty initially)
/// let late_bound = LateBoundHandle::new();
///
/// // Pass a forwarder using this handle to connect()
/// let virtual_conn = handle.connect(
///     metadata,
///     Some(Box::new(LateBoundForwarder::new(late_bound.clone()))),
/// ).await?;
///
/// // Accept the other connection to get its handle
/// let (browser_handle, driver) = accept_framed(transport, config, dispatcher).await?;
///
/// // NOW bind the handle - any incoming calls will be forwarded
/// late_bound.set(browser_handle);
/// ```
#[derive(Clone)]
pub struct LateBoundHandle {
    inner: Arc<std::sync::OnceLock<ConnectionHandle>>,
}

impl LateBoundHandle {
    /// Create a new unbound handle.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(std::sync::OnceLock::new()),
        }
    }

    /// Bind the handle to a connection. Can only be called once.
    ///
    /// # Panics
    ///
    /// Panics if called more than once.
    pub fn set(&self, handle: ConnectionHandle) {
        if self.inner.set(handle).is_err() {
            panic!("LateBoundHandle::set called more than once");
        }
    }

    /// Try to get the bound handle, if set.
    pub fn get(&self) -> Option<&ConnectionHandle> {
        self.inner.get()
    }
}

impl Default for LateBoundHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// A dispatcher that forwards all requests to a late-bound upstream connection.
///
/// Like [`ForwardingDispatcher`], but the upstream handle is provided after creation
/// via [`LateBoundHandle::set`]. This enables bidirectional proxying scenarios.
///
/// If a request arrives before the handle is bound, it returns `Cancelled`.
///
/// # Example
///
/// ```ignore
/// // Create late-bound handle and forwarder
/// let late_bound = LateBoundHandle::new();
/// let forwarder = LateBoundForwarder::new(late_bound.clone());
///
/// // Use forwarder with connect() for reverse-direction calls
/// let virtual_conn = handle.connect(metadata, Some(Box::new(forwarder))).await?;
///
/// // Later, bind the actual handle
/// let (browser_handle, driver) = accept_framed(...).await?;
/// late_bound.set(browser_handle);
/// ```
pub struct LateBoundForwarder {
    upstream: LateBoundHandle,
}

impl LateBoundForwarder {
    /// Create a new late-bound forwarding dispatcher.
    pub fn new(upstream: LateBoundHandle) -> Self {
        Self { upstream }
    }
}

impl Clone for LateBoundForwarder {
    fn clone(&self) -> Self {
        Self {
            upstream: self.upstream.clone(),
        }
    }
}

impl ServiceDispatcher for LateBoundForwarder {
    fn method_ids(&self) -> Vec<u64> {
        vec![]
    }

    fn dispatch(
        &self,
        cx: Context,
        payload: Vec<u8>,
        registry: &mut ChannelRegistry,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        let task_tx = registry.driver_tx();
        let conn_id = cx.conn_id;
        let request_id = cx.request_id.raw();

        // Try to get the upstream handle
        let Some(upstream) = self.upstream.get().cloned() else {
            // Handle not bound yet - return Cancelled
            debug!(
                method_id = cx.method_id.raw(),
                "LateBoundForwarder: upstream not bound, returning Cancelled"
            );
            return Box::pin(async move {
                let _ = task_tx
                    .send(DriverMessage::Response {
                        conn_id,
                        request_id,
                        channels: vec![],
                        payload: vec![1, 3], // Err(Cancelled)
                    })
                    .await;
            });
        };

        // Delegate to ForwardingDispatcher now that we have the handle
        ForwardingDispatcher::new(upstream).dispatch(cx, payload, registry)
    }
}
