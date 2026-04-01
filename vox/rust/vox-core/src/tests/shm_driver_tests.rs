use std::sync::Arc;

use moire::task::FutureExt;
use shm_primitives::FileCleanup;
use vox_shm::varslot::SizeClassConfig;
use vox_shm::{Segment, SegmentConfig, ShmLink, create_test_link_pair};
use vox_types::{
    ConnectionSettings, Handler, HandshakeResult, MessageFamily, MethodId, Parity, Payload,
    ReplySink, RequestCall, RequestResponse, SelfRef, SessionRole,
};

use crate::session::{acceptor_conduit, initiator_conduit};
use crate::{BareConduit, DriverReplySink};

fn test_acceptor_handshake() -> HandshakeResult {
    HandshakeResult {
        role: SessionRole::Acceptor,
        our_settings: ConnectionSettings {
            parity: Parity::Even,
            max_concurrent_requests: 64,
        },
        peer_settings: ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
        },
        peer_supports_retry: true,
        session_resume_key: None,
        peer_resume_key: None,
        our_schema: vec![],
        peer_schema: vec![],
    }
}

fn test_initiator_handshake() -> HandshakeResult {
    HandshakeResult {
        role: SessionRole::Initiator,
        our_settings: ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
        },
        peer_settings: ConnectionSettings {
            parity: Parity::Even,
            max_concurrent_requests: 64,
        },
        peer_supports_retry: true,
        session_resume_key: None,
        peer_resume_key: None,
        our_schema: vec![],
        peer_schema: vec![],
    }
}

type MessageConduit = BareConduit<MessageFamily, ShmLink>;

async fn message_conduit_pair() -> (MessageConduit, MessageConduit, tempfile::TempDir) {
    let classes = [SizeClassConfig {
        slot_size: 4096,
        slot_count: 16,
    }];
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("shm-driver-test.shm");
    let segment = Arc::new(
        Segment::create(
            &path,
            SegmentConfig {
                max_guests: 1,
                bipbuf_capacity: 1 << 16,
                max_payload_size: 1 << 20,
                inline_threshold: 256,
                heartbeat_interval: 0,
                size_classes: &classes,
            },
            FileCleanup::Manual,
        )
        .expect("create segment"),
    );
    let (a, b) = create_test_link_pair(segment)
        .await
        .expect("create_test_link_pair");
    (BareConduit::new(a), BareConduit::new(b), dir)
}

struct EchoHandler;

impl Handler<DriverReplySink> for EchoHandler {
    async fn handle(
        &self,
        call: SelfRef<RequestCall<'static>>,
        reply: DriverReplySink,
        _schemas: std::sync::Arc<vox_types::SchemaRecvTracker>,
    ) {
        let args_bytes = match &call.args {
            Payload::PostcardBytes(bytes) => *bytes,
            _ => panic!("expected incoming payload"),
        };

        let result: u32 = vox_postcard::from_slice(args_bytes).expect("deserialize args");
        reply
            .send_reply(RequestResponse {
                ret: Payload::outgoing(&result),
                schemas: Default::default(),
                metadata: Default::default(),
            })
            .await;
    }
}

#[tokio::test]
async fn echo_call_across_shm_link() {
    let (client_conduit, server_conduit, _dir) = message_conduit_pair().await;

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .establish::<crate::NoopClient>(EchoHandler)
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (caller, _sh) = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<crate::NoopClient>(())
        .await
        .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let args_value: u32 = 42;
    let response = caller
        .caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            schemas: Default::default(),
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed");

    let ret_bytes = match &response.ret {
        Payload::PostcardBytes(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let result: u32 = vox_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(result, 42);
}

struct BlobEchoHandler;

impl Handler<DriverReplySink> for BlobEchoHandler {
    async fn handle(
        &self,
        call: SelfRef<RequestCall<'static>>,
        reply: DriverReplySink,
        _schemas: std::sync::Arc<vox_types::SchemaRecvTracker>,
    ) {
        let args_bytes = match &call.args {
            Payload::PostcardBytes(bytes) => *bytes,
            _ => panic!("expected incoming payload"),
        };

        eprintln!(
            "[blob handler] args_bytes len={}: {:02x?}",
            args_bytes.len(),
            &args_bytes[..args_bytes.len().min(32)]
        );
        let blob: Vec<u8> = vox_postcard::from_slice(args_bytes).expect("deserialize blob");
        eprintln!("[blob handler] got blob len={}, sending back", blob.len());
        let response_bytes = vox_postcard::to_vec(&blob).unwrap();
        eprintln!(
            "[blob handler] response_bytes len={}: {:02x?}",
            response_bytes.len(),
            &response_bytes[..response_bytes.len().min(16)]
        );
        reply
            .send_reply(RequestResponse {
                ret: Payload::outgoing(&blob),
                schemas: Default::default(),
                metadata: Default::default(),
            })
            .await;
    }
}

#[tokio::test]
async fn echo_blob_stress_over_shm_link() {
    let (client_conduit, server_conduit, _dir) = message_conduit_pair().await;

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor_conduit(server_conduit, test_acceptor_handshake())
                .establish::<crate::NoopClient>(BlobEchoHandler)
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (caller, _sh) = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<crate::NoopClient>(())
        .await
        .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    // Alternate tiny and large payloads to exercise both inline and slot-ref SHM paths.
    for i in 0..200 {
        let len = if i % 2 == 0 { 32 } else { 2048 };
        let payload = vec![(i % 251) as u8; len];
        let response = caller
            .caller
            .call(RequestCall {
                method_id: MethodId(2),
                args: Payload::outgoing(&payload),
                schemas: Default::default(),
                metadata: Default::default(),
            })
            .await
            .expect("blob echo call should succeed");

        let ret_bytes = match &response.ret {
            Payload::PostcardBytes(bytes) => *bytes,
            _ => panic!("expected incoming payload in response"),
        };
        let echoed: Vec<u8> = vox_postcard::from_slice(ret_bytes).unwrap_or_else(|e| {
            panic!(
                "iter {i}: deserialize echoed blob (ret_bytes len={}): {e}",
                ret_bytes.len()
            )
        });
        assert_eq!(echoed, payload);
    }
}
