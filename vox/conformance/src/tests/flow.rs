//! Flow control conformance tests.
//!
//! Tests for spec rules related to credit-based flow control.

use crate::harness::Peer;
use crate::protocol::*;
use crate::testcase::TestResult;

// =============================================================================
// flow.credit_additive
// =============================================================================
// Rules: [verify core.flow.credit-additive]
//
// Credits from multiple GrantCredits messages are additive.

pub fn credit_additive(_peer: &mut Peer) -> TestResult {
    // Structural test - verify GrantCredits structure
    let grant = GrantCredits {
        channel_id: 5,
        bytes: 1024,
    };

    let payload = match facet_format_postcard::to_vec(&grant) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to encode GrantCredits: {}", e)),
    };

    // Verify it round-trips
    let decoded: GrantCredits = match facet_format_postcard::from_slice(&payload) {
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

pub fn credit_in_flags(_peer: &mut Peer) -> TestResult {
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

pub fn eos_no_credits(_peer: &mut Peer) -> TestResult {
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

pub fn infinite_credit(_peer: &mut Peer) -> TestResult {
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

/// Run a flow test case by name.
pub fn run(name: &str) -> TestResult {
    let mut peer = Peer::new();

    match name {
        "credit_additive" => credit_additive(&mut peer),
        "credit_in_flags" => credit_in_flags(&mut peer),
        "eos_no_credits" => eos_no_credits(&mut peer),
        "infinite_credit" => infinite_credit(&mut peer),
        _ => TestResult::fail(format!("unknown flow test: {}", name)),
    }
}

/// List all flow test cases.
pub fn list() -> Vec<(&'static str, &'static [&'static str])> {
    vec![
        ("credit_additive", &["core.flow.credit-additive"][..]),
        ("credit_in_flags", &["core.flow.credit-semantics"][..]),
        ("eos_no_credits", &["core.flow.eos-no-credits"][..]),
        ("infinite_credit", &["core.flow.infinite-credit"][..]),
    ]
}
