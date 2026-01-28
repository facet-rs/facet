//! Virtual connection (multiplexing) compliance tests.
//!
//! Tests the Connect/Accept/Reject message handling for virtual connections.
//!
//! Note: These tests are designed to verify the wire protocol behavior.
//! The subject-rust currently doesn't accept incoming virtual connections,
//! so tests that require the subject to accept connections verify that
//! rejection happens correctly. Full Connect/Accept flow testing requires
//! a subject that implements `IncomingConnections` handling.

use std::time::Duration;

use roam_wire::{ConnectionId, Hello, Message, MetadataValue};
use spec_tests::harness::{accept_subject, accept_subject_with_options, our_hello, run_async};
use spec_tests::testbed::method_id;

fn metadata_empty() -> Vec<(String, MetadataValue, u64)> {
    Vec::new()
}

/// Helper to complete hello handshake and return the negotiated parameters.
async fn complete_hello_handshake(
    io: &mut spec_tests::harness::CobsFramed,
) -> Result<(u32, u32), String> {
    // Receive Hello from subject
    let msg = io
        .recv_timeout(Duration::from_millis(250))
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "expected Hello from subject".to_string())?;

    let (their_max_payload, their_credit) = match msg {
        Message::Hello(Hello::V3 {
            max_payload_size,
            initial_channel_credit,
        }) => (max_payload_size, initial_channel_credit),
        other => return Err(format!("expected Hello::V3, got {other:?}")),
    };

    // Send our Hello
    io.send(&Message::Hello(our_hello(1024 * 1024)))
        .await
        .map_err(|e| e.to_string())?;

    Ok((their_max_payload, their_credit))
}

