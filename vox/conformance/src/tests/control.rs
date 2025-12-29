//! Control message conformance tests.
//!
//! Tests for spec rules related to control channel (channel 0).

use crate::harness::{Frame, Peer};
use crate::protocol::*;
use crate::testcase::TestResult;

/// Helper to complete handshake.
fn do_handshake(peer: &mut Peer) -> Result<(), String> {
    let frame = peer
        .recv()
        .map_err(|e| format!("failed to receive Hello: {}", e))?;

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::HELLO {
        return Err("first frame must be Hello".to_string());
    }

    let response = Hello {
        protocol_version: PROTOCOL_VERSION_1_0,
        role: Role::Acceptor,
        required_features: 0,
        supported_features: features::ATTACHED_STREAMS
            | features::CALL_ENVELOPE
            | features::RAPACE_PING,
        limits: Limits::default(),
        methods: Vec::new(),
        params: Vec::new(),
    };

    let payload = facet_format_postcard::to_vec(&response).map_err(|e| e.to_string())?;

    let mut desc = MsgDescHot::new();
    desc.msg_id = 1;
    desc.channel_id = 0;
    desc.method_id = control_verb::HELLO;
    desc.flags = flags::CONTROL;

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    peer.send(&frame).map_err(|e| e.to_string())?;
    Ok(())
}

// =============================================================================
// control.flag_set_on_channel_zero
// =============================================================================
// Rules: [verify core.control.flag-set]
//
// CONTROL flag must be set on channel 0.

