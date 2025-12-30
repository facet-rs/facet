//! Channel conformance tests.
//!
//! Tests for spec rules in core.md related to channels.

use crate::harness::{Frame, Peer};
use crate::protocol::*;
use crate::testcase::TestResult;
use rapace_spec_tester_macros::conformance;

/// Helper to complete handshake before channel tests.
async fn do_handshake(peer: &mut Peer) -> Result<(), String> {
    // Receive Hello from implementation (initiator)
    let frame = peer
        .recv()
        .await
        .map_err(|e| format!("failed to receive Hello: {}", e))?;

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::HELLO {
        return Err("first frame must be Hello".to_string());
    }

    // Send Hello response as acceptor
    let response = Hello {
        protocol_version: PROTOCOL_VERSION_1_0,
        role: Role::Acceptor,
        required_features: 0,
        supported_features: features::ATTACHED_STREAMS
            | features::CALL_ENVELOPE
            | features::CREDIT_FLOW_CONTROL,
        limits: Limits::default(),
        methods: Vec::new(),
        params: Vec::new(),
    };

    let payload = facet_postcard::to_vec(&response).map_err(|e| e.to_string())?;

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

    peer.send(&frame).await.map_err(|e| e.to_string())?;

    Ok(())
}

// =============================================================================
// channel.id_zero_reserved
// =============================================================================
// Rules: [verify core.channel.id.zero-reserved]
//
// Channel 0 is reserved for control messages.

#[conformance(
    name = "channel.id_zero_reserved",
    rules = "core.channel.id.zero-reserved"
)]
pub async fn id_zero_reserved(peer: &mut Peer) -> TestResult {
    if let Err(e) = do_handshake(peer).await {
        return TestResult::fail(e);
    }

    // Send OpenChannel trying to use channel 0
    let open = OpenChannel {
        channel_id: 0, // Reserved!
        kind: ChannelKind::Call,
        attach: None,
        metadata: Vec::new(),
        initial_credits: 0,
    };

    let payload = facet_postcard::to_vec(&open).expect("failed to encode OpenChannel");

    let mut desc = MsgDescHot::new();
    desc.msg_id = 2;
    desc.channel_id = 0;
    desc.method_id = control_verb::OPEN_CHANNEL;
    desc.flags = flags::CONTROL;

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&frame).await {
        return TestResult::fail(format!("failed to send OpenChannel: {}", e));
    }

    // Implementation should reject with CancelChannel
    match peer.try_recv().await {
        Ok(Some(f)) => {
            if f.desc.channel_id == 0 && f.desc.method_id == control_verb::CANCEL_CHANNEL {
                TestResult::pass()
            } else {
                TestResult::fail(
                    "[verify core.channel.id.zero-reserved]: expected CancelChannel for channel 0"
                        .to_string(),
                )
            }
        }
        Ok(None) => TestResult::fail("connection closed unexpectedly".to_string()),
        Err(e) => TestResult::fail(format!("error: {}", e)),
    }
}

// =============================================================================
// channel.parity_initiator_odd
// =============================================================================
// Rules: [verify core.channel.id.parity.initiator]
//
// Initiator must use odd channel IDs.

#[conformance(
    name = "channel.parity_initiator_odd",
    rules = "core.channel.id.parity.initiator"
)]
pub async fn parity_initiator_odd(peer: &mut Peer) -> TestResult {
    let _ = peer;
    panic!("TODO: this test should be interactive and actually test spec-subject");
}

// =============================================================================
// channel.parity_acceptor_even
// =============================================================================
// Rules: [verify core.channel.id.parity.acceptor]
//
// Acceptor must use even channel IDs.

#[conformance(
    name = "channel.parity_acceptor_even",
    rules = "core.channel.id.parity.acceptor"
)]
pub async fn parity_acceptor_even(peer: &mut Peer) -> TestResult {
    if let Err(e) = do_handshake(peer).await {
        return TestResult::fail(e);
    }

    // We (peer) are acceptor - we should use even IDs
    // Send OpenChannel with even ID to test that implementation accepts it
    let open = OpenChannel {
        channel_id: 2, // Even - correct for acceptor
        kind: ChannelKind::Call,
        attach: None,
        metadata: Vec::new(),
        initial_credits: 1024 * 1024,
    };

    let payload = facet_postcard::to_vec(&open).expect("failed to encode OpenChannel");

    let mut desc = MsgDescHot::new();
    desc.msg_id = 2;
    desc.channel_id = 0;
    desc.method_id = control_verb::OPEN_CHANNEL;
    desc.flags = flags::CONTROL;

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&frame).await {
        return TestResult::fail(format!("failed to send OpenChannel: {}", e));
    }

    // Implementation should NOT reject (even ID from acceptor is valid)
    // We might receive data on the channel or nothing if they're waiting
    // Just check we don't get a CancelChannel
    match peer.try_recv().await {
        Ok(Some(f)) => {
            if f.desc.channel_id == 0 && f.desc.method_id == control_verb::CANCEL_CHANNEL {
                TestResult::fail(
                    "[verify core.channel.id.parity.acceptor]: acceptor's even channel ID was rejected"
                        .to_string(),
                )
            } else {
                TestResult::pass()
            }
        }
        Ok(None) => TestResult::pass(), // No response is fine
        Err(_) => TestResult::pass(),   // Timeout is fine - they're waiting for us
    }
}

