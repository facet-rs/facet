//! VarSlotPool exports.
//!
//! The core implementation lives in `shm-primitives`; vox-shm re-exports
//! these types for API compatibility.

pub use shm_primitives::{
    ClassOffsets, DoubleFreeError, PoolLayout, SizeClassConfig, SlotRef, VarSlotPool,
};
