//! rapace-wasm-client: WebAssembly client for rapace RPC.
//!
//! This crate provides a browser-compatible WebSocket client for rapace RPC.
//! It uses web_sys::WebSocket directly and exposes a JavaScript-friendly API
//! via wasm-bindgen.
//!
//! # Usage
//!
//! ```javascript
//! import init, { RapaceClient } from './rapace_wasm_client.js';
//!
//! await init();
//!
//! const client = await RapaceClient.connect('ws://localhost:9000');
//! const result = await client.call_adder(2, 3);
//! console.log('2 + 3 =', result);
//!
//! // Server streaming
//! const stream = client.call_range(5);
//! for await (const value of stream) {
//!     console.log('Got:', value);
//! }
//!
//! client.close();
//! ```

mod transport;

use std::cell::RefCell;
use std::rc::Rc;

use rapace_core::{Frame, FrameFlags, MsgDescHot, INLINE_PAYLOAD_SIZE, INLINE_PAYLOAD_SLOT};
use transport::WasmWebSocket;
use wasm_bindgen::prelude::*;

/// Size of MsgDescHot in bytes (must be 64).
const DESC_SIZE: usize = 64;
const _: () = assert!(std::mem::size_of::<MsgDescHot>() == DESC_SIZE);

/// A rapace RPC client for use in WebAssembly.
#[wasm_bindgen]
pub struct RapaceClient {
    ws: Rc<RefCell<WasmWebSocket>>,
    next_msg_id: u64,
    next_channel_id: u32,
}

#[wasm_bindgen]
impl RapaceClient {
    /// Connect to a rapace WebSocket server.
    ///
    /// Returns a Promise that resolves to a RapaceClient.
    #[wasm_bindgen]
    pub async fn connect(url: &str) -> Result<RapaceClient, JsValue> {
        let ws = WasmWebSocket::connect(url).await?;
        Ok(RapaceClient {
            ws: Rc::new(RefCell::new(ws)),
            next_msg_id: 1,
            next_channel_id: 1,
        })
    }

    /// Call the Adder service's add method.
    ///
    /// Returns a Promise<number>.
    #[wasm_bindgen]
    pub async fn call_adder(&mut self, a: i32, b: i32) -> Result<i32, JsValue> {
        // Encode request: AdderRequest { a, b }
        #[derive(facet::Facet)]
        struct AdderRequest {
            a: i32,
            b: i32,
        }

        let request = AdderRequest { a, b };
        let payload = facet_postcard::to_vec(&request)
            .map_err(|e| JsValue::from_str(&format!("encode error: {}", e)))?;

        // Build frame
        let channel_id = self.next_channel_id;
        self.next_channel_id += 1;

        let msg_id = self.next_msg_id;
        self.next_msg_id += 1;

        let mut desc = MsgDescHot::new();
        desc.msg_id = msg_id;
        desc.channel_id = channel_id;
        desc.method_id = 1; // AdderService::add method ID
        desc.flags = FrameFlags::DATA | FrameFlags::EOS;

        let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
            Frame::with_inline_payload(desc, &payload)
                .ok_or_else(|| JsValue::from_str("payload too large for inline"))?
        } else {
            Frame::with_payload(desc, payload.clone())
        };

        // Send request
        self.send_frame(&frame).await?;

        // Wait for response
        let response_frame = self.recv_frame().await?;

        // Check for error
        if response_frame.desc.flags.contains(FrameFlags::ERROR) {
            let error_msg = String::from_utf8_lossy(response_frame.payload()).to_string();
            return Err(JsValue::from_str(&error_msg));
        }

        // Decode response: AdderResponse { result }
        #[derive(facet::Facet)]
        struct AdderResponse {
            result: i32,
        }

        let response: AdderResponse = facet_postcard::from_bytes(response_frame.payload())
            .map_err(|e| JsValue::from_str(&format!("decode error: {}", e)))?;

        Ok(response.result)
    }

    /// Call the Range service to get a stream of numbers 0..n.
    ///
    /// Returns an async iterator that yields numbers.
    #[wasm_bindgen]
    pub fn call_range(&mut self, n: u32) -> RangeStream {
        let channel_id = self.next_channel_id;
        self.next_channel_id += 1;

        let msg_id = self.next_msg_id;
        self.next_msg_id += 1;

        RangeStream {
            ws: Rc::clone(&self.ws),
            channel_id,
            msg_id,
            n,
            started: false,
            finished: false,
        }
    }

    /// Close the connection.
    #[wasm_bindgen]
    pub fn close(&self) {
        self.ws.borrow().close();
    }

    async fn send_frame(&self, frame: &Frame) -> Result<(), JsValue> {
        let mut data = Vec::with_capacity(DESC_SIZE + frame.payload().len());
        data.extend_from_slice(&desc_to_bytes(&frame.desc));
        data.extend_from_slice(frame.payload());

        self.ws.borrow().send(&data).await
    }

    async fn recv_frame(&self) -> Result<Frame, JsValue> {
        let data = self.ws.borrow_mut().recv().await?;

        if data.len() < DESC_SIZE {
            return Err(JsValue::from_str(&format!(
                "frame too small: {} < {}",
                data.len(),
                DESC_SIZE
            )));
        }

        let desc_bytes: [u8; DESC_SIZE] = data[..DESC_SIZE].try_into().unwrap();
        let mut desc = bytes_to_desc(&desc_bytes);

        let payload = data[DESC_SIZE..].to_vec();
        desc.payload_len = payload.len() as u32;

        if payload.len() <= INLINE_PAYLOAD_SIZE {
            desc.payload_slot = INLINE_PAYLOAD_SLOT;
            desc.inline_payload[..payload.len()].copy_from_slice(&payload);
            Ok(Frame::new(desc))
        } else {
            Ok(Frame::with_payload(desc, payload))
        }
    }
}