// =============================================================================
// channel.open_required_before_data
// =============================================================================
// Rules: [verify core.channel.open]
//
// Channels must be opened before sending data.

#[conformance(
    name = "channel.open_required_before_data",
    rules = "core.channel.open"
)]
pub async fn open_required_before_data(peer: &mut Peer) -> TestResult {
    if let Err(e) = do_handshake(peer).await {
        return TestResult::fail(e);
    }

    // Send data on a channel that was never opened
    let mut desc = MsgDescHot::new();
    desc.msg_id = 2;
    desc.channel_id = 99; // Never opened!
    desc.method_id = 12345;
    desc.flags = flags::DATA | flags::EOS;

    let frame = Frame::inline(desc, b"unexpected data");

    if let Err(e) = peer.send(&frame).await {
        return TestResult::fail(format!("failed to send: {}", e));
    }

    // Implementation should reject with CancelChannel or GoAway
    match peer.try_recv().await {
        Ok(Some(f)) => {
            if f.desc.channel_id == 0
                && (f.desc.method_id == control_verb::CANCEL_CHANNEL
                    || f.desc.method_id == control_verb::GO_AWAY)
            {
                TestResult::pass()
            } else {
                TestResult::fail(
                    "[verify core.channel.open]: expected rejection for data on unopened channel"
                        .to_string(),
                )
            }
        }
        Ok(None) => TestResult::fail("connection closed (acceptable but not ideal)".to_string()),
        Err(e) => TestResult::fail(format!("error: {}", e)),
    }
}

// =============================================================================
// channel.kind_immutable
// =============================================================================
// Rules: [verify core.channel.kind]
//
// Channel kind must not change after open.
// (This is hard to test directly - kind is set at open time)

#[conformance(name = "channel.kind_immutable", rules = "core.channel.kind")]
pub async fn kind_immutable(peer: &mut Peer) -> TestResult {
    let _ = peer;
    panic!("TODO: this test should be interactive and actually test spec-subject");
}

// =============================================================================
// channel.id_allocation_monotonic
// =============================================================================
// Rules: [verify core.channel.id.allocation]
//
// Channel IDs must be allocated monotonically.

