//! Priority and QoS conformance tests.
//!
//! Tests for spec rules in prioritization.md

use crate::harness::Peer;
use crate::testcase::TestResult;
use rapace_spec_tester_macros::conformance;

// =============================================================================
// priority.value_range
// =============================================================================
// Rules: [verify priority.value.range]
//
// Rapace uses an 8-bit priority value (0-255).

#[conformance(name = "priority.value_range", rules = "priority.value.range")]
pub async fn value_range(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// priority.value_default
// =============================================================================
// Rules: [verify priority.value.default]
//
// The default priority MUST be 128 when no priority is specified.

#[conformance(name = "priority.value_default", rules = "priority.value.default")]
pub async fn value_default(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// priority.precedence
// =============================================================================
// Rules: [verify priority.precedence]
//
// Priority sources: per-call metadata > frame flag > connection default.

#[conformance(name = "priority.precedence", rules = "priority.precedence")]
pub async fn precedence(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// priority.high_flag_mapping
// =============================================================================
// Rules: [verify priority.high-flag.mapping]
//
// HIGH_PRIORITY flag MUST be interpreted as priority 192.

#[conformance(
    name = "priority.high_flag_mapping",
    rules = "priority.high-flag.mapping"
)]
pub async fn high_flag_mapping(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// priority.scheduling_queue
// =============================================================================
// Rules: [verify priority.scheduling.queue]
//
// Servers SHOULD use priority-aware scheduling.

#[conformance(
    name = "priority.scheduling_queue",
    rules = "priority.scheduling.queue"
)]
pub async fn scheduling_queue(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// priority.credits_minimum
// =============================================================================
// Rules: [verify priority.credits.minimum]
//
// Low-priority channels MUST receive minimum credits to prevent deadlock.

#[conformance(name = "priority.credits_minimum", rules = "priority.credits.minimum")]
pub async fn credits_minimum(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// priority.propagation_rules
// =============================================================================
// Rules: [verify priority.propagation.rules]
//
// Priority propagation: SHOULD propagate for sync, reduce for fan-out, MUST NOT increase.

#[conformance(
    name = "priority.propagation_rules",
    rules = "priority.propagation.rules"
)]
pub async fn propagation_rules(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// priority.guarantee_starvation
// =============================================================================
// Rules: [verify priority.guarantee.starvation]
//
// Weighted fair queuing MUST ensure every priority level gets service.

#[conformance(
    name = "priority.guarantee_starvation",
    rules = "priority.guarantee.starvation"
)]
pub async fn guarantee_starvation(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// priority.guarantee_ordering
// =============================================================================
// Rules: [verify priority.guarantee.ordering]
//
// Higher priority SHOULD be more likely to be scheduled first.

#[conformance(
    name = "priority.guarantee_ordering",
    rules = "priority.guarantee.ordering"
)]
pub async fn guarantee_ordering(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// priority.guarantee_deadline
// =============================================================================
// Rules: [verify priority.guarantee.deadline]
//
// MUST NOT forget requests with deadlines.

#[conformance(
    name = "priority.guarantee_deadline",
    rules = "priority.guarantee.deadline"
)]
pub async fn guarantee_deadline(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// priority.non_guarantee
// =============================================================================
// Rules: [verify priority.non-guarantee]
//
// NOT required: strict priority, latency bounds, cross-connection fairness.

#[conformance(name = "priority.non_guarantee", rules = "priority.non-guarantee")]
pub async fn non_guarantee(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}
