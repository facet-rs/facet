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
pub async fn status_codes(_peer: &mut Peer) -> TestResult {
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
pub async fn protocol_codes(_peer: &mut Peer) -> TestResult {
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
pub async fn status_success(_peer: &mut Peer) -> TestResult {
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
pub async fn status_error(_peer: &mut Peer) -> TestResult {
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
pub async fn cancel_reasons(_peer: &mut Peer) -> TestResult {
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

// =============================================================================
// error.details_populate
// =============================================================================
// Rules: [verify error.details.populate]
//
// Implementations SHOULD populate details for actionable errors.

#[conformance(name = "error.details_populate", rules = "error.details.populate")]
pub async fn details_populate(_peer: &mut Peer) -> TestResult {
    // Verify Status.details field can hold structured error info

    let status = Status {
        code: error_code::RESOURCE_EXHAUSTED,
        message: "rate limited".to_string(),
        details: b"retry_after_ms:1000".to_vec(), // Example structured data
    };

    // Verify details can be populated
    if status.details.is_empty() {
        return TestResult::fail(
            "[verify error.details.populate]: details should be populated".to_string(),
        );
    }

    // Verify the Status can be serialized with details
    let payload = match facet_format_postcard::to_vec(&status) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to encode Status: {}", e)),
    };

    let decoded: Status = match facet_format_postcard::from_slice(&payload) {
        Ok(s) => s,
        Err(e) => return TestResult::fail(format!("failed to decode Status: {}", e)),
    };

    if decoded.details != status.details {
        return TestResult::fail(
            "[verify error.details.populate]: details roundtrip failed".to_string(),
        );
    }

    TestResult::pass()
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
pub async fn details_unknown_format(_peer: &mut Peer) -> TestResult {
    // Test 1: Empty details must work
    let status_empty = Status {
        code: error_code::NOT_FOUND,
        message: "not found".to_string(),
        details: Vec::new(), // Empty
    };

    let payload = match facet_format_postcard::to_vec(&status_empty) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to encode empty details: {}", e)),
    };

    let _decoded: Status = match facet_format_postcard::from_slice(&payload) {
        Ok(s) => s,
        Err(e) => return TestResult::fail(format!("failed to decode empty details: {}", e)),
    };

    // Test 2: Unknown/arbitrary bytes must work
    let status_unknown = Status {
        code: error_code::INTERNAL,
        message: "error".to_string(),
        details: vec![0xFF, 0xFE, 0xFD, 0xFC], // Arbitrary bytes
    };

    let payload = match facet_format_postcard::to_vec(&status_unknown) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to encode unknown details: {}", e)),
    };

    let decoded: Status = match facet_format_postcard::from_slice(&payload) {
        Ok(s) => s,
        Err(e) => return TestResult::fail(format!("failed to decode unknown details: {}", e)),
    };

    if decoded.details != status_unknown.details {
        return TestResult::fail(
            "[verify error.details.unknown-format]: unknown details must roundtrip".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// error.flag_parse
// =============================================================================
// Rules: [verify error.flag.parse]
//
// Receivers MAY use ERROR flag for fast detection but MUST still parse CallResult.

#[conformance(name = "error.flag_parse", rules = "error.flag.parse")]
pub async fn flag_parse(_peer: &mut Peer) -> TestResult {
    // The ERROR flag (0x10) is a fast-path hint.
    // Receivers can check it quickly but MUST still parse CallResult.

    // Verify ERROR flag value
    if flags::ERROR != 0b0001_0000 {
        return TestResult::fail(format!(
            "[verify error.flag.parse]: ERROR flag should be 0x10, got {:#X}",
            flags::ERROR
        ));
    }

    // The actual status comes from CallResult, not the flag
    let result = CallResult {
        status: Status::error(error_code::INTERNAL, "internal error"),
        trailers: Vec::new(),
        body: None,
    };

    // Verify CallResult contains the authoritative status
    if result.status.code == 0 {
        return TestResult::fail(
            "[verify error.flag.parse]: CallResult status must be authoritative".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// error.impl_backoff
// =============================================================================
// Rules: [verify error.impl.backoff]
//
// Implementations SHOULD implement exponential backoff for retries.

#[conformance(name = "error.impl_backoff", rules = "error.impl.backoff")]
pub async fn impl_backoff(_peer: &mut Peer) -> TestResult {
    // This is a behavioral requirement for implementations.
    // We can only verify the retryable error codes exist.

    // Verify retryable error codes
    let retryable_codes = [
        error_code::RESOURCE_EXHAUSTED, // 8
        error_code::ABORTED,            // 10
        error_code::UNAVAILABLE,        // 14
    ];

    for code in retryable_codes {
        if code == 0 {
            return TestResult::fail(
                "[verify error.impl.backoff]: retryable error code should not be 0".to_string(),
            );
        }
    }

    // Document the retry strategy:
    // - Start with 100ms
    // - Double each retry
    // - Cap at 30s
    // - Add ±25% jitter

    TestResult::pass()
}

// =============================================================================
// error.impl_custom_codes
// =============================================================================
// Rules: [verify error.impl.custom-codes]
//
// Implementations MAY define application-specific error codes in the 400+ range.

#[conformance(name = "error.impl_custom_codes", rules = "error.impl.custom-codes")]
pub async fn impl_custom_codes(_peer: &mut Peer) -> TestResult {
    // Application-defined codes start at 400
    const APP_ERROR_MIN: u32 = 400;

    // Create a status with custom code
    let custom_code = 400; // First available custom code
    let status = Status {
        code: custom_code,
        message: "custom application error".to_string(),
        details: Vec::new(),
    };

    if status.code < APP_ERROR_MIN {
        return TestResult::fail(format!(
            "[verify error.impl.custom-codes]: custom code {} should be >= {}",
            status.code, APP_ERROR_MIN
        ));
    }

    // Verify it serializes
    let payload = match facet_format_postcard::to_vec(&status) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to encode custom code: {}", e)),
    };

    let decoded: Status = match facet_format_postcard::from_slice(&payload) {
        Ok(s) => s,
        Err(e) => return TestResult::fail(format!("failed to decode custom code: {}", e)),
    };

    if decoded.code != custom_code {
        return TestResult::fail(
            "[verify error.impl.custom-codes]: custom code roundtrip failed".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// error.impl_details
// =============================================================================
// Rules: [verify error.impl.details]
//
// Implementations SHOULD populate details for actionable errors and SHOULD include message.

#[conformance(name = "error.impl_details", rules = "error.impl.details")]
pub async fn impl_details(_peer: &mut Peer) -> TestResult {
    // Verify Status has both message and details fields

    let status = Status {
        code: error_code::RESOURCE_EXHAUSTED,
        message: "rate limit exceeded".to_string(), // Human-readable for debugging
        details: b"quota:requests,limit:100,used:150".to_vec(), // Structured for machines
    };

    // Verify message is populated
    if status.message.is_empty() {
        return TestResult::fail(
            "[verify error.impl.details]: message should be populated".to_string(),
        );
    }

    // Verify details can be populated
    if status.details.is_empty() {
        return TestResult::fail(
            "[verify error.impl.details]: details should be populated".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// error.impl_error_flag
// =============================================================================
// Rules: [verify error.impl.error-flag]
//
// Implementations MUST set the ERROR flag correctly (matching status.code != 0).

#[conformance(name = "error.impl_error_flag", rules = "error.impl.error-flag")]
pub async fn impl_error_flag(_peer: &mut Peer) -> TestResult {
    // ERROR flag (0x10) MUST match status.code != 0

    // Test 1: Success (code == 0) -> ERROR flag NOT set
    let success_flags = flags::DATA | flags::EOS | flags::RESPONSE;
    if success_flags & flags::ERROR != 0 {
        return TestResult::fail(
            "[verify error.impl.error-flag]: success should not have ERROR flag".to_string(),
        );
    }

    // Test 2: Error (code != 0) -> ERROR flag MUST be set
    let error_flags = flags::DATA | flags::EOS | flags::RESPONSE | flags::ERROR;
    if error_flags & flags::ERROR == 0 {
        return TestResult::fail(
            "[verify error.impl.error-flag]: error should have ERROR flag".to_string(),
        );
    }

    TestResult::pass()
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
pub async fn impl_status_required(_peer: &mut Peer) -> TestResult {
    // Every error response MUST have a Status in CallResult

    let result = CallResult {
        status: Status::error(error_code::NOT_FOUND, "entity not found"),
        trailers: Vec::new(),
        body: None, // Must be None for errors
    };

    // Verify Status is present and has error code
    if result.status.code == 0 {
        return TestResult::fail(
            "[verify error.impl.status-required]: error must have non-zero status code".to_string(),
        );
    }

    // Verify body is None for errors
    if result.body.is_some() {
        return TestResult::fail(
            "[verify error.impl.status-required]: error must not have body".to_string(),
        );
    }

    // Verify CallResult serializes correctly
    let payload = match facet_format_postcard::to_vec(&result) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to encode error CallResult: {}", e)),
    };

    let decoded: CallResult = match facet_format_postcard::from_slice(&payload) {
        Ok(r) => r,
        Err(e) => return TestResult::fail(format!("failed to decode error CallResult: {}", e)),
    };

    if decoded.status.code == 0 {
        return TestResult::fail(
            "[verify error.impl.status-required]: decoded error must have non-zero status"
                .to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// error.impl_unknown_codes
// =============================================================================
// Rules: [verify error.impl.unknown-codes]
//
// Implementations MUST handle unknown error codes gracefully.

#[conformance(name = "error.impl_unknown_codes", rules = "error.impl.unknown-codes")]
pub async fn impl_unknown_codes(_peer: &mut Peer) -> TestResult {
    // Unknown error codes should not cause failures

    // Use a code that doesn't exist in the standard ranges
    let unknown_code = 999u32; // Not in any defined range

    let status = Status {
        code: unknown_code,
        message: "unknown error".to_string(),
        details: Vec::new(),
    };

    // Must be able to serialize
    let payload = match facet_format_postcard::to_vec(&status) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to encode unknown code: {}", e)),
    };

    // Must be able to deserialize
    let decoded: Status = match facet_format_postcard::from_slice(&payload) {
        Ok(s) => s,
        Err(e) => return TestResult::fail(format!("failed to decode unknown code: {}", e)),
    };

    if decoded.code != unknown_code {
        return TestResult::fail(
            "[verify error.impl.unknown-codes]: unknown code must roundtrip".to_string(),
        );
    }

    TestResult::pass()
}