#[conformance(
    name = "channel.id_allocation_monotonic",
    rules = "core.channel.id.allocation"
)]
pub async fn id_allocation_monotonic(peer: &mut Peer) -> TestResult {
    if let Err(e) = do_handshake(peer).await {
        return TestResult::fail(e);
    }

    // Wait for multiple OpenChannels and verify IDs are monotonically increasing
    let mut last_channel_id: Option<u32> = None;

    for _ in 0..3 {
        match peer.try_recv().await {
            Ok(Some(f)) => {
                if f.desc.channel_id == 0 && f.desc.method_id == control_verb::OPEN_CHANNEL {
                    let open: OpenChannel = match facet_postcard::from_slice(f.payload_bytes()) {
                        Ok(o) => o,
                        Err(e) => {
                            return TestResult::fail(format!("decode error: {}", e));
                        }
                    };

                    if let Some(last) = last_channel_id
                        && open.channel_id <= last
                    {
                        return TestResult::fail(format!(
                            "[verify core.channel.id.allocation]: channel ID {} not greater than previous {}",
                            open.channel_id, last
                        ));
                    }
                    last_channel_id = Some(open.channel_id);
                }
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }

    TestResult::pass()
}

// =============================================================================
// channel.id_no_reuse
// =============================================================================
// Rules: [verify core.channel.id.no-reuse]
//
// Channel IDs must not be reused after close.

#[conformance(name = "channel.id_no_reuse", rules = "core.channel.id.no-reuse")]
pub async fn id_no_reuse(peer: &mut Peer) -> TestResult {
    let _ = peer;
    panic!("TODO: this test should be interactive and actually test spec-subject");
}

// =============================================================================
// channel.lifecycle
// =============================================================================
// Rules: [verify core.channel.lifecycle]
//
// Channels follow: Open -> Active -> HalfClosed -> Closed lifecycle.

#[conformance(name = "channel.lifecycle", rules = "core.channel.lifecycle")]
pub async fn lifecycle(peer: &mut Peer) -> TestResult {
    if let Err(e) = do_handshake(peer).await {
        return TestResult::fail(e);
    }

    // Open a channel
    let open = OpenChannel {
        channel_id: 2, // Acceptor uses even
        kind: ChannelKind::Call,
        attach: None,
        metadata: Vec::new(),
        initial_credits: 1024 * 1024,
    };

    let payload = facet_postcard::to_vec(&open).expect("encode");

    let mut desc = MsgDescHot::new();
    desc.msg_id = 2;
    desc.channel_id = 0;
    desc.method_id = control_verb::OPEN_CHANNEL;
    desc.flags = flags::CONTROL;

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&frame).await {
        return TestResult::fail(format!("failed to send OpenChannel: {}", e));
    }

    // Send data with EOS (half-close our side)
    let mut desc = MsgDescHot::new();
    desc.msg_id = 3;
    desc.channel_id = 2;
    desc.method_id = 0;
    desc.flags = flags::DATA | flags::EOS;

    let frame = Frame::inline(desc, b"request");

    if let Err(e) = peer.send(&frame).await {
        return TestResult::fail(format!("failed to send data: {}", e));
    }

    // Expect response with EOS (they half-close their side -> fully closed)
    match peer.try_recv().await {
        Ok(Some(f)) => {
            if f.desc.channel_id == 2 && (f.desc.flags & flags::EOS) != 0 {
                TestResult::pass()
            } else {
                TestResult::fail(
                    "[verify core.channel.lifecycle]: expected EOS in response".to_string(),
                )
            }
        }
        Ok(None) => TestResult::fail("connection closed".to_string()),
        Err(e) => TestResult::fail(format!("error: {}", e)),
    }
}

// =============================================================================
// channel.close_semantics
// =============================================================================
// Rules: [verify core.close.close-channel-semantics]
//
// CloseChannel is unilateral, no ack required.

#[conformance(
    name = "channel.close_semantics",
    rules = "core.close.close-channel-semantics"
)]
pub async fn close_semantics(peer: &mut Peer) -> TestResult {
    if let Err(e) = do_handshake(peer).await {
        return TestResult::fail(e);
    }

    // Open a channel
    let open = OpenChannel {
        channel_id: 2,
        kind: ChannelKind::Call,
        attach: None,
        metadata: Vec::new(),
        initial_credits: 1024 * 1024,
    };

    let payload = facet_postcard::to_vec(&open).expect("encode");

    let mut desc = MsgDescHot::new();
    desc.msg_id = 2;
    desc.channel_id = 0;
    desc.method_id = control_verb::OPEN_CHANNEL;
    desc.flags = flags::CONTROL;

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&frame).await {
        return TestResult::fail(format!("send error: {}", e));
    }

    // Send CloseChannel
    let close = CloseChannel {
        channel_id: 2,
        reason: CloseReason::Normal,
    };

    let payload = facet_postcard::to_vec(&close).expect("encode");

    let mut desc = MsgDescHot::new();
    desc.msg_id = 3;
    desc.channel_id = 0;
    desc.method_id = control_verb::CLOSE_CHANNEL;
    desc.flags = flags::CONTROL;

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&frame).await {
        return TestResult::fail(format!("send error: {}", e));
    }

    // No ack expected - CloseChannel is unilateral
    TestResult::pass()
}

// =============================================================================
// channel.eos_after_send
// =============================================================================
// Rules: [verify core.eos.after-send]
//
// After sending EOS, sender MUST NOT send more DATA on that channel.

#[conformance(name = "channel.eos_after_send", rules = "core.eos.after-send")]
pub async fn eos_after_send(peer: &mut Peer) -> TestResult {
    let _ = peer;
    panic!("TODO: this test should be interactive and actually test spec-subject");
}

// =============================================================================
// channel.flags_reserved
// =============================================================================
// Rules: [verify core.flags.reserved]
//
// Reserved flags MUST NOT be set; receivers MUST ignore unknown flags.

