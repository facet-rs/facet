use std::future::Future;
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
pub mod error {
    pub use tokio::time::error::Elapsed;
}

#[cfg(target_arch = "wasm32")]
pub mod error {
    pub use wasmtimer::tokio::error::Elapsed;
}

#[cfg(not(target_arch = "wasm32"))]
pub use tokio::time::Instant;
#[cfg(target_arch = "wasm32")]
pub use wasmtimer::std::Instant;

#[cfg(not(target_arch = "wasm32"))]
pub fn sleep(duration: Duration) -> impl Future<Output = ()> {
    tokio::time::sleep(duration)
}

#[cfg(target_arch = "wasm32")]
pub fn sleep(duration: Duration) -> impl Future<Output = ()> {
    wasmtimer::tokio::sleep(duration)
}

#[cfg(not(target_arch = "wasm32"))]
pub struct Interval(tokio::time::Interval);

#[cfg(target_arch = "wasm32")]
pub struct Interval(wasmtimer::tokio::Interval);

impl Interval {
    pub async fn tick(&mut self) -> Instant {
        self.0.tick().await
    }
}

impl std::fmt::Debug for Interval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn interval(period: Duration) -> Interval {
    Interval(tokio::time::interval(period))
}

#[cfg(target_arch = "wasm32")]
pub fn interval(period: Duration) -> Interval {
    Interval(wasmtimer::tokio::interval(period))
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn timeout<F, T>(duration: Duration, future: F) -> Result<T, error::Elapsed>
where
    F: Future<Output = T>,
{
    tokio::time::timeout(duration, future).await
}

#[cfg(target_arch = "wasm32")]
pub async fn timeout<F, T>(duration: Duration, future: F) -> Result<T, error::Elapsed>
where
    F: Future<Output = T>,
{
    wasmtimer::tokio::timeout(duration, future).await
}
