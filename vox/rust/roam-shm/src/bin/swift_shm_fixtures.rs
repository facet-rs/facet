//! Generate SHM golden vectors for Swift runtime parity tests (v7 spec).

use roam_shm::framing::{
    DEFAULT_INLINE_THRESHOLD, FLAG_MMAP_REF, FLAG_SLOT_REF, FRAME_HEADER_SIZE, MMAP_REF_ENTRY_SIZE,
    SLOT_REF_ENTRY_SIZE,
};
use roam_shm::peer_table::{PEER_ENTRY_SIZE, PeerTable, bipbuf_pair_size};
use roam_shm::segment::{SegmentConfig, SegmentLayout};
use roam_shm::varslot::{SizeClassConfig, VarSlotPool};
use shm_primitives::{
    HeapRegion, MAGIC, SEGMENT_HEADER_SIZE, SEGMENT_VERSION, SegmentHeader, SegmentHeaderInit,
};
use std::fs;
use std::path::{Path, PathBuf};

fn put_u32_le(buf: &mut [u8], off: usize, value: u32) {
    buf[off..off + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64_le(buf: &mut [u8], off: usize, value: u64) {
    buf[off..off + 8].copy_from_slice(&value.to_le_bytes());
}

fn write_vector(dir: &Path, name: &str, bytes: &[u8]) {
    let path = dir.join(format!("{name}.bin"));
    fs::write(&path, bytes).expect("failed to write fixture");
    println!("Wrote {} bytes to {}", bytes.len(), path.display());
}

fn output_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-fixtures/golden-vectors/shm-v7")
}

fn main() {
    let out_dir = output_dir();
    fs::create_dir_all(&out_dir).expect("failed to create output directory");

    let size_classes = vec![
        SizeClassConfig {
            slot_size: 1024,
            slot_count: 4,
        },
        SizeClassConfig {
            slot_size: 16384,
            slot_count: 2,
        },
    ];
    let config = SegmentConfig {
        max_guests: 2,
        bipbuf_capacity: 128,
        max_payload_size: 1024,
        inline_threshold: DEFAULT_INLINE_THRESHOLD,
        heartbeat_interval: 1_000_000,
        size_classes: &size_classes,
    };
    let bipbuf_capacity = config.bipbuf_capacity;
    let layout = SegmentLayout::compute(config.max_guests, config.bipbuf_capacity, &size_classes);

    let heap = HeapRegion::new_zeroed(layout.total_size);
    let region = heap.region();
    let ring_base_offset =
        layout.peer_table_offset + usize::from(config.max_guests) * PEER_ENTRY_SIZE;

    let header: *mut SegmentHeader = unsafe { region.get_mut::<SegmentHeader>(0) };
    unsafe {
        (*header).init(SegmentHeaderInit {
            total_size: layout.total_size as u64,
            max_payload_size: config.max_payload_size,
            inline_threshold: config.inline_threshold,
            max_guests: u32::from(config.max_guests),
            bipbuf_capacity: config.bipbuf_capacity,
            peer_table_offset: layout.peer_table_offset as u64,
            var_pool_offset: layout.var_pool_offset as u64,
            heartbeat_interval: config.heartbeat_interval,
            num_var_slot_classes: size_classes.len() as u32,
        });
    }

    let _peer_table = unsafe {
        PeerTable::init(
            region,
            layout.peer_table_offset,
            config.max_guests,
            ring_base_offset,
            config.bipbuf_capacity,
        )
    };
    let _var_pool = unsafe { VarSlotPool::init(region, layout.var_pool_offset, &size_classes) };

    let mut segment_bytes =
        unsafe { std::slice::from_raw_parts(region.as_ptr() as *const u8, layout.total_size) }
            .to_vec();

    // Make peer #1 non-empty for parser parity checks.
    let peer1_off = layout.peer_table_offset;
    put_u32_le(&mut segment_bytes, peer1_off, 1); // Attached
    put_u32_le(&mut segment_bytes, peer1_off + 4, 7); // epoch
    put_u64_le(&mut segment_bytes, peer1_off + 8, 12_345_678); // heartbeat

    write_vector(&out_dir, "segment_layout", &segment_bytes);
    write_vector(
        &out_dir,
        "segment_header",
        &segment_bytes[0..SEGMENT_HEADER_SIZE],
    );

    // 8-byte frame header fixture
    let mut frame_header = [0u8; FRAME_HEADER_SIZE];
    put_u32_le(&mut frame_header, 0, 20); // slot-ref entry length
    frame_header[4] = FLAG_SLOT_REF;
    write_vector(&out_dir, "frame_header", &frame_header);

    // 12-byte slot-ref body
    let mut slot_ref = [0u8; 12];
    slot_ref[0] = 2; // class_idx
    slot_ref[1] = 1; // extent_idx
    put_u32_le(&mut slot_ref, 4, 42); // slot_idx
    put_u32_le(&mut slot_ref, 8, 7); // generation
    write_vector(&out_dir, "slot_ref", &slot_ref);

    // Inline frame: header(8) + payload + 0-padding to align4.
    let inline_payload = b"swift-shm";
    let inline_total_len = ((FRAME_HEADER_SIZE + inline_payload.len() + 3) & !3) as u32;
    let mut inline_frame = vec![0u8; inline_total_len as usize];
    put_u32_le(&mut inline_frame, 0, inline_total_len);
    inline_frame[4] = 0; // inline
    inline_frame[8..8 + inline_payload.len()].copy_from_slice(inline_payload);
    write_vector(&out_dir, "frame_inline", &inline_frame);

    // Slot-ref frame: 8-byte header + 12-byte slot-ref body.
    let mut slot_ref_frame = vec![0u8; SLOT_REF_ENTRY_SIZE as usize];
    put_u32_le(&mut slot_ref_frame, 0, SLOT_REF_ENTRY_SIZE);
    slot_ref_frame[4] = FLAG_SLOT_REF;
    slot_ref_frame[8..20].copy_from_slice(&slot_ref);
    write_vector(&out_dir, "frame_slot_ref", &slot_ref_frame);

    // Mmap-ref frame: 8-byte header + 24-byte mmap-ref body.
    let mut mmap_ref_frame = vec![0u8; MMAP_REF_ENTRY_SIZE as usize];
    put_u32_le(&mut mmap_ref_frame, 0, MMAP_REF_ENTRY_SIZE);
    mmap_ref_frame[4] = FLAG_MMAP_REF;
    put_u32_le(&mut mmap_ref_frame, 8, 9); // map_id
    put_u32_le(&mut mmap_ref_frame, 12, 3); // map_generation
    put_u64_le(&mut mmap_ref_frame, 16, 4096); // map_offset
    put_u32_le(&mut mmap_ref_frame, 24, 8192); // payload_len
    put_u32_le(&mut mmap_ref_frame, 28, 0); // reserved
    write_vector(&out_dir, "frame_mmap_ref", &mmap_ref_frame);

    println!("\nGenerated SHM v7 vectors in {}", out_dir.display());
    println!("Header magic: {:?}", MAGIC);
    println!("Header version: {}", SEGMENT_VERSION);
    println!("Peer entry size: {}", PEER_ENTRY_SIZE);
    println!(
        "Bipbuf pair size (cap={}): {}",
        bipbuf_capacity,
        bipbuf_pair_size(bipbuf_capacity)
    );
}
