//! WASM runtime implementation.
//!
//! Uses browser-compatible primitives:
//! - `wasm_bindgen_futures::spawn_local` for spawning
//! - `async-channel` for mpsc channels (has `send(&self)` like tokio)
//! - `futures_channel::oneshot` for oneshot channels
//! - `gloo_timers` for timeout/sleep

use std::future::Future;
use std::time::Duration;

// For oneshot, use futures-channel (async-channel doesn't have oneshot)
pub use futures_channel::oneshot::Sender as OneshotSender;

/// Wrapper around futures_channel oneshot Receiver to match peeps's API
/// (which requires `.recv().await` instead of just `.await`).
pub struct OneshotReceiver<T>(futures_channel::oneshot::Receiver<T>);

impl<T> OneshotReceiver<T> {
    pub async fn recv(self) -> Result<T, futures_channel::oneshot::Canceled> {
        self.0.await
    }

    pub fn try_recv(&mut self) -> Result<Option<T>, futures_channel::oneshot::Canceled> {
        self.0.try_recv()
    }
}

// Wrapper around std::sync::Mutex that accepts a name for API compatibility
// with peeps::Mutex on native. The name is ignored on wasm.
pub struct Mutex<T>(std::sync::Mutex<T>);

impl<T> Mutex<T> {
    #[inline]
    pub fn new(_name: &'static str, value: T) -> Self {
        Self(std::sync::Mutex::new(value))
    }

    #[inline]
    pub fn lock(&self) -> std::sync::MutexGuard<'_, T> {
        self.0.lock().unwrap()
    }
}

/// Wrapper around async-channel Sender to match tokio's API.
pub struct Sender<T>(async_channel::Sender<T>);

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Sender(self.0.clone())
    }
}

impl<T> Sender<T> {
    /// Send a value, waiting if the channel is full.
    pub async fn send(&self, value: T) -> Result<(), SendError<T>> {
        self.0.send(value).await.map_err(|e| SendError(e.0))
    }

    /// Try to send a value without blocking.
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        self.0.try_send(value).map_err(|e| match e {
            async_channel::TrySendError::Full(v) => TrySendError::Full(v),
            async_channel::TrySendError::Closed(v) => TrySendError::Closed(v),
        })
    }

    /// Check if the channel is closed.
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }
}

/// Error returned when sending fails because the receiver was dropped.
pub struct SendError<T>(pub T);

impl<T> std::fmt::Debug for SendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SendError").finish_non_exhaustive()
    }
}

impl<T> std::fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "channel closed")
    }
}

impl<T> std::error::Error for SendError<T> {}

/// Error returned when try_send fails.
pub enum TrySendError<T> {
    /// Channel is full.
    Full(T),
    /// Channel is closed.
    Closed(T),
}

impl<T> std::fmt::Debug for TrySendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrySendError::Full(_) => f.debug_struct("TrySendError::Full").finish_non_exhaustive(),
            TrySendError::Closed(_) => f
                .debug_struct("TrySendError::Closed")
                .finish_non_exhaustive(),
        }
    }
}

/// Wrapper around async-channel Receiver to match tokio's API.
pub struct Receiver<T>(async_channel::Receiver<T>);

impl<T> Receiver<T> {
    /// Receive a value, returning None if the channel is closed.
    pub async fn recv(&mut self) -> Option<T> {
        self.0.recv().await.ok()
    }
}

/// Unbounded sender (same as bounded for async-channel).
pub type UnboundedSender<T> = Sender<T>;
/// Unbounded receiver (same as bounded for async-channel).
pub type UnboundedReceiver<T> = Receiver<T>;

/// Create a bounded mpsc channel.
pub fn channel<T>(_name: impl Into<String>, buffer: usize) -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = async_channel::bounded(buffer);
    (Sender(tx), Receiver(rx))
}

/// Create an unbounded mpsc channel.
pub fn unbounded_channel<T>(_name: impl Into<String>) -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = async_channel::unbounded();
    (Sender(tx), Receiver(rx))
}

/// Create a oneshot channel.
pub fn oneshot<T>(_name: impl Into<String>) -> (OneshotSender<T>, OneshotReceiver<T>) {
    let (tx, rx) = futures_channel::oneshot::channel();
    (tx, OneshotReceiver(rx))
}

/// Handle that can be used to abort a spawned task.
///
/// On WASM, abort is a no-op since there's no way to cancel fire-and-forget tasks.
/// This is a placeholder for API compatibility with the native runtime.
#[derive(Debug)]
pub struct AbortHandle;

impl AbortHandle {
    /// Abort the associated task.
    ///
    /// On WASM, this is a no-op and always returns `false` since tasks can't
    /// be cancelled.
    pub fn abort(&self) -> bool {
        // WASM tasks are fire-and-forget, can't be cancelled
        false
    }
}

/// Spawn a task that runs concurrently.
///
/// On WASM, futures don't need to be `Send` since everything is single-threaded.
/// This is fire-and-forget; there's no JoinHandle.
pub fn spawn<F>(_name: &'static str, future: F)
where
    F: Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(future);
}

/// Spawn a task and return an abort handle that can be used to cancel it.
///
/// On WASM, the abort handle is a no-op since tasks can't be cancelled.
/// The task will still run to completion.
pub fn spawn_with_abort<F>(_name: &'static str, future: F) -> AbortHandle
where
    F: Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(future);
    AbortHandle
}

/// Sleep for the given duration.
pub async fn sleep(duration: Duration, _label: impl Into<String>) {
    gloo_timers::future::sleep(duration).await;
}

/// Run a future with a timeout.
///
/// Returns `Some(result)` if the future completes within the timeout,
/// or `None` if the timeout expires.
pub async fn timeout<F, T>(duration: Duration, future: F, _label: impl Into<String>) -> Option<T>
where
    F: Future<Output = T>,
{
    use futures_util::future::{Either, select};
    use std::pin::pin;

    let sleep_fut = pin!(gloo_timers::future::sleep(duration));
    let work_fut = pin!(future);

    match select(work_fut, sleep_fut).await {
        Either::Left((result, _)) => Some(result),
        Either::Right((_, _)) => None,
    }
}
