use std::time::Duration;

use facet::Facet;
use roam_hash::method_id_from_detail;
use roam_schema::{ArgDetail, MethodDetail};
use roam_wire::{Hello, Message, MetadataValue};
use spec_proto::MathError;
use spec_tests::harness::{accept_subject, our_hello, run_async};
use spec_tests::testbed::method_id;

fn compute_divide_method_id() -> u64 {
    let detail = MethodDetail {
        service_name: "Testbed".into(),
        method_name: "divide".into(),
        args: vec![
            ArgDetail {
                name: "dividend".into(),
                ty: <i64 as Facet>::SHAPE,
            },
            ArgDetail {
                name: "divisor".into(),
                ty: <i64 as Facet>::SHAPE,
            },
        ],
        return_type: <Result<i64, MathError> as Facet>::SHAPE,
        doc: None,
    };
    method_id_from_detail(&detail)
}

/// Wire-level RoamError with user error type E.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
enum RoamErrorWithUser<E> {
    User(E) = 0,
    UnknownMethod = 1,
    InvalidPayload = 2,
    Cancelled = 3,
}

// TODO: Remove this shim once facet implements `Facet` for `core::convert::Infallible`
// and for the never type `!`, then use `Infallible` as the error type parameter.
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

fn compute_method_id(method_name: &str) -> u64 {
    let detail = MethodDetail {
        service_name: "Testbed".into(),
        method_name: String::from(method_name).into(),
        args: vec![ArgDetail {
            name: "message".into(),
            ty: <String as Facet>::SHAPE,
        }],
        return_type: <String as Facet>::SHAPE,
        doc: None,
    };
    method_id_from_detail(&detail)
}

fn metadata_empty() -> Vec<(String, MetadataValue)> {
    Vec::new()
}

/// Verify hardcoded IDs match computed IDs.
fn ensure_expected_ids() {
    assert_eq!(compute_method_id("echo"), method_id::echo());
    assert_eq!(compute_method_id("reverse"), method_id::reverse());
}

