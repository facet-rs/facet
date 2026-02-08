//! SHM BipBuffer message frame format (v2).
//!
//! Variable-length frames written into BipBuffer rings. Each frame has
//! a 24-byte header followed by either inline payload or a slot reference.

/// SHM frame header size in bytes.
pub const SHM_FRAME_HEADER_SIZE: usize = 24;

/// Size of a slot reference in bytes.
pub const SLOT_REF_SIZE: usize = 12;

/// Frame size when using a slot reference (header + slot ref + padding).
pub const SLOT_REF_FRAME_SIZE: usize = 36;

/// Default inline threshold: messages with `24 + payload_len <= threshold` go inline.
pub const DEFAULT_INLINE_THRESHOLD: u32 = 256;

/// Flag bit: payload is in VarSlotPool rather than inline.
pub const FLAG_SLOT_REF: u8 = 0x01;

/// SHM frame header (24 bytes, little-endian).
///
/// ```text
/// [0..4)   total_len:   u32 LE  — frame size including this field, padded to 4
/// [4)      msg_type:    u8      — message type
/// [5)      flags:       u8      — bit 0: SLOT_REF
/// [6..8)   _reserved:   u16
/// [8..12)  id:          u32 LE  — request_id or channel_id
/// [12..20) method_id:   u64 LE  — method hash (0 for non-Request)
/// [20..24) payload_len: u32 LE  — actual payload byte count
/// ```
///
/// shm[impl shm.frame.header]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShmFrameHeader {
    pub total_len: u32,
    pub msg_type: u8,
    pub flags: u8,
    pub id: u32,
    pub method_id: u64,
    pub payload_len: u32,
}

impl ShmFrameHeader {
    /// Write this header into a byte buffer (must be >= 24 bytes).
    pub fn write_to(&self, buf: &mut [u8]) {
        assert!(buf.len() >= SHM_FRAME_HEADER_SIZE);
        buf[0..4].copy_from_slice(&self.total_len.to_le_bytes());
        buf[4] = self.msg_type;
        buf[5] = self.flags;
        buf[6..8].copy_from_slice(&0u16.to_le_bytes()); // reserved
        buf[8..12].copy_from_slice(&self.id.to_le_bytes());
        buf[12..20].copy_from_slice(&self.method_id.to_le_bytes());
        buf[20..24].copy_from_slice(&self.payload_len.to_le_bytes());
    }

    /// Read a header from a byte buffer (must be >= 24 bytes).
    ///
    /// Returns `None` if the buffer is too small.
    pub fn read_from(buf: &[u8]) -> Option<Self> {
        if buf.len() < SHM_FRAME_HEADER_SIZE {
            return None;
        }
        Some(Self {
            total_len: u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
            msg_type: buf[4],
            flags: buf[5],
            // buf[6..8] reserved
            id: u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]),
            method_id: u64::from_le_bytes([
                buf[12], buf[13], buf[14], buf[15], buf[16], buf[17], buf[18], buf[19],
            ]),
            payload_len: u32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]),
        })
    }

    /// Returns true if the SLOT_REF flag is set.
    #[inline]
    pub fn has_slot_ref(&self) -> bool {
        self.flags & FLAG_SLOT_REF != 0
    }
}

/// Reference to a payload stored in the VarSlotPool.
///
/// ```text
/// [0)      class_idx:       u8
/// [1)      extent_idx:      u8
/// [2..4)   _pad:            u16
/// [4..8)   slot_idx:        u32 LE
/// [8..12)  slot_generation: u32 LE
/// ```
///
/// shm[impl shm.frame.slot-ref]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotRef {
    pub class_idx: u8,
    pub extent_idx: u8,
    pub slot_idx: u32,
    pub slot_generation: u32,
}

