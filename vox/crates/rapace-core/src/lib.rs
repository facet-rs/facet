//! rapace-core: Core types and traits for the rapace RPC system.
//!
//! This crate defines:
//! - Frame descriptors (`MsgDescHot`, `MsgDescCold`)
//! - Ring buffer structures (`DescRing`)
//! - Slot allocator (`DataSegment`, `SlotMeta`)
//! - Frame types (`Frame`, `FrameView`)
//! - Transport traits (`Transport`, `DynTransport`)
//! - Error codes and flags (`ErrorCode`, `FrameFlags`, `Encoding`)
//! - Validation (`validate_descriptor`, `DescriptorLimits`)

#![forbid(unsafe_op_in_unsafe_fn)]

// TODO: implement core types
