//! Compliance tests live in `tests/`.

pub mod harness;

// Re-export types from spec-proto for use in generated code
pub use spec_proto::{Canvas, Color, Message, Person, Point, Rectangle, Shape};

// Re-export the generated service items from spec-proto as a `testbed` module
// for backwards compatibility with existing tests
pub mod testbed {
    pub use roam::session::{Never, RoamError, Rx, Tx};
    pub use spec_proto::{
        Testbed, TestbedClient, TestbedDispatcher, testbed_method_id as method_id,
    };
}