impl SlotRef {
    /// Write this slot reference into a byte buffer (must be >= 12 bytes).
    pub fn write_to(&self, buf: &mut [u8]) {
        assert!(buf.len() >= SLOT_REF_SIZE);
        buf[0] = self.class_idx;
        buf[1] = self.extent_idx;
        buf[2..4].copy_from_slice(&0u16.to_le_bytes()); // pad
        buf[4..8].copy_from_slice(&self.slot_idx.to_le_bytes());
        buf[8..12].copy_from_slice(&self.slot_generation.to_le_bytes());
    }

    /// Read a slot reference from a byte buffer (must be >= 12 bytes).
    pub fn read_from(buf: &[u8]) -> Option<Self> {
        if buf.len() < SLOT_REF_SIZE {
            return None;
        }
        Some(Self {
            class_idx: buf[0],
            extent_idx: buf[1],
            // buf[2..4] pad
            slot_idx: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
            slot_generation: u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]),
        })
    }
}

/// Align a value up to a multiple of 4.
#[inline]
fn align4(n: u32) -> u32 {
    (n + 3) & !3
}

/// Encode an inline frame into `buf`.
///
/// Returns the total number of bytes written (the frame's `total_len`).
///
/// The buffer must be large enough: `align4(24 + payload.len())`.
///
/// shm[impl shm.frame.inline]
/// shm[impl shm.frame.alignment]
pub fn encode_inline_frame(
    msg_type: u8,
    id: u32,
    method_id: u64,
    payload: &[u8],
    buf: &mut [u8],
) -> usize {
    let payload_len = payload.len() as u32;
    let total_len = align4(SHM_FRAME_HEADER_SIZE as u32 + payload_len);
    assert!(buf.len() >= total_len as usize);

    let header = ShmFrameHeader {
        total_len,
        msg_type,
        flags: 0,
        id,
        method_id,
        payload_len,
    };
    header.write_to(&mut buf[..SHM_FRAME_HEADER_SIZE]);
    buf[SHM_FRAME_HEADER_SIZE..SHM_FRAME_HEADER_SIZE + payload.len()].copy_from_slice(payload);

    // Zero padding bytes
    let padded_end = total_len as usize;
    let data_end = SHM_FRAME_HEADER_SIZE + payload.len();
    for byte in &mut buf[data_end..padded_end] {
        *byte = 0;
    }

    total_len as usize
}

/// Encode a slot-referenced frame into `buf`.
///
/// Returns the total number of bytes written (always 36).
pub fn encode_slot_ref_frame(
    msg_type: u8,
    id: u32,
    method_id: u64,
    payload_len: u32,
    slot_ref: &SlotRef,
    buf: &mut [u8],
) -> usize {
    assert!(buf.len() >= SLOT_REF_FRAME_SIZE);

    let header = ShmFrameHeader {
        total_len: SLOT_REF_FRAME_SIZE as u32,
        msg_type,
        flags: FLAG_SLOT_REF,
        id,
        method_id,
        payload_len,
    };
    header.write_to(&mut buf[..SHM_FRAME_HEADER_SIZE]);
    slot_ref.write_to(&mut buf[SHM_FRAME_HEADER_SIZE..SHM_FRAME_HEADER_SIZE + SLOT_REF_SIZE]);

    SLOT_REF_FRAME_SIZE
}

/// Compute the total frame size for an inline payload.
#[inline]
pub fn inline_frame_size(payload_len: u32) -> u32 {
    align4(SHM_FRAME_HEADER_SIZE as u32 + payload_len)
}

