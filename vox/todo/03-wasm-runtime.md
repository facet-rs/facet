# Phase 3: WASM Runtime Implementation

## Goal

Implement the runtime abstraction for WASM using browser-compatible primitives.

## Mapping

| Tokio | WASM Equivalent | Crate |
|-------|-----------------|-------|
| `tokio::spawn` | `wasm_bindgen_futures::spawn_local` | wasm-bindgen-futures |
| `tokio::sync::mpsc` | `futures_channel::mpsc` | futures-channel |
| `tokio::sync::oneshot` | `futures_channel::oneshot` | futures-channel |
| `tokio::time::timeout` | Manual with `gloo_timers` | gloo-timers |
| `tokio::time::sleep` | `gloo_timers::future::sleep` | gloo-timers |

## Implementation

### roam-session/src/runtime/wasm.rs

```rust
use std::future::Future;
use std::time::Duration;
use futures_channel::{mpsc, oneshot};
use gloo_timers::future::sleep as gloo_sleep;

pub fn spawn<F>(future: F)
where
    F: Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(future);
}

pub fn channel<T>(buffer: usize) -> (mpsc::Sender<T>, mpsc::Receiver<T>) {
    mpsc::channel(buffer)
}

pub fn unbounded<T>() -> (mpsc::UnboundedSender<T>, mpsc::UnboundedReceiver<T>) {
    mpsc::unbounded()
}

pub fn oneshot<T>() -> (oneshot::Sender<T>, oneshot::Receiver<T>) {
    oneshot::channel()
}

pub async fn sleep(duration: Duration) {
    gloo_sleep(duration).await;
}

pub async fn timeout<F, T>(duration: Duration, future: F) -> Option<T>
where
    F: Future<Output = T>,
{
    use futures_util::future::{select, Either};
    use std::pin::pin;

    let sleep_fut = pin!(gloo_sleep(duration));
    let work_fut = pin!(future);

    match select(work_fut, sleep_fut).await {
        Either::Left((result, _)) => Some(result),
        Either::Right((_, _)) => None,
    }
}
```

## WASM Constraints

### No `Send` bounds
WASM is single-threaded. Futures don't need to be `Send`.
The runtime abstraction should handle this:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{ ... }

#[cfg(target_arch = "wasm32")]
pub fn spawn<F>(future: F)
where
    F: Future<Output = ()> + 'static,  // No Send!
{ ... }
```

### No JoinHandle return
`spawn_local` doesn't return a JoinHandle. For cases where we need to
await task completion, we'd use a oneshot channel.

## Dependencies

Add to `roam-session/Cargo.toml`:

```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen-futures = "0.4"
futures-channel = "0.3"
gloo-timers = { version = "0.3", features = ["futures"] }
futures-util = { version = "0.3", default-features = false, features = ["alloc"] }
```

## Testing

WASM tests require `wasm-pack test` or similar.
Consider adding a `tests/wasm.rs` that runs in browser/node.
