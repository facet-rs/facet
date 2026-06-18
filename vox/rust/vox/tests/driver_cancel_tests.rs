//! Cancellation tests.
//!
//! Wire-level RequestCancel behavior (volatile vs persist handlers) is tested
//! in vox-core/src/tests/driver_tests.rs using the connection_sender() escape
//! hatch — it requires injecting raw protocol messages that aren't reachable
//! through the public client API.
