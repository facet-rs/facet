//! Re-exports different asynchronous types that we need as tokio on non-WASM platforms and
//! futures-* crates on WASM platforms.

#[cfg(not(target_arch = "wasm32"))]
mod tokio_runtime;
#[cfg(not(target_arch = "wasm32"))]
pub use tokio_runtime::*;

#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(target_arch = "wasm32")]
pub use wasm::*;