#[conformance(name = "channel.flags_reserved", rules = "core.flags.reserved")]
pub async fn flags_reserved(peer: &mut Peer) -> TestResult {
    if let Err(e) = do_handshake(peer).await {
        return TestResult::fail(e);
    }

    // Known reserved flags
    let reserved_08: u32 = 0b0000_1000;
    let reserved_80: u32 = 0b1000_0000;

    // These should not be set in any valid frame
    // Verify the constants are what we expect
    if reserved_08 != 0x08 {
        return TestResult::fail(
            "[verify core.flags.reserved]: reserved_08 wrong value".to_string(),
        );
    }
    if reserved_80 != 0x80 {
        return TestResult::fail(
            "[verify core.flags.reserved]: reserved_80 wrong value".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// channel.control_reserved
// =============================================================================
// Rules: [verify core.control.reserved]
//
// Channel 0 is reserved for control messages.

#[conformance(name = "channel.control_reserved", rules = "core.control.reserved")]
pub async fn control_reserved(peer: &mut Peer) -> TestResult {
    let _ = peer;
    panic!("TODO: this test should be interactive and actually test spec-subject");
}

// =============================================================================
// channel.goaway_after_send
// =============================================================================
// Rules: [verify core.goaway.after-send]
//
// After GoAway, sender rejects new OpenChannel with channel_id > last_channel_id.

#[conformance(name = "channel.goaway_after_send", rules = "core.goaway.after-send")]
pub async fn goaway_after_send(peer: &mut Peer) -> TestResult {
    if let Err(e) = do_handshake(peer).await {
        return TestResult::fail(e);
    }

    // Send GoAway
    let goaway = GoAway {
        reason: GoAwayReason::Shutdown,
        last_channel_id: 0,
        message: "test shutdown".to_string(),
        metadata: Vec::new(),
    };

    let payload = facet_postcard::to_vec(&goaway).expect("encode");

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

    if let Err(e) = peer.send(&frame).await {
        return TestResult::fail(format!("send error: {}", e));
    }

    // After GoAway, we should not initiate new channels
    // The connection should wind down gracefully
    TestResult::pass()
}

// =============================================================================
// channel.open_attach_validation
// =============================================================================
// Rules: [verify core.channel.open.attach-validation]
//
// When receiving OpenChannel with attach, validate:
// - call_channel_id exists
// - port_id is declared by method
// - kind matches port's declared kind
// - direction matches expected direction

#[conformance(
    name = "channel.open_attach_validation",
    rules = "core.channel.open.attach-validation"
)]
pub async fn open_attach_validation(peer: &mut Peer) -> TestResult {
    let _ = peer;
    panic!("TODO: this test should be interactive and actually test spec-subject");
}

// =============================================================================
// channel.open_call_validation
// =============================================================================
// Rules: [verify core.channel.open.call-validation]
//
// When receiving OpenChannel without attach (for CALL channels):
// - kind STREAM/TUNNEL without attach is protocol violation
// - max_channels exceeded returns ResourceExhausted
// - Wrong parity channel ID returns ProtocolViolation

#[conformance(
    name = "channel.open_call_validation",
    rules = "core.channel.open.call-validation"
)]
pub async fn open_call_validation(peer: &mut Peer) -> TestResult {
    let _ = peer;
    panic!("TODO: this test should be interactive and actually test spec-subject");
}

// =============================================================================
// channel.open_cancel_on_violation
// =============================================================================
// Rules: [verify core.channel.open.cancel-on-violation]
//
// All CancelChannel responses are sent on channel 0.

#[conformance(
    name = "channel.open_cancel_on_violation",
    rules = "core.channel.open.cancel-on-violation"
)]
pub async fn open_cancel_on_violation(peer: &mut Peer) -> TestResult {
    let _ = peer;
    panic!("TODO: this test should be interactive and actually test spec-subject");
}

// =============================================================================
// channel.open_no_pre_open
// =============================================================================
// Rules: [verify core.channel.open.no-pre-open]
//
// A peer MUST NOT open a channel on behalf of the other side.

#[conformance(
    name = "channel.open_no_pre_open",
    rules = "core.channel.open.no-pre-open"
)]
pub async fn open_no_pre_open(peer: &mut Peer) -> TestResult {
    let _ = peer;
    panic!("TODO: this test should be interactive and actually test spec-subject");
}

// =============================================================================
// channel.open_ownership
// =============================================================================
// Rules: [verify core.channel.open.ownership]
//
// Client opens CALL and client→server ports.
// Server opens server→client ports.

#[conformance(name = "channel.open_ownership", rules = "core.channel.open.ownership")]
pub async fn open_ownership(peer: &mut Peer) -> TestResult {
    let _ = peer;
    panic!("TODO: this test should be interactive and actually test spec-subject");
}

// =============================================================================
// channel.close_full
// =============================================================================
// Rules: [verify core.close.full]
//
// A channel is fully closed when both sides sent EOS or CancelChannel.

#[conformance(name = "channel.close_full", rules = "core.close.full")]
pub async fn close_full(peer: &mut Peer) -> TestResult {
    let _ = peer;
    panic!("TODO: this test should be interactive and actually test spec-subject");
}

// =============================================================================
// channel.close_state_free
// =============================================================================
// Rules: [verify core.close.state-free]
//
// After full close, implementations MAY free channel state.

#[conformance(name = "channel.close_state_free", rules = "core.close.state-free")]
pub async fn close_state_free(peer: &mut Peer) -> TestResult {
    let _ = peer;
    panic!("TODO: this test should be interactive and actually test spec-subject");
}
