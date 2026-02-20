use std::future::Future;
use std::time::Duration;

use moire::task::FutureExt as _;

pub use moire::sync::Mutex;
pub use moire::sync::mpsc::{Receiver, Sender, UnboundedReceiver, UnboundedSender};
pub use moire::sync::oneshot::Sender as OneshotSender;

pub struct OneshotReceiver<T>(moire::sync::oneshot::Receiver<T>);

impl<T> OneshotReceiver<T> {
    pub async fn recv(self) -> Result<T, tokio::sync::oneshot::error::RecvError> {
        self.0.await
    }
}

pub fn channel<T>(name: impl Into<String>, buffer: usize) -> (Sender<T>, Receiver<T>) {
    moire::sync::mpsc::channel(name, buffer)
}

pub fn unbounded_channel<T>(name: impl Into<String>) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    moire::sync::mpsc::unbounded_channel(name)
}

pub fn oneshot<T>(name: impl Into<String>) -> (OneshotSender<T>, OneshotReceiver<T>) {
    let (tx, rx) = moire::sync::oneshot::channel(name);
    (tx, OneshotReceiver(rx))
}

pub struct AbortHandle {
    #[cfg(not(target_arch = "wasm32"))]
    inner: moire::task::JoinHandle<()>,
}

impl std::fmt::Debug for AbortHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AbortHandle").finish_non_exhaustive()
    }
}

impl AbortHandle {
    pub fn abort(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let was_finished = self.inner.is_finished();
            self.inner.abort();
            !was_finished
        }
        #[cfg(target_arch = "wasm32")]
        {
            false
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn spawn<F>(name: &'static str, future: F) -> moire::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    moire::task::spawn(future.named(name))
}

#[cfg(target_arch = "wasm32")]
pub fn spawn<F>(name: &'static str, future: F)
where
    F: Future<Output = ()> + 'static,
{
    moire::spawn(future.named(name));
}

#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_with_abort<F>(name: &'static str, future: F) -> AbortHandle
where
    F: Future<Output = ()> + Send + 'static,
{
    let inner = moire::task::spawn(future.named(name));
    AbortHandle { inner }
}

#[cfg(target_arch = "wasm32")]
pub fn spawn_with_abort<F>(name: &'static str, future: F) -> AbortHandle
where
    F: Future<Output = ()> + 'static,
{
    moire::spawn(future.named(name));
    AbortHandle {}
}

pub fn sleep(duration: Duration, label: impl Into<String>) -> impl Future<Output = ()> {
    let _ = label.into();
    moire::time::sleep(duration)
}

#[allow(clippy::manual_async_fn)]
pub fn timeout<F, T>(
    duration: Duration,
    future: F,
    label: impl Into<String>,
) -> impl Future<Output = Option<T>>
where
    F: Future<Output = T>,
{
    async move {
        let _ = label.into();
        moire::time::timeout(duration, future).await.ok()
    }
}
