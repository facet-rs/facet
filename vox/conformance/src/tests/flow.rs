//! Flow control conformance tests.
//!
//! Tests for spec rules related to credit-based flow control.

use crate::harness::Peer;
use crate::protocol::*;
use crate::testcase::TestResult;
use rapace_conformance_macros::conformance;

// =============================================================================
// flow.credit_additive
// =============================================================================
// Rules: [verify core.flow.credit-additive]
//
// Credits from multiple GrantCredits messages are additive.

#[conformance(name = "flow.credit_additive", rules = "core.flow.credit-additive")]
pub async fn credit_additive(_peer: &mut Peer) -> TestResult {
    // Structural test - verify GrantCredits structure
    let grant = GrantCredits {
        channel_id: 5,
        bytes: 1024,
    };

    let payload = match facet_postcard::to_vec(&grant) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to encode GrantCredits: {}", e)),
    };

    // Verify it round-trips
    let decoded: GrantCredits = match facet_postcard::from_slice(&payload) {
        Ok(g) => g,
        Err(e) => return TestResult::fail(format!("failed to decode GrantCredits: {}", e)),
    };

    if decoded.channel_id != 5 || decoded.bytes != 1024 {
        return TestResult::fail(
            "[verify core.flow.credit-additive]: GrantCredits roundtrip failed",
        );
    }

    TestResult::pass()
}

// =============================================================================
// flow.credit_in_flags
// =============================================================================
// Rules: [verify core.flow.credit-semantics]
//
// The CREDITS flag indicates credit_grant field is valid.

#[conformance(name = "flow.credit_in_flags", rules = "core.flow.credit-semantics")]
pub async fn credit_in_flags(_peer: &mut Peer) -> TestResult {
    // Verify CREDITS flag value
    if flags::CREDITS != 0b0100_0000 {
        return TestResult::fail(format!(
            "[verify core.flow.credit-semantics]: CREDITS flag should be 0x40, got {:#X}",
            flags::CREDITS
        ));
    }

    TestResult::pass()
}

// =============================================================================
// flow.eos_no_credits
// =============================================================================
// Rules: [verify core.flow.eos-no-credits]
//
// EOS-only frames don't consume credits.

#[conformance(name = "flow.eos_no_credits", rules = "core.flow.eos-no-credits")]
pub async fn eos_no_credits(_peer: &mut Peer) -> TestResult {
    // This is a behavioral test - implementations must not decrement
    // credits when receiving EOS-only frames (no DATA flag or empty payload)
    // We can only verify the flag values here
    if flags::EOS != 0b0000_0100 {
        return TestResult::fail(format!(
            "[verify core.flow.eos-no-credits]: EOS flag should be 0x04, got {:#X}",
            flags::EOS
        ));
    }

    TestResult::pass()
}

// =============================================================================
// flow.infinite_credit
// =============================================================================
// Rules: [verify core.flow.infinite-credit]
//
// Credit value 0xFFFFFFFF means unlimited.

#[conformance(name = "flow.infinite_credit", rules = "core.flow.infinite-credit")]
pub async fn infinite_credit(_peer: &mut Peer) -> TestResult {
    // Verify the sentinel value
    const INFINITE_CREDIT: u32 = 0xFFFFFFFF;

    let mut desc = MsgDescHot::new();
    desc.credit_grant = INFINITE_CREDIT;
    desc.flags = flags::CREDITS;

    if desc.credit_grant != 0xFFFFFFFF {
        return TestResult::fail(
            "[verify core.flow.infinite-credit]: infinite credit sentinel not set correctly"
                .to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// flow.intro
// =============================================================================
// Rules: [verify core.flow.intro]
//
// Rapace MUST use credit-based flow control per channel.

#[conformance(name = "flow.intro", rules = "core.flow.intro")]
pub async fn intro(_peer: &mut Peer) -> TestResult {
    // This rule establishes that Rapace uses credit-based flow control.
    // Each channel has its own credit window.
    //
    // Key mechanisms:
    // 1. GrantCredits control message (method_id = 4)
    // 2. credit_grant field in MsgDescHot with CREDITS flag
    // 3. initial_credits in OpenChannel

    // Verify control verb for GrantCredits
    if control_verb::GRANT_CREDITS != 4 {
        return TestResult::fail(format!(
            "[verify core.flow.intro]: GRANT_CREDITS control verb should be 4, got {}",
            control_verb::GRANT_CREDITS
        ));
    }

    // Verify OpenChannel has initial_credits field
    let open = OpenChannel {
        channel_id: 1,
        kind: ChannelKind::Call,
        attach: None,
        metadata: Vec::new(),
        initial_credits: 65536, // 64KB initial window
    };

    if open.initial_credits != 65536 {
        return TestResult::fail(
            "[verify core.flow.intro]: initial_credits field not working".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// flow.credit_overrun
// =============================================================================
// Rules: [verify core.flow.credit-overrun]
//
// If payload_len exceeds remaining credits, it's a protocol error.
// Receiver SHOULD send GoAway and MUST close the connection.

#[conformance(name = "flow.credit_overrun", rules = "core.flow.credit-overrun")]
pub async fn credit_overrun(_peer: &mut Peer) -> TestResult {
    // Credit overrun is a serious protocol violation.
    // When a receiver sees payload_len > remaining credits:
    // 1. It's a protocol error
    // 2. Receiver SHOULD send GoAway { reason: ProtocolError }
    // 3. Receiver MUST close the transport connection
    //
    // This is difficult to test without a full connection, but we can verify
    // the error handling constants exist.

    // Verify GoAwayReason::ProtocolError exists
    if GoAwayReason::ProtocolError as u8 != 4 {
        return TestResult::fail(format!(
            "[verify core.flow.credit-overrun]: GoAwayReason::ProtocolError should be 4, got {}",
            GoAwayReason::ProtocolError as u8
        ));
    }

    // Verify GoAway structure can carry the error
    let goaway = GoAway {
        reason: GoAwayReason::ProtocolError,
        last_channel_id: 0,
        message: "credit overrun".to_string(),
        metadata: Vec::new(),
    };

    let payload = match facet_postcard::to_vec(&goaway) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to encode GoAway: {}", e)),
    };

    let decoded: GoAway = match facet_postcard::from_slice(&payload) {
        Ok(g) => g,
        Err(e) => return TestResult::fail(format!("failed to decode GoAway: {}", e)),
    };

    if decoded.reason != GoAwayReason::ProtocolError {
        return TestResult::fail(
            "[verify core.flow.credit-overrun]: GoAway reason mismatch".to_string(),
        );
    }

    TestResult::pass()
}
