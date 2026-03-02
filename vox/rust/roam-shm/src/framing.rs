//! SHM framing — the 8-byte header that wraps each BipBuffer entry.
//!
//! Every entry written to a BipBuffer is prefixed with a [`FrameHeader`].
//! The payload is either:
//!
//! - **Inline**: payload bytes follow the header directly. The entry is
//!   `align4(8 + payload_len)` bytes.
//! - **Slot-ref**: a [`SlotRefBody`] follows the header (bit 0 set). The entry
//!   is exactly 20 bytes.
//! - **Mmap-ref**: a [`MmapRefBody`] follows the header (bit 1 set). The entry
//!   is exactly 32 bytes.
//!
//! Bits 0 and 1 of flags MUST NOT both be set.
//!
//! All values are native-endian (little-endian on all supported platforms).

use shm_primitives::BipBufFull;
use shm_primitives::bipbuf::{BipBufConsumer, BipBufProducer};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::varslot::SlotRef;

// ── constants ─────────────────────────────────────────────────────────────────

/// Bit 0 of `FrameHeader::flags`: payload is in VarSlotPool, not inline.
pub const FLAG_SLOT_REF: u8 = 0x01;

/// Bit 1 of `FrameHeader::flags`: payload is in an external mmap region.
pub const FLAG_MMAP_REF: u8 = 0x02;

/// Size of [`FrameHeader`] in bytes.
pub const FRAME_HEADER_SIZE: usize = 8;

/// Size of [`SlotRefBody`] in bytes.
pub const SLOT_REF_BODY_SIZE: usize = 12;

/// Size of a slot-ref entry (header + body), always exactly 20 bytes.
pub const SLOT_REF_ENTRY_SIZE: u32 = 20;

/// Size of [`MmapRefBody`] in bytes.
pub const MMAP_REF_BODY_SIZE: usize = 24;

/// Size of an mmap-ref entry (header + body), always exactly 32 bytes.
pub const MMAP_REF_ENTRY_SIZE: u32 = 32;

/// Default inline threshold when the segment header field is 0.
pub const DEFAULT_INLINE_THRESHOLD: u32 = 256;

// ── wire types ────────────────────────────────────────────────────────────────

/// The 8-byte header that begins every BipBuffer entry.
///
/// r[impl shm.framing]
/// r[impl shm.framing.header]
/// r[impl shm.framing.alignment]
#[derive(FromBytes, IntoBytes, KnownLayout, Immutable)]
#[repr(C)]
pub struct FrameHeader {
    /// Total entry size in bytes, padded to a 4-byte boundary.
    /// Includes the 8-byte header itself.
    pub total_len: u32,
    /// Bit 0: `FLAG_SLOT_REF`. Bit 1: `FLAG_MMAP_REF`. Both MUST NOT be set
    /// simultaneously. All other bits reserved and zero.
    pub flags: u8,
    /// Reserved byte, must be zero.
    pub _reserved0: u8,
    /// For inline frames: the actual payload length (excluding padding) as
    /// little-endian u16. Zero means "unknown" (legacy writer), in which case
    /// the reader should use `total_len - 8`.
    pub inline_payload_len: [u8; 2],
}

/// The 12-byte slot reference body that follows a `FrameHeader` when
/// `FLAG_SLOT_REF` is set.
///
/// r[impl shm.framing.slot-ref]
#[derive(FromBytes, IntoBytes, KnownLayout, Immutable)]
#[repr(C)]
pub struct SlotRefBody {
    pub class_idx: u8,
    pub extent_idx: u8,
    pub _reserved: [u8; 2],
    pub slot_idx: u32,
    pub generation: u32,
}

/// The 24-byte mmap reference body that follows a `FrameHeader` when
/// `FLAG_MMAP_REF` is set.
///
/// This type is read/written manually via LE byte copies rather than zerocopy,
/// because the BipBuffer is only 4-byte aligned and MmapRefBody contains a u64.
///
/// Layout: map_id(4) + map_generation(4) + map_offset(8) + payload_len(4) + reserved(4) = 24
///
/// r[impl shm.framing.mmap-ref]
pub struct MmapRefBody;

impl MmapRefBody {
    fn write(buf: &mut [u8], mmap: &MmapRef) {
        debug_assert!(buf.len() >= MMAP_REF_BODY_SIZE);
        buf[0..4].copy_from_slice(&mmap.map_id.to_le_bytes());
        buf[4..8].copy_from_slice(&mmap.map_generation.to_le_bytes());
        buf[8..16].copy_from_slice(&mmap.map_offset.to_le_bytes());
        buf[16..20].copy_from_slice(&mmap.payload_len.to_le_bytes());
        buf[20..24].copy_from_slice(&0u32.to_le_bytes()); // reserved
    }

