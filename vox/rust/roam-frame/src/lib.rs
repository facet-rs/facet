#![deny(unsafe_code)]

//! Transport-agnostic frame representation.
//!
//! Canonical definitions live in `docs/content/spec/_index.md` and
//! `docs/content/shm-spec/_index.md`.

mod frame;
mod owned_message;
pub mod shm_frame;

pub use frame::{Frame, INLINE_PAYLOAD_LEN, INLINE_PAYLOAD_SLOT, MsgDesc, Payload};
pub use owned_message::OwnedMessage;
pub use shm_frame::{
    DEFAULT_INLINE_THRESHOLD, FLAG_SLOT_REF, SHM_FRAME_HEADER_SIZE, SLOT_REF_FRAME_SIZE,
    SLOT_REF_SIZE, ShmFrameHeader, SlotRef, encode_inline_frame, encode_slot_ref_frame,
    inline_frame_size, should_inline,
};
