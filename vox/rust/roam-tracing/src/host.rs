//! Host-side tracing collector.
//!
//! Manages tracing subscriptions for multiple cells and provides
//! a unified stream of tagged records.

use std::collections::HashMap;

use roam::session::ConnectionHandle;
use tokio::sync::mpsc;

use crate::record::TracingRecord;
use crate::service::{CellTracingClient, TracingConfig};

/// Identifies a peer/cell.
pub type PeerId = u64;

/// A tracing record tagged with its source peer.
#[derive(Debug, Clone)]
pub struct TaggedRecord {
    /// The peer ID that emitted this record.
    pub peer_id: PeerId,
    /// Optional human-readable peer name.
    pub peer_name: Option<String>,
    /// The tracing record.
    pub record: TracingRecord,
}

/// Error registering a peer for tracing.
#[derive(Debug)]
pub enum RegisterError {
    /// RPC call failed.
    Call(String),
}

impl std::fmt::Display for RegisterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegisterError::Call(msg) => write!(f, "RPC call failed: {msg}"),
        }
    }
}

impl std::error::Error for RegisterError {}

/// Error configuring a peer's tracing.
#[derive(Debug)]
pub enum ConfigureError {
    /// Peer not found.
    PeerNotFound,
    /// RPC call failed.
    Call(String),
}

impl std::fmt::Display for ConfigureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigureError::PeerNotFound => write!(f, "peer not found"),
            ConfigureError::Call(msg) => write!(f, "RPC call failed: {msg}"),
        }
    }
}

impl std::error::Error for ConfigureError {}

/// State for a registered peer.
struct PeerState {
    #[allow(dead_code)]
    name: Option<String>,
    handle: ConnectionHandle,
    /// Dropping this signals the receiver task to stop.
    _cancel: tokio::sync::oneshot::Sender<()>,
}

/// Host-side tracing collector.
///
/// Manages tracing subscriptions for multiple cells and provides
/// a unified stream of tagged records.
///
/// # Example
///
/// ```ignore
/// use roam_tracing::{TracingHost, TracingConfig};
///
/// // Create the host collector
/// let mut tracing_host = TracingHost::new(4096);
///
/// // Take the receiver (do this once)
/// let mut records = tracing_host.take_receiver().unwrap();
///
/// // When spawning a cell:
/// tracing_host.register_peer(peer_id, Some("my-cell".into()), &handle).await?;
///
/// // Consume records
/// while let Some(tagged) = records.recv().await {
///     println!("[{}] {:?}", tagged.peer_name.unwrap_or_default(), tagged.record);
/// }
/// ```
pub struct TracingHost {
    /// Channel for receiving tagged records from all peers.
    record_tx: mpsc::Sender<TaggedRecord>,
    /// Receiver end (taken by consumer).
    record_rx: Option<mpsc::Receiver<TaggedRecord>>,
    /// Active peer subscriptions.
    peers: HashMap<PeerId, PeerState>,
    /// Default config for new peers.
    default_config: TracingConfig,
}

impl TracingHost {
    /// Create a new tracing host with the given buffer size.
    ///
    /// `buffer_size` is the capacity of the aggregated record channel.
    /// If the consumer is slow, newest records are dropped (backpressure).
    pub fn new(buffer_size: usize) -> Self {
        let (record_tx, record_rx) = mpsc::channel(buffer_size);
        Self {
            record_tx,
            record_rx: Some(record_rx),
            peers: HashMap::new(),
            default_config: TracingConfig::default(),
        }
    }

    /// Set the default configuration for new peers.
    pub fn set_default_config(&mut self, config: TracingConfig) {
        self.default_config = config;
    }

    /// Get the default configuration.
    pub fn default_config(&self) -> &TracingConfig {
        &self.default_config
    }

    /// Take the record receiver.
    ///
    /// Call this once to get the stream of tagged records from all peers.
    /// Returns `None` if already taken.
    pub fn take_receiver(&mut self) -> Option<mpsc::Receiver<TaggedRecord>> {
        self.record_rx.take()
    }

    /// Register a peer and establish tracing subscription.
    ///
    /// This calls the cell's `CellTracing::subscribe` method to establish
    /// the tracing stream, then spawns a task to forward records.
    ///
    /// # Arguments
    ///
    /// * `peer_id` - Unique identifier for this peer
    /// * `peer_name` - Optional human-readable name
    /// * `handle` - Connection handle to the cell
    pub async fn register_peer(
        &mut self,
        peer_id: PeerId,
        peer_name: Option<String>,
        handle: &ConnectionHandle,
    ) -> Result<(), RegisterError> {
        let client = CellTracingClient::new(handle.clone());

        // Create channel for cell to send records
        let (tx, rx) = roam::channel::<TracingRecord>();

        // Call subscribe - cell keeps the Tx, we keep the Rx
        // The inner Result has Never as error type, so unwrap is safe.
        client
            .subscribe(tx)
            .await
            .map_err(|e| RegisterError::Call(format!("{e:?}")))?
            .unwrap();

        // Optionally configure with defaults
        let _ = client.configure(self.default_config.clone()).await;

        // Spawn task to forward records with peer tagging
        let record_tx = self.record_tx.clone();
        let peer_name_clone = peer_name.clone();
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            let mut rx = rx;
            loop {
                tokio::select! {
                    biased;
                    _ = &mut cancel_rx => break,
                    result = rx.recv() => {
                        match result {
                            Ok(Some(record)) => {
                                let tagged = TaggedRecord {
                                    peer_id,
                                    peer_name: peer_name_clone.clone(),
                                    record,
                                };
                                // If host channel is full, drop newest (backpressure)
                                let _ = record_tx.try_send(tagged);
                            }
                            Ok(None) => break, // Stream closed
                            Err(_) => break,   // Deserialize error
                        }
                    }
                }
            }
        });

        self.peers.insert(
            peer_id,
            PeerState {
                name: peer_name,
                handle: handle.clone(),
                _cancel: cancel_tx,
            },
        );

        Ok(())
    }

    /// Unregister a peer (stops receiving records).
    ///
    /// Returns `true` if the peer was found and removed.
    pub fn unregister_peer(&mut self, peer_id: PeerId) -> bool {
        // Dropping PeerState drops cancel_tx, which signals the task to stop
        self.peers.remove(&peer_id).is_some()
    }

    /// Update configuration for a specific peer.
    pub async fn configure_peer(
        &self,
        peer_id: PeerId,
        config: TracingConfig,
    ) -> Result<(), ConfigureError> {
        let peer = self
            .peers
            .get(&peer_id)
            .ok_or(ConfigureError::PeerNotFound)?;
        let client = CellTracingClient::new(peer.handle.clone());
        // The inner Result has Never as error type, so we can ignore it.
        let _ = client
            .configure(config)
            .await
            .map_err(|e| ConfigureError::Call(format!("{e:?}")))?;
        Ok(())
    }

    /// Returns the number of registered peers.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Check if a peer is registered.
    pub fn has_peer(&self, peer_id: PeerId) -> bool {
        self.peers.contains_key(&peer_id)
    }
}
