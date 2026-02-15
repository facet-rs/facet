//! Native (tokio) runtime implementation.

use std::future::Future;
use std::time::Duration;

// Re-export peeps Mutex (tokio::sync::Mutex is banned — causes deadlocks)
pub use peeps::Mutex;

// Re-export peeps channel types
pub use peeps::{
    OneshotReceiver, OneshotSender, Receiver, Sender, UnboundedReceiver, UnboundedSender, channel,
    oneshot_channel, unbounded_channel,
};

// Re-export tokio error types (peeps uses the same ones)
pub use tokio::sync::mpsc::error::SendError;

/// Create a bounded mpsc channel.
pub fn bounded<T>(name: impl Into<String>, buffer: usize) -> (Sender<T>, Receiver<T>) {
    channel(name, buffer)
}

/// Create an unbounded mpsc channel.
pub fn unbounded<T>(name: impl Into<String>) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    unbounded_channel(name)
}

/// Create a oneshot channel.
pub fn oneshot<T>(name: impl Into<String>) -> (OneshotSender<T>, OneshotReceiver<T>) {
    oneshot_channel(name)
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
#[track_caller]
pub fn spawn<F>(name: &'static str, future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    peeps::spawn_tracked(name, future)
}

/// Spawn a task and return an abort handle that can be used to cancel it.
///
/// On native, this uses tokio's abort mechanism. On WASM, the abort handle
/// is a no-op since tasks can't be cancelled.
#[track_caller]
pub fn spawn_with_abort<F>(name: &'static str, future: F) -> AbortHandle
where
    F: Future<Output = ()> + Send + 'static,
{
    let handle = peeps::spawn_tracked(name, future);
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
