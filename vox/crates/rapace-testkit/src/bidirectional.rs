//! Bidirectional RPC test harness.
//!
//! This module provides a shared test harness for bidirectional RPC patterns
//! where both peers can call each other (like the template engine example with
//! host callbacks).
//!
//! # Usage
//!
//! ```ignore
//! use rapace_testkit::bidirectional::{run_bidirectional_scenario, BidirectionalScenario};
//!
//! struct MyFactory;
//! impl TransportFactory for MyFactory { ... }
//!
//! #[tokio::test]
//! async fn test_bidirectional() {
//!     run_bidirectional_scenario::<MyFactory>(BidirectionalScenario::NestedCallback).await;
//! }
//! ```

use std::sync::Arc;

use rapace_core::{ErrorCode, Frame, FrameFlags, MsgDescHot, RpcError, Transport};

use crate::RpcSession;
use crate::{TestError, TransportFactory};

/// Scenarios for bidirectional RPC testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BidirectionalScenario {
    /// Simple echo: A calls B, B echoes back.
    SimpleEcho,

    /// A calls B, B calls A during processing (nested callback).
    NestedCallback,

    /// Multiple nested calls: A calls B, B calls A multiple times.
    MultipleNestedCallbacks,
}

/// Run a bidirectional RPC scenario.
pub async fn run_bidirectional_scenario<F: TransportFactory>(scenario: BidirectionalScenario) {
    let result = match scenario {
        BidirectionalScenario::SimpleEcho => run_simple_echo::<F>().await,
        BidirectionalScenario::NestedCallback => run_nested_callback::<F>().await,
        BidirectionalScenario::MultipleNestedCallbacks => run_multiple_nested::<F>().await,
    };

    if let Err(e) = result {
        panic!("bidirectional scenario {:?} failed: {}", scenario, e);
    }
}

// ============================================================================
// Scenario: Simple Echo
// ============================================================================

async fn run_simple_echo<F: TransportFactory>() -> Result<(), TestError> {
    let (transport_a, transport_b) = F::connect_pair().await?;
    let transport_a = Arc::new(transport_a);
    let transport_b = Arc::new(transport_b);

    // Session A (uses odd channel IDs)
    let session_a = Arc::new(RpcSession::with_channel_start(transport_a.clone(), 1));

    // Session B (uses even channel IDs) - simple echo dispatcher
    let session_b = Arc::new(RpcSession::with_channel_start(transport_b.clone(), 2));
    session_b.set_dispatcher(|_channel_id, _method_id, payload| async move {
        // Echo: respond with the same payload
        let mut desc = MsgDescHot::new();
        desc.flags = FrameFlags::DATA | FrameFlags::EOS;
        Ok(Frame::with_payload(desc, payload))
    });

    // Spawn demux loops
    let session_a_clone = session_a.clone();
    let handle_a = tokio::spawn(async move { session_a_clone.run().await });

    let session_b_clone = session_b.clone();
    let handle_b = tokio::spawn(async move { session_b_clone.run().await });

    // A calls B
    let channel_id = session_a.next_channel_id();
    let response = session_a
        .call(channel_id, 1, b"hello".to_vec())
        .await
        .map_err(TestError::Rpc)?;

    if response.payload != b"hello" {
        return Err(TestError::Assertion(format!(
            "expected echo 'hello', got {:?}",
            response.payload
        )));
    }

    // Cleanup
    let _ = transport_a.close().await;
    let _ = transport_b.close().await;
    handle_a.abort();
    handle_b.abort();

    Ok(())
}

// ============================================================================
// Scenario: Nested Callback
// ============================================================================

async fn run_nested_callback<F: TransportFactory>() -> Result<(), TestError> {
    let (transport_a, transport_b) = F::connect_pair().await?;
    let transport_a = Arc::new(transport_a);
    let transport_b = Arc::new(transport_b);

    // Session A (uses odd channel IDs)
    // A provides a "get_prefix" service: returns "PREFIX:"
    let session_a = Arc::new(RpcSession::with_channel_start(transport_a.clone(), 1));
    session_a.set_dispatcher(|_channel_id, method_id, _payload| async move {
        // method 1 = get_prefix
        if method_id == 1 {
            let prefix = b"PREFIX:";
            let mut desc = MsgDescHot::new();
            desc.flags = FrameFlags::DATA | FrameFlags::EOS;
            Ok(Frame::with_payload(desc, prefix.to_vec()))
        } else {
            Err(RpcError::Status {
                code: ErrorCode::Unimplemented,
                message: "unknown method".into(),
            })
        }
    });

    // Session B (uses even channel IDs)
    // B provides a "format" service: calls A's get_prefix, then appends the input
    let session_b = Arc::new(RpcSession::with_channel_start(transport_b.clone(), 2));
    let session_b_for_dispatcher = session_b.clone();
    session_b.set_dispatcher(move |_channel_id, method_id, payload| {
        let session = session_b_for_dispatcher.clone();
        async move {
            // method 1 = format
            if method_id == 1 {
                // Call A's get_prefix
                let cb_channel = session.next_channel_id();
                let cb_response =
                    session
                        .call(cb_channel, 1, vec![])
                        .await
                        .map_err(|e| RpcError::Status {
                            code: ErrorCode::Internal,
                            message: format!("callback failed: {:?}", e),
                        })?;

                // Combine prefix + input
                let mut result = cb_response.payload;
                result.extend(payload);

                let mut desc = MsgDescHot::new();
                desc.flags = FrameFlags::DATA | FrameFlags::EOS;
                Ok(Frame::with_payload(desc, result))
            } else {
                Err(RpcError::Status {
                    code: ErrorCode::Unimplemented,
                    message: "unknown method".into(),
                })
            }
        }
    });

    // Spawn demux loops
    let session_a_clone = session_a.clone();
    let handle_a = tokio::spawn(async move { session_a_clone.run().await });

    let session_b_clone = session_b.clone();
    let handle_b = tokio::spawn(async move { session_b_clone.run().await });

    // A calls B's format service
    let channel_id = session_a.next_channel_id();
    let response = session_a
        .call(channel_id, 1, b"test".to_vec())
        .await
        .map_err(TestError::Rpc)?;

    if response.payload != b"PREFIX:test" {
        return Err(TestError::Assertion(format!(
            "expected 'PREFIX:test', got {:?}",
            String::from_utf8_lossy(&response.payload)
        )));
    }

    // Cleanup
    let _ = transport_a.close().await;
    let _ = transport_b.close().await;
    handle_a.abort();
    handle_b.abort();

    Ok(())
}

