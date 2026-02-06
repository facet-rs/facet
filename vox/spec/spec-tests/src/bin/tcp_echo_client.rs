//! Rust TCP client for cross-language testing.
//!
//! Connects to a TCP server, performs Hello exchange, and runs Echo tests.

use std::env;
use std::io::{Read, Write};
use std::net::TcpStream;

fn main() {
    let addr = env::var("SERVER_ADDR").unwrap_or_else(|_| "127.0.0.1:9000".to_string());

    eprintln!("Connecting to {}...", addr);
    let mut stream = TcpStream::connect(&addr).expect("Failed to connect");
    eprintln!("Connected! Running tests...");

    // Send Hello
    let mut hello = Vec::new();
    write_varint(&mut hello, 0); // Message::Hello
    write_varint(&mut hello, 3); // Hello::V4 (discriminant 3)
    write_varint(&mut hello, 1024 * 1024); // max_payload
    write_varint(&mut hello, 64 * 1024); // initial_channel_credit
    write_message(&mut stream, &hello).expect("Failed to send hello");

    // Read Hello from server
    let _hello_msg = read_message(&mut stream).expect("Failed to read hello");

    // Test 1: Echo
    let result = call_method(&mut stream, 0x3d66dd9ee36b4240, "Hello, World!");
    assert_eq!(result, "Hello, World!", "Echo failed");
    eprintln!("Echo: PASS");

    // Test 2: Reverse
    let result = call_method(&mut stream, 0x268246d3219503fb, "Hello, World!");
    assert_eq!(result, "!dlroW ,olleH", "Reverse failed");
    eprintln!("Reverse: PASS");

    // Test 3: Echo with unicode
    let result = call_method(&mut stream, 0x3d66dd9ee36b4240, "こんにちは世界");
    assert_eq!(result, "こんにちは世界", "Echo unicode failed");
    eprintln!("Echo unicode: PASS");

    // Test 4: Reverse with unicode
    let result = call_method(&mut stream, 0x268246d3219503fb, "こんにちは世界");
    assert_eq!(result, "界世はちにんこ", "Reverse unicode failed");
    eprintln!("Reverse unicode: PASS");

    println!("All tests passed!");
}

fn write_varint(buf: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn read_varint(buf: &[u8], off: &mut usize) -> u64 {
    let mut result: u64 = 0;
    let mut shift = 0;
    loop {
        let byte = buf[*off];
        *off += 1;
        result |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    result
}

fn write_message(stream: &mut TcpStream, payload: &[u8]) -> Result<(), String> {
    let len = u32::try_from(payload.len())
        .map_err(|_| "frame too large".to_string())?
        .to_le_bytes();
    stream.write_all(&len).map_err(|e| e.to_string())?;
    stream.write_all(payload).map_err(|e| e.to_string())
}

fn read_message(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut len = [0u8; 4];
    stream.read_exact(&mut len).map_err(|e| e.to_string())?;
    let frame_len = u32::from_le_bytes(len) as usize;

    let mut frame = vec![0u8; frame_len];
    stream.read_exact(&mut frame).map_err(|e| e.to_string())?;
    Ok(frame)
}

static mut REQUEST_ID: u64 = 1;

fn call_method(stream: &mut TcpStream, method_id: u64, message: &str) -> String {
    let request_id = unsafe {
        let id = REQUEST_ID;
        REQUEST_ID += 1;
        id
    };

    // Build payload first (postcard-encoded string: varint length + bytes)
    let msg_bytes = message.as_bytes();
    let mut payload = Vec::new();
    write_varint(&mut payload, msg_bytes.len() as u64);
    payload.extend_from_slice(msg_bytes);

    // Build request
    let mut request = Vec::new();
    write_varint(&mut request, 2); // Message::Request
    write_varint(&mut request, 0); // conn_id
    write_varint(&mut request, request_id);
    write_varint(&mut request, method_id);
    write_varint(&mut request, 0); // metadata length = 0
    write_varint(&mut request, 0); // channels length = 0
    write_varint(&mut request, payload.len() as u64); // payload length
    request.extend_from_slice(&payload);

    write_message(stream, &request).expect("Failed to send request");

    // Read response
    let response = read_message(stream).expect("Failed to read response");

    // Parse response
    let mut off = 0;
    let msg_type = read_varint(&response, &mut off);
    assert_eq!(msg_type, 3, "Expected Response message");

    let _conn_id = read_varint(&response, &mut off);
    let resp_id = read_varint(&response, &mut off);
    assert_eq!(resp_id, request_id, "Request ID mismatch");

    let md_len = read_varint(&response, &mut off);
    // Skip metadata
    for _ in 0..md_len {
        let key_len = read_varint(&response, &mut off) as usize;
        off += key_len;
        let value_disc = read_varint(&response, &mut off);
        match value_disc {
            0 => {
                let s_len = read_varint(&response, &mut off) as usize;
                off += s_len;
            }
            1 => {
                let b_len = read_varint(&response, &mut off) as usize;
                off += b_len;
            }
            2 => {
                let _ = read_varint(&response, &mut off);
            }
            _ => panic!("Unknown metadata value type"),
        }
        let _flags = read_varint(&response, &mut off);
    }

    let channels_len = read_varint(&response, &mut off) as usize;
    for _ in 0..channels_len {
        let _ = read_varint(&response, &mut off);
    }

    let _payload_len = read_varint(&response, &mut off);

    // Result tag
    let result_tag = read_varint(&response, &mut off);
    assert_eq!(result_tag, 0, "Expected Ok result");

    // Read string
    let str_len = read_varint(&response, &mut off) as usize;
    String::from_utf8(response[off..off + str_len].to_vec()).expect("Invalid UTF-8")
}