    fn read(buf: &[u8]) -> Option<MmapRef> {
        if buf.len() < MMAP_REF_BODY_SIZE {
            return None;
        }
        Some(MmapRef {
            map_id: u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
            map_generation: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
            map_offset: u64::from_le_bytes([
                buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
            ]),
            payload_len: u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]),
        })
    }
}

/// Decoded mmap reference (no raw pointers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MmapRef {
    pub map_id: u32,
    pub map_generation: u32,
    pub map_offset: u64,
    pub payload_len: u32,
}

#[cfg(not(loom))]
const _: () = assert!(core::mem::size_of::<FrameHeader>() == FRAME_HEADER_SIZE);
#[cfg(not(loom))]
const _: () = assert!(core::mem::size_of::<SlotRefBody>() == SLOT_REF_BODY_SIZE);

// ── align helper ──────────────────────────────────────────────────────────────

#[inline]
const fn align4(n: u32) -> u32 {
    (n + 3) & !3
}

// ── writer ────────────────────────────────────────────────────────────────────

/// Write an inline frame to the producer.
///
/// Grants `align4(8 + payload.len())` bytes, writes the header and payload,
/// zeroes any padding bytes, then commits.
///
/// r[impl shm.framing.inline]
/// r[impl shm.framing.threshold]
pub fn write_inline(producer: &mut BipBufProducer<'_>, payload: &[u8]) -> Result<(), BipBufFull> {
    let entry_len = align4(8 + payload.len() as u32);
    let buf = producer.try_grant(entry_len).ok_or(BipBufFull)?;

    let payload_len_u16 = payload.len() as u16;
    let (hdr, rest) = FrameHeader::mut_from_prefix(buf).expect("buf alignment/size");
    hdr.total_len = entry_len;
    hdr.flags = 0;
    hdr._reserved0 = 0;
    hdr.inline_payload_len = payload_len_u16.to_le_bytes();

    rest[..payload.len()].copy_from_slice(payload);
    rest[payload.len()..].fill(0);

    producer.commit(entry_len);
    Ok(())
}

/// Write an mmap-ref frame to the producer.
///
/// Always 32 bytes: 8-byte header + 24-byte body.
///
/// r[impl shm.framing.mmap-ref]
pub fn write_mmap_ref(producer: &mut BipBufProducer<'_>, mmap: &MmapRef) -> Result<(), BipBufFull> {
    let buf = producer.try_grant(MMAP_REF_ENTRY_SIZE).ok_or(BipBufFull)?;

    let (hdr, rest) = FrameHeader::mut_from_prefix(buf).expect("buf alignment/size");
    hdr.total_len = MMAP_REF_ENTRY_SIZE;
    hdr.flags = FLAG_MMAP_REF;
    hdr._reserved0 = 0;
    hdr.inline_payload_len = [0; 2];

    MmapRefBody::write(rest, mmap);

    producer.commit(MMAP_REF_ENTRY_SIZE);
    Ok(())
}

/// Write a slot-ref frame to the producer.
///
/// Always 20 bytes: 8-byte header + 12-byte [`SlotRefBody`].
///
/// r[impl shm.framing.slot-ref]
pub fn write_slot_ref(producer: &mut BipBufProducer<'_>, slot: &SlotRef) -> Result<(), BipBufFull> {
    let buf = producer.try_grant(SLOT_REF_ENTRY_SIZE).ok_or(BipBufFull)?;

    let (hdr, rest) = FrameHeader::mut_from_prefix(buf).expect("buf alignment/size");
    hdr.total_len = SLOT_REF_ENTRY_SIZE;
    hdr.flags = FLAG_SLOT_REF;
    hdr._reserved0 = 0;
    hdr.inline_payload_len = [0; 2];

    let body = SlotRefBody::mut_from_bytes(rest).expect("rest alignment/size");
    body.class_idx = slot.class_idx;
    body.extent_idx = slot.extent_idx;
    body._reserved = [0; 2];
    body.slot_idx = slot.slot_idx;
    body.generation = slot.generation;

    producer.commit(SLOT_REF_ENTRY_SIZE);
    Ok(())
}

// ── reader ────────────────────────────────────────────────────────────────────

/// A parsed frame from a BipBuffer entry.
pub enum Frame<'a> {
    /// Inline payload. The slice is `data[8..total_len]` and may include up
    /// to 3 trailing zero-padding bytes. The roam wire format is
    /// self-delimiting, so trailing zeros are harmless.
    Inline(&'a [u8]),
    /// The payload lives in the VarSlotPool at the given slot reference.
    SlotRef(SlotRef),
    /// The payload lives in an external mmap region.
    MmapRef(MmapRef),
}