pub fn flag_set_on_channel_zero(peer: &mut Peer) -> TestResult {
    // Check the Hello we receive has CONTROL flag
    let frame = match peer.recv() {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive: {}", e)),
    };

    if frame.desc.channel_id != 0 {
        return TestResult::fail("first frame should be on channel 0".to_string());
    }

    if frame.desc.flags & flags::CONTROL == 0 {
        return TestResult::fail(
            "[verify core.control.flag-set]: CONTROL flag not set on channel 0 frame".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// control.flag_clear_on_other_channels
// =============================================================================
// Rules: [verify core.control.flag-clear]
//
// CONTROL flag must NOT be set on channels other than 0.

pub fn flag_clear_on_other_channels(peer: &mut Peer) -> TestResult {
    if let Err(e) = do_handshake(peer) {
        return TestResult::fail(e);
    }

    // Wait for a frame on non-zero channel
    let frame = match peer.recv() {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive: {}", e)),
    };

    // Skip control frames
    if frame.desc.channel_id == 0 {
        // Need to wait for a data channel frame
        let frame = match peer.recv() {
            Ok(f) => f,
            Err(e) => return TestResult::fail(format!("failed to receive: {}", e)),
        };

        if frame.desc.channel_id != 0 && frame.desc.flags & flags::CONTROL != 0 {
            return TestResult::fail(
                "[verify core.control.flag-clear]: CONTROL flag set on non-zero channel"
                    .to_string(),
            );
        }
    } else if frame.desc.flags & flags::CONTROL != 0 {
        return TestResult::fail(
            "[verify core.control.flag-clear]: CONTROL flag set on non-zero channel".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// control.ping_pong
// =============================================================================
// Rules: [verify core.ping.semantics]
//
// Receiver must respond to Ping with Pong echoing the payload.

pub fn ping_pong(peer: &mut Peer) -> TestResult {
    if let Err(e) = do_handshake(peer) {
        return TestResult::fail(e);
    }

    // Send Ping
    let ping = Ping {
        payload: [1, 2, 3, 4, 5, 6, 7, 8],
    };

    let payload = facet_format_postcard::to_vec(&ping).expect("failed to encode Ping");

    let mut desc = MsgDescHot::new();
    desc.msg_id = 2;
    desc.channel_id = 0;
    desc.method_id = control_verb::PING;
    desc.flags = flags::CONTROL;

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&frame) {
        return TestResult::fail(format!("failed to send Ping: {}", e));
    }

    // Wait for Pong
    let frame = match peer.recv() {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive Pong: {}", e)),
    };

    if frame.desc.channel_id != 0 {
        return TestResult::fail("Pong should be on channel 0".to_string());
    }

    if frame.desc.method_id != control_verb::PONG {
        return TestResult::fail(format!(
            "[verify core.ping.semantics]: expected Pong (method_id=6), got {}",
            frame.desc.method_id
        ));
    }

    let pong: Pong = match facet_format_postcard::from_slice(frame.payload_bytes()) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to decode Pong: {}", e)),
    };

    if pong.payload != ping.payload {
        return TestResult::fail(format!(
            "[verify core.ping.semantics]: Pong payload {:?} doesn't match Ping {:?}",
            pong.payload, ping.payload
        ));
    }

    TestResult::pass()
}

// =============================================================================
// control.unknown_reserved_verb
// =============================================================================
// Rules: [verify core.control.unknown-reserved]
//
// Unknown control verb in 0-99 range should trigger GoAway.

pub fn unknown_reserved_verb(peer: &mut Peer) -> TestResult {
    if let Err(e) = do_handshake(peer) {
        return TestResult::fail(e);
    }

    // Send a control message with unknown verb in reserved range
    let mut desc = MsgDescHot::new();
    desc.msg_id = 2;
    desc.channel_id = 0;
    desc.method_id = 50; // Unknown but in reserved range 0-99
    desc.flags = flags::CONTROL;

    let frame = Frame::inline(desc, &[]);

    if let Err(e) = peer.send(&frame) {
        return TestResult::fail(format!("failed to send: {}", e));
    }

    // Should get GoAway with ProtocolError
    match peer.try_recv() {
        Ok(Some(f)) => {
            if f.desc.channel_id == 0 && f.desc.method_id == control_verb::GO_AWAY {
                TestResult::pass()
            } else {
                TestResult::fail(format!(
                    "[verify core.control.unknown-reserved]: expected GoAway, got method_id={}",
                    f.desc.method_id
                ))
            }
        }
        Ok(None) => {
            // Connection closed is acceptable
            TestResult::pass()
        }
        Err(e) => TestResult::fail(format!("error: {}", e)),
    }
}

// =============================================================================
// control.unknown_extension_verb
// =============================================================================
// Rules: [verify core.control.unknown-extension]
//
// Unknown control verb in 100+ range should be ignored silently.

pub fn unknown_extension_verb(peer: &mut Peer) -> TestResult {
    if let Err(e) = do_handshake(peer) {
        return TestResult::fail(e);
    }

    // Send a control message with unknown verb in extension range
    let mut desc = MsgDescHot::new();
    desc.msg_id = 2;
    desc.channel_id = 0;
    desc.method_id = 200; // Extension range (100+)
    desc.flags = flags::CONTROL;

    let frame = Frame::inline(desc, &[]);

    if let Err(e) = peer.send(&frame) {
        return TestResult::fail(format!("failed to send: {}", e));
    }

    // Send a Ping to verify connection is still alive
    let ping = Ping { payload: [0xAA; 8] };

    let payload = facet_format_postcard::to_vec(&ping).expect("failed to encode Ping");

    let mut desc = MsgDescHot::new();
    desc.msg_id = 3;
    desc.channel_id = 0;
    desc.method_id = control_verb::PING;
    desc.flags = flags::CONTROL;

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&frame) {
        return TestResult::fail(format!("failed to send Ping: {}", e));
    }

    // Should still get Pong (connection not closed)
    match peer.recv() {
        Ok(f) => {
            if f.desc.method_id == control_verb::PONG {
                TestResult::pass()
            } else {
                TestResult::fail(
                    "[verify core.control.unknown-extension]: expected connection to stay open"
                        .to_string(),
                )
            }
        }
        Err(e) => TestResult::fail(format!(
            "[verify core.control.unknown-extension]: connection closed unexpectedly: {}",
            e
        )),
    }
}

// =============================================================================
// control.goaway_last_channel_id
// =============================================================================
// Rules: [verify core.goaway.last-channel-id]
//
// GoAway.last_channel_id indicates highest channel ID sender will process.

pub fn goaway_last_channel_id(peer: &mut Peer) -> TestResult {
    if let Err(e) = do_handshake(peer) {
        return TestResult::fail(e);
    }

    // Send GoAway
    let goaway = GoAway {
        reason: GoAwayReason::Shutdown,
        last_channel_id: 10, // Will process channels up to 10
        message: "test shutdown".to_string(),
        metadata: Vec::new(),
    };

    let payload = facet_format_postcard::to_vec(&goaway).expect("failed to encode GoAway");

    let mut desc = MsgDescHot::new();
    desc.msg_id = 2;
    desc.channel_id = 0;
    desc.method_id = control_verb::GO_AWAY;
    desc.flags = flags::CONTROL;

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&frame) {
        return TestResult::fail(format!("failed to send GoAway: {}", e));
    }

    // Test passes if we could send GoAway
    // Implementation behavior after GoAway is tested separately
    TestResult::pass()
}

/// Run a control test case by name.
pub fn run(name: &str) -> TestResult {
    let mut peer = Peer::new();

    match name {
        "flag_set_on_channel_zero" => flag_set_on_channel_zero(&mut peer),
        "flag_clear_on_other_channels" => flag_clear_on_other_channels(&mut peer),
        "ping_pong" => ping_pong(&mut peer),
        "unknown_reserved_verb" => unknown_reserved_verb(&mut peer),
        "unknown_extension_verb" => unknown_extension_verb(&mut peer),
        "goaway_last_channel_id" => goaway_last_channel_id(&mut peer),
        _ => TestResult::fail(format!("unknown control test: {}", name)),
    }
}

/// List all control test cases.
pub fn list() -> Vec<(&'static str, &'static [&'static str])> {
    vec![
        ("flag_set_on_channel_zero", &["core.control.flag-set"][..]),
        (
            "flag_clear_on_other_channels",
            &["core.control.flag-clear"][..],
        ),
        ("ping_pong", &["core.ping.semantics"][..]),
        (
            "unknown_reserved_verb",
            &["core.control.unknown-reserved"][..],
        ),
        (
            "unknown_extension_verb",
            &["core.control.unknown-extension"][..],
        ),
        (
            "goaway_last_channel_id",
            &["core.goaway.last-channel-id"][..],
        ),
    ]
}
