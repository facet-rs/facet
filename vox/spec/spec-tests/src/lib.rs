//! Compliance tests live in `tests/`.

pub mod harness;

// Re-export types from spec-proto for use in generated code
pub use spec_proto::{Canvas, Color, Message, Person, Point, Rectangle, Shape};

// Include generated dispatchers and handlers for spec-proto services
include!(concat!(env!("OUT_DIR"), "/generated.rs"));
