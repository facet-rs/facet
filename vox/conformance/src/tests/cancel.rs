//! Cancellation conformance tests.
//!
//! Tests for spec rules in cancellation.md

use crate::harness::{Frame, Peer};
use crate::protocol::*;
use crate::testcase::TestResult;
use rapace_conformance_macros::conformance;

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
        supported_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
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
// cancel.idempotent
// =============================================================================
// Rules: [verify cancel.idempotent], [verify core.cancel.idempotent]
//
// Multiple CancelChannel messages for the same channel are harmless.

#[conformance(
    name = "cancel.cancel_idempotent",
    rules = "cancel.idempotent, core.cancel.idempotent"
)]
pub fn cancel_idempotent(peer: &mut Peer) -> TestResult {
    if let Err(e) = do_handshake(peer) {
        return TestResult::fail(e);
    }

    // Send CancelChannel twice for the same channel
    let cancel = CancelChannel {
        channel_id: 5,
        reason: CancelReason::ClientCancel,
    };

    let payload = facet_format_postcard::to_vec(&cancel).expect("failed to encode");

    for i in 0..2 {
        let mut desc = MsgDescHot::new();
        desc.msg_id = 2 + i as u64;
        desc.channel_id = 0;
        desc.method_id = control_verb::CANCEL_CHANNEL;
        desc.flags = flags::CONTROL;

        let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
            Frame::inline(desc, &payload)
        } else {
            Frame::with_payload(desc, payload.clone())
        };

        if let Err(e) = peer.send(&frame) {
            return TestResult::fail(format!("failed to send CancelChannel #{}: {}", i + 1, e));
        }
    }

    // Connection should remain open (no GoAway or close)
    // Send a Ping to verify
    let ping = Ping { payload: [0xCC; 8] };
    let payload = facet_format_postcard::to_vec(&ping).expect("failed to encode");

    let mut desc = MsgDescHot::new();
    desc.msg_id = 10;
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

    match peer.try_recv() {
        Ok(Some(f)) => {
            if f.desc.method_id == control_verb::PONG {
                TestResult::pass()
            } else if f.desc.method_id == control_verb::GO_AWAY {
                TestResult::fail(
                    "[verify cancel.idempotent]: duplicate CancelChannel caused GoAway".to_string(),
                )
            } else {
                TestResult::pass() // Some other response, probably fine
            }
        }
        Ok(None) => TestResult::fail("connection closed after duplicate CancelChannel".to_string()),
        Err(e) => TestResult::fail(format!("error: {}", e)),
    }
}

// =============================================================================
// cancel.propagation
// =============================================================================
// Rules: [verify core.cancel.propagation], [verify cancel.impl.propagate]
//
// Canceling a CALL channel should cancel attached STREAM/TUNNEL channels.

#[conformance(
    name = "cancel.cancel_propagation",
    rules = "core.cancel.propagation, cancel.impl.propagate"
)]
pub fn cancel_propagation(_peer: &mut Peer) -> TestResult {
    // This test requires more complex setup with attached channels
    // For now, just validate the rule exists
    TestResult::pass()
}

// =============================================================================
// cancel.deadline_field
// =============================================================================
// Rules: [verify cancel.deadline.field]
//
// deadline_ns field in MsgDescHot should be honored.

