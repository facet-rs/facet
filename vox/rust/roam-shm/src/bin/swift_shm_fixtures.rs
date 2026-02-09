//! Generate SHM golden vectors for Swift runtime parity tests.

use roam_frame::{
    SHM_FRAME_HEADER_SIZE, SLOT_REF_FRAME_SIZE, ShmFrameHeader, SlotRef, encode_inline_frame,
    encode_slot_ref_frame,
};
use roam_shm::layout::{HEADER_SIZE, MAGIC, SegmentConfig, VERSION};
use std::fs;
use std::path::Path;

fn put_u32_le(buf: &mut [u8], off: usize, value: u32) {
    buf[off..off + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64_le(buf: &mut [u8], off: usize, value: u64) {
    buf[off..off + 8].copy_from_slice(&value.to_le_bytes());
}

fn write_vector(dir: &Path, name: &str, bytes: &[u8]) {
    let path = dir.join(format!("{}.bin", name));
    fs::write(&path, bytes).expect("failed to write fixture");
    println!("Wrote {} bytes to {}", bytes.len(), path.display());
}

fn main() {
    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-fixtures/golden-vectors/shm");

    fs::create_dir_all(&out_dir).expect("failed to create output directory");
    println!("Writing SHM vectors to {}\n", out_dir.display());

    let config = SegmentConfig {
        max_payload_size: 1024,
        initial_credit: 1024,
        max_guests: 2,
        bipbuf_capacity: 128,
        inline_threshold: 256,
        max_channels: 8,
        heartbeat_interval: 1_000_000,
        var_slot_classes: vec![roam_shm::layout::SizeClass::new(1024, 4)],
        file_cleanup: shm_primitives::FileCleanup::Manual,
    };
    let layout = config.layout().expect("layout should be valid");

    let mut segment = vec![0u8; layout.total_size as usize];

    // SegmentHeader (128 bytes) at offset 0
    segment[0..8].copy_from_slice(&MAGIC);
    put_u32_le(&mut segment, 8, VERSION);
    put_u32_le(&mut segment, 12, HEADER_SIZE as u32);
    put_u64_le(&mut segment, 16, layout.total_size);
    put_u32_le(&mut segment, 24, config.max_payload_size);
    put_u32_le(&mut segment, 28, config.initial_credit);
    put_u32_le(&mut segment, 32, config.max_guests);
    put_u32_le(&mut segment, 36, config.bipbuf_capacity);
    put_u64_le(&mut segment, 40, layout.peer_table_offset);
    put_u64_le(&mut segment, 48, 0);
    put_u32_le(&mut segment, 56, 0);
    put_u32_le(&mut segment, 60, config.inline_threshold);
    put_u32_le(&mut segment, 64, config.max_channels);
    put_u32_le(&mut segment, 68, 0);
    put_u64_le(&mut segment, 72, config.heartbeat_interval);
    put_u64_le(&mut segment, 80, layout.var_slot_pool_offset);
    put_u64_le(&mut segment, 88, layout.total_size);
    put_u64_le(&mut segment, 96, layout.guest_areas_offset);
    put_u32_le(&mut segment, 104, config.var_slot_classes.len() as u32);

    let peer1_off = layout.peer_entry_offset(1) as usize;
    // Peer state: Attached, epoch: 7, heartbeat: 12345678
    put_u32_le(&mut segment, peer1_off, 1);
    put_u32_le(&mut segment, peer1_off + 4, 7);
    put_u64_le(&mut segment, peer1_off + 24, 12_345_678);
    put_u64_le(
        &mut segment,
        peer1_off + 32,
        layout.guest_to_host_bipbuf_offset(1),
    );
    put_u64_le(&mut segment, peer1_off + 40, 0);
    put_u64_le(
        &mut segment,
        peer1_off + 48,
        layout.guest_channel_table_offset(1),
    );

    // Channel entry 1: Active, granted_total=4096
    let ch1_off = layout.guest_channel_table_offset(1) as usize + 16;
    put_u32_le(&mut segment, ch1_off, 1);
    put_u32_le(&mut segment, ch1_off + 4, 4096);

    write_vector(&out_dir, "segment_layout", &segment);
    write_vector(&out_dir, "segment_header", &segment[0..HEADER_SIZE]);

    let frame_header = ShmFrameHeader {
        total_len: 28,
        msg_type: 1,
        flags: 0,
        id: 99,
        method_id: 0x1234_5678_9ABC_DEF0,
        payload_len: 4,
    };
    let mut header_buf = [0u8; SHM_FRAME_HEADER_SIZE];
    frame_header.write_to(&mut header_buf);
    write_vector(&out_dir, "frame_header", &header_buf);

    let slot_ref = SlotRef {
        class_idx: 2,
        extent_idx: 1,
        slot_idx: 42,
        slot_generation: 7,
    };
    let mut slot_ref_buf = [0u8; 12];
    slot_ref.write_to(&mut slot_ref_buf);
    write_vector(&out_dir, "slot_ref", &slot_ref_buf);

    let mut inline_buf = [0u8; 64];
    let inline_len = encode_inline_frame(1, 99, 0x42, b"swift-shm", &mut inline_buf);
    write_vector(&out_dir, "frame_inline", &inline_buf[..inline_len]);

    let mut slot_ref_frame_buf = [0u8; SLOT_REF_FRAME_SIZE];
    let slot_ref_frame_len =
        encode_slot_ref_frame(4, 7, 0, 8192, &slot_ref, &mut slot_ref_frame_buf);
    write_vector(
        &out_dir,
        "frame_slot_ref",
        &slot_ref_frame_buf[..slot_ref_frame_len],
    );
}
