//! Frame for tracking partial value construction.

use facet_core::{PtrUninit, Shape};

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct FrameFlags: u8 {
        /// The value is initialized
        const INIT = 1 << 0;
        /// This frame owns its allocation
        const OWNS_ALLOC = 1 << 1;
    }
}

/// A frame tracking construction of a single value.
pub struct Frame {
    /// Pointer to the memory being written.
    pub data: PtrUninit,

    /// Shape (type metadata) of the value.
    pub shape: &'static Shape,

    /// State flags.
    pub flags: FrameFlags,
}

impl Frame {
    pub fn new(data: PtrUninit, shape: &'static Shape) -> Self {
        Frame {
            data,
            shape,
            flags: FrameFlags::empty(),
        }
    }
}
