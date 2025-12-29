//! Handshake conformance tests.
//!
//! Tests for spec rules in handshake.md

use crate::harness::{Frame, Peer};
use crate::protocol::*;
use crate::testcase::TestResult;
use rapace_conformance_macros::conformance;

// =============================================================================
// handshake.valid_hello_exchange
// =============================================================================
// Rules: [verify handshake.required], [verify handshake.ordering]
//
// The peer acts as ACCEPTOR. The implementation (INITIATOR) should:
// 1. Send a valid Hello
// 2. Receive our Hello response
// 3. Connection is ready

#[conformance(
    name = "handshake.valid_hello_exchange",
    rules = "handshake.required, handshake.ordering"
)]
pub fn valid_hello_exchange(peer: &mut Peer) -> TestResult {
    // Wait for Hello from implementation (initiator)
    let frame = match peer.recv() {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive Hello: {}", e)),
    };

    // Validate it's a Hello on channel 0
    if frame.desc.channel_id != 0 {
        return TestResult::fail(format!(
            "[verify handshake.ordering]: first frame must be on channel 0, got {}",
            frame.desc.channel_id
        ));
    }

    if frame.desc.flags & flags::CONTROL == 0 {
        return TestResult::fail(
            "[verify core.control.flag-set]: Hello must have CONTROL flag set".to_string(),
        );
    }

    if frame.desc.method_id != control_verb::HELLO {
        return TestResult::fail(format!(
            "[verify handshake.ordering]: first frame must be Hello (method_id=0), got {}",
            frame.desc.method_id
        ));
    }

    // Decode Hello payload
    let hello: Hello = match facet_format_postcard::from_slice(frame.payload_bytes()) {
        Ok(h) => h,
        Err(e) => {
            return TestResult::fail(format!(
                "[verify core.control.payload-encoding]: failed to decode Hello: {}",
                e
            ));
        }
    };

    // Validate Hello contents
    if hello.role != Role::Initiator {
        return TestResult::fail(format!(
            "[verify handshake.role.validation]: initiator must declare Role::Initiator, got {:?}",
            hello.role
        ));
    }

    // Check protocol version
    let major = hello.protocol_version >> 16;
    if major != 1 {
        return TestResult::fail(format!(
            "[verify handshake.version.major]: expected major version 1, got {}",
            major
        ));
    }

    // Send our Hello response as acceptor
    let response = Hello {
        protocol_version: PROTOCOL_VERSION_1_0,
        role: Role::Acceptor,
        required_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
        supported_features: features::ATTACHED_STREAMS
            | features::CALL_ENVELOPE
            | features::CREDIT_FLOW_CONTROL
            | features::RAPACE_PING,
        limits: Limits::default(),
        methods: Vec::new(),
        params: Vec::new(),
    };

    let payload = facet_format_postcard::to_vec(&response).expect("failed to encode Hello");

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

    if let Err(e) = peer.send(&frame) {
        return TestResult::fail(format!("failed to send Hello response: {}", e));
    }

    TestResult::pass()
}

// =============================================================================
// handshake.missing_hello
// =============================================================================
// Rules: [verify handshake.first-frame], [verify handshake.failure]
//
// The peer acts as ACCEPTOR. If implementation sends non-Hello first frame,
// peer should detect the violation.

#[conformance(
    name = "handshake.missing_hello",
    rules = "handshake.first-frame, handshake.failure"
)]
pub fn missing_hello(peer: &mut Peer) -> TestResult {
    // Implementation should send Hello first
    // This test expects the implementation to INCORRECTLY send a non-Hello frame
    // The peer validates that it's NOT a Hello

    let frame = match peer.recv() {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive frame: {}", e)),
    };

    // If it's a valid Hello, the test fails (implementation did the right thing)
    if frame.desc.channel_id == 0
        && frame.desc.method_id == control_verb::HELLO
        && frame.desc.flags & flags::CONTROL != 0
    {
        return TestResult::fail(
            "expected non-Hello frame for this test, but got valid Hello".to_string(),
        );
    }

    // Good - implementation sent wrong frame. We should close.
    // Send CloseChannel with error
    let close = CloseChannel {
        channel_id: 0,
        reason: CloseReason::Error("[verify handshake.first-frame]: expected Hello".to_string()),
    };

    let payload = facet_format_postcard::to_vec(&close).expect("failed to encode CloseChannel");

    let mut desc = MsgDescHot::new();
    desc.msg_id = 1;
    desc.channel_id = 0;
    desc.method_id = control_verb::CLOSE_CHANNEL;
    desc.flags = flags::CONTROL;

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    let _ = peer.send(&frame);

    TestResult::pass()
}