/// Parse the next frame from the front of `data`.
///
/// Returns `(frame, bytes_to_release)` on success, or `None` if `data` is
/// too short to contain a complete frame.  The caller must call
/// `consumer.release(bytes_to_release)` after processing the frame.
///
/// r[impl shm.framing.header]
pub fn peek_frame(data: &[u8]) -> Option<(Frame<'_>, u32)> {
    let (hdr, rest) = FrameHeader::ref_from_prefix(data).ok()?;
    let total_len = hdr.total_len as usize;

    if total_len < FRAME_HEADER_SIZE {
        return None;
    }
    if !total_len.is_multiple_of(4) {
        return None;
    }
    if data.len() < total_len {
        return None;
    }

    // r[impl shm.framing.flags]
    let both = FLAG_SLOT_REF | FLAG_MMAP_REF;
    if hdr.flags & both == both {
        return None; // invalid: both bits set
    }

    let frame = if hdr.flags & FLAG_SLOT_REF != 0 {
        if total_len < FRAME_HEADER_SIZE + SLOT_REF_BODY_SIZE {
            return None;
        }
        let (body, _) = SlotRefBody::ref_from_prefix(rest).ok()?;
        Frame::SlotRef(SlotRef {
            class_idx: body.class_idx,
            extent_idx: body.extent_idx,
            slot_idx: body.slot_idx,
            generation: body.generation,
        })
    } else if hdr.flags & FLAG_MMAP_REF != 0 {
        if total_len < FRAME_HEADER_SIZE + MMAP_REF_BODY_SIZE {
            return None;
        }
        Frame::MmapRef(MmapRefBody::read(rest)?)
    } else {
        // Use inline_payload_len if set (non-zero), otherwise fall back to total_len.
        let payload_len = u16::from_le_bytes(hdr.inline_payload_len) as usize;
        let end = if payload_len > 0 {
            let end = FRAME_HEADER_SIZE + payload_len;
            if end > total_len {
                return None; // corrupt: payload_len exceeds frame
            }
            end
        } else {
            // Legacy writer didn't set inline_payload_len — include padding.
            total_len
        };
        Frame::Inline(&data[FRAME_HEADER_SIZE..end])
    };

    Some((frame, hdr.total_len))
}

/// Convenience wrapper: read the next frame from `consumer`.
///
/// Calls `consumer.try_read()`, parses the frame header, then calls
/// `consumer.release()`.  Returns `None` if the ring is empty or the data
/// is too short (shouldn't happen with a well-behaved writer).
pub fn read_frame(consumer: &mut BipBufConsumer<'_>) -> Option<OwnedFrame> {
    let data = consumer.try_read()?;
    let (frame, consumed) = peek_frame(data)?;
    let owned = match frame {
        Frame::Inline(payload) => OwnedFrame::Inline(payload.to_vec()),
        Frame::SlotRef(slot) => OwnedFrame::SlotRef(slot),
        Frame::MmapRef(r) => OwnedFrame::MmapRef(r),
    };
    consumer.release(consumed);
    Some(owned)
}

/// An owned version of [`Frame`] returned by [`read_frame`].
#[derive(Debug)]
pub enum OwnedFrame {
    Inline(Vec<u8>),
    SlotRef(SlotRef),
    MmapRef(MmapRef),
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use shm_primitives::{BIPBUF_HEADER_SIZE, BipBuf, HeapRegion};

    fn make_bipbuf(capacity: usize) -> (HeapRegion, BipBuf) {
        let total = BIPBUF_HEADER_SIZE + capacity;
        let region = HeapRegion::new_zeroed(total);
        let bip = unsafe { BipBuf::init(region.region(), 0, capacity as u32) };
        (region, bip)
    }

    #[test]
    fn inline_roundtrip() {
        let (_region, bip) = make_bipbuf(1024);
        let (mut tx, mut rx) = bip.split();

        let payload = b"hello, world";
        write_inline(&mut tx, payload).unwrap();

        let frame = read_frame(&mut rx).unwrap();
        match frame {
            OwnedFrame::Inline(data) => {
                assert!(data.starts_with(payload));
            }
            _ => panic!("expected inline frame"),
        }
    }

    #[test]
    fn inline_alignment() {
        let (_region, bip) = make_bipbuf(1024);
        let (mut tx, mut rx) = bip.split();

        // Payload of 5 bytes → total_len should be align4(8+5) = 16
        write_inline(&mut tx, b"hello").unwrap();
        let (_, consumed) = peek_frame(rx.try_read().unwrap()).unwrap();
        assert_eq!(consumed, 16);
    }

