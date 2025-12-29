//! Error handling conformance tests.
//!
//! Tests for spec rules in errors.md

use crate::harness::Peer;
use crate::protocol::*;
use crate::testcase::TestResult;
use rapace_conformance_macros::conformance;

// =============================================================================
// error.status_codes
// =============================================================================
// Rules: [verify error.impl.standard-codes]
//
// Validates standard error code values.

#[conformance(name = "error.status_codes", rules = "error.impl.standard-codes")]
pub fn status_codes(_peer: &mut Peer) -> TestResult {
    // Verify code values match spec
    let checks = [
        (error_code::OK, 0, "OK"),
        (error_code::CANCELLED, 1, "CANCELLED"),
        (error_code::UNKNOWN, 2, "UNKNOWN"),
        (error_code::INVALID_ARGUMENT, 3, "INVALID_ARGUMENT"),
        (error_code::DEADLINE_EXCEEDED, 4, "DEADLINE_EXCEEDED"),
        (error_code::NOT_FOUND, 5, "NOT_FOUND"),
        (error_code::ALREADY_EXISTS, 6, "ALREADY_EXISTS"),
        (error_code::PERMISSION_DENIED, 7, "PERMISSION_DENIED"),
        (error_code::RESOURCE_EXHAUSTED, 8, "RESOURCE_EXHAUSTED"),
        (error_code::FAILED_PRECONDITION, 9, "FAILED_PRECONDITION"),
        (error_code::ABORTED, 10, "ABORTED"),
        (error_code::OUT_OF_RANGE, 11, "OUT_OF_RANGE"),
        (error_code::UNIMPLEMENTED, 12, "UNIMPLEMENTED"),
        (error_code::INTERNAL, 13, "INTERNAL"),
        (error_code::UNAVAILABLE, 14, "UNAVAILABLE"),
        (error_code::DATA_LOSS, 15, "DATA_LOSS"),
        (error_code::UNAUTHENTICATED, 16, "UNAUTHENTICATED"),
        (error_code::INCOMPATIBLE_SCHEMA, 17, "INCOMPATIBLE_SCHEMA"),
    ];

    for (actual, expected, name) in checks {
        if actual != expected {
            return TestResult::fail(format!(
                "[verify error.impl.standard-codes]: {} should be {}, got {}",
                name, expected, actual
            ));
        }
    }

    TestResult::pass()
}

// =============================================================================
// error.protocol_codes
// =============================================================================
// Rules: [verify error.impl.standard-codes]
//
// Validates protocol error code values (50-99 range).

#[conformance(name = "error.protocol_codes", rules = "error.impl.standard-codes")]
pub fn protocol_codes(_peer: &mut Peer) -> TestResult {
    let checks = [
        (error_code::PROTOCOL_ERROR, 50, "PROTOCOL_ERROR"),
        (error_code::INVALID_FRAME, 51, "INVALID_FRAME"),
        (error_code::INVALID_CHANNEL, 52, "INVALID_CHANNEL"),
        (error_code::INVALID_METHOD, 53, "INVALID_METHOD"),
        (error_code::DECODE_ERROR, 54, "DECODE_ERROR"),
        (error_code::ENCODE_ERROR, 55, "ENCODE_ERROR"),
    ];

    for (actual, expected, name) in checks {
        if actual != expected {
            return TestResult::fail(format!(
                "protocol error code {} should be {}, got {}",
                name, expected, actual
            ));
        }
    }

    TestResult::pass()
}

// =============================================================================
// error.status_success
// =============================================================================
// Rules: [verify error.status.success]
//
// On success, status.code must be 0 and body must be present.

#[conformance(name = "error.status_success", rules = "error.status.success")]
pub fn status_success(_peer: &mut Peer) -> TestResult {
    let result = CallResult {
        status: Status::ok(),
        trailers: Vec::new(),
        body: Some(vec![1, 2, 3]),
    };

    if result.status.code != 0 {
        return TestResult::fail("[verify error.status.success]: success status.code should be 0");
    }

    if result.body.is_none() {
        return TestResult::fail("[verify error.status.success]: success should have body");
    }

    TestResult::pass()
}

// =============================================================================
// error.status_error
// =============================================================================
// Rules: [verify error.status.error]
//
// On error, status.code must not be 0 and body must be None.

#[conformance(name = "error.status_error", rules = "error.status.error")]
pub fn status_error(_peer: &mut Peer) -> TestResult {
    let result = CallResult {
        status: Status::error(error_code::NOT_FOUND, "not found"),
        trailers: Vec::new(),
        body: None,
    };

    if result.status.code == 0 {
        return TestResult::fail("[verify error.status.error]: error status.code should not be 0");
    }

    if result.body.is_some() {
        return TestResult::fail("[verify error.status.error]: error should not have body");
    }

    TestResult::pass()
}

// =============================================================================
// error.cancel_reasons
// =============================================================================
// Rules: [verify core.cancel.behavior]
//
// Validates CancelReason enum values.

#[conformance(name = "error.cancel_reasons", rules = "core.cancel.behavior")]
pub fn cancel_reasons(_peer: &mut Peer) -> TestResult {
    // Verify discriminants
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
                "CancelReason::{} should be {}, got {}",
                name, expected, actual
            ));
        }
    }

    TestResult::pass()
}