// =============================================================================
// handshake.version_mismatch
// =============================================================================
// Rules: [verify handshake.version.major]
//
// Peer sends Hello with incompatible major version.
// Implementation should reject/close.

#[conformance(name = "handshake.version_mismatch", rules = "handshake.version.major")]
pub fn version_mismatch(peer: &mut Peer) -> TestResult {
    // Wait for Hello from implementation
    let frame = match peer.recv() {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive Hello: {}", e)),
    };

    // Basic validation
    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::HELLO {
        return TestResult::fail("first frame must be Hello".to_string());
    }

    // Send Hello with wrong major version (v99.0)
    let response = Hello {
        protocol_version: 99 << 16, // Major version 99
        role: Role::Acceptor,
        required_features: 0,
        supported_features: 0,
        limits: Limits::default(),
        methods: Vec::new(),
        params: Vec::new(),
    };

    let payload = facet_format_postcard::to_vec(&response).expect("failed to encode Hello");

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

    if let Err(e) = peer.send(&frame) {
        return TestResult::fail(format!("failed to send Hello: {}", e));
    }

    // Implementation should close the connection or send error
    // We expect either EOF or a CloseChannel
    match peer.try_recv() {
        Ok(None) => TestResult::pass(), // EOF - connection closed, good
        Ok(Some(f)) => {
            // Check if it's a close/error
            if f.desc.channel_id == 0 && f.desc.method_id == control_verb::CLOSE_CHANNEL {
                TestResult::pass()
            } else {
                TestResult::fail(format!(
                    "[verify handshake.version.major]: expected close after version mismatch, got frame with method_id={}",
                    f.desc.method_id
                ))
            }
        }
        Err(e) => TestResult::fail(format!("error waiting for close: {}", e)),
    }
}

// =============================================================================
// handshake.role_conflict
// =============================================================================
// Rules: [verify handshake.role.validation]
//
// Peer sends Hello claiming to be INITIATOR (same as implementation).
// Implementation should reject.

#[conformance(name = "handshake.role_conflict", rules = "handshake.role.validation")]
pub fn role_conflict(peer: &mut Peer) -> TestResult {
    // Wait for Hello from implementation (initiator)
    let frame = match peer.recv() {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive Hello: {}", e)),
    };

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::HELLO {
        return TestResult::fail("first frame must be Hello".to_string());
    }

    // Send Hello also claiming INITIATOR (conflict!)
    let response = Hello {
        protocol_version: PROTOCOL_VERSION_1_0,
        role: Role::Initiator, // Wrong! Should be Acceptor
        required_features: 0,
        supported_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
        limits: Limits::default(),
        methods: Vec::new(),
        params: Vec::new(),
    };

    let payload = facet_format_postcard::to_vec(&response).expect("failed to encode Hello");

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

    if let Err(e) = peer.send(&frame) {
        return TestResult::fail(format!("failed to send Hello: {}", e));
    }

    // Implementation should reject
    match peer.try_recv() {
        Ok(None) => TestResult::pass(),
        Ok(Some(f)) => {
            if f.desc.channel_id == 0 && f.desc.method_id == control_verb::CLOSE_CHANNEL {
                TestResult::pass()
            } else {
                TestResult::fail(
                    "[verify handshake.role.validation]: expected close after role conflict"
                        .to_string(),
                )
            }
        }
        Err(e) => TestResult::fail(format!("error: {}", e)),
    }
}

// =============================================================================
// handshake.required_features_missing
// =============================================================================
// Rules: [verify handshake.features.required]
//
// Peer requires a feature the implementation doesn't support.

#[conformance(
    name = "handshake.required_features_missing",
    rules = "handshake.features.required"
)]
pub fn required_features_missing(peer: &mut Peer) -> TestResult {
    // Wait for Hello from implementation
    let frame = match peer.recv() {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive Hello: {}", e)),
    };

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::HELLO {
        return TestResult::fail("first frame must be Hello".to_string());
    }

    let hello: Hello = match facet_format_postcard::from_slice(frame.payload_bytes()) {
        Ok(h) => h,
        Err(e) => return TestResult::fail(format!("failed to decode Hello: {}", e)),
    };

    // Send Hello requiring a feature we know they don't support
    // Use a high bit that's definitely not implemented
    let fake_required_feature: u64 = 1 << 63;

    // Check if by some miracle they support it
    if hello.supported_features & fake_required_feature != 0 {
        return TestResult::fail("implementation supports our fake feature?!".to_string());
    }

    let response = Hello {
        protocol_version: PROTOCOL_VERSION_1_0,
        role: Role::Acceptor,
        required_features: fake_required_feature, // Require unsupported feature
        supported_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
        limits: Limits::default(),
        methods: Vec::new(),
        params: Vec::new(),
    };

    let payload = facet_format_postcard::to_vec(&response).expect("failed to encode Hello");

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

    if let Err(e) = peer.send(&frame) {
        return TestResult::fail(format!("failed to send Hello: {}", e));
    }

    // Implementation should reject
    match peer.try_recv() {
        Ok(None) => TestResult::pass(),
        Ok(Some(f)) => {
            if f.desc.channel_id == 0 && f.desc.method_id == control_verb::CLOSE_CHANNEL {
                TestResult::pass()
            } else {
                TestResult::fail(
                    "[verify handshake.features.required]: expected close when required features missing"
                        .to_string(),
                )
            }
        }
        Err(e) => TestResult::fail(format!("error: {}", e)),
    }
}

