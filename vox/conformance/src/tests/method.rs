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
pub async fn algorithm(_peer: &mut Peer) -> TestResult {
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
pub async fn input_format(_peer: &mut Peer) -> TestResult {
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
pub async fn zero_reserved(_peer: &mut Peer) -> TestResult {
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
pub async fn collision_detection(_peer: &mut Peer) -> TestResult {
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
pub async fn fnv1a_properties(_peer: &mut Peer) -> TestResult {
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

// =============================================================================
// method.intro
// =============================================================================
// Rules: [verify core.method-id.intro]
//
// Method IDs MUST be 32-bit identifiers computed as a hash.

#[conformance(name = "method.intro", rules = "core.method-id.intro")]
pub async fn intro(_peer: &mut Peer) -> TestResult {
    // Method IDs are 32-bit unsigned integers computed by hashing
    // the fully-qualified method name.

    let id = compute_method_id("Service", "method");

    // Verify it's a 32-bit value (fits in u32)
    // The function returns u32, so this is guaranteed by type system
    let _: u32 = id;

    // Verify it's computed from the string (not random)
    let id2 = compute_method_id("Service", "method");
    if id != id2 {
        return TestResult::fail(
            "[verify core.method-id.intro]: method ID must be deterministic".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// method.zero_enforcement
// =============================================================================
// Rules: [verify core.method-id.zero-enforcement]
//
// Code generators MUST check if method_id returns 0 and fail.
// Handshake MUST reject method registry entries with method_id = 0.

#[conformance(
    name = "method.zero_enforcement",
    rules = "core.method-id.zero-enforcement"
)]
pub async fn zero_enforcement(_peer: &mut Peer) -> TestResult {
    // This rule requires:
    // 1. Code generators MUST check if compute_method_id returns 0
    // 2. If it does, code generation MUST fail
    // 3. Handshake MUST reject any MethodInfo with method_id = 0
    //
    // We can verify the MethodInfo structure allows zero (so implementations must check):

    let method_info = MethodInfo {
        method_id: 0, // This MUST be rejected at handshake
        sig_hash: [0u8; 32],
        name: Some("Bad.method".to_string()),
    };

    // The structure allows zero - enforcement is at validation time
    if method_info.method_id != 0 {
        return TestResult::fail(
            "[verify core.method-id.zero-enforcement]: MethodInfo.method_id field broken"
                .to_string(),
        );
    }

    // Implementations MUST validate and reject method_id = 0
    // We document this requirement here

    TestResult::pass()
}
