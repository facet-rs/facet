//! Compliance tests live in `tests/`.

pub mod harness;

// Re-export types from spec-proto for use in tests
pub use spec_proto::{Canvas, Color, Message, Person, Point, Rectangle, Shape};

// Re-export method IDs for the Testbed service
pub mod testbed {
    pub use spec_proto::testbed_method_id as method_id;
}
