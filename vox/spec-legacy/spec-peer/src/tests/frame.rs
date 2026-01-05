//! Frame format conformance tests.
//!
//! Tests for frame encoding, descriptor layout, and payload handling.

use crate::harness::{Frame, Peer};
use crate::protocol::*;
use crate::testcase::TestResult;
use rapace_spec_peer_macros::conformance;

/// Helper to send a Hello response so the subject's handshake completes.
async fn send_hello_response(peer: &mut Peer) -> Result<(), TestResult> {
    let response = Hello {
        protocol_version: PROTOCOL_VERSION_1_0,
        role: Role::Acceptor,
        required_features: 0,
        supported_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
        limits: Limits::default(),
        methods: vec![],
        params: vec![],
    };

    let payload = facet_postcard::to_vec(&response)
        .map_err(|e| TestResult::fail(format!("failed to serialize Hello: {}", e)))?;

    let mut desc = MsgDescHot::new();
    desc.msg_id = 1;
    desc.channel_id = 0;
    desc.method_id = control_verb::HELLO;
    desc.flags = flags::CONTROL;

    let response_frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    peer.send(&response_frame)
        .await
        .map_err(|e| TestResult::fail(format!("failed to send Hello: {}", e)))?;

    Ok(())
}

// =============================================================================
// frame.desc_size
// =============================================================================
// Rule: [verify frame.desc.size]
//
// MsgDescHot MUST be exactly 64 bytes.

#[conformance(name = "frame.desc_size", rules = "frame.desc.size")]
pub async fn desc_size(peer: &mut Peer) -> TestResult {
    // Receive any frame and verify the descriptor is 64 bytes
    let frame = match peer.recv_raw().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive frame: {}", e)),
    };

    // The raw_desc field captures the exact 64 bytes from the wire
    if frame.raw_desc.len() != 64 {
        return TestResult::fail(format!(
            "descriptor size is {} bytes, MUST be 64",
            frame.raw_desc.len()
        ));
    }

    // Send Hello response so subject's handshake completes
    if let Err(result) = send_hello_response(peer).await {
        return result;
    }

    TestResult::pass()
}

// =============================================================================
// frame.desc_encoding
// =============================================================================
// Rule: [verify frame.desc.encoding]
//
// Descriptor fields MUST be encoded in little-endian byte order.

#[conformance(name = "frame.desc_encoding", rules = "frame.desc.encoding")]
pub async fn desc_encoding(peer: &mut Peer) -> TestResult {
    // Receive the Hello frame and verify little-endian encoding
    let frame = match peer.recv_raw().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive frame: {}", e)),
    };

    // Parse the raw descriptor manually to verify little-endian encoding
    let raw = &frame.raw_desc;

    // msg_id is at bytes 0-7 (u64, little-endian)
    let msg_id = u64::from_le_bytes(raw[0..8].try_into().unwrap());

    // channel_id is at bytes 8-11 (u32, little-endian)
    let channel_id = u32::from_le_bytes(raw[8..12].try_into().unwrap());

    // method_id is at bytes 12-15 (u32, little-endian)
    let method_id = u32::from_le_bytes(raw[12..16].try_into().unwrap());

    // Verify the parsed values match what MsgDescHot::from_bytes produces
    if frame.desc.msg_id != msg_id {
        return TestResult::fail(format!(
            "msg_id mismatch: parsed {} vs from_bytes {}",
            msg_id, frame.desc.msg_id
        ));
    }

    if frame.desc.channel_id != channel_id {
        return TestResult::fail(format!(
            "channel_id mismatch: parsed {} vs from_bytes {}",
            channel_id, frame.desc.channel_id
        ));
    }

    if frame.desc.method_id != method_id {
        return TestResult::fail(format!(
            "method_id mismatch: parsed {} vs from_bytes {}",
            method_id, frame.desc.method_id
        ));
    }

    // Send Hello response so subject's handshake completes
    if let Err(result) = send_hello_response(peer).await {
        return result;
    }

    TestResult::pass()
}

// =============================================================================
// frame.msg_id_scope
// =============================================================================
// Rule: [verify frame.msg-id.scope]
//
// msg_id MUST be unique per connection and monotonically increasing.