// ============================================================================
// Scenario: Multiple Nested Callbacks
// ============================================================================

async fn run_multiple_nested<F: TransportFactory>() -> Result<(), TestError> {
    let (transport_a, transport_b) = F::connect_pair().await?;
    let transport_a = Arc::new(transport_a);
    let transport_b = Arc::new(transport_b);

    // Session A (uses odd channel IDs)
    // A provides a "get_value" service: returns "value_N" where N is from the request
    let session_a = Arc::new(RpcSession::with_channel_start(transport_a.clone(), 1));
    session_a.set_dispatcher(|_channel_id, method_id, payload| async move {
        // method 1 = get_value
        if method_id == 1 {
            // payload is the key, return "value_" + key
            let mut result = b"value_".to_vec();
            result.extend(payload);
            let mut desc = MsgDescHot::new();
            desc.flags = FrameFlags::DATA | FrameFlags::EOS;
            Ok(Frame::with_payload(desc, result))
        } else {
            Err(RpcError::Status {
                code: ErrorCode::Unimplemented,
                message: "unknown method".into(),
            })
        }
    });

    // Session B (uses even channel IDs)
    // B provides a "combine" service: calls A's get_value 3 times and combines results
    let session_b = Arc::new(RpcSession::with_channel_start(transport_b.clone(), 2));
    let session_b_for_dispatcher = session_b.clone();
    session_b.set_dispatcher(move |_channel_id, method_id, _payload| {
        let session = session_b_for_dispatcher.clone();
        async move {
            // method 1 = combine
            if method_id == 1 {
                let mut result = Vec::new();

                // Call A three times
                for key in [b"a".as_slice(), b"b", b"c"] {
                    let cb_channel = session.next_channel_id();
                    let cb_response =
                        session
                            .call(cb_channel, 1, key.to_vec())
                            .await
                            .map_err(|e| RpcError::Status {
                                code: ErrorCode::Internal,
                                message: format!("callback failed: {:?}", e),
                            })?;
                    result.extend(&cb_response.payload);
                    result.push(b',');
                }

                // Remove trailing comma
                if !result.is_empty() {
                    result.pop();
                }

                let mut desc = MsgDescHot::new();
                desc.flags = FrameFlags::DATA | FrameFlags::EOS;
                Ok(Frame::with_payload(desc, result))
            } else {
                Err(RpcError::Status {
                    code: ErrorCode::Unimplemented,
                    message: "unknown method".into(),
                })
            }
        }
    });

    // Spawn demux loops
    let session_a_clone = session_a.clone();
    let handle_a = tokio::spawn(async move { session_a_clone.run().await });

    let session_b_clone = session_b.clone();
    let handle_b = tokio::spawn(async move { session_b_clone.run().await });

    // A calls B's combine service
    let channel_id = session_a.next_channel_id();
    let response = session_a
        .call(channel_id, 1, vec![])
        .await
        .map_err(TestError::Rpc)?;

    let expected = b"value_a,value_b,value_c";
    if response.payload != expected {
        return Err(TestError::Assertion(format!(
            "expected '{}', got '{}'",
            String::from_utf8_lossy(expected),
            String::from_utf8_lossy(&response.payload)
        )));
    }

    // Cleanup
    let _ = transport_a.close().await;
    let _ = transport_b.close().await;
    handle_a.abort();
    handle_b.abort();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rapace_transport_mem::InProcTransport;

    struct InProcFactory;

    impl TransportFactory for InProcFactory {
        type Transport = InProcTransport;

        async fn connect_pair() -> Result<(Self::Transport, Self::Transport), TestError> {
            Ok(InProcTransport::pair())
        }
    }

    #[tokio::test]
    async fn test_simple_echo_inproc() {
        run_bidirectional_scenario::<InProcFactory>(BidirectionalScenario::SimpleEcho).await;
    }

    #[tokio::test]
    async fn test_nested_callback_inproc() {
        run_bidirectional_scenario::<InProcFactory>(BidirectionalScenario::NestedCallback).await;
    }

    #[tokio::test]
    async fn test_multiple_nested_inproc() {
        run_bidirectional_scenario::<InProcFactory>(BidirectionalScenario::MultipleNestedCallbacks)
            .await;
    }
}
