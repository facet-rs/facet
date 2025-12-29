//! Test for nested enum with #[repr] attribute - reproducer for issue #1430.
//!
//! This simulates the rapace ControlPayload enum which has nested struct variants
//! that may not be Tier-2 compatible initially.

#![cfg(feature = "jit")]

use facet::Facet;
use facet_postcard::from_slice;
use postcard::to_allocvec as postcard_to_vec;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[allow(clippy::enum_variant_names)]
enum CancelReason {
    NoError,
    ProtocolError,
    InternalError,
}

#[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
#[repr(u8)]
enum ControlPayload {
    CancelChannel {
        channel_id: u32,
        reason: CancelReason,
    },
    GrantCredits {
        channel_id: u32,
        bytes: u32,
    },
}

#[test]
fn test_control_payload_cancel() {
    facet_testhelpers::setup();

    let payload = ControlPayload::CancelChannel {
        channel_id: 42,
        reason: CancelReason::ProtocolError,
    };

    // Serialize with postcard
    let bytes = postcard_to_vec(&payload).expect("postcard should encode");

    // Deserialize with facet-postcard
    // This should work even if Tier-2 doesn't support this type yet
    let decoded: ControlPayload = from_slice(&bytes).expect("should deserialize");

    assert_eq!(decoded, payload);
}

#[test]
fn test_control_payload_grant() {
    facet_testhelpers::setup();

    let payload = ControlPayload::GrantCredits {
        channel_id: 100,
        bytes: 1024,
    };

    // Serialize with postcard
    let bytes = postcard_to_vec(&payload).expect("postcard should encode");

    // Deserialize with facet-postcard
    let decoded: ControlPayload = from_slice(&bytes).expect("should deserialize");

    assert_eq!(decoded, payload);
}