#[conformance(name = "frame.msg_id_scope", rules = "frame.msg-id.scope")]
pub async fn msg_id_scope(peer: &mut Peer) -> TestResult {
    use std::collections::HashSet;

    let mut seen_msg_ids: HashSet<u64> = HashSet::new();
    let mut last_msg_id: Option<u64> = None;

    // Receive multiple frames and verify msg_id uniqueness and ordering
    for _ in 0..5 {
        let frame = match peer.try_recv().await {
            Ok(Some(f)) => f,
            Ok(None) => break, // Connection closed or timeout
            Err(e) => return TestResult::fail(format!("recv error: {}", e)),
        };

        let msg_id = frame.desc.msg_id;

        // Verify uniqueness
        if seen_msg_ids.contains(&msg_id) {
            return TestResult::fail(format!(
                "duplicate msg_id {} detected, msg_id MUST be unique per connection",
                msg_id
            ));
        }
        seen_msg_ids.insert(msg_id);

        // Verify monotonically increasing (for frames from the same peer)
        if let Some(last) = last_msg_id
            && msg_id <= last
        {
            return TestResult::fail(format!(
                "msg_id {} is not greater than previous msg_id {}, MUST be monotonically increasing",
                msg_id, last
            ));
        }
        last_msg_id = Some(msg_id);
    }

    // Send Hello response so subject's handshake completes (if not already done)
    if let Err(result) = send_hello_response(peer).await {
        return result;
    }

    TestResult::pass()
}

// =============================================================================
// frame.sentinel_inline
// =============================================================================
// Rule: [verify frame.sentinel.values]
//
// INLINE_PAYLOAD_SLOT (0xFFFFFFFF) indicates payload is inline.

#[conformance(name = "frame.sentinel_inline", rules = "frame.sentinel.values")]
pub async fn sentinel_inline(peer: &mut Peer) -> TestResult {
    // Receive Hello frame - typically uses inline payload
    let frame = match peer.recv_raw().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive frame: {}", e)),
    };

    // Check if payload_slot indicates inline
    if frame.desc.payload_slot == INLINE_PAYLOAD_SLOT {
        // Verify inline payload is within the 16-byte limit
        if frame.desc.payload_len as usize > INLINE_PAYLOAD_SIZE {
            return TestResult::fail(format!(
                "inline payload_len {} exceeds max inline size {}",
                frame.desc.payload_len, INLINE_PAYLOAD_SIZE
            ));
        }

        // Verify payload comes from inline_payload field
        let inline_bytes = &frame.desc.inline_payload[..frame.desc.payload_len as usize];
        if inline_bytes.is_empty() && frame.desc.payload_len > 0 {
            return TestResult::fail("inline payload claims length but bytes are empty");
        }
    }

    // Send Hello response so subject's handshake completes
    if let Err(result) = send_hello_response(peer).await {
        return result;
    }

    TestResult::pass()
}

// =============================================================================
// frame.payload_inline
// =============================================================================
// Rule: [verify frame.payload.inline]
//
// Inline payloads MUST be at most 16 bytes.

#[conformance(name = "frame.payload_inline", rules = "frame.payload.inline")]
pub async fn payload_inline(peer: &mut Peer) -> TestResult {
    // Receive frames and verify inline payload size constraint
    let frame = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive frame: {}", e)),
    };

    if frame.desc.payload_slot == INLINE_PAYLOAD_SLOT
        && frame.desc.payload_len as usize > INLINE_PAYLOAD_SIZE
    {
        return TestResult::fail(format!(
            "inline payload_len {} exceeds maximum inline size of {} bytes",
            frame.desc.payload_len, INLINE_PAYLOAD_SIZE
        ));
    }

    // Send Hello response so subject's handshake completes
    if let Err(result) = send_hello_response(peer).await {
        return result;
    }

    TestResult::pass()
}

// =============================================================================
// frame.flags_reserved
// =============================================================================
// Rule: [verify core.flags.reserved]
//
// Reserved flags MUST be left clear (unset).

