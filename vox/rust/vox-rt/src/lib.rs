//! Runtime facade used by Vox.
//!
//! This crate intentionally keeps only the named Tokio-shaped primitives Vox
//! uses. Names are accepted at construction sites so existing runtime code can
//! stay descriptive, but this crate does not provide async debugger plumbing.
// r[impl connection.protocol] r[impl rpc.channel]

pub mod sync;
pub mod task;
pub mod time;

pub use task::spawn;
#[cfg(not(target_arch = "wasm32"))]
pub use task::spawn_blocking;
pub use vox_rt_macros::instrument;