/// Returns true if a payload should go inline given the threshold.
///
/// shm[impl shm.frame.threshold]
#[inline]
pub fn should_inline(payload_len: u32, inline_threshold: u32) -> bool {
    SHM_FRAME_HEADER_SIZE as u32 + payload_len <= inline_threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_roundtrip() {
        let header = ShmFrameHeader {
            total_len: 48,
            msg_type: 1,
            flags: 0,
            id: 42,
            method_id: 0xDEAD_BEEF_CAFE_1234,
            payload_len: 24,
        };

        let mut buf = [0u8; 24];
        header.write_to(&mut buf);
        let decoded = ShmFrameHeader::read_from(&buf).unwrap();
        assert_eq!(header, decoded);
    }

    #[test]
    fn slot_ref_roundtrip() {
        let slot_ref = SlotRef {
            class_idx: 2,
            extent_idx: 1,
            slot_idx: 42,
            slot_generation: 7,
        };

        let mut buf = [0u8; 12];
        slot_ref.write_to(&mut buf);
        let decoded = SlotRef::read_from(&buf).unwrap();
        assert_eq!(slot_ref, decoded);
    }

    #[test]
    fn inline_frame_roundtrip() {
        let mut buf = [0u8; 256];
        let payload = b"hello, world!";
        let total = encode_inline_frame(1, 99, 0x1234_5678, payload, &mut buf);

        let header = ShmFrameHeader::read_from(&buf).unwrap();
        assert_eq!(header.total_len, total as u32);
        assert_eq!(header.msg_type, 1);
        assert_eq!(header.flags, 0);
        assert!(!header.has_slot_ref());
        assert_eq!(header.id, 99);
        assert_eq!(header.method_id, 0x1234_5678);
        assert_eq!(header.payload_len, 13);

        // Payload follows header
        let payload_start = SHM_FRAME_HEADER_SIZE;
        assert_eq!(&buf[payload_start..payload_start + 13], payload);

        // Verify alignment
        assert_eq!(total % 4, 0);
    }

    #[test]
    fn slot_ref_frame_roundtrip() {
        let mut buf = [0u8; 64];
        let slot_ref = SlotRef {
            class_idx: 0,
            extent_idx: 0,
            slot_idx: 17,
            slot_generation: 3,
        };
        let total = encode_slot_ref_frame(4, 5, 0, 4096, &slot_ref, &mut buf);

        assert_eq!(total, SLOT_REF_FRAME_SIZE);

        let header = ShmFrameHeader::read_from(&buf).unwrap();
        assert_eq!(header.total_len, 36);
        assert_eq!(header.msg_type, 4);
        assert!(header.has_slot_ref());
        assert_eq!(header.id, 5);
        assert_eq!(header.payload_len, 4096);

        let decoded_ref = SlotRef::read_from(&buf[SHM_FRAME_HEADER_SIZE..]).unwrap();
        assert_eq!(decoded_ref, slot_ref);
    }

    #[test]
    fn align4_works() {
        assert_eq!(align4(0), 0);
        assert_eq!(align4(1), 4);
        assert_eq!(align4(2), 4);
        assert_eq!(align4(3), 4);
        assert_eq!(align4(4), 4);
        assert_eq!(align4(5), 8);
        assert_eq!(align4(24), 24);
        assert_eq!(align4(25), 28);
    }

    #[test]
    fn inline_threshold_logic() {
        // Default threshold is 256
        assert!(should_inline(0, DEFAULT_INLINE_THRESHOLD));
        assert!(should_inline(232, DEFAULT_INLINE_THRESHOLD)); // 24 + 232 = 256
        assert!(!should_inline(233, DEFAULT_INLINE_THRESHOLD)); // 24 + 233 = 257
    }

    #[test]
    fn empty_payload_frame() {
        let mut buf = [0u8; 64];
        let total = encode_inline_frame(3, 1, 0, &[], &mut buf);
        assert_eq!(total, 24); // align4(24 + 0) = 24

        let header = ShmFrameHeader::read_from(&buf).unwrap();
        assert_eq!(header.payload_len, 0);
        assert_eq!(header.total_len, 24);
    }

    #[test]
    fn header_too_small() {
        let buf = [0u8; 10];
        assert!(ShmFrameHeader::read_from(&buf).is_none());
    }

    #[test]
    fn slot_ref_too_small() {
        let buf = [0u8; 8];
        assert!(SlotRef::read_from(&buf).is_none());
    }
}
