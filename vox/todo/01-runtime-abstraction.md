# Phase 1: Runtime Abstraction Layer

## Goal

Create traits that abstract over tokio's runtime primitives so the same code
can run on both tokio (native) and WASM runtimes.

## Primitives to Abstract

```rust
// Spawning tasks
tokio::spawn(future)           → Runtime::spawn(future)

// Channels
tokio::sync::mpsc              → abstract Sender<T>/Receiver<T>
tokio::sync::oneshot           → abstract OneshotSender<T>/OneshotReceiver<T>

// Time
tokio::time::timeout(dur, fut) → Runtime::timeout(dur, fut)
tokio::time::sleep(dur)        → Runtime::sleep(dur)
```

## Design Options

### Option A: Runtime trait with associated types

```rust
pub trait Runtime {
    type Sender<T>: Sender<T>;
    type Receiver<T>: Receiver<T>;
    type OneshotSender<T>: OneshotSender<T>;
    type OneshotReceiver<T>: OneshotReceiver<T>;
    type JoinHandle<T>: Future<Output = T>;

    fn spawn<F>(future: F) -> Self::JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static;

    fn channel<T>(buffer: usize) -> (Self::Sender<T>, Self::Receiver<T>);
    fn oneshot<T>() -> (Self::OneshotSender<T>, Self::OneshotReceiver<T>);

    fn timeout<F>(duration: Duration, future: F) -> impl Future<Output = Option<F::Output>>
    where
        F: Future;

    fn sleep(duration: Duration) -> impl Future<Output = ()>;
}
```

### Option B: Feature flags with type aliases

```rust
#[cfg(not(target_arch = "wasm32"))]
mod runtime {
    pub use tokio::sync::mpsc::{Sender, Receiver, channel};
    pub use tokio::sync::oneshot;
    pub use tokio::spawn;
    pub use tokio::time::{timeout, sleep};
}

#[cfg(target_arch = "wasm32")]
mod runtime {
    pub use futures_channel::mpsc::{Sender, Receiver, channel};
    pub use futures_channel::oneshot;
    pub use wasm_bindgen_futures::spawn_local as spawn;
    // timeout/sleep via gloo-timers
}
```

### Recommendation

**Option B (feature flags)** is simpler and avoids the complexity of threading
a Runtime type parameter through everything. The API surface is small enough
that cfg-based switching is manageable.

## New Crate or Module?

Could be:
- A new `roam-runtime` crate
- A `runtime` module in `roam-session`

Recommendation: **Module in roam-session** to avoid another crate. It's internal plumbing.

## Files to Create

```
roam-session/src/runtime.rs      # Abstraction layer
roam-session/src/runtime/mod.rs  # (alternative: submodules)
roam-session/src/runtime/tokio.rs
roam-session/src/runtime/wasm.rs
```

## Dependencies to Add (roam-session)

```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen-futures = "0.4"
futures-channel = "0.3"
gloo-timers = { version = "0.3", features = ["futures"] }
```