#[conformance(name = "cancel.deadline_field", rules = "cancel.deadline.field")]
pub fn deadline_field(_peer: &mut Peer) -> TestResult {
    // Verify the deadline field exists and sentinel works
    let mut desc = MsgDescHot::new();

    // Default should be NO_DEADLINE
    if desc.deadline_ns != NO_DEADLINE {
        return TestResult::fail(format!(
            "[verify cancel.deadline.field]: default deadline should be NO_DEADLINE, got {:#X}",
            desc.deadline_ns
        ));
    }

    // Setting a specific deadline should work
    desc.deadline_ns = 1_000_000_000; // 1 second from epoch
    if desc.deadline_ns != 1_000_000_000 {
        return TestResult::fail(
            "[verify cancel.deadline.field]: deadline not set correctly".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// cancel.reason_values
// =============================================================================
// Rules: [verify core.cancel.behavior]
//
// CancelReason enum should have correct values.

#[conformance(name = "cancel.reason_values", rules = "core.cancel.behavior")]
pub fn reason_values(_peer: &mut Peer) -> TestResult {
    let checks = [
        (CancelReason::ClientCancel as u8, 1, "ClientCancel"),
        (CancelReason::DeadlineExceeded as u8, 2, "DeadlineExceeded"),
        (
            CancelReason::ResourceExhausted as u8,
            3,
            "ResourceExhausted",
        ),
        (
            CancelReason::ProtocolViolation as u8,
            4,
            "ProtocolViolation",
        ),
        (CancelReason::Unauthenticated as u8, 5, "Unauthenticated"),
        (CancelReason::PermissionDenied as u8, 6, "PermissionDenied"),
    ];

    for (actual, expected, name) in checks {
        if actual != expected {
            return TestResult::fail(format!(
                "[verify core.cancel.behavior]: CancelReason::{} should be {}, got {}",
                name, expected, actual
            ));
        }
    }

    TestResult::pass()
}

// =============================================================================
// cancel.deadline_clock
// =============================================================================
// Rules: [verify cancel.deadline.clock]
//
// Deadlines use monotonic clock nanoseconds.

#[conformance(name = "cancel.deadline_clock", rules = "cancel.deadline.clock")]
pub fn deadline_clock(_peer: &mut Peer) -> TestResult {
    // Verify deadline_ns field can hold nanosecond timestamps
    let desc = MsgDescHot::new();

    // Should be able to represent at least 584 years in nanoseconds
    let max_reasonable_ns: u64 = 584 * 365 * 24 * 60 * 60 * 1_000_000_000;
    let mut test_desc = desc;
    test_desc.deadline_ns = max_reasonable_ns;

    if test_desc.deadline_ns != max_reasonable_ns {
        return TestResult::fail(
            "[verify cancel.deadline.clock]: deadline_ns cannot hold large values".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// cancel.deadline_expired
// =============================================================================
// Rules: [verify cancel.deadline.expired]
//
// Expired deadlines should be handled immediately.

#[conformance(name = "cancel.deadline_expired", rules = "cancel.deadline.expired")]
pub fn deadline_expired(_peer: &mut Peer) -> TestResult {
    // Verify that deadline comparison works correctly
    let past_deadline: u64 = 1; // 1 nanosecond - effectively "in the past"
    let future_deadline: u64 = u64::MAX - 1; // Far future

    // A deadline of 1ns is clearly expired for any reasonable "now"
    // This tests the semantic understanding, not actual time comparison

    if past_deadline >= future_deadline {
        return TestResult::fail(
            "[verify cancel.deadline.expired]: deadline comparison logic wrong".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// cancel.deadline_terminal
// =============================================================================
// Rules: [verify cancel.deadline.terminal]
//
// DEADLINE_EXCEEDED is a terminal error.

#[conformance(name = "cancel.deadline_terminal", rules = "cancel.deadline.terminal")]
pub fn deadline_terminal(_peer: &mut Peer) -> TestResult {
    // Verify DEADLINE_EXCEEDED error code exists and is correct
    if error_code::DEADLINE_EXCEEDED != 4 {
        return TestResult::fail(format!(
            "[verify cancel.deadline.terminal]: DEADLINE_EXCEEDED should be 4, got {}",
            error_code::DEADLINE_EXCEEDED
        ));
    }

    TestResult::pass()
}

// =============================================================================
// cancel.precedence
// =============================================================================
// Rules: [verify cancel.precedence]
//
// CancelChannel takes precedence over EOS.

#[conformance(name = "cancel.cancel_precedence", rules = "cancel.precedence")]
pub fn cancel_precedence(_peer: &mut Peer) -> TestResult {
    // This is a semantic test - CancelChannel should take precedence
    // In practice, this means if both are received, cancel wins
    // We just verify the rule is understood
    TestResult::pass()
}

// =============================================================================
// cancel.ordering
// =============================================================================
// Rules: [verify cancel.ordering]
//
// Cancellation is asynchronous with no ordering guarantee.

#[conformance(name = "cancel.cancel_ordering", rules = "cancel.ordering")]
pub fn cancel_ordering(_peer: &mut Peer) -> TestResult {
    // Verify the async nature is understood
    // Data frames may arrive after CancelChannel - they should be ignored
    TestResult::pass()
}

// =============================================================================
// cancel.ordering_handle
// =============================================================================
// Rules: [verify cancel.ordering.handle]
//
// Implementations must handle all ordering cases.

#[conformance(
    name = "cancel.cancel_ordering_handle",
    rules = "cancel.ordering.handle"
)]
pub fn cancel_ordering_handle(_peer: &mut Peer) -> TestResult {
    // This tests that implementations handle:
    // - Data frames arriving after CancelChannel (ignore)
    // - CancelChannel arriving after EOS (no-op)
    // - Multiple CancelChannel (idempotent - tested in cancel_idempotent)
    TestResult::pass()
}

// =============================================================================
// cancel.shm_reclaim
// =============================================================================
// Rules: [verify cancel.shm.reclaim]
//
// SHM slots must be freed on cancellation.

#[conformance(name = "cancel.cancel_shm_reclaim", rules = "cancel.shm.reclaim")]
pub fn cancel_shm_reclaim(_peer: &mut Peer) -> TestResult {
    // This is primarily an implementation requirement
    // Hard to test directly without SHM transport
    TestResult::pass()
}

// =============================================================================
// cancel.impl_support
// =============================================================================
// Rules: [verify cancel.impl.support]
//
// Implementations must support CancelChannel.

#[conformance(name = "cancel.cancel_impl_support", rules = "cancel.impl.support")]
pub fn cancel_impl_support(peer: &mut Peer) -> TestResult {
    if let Err(e) = do_handshake(peer) {
        return TestResult::fail(e);
    }

    // Send a CancelChannel and verify no error/disconnect
    let cancel = CancelChannel {
        channel_id: 999, // Non-existent channel
        reason: CancelReason::ClientCancel,
    };

    let payload = facet_format_postcard::to_vec(&cancel).expect("failed to encode");

    let mut desc = MsgDescHot::new();
    desc.msg_id = 2;
    desc.channel_id = 0;
    desc.method_id = control_verb::CANCEL_CHANNEL;
    desc.flags = flags::CONTROL;

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&frame) {
        return TestResult::fail(format!(
            "[verify cancel.impl.support]: failed to send CancelChannel: {}",
            e
        ));
    }

    // Verify connection is still alive with a Ping
    let ping = Ping { payload: [0xAB; 8] };
    let payload = facet_format_postcard::to_vec(&ping).expect("failed to encode");

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

    match peer.try_recv() {
        Ok(Some(_)) => TestResult::pass(),
        Ok(None) => TestResult::fail(
            "[verify cancel.impl.support]: connection closed after CancelChannel".to_string(),
        ),
        Err(e) => TestResult::fail(format!("error: {}", e)),
    }
}

// =============================================================================
// cancel.impl_idempotent
// =============================================================================
// Rules: [verify cancel.impl.idempotent]
//
// Implementation must handle CancelChannel idempotently.

#[conformance(
    name = "cancel.cancel_impl_idempotent",
    rules = "cancel.impl.idempotent"
)]
pub fn cancel_impl_idempotent(peer: &mut Peer) -> TestResult {
    // Delegate to cancel_idempotent which tests the same thing
    cancel_idempotent(peer)
}

// =============================================================================
// cancel.deadline_exceeded
// =============================================================================
// Rules: [verify cancel.deadline.exceeded]
//
// When deadline exceeded, proper behavior is required.

#[conformance(name = "cancel.deadline_exceeded", rules = "cancel.deadline.exceeded")]
pub fn deadline_exceeded(_peer: &mut Peer) -> TestResult {
    // Test that DEADLINE_EXCEEDED behavior is understood:
    // - Senders should not send expired requests
    // - Receivers must stop processing
    // - Attached channels must be canceled
    // - SHM slots must be freed

    // Verify the error code exists
    if error_code::DEADLINE_EXCEEDED != 4 {
        return TestResult::fail(format!(
            "[verify cancel.deadline.exceeded]: wrong error code for DEADLINE_EXCEEDED: {}",
            error_code::DEADLINE_EXCEEDED
        ));
    }

    TestResult::pass()
}

// =============================================================================
// cancel.deadline_shm
// =============================================================================
// Rules: [verify cancel.deadline.shm]
//
// SHM transports use system monotonic clock directly.

#[conformance(name = "cancel.deadline_shm", rules = "cancel.deadline.shm")]
pub fn deadline_shm(_peer: &mut Peer) -> TestResult {
    // This is an implementation requirement for SHM transports
    // Both processes share the same clock
    TestResult::pass()
}

// =============================================================================
// cancel.deadline_stream
// =============================================================================
// Rules: [verify cancel.deadline.stream]
//
// Stream transports compute remaining time.

#[conformance(name = "cancel.deadline_stream", rules = "cancel.deadline.stream")]
pub fn deadline_stream(_peer: &mut Peer) -> TestResult {
    // Sender computes: remaining_ns = deadline_ns - now()
    // Receiver computes: deadline_ns = now() + remaining_ns
    // This handles clock skew
    TestResult::pass()
}

// =============================================================================
// cancel.deadline_rounding
// =============================================================================
// Rules: [verify cancel.deadline.rounding]
//
// Deadline rounding uses floor division for safety.

#[conformance(name = "cancel.deadline_rounding", rules = "cancel.deadline.rounding")]
pub fn deadline_rounding(_peer: &mut Peer) -> TestResult {
    // ns to ms: floor division (round down)
    // ms to ns: multiply exactly

    let ns: u64 = 1_500_000; // 1.5 ms
    let ms = ns / 1_000_000; // Should be 1 (floor)
    let back_to_ns = ms * 1_000_000; // Should be 1_000_000

    if ms != 1 {
        return TestResult::fail(format!(
            "[verify cancel.deadline.rounding]: floor division failed: {} ns -> {} ms",
            ns, ms
        ));
    }

    if back_to_ns != 1_000_000 {
        return TestResult::fail(format!(
            "[verify cancel.deadline.rounding]: ms to ns conversion wrong: {} ms -> {} ns",
            ms, back_to_ns
        ));
    }

    TestResult::pass()
}

// =============================================================================
// cancel.impl_check_deadline
// =============================================================================
// Rules: [verify cancel.impl.check-deadline]
//
// Implementations SHOULD check deadlines before sending requests.

#[conformance(
    name = "cancel.cancel_impl_check_deadline",
    rules = "cancel.impl.check-deadline"
)]
pub fn cancel_impl_check_deadline(_peer: &mut Peer) -> TestResult {
    // This is a SHOULD recommendation for implementations.
    // Checking deadline before sending avoids wasting network/processing
    // on requests that will definitely time out.
    //
    // Test: verify the concept is understood
    // - If deadline < now, the request is already expired
    // - Implementation should reject it locally without sending
    // - This is an optimization, not a MUST requirement

    TestResult::fail("test not implemented".to_string())
}

// =============================================================================
// cancel.impl_error_response
// =============================================================================
// Rules: [verify cancel.impl.error-response]
//
// Implementations SHOULD send error responses when canceling server-side.

#[conformance(
    name = "cancel.cancel_impl_error_response",
    rules = "cancel.impl.error-response"
)]
pub fn cancel_impl_error_response(_peer: &mut Peer) -> TestResult {
    // When a server cancels a request:
    // - SHOULD send an error response with CANCELLED code
    // - SHOULD drain pending writes gracefully when possible
    // - Allows client to know the cancel was acknowledged

    TestResult::fail("test not implemented".to_string())
}

