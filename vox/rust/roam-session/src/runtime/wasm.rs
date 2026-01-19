//! WASM runtime implementation.
//!
//! Uses browser-compatible primitives:
//! - `wasm_bindgen_futures::spawn_local` for spawning
//! - `futures_channel` for channels
//! - `gloo_timers` for timeout/sleep

use std::future::Future;
use std::time::Duration;

// Re-export futures-channel types
pub use futures_channel::mpsc::{
    Receiver, Sender, UnboundedReceiver, UnboundedSender,
};
pub use futures_channel::oneshot::{Receiver as OneshotReceiver, Sender as OneshotSender};

/// Create a bounded mpsc channel.
pub fn bounded<T>(buffer: usize) -> (Sender<T>, Receiver<T>) {
    futures_channel::mpsc::channel(buffer)
}

/// Create an unbounded mpsc channel.
pub fn unbounded<T>() -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    futures_channel::mpsc::unbounded()
}

/// Create a oneshot channel.
pub fn oneshot<T>() -> (OneshotSender<T>, OneshotReceiver<T>) {
    futures_channel::oneshot::channel()
}

/// Spawn a task that runs concurrently.
///
/// On WASM, futures don't need to be `Send` since everything is single-threaded.
/// This is fire-and-forget; there's no JoinHandle.
pub fn spawn<F>(future: F)
where
    F: Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(future);
}

/// Sleep for the given duration.
pub async fn sleep(duration: Duration) {
    gloo_timers::future::sleep(duration).await;
}

/// Run a future with a timeout.
///
/// Returns `Some(result)` if the future completes within the timeout,
/// or `None` if the timeout expires.
pub async fn timeout<F, T>(duration: Duration, future: F) -> Option<T>
where
    F: Future<Output = T>,
{
    use futures_util::future::{select, Either};
    use std::pin::pin;

    let sleep_fut = pin!(gloo_timers::future::sleep(duration));
    let work_fut = pin!(future);

    match select(work_fut, sleep_fut).await {
        Either::Left((result, _)) => Some(result),
        Either::Right((_, _)) => None,
    }
}
