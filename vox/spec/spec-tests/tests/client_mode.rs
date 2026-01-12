//! Client-mode compliance tests.
//!
//! These tests run the spec-tests harness as a wire-level server and spawn
//! the subject in client mode. This validates that the generated client code
//! works correctly against a minimal protocol implementation.
//!
//! The harness does NOT use any roam runtime types - it implements the server
//! at the wire level using only roam_wire messages.

use spec_tests::harness::{run_async, wire_server};
use spec_tests::testbed::method_id;

fn method_ids() -> wire_server::MethodIds {
    wire_server::MethodIds {
        echo: method_id::echo(),
        reverse: method_id::reverse(),
        sum: method_id::sum(),
        generate: method_id::generate(),
        transform: method_id::transform(),
    }
}

// r[verify call.initiate] - Generated client can make RPC calls
#[test]
fn client_mode_echo() {
    run_async(async { wire_server::run("echo", &method_ids()).await }).unwrap();
}

// r[verify channeling.type] - Generated client can send streaming data
// r[verify channeling.type] - Client pushes data via Rx channel
#[test]
fn client_mode_sum() {
    run_async(async { wire_server::run("sum", &method_ids()).await }).unwrap();
}

// r[verify channeling.type] - Generated client can receive streaming data
// r[verify channeling.type] - Server pushes data via Tx channel
#[test]
fn client_mode_generate() {
    run_async(async { wire_server::run("generate", &method_ids()).await }).unwrap();
}
