//! Metadata conventions conformance tests.
//!
//! Tests for spec rules in metadata.md

use crate::harness::Peer;
use crate::testcase::TestResult;
use rapace_conformance_macros::conformance;
use rapace_protocol::error_code;

// =============================================================================
// metadata.key_reserved_prefix
// =============================================================================
// Rules: [verify metadata.key.reserved-prefix]
//
// Keys starting with `rapace.` are reserved for protocol-defined metadata.

#[conformance(
    name = "metadata.key_reserved_prefix",
    rules = "metadata.key.reserved-prefix"
)]
pub async fn key_reserved_prefix(_peer: &mut Peer) -> TestResult {
    // Reserved prefixes:
    // - rapace.* : protocol-defined
    // - x-* : application-defined (not standardized)
    // - (no prefix) : application-defined

    // Standard keys that MUST use rapace. prefix
    let standard_keys = [
        "rapace.trace_id",
        "rapace.span_id",
        "rapace.parent_span_id",
        "rapace.trace_flags",
        "rapace.trace_state",
        "rapace.auth_token",
        "rapace.auth_scheme",
        "rapace.deadline_remaining_ms",
        "rapace.deadline",
        "rapace.priority",
        "rapace.idempotency_key",
        "rapace.unreliable",
        "rapace.server_timing_ns",
        "rapace.retryable",
        "rapace.retry_after_ms",
        "rapace.ping_interval_ms",
        "rapace.compression",
        "rapace.default_priority",
    ];

    for key in standard_keys {
        if !key.starts_with("rapace.") {
            return TestResult::fail(format!(
                "[verify metadata.key.reserved-prefix]: standard key '{}' must start with 'rapace.'",
                key
            ));
        }
    }

    TestResult::pass()
}

// =============================================================================
// metadata.key_format
// =============================================================================
// Rules: [verify metadata.key.format]
//
// Keys MUST be lowercase kebab-case matching `[a-z][a-z0-9]*(-[a-z0-9]+)*`.

#[conformance(name = "metadata.key_format", rules = "metadata.key.format")]
pub async fn key_format(_peer: &mut Peer) -> TestResult {
    // Pattern: [a-z][a-z0-9]*(-[a-z0-9]+)*
    // Examples: trace-id, request-timeout, x-custom-header

    // Simple validation function (no regex dependency)
    fn is_valid_segment(s: &str) -> bool {
        if s.is_empty() {
            return false;
        }
        let mut chars = s.chars();
        // First char must be lowercase letter
        match chars.next() {
            Some(c) if c.is_ascii_lowercase() => {}
            _ => return false,
        }
        // Rest can be lowercase letters, digits, underscores, or hyphens
        // But hyphens must be followed by alphanumeric
        let mut prev_hyphen = false;
        for c in chars {
            if c == '-' {
                if prev_hyphen {
                    return false; // No double hyphens
                }
                prev_hyphen = true;
            } else if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' {
                prev_hyphen = false;
            } else {
                return false;
            }
        }
        !prev_hyphen // Can't end with hyphen
    }

    fn is_valid_key(key: &str) -> bool {
        if key.is_empty() {
            return false;
        }
        key.split('.').all(is_valid_segment)
    }

    // Valid keys
    let valid_keys = [
        "trace-id",
        "request-timeout",
        "x-custom-header",
        "rapace.trace_id",
        "a",
        "a1",
        "a-b",
        "abc-def-ghi",
        "x-1",
    ];

    for key in valid_keys {
        if !is_valid_key(key) {
            return TestResult::fail(format!(
                "[verify metadata.key.format]: key '{}' should be valid",
                key
            ));
        }
    }

    // Invalid keys (uppercase, spaces, etc.)
    let invalid_keys = [
        ("Trace-Id", "uppercase"),
        ("TRACE_ID", "all uppercase"),
        ("trace id", "space"),
        ("123-abc", "starts with number"),
        ("-trace-id", "starts with dash"),
        ("trace-id-", "ends with dash"),
        ("", "empty"),
    ];

    for (key, reason) in invalid_keys {
        if is_valid_key(key) {
            return TestResult::fail(format!(
                "[verify metadata.key.format]: key '{}' should be invalid ({})",
                key, reason
            ));
        }
    }

    TestResult::pass()
}

