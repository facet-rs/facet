// src/types.rs

use std::num::NonZeroU32;

/// Channel ID. 0 is reserved for control channel (not representable here).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ChannelId(NonZeroU32);

impl ChannelId {
    /// Create a new data channel ID. Returns None if id == 0.
    pub fn new(id: u32) -> Option<Self> {
        NonZeroU32::new(id).map(ChannelId)
    }

    pub fn get(self) -> u32 {
        self.0.get()
    }
}

/// Method ID for RPC dispatch.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MethodId(pub(crate) u32);

impl MethodId {
    pub fn new(id: u32) -> Self {
        MethodId(id)
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

/// Message ID, monotonically increasing per session.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MsgId(pub(crate) u64);

impl MsgId {
    pub fn new(id: u64) -> Self {
        MsgId(id)
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

/// Slot index in the data segment.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SlotIndex(pub(crate) u32);

impl SlotIndex {
    pub fn new(idx: u32) -> Self {
        SlotIndex(idx)
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

/// Generation counter for ABA safety.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Generation(pub(crate) u32);

impl Generation {
    pub fn new(gen: u32) -> Self {
        Generation(gen)
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

/// Validated byte length (checked against max).
#[derive(Clone, Copy, Debug)]
pub struct ByteLen(u32);

impl ByteLen {
    pub fn new(len: u32, max: u32) -> Option<Self> {
        (len <= max).then_some(ByteLen(len))
    }

    /// Create without validation (for internal use where bounds are already checked)
    pub(crate) fn new_unchecked(len: u32) -> Self {
        ByteLen(len)
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_id_zero_is_none() {
        assert!(ChannelId::new(0).is_none());
    }

    #[test]
    fn channel_id_nonzero_works() {
        let id = ChannelId::new(42).unwrap();
        assert_eq!(id.get(), 42);
    }

    #[test]
    fn byte_len_respects_max() {
        assert!(ByteLen::new(100, 50).is_none());
        assert!(ByteLen::new(50, 100).is_some());
        assert!(ByteLen::new(100, 100).is_some());
    }
}
