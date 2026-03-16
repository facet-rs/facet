use std::sync::Arc;

use moire::task::FutureExt;
use roam_shm::varslot::SizeClassConfig;
use roam_shm::{Segment, SegmentConfig, ShmLink, create_test_link_pair};
use roam_types::{
    Caller, Handler, MessageFamily, MethodId, Payload, ReplySink, RequestCall, RequestResponse,
    SelfRef,
};
use shm_primitives::FileCleanup;

use crate::session::{acceptor, initiator_conduit};
use crate::{BareConduit, DriverCaller, DriverReplySink};

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
    async fn handle(&self, call: SelfRef<RequestCall<'static>>, reply: DriverReplySink) {
        let args_bytes = match &call.args {
            Payload::Incoming(bytes) => *bytes,
            _ => panic!("expected incoming payload"),
        };

        let result: u32 = roam_postcard::from_slice(args_bytes).expect("deserialize args");
        reply
            .send_reply(RequestResponse {
                ret: Payload::outgoing(&result),
                channels: vec![],
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
            let (server_caller, _sh) = acceptor(server_conduit)
                .establish::<DriverCaller>(EchoHandler)
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (caller, _sh) = initiator_conduit(client_conduit)
        .establish::<DriverCaller>(())
        .await
        .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let args_value: u32 = 42;
    let response = caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            channels: vec![],
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed");

    let ret_bytes = match &response.ret {
        Payload::Incoming(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let result: u32 = roam_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(result, 42);
}

struct BlobEchoHandler;

impl Handler<DriverReplySink> for BlobEchoHandler {
    async fn handle(&self, call: SelfRef<RequestCall<'static>>, reply: DriverReplySink) {
        let args_bytes = match &call.args {
            Payload::Incoming(bytes) => *bytes,
            _ => panic!("expected incoming payload"),
        };

        eprintln!(
            "[blob handler] args_bytes len={}: {:02x?}",
            args_bytes.len(),
            &args_bytes[..args_bytes.len().min(32)]
        );
        let blob: Vec<u8> = roam_postcard::from_slice(args_bytes).expect("deserialize blob");
        eprintln!("[blob handler] got blob len={}, sending back", blob.len());
        let response_bytes = roam_postcard::to_vec(&blob).unwrap();
        eprintln!(
            "[blob handler] response_bytes len={}: {:02x?}",
            response_bytes.len(),
            &response_bytes[..response_bytes.len().min(16)]
        );
        reply
            .send_reply(RequestResponse {
                ret: Payload::outgoing(&blob),
                channels: vec![],
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
            let (server_caller, _sh) = acceptor(server_conduit)
                .establish::<DriverCaller>(BlobEchoHandler)
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (caller, _sh) = initiator_conduit(client_conduit)
        .establish::<DriverCaller>(())
        .await
        .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    // Alternate tiny and large payloads to exercise both inline and slot-ref SHM paths.
    for i in 0..200 {
        let len = if i % 2 == 0 { 32 } else { 2048 };
        let payload = vec![(i % 251) as u8; len];
        let response = caller
            .call(RequestCall {
                method_id: MethodId(2),
                args: Payload::outgoing(&payload),
                channels: vec![],
                metadata: Default::default(),
            })
            .await
            .expect("blob echo call should succeed");

        let ret_bytes = match &response.ret {
            Payload::Incoming(bytes) => *bytes,
            _ => panic!("expected incoming payload in response"),
        };
        let echoed: Vec<u8> = roam_postcard::from_slice(ret_bytes).unwrap_or_else(|e| {
            panic!(
                "iter {i}: deserialize echoed blob (ret_bytes len={}): {e}",
                ret_bytes.len()
            )
        });
        assert_eq!(echoed, payload);
    }
}