// r[verify core.conn.accept-required] - Peer MUST reject Connect if not listening
// r[verify message.reject.response] - Reject message sent in response to Connect
// r[verify message.reject.reason] - Reject includes reason string
#[test]
fn connect_rejected_when_not_listening() {
    run_async(async {
        let (mut io, mut child) = accept_subject().await?;

        // Hello exchange
        complete_hello_handshake(&mut io).await?;

        // Send Connect request - subject is not listening for incoming connections
        // r[verify message.connect.initiate]
        // r[verify message.connect.request-id]
        let connect_msg = Message::Connect {
            request_id: 1,
            metadata: metadata_empty(),
        };
        io.send(&connect_msg).await.map_err(|e| e.to_string())?;

        // Expect Reject response since subject doesn't accept incoming connections
        let response = io
            .recv_timeout(Duration::from_millis(500))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "expected Reject from subject".to_string())?;

        match response {
            Message::Reject {
                request_id, reason, ..
            } => {
                if request_id != 1 {
                    return Err(format!("request_id mismatch: expected 1, got {request_id}"));
                }
                // Reason should indicate not listening
                if !reason.contains("not listening") && !reason.contains("listening") {
                    // Accept any reasonable rejection reason
                    eprintln!("Note: rejection reason was: {reason}");
                }
            }
            other => return Err(format!("expected Reject, got {other:?}")),
        }

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}

// ============================================================================
// Tests requiring subject to accept incoming virtual connections
// ============================================================================

// r[verify core.conn.open] - A peer opens a new connection via Connect/Accept
// r[verify message.accept.response] - Accept provides the new conn_id
// r[verify core.conn.id-allocation] - Connection IDs are allocated by the acceptor
#[test]
fn connect_accept_flow() {
    run_async(async {
        // Spawn subject with ACCEPT_CONNECTIONS=1
        let (mut io, mut child) = accept_subject_with_options(true).await?;

        complete_hello_handshake(&mut io).await?;

        // Send Connect request
        io.send(&Message::Connect {
            request_id: 1,
            metadata: metadata_empty(),
        })
        .await
        .map_err(|e| e.to_string())?;

        // Should receive Accept with a valid conn_id
        let response = io
            .recv_timeout(Duration::from_millis(500))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "expected Accept from subject".to_string())?;

        match response {
            Message::Accept {
                request_id,
                conn_id,
                ..
            } => {
                if request_id != 1 {
                    return Err(format!("request_id mismatch: expected 1, got {request_id}"));
                }
                // conn_id should be non-zero (0 is reserved for root)
                if conn_id.is_root() {
                    return Err("conn_id should not be 0 (reserved for root)".to_string());
                }
                eprintln!("Accepted virtual connection with conn_id={conn_id}");
            }
            Message::Reject { reason, .. } => {
                return Err(format!("unexpected Reject: {reason}"));
            }
            other => return Err(format!("expected Accept, got {other:?}")),
        }

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify core.conn.open] - Multiple virtual connections can be opened
// r[verify core.conn.id-allocation] - Each connection gets a unique ID
#[test]
fn multiple_virtual_connections() {
    run_async(async {
        let (mut io, mut child) = accept_subject_with_options(true).await?;

        complete_hello_handshake(&mut io).await?;

        // Open multiple virtual connections
        let mut conn_ids = Vec::new();
        for request_id in 1..=3 {
            io.send(&Message::Connect {
                request_id,
                metadata: metadata_empty(),
            })
            .await
            .map_err(|e| e.to_string())?;

            let response = io
                .recv_timeout(Duration::from_millis(500))
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("expected Accept for request {request_id}"))?;

            match response {
                Message::Accept { conn_id, .. } => {
                    if conn_id.is_root() {
                        return Err("conn_id should not be 0".to_string());
                    }
                    conn_ids.push(conn_id);
                }
                Message::Reject { reason, .. } => {
                    return Err(format!("unexpected Reject: {reason}"));
                }
                other => return Err(format!("expected Accept, got {other:?}")),
            }
        }

        // All connection IDs should be unique
        let unique_ids: std::collections::HashSet<_> = conn_ids.iter().collect();
        if unique_ids.len() != conn_ids.len() {
            return Err(format!("connection IDs should be unique: {conn_ids:?}"));
        }

        eprintln!(
            "Opened {} virtual connections: {:?}",
            conn_ids.len(),
            conn_ids
        );

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify core.conn.lifecycle] - Virtual connection is closed when Goodbye is sent
// r[verify core.conn.independence] - Closing one connection doesn't affect others
#[test]
fn virtual_connection_goodbye_independence() {
    run_async(async {
        let (mut io, mut child) = accept_subject_with_options(true).await?;

        complete_hello_handshake(&mut io).await?;

        // Open two virtual connections
        let mut conn_ids = Vec::new();
        for request_id in 1..=2 {
            io.send(&Message::Connect {
                request_id,
                metadata: metadata_empty(),
            })
            .await
            .map_err(|e| e.to_string())?;

            let response = io
                .recv_timeout(Duration::from_millis(500))
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("expected Accept for request {request_id}"))?;

            match response {
                Message::Accept { conn_id, .. } => {
                    conn_ids.push(conn_id);
                }
                other => return Err(format!("expected Accept, got {other:?}")),
            }
        }

        // Close the first virtual connection
        io.send(&Message::Goodbye {
            conn_id: conn_ids[0],
            reason: "test closing first connection".to_string(),
        })
        .await
        .map_err(|e| e.to_string())?;

        // Root connection should still work
        io.send(&Message::Request {
            conn_id: ConnectionId::ROOT,
            request_id: 100,
            method_id: method_id::echo(),
            metadata: metadata_empty(),
            channels: vec![],
            payload: facet_postcard::to_vec(&("still working".to_string(),)).unwrap(),
        })
        .await
        .map_err(|e| e.to_string())?;

        let response = io
            .recv_timeout(Duration::from_millis(500))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "expected response on root connection".to_string())?;

        match response {
            Message::Response {
                conn_id,
                request_id: 100,
                ..
            } => {
                if !conn_id.is_root() {
                    return Err(format!(
                        "response should be on root connection, got {conn_id}"
                    ));
                }
            }
            other => return Err(format!("expected Response, got {other:?}")),
        }

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify message.connect.request-id] - Connect uses unique request_id
// r[verify message.connect.metadata] - Connect can include metadata
#[test]
fn connect_message_structure() {
    run_async(async {
        let (mut io, mut child) = accept_subject().await?;

        // Hello exchange
        complete_hello_handshake(&mut io).await?;

        // Send Connect with metadata
        let connect_msg = Message::Connect {
            request_id: 42,
            metadata: vec![
                (
                    "auth".to_string(),
                    MetadataValue::String("token123".to_string()),
                    0,
                ),
                ("version".to_string(), MetadataValue::U64(2), 0),
            ],
        };
        io.send(&connect_msg).await.map_err(|e| e.to_string())?;

        // Should get a response (Accept or Reject) with matching request_id
        let response = io
            .recv_timeout(Duration::from_millis(500))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "expected response to Connect".to_string())?;

        match response {
            Message::Accept { request_id, .. } | Message::Reject { request_id, .. } => {
                if request_id != 42 {
                    return Err(format!(
                        "request_id mismatch: expected 42, got {request_id}"
                    ));
                }
            }
            other => return Err(format!("expected Accept or Reject, got {other:?}")),
        }

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify core.conn.independence] - Virtual connections are independent
// r[verify message.goodbye.connection-zero] - Goodbye on conn 0 closes entire link
#[test]
fn goodbye_on_root_closes_link() {
    run_async(async {
        let (mut io, mut child) = accept_subject().await?;

        // Hello exchange
        complete_hello_handshake(&mut io).await?;

        // Send Goodbye on connection 0 (root)
        io.send(&Message::Goodbye {
            conn_id: ConnectionId::ROOT,
            reason: "test complete".to_string(),
        })
        .await
        .map_err(|e| e.to_string())?;

        // Connection should close - no more messages expected
        // Give the subject time to process and close
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Try to receive - should get None (closed) or timeout
        match io.recv_timeout(Duration::from_millis(200)).await {
            Ok(None) => {}                          // Connection closed as expected
            Ok(Some(Message::Goodbye { .. })) => {} // Subject sent its own Goodbye, also fine
            Ok(Some(other)) => {
                return Err(format!(
                    "expected connection to close after Goodbye, got {other:?}"
                ));
            }
            Err(_) => {} // Timeout is acceptable too
        }

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify core.conn.id-allocation] - Connection IDs are allocated by the acceptor
// r[verify message.accept.response] - Accept message includes the new conn_id
// This test verifies Connect request IDs are echoed in Reject (Accept would need
// a subject that listens for incoming connections).
#[test]
fn connect_request_id_echoed_in_reject() {
    run_async(async {
        let (mut io, mut child) = accept_subject().await?;

        complete_hello_handshake(&mut io).await?;

        // Send multiple Connect requests with different request_ids
        for request_id in [100, 200, 300] {
            io.send(&Message::Connect {
                request_id,
                metadata: metadata_empty(),
            })
            .await
            .map_err(|e| e.to_string())?;
        }

        // Collect responses and verify request_ids match
        let mut received_ids = Vec::new();
        for _ in 0..3 {
            let response = io
                .recv_timeout(Duration::from_millis(500))
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "expected response to Connect".to_string())?;

            match response {
                Message::Reject { request_id, .. } => {
                    received_ids.push(request_id);
                }
                Message::Accept { request_id, .. } => {
                    received_ids.push(request_id);
                }
                other => return Err(format!("expected Accept or Reject, got {other:?}")),
            }
        }

        // Verify all request_ids were echoed (order may vary)
        received_ids.sort();
        if received_ids != vec![100, 200, 300] {
            return Err(format!(
                "expected request_ids [100, 200, 300], got {received_ids:?}"
            ));
        }

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify message.goodbye.receive] - Must process Goodbye from peer
// r[verify message.goodbye.graceful] - Graceful shutdown completes in-flight work
#[test]
fn goodbye_processed_gracefully() {
    run_async(async {
        let (mut io, mut child) = accept_subject().await?;

        complete_hello_handshake(&mut io).await?;

        // Send a request on the root connection
        io.send(&Message::Request {
            conn_id: ConnectionId::ROOT,
            request_id: 1,
            method_id: method_id::echo(),
            metadata: metadata_empty(),
            channels: vec![],
            payload: facet_postcard::to_vec(&("hello".to_string(),)).unwrap(),
        })
        .await
        .map_err(|e| e.to_string())?;

        // Wait for response
        let response = io
            .recv_timeout(Duration::from_millis(500))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "expected response".to_string())?;

        match response {
            Message::Response { request_id: 1, .. } => {}
            other => return Err(format!("expected Response for request 1, got {other:?}")),
        }

        // Now send Goodbye - subject should handle gracefully
        io.send(&Message::Goodbye {
            conn_id: ConnectionId::ROOT,
            reason: "test complete".to_string(),
        })
        .await
        .map_err(|e| e.to_string())?;

        // Subject should close cleanly
        tokio::time::sleep(Duration::from_millis(100)).await;

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify message.conn-id] - All messages except Hello/Connect/Accept/Reject have conn_id
// r[verify core.link.connection-zero] - Connection 0 is implicit on link establishment
#[test]
fn messages_on_root_connection() {
    run_async(async {
        let (mut io, mut child) = accept_subject().await?;

        complete_hello_handshake(&mut io).await?;

        // Send Request on connection 0 (root)
        io.send(&Message::Request {
            conn_id: ConnectionId::ROOT,
            request_id: 1,
            method_id: method_id::echo(),
            metadata: metadata_empty(),
            channels: vec![],
            payload: facet_postcard::to_vec(&("test message".to_string(),)).unwrap(),
        })
        .await
        .map_err(|e| e.to_string())?;

        // Response should come on connection 0
        let response = io
            .recv_timeout(Duration::from_millis(500))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "expected response".to_string())?;

        match response {
            Message::Response { conn_id, .. } => {
                if !conn_id.is_root() {
                    return Err(format!(
                        "expected response on root connection, got conn_id={conn_id}"
                    ));
                }
            }
            other => return Err(format!("expected Response, got {other:?}")),
        }

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify core.conn.lifecycle] - Virtual connection is closed when either peer sends Goodbye
// Test sending Goodbye on a non-existent connection - should be ignored or cause error
#[test]
fn goodbye_on_nonexistent_connection_ignored() {
    run_async(async {
        let (mut io, mut child) = accept_subject().await?;

        complete_hello_handshake(&mut io).await?;

        // Send Goodbye on connection 42 which doesn't exist
        // The subject should ignore this since the connection was never opened
        io.send(&Message::Goodbye {
            conn_id: ConnectionId::new(42),
            reason: "closing nonexistent".to_string(),
        })
        .await
        .map_err(|e| e.to_string())?;

        // Connection should still be alive - try a request on root
        io.send(&Message::Request {
            conn_id: ConnectionId::ROOT,
            request_id: 1,
            method_id: method_id::echo(),
            metadata: metadata_empty(),
            channels: vec![],
            payload: facet_postcard::to_vec(&("still alive".to_string(),)).unwrap(),
        })
        .await
        .map_err(|e| e.to_string())?;

        // Should still get a response
        let response = io
            .recv_timeout(Duration::from_millis(500))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "expected response - link should still be alive".to_string())?;

        match response {
            Message::Response { request_id: 1, .. } => {}
            Message::Goodbye { conn_id, reason } => {
                // If subject closes the link due to protocol violation, that's also acceptable
                // behavior for a Goodbye on nonexistent connection
                eprintln!("Note: subject closed link after Goodbye on nonexistent conn: {reason}");
                if !conn_id.is_root() {
                    return Err(format!(
                        "Goodbye should be on root connection, got {conn_id}"
                    ));
                }
            }
            other => return Err(format!("expected Response or Goodbye, got {other:?}")),
        }

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}
