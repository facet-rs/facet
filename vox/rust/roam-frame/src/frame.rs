use bytes::Bytes;

pub const INLINE_PAYLOAD_LEN: usize = 32;
pub const INLINE_PAYLOAD_SLOT: u32 = 0xFFFF_FFFF;

/// Message descriptor (64 bytes) per SHM spec.
///
/// Spec: `docs/content/shm-spec/_index.md` "MsgDesc (64 bytes)".
#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct MsgDesc {
    // Identity (16 bytes)
    pub msg_type: u8,
    pub flags: u8,
    pub _reserved: [u8; 2],
    pub id: u32,
    pub method_id: u64,

    // Payload location (16 bytes)
    pub payload_slot: u32,
    pub payload_generation: u32,
    pub payload_offset: u32,
    pub payload_len: u32,

    // Inline payload (32 bytes)
    pub inline_payload: [u8; INLINE_PAYLOAD_LEN],
}

impl MsgDesc {
    #[inline]
    pub fn new(msg_type: u8, id: u32, method_id: u64) -> Self {
        Self {
            msg_type,
            flags: 0,
            _reserved: [0; 2],
            id,
            method_id,
            payload_slot: INLINE_PAYLOAD_SLOT,
            payload_generation: 0,
            payload_offset: 0,
            payload_len: 0,
            inline_payload: [0; INLINE_PAYLOAD_LEN],
        }
    }

    #[inline]
    pub fn inline_payload_bytes(&self) -> &[u8] {
        let len = (self.payload_len as usize).min(INLINE_PAYLOAD_LEN);
        &self.inline_payload[..len]
    }
}

/// Payload storage for a frame.
#[derive(Debug)]
pub enum Payload {
    /// Payload bytes live inside `MsgDesc::inline_payload`.
    Inline,
    /// Payload bytes are owned as a heap allocation.
    Owned(Vec<u8>),
    /// Payload bytes are stored in a ref-counted buffer (cheap clone).
    Bytes(Bytes),
}

impl Payload {
    pub fn as_slice<'a>(&'a self, desc: &'a MsgDesc) -> &'a [u8] {
        match self {
            Self::Inline => desc.inline_payload_bytes(),
            Self::Owned(buf) => buf.as_slice(),
            Self::Bytes(buf) => buf.as_ref(),
        }
    }

    pub fn external_slice(&self) -> Option<&[u8]> {
        match self {
            Self::Inline => None,
            Self::Owned(buf) => Some(buf.as_slice()),
            Self::Bytes(buf) => Some(buf.as_ref()),
        }
    }

    pub fn len(&self, desc: &MsgDesc) -> usize {
        self.as_slice(desc).len()
    }

    pub fn is_inline(&self) -> bool {
        matches!(self, Self::Inline)
    }
}

/// Owned frame for sending, receiving, or routing.
#[derive(Debug)]
pub struct Frame {
    pub desc: MsgDesc,
    pub payload: Payload,
}

impl Frame {
    #[inline]
    pub fn new(desc: MsgDesc) -> Self {
        Self {
            desc,
            payload: Payload::Inline,
        }
    }

    #[inline]
    pub fn with_inline_payload(mut desc: MsgDesc, payload: &[u8]) -> Option<Self> {
        if payload.len() > INLINE_PAYLOAD_LEN {
            return None;
        }
        desc.payload_slot = INLINE_PAYLOAD_SLOT;
        desc.payload_generation = 0;
        desc.payload_offset = 0;
        desc.payload_len = payload.len() as u32;
        desc.inline_payload[..payload.len()].copy_from_slice(payload);
        Some(Self {
            desc,
            payload: Payload::Inline,
        })
    }

    #[inline]
    pub fn with_owned_payload(mut desc: MsgDesc, payload: Vec<u8>) -> Self {
        desc.payload_slot = 0;
        desc.payload_generation = 0;
        desc.payload_offset = 0;
        desc.payload_len = payload.len() as u32;
        Self {
            desc,
            payload: Payload::Owned(payload),
        }
    }

    #[inline]
    pub fn with_bytes_payload(mut desc: MsgDesc, payload: Bytes) -> Self {
        desc.payload_slot = 0;
        desc.payload_generation = 0;
        desc.payload_offset = 0;
        desc.payload_len = payload.len() as u32;
        Self {
            desc,
            payload: Payload::Bytes(payload),
        }
    }

    #[inline]
    pub fn payload_bytes(&self) -> &[u8] {
        self.payload.as_slice(&self.desc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msg_desc_is_one_cache_line() {
        static_assertions::const_assert!(std::mem::size_of::<MsgDesc>() == 64);
        static_assertions::const_assert!(std::mem::align_of::<MsgDesc>() == 64);
    }

    #[test]
    fn inline_payload_roundtrips() {
        let mut desc = MsgDesc::new(1, 7, 0);
        let payload = b"hello";
        let frame = Frame::with_inline_payload(desc, payload).expect("inline payload");
        assert!(frame.payload.is_inline());
        assert_eq!(frame.payload_bytes(), payload);
        desc.payload_len = 999;
        assert_eq!(desc.inline_payload_bytes().len(), INLINE_PAYLOAD_LEN);
    }
}
