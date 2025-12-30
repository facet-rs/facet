//! Flow control conformance tests.
//!
//! Tests for spec rules related to credit-based flow control.

use crate::harness::Peer;
use crate::testcase::TestResult;
use rapace_spec_tester_macros::conformance;

// =============================================================================
// flow.credit_additive
// =============================================================================
// Rules: [verify core.flow.credit-additive]
//
// Credits from multiple GrantCredits messages are additive.

#[conformance(name = "flow.credit_additive", rules = "core.flow.credit-additive")]
pub async fn credit_additive(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// flow.credit_in_flags
// =============================================================================
// Rules: [verify core.flow.credit-semantics]
//
// The CREDITS flag indicates credit_grant field is valid.

#[conformance(name = "flow.credit_in_flags", rules = "core.flow.credit-semantics")]
pub async fn credit_in_flags(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// flow.eos_no_credits
// =============================================================================
// Rules: [verify core.flow.eos-no-credits]
//
// EOS-only frames don't consume credits.

#[conformance(name = "flow.eos_no_credits", rules = "core.flow.eos-no-credits")]
pub async fn eos_no_credits(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// flow.infinite_credit
// =============================================================================
// Rules: [verify core.flow.infinite-credit]
//
// Credit value 0xFFFFFFFF means unlimited.

#[conformance(name = "flow.infinite_credit", rules = "core.flow.infinite-credit")]
pub async fn infinite_credit(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// flow.intro
// =============================================================================
// Rules: [verify core.flow.intro]
//
// Rapace MUST use credit-based flow control per channel.

#[conformance(name = "flow.intro", rules = "core.flow.intro")]
pub async fn intro(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// flow.credit_overrun
// =============================================================================
// Rules: [verify core.flow.credit-overrun]
//
// If payload_len exceeds remaining credits, it's a protocol error.
// Receiver SHOULD send GoAway and MUST close the connection.

#[conformance(name = "flow.credit_overrun", rules = "core.flow.credit-overrun")]
pub async fn credit_overrun(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}
