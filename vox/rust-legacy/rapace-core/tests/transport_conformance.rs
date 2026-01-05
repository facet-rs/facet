//! Transport conformance tests.
//!
//! These tests verify that RPC calls work correctly across different transport
//! implementations (mem, stream, shm). All tests use proper RpcSession on both
//! client and server sides with full handshake.

use std::sync::Arc;

use rapace_core::{
    AnyTransport, ErrorCode, Frame, FrameFlags, INLINE_PAYLOAD_SIZE, MsgDescHot, RpcError,
    RpcSession,
};

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
}

/// Create an echo dispatcher that returns the request payload as-is.
fn echo_dispatcher(
    error_on_method: Option<u32>,
) -> impl Fn(
    Frame,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Frame, RpcError>> + Send>>
+ Send
+ Sync
+ 'static {
    move |request| {
        let should_error = error_on_method == Some(request.desc.method_id);
        Box::pin(async move {
            let mut desc = MsgDescHot::new();
            desc.msg_id = request.desc.msg_id;
            desc.channel_id = request.desc.channel_id;
            desc.method_id = request.desc.method_id;

            if should_error {
                let code = ErrorCode::InvalidArgument as u32;
                let message = "test error message";
                let mut bytes = Vec::new();
                bytes.extend_from_slice(&code.to_le_bytes());
                bytes.extend_from_slice(&(message.len() as u32).to_le_bytes());
                bytes.extend_from_slice(message.as_bytes());
                desc.flags = FrameFlags::ERROR | FrameFlags::EOS | FrameFlags::RESPONSE;
                Ok(Frame::with_payload(desc, bytes))
            } else {
                desc.flags = FrameFlags::DATA | FrameFlags::EOS | FrameFlags::RESPONSE;
                let payload = request.payload_bytes().to_vec();
                if payload.len() <= INLINE_PAYLOAD_SIZE {
                    Ok(Frame::with_inline_payload(desc, &payload)
                        .expect("inline payload should fit"))
                } else {
                    Ok(Frame::with_payload(desc, payload))
                }
            }
        })
    }
}

/// Create a streaming dispatcher that sends multiple chunks.
fn streaming_dispatcher(
    transport: AnyTransport,
) -> impl Fn(
    Frame,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Frame, RpcError>> + Send>>
+ Send
+ Sync
+ 'static {
    move |request| {
        let transport = transport.clone();
        let channel_id = request.desc.channel_id;
        let method_id = request.desc.method_id;
        let is_no_reply = request.desc.flags.contains(FrameFlags::NO_REPLY);

        Box::pin(async move {
            if is_no_reply {
                // Streaming: send multiple DATA frames + EOS
                for i in 0..3u8 {
                    let mut desc = MsgDescHot::new();
                    desc.msg_id = i as u64 + 1;
                    desc.channel_id = channel_id;
                    desc.method_id = method_id;
                    desc.flags = FrameFlags::DATA;

                    let payload = vec![i; 8];
                    let frame =
                        Frame::with_inline_payload(desc, &payload).expect("should fit inline");
                    transport
                        .send_frame(frame)
                        .await
                        .map_err(RpcError::Transport)?;
                }

                // Send EOS
                let mut desc = MsgDescHot::new();
                desc.msg_id = 999;
                desc.channel_id = channel_id;
                desc.method_id = method_id;
                desc.flags = FrameFlags::DATA | FrameFlags::EOS;
                let eos =
                    Frame::with_inline_payload(desc, &[]).expect("empty frame should fit inline");
                transport
                    .send_frame(eos)
                    .await
                    .map_err(RpcError::Transport)?;
            }

            // Return empty frame (ignored for NO_REPLY requests)
            Ok(Frame::new(MsgDescHot::new()))
        })
    }
}

async fn run_unary_round_trip(make_pair: impl FnOnce() -> (AnyTransport, AnyTransport)) {
    init_tracing();
    let (client_transport, server_transport) = make_pair();

    // Create server session
    let server_session = Arc::new(RpcSession::new_acceptor(server_transport.clone()));
    server_session.set_dispatcher(echo_dispatcher(None));
    let server_session_for_run = server_session.clone();
    let server_handle = tokio::spawn(server_session_for_run.run());

    // Create client session
    let client_session = Arc::new(RpcSession::new(client_transport));
    tokio::spawn(client_session.clone().run());

    let channel_id = client_session.next_channel_id();
    let method_id = 42;
    let payload = b"hello".to_vec();

    let response = client_session
        .call(channel_id, method_id, payload.clone())
        .await
        .expect("rpc call failed");

    assert_eq!(response.frame.payload_bytes(), payload);

    // Cleanup - abort server and close client
    server_handle.abort();
    client_session.close();
}

async fn run_unary_multiple_calls(make_pair: impl FnOnce() -> (AnyTransport, AnyTransport)) {
    init_tracing();
    let (client_transport, server_transport) = make_pair();

    // Create server session
    let server_session = Arc::new(RpcSession::new_acceptor(server_transport.clone()));
    server_session.set_dispatcher(echo_dispatcher(None));
    let server_handle = tokio::spawn(server_session.clone().run());

    // Create client session
    let client_session = Arc::new(RpcSession::new(client_transport));
    tokio::spawn(client_session.clone().run());

    for i in 0..3u8 {
        let channel_id = client_session.next_channel_id();
        let method_id = 7;
        let payload = vec![i; 16];
        let response = client_session
            .call(channel_id, method_id, payload.clone())
            .await
            .expect("rpc call failed");
        assert_eq!(response.frame.payload_bytes(), payload);
    }

    // Cleanup - abort server and close client
    server_handle.abort();
    client_session.close();
}

