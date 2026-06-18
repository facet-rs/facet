//! In-process transport for vox — direct WASM ↔ JS message passing.
//!
//! Provides a [`Link`](vox_types::Link) implementation that communicates
//! with TypeScript in the same browser tab via `js_sys::Function` callbacks
//! and `futures_channel::mpsc` channels, with no network involved.
//!
//! This crate only provides types on `wasm32` targets.

#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(target_arch = "wasm32")]
pub use wasm::*;