    #[test]
    fn slot_ref_roundtrip() {
        let (_region, bip) = make_bipbuf(1024);
        let (mut tx, mut rx) = bip.split();

        let slot = SlotRef {
            class_idx: 2,
            extent_idx: 0,
            slot_idx: 7,
            generation: 42,
        };
        write_slot_ref(&mut tx, &slot).unwrap();

        let frame = read_frame(&mut rx).unwrap();
        match frame {
            OwnedFrame::SlotRef(s) => {
                assert_eq!(s.class_idx, 2);
                assert_eq!(s.slot_idx, 7);
                assert_eq!(s.generation, 42);
            }
            _ => panic!("expected slot-ref frame"),
        }
    }

    #[test]
    fn slot_ref_entry_size() {
        let (_region, bip) = make_bipbuf(1024);
        let (mut tx, mut rx) = bip.split();

        let slot = SlotRef {
            class_idx: 0,
            extent_idx: 0,
            slot_idx: 0,
            generation: 0,
        };
        write_slot_ref(&mut tx, &slot).unwrap();
        let (_, consumed) = peek_frame(rx.try_read().unwrap()).unwrap();
        assert_eq!(consumed, 20);
    }

    #[test]
    fn multiple_frames_sequential() {
        let (_region, bip) = make_bipbuf(1024);
        let (mut tx, mut rx) = bip.split();

        write_inline(&mut tx, b"first").unwrap();
        write_inline(&mut tx, b"second frame").unwrap();

        match read_frame(&mut rx).unwrap() {
            OwnedFrame::Inline(d) => assert!(d.starts_with(b"first")),
            _ => panic!(),
        }
        match read_frame(&mut rx).unwrap() {
            OwnedFrame::Inline(d) => assert!(d.starts_with(b"second frame")),
            _ => panic!(),
        }
        assert!(read_frame(&mut rx).is_none());
    }

    #[test]
    fn empty_payload() {
        let (_region, bip) = make_bipbuf(1024);
        let (mut tx, mut rx) = bip.split();

        write_inline(&mut tx, b"").unwrap();
        let (_, consumed) = peek_frame(rx.try_read().unwrap()).unwrap();
        // align4(8 + 0) = 8
        assert_eq!(consumed, 8);
    }

    #[test]
    fn mmap_ref_entry_size() {
        let (_region, bip) = make_bipbuf(1024);
        let (mut tx, mut rx) = bip.split();

        let mmap = MmapRef {
            map_id: 0,
            map_generation: 0,
            map_offset: 0,
            payload_len: 0,
        };
        write_mmap_ref(&mut tx, &mmap).unwrap();
        let (_, consumed) = peek_frame(rx.try_read().unwrap()).unwrap();
        assert_eq!(consumed, 32); // MMAP_REF_ENTRY_SIZE
    }

    #[test]
    fn mmap_ref_read_write() {
        let (_region, bip) = make_bipbuf(1024);
        let (mut tx, mut rx) = bip.split();

        let mmap = MmapRef {
            map_id: 5,
            map_generation: 11,
            map_offset: 4096,
            payload_len: 1024,
        };
        write_mmap_ref(&mut tx, &mmap).unwrap();

        match read_frame(&mut rx).unwrap() {
            OwnedFrame::MmapRef(r) => {
                assert_eq!(r.map_id, 5);
                assert_eq!(r.map_generation, 11);
                assert_eq!(r.map_offset, 4096);
                assert_eq!(r.payload_len, 1024);
            }
            _ => panic!("expected mmap-ref frame"),
        }
    }

    #[test]
    fn flags_both_set_rejected() {
        let (_region, bip) = make_bipbuf(1024);
        let (mut tx, mut rx) = bip.split();

        // Manually write a frame with both flag bits set (invalid).
        let entry_len: u32 = 32;
        let buf = tx.try_grant(entry_len).unwrap();
        let (hdr, rest) = FrameHeader::mut_from_prefix(buf).unwrap();
        hdr.total_len = entry_len;
        hdr.flags = FLAG_SLOT_REF | FLAG_MMAP_REF;
        hdr._reserved0 = 0;
        hdr.inline_payload_len = [0; 2];
        rest.fill(0);
        tx.commit(entry_len);

        // peek_frame must reject it
        let data = rx.try_read().unwrap();
        assert!(peek_frame(data).is_none());
    }

    #[test]
    fn total_len_smaller_than_header_rejected() {
        let data = [0_u8; 12];
        assert!(peek_frame(&data).is_none());
    }

    #[test]
    fn unaligned_total_len_rejected() {
        let mut data = [0_u8; 12];
        data[0] = 9; // total_len = 9 (not 4-byte aligned)
        assert!(peek_frame(&data).is_none());
    }

    #[test]
    fn ring_full_returns_err() {
        // Capacity just big enough for the header region but not a frame
        let (_region, bip) = make_bipbuf(8);
        let (mut tx, _rx) = bip.split();

        // First write succeeds (8 bytes fits exactly)
        write_inline(&mut tx, b"").unwrap();
        // Ring is now full
        assert!(write_inline(&mut tx, b"").is_err());
    }
}
