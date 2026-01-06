use std::time::Duration;

use cobs::encode_vec as cobs_encode_vec;
use rapace_wire::{Hello, Message, MetadataValue};
use spec_tests::harness::{accept_subject, our_hello, run_async};
use tokio::io::AsyncWriteExt;

fn metadata_empty() -> Vec<(String, MetadataValue)> {
    Vec::new()
}

/// Spec: `[verify message.hello.timing]` - Both peers MUST send Hello immediately
/// after connection establishment, before any other message.
#[test]
fn handshake_subject_sends_hello_without_prompt() {
    run_async(async {
        let (mut io, mut child) = accept_subject().await?;

        // Do NOT send our Hello yet. Subject should still send Hello immediately.
        let msg = io
            .recv_timeout(Duration::from_millis(250))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "did not receive any message (expected Hello)".to_string())?;

        match msg {
            Message::Hello(Hello::V1 { .. }) => {}
            other => return Err(format!("first message must be Hello, got {other:?}")),
        }

        // Clean shutdown: send our Hello so a well-behaved subject can proceed.
        io.send(&Message::Hello(our_hello(1024 * 1024)))
            .await
            .map_err(|e| e.to_string())?;

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}

/// Spec: `[verify message.hello.ordering]` - A peer MUST NOT send any message other
/// than Hello until it has both sent and received Hello.
#[test]
fn handshake_no_non_hello_before_hello_exchange() {
    run_async(async {
        let (mut io, mut child) = accept_subject().await?;

        // Expect subject Hello first.
        let msg = io
            .recv_timeout(Duration::from_millis(250))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "did not receive any message (expected Hello)".to_string())?;

        if !matches!(msg, Message::Hello(_)) {
            return Err(format!("first message must be Hello, got {msg:?}"));
        }

        // Before we send our Hello, subject MUST NOT send other messages.
        if let Some(extra) = io
            .recv_timeout(Duration::from_millis(100))
            .await
            .map_err(|e| e.to_string())?
        {
            return Err(format!(
                "subject sent a message before hello exchange completed: {extra:?}"
            ));
        }

        // Complete exchange and exit.
        io.send(&Message::Hello(our_hello(1024 * 1024)))
            .await
            .map_err(|e| e.to_string())?;

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}

/// Spec: `[verify message.hello.unknown-version]` - If a peer receives a Hello with
/// an unknown variant, it MUST send a Goodbye message and close the connection.
#[test]
fn handshake_unknown_hello_variant_triggers_goodbye() {
    run_async(async {
        let (mut io, mut child) = accept_subject().await?;

        // Send a malformed Hello-in-Message: Message::Hello + unknown Hello variant discriminant.
        //
        // Postcard enum encoding uses a varint discriminant. For `Message`, `Hello` is variant 0,
        // and for `Hello`, `V1` is variant 0. We send Hello discriminant=1 to simulate unknown.
        let malformed = vec![0x00, 0x01]; // Message::Hello (0), Hello::<unknown> (1)
        let mut framed = cobs_encode_vec(&malformed);
        framed.push(0x00);
        io.stream
            .write_all(&framed)
            .await
            .map_err(|e| e.to_string())?;

        // Look for Goodbye (subject may also send Hello; ignore it).
        let mut saw_goodbye = None::<String>;
        for _ in 0..10 {
            match io
                .recv_timeout(Duration::from_millis(250))
                .await
                .map_err(|e| e.to_string())?
            {
                None => break,
                Some(Message::Goodbye { reason }) => {
                    saw_goodbye = Some(reason);
                    break;
                }
                Some(_) => continue,
            }
        }

        let reason = saw_goodbye.ok_or_else(|| "expected Goodbye, got none".to_string())?;
        if !reason.contains("message.hello.unknown-version") {
            return Err(format!(
                "Goodbye reason must mention message.hello.unknown-version, got {reason:?}"
            ));
        }

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}

/// Spec: `[verify flow.unary.payload-limit]` - Unary RPC payloads are bounded by
/// max_payload_size negotiated during handshake.
#[test]
fn unary_payload_over_max_triggers_goodbye() {
    run_async(async {
        let (mut io, mut child) = accept_subject().await?;

        // Receive subject hello (ignore contents for now).
        let _ = io
            .recv_timeout(Duration::from_millis(250))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "expected Hello from subject".to_string())?;

        // Send our hello with a tiny max payload size.
        io.send(&Message::Hello(our_hello(16)))
            .await
            .map_err(|e| e.to_string())?;

        // Send an oversized Request payload (17 bytes).
        let req = Message::Request {
            request_id: 1,
            method_id: 1,
            metadata: metadata_empty(),
            payload: vec![0u8; 17],
        };
        io.send(&req).await.map_err(|e| e.to_string())?;

        // Expect Goodbye with the relevant rule id.
        let mut reason = None::<String>;
        for _ in 0..10 {
            match io
                .recv_timeout(Duration::from_millis(250))
                .await
                .map_err(|e| e.to_string())?
            {
                None => break,
                Some(Message::Goodbye { reason: r }) => {
                    reason = Some(r);
                    break;
                }
                Some(_) => continue,
            }
        }

        let reason =
            reason.ok_or_else(|| "expected Goodbye after oversized Request".to_string())?;
        if !reason.contains("flow.unary.payload-limit") {
            return Err(format!(
                "Goodbye reason must mention flow.unary.payload-limit, got {reason:?}"
            ));
        }

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}

/// Spec: `[verify streaming.id.zero-reserved]` - Stream ID 0 is reserved; if a peer
/// receives a stream message with stream_id of 0, it MUST send a Goodbye message.
#[test]
fn stream_id_zero_triggers_goodbye() {
    run_async(async {
        let (mut io, mut child) = accept_subject().await?;

        // Receive subject hello.
        let _ = io
            .recv_timeout(Duration::from_millis(250))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "expected Hello from subject".to_string())?;

        // Send our hello.
        io.send(&Message::Hello(our_hello(1024 * 1024)))
            .await
            .map_err(|e| e.to_string())?;

        // Violate stream-id=0 reserved.
        io.send(&Message::Close { stream_id: 0 })
            .await
            .map_err(|e| e.to_string())?;

        let mut reason = None::<String>;
        for _ in 0..10 {
            match io
                .recv_timeout(Duration::from_millis(250))
                .await
                .map_err(|e| e.to_string())?
            {
                None => break,
                Some(Message::Goodbye { reason: r }) => {
                    reason = Some(r);
                    break;
                }
                Some(_) => continue,
            }
        }

        let reason = reason.ok_or_else(|| "expected Goodbye after stream_id=0".to_string())?;
        let ok = reason.contains("streaming.id.zero-reserved")
            || reason.contains("core.stream.id.zero-reserved");
        if !ok {
            return Err(format!(
                "Goodbye reason must mention a stream-id-zero rule id, got {reason:?}"
            ));
        }

        let _ = child.kill().await;
        Ok::<_, String>(())
    })
    .unwrap();
}
