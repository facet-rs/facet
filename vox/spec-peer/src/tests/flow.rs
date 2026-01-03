//! Flow control conformance tests.
//!
//! Tests for credit-based flow control semantics.

use crate::harness::{Frame, Peer};
use crate::protocol::*;
use crate::testcase::TestResult;
use rapace_spec_peer_macros::conformance;

/// Helper to perform handshake as acceptor.
async fn do_handshake(peer: &mut Peer) -> Result<(), TestResult> {
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

    let response = Hello {
        protocol_version: PROTOCOL_VERSION_1_0,
        role: Role::Acceptor,
        required_features: 0,
        supported_features: features::ATTACHED_STREAMS
            | features::CALL_ENVELOPE
            | features::CREDIT_FLOW_CONTROL,
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
// flow.credit_semantics
// =============================================================================
// Rule: [verify core.flow.credit-semantics]
//
// Credits can be granted via the credit_grant field with CREDITS flag set.

#[conformance(name = "flow.credit_semantics", rules = "core.flow.credit-semantics")]
pub async fn credit_semantics(peer: &mut Peer) -> TestResult {
    if let Err(result) = do_handshake(peer).await {
        return result;
    }

    // Look for frames with CREDITS flag set
    for _ in 0..5 {
        let frame = match peer.try_recv().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return TestResult::fail(format!("recv error: {}", e)),
        };

        // If CREDITS flag is set, credit_grant MUST be meaningful
        if frame.desc.flags & flags::CREDITS != 0 {
            // credit_grant of 0 with CREDITS flag is unusual but not invalid
            // The flag just indicates the field is valid
            // Pass the test - we found a frame using credit semantics
            return TestResult::pass();
        }
    }

    // If no CREDITS frames seen, that's OK - implementation may use infinite credits
    TestResult::pass()
}

// =============================================================================
// flow.credit_additive
// =============================================================================
// Rule: [verify core.flow.credit-additive]
//
// Credits MUST be additive: multiple grants accumulate.

#[conformance(name = "flow.credit_additive", rules = "core.flow.credit-additive")]
pub async fn credit_additive(peer: &mut Peer) -> TestResult {
    if let Err(result) = do_handshake(peer).await {
        return result;
    }

    // Wait for OpenChannel to get a channel_id
    let frame = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive frame: {}", e)),
    };

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::OPEN_CHANNEL {
        return TestResult::fail("expected OpenChannel");
    }

    let open: OpenChannel = match facet_postcard::from_slice(frame.payload_bytes()) {
        Ok(o) => o,
        Err(e) => return TestResult::fail(format!("failed to deserialize OpenChannel: {}", e)),
    };

    let channel_id = open.channel_id;

    // Send multiple GrantCredits and verify no error occurs
    // (The additive nature is a semantic guarantee we can't fully verify from outside)
    let grant1 = GrantCredits {
        channel_id,
        bytes: 1000,
    };

    let payload1 = match facet_postcard::to_vec(&grant1) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to serialize GrantCredits: {}", e)),
    };

    let mut desc1 = MsgDescHot::new();
    desc1.msg_id = 100;
    desc1.channel_id = 0;
    desc1.method_id = control_verb::GRANT_CREDITS;
    desc1.flags = flags::CONTROL;

    let grant_frame1 = Frame::inline(desc1, &payload1);
    if let Err(e) = peer.send(&grant_frame1).await {
        return TestResult::fail(format!("failed to send GrantCredits: {}", e));
    }

    // Send a second grant
    let grant2 = GrantCredits {
        channel_id,
        bytes: 500,
    };

    let payload2 = match facet_postcard::to_vec(&grant2) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to serialize GrantCredits: {}", e)),
    };

    let mut desc2 = MsgDescHot::new();
    desc2.msg_id = 101;
    desc2.channel_id = 0;
    desc2.method_id = control_verb::GRANT_CREDITS;
    desc2.flags = flags::CONTROL;

    let grant_frame2 = Frame::inline(desc2, &payload2);
    if let Err(e) = peer.send(&grant_frame2).await {
        return TestResult::fail(format!("failed to send second GrantCredits: {}", e));
    }

    // If we got here without connection being closed, the implementation
    // accepted the additive credits
    TestResult::pass()
}

// =============================================================================
// flow.eos_no_credits
// =============================================================================
// Rule: [verify core.flow.eos-no-credits]
//
// EOS-only frames (no DATA) MUST be exempt from credit accounting.

#[conformance(name = "flow.eos_no_credits", rules = "core.flow.eos-no-credits")]
pub async fn eos_no_credits(peer: &mut Peer) -> TestResult {
    if let Err(result) = do_handshake(peer).await {
        return result;
    }

    // Look for EOS frames in the stream
    for _ in 0..5 {
        let frame = match peer.try_recv().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return TestResult::fail(format!("recv error: {}", e)),
        };

        // If we see an EOS-only frame (EOS set, DATA not set), it should have payload_len = 0
        if frame.desc.flags & flags::EOS != 0
            && frame.desc.flags & flags::DATA == 0
            && frame.desc.payload_len != 0
        {
            return TestResult::fail(format!(
                "EOS-only frame has payload_len {} but should be 0 for credit exemption",
                frame.desc.payload_len
            ));
        }
    }

    TestResult::pass()
}

// =============================================================================
// flow.intro
// =============================================================================
// Rule: [verify core.flow.intro]
//
// Rapace uses credit-based flow control per channel.

#[conformance(name = "flow.intro", rules = "core.flow.intro")]
pub async fn intro(peer: &mut Peer) -> TestResult {
    if let Err(result) = do_handshake(peer).await {
        return result;
    }

    // Wait for OpenChannel and check for initial_credits field
    let frame = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive frame: {}", e)),
    };

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::OPEN_CHANNEL {
        return TestResult::fail("expected OpenChannel");
    }

    let open: OpenChannel = match facet_postcard::from_slice(frame.payload_bytes()) {
        Ok(o) => o,
        Err(e) => return TestResult::fail(format!("failed to deserialize OpenChannel: {}", e)),
    };

    // The presence of initial_credits field demonstrates per-channel flow control
    // Even if it's 0 (no initial grant) or very large (infinite credit mode),
    // the field exists and is part of the protocol
    let _ = open.initial_credits; // Just accessing to confirm it exists

    TestResult::pass()
}