// =============================================================================
// handshake.method_registry_duplicate
// =============================================================================
// Rules: [verify handshake.registry.no-duplicates]
//
// Peer sends Hello with duplicate method_id in registry.

#[conformance(
    name = "handshake.method_registry_duplicate",
    rules = "handshake.registry.no-duplicates"
)]
pub fn method_registry_duplicate(peer: &mut Peer) -> TestResult {
    // Wait for Hello from implementation
    let frame = match peer.recv() {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive Hello: {}", e)),
    };

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::HELLO {
        return TestResult::fail("first frame must be Hello".to_string());
    }

    // Send Hello with duplicate method IDs
    let response = Hello {
        protocol_version: PROTOCOL_VERSION_1_0,
        role: Role::Acceptor,
        required_features: 0,
        supported_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
        limits: Limits::default(),
        methods: vec![
            MethodInfo {
                method_id: 12345,
                sig_hash: [0u8; 32],
                name: Some("Test.foo".to_string()),
            },
            MethodInfo {
                method_id: 12345, // Duplicate!
                sig_hash: [1u8; 32],
                name: Some("Test.bar".to_string()),
            },
        ],
        params: Vec::new(),
    };

    let payload = facet_format_postcard::to_vec(&response).expect("failed to encode Hello");

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

    if let Err(e) = peer.send(&frame) {
        return TestResult::fail(format!("failed to send Hello: {}", e));
    }

    // Implementation should reject due to duplicate
    match peer.try_recv() {
        Ok(None) => TestResult::pass(),
        Ok(Some(f)) => {
            if f.desc.channel_id == 0 && f.desc.method_id == control_verb::CLOSE_CHANNEL {
                TestResult::pass()
            } else {
                TestResult::fail(
                    "[verify handshake.registry.no-duplicates]: expected close on duplicate method_id"
                        .to_string(),
                )
            }
        }
        Err(e) => TestResult::fail(format!("error: {}", e)),
    }
}

// =============================================================================
// handshake.method_registry_zero
// =============================================================================
// Rules: [verify handshake.registry.no-zero]
//
// Peer sends Hello with method_id=0 in registry.

#[conformance(
    name = "handshake.method_registry_zero",
    rules = "handshake.registry.no-zero"
)]
pub fn method_registry_zero(peer: &mut Peer) -> TestResult {
    // Wait for Hello from implementation
    let frame = match peer.recv() {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive Hello: {}", e)),
    };

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::HELLO {
        return TestResult::fail("first frame must be Hello".to_string());
    }

    // Send Hello with method_id=0 (reserved)
    let response = Hello {
        protocol_version: PROTOCOL_VERSION_1_0,
        role: Role::Acceptor,
        required_features: 0,
        supported_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
        limits: Limits::default(),
        methods: vec![MethodInfo {
            method_id: 0, // Reserved!
            sig_hash: [0u8; 32],
            name: Some("Bad.method".to_string()),
        }],
        params: Vec::new(),
    };

    let payload = facet_format_postcard::to_vec(&response).expect("failed to encode Hello");

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

    if let Err(e) = peer.send(&frame) {
        return TestResult::fail(format!("failed to send Hello: {}", e));
    }

    // Implementation should reject
    match peer.try_recv() {
        Ok(None) => TestResult::pass(),
        Ok(Some(f)) => {
            if f.desc.channel_id == 0 && f.desc.method_id == control_verb::CLOSE_CHANNEL {
                TestResult::pass()
            } else {
                TestResult::fail(
                    "[verify handshake.registry.no-zero]: expected close on method_id=0"
                        .to_string(),
                )
            }
        }
        Err(e) => TestResult::fail(format!("error: {}", e)),
    }
}