/// Async iterator for Range streaming results.
#[wasm_bindgen]
pub struct RangeStream {
    ws: Rc<RefCell<WasmWebSocket>>,
    channel_id: u32,
    msg_id: u64,
    n: u32,
    started: bool,
    finished: bool,
}

#[wasm_bindgen]
impl RangeStream {
    /// Get the next value from the stream.
    ///
    /// Returns null when the stream is complete.
    #[wasm_bindgen]
    pub async fn next(&mut self) -> Result<JsValue, JsValue> {
        if self.finished {
            return Ok(JsValue::NULL);
        }

        // Send initial request if not started
        if !self.started {
            self.started = true;

            // Encode request: RangeRequest { n }
            #[derive(facet::Facet)]
            struct RangeRequest {
                n: u32,
            }

            let request = RangeRequest { n: self.n };
            let payload = facet_postcard::to_vec(&request)
                .map_err(|e| JsValue::from_str(&format!("encode error: {}", e)))?;

            let mut desc = MsgDescHot::new();
            desc.msg_id = self.msg_id;
            desc.channel_id = self.channel_id;
            desc.method_id = 2; // RangeService::range method ID
            desc.flags = FrameFlags::DATA | FrameFlags::EOS;

            let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
                Frame::with_inline_payload(desc, &payload)
                    .ok_or_else(|| JsValue::from_str("payload too large for inline"))?
            } else {
                Frame::with_payload(desc, payload.clone())
            };

            self.send_frame(&frame).await?;
        }

        // Receive next frame
        let frame = self.recv_frame().await?;

        // Check for error
        if frame.desc.flags.contains(FrameFlags::ERROR) {
            self.finished = true;
            let error_msg = String::from_utf8_lossy(frame.payload()).to_string();
            return Err(JsValue::from_str(&error_msg));
        }

        // Check for end of stream
        if frame.desc.flags.contains(FrameFlags::EOS) {
            self.finished = true;
            if frame.payload().is_empty() {
                return Ok(JsValue::NULL);
            }
        }

        // Decode streaming item (just a u32)
        let value: u32 = facet_postcard::from_bytes(frame.payload())
            .map_err(|e| JsValue::from_str(&format!("decode error: {}", e)))?;

        Ok(JsValue::from(value))
    }

    async fn send_frame(&self, frame: &Frame) -> Result<(), JsValue> {
        let mut data = Vec::with_capacity(DESC_SIZE + frame.payload().len());
        data.extend_from_slice(&desc_to_bytes(&frame.desc));
        data.extend_from_slice(frame.payload());

        self.ws.borrow().send(&data).await
    }

    async fn recv_frame(&mut self) -> Result<Frame, JsValue> {
        let data = self.ws.borrow_mut().recv().await?;

        if data.len() < DESC_SIZE {
            return Err(JsValue::from_str(&format!(
                "frame too small: {} < {}",
                data.len(),
                DESC_SIZE
            )));
        }

        let desc_bytes: [u8; DESC_SIZE] = data[..DESC_SIZE].try_into().unwrap();
        let mut desc = bytes_to_desc(&desc_bytes);

        let payload = data[DESC_SIZE..].to_vec();
        desc.payload_len = payload.len() as u32;

        if payload.len() <= INLINE_PAYLOAD_SIZE {
            desc.payload_slot = INLINE_PAYLOAD_SLOT;
            desc.inline_payload[..payload.len()].copy_from_slice(&payload);
            Ok(Frame::new(desc))
        } else {
            Ok(Frame::with_payload(desc, payload))
        }
    }
}

/// Convert MsgDescHot to raw bytes.
fn desc_to_bytes(desc: &MsgDescHot) -> [u8; DESC_SIZE] {
    // SAFETY: MsgDescHot is repr(C), Copy, and exactly 64 bytes.
    unsafe { std::mem::transmute_copy(desc) }
}

/// Convert raw bytes to MsgDescHot.
fn bytes_to_desc(bytes: &[u8; DESC_SIZE]) -> MsgDescHot {
    // SAFETY: Same as desc_to_bytes.
    unsafe { std::mem::transmute_copy(bytes) }
}
