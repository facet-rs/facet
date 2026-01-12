//! Channeling (streaming) RPC compliance tests.
//!
//! Tests the channeling methods from the `Testbed` service:
//! - `sum(numbers: Rx<i32>) -> i64` - client-to-server channel
//! - `generate(count: u32, output: Tx<i32>)` - server-to-client channel
//! - `transform(input: Rx<String>, output: Tx<String>)` - bidirectional channels

use std::time::Duration;

use facet::Facet;
use roam_wire::{Hello, Message, MetadataValue};
use spec_tests::harness::{accept_subject, our_hello, run_async};
use spec_tests::testbed::method_id;

// TODO: Remove this shim once facet implements `Facet` for `core::convert::Infallible`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
struct Never;

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
enum RoamError<E> {
    User(E) = 0,
    UnknownMethod = 1,
    InvalidPayload = 2,
    Cancelled = 3,
}

fn metadata_empty() -> Vec<(String, MetadataValue)> {
    Vec::new()
}

/// Helper to do hello exchange.
async fn hello_exchange(io: &mut spec_tests::harness::CobsFramed) -> Result<(), String> {
    // Subject sends Hello first.
    let msg = io
        .recv_timeout(Duration::from_millis(250))
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "expected Hello from subject".to_string())?;
    if !matches!(msg, Message::Hello(Hello::V1 { .. })) {
        return Err(format!("first message must be Hello, got {msg:?}"));
    }

    io.send(&Message::Hello(our_hello(1024 * 1024)))
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

