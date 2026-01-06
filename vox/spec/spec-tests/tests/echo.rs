use std::time::Duration;

use facet::Facet;
use rapace_hash::method_id_from_detail;
use rapace_schema::{ArgDetail, MethodDetail, TypeDetail};
use rapace_wire::{Hello, Message, MetadataValue};
use spec_tests::harness::{accept_subject, our_hello, run_async};

// TODO: Remove this shim once facet implements `Facet` for `core::convert::Infallible`
// and for the never type `!`, then use `Infallible` as the error type parameter.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
struct Never;

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
enum RapaceError<E> {
    User(E) = 0,
    UnknownMethod = 1,
    InvalidPayload = 2,
    Cancelled = 3,
}

fn echo_method_id(method_name: &str) -> u64 {
    let detail = MethodDetail {
        service_name: "Echo".into(),
        method_name: method_name.into(),
        args: vec![ArgDetail {
            name: "message".into(),
            type_info: TypeDetail::String,
        }],
        return_type: TypeDetail::String,
        doc: None,
    };
    method_id_from_detail(&detail)
}

fn metadata_empty() -> Vec<(String, MetadataValue)> {
    Vec::new()
}

fn ensure_expected_ids() {
    // Keep this in sync with subjects that hardcode IDs for now.
    assert_eq!(echo_method_id("echo"), 0x3d66dd9ee36b4240);
    assert_eq!(echo_method_id("reverse"), 0x268246d3219503fb);

    // Ensure the proto crate matches the spec-derived IDs.
    let svc = spec_proto::echo_service_detail();
    let ids = svc
        .methods
        .iter()
        .map(|m| (m.method_name.as_str(), method_id_from_detail(m)))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(ids.get("echo").copied(), Some(echo_method_id("echo")));
    assert_eq!(ids.get("reverse").copied(), Some(echo_method_id("reverse")));
}

#[test]
fn unary_echo_roundtrip() {
    ensure_expected_ids();

    run_async(async {
        let (mut io, mut child) = accept_subject().await?;

        // Subject hello first.
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

        let req_payload = facet_postcard::to_vec(&(String::from("hello"),))
            .map_err(|e| format!("postcard args: {e}"))?;
        let req = Message::Request {
            request_id: 1,
            method_id: echo_method_id("echo"),
            metadata: metadata_empty(),
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
                metadata: _,
                payload,
            } => {
                if request_id != 1 {
                    return Err(format!("response request_id mismatch: {request_id}"));
                }
                payload
            }
            Message::Goodbye { reason } => return Err(format!("unexpected Goodbye: {reason}")),
            other => return Err(format!("expected Response, got {other:?}")),
        };

        let decoded: Result<String, RapaceError<Never>> =
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

#[test]
fn unary_unknown_method_returns_unknownmethod_error() {
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
            request_id: 2,
            method_id: 0xdeadbeef,
            metadata: metadata_empty(),
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
                metadata: _,
                payload,
            } => {
                if request_id != 2 {
                    return Err(format!("response request_id mismatch: {request_id}"));
                }
                payload
            }
            Message::Goodbye { reason } => return Err(format!("unexpected Goodbye: {reason}")),
            other => return Err(format!("expected Response, got {other:?}")),
        };

        let decoded: Result<String, RapaceError<Never>> =
            facet_postcard::from_slice(&payload).map_err(|e| format!("postcard resp: {e}"))?;

        match decoded {
            Ok(v) => return Err(format!("expected Err(UnknownMethod), got Ok({v:?})")),
            Err(RapaceError::UnknownMethod) => {}
            Err(other) => return Err(format!("expected UnknownMethod, got {other:?}")),
        }

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}
