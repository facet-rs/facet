#![deny(unsafe_code)]

//! Transport-agnostic frame representation.
//!
//! Canonical definitions live in `docs/content/spec/_index.md` and
//! `docs/content/shm-spec/_index.md`.

mod frame;
mod owned_message;

pub use frame::{Frame, INLINE_PAYLOAD_LEN, INLINE_PAYLOAD_SLOT, MsgDesc, Payload};
pub use owned_message::OwnedMessage;
