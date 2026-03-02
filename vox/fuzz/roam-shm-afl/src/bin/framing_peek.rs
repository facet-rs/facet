use afl::fuzz;
use roam_shm::framing::{self, Frame};

fn main() {
    fuzz!(|data: &[u8]| {
        if let Some((frame, consumed)) = framing::peek_frame(data) {
            assert!((consumed as usize) <= data.len());
            match frame {
                Frame::Inline(bytes) => {
                    let _ = bytes.len();
                }
                Frame::SlotRef(slot) => {
                    let _ = (
                        slot.class_idx,
                        slot.extent_idx,
                        slot.slot_idx,
                        slot.generation,
                    );
                }
                Frame::MmapRef(mmap) => {
                    let _ = (
                        mmap.map_id,
                        mmap.map_generation,
                        mmap.map_offset,
                        mmap.payload_len,
                    );
                }
            }
        }
    });
}
