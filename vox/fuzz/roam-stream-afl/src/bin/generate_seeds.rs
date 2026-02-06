use std::fs;
use std::io;
use std::path::PathBuf;

use cobs::encode_vec as cobs_encode_vec;
use roam_wire::{ConnectionId, Message, MetadataValue};

fn frame_message(msg: &Message) -> io::Result<Vec<u8>> {
    let postcard = facet_postcard::to_vec(msg)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("postcard: {e}")))?;
    let mut framed = cobs_encode_vec(&postcard);
    framed.push(0x00);
    Ok(framed)
}

fn write_seed(name: &str, bytes: &[u8]) -> io::Result<()> {
    let out_dir = PathBuf::from("fuzz/roam-stream-afl/in");
    fs::create_dir_all(&out_dir)?;
    fs::write(out_dir.join(name), bytes)
}

fn main() -> io::Result<()> {
    // Delimiter-only and tiny frame edge cases.
    write_seed("tiny-delimiter", &[0x00])?;
    write_seed("double-delimiter", &[0x00, 0x00])?;
    write_seed("short-garbage-delimited", &[0xff, 0x10, 0x00, 0xab, 0x00])?;

    // 32 KiB payload with many zero bytes after postcard decode.
    let large_zeros = vec![0u8; 32 * 1024];
    let response_zeros = Message::Response {
        conn_id: ConnectionId::ROOT,
        request_id: 1,
        metadata: vec![],
        channels: vec![],
        payload: large_zeros,
    };
    write_seed("response-32k-zeros", &frame_message(&response_zeros)?)?;

    // 64 KiB patterned payload with frequent 0x00 delimitable patterns.
    let large_pattern: Vec<u8> = (0..(64 * 1024))
        .map(|i| {
            if i % 17 == 0 {
                0
            } else {
                (i as u8).wrapping_mul(37)
            }
        })
        .collect();
    let response_pattern = Message::Response {
        conn_id: ConnectionId::ROOT,
        request_id: 2,
        metadata: vec![],
        channels: vec![],
        payload: large_pattern,
    };
    write_seed("response-64k-pattern", &frame_message(&response_pattern)?)?;

    // Data frame with large payload.
    let data_payload: Vec<u8> = (0..(96 * 1024))
        .map(|i| (i as u8).rotate_left((i % 7) as u32))
        .collect();
    let data_msg = Message::Data {
        conn_id: ConnectionId::ROOT,
        channel_id: 7,
        payload: data_payload,
    };
    write_seed("data-96k-pattern", &frame_message(&data_msg)?)?;

    // Rich metadata message to exercise string/bytes parsing paths.
    let metadata_msg = Message::Response {
        conn_id: ConnectionId::ROOT,
        request_id: 3,
        metadata: vec![
            (
                "x-large".into(),
                MetadataValue::String("a".repeat(8 * 1024)),
                0,
            ),
            (
                "blob".into(),
                MetadataValue::Bytes((0..4096).map(|i| (i % 251) as u8).collect()),
                0,
            ),
        ],
        channels: vec![1, 2, 3, 4],
        payload: vec![0, 1, 2, 3, 4, 0, 5, 6],
    };
    write_seed("response-heavy-metadata", &frame_message(&metadata_msg)?)?;

    Ok(())
}