// r[verify channeling.type] - Client pushes data, server aggregates
// r[verify channeling.data] - Data messages carry stream payloads
// r[verify channeling.close] - Close terminates stream gracefully
// r[verify channeling.id.parity] - Client uses odd stream IDs (initiator)
#[test]
fn streaming_sum_client_to_server() {
    run_async(async {
        let (mut io, mut child) = accept_subject().await?;
        hello_exchange(&mut io).await?;

        // Get the method ID for `sum(numbers: Rx<i32>) -> i64`
        let method_id = method_id::sum();

        // Allocate stream ID (odd = initiator)
        let channel_id: u64 = 1;

        // Send Request with stream ID as the payload
        // Payload: tuple of (channel_id: u64)
        let req_payload =
            facet_postcard::to_vec(&(channel_id,)).map_err(|e| format!("postcard args: {e}"))?;
        let req = Message::Request {
            request_id: 1,
            method_id,
            metadata: metadata_empty(),
            payload: req_payload,
        };
        io.send(&req).await.map_err(|e| e.to_string())?;

        // Send Data messages with numbers
        for n in [1i32, 2, 3, 4, 5] {
            let data_payload =
                facet_postcard::to_vec(&n).map_err(|e| format!("postcard data: {e}"))?;
            io.send(&Message::Data {
                channel_id,
                payload: data_payload,
            })
            .await
            .map_err(|e| e.to_string())?;
        }

        // Send Close to end the stream
        io.send(&Message::Close { channel_id })
            .await
            .map_err(|e| e.to_string())?;

        // Wait for Response
        let resp = io
            .recv_timeout(Duration::from_millis(500))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "expected Response from subject".to_string())?;

        let payload = match resp {
            Message::Response {
                request_id,
                payload,
                ..
            } => {
                if request_id != 1 {
                    return Err(format!("response request_id mismatch: {request_id}"));
                }
                payload
            }
            Message::Goodbye { reason } => return Err(format!("unexpected Goodbye: {reason}")),
            other => return Err(format!("expected Response, got {other:?}")),
        };

        let decoded: Result<i64, RoamError<Never>> =
            facet_postcard::from_slice(&payload).map_err(|e| format!("postcard resp: {e}"))?;

        match decoded {
            Ok(sum) => {
                if sum != 15 {
                    // 1 + 2 + 3 + 4 + 5 = 15
                    return Err(format!("expected sum 15, got {sum}"));
                }
            }
            Err(e) => return Err(format!("expected Ok response, got Err({e:?})")),
        }

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify channeling.type] - Server pushes data to client
// r[verify channeling.data] - Data messages carry stream payloads
// r[verify channeling.close] - Close terminates stream gracefully
#[test]
fn streaming_generate_server_to_client() {
    run_async(async {
        let (mut io, mut child) = accept_subject().await?;
        hello_exchange(&mut io).await?;

        // Get the method ID for `generate(count: u32, output: Tx<i32>)`
        let method_id = method_id::generate();

        // Allocate stream ID (odd = initiator)
        let channel_id: u64 = 1;
        let count: u32 = 5;

        // Send Request with (count, channel_id)
        let req_payload = facet_postcard::to_vec(&(count, channel_id))
            .map_err(|e| format!("postcard args: {e}"))?;
        let req = Message::Request {
            request_id: 1,
            method_id,
            metadata: metadata_empty(),
            payload: req_payload,
        };
        io.send(&req).await.map_err(|e| e.to_string())?;

        // Collect Data messages from server
        // Protocol requires: Data messages BEFORE Response (for server-to-client streaming)
        let mut received: Vec<i32> = Vec::new();
        let mut got_close = false;
        let mut got_response = false;

        // Keep receiving until we have both Close and Response
        while !got_close || !got_response {
            let msg = io
                .recv_timeout(Duration::from_millis(500))
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!(
                    "connection closed unexpectedly (got_close={got_close}, got_response={got_response}, received={received:?})"
                ))?;

            match msg {
                Message::Data { channel_id: sid, payload } => {
                    if sid != channel_id {
                        return Err(format!("unexpected channel_id {sid}, expected {channel_id}"));
                    }
                    // Data must arrive BEFORE Response
                    if got_response {
                        return Err("received Data after Response - protocol violation".to_string());
                    }
                    let n: i32 = facet_postcard::from_slice(&payload)
                        .map_err(|e| format!("postcard data: {e}"))?;
                    received.push(n);
                }
                Message::Close { channel_id: sid } => {
                    if sid != channel_id {
                        return Err(format!("close channel_id mismatch: {sid}"));
                    }
                    // Close must arrive BEFORE Response
                    if got_response {
                        return Err("received Close after Response - protocol violation".to_string());
                    }
                    got_close = true;
                }
                Message::Response { request_id, .. } => {
                    if request_id != 1 {
                        return Err(format!("response request_id mismatch: {request_id}"));
                    }
                    // Response must come AFTER all Data and Close
                    if !got_close {
                        return Err(format!("received Response before Close - protocol violation (received so far: {received:?})"));
                    }
                    got_response = true;
                }
                Message::Goodbye { reason } => {
                    return Err(format!("unexpected Goodbye: {reason}"));
                }
                other => {
                    return Err(format!("unexpected message: {other:?}"));
                }
            }
        }

        // Verify received numbers
        let expected: Vec<i32> = (0..count as i32).collect();
        if received != expected {
            return Err(format!("expected {expected:?}, got {received:?}"));
        }

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify channeling.type] - Both sides can push data
// r[verify channeling.lifecycle.immediate-data] - Input/output streams are independent
#[test]
fn streaming_transform_bidirectional() {
    run_async(async {
        let (mut io, mut child) = accept_subject().await?;
        hello_exchange(&mut io).await?;

        // Get the method ID for `transform(input: Rx<String>, output: Tx<String>)`
        let method_id = method_id::transform();

        // Allocate stream IDs (odd = initiator)
        let input_channel_id: u64 = 1;
        let output_channel_id: u64 = 3;

        // Send Request with (input_channel_id, output_channel_id)
        let req_payload = facet_postcard::to_vec(&(input_channel_id, output_channel_id))
            .map_err(|e| format!("postcard args: {e}"))?;
        let req = Message::Request {
            request_id: 1,
            method_id,
            metadata: metadata_empty(),
            payload: req_payload,
        };
        io.send(&req).await.map_err(|e| e.to_string())?;

        // Send some strings and collect echoes
        let messages = ["hello", "world", "test"];
        let mut received: Vec<String> = Vec::new();

        for msg in &messages {
            // Send input
            let data_payload = facet_postcard::to_vec(&msg.to_string())
                .map_err(|e| format!("postcard data: {e}"))?;
            io.send(&Message::Data {
                channel_id: input_channel_id,
                payload: data_payload,
            })
            .await
            .map_err(|e| e.to_string())?;

            // Receive echo on output stream
            let resp_msg = io
                .recv_timeout(Duration::from_millis(500))
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "expected Data from subject".to_string())?;

            match resp_msg {
                Message::Data {
                    channel_id,
                    payload,
                } => {
                    if channel_id != output_channel_id {
                        return Err(format!(
                            "unexpected channel_id {channel_id}, expected {output_channel_id}"
                        ));
                    }
                    let s: String = facet_postcard::from_slice(&payload)
                        .map_err(|e| format!("postcard data: {e}"))?;
                    received.push(s);
                }
                other => return Err(format!("expected Data, got {other:?}")),
            }
        }

        // Close input stream
        io.send(&Message::Close {
            channel_id: input_channel_id,
        })
        .await
        .map_err(|e| e.to_string())?;

        // Expect Close on output stream and Response (order may vary)
        let mut got_close = false;
        let mut got_response = false;

        while !got_close || !got_response {
            let msg = io
                .recv_timeout(Duration::from_millis(500))
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "expected Close/Response from subject".to_string())?;

            match msg {
                Message::Close { channel_id } => {
                    if channel_id != output_channel_id {
                        return Err(format!(
                            "close channel_id mismatch: {channel_id}, expected {output_channel_id}"
                        ));
                    }
                    got_close = true;
                }
                Message::Response { request_id, .. } => {
                    if request_id != 1 {
                        return Err(format!("response request_id mismatch: {request_id}"));
                    }
                    got_response = true;
                }
                other => return Err(format!("expected Close or Response, got {other:?}")),
            }
        }

        // Verify echoes
        let expected: Vec<String> = messages.iter().map(|s| s.to_string()).collect();
        if received != expected {
            return Err(format!("expected {expected:?}, got {received:?}"));
        }

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}