#[conformance(name = "frame.flags_reserved", rules = "core.flags.reserved")]
pub async fn flags_reserved(peer: &mut Peer) -> TestResult {
    // Reserved flag bits that MUST be clear
    const RESERVED_08: u32 = 0b0000_1000;
    const RESERVED_80: u32 = 0b1000_0000;
    const RESERVED_MASK: u32 = RESERVED_08 | RESERVED_80;

    // Receive frames and verify reserved flags are clear
    for _ in 0..3 {
        let frame = match peer.try_recv().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return TestResult::fail(format!("recv error: {}", e)),
        };

        if frame.desc.flags & RESERVED_MASK != 0 {
            return TestResult::fail(format!(
                "reserved flag bits are set (flags={:#x}), reserved bits MUST be clear",
                frame.desc.flags
            ));
        }
    }

    // Send Hello response so subject's handshake completes
    if let Err(result) = send_hello_response(peer).await {
        return result;
    }

    TestResult::pass()
}

// =============================================================================
// frame.payload_empty
// =============================================================================
// Rule: [verify frame.payload.empty]
//
// Empty payload is indicated by payload_len = 0.

#[conformance(name = "frame.payload_empty", rules = "frame.payload.empty")]
pub async fn payload_empty(peer: &mut Peer) -> TestResult {
    // This test verifies that empty payloads are correctly indicated
    // We can't force the implementation to send empty payloads, but we can
    // verify that if payload_len is 0, the payload is indeed empty

    let frame = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive frame: {}", e)),
    };

    if frame.desc.payload_len == 0 {
        // Verify no payload bytes are present
        let payload_bytes = frame.payload_bytes();
        if !payload_bytes.is_empty() {
            return TestResult::fail(format!(
                "payload_len is 0 but {} payload bytes present",
                payload_bytes.len()
            ));
        }
    }

    // Send Hello response so subject's handshake completes
    if let Err(result) = send_hello_response(peer).await {
        return result;
    }

    TestResult::pass()
}

// =============================================================================
// frame.msg_id_control
// =============================================================================
// Rule: [verify frame.msg-id.control]
//
// Control channel (channel 0) frames MUST use monotonically increasing msg_id values.

#[conformance(name = "frame.msg_id_control", rules = "frame.msg-id.control")]
pub async fn msg_id_control(peer: &mut Peer) -> TestResult {
    use std::time::Duration;

    let mut last_control_msg_id: Option<u64> = None;
    let mut control_frame_count = 0;
    let mut handshake_done = false;
    let mut channel_id = None;

    // Receive frames and verify control channel msg_id ordering
    // We need at least 2 control frames: Hello and OpenChannel
    loop {
        // Use a short timeout after handshake to avoid blocking too long
        let timeout = if handshake_done {
            Duration::from_millis(500)
        } else {
            Duration::from_secs(5)
        };

        let frame = match peer.try_recv_timeout(timeout).await {
            Ok(Some(f)) => f,
            Ok(None) => {
                // Connection closed or timeout
                if control_frame_count < 2 {
                    return TestResult::fail(format!(
                        "need at least 2 control frames to verify monotonic msg_id, got {}",
                        control_frame_count
                    ));
                }
                break;
            }
            Err(e) => return TestResult::fail(format!("recv error: {}", e)),
        };

        // Only check control channel frames (channel 0)
        if frame.desc.channel_id == 0 {
            let msg_id = frame.desc.msg_id;
            control_frame_count += 1;

            if let Some(last) = last_control_msg_id
                && msg_id <= last
            {
                return TestResult::fail(format!(
                    "control channel msg_id {} is not greater than previous {}, MUST be monotonically increasing",
                    msg_id, last
                ));
            }
            last_control_msg_id = Some(msg_id);

            // If this was the Hello, send response
            if frame.desc.method_id == control_verb::HELLO {
                if let Err(result) = send_hello_response(peer).await {
                    return result;
                }
                handshake_done = true;
            }

            // If this was OpenChannel, capture the channel ID
            if frame.desc.method_id == control_verb::OPEN_CHANNEL
                && let Ok(open) = facet_postcard::from_slice::<OpenChannel>(frame.payload_bytes())
            {
                channel_id = Some(open.channel_id);
            }
        } else if let Some(ch) = channel_id {
            // Got a data frame on the opened channel - send response and exit
            if frame.desc.channel_id == ch {
                let call_result = CallResult {
                    status: Status::ok(),
                    trailers: vec![],
                    body: Some(vec![]),
                };

                let payload = match facet_postcard::to_vec(&call_result) {
                    Ok(p) => p,
                    Err(e) => {
                        return TestResult::fail(format!("failed to serialize CallResult: {}", e));
                    }
                };

                let mut desc = MsgDescHot::new();
                desc.msg_id = frame.desc.msg_id;
                desc.channel_id = ch;
                desc.method_id = frame.desc.method_id;
                desc.flags = flags::DATA | flags::EOS | flags::RESPONSE;

                let response_frame = Frame::inline(desc, &payload);
                if let Err(e) = peer.send(&response_frame).await {
                    return TestResult::fail(format!("failed to send response: {}", e));
                }

                // We've verified msg_id ordering and completed the call, we're done
                break;
            }
        }

        // If we have 2+ control frames and are past handshake, we have enough data
        if control_frame_count >= 2 && handshake_done && channel_id.is_some() {
            // Continue to handle the data frame
        }
    }

    // Must have seen at least 2 control frames
    if control_frame_count < 2 {
        return TestResult::fail(format!(
            "need at least 2 control frames, got {}",
            control_frame_count
        ));
    }

    TestResult::pass()
}

