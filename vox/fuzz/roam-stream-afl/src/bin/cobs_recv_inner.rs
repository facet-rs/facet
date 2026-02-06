use afl::fuzz;
use cobs::encode_vec as cobs_encode_vec;
use roam_stream::try_decode_one_from_buffer_for_fuzz;
use roam_wire::{ConnectionId, Message};

fn drive_stateful(stream: &[u8], chunk_a: usize, chunk_b: usize, max_steps: usize) {
    let mut in_pos = 0usize;
    let mut chunk_ix = 0usize;
    let chunk_pattern = [chunk_a.max(1), chunk_b.max(1)];

    let mut recv_buf = Vec::new();
    let mut unread_start = 0usize;
    let mut scan_from = 0usize;
    let mut last_decoded = Vec::new();

    for _ in 0..max_steps {
        match try_decode_one_from_buffer_for_fuzz(
            &mut recv_buf,
            &mut unread_start,
            &mut scan_from,
            &mut last_decoded,
        ) {
            Ok(Some(_)) => {
                continue;
            }
            Ok(None) => {
                if in_pos >= stream.len() {
                    return;
                }
                let chunk = chunk_pattern[chunk_ix % 2];
                chunk_ix += 1;
                let end = (in_pos + chunk).min(stream.len());
                recv_buf.extend_from_slice(&stream[in_pos..end]);
                in_pos = end;
            }
            Err(_) => return,
        }
    }
}

fn build_valid_stream(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return vec![0x00];
    }

    let mut out = Vec::new();
    let mut cursor = 0usize;
    let mut req_id = 1u64;

    while cursor < data.len() && req_id <= 16 {
        let remaining = data.len() - cursor;
        let take = remaining.min(4096);
        let payload = data[cursor..cursor + take].to_vec();
        cursor += take;

        let msg = Message::Response {
            conn_id: ConnectionId::ROOT,
            request_id: req_id,
            metadata: vec![],
            channels: vec![],
            payload,
        };
        req_id += 1;

        let Ok(postcard) = facet_postcard::to_vec(&msg) else {
            break;
        };
        let mut framed = cobs_encode_vec(&postcard);
        framed.push(0x00);
        out.extend_from_slice(&framed);
    }

    out
}

fn main() {
    fuzz!(|data: &[u8]| {
        if data.len() < 3 {
            return;
        }

        let mode = data[0];
        let chunk_a = (data[1] as usize % 4096) + 1;
        let chunk_b = (data[2] as usize % 4096) + 1;
        let body = &data[3..];

        // Path 1: raw inbound bytes.
        drive_stateful(body, chunk_a, chunk_b, 20_000);

        // Path 2: grammar-ish valid stream composed from input bytes.
        if (mode & 1) != 0 {
            let valid = build_valid_stream(body);
            drive_stateful(&valid, chunk_b, chunk_a, 20_000);
        }
    });
}
