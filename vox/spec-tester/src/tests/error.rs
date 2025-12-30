//! Error handling conformance tests.
//!
//! Tests for spec rules in errors.md

use crate::harness::Peer;
use crate::testcase::TestResult;
use rapace_spec_tester_macros::conformance;

// =============================================================================
// error.status_codes
// =============================================================================
// Rules: [verify error.impl.standard-codes]
//
// Validates standard error code values.

#[conformance(name = "error.status_codes", rules = "error.impl.standard-codes")]
pub async fn status_codes(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// error.protocol_codes
// =============================================================================
// Rules: [verify error.impl.standard-codes]
//
// Validates protocol error code values (50-99 range).

#[conformance(name = "error.protocol_codes", rules = "error.impl.standard-codes")]
pub async fn protocol_codes(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// error.status_success
// =============================================================================
// Rules: [verify error.status.success]
//
// On success, status.code must be 0 and body must be present.

#[conformance(name = "error.status_success", rules = "error.status.success")]
pub async fn status_success(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// error.status_error
// =============================================================================
// Rules: [verify error.status.error]
//
// On error, status.code must not be 0 and body must be None.

#[conformance(name = "error.status_error", rules = "error.status.error")]
pub async fn status_error(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// error.cancel_reasons
// =============================================================================
// Rules: [verify core.cancel.behavior]
//
// Validates CancelReason enum values.

#[conformance(name = "error.cancel_reasons", rules = "core.cancel.behavior")]
pub async fn cancel_reasons(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// error.details_populate
// =============================================================================
// Rules: [verify error.details.populate]
//
// Implementations SHOULD populate details for actionable errors.

#[conformance(name = "error.details_populate", rules = "error.details.populate")]
pub async fn details_populate(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// error.details_unknown_format
// =============================================================================
// Rules: [verify error.details.unknown-format]
//
// Implementations MUST NOT fail if details is empty or contains unknown format.

#[conformance(
    name = "error.details_unknown_format",
    rules = "error.details.unknown-format"
)]
pub async fn details_unknown_format(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// error.flag_parse
// =============================================================================
// Rules: [verify error.flag.parse]
//
// Receivers MAY use ERROR flag for fast detection but MUST still parse CallResult.

#[conformance(name = "error.flag_parse", rules = "error.flag.parse")]
pub async fn flag_parse(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// error.impl_backoff
// =============================================================================
// Rules: [verify error.impl.backoff]
//
// Implementations SHOULD implement exponential backoff for retries.

#[conformance(name = "error.impl_backoff", rules = "error.impl.backoff")]
pub async fn impl_backoff(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// error.impl_custom_codes
// =============================================================================
// Rules: [verify error.impl.custom-codes]
//
// Implementations MAY define application-specific error codes in the 400+ range.

#[conformance(name = "error.impl_custom_codes", rules = "error.impl.custom-codes")]
pub async fn impl_custom_codes(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// error.impl_details
// =============================================================================
// Rules: [verify error.impl.details]
//
// Implementations SHOULD populate details for actionable errors and SHOULD include message.

#[conformance(name = "error.impl_details", rules = "error.impl.details")]
pub async fn impl_details(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// error.impl_error_flag
// =============================================================================
// Rules: [verify error.impl.error-flag]
//
// Implementations MUST set the ERROR flag correctly (matching status.code != 0).

#[conformance(name = "error.impl_error_flag", rules = "error.impl.error-flag")]
pub async fn impl_error_flag(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// error.impl_status_required
// =============================================================================
// Rules: [verify error.impl.status-required]
//
// Implementations MUST include Status in all error responses.

#[conformance(
    name = "error.impl_status_required",
    rules = "error.impl.status-required"
)]
pub async fn impl_status_required(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// error.impl_unknown_codes
// =============================================================================
// Rules: [verify error.impl.unknown-codes]
//
// Implementations MUST handle unknown error codes gracefully.

#[conformance(name = "error.impl_unknown_codes", rules = "error.impl.unknown-codes")]
pub async fn impl_unknown_codes(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}
