//! Rapace conformance test suite.
//!
//! This crate provides a reference peer that implementations can use to
//! validate their conformance to the Rapace specification.
//!
//! # Usage
//!
//! Implementations spawn the reference peer for each test case:
//!
//! ```bash
//! rapace-conformance --case handshake.valid_hello
//! ```
//!
//! The peer communicates via stdin/stdout using raw Rapace frames:
//! - Length-prefixed: 4 bytes (little-endian u32) + frame data
//! - Frame data: 64-byte MsgDescHot + payload (if not inline)
//!
//! The peer exits with:
//! - 0: Test passed (peer saw correct behavior)
//! - 1: Test failed (protocol violation detected)
//! - 2: Internal error (bug in the peer itself)

pub mod harness;
pub mod protocol;
pub mod testcase;
pub mod tests;

use std::future::Future;
use std::pin::Pin;

use harness::Peer;
pub use harness::RawFrame;
use testcase::TestResult;

/// The async test function type.
pub type AsyncTestFn = fn(&mut Peer) -> Pin<Box<dyn Future<Output = TestResult> + Send + '_>>;

/// A registered conformance test.
///
/// Tests are registered using the `#[conformance(rules = "...")]` attribute macro.
pub struct ConformanceTest {
    /// The test function name.
    pub name: &'static str,
    /// The spec rules this test covers.
    pub rules: &'static [&'static str],
    /// The async test function itself.
    pub func: AsyncTestFn,
}

inventory::collect!(ConformanceTest);