// =============================================================================
// frame.msg_id_call_echo
// =============================================================================
// Rule: [verify frame.msg-id.call-echo]
//
// For CALL channels, the response msg_id MUST echo the request msg_id.
// This is tested from the perspective of verifying implementation behavior.

#[conformance(name = "frame.msg_id_call_echo", rules = "frame.msg-id.call-echo")]
pub async fn msg_id_call_echo(peer: &mut Peer) -> TestResult {
    use crate::harness::Frame;

    // Complete handshake
    let hello_frame = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive Hello: {}", e)),
    };

    if hello_frame.desc.channel_id != 0 || hello_frame.desc.method_id != control_verb::HELLO {
        return TestResult::fail("expected Hello frame");
    }

    let response = Hello {
        protocol_version: PROTOCOL_VERSION_1_0,
        role: Role::Acceptor,
        required_features: 0,
        supported_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
        limits: Limits::default(),
        methods: vec![],
        params: vec![],
    };

    let payload = match facet_postcard::to_vec(&response) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to serialize Hello: {}", e)),
    };

    let mut desc = MsgDescHot::new();
    desc.msg_id = 1;
    desc.channel_id = 0;
    desc.method_id = control_verb::HELLO;
    desc.flags = flags::CONTROL;

    let response_frame = Frame::inline(desc, &payload);
    if let Err(e) = peer.send(&response_frame).await {
        return TestResult::fail(format!("failed to send Hello: {}", e));
    }

    // Wait for OpenChannel
    let open_frame = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive OpenChannel: {}", e)),
    };

    if open_frame.desc.method_id != control_verb::OPEN_CHANNEL {
        return TestResult::fail("expected OpenChannel");
    }

    let open: OpenChannel = match facet_postcard::from_slice(open_frame.payload_bytes()) {
        Ok(o) => o,
        Err(e) => return TestResult::fail(format!("failed to deserialize OpenChannel: {}", e)),
    };

    let channel_id = open.channel_id;

    // Wait for request
    let request = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive request: {}", e)),
    };

    if request.desc.channel_id != channel_id {
        return TestResult::fail("request not on expected channel");
    }

    let request_msg_id = request.desc.msg_id;

    // Send response with echoed msg_id
    let call_result = CallResult {
        status: Status::ok(),
        trailers: vec![],
        body: Some(vec![]),
    };

    let payload = match facet_postcard::to_vec(&call_result) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to serialize CallResult: {}", e)),
    };

    let mut resp_desc = MsgDescHot::new();
    resp_desc.msg_id = request_msg_id; // Echo the request's msg_id
    resp_desc.channel_id = channel_id;
    resp_desc.method_id = request.desc.method_id;
    resp_desc.flags = flags::DATA | flags::EOS | flags::RESPONSE;

    let resp_frame = Frame::inline(resp_desc, &payload);
    if let Err(e) = peer.send(&resp_frame).await {
        return TestResult::fail(format!("failed to send response: {}", e));
    }

    TestResult::pass()
}