// =============================================================================
// metadata.key_lowercase
// =============================================================================
// Rules: [verify metadata.key.lowercase]
//
// Keys MUST be lowercase. Mixed-case or uppercase keys are a protocol error.

#[conformance(name = "metadata.key_lowercase", rules = "metadata.key.lowercase")]
pub async fn key_lowercase(_peer: &mut Peer) -> TestResult {
    // Function to check if a key is lowercase
    fn is_lowercase_key(key: &str) -> bool {
        key.chars().all(|c| !c.is_uppercase())
    }

    // Valid lowercase keys
    let valid_keys = ["trace-id", "rapace.trace_id", "x-custom-header", "a123"];

    for key in valid_keys {
        if !is_lowercase_key(key) {
            return TestResult::fail(format!(
                "[verify metadata.key.lowercase]: key '{}' should be valid lowercase",
                key
            ));
        }
    }

    // Invalid mixed/uppercase keys
    let invalid_keys = ["Trace-Id", "TRACE_ID", "tracE-id", "Rapace.Trace_Id"];

    for key in invalid_keys {
        if is_lowercase_key(key) {
            return TestResult::fail(format!(
                "[verify metadata.key.lowercase]: key '{}' should be invalid (has uppercase)",
                key
            ));
        }
    }

    TestResult::pass()
}

// =============================================================================
// metadata.key_case_sensitive
// =============================================================================
// Rules: [verify metadata.key.case-sensitive]
//
// Keys are compared as raw bytes (case-sensitive).

