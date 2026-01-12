//! Compliance tests live in `tests/`.

pub mod harness;

/// Re-export method IDs from spec-proto for the Testbed service.
pub mod testbed {
    pub use spec_proto::testbed_method_id as method_id;
}