async fn run_error_response(make_pair: impl FnOnce() -> (AnyTransport, AnyTransport)) {
    init_tracing();
    let (client_transport, server_transport) = make_pair();

    // Create server session that returns error on method 9
    let server_session = Arc::new(RpcSession::new_acceptor(server_transport.clone()));
    server_session.set_dispatcher(echo_dispatcher(Some(9)));
    let server_handle = tokio::spawn(server_session.clone().run());

    // Create client session
    let client_session = Arc::new(RpcSession::new(client_transport));
    tokio::spawn(client_session.clone().run());

    let channel_id = client_session.next_channel_id();
    let method_id = 9;
    let payload = b"ignored".to_vec();

    let response = client_session
        .call(channel_id, method_id, payload)
        .await
        .expect("rpc call failed");

    assert!(response.frame.desc.flags.contains(FrameFlags::ERROR));
    let err = rapace_core::parse_error_payload(response.frame.payload_bytes());
    match err {
        RpcError::Status { code, message } => {
            assert_eq!(code, ErrorCode::InvalidArgument);
            assert_eq!(message, "test error message");
        }
        other => panic!("expected Status error, got {other:?}"),
    }

    // Cleanup - abort server and close client
    server_handle.abort();
    client_session.close();
}

async fn run_large_payload(make_pair: impl FnOnce() -> (AnyTransport, AnyTransport)) {
    init_tracing();
    let (client_transport, server_transport) = make_pair();

    // Create server session
    let server_session = Arc::new(RpcSession::new_acceptor(server_transport.clone()));
    server_session.set_dispatcher(echo_dispatcher(None));
    let server_handle = tokio::spawn(server_session.clone().run());

    // Create client session
    let client_session = Arc::new(RpcSession::new(client_transport));
    tokio::spawn(client_session.clone().run());

    let channel_id = client_session.next_channel_id();
    let method_id = 123;
    let payload = vec![0xAB; INLINE_PAYLOAD_SIZE + 1024];

    let response = client_session
        .call(channel_id, method_id, payload.clone())
        .await
        .expect("rpc call failed");

    assert_eq!(response.frame.payload_bytes(), payload);

    // Cleanup - abort server and close client
    server_handle.abort();
    client_session.close();
}

async fn run_server_streaming(make_pair: impl FnOnce() -> (AnyTransport, AnyTransport)) {
    init_tracing();
    let (client_transport, server_transport) = make_pair();

    // Create server session with streaming dispatcher
    let server_session = Arc::new(RpcSession::new_acceptor(server_transport.clone()));
    server_session.set_dispatcher(streaming_dispatcher(server_transport));
    let server_handle = tokio::spawn(server_session.clone().run());

    // Create client session
    let client_session = Arc::new(RpcSession::new(client_transport));
    tokio::spawn(client_session.clone().run());

    let mut rx = client_session
        .start_streaming_call(77, b"req".to_vec())
        .await
        .expect("start_streaming_call failed");

    let mut received = Vec::new();
    while let Some(chunk) = rx.recv().await {
        if chunk.is_error() {
            let err = rapace_core::parse_error_payload(chunk.payload_bytes());
            panic!("unexpected streaming error: {err:?}");
        }
        if chunk.is_eos() {
            break;
        }
        received.push(chunk.payload_bytes().to_vec());
    }

    assert_eq!(received, vec![vec![0u8; 8], vec![1u8; 8], vec![2u8; 8]]);

    // Cleanup - abort server and close client
    server_handle.abort();
    client_session.close();
}

#[tokio_test_lite::test]
async fn mem_unary_round_trip() {
    run_unary_round_trip(AnyTransport::mem_pair).await;
}

#[tokio_test_lite::test]
async fn mem_unary_multiple_calls() {
    run_unary_multiple_calls(AnyTransport::mem_pair).await;
}

#[tokio_test_lite::test]
async fn mem_error_response() {
    run_error_response(AnyTransport::mem_pair).await;
}

#[tokio_test_lite::test]
async fn mem_large_payload() {
    run_large_payload(AnyTransport::mem_pair).await;
}

#[tokio_test_lite::test]
async fn mem_server_streaming() {
    run_server_streaming(AnyTransport::mem_pair).await;
}

#[cfg(feature = "stream")]
#[tokio_test_lite::test]
async fn stream_unary_round_trip() {
    run_unary_round_trip(AnyTransport::stream_pair).await;
}

#[cfg(feature = "stream")]
#[tokio_test_lite::test]
async fn stream_server_streaming() {
    run_server_streaming(AnyTransport::stream_pair).await;
}

#[cfg(feature = "shm")]
#[tokio_test_lite::test]
async fn shm_unary_round_trip() {
    let make_pair = || {
        let (a, b) = rapace_core::shm::ShmTransport::hub_pair().expect("shm hub pair");
        (AnyTransport::new(a), AnyTransport::new(b))
    };
    run_unary_round_trip(make_pair).await;
}