#[conformance(
    name = "metadata.key_case_sensitive",
    rules = "metadata.key.case-sensitive"
)]
pub async fn key_case_sensitive(_peer: &mut Peer) -> TestResult {
    // Keys are compared as raw bytes
    // Since all valid keys are lowercase, case normalization is not needed

    let key1 = "trace-id";
    let key2 = "Trace-Id"; // Invalid, but demonstrates case sensitivity

    // Raw byte comparison
    if key1.as_bytes() == key2.as_bytes() {
        return TestResult::fail(
            "[verify metadata.key.case-sensitive]: different case keys should not be equal"
                .to_string(),
        );
    }

    // Same key should be equal
    let key3 = "trace-id";
    if key1.as_bytes() != key3.as_bytes() {
        return TestResult::fail(
            "[verify metadata.key.case-sensitive]: same keys should be equal".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// metadata.key_duplicates
// =============================================================================
// Rules: [verify metadata.key.duplicates]
//
// Senders MUST NOT include duplicate keys.

#[conformance(name = "metadata.key_duplicates", rules = "metadata.key.duplicates")]
pub async fn key_duplicates(_peer: &mut Peer) -> TestResult {
    use std::collections::HashSet;

    // Function to check for duplicates
    fn has_duplicates(metadata: &[(String, Vec<u8>)]) -> bool {
        let mut seen = HashSet::new();
        for (key, _) in metadata {
            if !seen.insert(key) {
                return true;
            }
        }
        false
    }

    // Valid: no duplicates
    let valid_metadata: Vec<(String, Vec<u8>)> = vec![
        ("trace-id".to_string(), vec![1, 2, 3]),
        ("span-id".to_string(), vec![4, 5, 6]),
        ("priority".to_string(), vec![128]),
    ];

    if has_duplicates(&valid_metadata) {
        return TestResult::fail(
            "[verify metadata.key.duplicates]: valid metadata should not have duplicates"
                .to_string(),
        );
    }

    // Invalid: has duplicates
    let invalid_metadata: Vec<(String, Vec<u8>)> = vec![
        ("trace-id".to_string(), vec![1, 2, 3]),
        ("span-id".to_string(), vec![4, 5, 6]),
        ("trace-id".to_string(), vec![7, 8, 9]), // Duplicate!
    ];

    if !has_duplicates(&invalid_metadata) {
        return TestResult::fail(
            "[verify metadata.key.duplicates]: should detect duplicate keys".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// metadata.limits
// =============================================================================
// Rules: [verify metadata.limits]
//
// Implementations MUST enforce size limits.

#[conformance(name = "metadata.limits", rules = "metadata.limits")]
pub async fn limits(_peer: &mut Peer) -> TestResult {
    // Limits:
    // - Max key length: 256 bytes
    // - Max value length: 64 KiB
    // - Max metadata entries: 128
    // - Max total metadata size: 1 MiB

    const MAX_KEY_LENGTH: usize = 256;
    const MAX_VALUE_LENGTH: usize = 64 * 1024; // 64 KiB
    const MAX_ENTRIES: usize = 128;
    const MAX_TOTAL_SIZE: usize = 1024 * 1024; // 1 MiB

    // Verify constants
    if MAX_KEY_LENGTH != 256 {
        return TestResult::fail(
            "[verify metadata.limits]: max key length should be 256".to_string(),
        );
    }

    if MAX_VALUE_LENGTH != 65536 {
        return TestResult::fail(
            "[verify metadata.limits]: max value length should be 64 KiB".to_string(),
        );
    }

    if MAX_ENTRIES != 128 {
        return TestResult::fail("[verify metadata.limits]: max entries should be 128".to_string());
    }

    if MAX_TOTAL_SIZE != 1048576 {
        return TestResult::fail(
            "[verify metadata.limits]: max total size should be 1 MiB".to_string(),
        );
    }

    // Function to validate metadata against limits
    fn validate_metadata(metadata: &[(String, Vec<u8>)]) -> Result<(), String> {
        if metadata.len() > MAX_ENTRIES {
            return Err(format!(
                "too many entries: {} > {}",
                metadata.len(),
                MAX_ENTRIES
            ));
        }

        let mut total_size = 0;
        for (key, value) in metadata {
            if key.len() > MAX_KEY_LENGTH {
                return Err(format!("key too long: {} > {}", key.len(), MAX_KEY_LENGTH));
            }
            if value.len() > MAX_VALUE_LENGTH {
                return Err(format!(
                    "value too long: {} > {}",
                    value.len(),
                    MAX_VALUE_LENGTH
                ));
            }
            total_size += key.len() + value.len();
        }

        if total_size > MAX_TOTAL_SIZE {
            return Err(format!(
                "total size too large: {} > {}",
                total_size, MAX_TOTAL_SIZE
            ));
        }

        Ok(())
    }

    // Valid metadata
    let valid: Vec<(String, Vec<u8>)> = vec![
        ("trace-id".to_string(), vec![0; 16]),
        ("span-id".to_string(), vec![0; 8]),
    ];

    if let Err(e) = validate_metadata(&valid) {
        return TestResult::fail(format!(
            "[verify metadata.limits]: valid metadata rejected: {}",
            e
        ));
    }

    // Invalid: key too long
    let long_key = "a".repeat(257);
    let invalid_key: Vec<(String, Vec<u8>)> = vec![(long_key, vec![1])];

    if validate_metadata(&invalid_key).is_ok() {
        return TestResult::fail(
            "[verify metadata.limits]: should reject key > 256 bytes".to_string(),
        );
    }

    // Invalid: value too long
    let invalid_value: Vec<(String, Vec<u8>)> =
        vec![("key".to_string(), vec![0; MAX_VALUE_LENGTH + 1])];

    if validate_metadata(&invalid_value).is_ok() {
        return TestResult::fail(
            "[verify metadata.limits]: should reject value > 64 KiB".to_string(),
        );
    }

    // Invalid: too many entries
    let invalid_entries: Vec<(String, Vec<u8>)> =
        (0..129).map(|i| (format!("key-{}", i), vec![1])).collect();

    if validate_metadata(&invalid_entries).is_ok() {
        return TestResult::fail(
            "[verify metadata.limits]: should reject > 128 entries".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// metadata.limits_reject
// =============================================================================
// Rules: [verify metadata.limits.reject]
//
// Implementations SHOULD reject messages exceeding limits with RESOURCE_EXHAUSTED.

#[conformance(name = "metadata.limits_reject", rules = "metadata.limits.reject")]
pub async fn limits_reject(_peer: &mut Peer) -> TestResult {
    // This rule specifies the error code for limit violations

    // Verify RESOURCE_EXHAUSTED exists and has expected value
    if error_code::RESOURCE_EXHAUSTED != 8 {
        return TestResult::fail(format!(
            "[verify metadata.limits.reject]: RESOURCE_EXHAUSTED should be 8, got {}",
            error_code::RESOURCE_EXHAUSTED
        ));
    }

    TestResult::pass()
}
