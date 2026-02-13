//! Auditable channels for debugging queue depths.
//!
//! Provides bounded channels that register themselves in a global registry,
//! allowing applications to dump the state of all queues for diagnostics.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use tokio::sync::mpsc;

/// Global registry of all auditable channels.
static CHANNEL_REGISTRY: Mutex<Vec<Weak<dyn ChannelDiagnostic + Send + Sync>>> =
    Mutex::new(Vec::new());

/// Trait for getting diagnostic info from a channel.
pub trait ChannelDiagnostic {
    /// Name of the channel (for diagnostics).
    fn name(&self) -> &str;
    /// Maximum capacity.
    fn capacity(&self) -> usize;
    /// Current number of items in the queue (approximate).
    fn len(&self) -> usize;
    /// Returns true if the channel is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Register a channel for diagnostics.
fn register_channel(channel: Arc<dyn ChannelDiagnostic + Send + Sync>) {
    if let Ok(mut registry) = CHANNEL_REGISTRY.lock() {
        // Clean up dead weak refs while we're here
        registry.retain(|weak| weak.strong_count() > 0);
        registry.push(Arc::downgrade(&channel));
    }
}

/// Dump all registered channels to a string for diagnostics.
pub fn dump_all_channels() -> String {
    let channels: Vec<Arc<dyn ChannelDiagnostic + Send + Sync>> = {
        let Ok(registry) = CHANNEL_REGISTRY.lock() else {
            return String::from("[channel registry locked]\n");
        };
        registry.iter().filter_map(|weak| weak.upgrade()).collect()
    };

    if channels.is_empty() {
        return String::new();
    }

    let mut out = String::from("[Channels]\n");
    for ch in channels {
        let len = ch.len();
        let cap = ch.capacity();
        let pct = if cap > 0 { (len * 100) / cap } else { 0 };
        out.push_str(&format!("  {}: {}/{} ({}%)\n", ch.name(), len, cap, pct));
    }
    out
}

/// Collect all live channel diagnostic references.
pub fn collect_live_channels() -> Vec<Arc<dyn ChannelDiagnostic + Send + Sync>> {
    let Ok(registry) = CHANNEL_REGISTRY.lock() else {
        return Vec::new();
    };
    registry.iter().filter_map(|weak| weak.upgrade()).collect()
}
// ============================================================================
// Auditable MPSC Channel
// ============================================================================

/// Shared state for tracking channel depth.
struct ChannelState {
    name: String,
    capacity: usize,
    /// Approximate count - incremented on send, decremented on recv.
    /// May be slightly off due to races, but good enough for diagnostics.
    count: std::sync::atomic::AtomicUsize,
}

impl ChannelDiagnostic for ChannelState {
    fn name(&self) -> &str {
        &self.name
    }

    fn capacity(&self) -> usize {
        self.capacity
    }

    fn len(&self) -> usize {
        self.count.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Sender half of an auditable channel.
pub struct AuditableSender<T> {
    inner: mpsc::Sender<T>,
    state: Arc<ChannelState>,
}

impl<T> Clone for AuditableSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            state: self.state.clone(),
        }
    }
}

impl<T> AuditableSender<T> {
    /// Send a value, waiting if the channel is full.
    /// Prints a warning every second if blocked due to backpressure.
    pub async fn send(&self, value: T) -> Result<(), mpsc::error::SendError<T>> {
        use std::time::Duration;

        // Try to send immediately
        match self.inner.try_reserve() {
            Ok(permit) => {
                permit.send(value);
                self.state
                    .count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return Ok(());
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                return Err(mpsc::error::SendError(value));
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                // Channel is full, will need to wait
            }
        }

        // Channel is full - wait with timeout warning
        let name = self.state.name.clone();
        let cap = self.state.capacity;

        let start = std::time::Instant::now();
        let warn_interval = Duration::from_secs(1);
        let mut warned = false;

        loop {
            let len = self.state.count.load(std::sync::atomic::Ordering::Relaxed);

            match tokio::time::timeout(warn_interval, self.inner.reserve()).await {
                Ok(Ok(permit)) => {
                    if warned {
                        eprintln!(
                            "[channel] {}: backpressure cleared after {:.1}s",
                            name,
                            start.elapsed().as_secs_f64()
                        );
                    }
                    permit.send(value);
                    self.state
                        .count
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return Ok(());
                }
                Ok(Err(_)) => {
                    // Channel closed
                    return Err(mpsc::error::SendError(value));
                }
                Err(_timeout) => {
                    // Still waiting - print warning
                    warned = true;
                    eprintln!(
                        "[channel] {}: backpressure! waiting {:.1}s ({}/{})",
                        name,
                        start.elapsed().as_secs_f64(),
                        len,
                        cap
                    );
                }
            }
        }
    }

    /// Try to send without waiting.
    pub fn try_send(&self, value: T) -> Result<(), mpsc::error::TrySendError<T>> {
        self.inner.try_send(value)?;
        self.state
            .count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Check if the channel is closed.
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }
}

