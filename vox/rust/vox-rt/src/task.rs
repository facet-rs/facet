// r[impl session] r[impl rpc.session-setup]

use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::task::{Context, Poll};

pub trait FutureExt: IntoFuture + Sized {
    fn named(self, _name: impl Into<String>) -> Self {
        self
    }
}

impl<F: IntoFuture + Sized> FutureExt for F {}

#[cfg(not(target_arch = "wasm32"))]
pub struct JoinHandle<T>(tokio::task::JoinHandle<T>);

#[cfg(not(target_arch = "wasm32"))]
impl<T> JoinHandle<T> {
    pub fn named(self, _name: impl Into<String>) -> Self {
        self
    }

    pub fn abort(&self) {
        self.0.abort();
    }

    pub fn is_finished(&self) -> bool {
        self.0.is_finished()
    }

    pub fn id(&self) -> tokio::task::Id {
        self.0.id()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T> Future for JoinHandle<T> {
    type Output = Result<T, tokio::task::JoinError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        unsafe { Pin::new_unchecked(&mut this.0) }.poll(cx)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T> std::fmt::Debug for JoinHandle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JoinHandle")
            .field("id", &self.id())
            .field("is_finished", &self.is_finished())
            .finish()
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn spawn<T, F>(future: F) -> JoinHandle<T>
where
    T: Send + 'static,
    F: Future<Output = T> + Send + 'static,
{
    JoinHandle(tokio::task::spawn(future))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_blocking<T, F>(f: F) -> JoinHandle<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    JoinHandle(tokio::task::spawn_blocking(f))
}

#[cfg(target_arch = "wasm32")]
pub struct JoinHandle<T> {
    rx: tokio::sync::oneshot::Receiver<T>,
}

#[cfg(target_arch = "wasm32")]
impl<T> JoinHandle<T> {
    pub fn named(self, _name: impl Into<String>) -> Self {
        self
    }

    pub fn abort(&self) {}
}

#[cfg(target_arch = "wasm32")]
impl<T> Future for JoinHandle<T> {
    type Output = Result<T, tokio::sync::oneshot::error::RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        unsafe { Pin::new_unchecked(&mut this.rx) }.poll(cx)
    }
}

#[cfg(target_arch = "wasm32")]
impl<T> std::fmt::Debug for JoinHandle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JoinHandle").finish_non_exhaustive()
    }
}

#[cfg(target_arch = "wasm32")]
pub fn spawn<T, F>(future: F) -> JoinHandle<T>
where
    T: 'static,
    F: Future<Output = T> + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    wasm_bindgen_futures::spawn_local(async move {
        let result = future.await;
        let _ = tx.send(result);
    });
    JoinHandle { rx }
}