// r[verify call.initiate] - Call initiated by sending Request message
// r[verify call.complete] - Response has matching request_id
// r[verify call.lifecycle.single-response] - Exactly one response per request
// r[verify call.lifecycle.ordering] - Response correlated by request_id
// r[verify call.request-id.uniqueness] - Uses unique request_id (1)
// r[verify call.metadata.type] - Metadata is Vec<(String, MetadataValue)>
// r[verify call.request.payload-encoding] - Payload is POSTCARD tuple of args
// r[verify call.response.encoding] - Response is POSTCARD Result<T, RoamError<E>>
// r[verify transport.message.binary] - Binary transport (TCP stream)
#[test]
fn rpc_echo_roundtrip() {
    ensure_expected_ids();

    run_async(async {
        let (mut io, mut child) = accept_subject().await?;

        // Subject hello first.
        let msg = io
            .recv_timeout(Duration::from_millis(250))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "expected Hello from subject".to_string())?;
        match msg {
            Message::Hello(Hello::V2 { .. }) => {}
            Message::Hello(Hello::V1 { .. }) => {
                return Err("received Hello::V1, but V1 is no longer supported".to_string());
            }
            _ => return Err(format!("first message must be Hello, got {msg:?}")),
        }

        io.send(&Message::Hello(our_hello(1024 * 1024)))
            .await
            .map_err(|e| e.to_string())?;

        let req_payload = facet_postcard::to_vec(&(String::from("hello"),))
            .map_err(|e| format!("postcard args: {e}"))?;
        let req = Message::Request {
            conn_id: roam_wire::ConnectionId::ROOT,
            request_id: 1,
            method_id: method_id::echo(),
            metadata: metadata_empty(),
            channels: vec![],
            payload: req_payload,
        };
        io.send(&req).await.map_err(|e| e.to_string())?;

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
            Message::Goodbye { reason, .. } => return Err(format!("unexpected Goodbye: {reason}")),
            other => return Err(format!("expected Response, got {other:?}")),
        };

        let decoded: Result<String, RoamError<Never>> =
            facet_postcard::from_slice(&payload).map_err(|e| format!("postcard resp: {e}"))?;

        match decoded {
            Ok(s) => {
                if s != "hello" {
                    return Err(format!("expected echo payload \"hello\", got {s:?}"));
                }
            }
            Err(e) => return Err(format!("expected Ok response, got Err({e:?})")),
        }

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify call.error.user] - User error from fallible method is returned as RoamError::User(E)
// r[verify call.response.encoding] - Response is POSTCARD Result<T, RoamError<E>>
#[test]
fn rpc_user_error_roundtrip() {
    run_async(async {
        let (mut io, mut child) = accept_subject().await?;

        // Hello exchange.
        let _ = io
            .recv_timeout(Duration::from_millis(250))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "expected Hello from subject".to_string())?;
        io.send(&Message::Hello(our_hello(1024 * 1024)))
            .await
            .map_err(|e| e.to_string())?;

        // Call divide(10, 0) - should return Err(MathError::DivisionByZero)
        let divide_method_id = compute_divide_method_id();
        let req_payload =
            facet_postcard::to_vec(&(10i64, 0i64)).map_err(|e| format!("postcard args: {e}"))?;
        let req = Message::Request {
            conn_id: roam_wire::ConnectionId::ROOT,
            request_id: 100,
            method_id: divide_method_id,
            metadata: metadata_empty(),
            channels: vec![],
            payload: req_payload,
        };
        io.send(&req).await.map_err(|e| e.to_string())?;

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
                if request_id != 100 {
                    return Err(format!("response request_id mismatch: {request_id}"));
                }
                payload
            }
            Message::Goodbye { reason, .. } => return Err(format!("unexpected Goodbye: {reason}")),
            other => return Err(format!("expected Response, got {other:?}")),
        };

        // The response should be Result<i64, RoamError<MathError>> = Err(User(DivisionByZero))
        let decoded: Result<i64, RoamErrorWithUser<MathError>> =
            facet_postcard::from_slice(&payload).map_err(|e| format!("postcard resp: {e}"))?;

        match decoded {
            Ok(v) => {
                return Err(format!("expected Err(User(DivisionByZero)), got Ok({v})"));
            }
            Err(RoamErrorWithUser::User(MathError::DivisionByZero)) => {
                // Success! The user error was properly roundtripped.
            }
            Err(other) => {
                return Err(format!(
                    "expected Err(User(DivisionByZero)), got Err({other:?})"
                ));
            }
        }

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify call.error.unknown-method] - Unknown method_id returns UnknownMethod error
// r[verify call.error.roam-error] - Protocol errors use RoamError variants
// r[verify call.error.protocol] - UnknownMethod is a protocol-level error (discriminant 1)
#[test]
fn rpc_unknown_method_returns_unknownmethod_error() {
    ensure_expected_ids();

    run_async(async {
        let (mut io, mut child) = accept_subject().await?;

        // Hello exchange.
        let _ = io
            .recv_timeout(Duration::from_millis(250))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "expected Hello from subject".to_string())?;
        io.send(&Message::Hello(our_hello(1024 * 1024)))
            .await
            .map_err(|e| e.to_string())?;

        // Well-formed request with an unknown method id.
        let req_payload = facet_postcard::to_vec(&(String::from("hello"),))
            .map_err(|e| format!("postcard args: {e}"))?;
        let req = Message::Request {
            conn_id: roam_wire::ConnectionId::ROOT,
            request_id: 2,
            method_id: 0xdeadbeef,
            metadata: metadata_empty(),
            channels: vec![],
            payload: req_payload,
        };
        io.send(&req).await.map_err(|e| e.to_string())?;

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
                if request_id != 2 {
                    return Err(format!("response request_id mismatch: {request_id}"));
                }
                payload
            }
            Message::Goodbye { reason, .. } => return Err(format!("unexpected Goodbye: {reason}")),
            other => return Err(format!("expected Response, got {other:?}")),
        };

        let decoded: Result<String, RoamError<Never>> =
            facet_postcard::from_slice(&payload).map_err(|e| format!("postcard resp: {e}"))?;

        match decoded {
            Ok(v) => return Err(format!("expected Err(UnknownMethod), got Ok({v:?})")),
            Err(RoamError::UnknownMethod) => {}
            Err(other) => return Err(format!("expected UnknownMethod, got {other:?}")),
        }

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify call.error.invalid-payload] - Malformed payload returns InvalidPayload error
#[test]
fn rpc_invalid_payload_returns_invalidpayload_error() {
    ensure_expected_ids();

    run_async(async {
        let (mut io, mut child) = accept_subject().await?;

        // Hello exchange.
        let _ = io
            .recv_timeout(Duration::from_millis(250))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "expected Hello from subject".to_string())?;
        io.send(&Message::Hello(our_hello(1024 * 1024)))
            .await
            .map_err(|e| e.to_string())?;

        // Send request with invalid payload (random bytes, not valid postcard).
        let req = Message::Request {
            conn_id: roam_wire::ConnectionId::ROOT,
            request_id: 3,
            method_id: method_id::echo(),
            metadata: metadata_empty(),
            channels: vec![],
            payload: vec![0xff, 0xff, 0xff, 0xff], // Invalid postcard data
        };
        io.send(&req).await.map_err(|e| e.to_string())?;

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
                if request_id != 3 {
                    return Err(format!("response request_id mismatch: {request_id}"));
                }
                payload
            }
            Message::Goodbye { reason, .. } => return Err(format!("unexpected Goodbye: {reason}")),
            other => return Err(format!("expected Response, got {other:?}")),
        };

        let decoded: Result<String, RoamError<Never>> =
            facet_postcard::from_slice(&payload).map_err(|e| format!("postcard resp: {e}"))?;

        match decoded {
            Ok(v) => return Err(format!("expected Err(InvalidPayload), got Ok({v:?})")),
            Err(RoamError::InvalidPayload) => {}
            Err(other) => return Err(format!("expected InvalidPayload, got {other:?}")),
        }

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify call.pipelining.allowed] - Multiple requests in flight simultaneously
// r[verify call.pipelining.independence] - Each request is independent
// r[verify core.call] - Each call has one Request and one Response
// r[verify core.call.request-id] - Request IDs correlate requests to responses
#[test]
fn rpc_pipelining_multiple_requests() {
    ensure_expected_ids();

    run_async(async {
        let (mut io, mut child) = accept_subject().await?;

        // Hello exchange.
        let _ = io
            .recv_timeout(Duration::from_millis(250))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "expected Hello from subject".to_string())?;
        io.send(&Message::Hello(our_hello(1024 * 1024)))
            .await
            .map_err(|e| e.to_string())?;

        // Send 3 requests without waiting for responses (pipelining).
        let messages = ["first", "second", "third"];
        for (i, msg) in messages.iter().enumerate() {
            let req_payload = facet_postcard::to_vec(&(msg.to_string(),))
                .map_err(|e| format!("postcard args: {e}"))?;
            let req = Message::Request {
                conn_id: roam_wire::ConnectionId::ROOT,
                request_id: (i + 10) as u64, // Use 10, 11, 12 to distinguish from other tests
                method_id: method_id::echo(),
                metadata: metadata_empty(),
                channels: vec![],
                payload: req_payload,
            };
            io.send(&req).await.map_err(|e| e.to_string())?;
        }

        // Collect all 3 responses (may arrive in any order).
        let mut responses: std::collections::HashMap<u64, String> =
            std::collections::HashMap::new();
        for _ in 0..3 {
            let resp = io
                .recv_timeout(Duration::from_millis(500))
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "expected Response from subject".to_string())?;

            match resp {
                Message::Response {
                    request_id,
                    payload,
                    ..
                } => {
                    let decoded: Result<String, RoamError<Never>> =
                        facet_postcard::from_slice(&payload)
                            .map_err(|e| format!("postcard resp: {e}"))?;
                    match decoded {
                        Ok(s) => {
                            responses.insert(request_id, s);
                        }
                        Err(e) => return Err(format!("expected Ok, got Err({e:?})")),
                    }
                }
                other => return Err(format!("expected Response, got {other:?}")),
            }
        }

        // Verify all 3 responses received with correct correlation.
        for (i, msg) in messages.iter().enumerate() {
            let request_id = (i + 10) as u64;
            match responses.get(&request_id) {
                Some(s) if s == *msg => {}
                Some(s) => {
                    return Err(format!(
                        "request_id {request_id}: expected {msg:?}, got {s:?}"
                    ));
                }
                None => return Err(format!("missing response for request_id {request_id}")),
            }
        }

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}