/// Receiver half of an auditable channel.
pub struct AuditableReceiver<T> {
    inner: mpsc::Receiver<T>,
    state: Arc<ChannelState>,
}

impl<T> AuditableReceiver<T> {
    /// Receive a value, waiting if the channel is empty.
    pub async fn recv(&mut self) -> Option<T> {
        let value = self.inner.recv().await?;
        self.state
            .count
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        Some(value)
    }

    /// Try to receive without waiting.
    pub fn try_recv(&mut self) -> Result<T, mpsc::error::TryRecvError> {
        let value = self.inner.try_recv()?;
        self.state
            .count
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        Ok(value)
    }

    /// Close the receiver.
    pub fn close(&mut self) {
        self.inner.close()
    }
}

/// Create a bounded, auditable channel.
///
/// The channel registers itself in a global registry, allowing callers to dump
/// the state of all queues via `dump_all_channels()`.
pub fn channel<T>(
    name: impl Into<String>,
    capacity: usize,
) -> (AuditableSender<T>, AuditableReceiver<T>) {
    let (tx, rx) = mpsc::channel(capacity);
    let state = Arc::new(ChannelState {
        name: name.into(),
        capacity,
        count: std::sync::atomic::AtomicUsize::new(0),
    });

    // Register for diagnostics
    register_channel(state.clone());

    (
        AuditableSender {
            inner: tx,
            state: state.clone(),
        },
        AuditableReceiver { inner: rx, state },
    )
}

// ============================================================================
// Auditable VecDeque (for pending_sends)
// ============================================================================

/// An auditable VecDeque that registers itself for diagnostics.
pub struct AuditableDeque<T> {
    inner: std::collections::VecDeque<T>,
    state: Arc<DequeState>,
}

struct DequeState {
    name: String,
    capacity: usize,
    count: std::sync::atomic::AtomicUsize,
}

impl ChannelDiagnostic for DequeState {
    fn name(&self) -> &str {
        &self.name
    }

    fn capacity(&self) -> usize {
        self.capacity
    }

    fn len(&self) -> usize {
        self.count.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl<T> AuditableDeque<T> {
    /// Create a new auditable deque with a name and soft capacity limit.
    pub fn new(name: impl Into<String>, capacity: usize) -> Self {
        let state = Arc::new(DequeState {
            name: name.into(),
            capacity,
            count: std::sync::atomic::AtomicUsize::new(0),
        });
        register_channel(state.clone());
        Self {
            inner: std::collections::VecDeque::new(),
            state,
        }
    }

    /// Push to the back.
    pub fn push_back(&mut self, value: T) {
        self.inner.push_back(value);
        self.state
            .count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Push to the front.
    pub fn push_front(&mut self, value: T) {
        self.inner.push_front(value);
        self.state
            .count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Pop from the front.
    pub fn pop_front(&mut self) -> Option<T> {
        let value = self.inner.pop_front()?;
        self.state
            .count
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        Some(value)
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Get the length.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Get the capacity.
    pub fn capacity(&self) -> usize {
        self.state.capacity
    }
}

// ============================================================================
// Auditable HashMap of Deques (for pending_sends per peer)
// ============================================================================

/// A collection of per-key deques, each auditable.
pub struct AuditableDequeMap<K, V> {
    inner: HashMap<K, AuditableDeque<V>>,
    name_prefix: String,
    capacity_per_key: usize,
}

impl<K: std::hash::Hash + Eq + std::fmt::Debug + Clone, V> AuditableDequeMap<K, V> {
    /// Create a new map with a name prefix and capacity per key.
    pub fn new(name_prefix: impl Into<String>, capacity_per_key: usize) -> Self {
        Self {
            inner: HashMap::new(),
            name_prefix: name_prefix.into(),
            capacity_per_key,
        }
    }

    /// Get a reference to a deque, if it exists.
    pub fn get(&self, key: &K) -> Option<&AuditableDeque<V>> {
        self.inner.get(key)
    }

    /// Get a mutable reference to a deque, if it exists.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut AuditableDeque<V>> {
        self.inner.get_mut(key)
    }

    /// Get or create a deque for a key.
    pub fn entry(&mut self, key: K) -> &mut AuditableDeque<V> {
        let name_prefix = &self.name_prefix;
        let capacity = self.capacity_per_key;
        self.inner
            .entry(key.clone())
            .or_insert_with(|| AuditableDeque::new(format!("{}{:?}", name_prefix, key), capacity))
    }

    /// Remove a deque.
    pub fn remove(&mut self, key: &K) -> Option<AuditableDeque<V>> {
        self.inner.remove(key)
    }

    /// Get all keys.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.inner.keys()
    }

    /// Check if a key exists and its deque is non-empty.
    pub fn has_pending(&self, key: &K) -> bool {
        self.inner.get(key).is_some_and(|q| !q.is_empty())
    }
}
