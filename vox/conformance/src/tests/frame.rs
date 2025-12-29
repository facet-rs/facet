//! Frame format conformance tests.
//!
//! Tests for spec rules in frame-format.md

use crate::harness::Peer;
use crate::protocol::*;
use crate::testcase::TestResult;

// =============================================================================
// frame.descriptor_size
// =============================================================================
// Rules: [verify frame.desc.size], [verify frame.desc.sizeof]
//
// Validates that descriptors are exactly 64 bytes.

pub fn descriptor_size(_peer: &mut Peer) -> TestResult {
    // This is a structural test - just verify our types
    if std::mem::size_of::<MsgDescHot>() != 64 {
        return TestResult::fail(format!(
            "[verify frame.desc.sizeof]: MsgDescHot is {} bytes, expected 64",
            std::mem::size_of::<MsgDescHot>()
        ));
    }
    TestResult::pass()
}

// =============================================================================
// frame.inline_payload_max
// =============================================================================
// Rules: [verify frame.payload.inline]
//
// Inline payloads must be ≤16 bytes.

pub fn inline_payload_max(_peer: &mut Peer) -> TestResult {
    if INLINE_PAYLOAD_SIZE != 16 {
        return TestResult::fail(format!(
            "[verify frame.payload.inline]: INLINE_PAYLOAD_SIZE is {}, expected 16",
            INLINE_PAYLOAD_SIZE
        ));
    }
    TestResult::pass()
}

// =============================================================================
// frame.sentinel_inline
// =============================================================================
// Rules: [verify frame.sentinel.values]
//
// payload_slot = 0xFFFFFFFF means inline.

pub fn sentinel_inline(_peer: &mut Peer) -> TestResult {
    if INLINE_PAYLOAD_SLOT != 0xFFFFFFFF {
        return TestResult::fail(format!(
            "[verify frame.sentinel.values]: INLINE_PAYLOAD_SLOT is {:#X}, expected 0xFFFFFFFF",
            INLINE_PAYLOAD_SLOT
        ));
    }
    TestResult::pass()
}

// =============================================================================
// frame.sentinel_no_deadline
// =============================================================================
// Rules: [verify frame.sentinel.values]
//
// deadline_ns = 0xFFFFFFFFFFFFFFFF means no deadline.

pub fn sentinel_no_deadline(_peer: &mut Peer) -> TestResult {
    if NO_DEADLINE != 0xFFFFFFFFFFFFFFFF {
        return TestResult::fail(format!(
            "[verify frame.sentinel.values]: NO_DEADLINE is {:#X}, expected 0xFFFFFFFFFFFFFFFF",
            NO_DEADLINE
        ));
    }
    TestResult::pass()
}

// =============================================================================
// frame.encoding_little_endian
// =============================================================================
// Rules: [verify frame.desc.encoding]
//
// Descriptor fields must be little-endian.

pub fn encoding_little_endian(_peer: &mut Peer) -> TestResult {
    let mut desc = MsgDescHot::new();
    desc.msg_id = 0x0102030405060708;
    desc.channel_id = 0x11121314;
    desc.method_id = 0x21222324;

    let bytes = desc.to_bytes();

    // Check msg_id (bytes 0-7, little-endian)
    if bytes[0..8] != [0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01] {
        return TestResult::fail(
            "[verify frame.desc.encoding]: msg_id not little-endian".to_string(),
        );
    }

    // Check channel_id (bytes 8-11)
    if bytes[8..12] != [0x14, 0x13, 0x12, 0x11] {
        return TestResult::fail(
            "[verify frame.desc.encoding]: channel_id not little-endian".to_string(),
        );
    }

    // Check method_id (bytes 12-15)
    if bytes[12..16] != [0x24, 0x23, 0x22, 0x21] {
        return TestResult::fail(
            "[verify frame.desc.encoding]: method_id not little-endian".to_string(),
        );
    }

    TestResult::pass()
}

/// Run a frame test case by name.
pub fn run(name: &str) -> TestResult {
    let mut peer = Peer::new();

    match name {
        "descriptor_size" => descriptor_size(&mut peer),
        "inline_payload_max" => inline_payload_max(&mut peer),
        "sentinel_inline" => sentinel_inline(&mut peer),
        "sentinel_no_deadline" => sentinel_no_deadline(&mut peer),
        "encoding_little_endian" => encoding_little_endian(&mut peer),
        _ => TestResult::fail(format!("unknown frame test: {}", name)),
    }
}

/// List all frame test cases.
pub fn list() -> Vec<(&'static str, &'static [&'static str])> {
    vec![
        (
            "descriptor_size",
            &["frame.desc.size", "frame.desc.sizeof"][..],
        ),
        ("inline_payload_max", &["frame.payload.inline"][..]),
        ("sentinel_inline", &["frame.sentinel.values"][..]),
        ("sentinel_no_deadline", &["frame.sentinel.values"][..]),
        ("encoding_little_endian", &["frame.desc.encoding"][..]),
    ]
}
