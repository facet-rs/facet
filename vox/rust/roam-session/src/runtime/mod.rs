//! Runtime abstraction layer for tokio/WASM portability.
//!
//! This module provides abstractions over async runtime primitives so the same
//! code can run on both tokio (native) and WASM runtimes.
//!
//! # Supported Primitives
//!
//! | Tokio              | WASM Equivalent              |
//! |--------------------|------------------------------|
//! | `tokio::spawn`     | `wasm_bindgen_futures::spawn_local` |
//! | `tokio::sync::mpsc`| `futures_channel::mpsc`      |
//! | `tokio::sync::oneshot` | `futures_channel::oneshot` |
//! | `tokio::time::timeout` | Manual with gloo-timers  |
//! | `tokio::time::sleep` | `gloo_timers::future::sleep` |
//! | `peeps::Mutex` | `Mutex` (std wrapper) |
//!
//! # Usage
//!
//! ```ignore
//! use roam_session::runtime;
//!
//! // Spawn a task
//! runtime::spawn(async { /* ... */ });
//!
//! // Create channels
//! let (tx, rx) = runtime::channel("my_channel", 16);
//! let (otx, orx) = runtime::oneshot("my_oneshot");
//!
//! // Timeouts and sleeping
//! runtime::sleep(Duration::from_secs(1), "my.sleep").await;
//! let result = runtime::timeout(Duration::from_secs(5), some_future, "my.timeout").await;
//! ```

#[cfg(not(target_arch = "wasm32"))]
mod tokio_runtime;
#[cfg(not(target_arch = "wasm32"))]
pub use tokio_runtime::*;

#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(target_arch = "wasm32")]
pub use wasm::*;
