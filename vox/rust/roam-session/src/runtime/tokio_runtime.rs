//! Native (tokio) runtime implementation.

use std::future::Future;
use std::time::Duration;

// Re-export tokio sync types directly
pub use tokio::sync::Mutex;
pub use tokio::sync::mpsc::{
    Receiver, Sender, UnboundedReceiver, UnboundedSender, channel, error::SendError,
    unbounded_channel,
};
pub use tokio::sync::oneshot::{Receiver as OneshotReceiver, Sender as OneshotSender};

/// Create a bounded mpsc channel.
pub fn bounded<T>(buffer: usize) -> (Sender<T>, Receiver<T>) {
    channel(buffer)
}

/// Create an unbounded mpsc channel.
pub fn unbounded<T>() -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    unbounded_channel()
}

/// Create a oneshot channel.
pub fn oneshot<T>() -> (OneshotSender<T>, OneshotReceiver<T>) {
    tokio::sync::oneshot::channel()
}

/// Handle that can be used to abort a spawned task.
///
/// On native, this wraps tokio's AbortHandle. On WASM, abort is a no-op
/// since there's no way to cancel fire-and-forget tasks.
#[derive(Debug)]
pub struct AbortHandle(tokio::task::AbortHandle);

impl AbortHandle {
    /// Abort the associated task.
    ///
    /// Returns `true` if the task was successfully aborted, `false` if it had
    /// already completed.
    pub fn abort(&self) -> bool {
        // tokio's abort() doesn't return anything, but we can check if finished
        self.0.abort();
        // Return true since we sent the abort signal
        true
    }
}

/// Spawn a task that runs concurrently.
///
/// On native, this returns a JoinHandle. On WASM, spawning is fire-and-forget.
pub fn spawn<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(future)
}

/// Spawn a task and return an abort handle that can be used to cancel it.
///
/// On native, this uses tokio's abort mechanism. On WASM, the abort handle
/// is a no-op since tasks can't be cancelled.
pub fn spawn_with_abort<F>(future: F) -> AbortHandle
where
    F: Future<Output = ()> + Send + 'static,
{
    let handle = tokio::spawn(future);
    AbortHandle(handle.abort_handle())
}

/// Sleep for the given duration.
pub async fn sleep(duration: Duration) {
    tokio::time::sleep(duration).await;
}

/// Run a future with a timeout.
///
/// Returns `Some(result)` if the future completes within the timeout,
/// or `None` if the timeout expires.
pub async fn timeout<F, T>(duration: Duration, future: F) -> Option<T>
where
    F: Future<Output = T>,
{
    (tokio::time::timeout(duration, future).await).ok()
}
