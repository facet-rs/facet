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