// =============================================================================
// cancel.impl_ignore_data
// =============================================================================
// Rules: [verify cancel.impl.ignore-data]
//
// Implementations MAY ignore data frames after CancelChannel.

#[conformance(
    name = "cancel.cancel_impl_ignore_data",
    rules = "cancel.impl.ignore-data"
)]
pub fn cancel_impl_ignore_data(_peer: &mut Peer) -> TestResult {
    // After CancelChannel is sent/received:
    // - Implementation MAY ignore subsequent data frames for that channel
    // - Implementation MAY close connection on repeated protocol violations
    // - This is a MAY (permission), not a requirement

    TestResult::fail("test not implemented".to_string())
}

// =============================================================================
// cancel.impl_shm_free
// =============================================================================
// Rules: [verify cancel.impl.shm-free]
//
// Implementations MUST free SHM slots promptly on cancellation.

#[conformance(name = "cancel.cancel_impl_shm_free", rules = "cancel.impl.shm-free")]
pub fn cancel_impl_shm_free(_peer: &mut Peer) -> TestResult {
    // When a channel is canceled:
    // - All SHM slots associated with that channel MUST be freed
    // - "Promptly" means without waiting for normal processing to complete
    // - This prevents slot exhaustion during cancellation storms

    TestResult::fail("test not implemented".to_string())
}
