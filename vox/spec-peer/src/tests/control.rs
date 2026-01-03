//! Control message conformance tests.
//!
//! Tests for control channel behavior (Ping/Pong, etc.)

use crate::harness::{Frame, Peer};
use crate::protocol::*;
use crate::testcase::TestResult;
use rapace_spec_peer_macros::conformance;

/// Helper to perform handshake as acceptor.
/// Returns Ok(()) on success, or TestResult::fail on error.
async fn do_handshake(peer: &mut Peer) -> Result<(), TestResult> {
    // Receive Hello from implementation (initiator)
    let frame = peer
        .recv()
        .await
        .map_err(|e| TestResult::fail(format!("failed to receive Hello: {}", e)))?;

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::HELLO {
        return Err(TestResult::fail(format!(
            "expected Hello on channel 0, got channel={} method_id={}",
            frame.desc.channel_id, frame.desc.method_id
        )));
    }

    // Send Hello response as Acceptor
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
// control.ping_pong
// =============================================================================
// Rule: [verify core.ping.semantics]
//
// After handshake, we send a Ping with a known payload.
// The subject (rapace-core) MUST respond with Pong containing the same payload.

#[conformance(name = "control.ping_pong", rules = "core.ping.semantics")]
pub async fn ping_pong(peer: &mut Peer) -> TestResult {
    // Step 1: Complete handshake
    if let Err(result) = do_handshake(peer).await {
        return result;
    }

    // Step 2: Send Ping with a distinctive payload
    let ping_payload: [u8; 8] = [0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE];
    let ping = Ping {
        payload: ping_payload,
    };

    let payload = match facet_postcard::to_vec(&ping) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to serialize Ping: {}", e)),
    };

    let mut desc = MsgDescHot::new();
    desc.msg_id = 2; // Next msg_id after handshake
    desc.channel_id = 0;
    desc.method_id = control_verb::PING;
    desc.flags = flags::CONTROL;

    let ping_frame = Frame::inline(desc, &payload);

    if let Err(e) = peer.send(&ping_frame).await {
        return TestResult::fail(format!("failed to send Ping: {}", e));
    }

    // Step 3: Receive Pong
    let frame = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive Pong: {}", e)),
    };

    // Verify it's on channel 0
    if frame.desc.channel_id != 0 {
        return TestResult::fail(format!(
            "expected Pong on channel 0, got channel {}",
            frame.desc.channel_id
        ));
    }

    // Verify it's the PONG control verb
    if frame.desc.method_id != control_verb::PONG {
        return TestResult::fail(format!(
            "expected method_id {} (PONG), got {}",
            control_verb::PONG,
            frame.desc.method_id
        ));
    }

    // Verify CONTROL flag is set
    if frame.desc.flags & flags::CONTROL == 0 {
        return TestResult::fail(format!(
            "CONTROL flag not set on Pong frame (flags: {:#x})",
            frame.desc.flags
        ));
    }

    // Deserialize the Pong payload
    let pong: Pong = match facet_postcard::from_slice(frame.payload_bytes()) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to deserialize Pong: {}", e)),
    };

    // Verify the payload matches what we sent
    if pong.payload != ping_payload {
        return TestResult::fail(format!(
            "Pong payload mismatch: expected {:02x?}, got {:02x?}",
            ping_payload, pong.payload
        ));
    }

    TestResult::pass()
}
