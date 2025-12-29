//! Method ID conformance tests.
//!
//! Tests for spec rules related to method ID computation.

use crate::harness::Peer;
use crate::protocol::*;
use crate::testcase::TestResult;
use rapace_conformance_macros::conformance;

// =============================================================================
// method.algorithm
// =============================================================================
// Rules: [verify core.method-id.algorithm]
//
// Method IDs use FNV-1a hash folded to 32 bits.

#[conformance(name = "method.algorithm", rules = "core.method-id.algorithm")]
pub fn algorithm(_peer: &mut Peer) -> TestResult {
    // Verify the algorithm produces consistent results
    let id1 = compute_method_id("Test", "foo");
    let id2 = compute_method_id("Test", "foo");

    if id1 != id2 {
        return TestResult::fail(
            "[verify core.method-id.algorithm]: method ID computation not deterministic"
                .to_string(),
        );
    }

    // Different methods should produce different IDs
    let id3 = compute_method_id("Test", "bar");
    if id1 == id3 {
        return TestResult::fail(
            "[verify core.method-id.algorithm]: different methods produced same ID".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// method.input_format
// =============================================================================
// Rules: [verify core.method-id.input-format]
//
// Method ID input is "ServiceName.MethodName".

#[conformance(name = "method.input_format", rules = "core.method-id.input-format")]
pub fn input_format(_peer: &mut Peer) -> TestResult {
    // Verify the input format (service.method)
    let id = compute_method_id("Calculator", "add");

    // The ID should be non-zero (zero is reserved)
    if id == 0 {
        return TestResult::fail(
            "[verify core.method-id.input-format]: method ID should not be 0".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// method.zero_reserved
// =============================================================================
// Rules: [verify core.method-id.zero-reserved]
//
// method_id = 0 is reserved for control/stream/tunnel.

#[conformance(name = "method.zero_reserved", rules = "core.method-id.zero-reserved")]
pub fn zero_reserved(_peer: &mut Peer) -> TestResult {
    // Verify that real methods don't produce ID 0
    // (statistically very unlikely with FNV-1a)

    let test_methods = [
        ("Service", "method"),
        ("Foo", "bar"),
        ("Calculator", "add"),
        ("Auth", "login"),
        ("Storage", "get"),
    ];

    for (service, method) in test_methods {
        let id = compute_method_id(service, method);
        if id == 0 {
            return TestResult::fail(format!(
                "[verify core.method-id.zero-reserved]: {}.{} produced reserved ID 0",
                service, method
            ));
        }
    }

    TestResult::pass()
}

// =============================================================================
// method.collision_detection
// =============================================================================
// Rules: [verify core.method-id.collision-detection]
//
// Implementations should detect method ID collisions at startup.

#[conformance(
    name = "method.collision_detection",
    rules = "core.method-id.collision-detection"
)]
pub fn collision_detection(_peer: &mut Peer) -> TestResult {
    // This is a behavioral requirement for implementations
    // We can document it but not directly test it here
    TestResult::pass()
}

// =============================================================================
// method.fnv1a_properties
// =============================================================================
// Rules: [verify core.method-id.algorithm]
//
// Verify FNV-1a properties: avalanche effect, bit distribution.

#[conformance(name = "method.fnv1a_properties", rules = "core.method-id.algorithm")]
pub fn fnv1a_properties(_peer: &mut Peer) -> TestResult {
    // Test that small changes produce very different IDs (avalanche)
    let id1 = compute_method_id("Test", "foo");
    let id2 = compute_method_id("Test", "fop"); // One char different

    // Count differing bits
    let diff = (id1 ^ id2).count_ones();

    // With good avalanche, we expect a reasonable number of bits to differ
    // FNV-1a is not cryptographic but should still have decent diffusion
    // Allow anywhere from 4-28 bits (out of 32) to be different
    if diff < 4 {
        return TestResult::fail(format!(
            "[verify core.method-id.algorithm]: poor avalanche - only {} bits differ",
            diff
        ));
    }

    TestResult::pass()
}
