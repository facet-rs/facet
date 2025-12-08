// src/layout.rs

use std::sync::atomic::{AtomicU32, AtomicU64};

/// Magic number for segment validation
pub const MAGIC: u64 = u64::from_le_bytes(*b"RAPACE2\0");

/// Size of inline payload in descriptor
pub const INLINE_PAYLOAD_SIZE: usize = 24;

/// Segment header (64 bytes, cache-line aligned)
#[repr(C, align(64))]
pub struct SegmentHeader {
    pub magic: u64,
    pub version: u32,
    pub flags: u32,
    pub peer_a_epoch: AtomicU64,
    pub peer_b_epoch: AtomicU64,
    pub peer_a_last_seen: AtomicU64,
    pub peer_b_last_seen: AtomicU64,
}

/// Message descriptor - hot path (64 bytes, one cache line)
#[repr(C, align(64))]
pub struct MsgDescHot {
    // Identity (16 bytes)
    pub msg_id: u64,
    pub channel_id: u32,
    pub method_id: u32,

    // Payload location (16 bytes)
    pub payload_slot: u32,        // u32::MAX = inline
    pub payload_generation: u32,
    pub payload_offset: u32,
    pub payload_len: u32,

    // Flow control & flags (8 bytes)
    pub flags: u32,
    pub credit_grant: u32,

    // Inline payload for small messages (24 bytes)
    pub inline_payload: [u8; INLINE_PAYLOAD_SIZE],
}

/// Descriptor ring header
#[repr(C, align(64))]
pub struct DescRingHeader {
    // Producer publication index (cache-line aligned)
    pub visible_head: AtomicU64,
    pub _pad1: [u8; 56],

    // Consumer side (separate cache line)
    pub tail: AtomicU64,
    pub _pad2: [u8; 56],

    // Ring configuration
    pub capacity: u32,
    pub _pad3: [u8; 60],
}

/// Slot metadata for data segment
#[repr(C)]
pub struct SlotMeta {
    pub generation: AtomicU32,
    pub state: AtomicU32,
}

/// Slot states
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotState {
    Free = 0,
    Allocated = 1,
    InFlight = 2,
}

// Compile-time size checks
const _: () = {
    assert!(std::mem::size_of::<SegmentHeader>() == 64);
    assert!(std::mem::size_of::<MsgDescHot>() == 64);
    assert!(std::mem::align_of::<SegmentHeader>() == 64);
    assert!(std::mem::align_of::<MsgDescHot>() == 64);
    assert!(std::mem::align_of::<DescRingHeader>() == 64);
};

impl MsgDescHot {
    /// Check if this descriptor uses inline payload
    pub fn is_inline(&self) -> bool {
        self.payload_slot == u32::MAX
    }
}

impl Default for MsgDescHot {
    fn default() -> Self {
        Self {
            msg_id: 0,
            channel_id: 0,
            method_id: 0,
            payload_slot: u32::MAX,
            payload_generation: 0,
            payload_offset: 0,
            payload_len: 0,
            flags: 0,
            credit_grant: 0,
            inline_payload: [0; INLINE_PAYLOAD_SIZE],
        }
    }
}

impl Clone for MsgDescHot {
    fn clone(&self) -> Self {
        Self {
            msg_id: self.msg_id,
            channel_id: self.channel_id,
            method_id: self.method_id,
            payload_slot: self.payload_slot,
            payload_generation: self.payload_generation,
            payload_offset: self.payload_offset,
            payload_len: self.payload_len,
            flags: self.flags,
            credit_grant: self.credit_grant,
            inline_payload: self.inline_payload,
        }
    }
}
