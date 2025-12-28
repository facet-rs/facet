use std::sync::Arc;

use rapace_core::{
    AnyTransport, ErrorCode, Frame, FrameFlags, INLINE_PAYLOAD_SIZE, MsgDescHot, RpcError,
    RpcSession,
};

async fn spawn_echo_server(
    server_transport: AnyTransport,
    expected_calls: usize,
    error_on_call: Option<usize>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        for idx in 0..expected_calls {
            let request = server_transport
                .recv_frame()
                .await
                .expect("server recv_frame failed");

            let mut desc = MsgDescHot::new();
            desc.msg_id = request.desc.msg_id;
            desc.channel_id = request.desc.channel_id;
            desc.method_id = 0; // Responses must use method_id = 0

            let (flags, payload) = if error_on_call == Some(idx) {
                let code = ErrorCode::InvalidArgument as u32;
                let message = "test error message";
                let mut bytes = Vec::new();
                bytes.extend_from_slice(&code.to_le_bytes());
                bytes.extend_from_slice(&(message.len() as u32).to_le_bytes());
                bytes.extend_from_slice(message.as_bytes());
                (FrameFlags::ERROR | FrameFlags::EOS, bytes)
            } else {
                (
                    FrameFlags::DATA | FrameFlags::EOS,
                    request.payload_bytes().to_vec(),
                )
            };

            desc.flags = flags;

            let response = if payload.len() <= INLINE_PAYLOAD_SIZE {
                Frame::with_inline_payload(desc, &payload).expect("inline payload should fit")
            } else {
                Frame::with_payload(desc, payload)
            };

            server_transport
                .send_frame(response)
                .await
                .expect("server send_frame failed");
        }
    })
}

async fn run_unary_round_trip(make_pair: impl FnOnce() -> (AnyTransport, AnyTransport)) {
    let (client_transport, server_transport) = make_pair();

    let server_task = spawn_echo_server(server_transport, 1, None).await;

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
    server_task.await.expect("server task join failed");
}

async fn run_unary_multiple_calls(make_pair: impl FnOnce() -> (AnyTransport, AnyTransport)) {
    let (client_transport, server_transport) = make_pair();

    let server_task = spawn_echo_server(server_transport, 3, None).await;

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

    server_task.await.expect("server task join failed");
}

async fn run_error_response(make_pair: impl FnOnce() -> (AnyTransport, AnyTransport)) {
    let (client_transport, server_transport) = make_pair();

    let server_task = spawn_echo_server(server_transport, 1, Some(0)).await;

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

    server_task.await.expect("server task join failed");
}

async fn run_large_payload(make_pair: impl FnOnce() -> (AnyTransport, AnyTransport)) {
    let (client_transport, server_transport) = make_pair();

    let server_task = spawn_echo_server(server_transport, 1, None).await;

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
    server_task.await.expect("server task join failed");
}

async fn run_server_streaming(make_pair: impl FnOnce() -> (AnyTransport, AnyTransport)) {
    let (client_transport, server_transport) = make_pair();

    let client_session = Arc::new(RpcSession::new(client_transport));
    tokio::spawn(client_session.clone().run());

    let server_task = tokio::spawn(async move {
        let request = server_transport
            .recv_frame()
            .await
            .expect("server recv_frame failed");
        assert!(
            request.desc.flags.contains(FrameFlags::NO_REPLY),
            "streaming requests must be flagged NO_REPLY so the server session doesn't send a unary response"
        );
        let channel_id = request.desc.channel_id;
        let method_id = request.desc.method_id;

        for i in 0..3u8 {
            let mut desc = MsgDescHot::new();
            desc.msg_id = i as u64 + 1;
            desc.channel_id = channel_id;
            desc.method_id = method_id;
            desc.flags = FrameFlags::DATA;

            let payload = vec![i; 8];
            let frame = Frame::with_inline_payload(desc, &payload).expect("should fit inline");
            server_transport
                .send_frame(frame)
                .await
                .expect("server send_frame failed");
        }

        let mut desc = MsgDescHot::new();
        desc.msg_id = 999;
        desc.channel_id = channel_id;
        desc.method_id = method_id;
        desc.flags = FrameFlags::DATA | FrameFlags::EOS;
        let eos = Frame::with_inline_payload(desc, &[]).expect("empty frame should fit inline");
        server_transport
            .send_frame(eos)
            .await
            .expect("server send_frame failed");
    });

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

    server_task.await.expect("server task join failed");
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
