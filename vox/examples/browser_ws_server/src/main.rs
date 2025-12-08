//! WebSocket server for browser client testing.
//!
//! This server implements the Adder and Range services over WebSocket,
//! serving browser clients using rapace-wasm-client.
//!
//! # Running
//!
//! ```bash
//! cargo run --package browser-ws-server
//! ```
//!
//! Then open examples/browser_ws/index.html in a browser.

use std::net::SocketAddr;

use futures::{SinkExt, StreamExt};
use rapace_core::{Frame, FrameFlags, MsgDescHot, INLINE_PAYLOAD_SIZE};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;

const DESC_SIZE: usize = 64;
const _: () = assert!(std::mem::size_of::<MsgDescHot>() == DESC_SIZE);

// Method IDs matching the client
const METHOD_ADDER_ADD: u32 = 1;
const METHOD_RANGE: u32 = 2;

#[derive(facet::Facet)]
struct AdderRequest {
    a: i32,
    b: i32,
}

#[derive(facet::Facet)]
struct AdderResponse {
    result: i32,
}

#[derive(facet::Facet)]
struct RangeRequest {
    n: u32,
}

#[tokio::main]
async fn main() {
    let addr = "0.0.0.0:9000";
    let listener = TcpListener::bind(addr).await.expect("Failed to bind");
    println!(
        "WebSocket server listening on ws://127.0.0.1:9000 (bound to {})",
        addr
    );
    println!();
    println!("To test:");
    println!("  1. cd examples/browser_ws");
    println!("  2. wasm-pack build --target web ../../crates/rapace-wasm-client");
    println!("  3. python3 -m http.server 8080");
    println!("  4. Open http://localhost:8080 in a browser");

    while let Ok((stream, addr)) = listener.accept().await {
        println!("New connection from {}", addr);
        tokio::spawn(handle_connection(stream, addr));
    }
}

async fn handle_connection(stream: TcpStream, addr: SocketAddr) {
    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("WebSocket handshake failed for {}: {}", addr, e);
            return;
        }
    };

    let (mut sink, mut stream) = ws_stream.split();

    while let Some(msg) = stream.next().await {
        let msg = match msg {
            Ok(Message::Binary(data)) => data,
            Ok(Message::Close(_)) => {
                println!("Connection closed by {}", addr);
                break;
            }
            Ok(_) => continue, // Ignore ping/pong/text
            Err(e) => {
                eprintln!("Error receiving from {}: {}", addr, e);
                break;
            }
        };

        // Parse the frame
        if msg.len() < DESC_SIZE {
            eprintln!("Frame too small from {}", addr);
            continue;
        }

        let desc_bytes: [u8; DESC_SIZE] = msg[..DESC_SIZE].try_into().unwrap();
        let desc = bytes_to_desc(&desc_bytes);
        let payload = &msg[DESC_SIZE..];

        println!(
            "Received: channel={}, method={}, payload_len={}",
            desc.channel_id,
            desc.method_id,
            payload.len()
        );

        // Dispatch based on method_id
        match desc.method_id {
            METHOD_ADDER_ADD => {
                // Handle Adder::add
                match facet_postcard::from_bytes::<AdderRequest>(payload) {
                    Ok(req) => {
                        let result = req.a + req.b;
                        println!("  Adder: {} + {} = {}", req.a, req.b, result);

                        let response = AdderResponse { result };
                        let resp_payload = facet_postcard::to_vec(&response).unwrap();

                        let mut resp_desc = MsgDescHot::new();
                        resp_desc.msg_id = desc.msg_id;
                        resp_desc.channel_id = desc.channel_id;
                        resp_desc.method_id = desc.method_id;
                        resp_desc.flags = FrameFlags::DATA | FrameFlags::EOS;

                        let frame = if resp_payload.len() <= INLINE_PAYLOAD_SIZE {
                            Frame::with_inline_payload(resp_desc, &resp_payload).unwrap()
                        } else {
                            Frame::with_payload(resp_desc, resp_payload.clone())
                        };

                        let mut data = Vec::with_capacity(DESC_SIZE + frame.payload().len());
                        data.extend_from_slice(&desc_to_bytes(&frame.desc));
                        data.extend_from_slice(frame.payload());

                        if let Err(e) = sink.send(Message::Binary(data.into())).await {
                            eprintln!("Error sending to {}: {}", addr, e);
                            break;
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to decode AdderRequest: {}", e);
                        // Send error response
                        let mut resp_desc = MsgDescHot::new();
                        resp_desc.msg_id = desc.msg_id;
                        resp_desc.channel_id = desc.channel_id;
                        resp_desc.flags = FrameFlags::ERROR | FrameFlags::EOS;

                        let error_msg = format!("decode error: {}", e);
                        let frame = Frame::with_payload(resp_desc, error_msg.into_bytes());

                        let mut data = Vec::with_capacity(DESC_SIZE + frame.payload().len());
                        data.extend_from_slice(&desc_to_bytes(&frame.desc));
                        data.extend_from_slice(frame.payload());

                        let _ = sink.send(Message::Binary(data.into())).await;
                    }
                }
            }
            METHOD_RANGE => {
                // Handle Range::range (streaming)
                match facet_postcard::from_bytes::<RangeRequest>(payload) {
                    Ok(req) => {
                        println!("  Range: streaming 0..{}", req.n);

                        for i in 0..req.n {
                            let item_payload = facet_postcard::to_vec(&i).unwrap();

                            let mut item_desc = MsgDescHot::new();
                            item_desc.msg_id = desc.msg_id;
                            item_desc.channel_id = desc.channel_id;
                            item_desc.method_id = desc.method_id;
                            item_desc.flags = FrameFlags::DATA;

                            let frame = if item_payload.len() <= INLINE_PAYLOAD_SIZE {
                                Frame::with_inline_payload(item_desc, &item_payload).unwrap()
                            } else {
                                Frame::with_payload(item_desc, item_payload.clone())
                            };

                            let mut data = Vec::with_capacity(DESC_SIZE + frame.payload().len());
                            data.extend_from_slice(&desc_to_bytes(&frame.desc));
                            data.extend_from_slice(frame.payload());

                            if let Err(e) = sink.send(Message::Binary(data.into())).await {
                                eprintln!("Error sending stream item to {}: {}", addr, e);
                                break;
                            }
                        }

                        // Send EOS
                        let mut eos_desc = MsgDescHot::new();
                        eos_desc.msg_id = desc.msg_id;
                        eos_desc.channel_id = desc.channel_id;
                        eos_desc.method_id = desc.method_id;
                        eos_desc.flags = FrameFlags::EOS;

                        let frame = Frame::new(eos_desc);

                        let mut data = Vec::with_capacity(DESC_SIZE);
                        data.extend_from_slice(&desc_to_bytes(&frame.desc));

                        if let Err(e) = sink.send(Message::Binary(data.into())).await {
                            eprintln!("Error sending EOS to {}: {}", addr, e);
                        }

                        println!("  Range: stream complete");
                    }
                    Err(e) => {
                        eprintln!("Failed to decode RangeRequest: {}", e);
                    }
                }
            }
            _ => {
                eprintln!("Unknown method_id: {}", desc.method_id);
            }
        }
    }

    println!("Connection from {} closed", addr);
}

fn desc_to_bytes(desc: &MsgDescHot) -> [u8; DESC_SIZE] {
    unsafe { std::mem::transmute_copy(desc) }
}

fn bytes_to_desc(bytes: &[u8; DESC_SIZE]) -> MsgDescHot {
    unsafe { std::mem::transmute_copy(bytes) }
}
