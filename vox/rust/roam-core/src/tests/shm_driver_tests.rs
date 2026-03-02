use std::sync::Arc;

use moire::task::FutureExt;
use roam_shm::varslot::SizeClassConfig;
use roam_shm::{Segment, SegmentConfig, ShmLink, create_test_link_pair};
use roam_types::{
    Caller, Handler, MessageFamily, MethodId, Parity, Payload, ReplySink, RequestCall,
    RequestResponse, SelfRef,
};
use shm_primitives::FileCleanup;

use crate::session::{acceptor, initiator};
use crate::{BareConduit, Driver, DriverReplySink};

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

        let result: u32 = facet_postcard::from_slice(args_bytes).expect("deserialize args");
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
            let (mut server_session, server_handle, _sh) = acceptor(server_conduit)
                .establish()
                .await
                .expect("server handshake failed");
            let mut server_driver = Driver::new(server_handle, EchoHandler, Parity::Even);
            moire::task::spawn(async move { server_session.run().await }.named("server_session"));
            moire::task::spawn(async move { server_driver.run().await }.named("server_driver"));
        }
        .named("server_setup"),
    );

    let (mut client_session, client_handle, _sh) = initiator(client_conduit)
        .establish()
        .await
        .expect("client handshake failed");
    let mut client_driver = Driver::new(client_handle, (), Parity::Odd);
    let caller = client_driver.caller();
    moire::task::spawn(async move { client_session.run().await }.named("client_session"));
    moire::task::spawn(async move { client_driver.run().await }.named("client_driver"));

    server_task.await.expect("server setup failed");

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
    let result: u32 = facet_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(result, 42);
}

struct BlobEchoHandler;

impl Handler<DriverReplySink> for BlobEchoHandler {
    async fn handle(&self, call: SelfRef<RequestCall<'static>>, reply: DriverReplySink) {
        let args_bytes = match &call.args {
            Payload::Incoming(bytes) => *bytes,
            _ => panic!("expected incoming payload"),
        };

        let blob: Vec<u8> = facet_postcard::from_slice(args_bytes).expect("deserialize blob");
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
            let (mut server_session, server_handle, _sh) = acceptor(server_conduit)
                .establish()
                .await
                .expect("server handshake failed");
            let mut server_driver = Driver::new(server_handle, BlobEchoHandler, Parity::Even);
            moire::task::spawn(async move { server_session.run().await }.named("server_session"));
            moire::task::spawn(async move { server_driver.run().await }.named("server_driver"));
        }
        .named("server_setup"),
    );

    let (mut client_session, client_handle, _sh) = initiator(client_conduit)
        .establish()
        .await
        .expect("client handshake failed");
    let mut client_driver = Driver::new(client_handle, (), Parity::Odd);
    let caller = client_driver.caller();
    moire::task::spawn(async move { client_session.run().await }.named("client_session"));
    moire::task::spawn(async move { client_driver.run().await }.named("client_driver"));

    server_task.await.expect("server setup failed");

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
        let echoed: Vec<u8> =
            facet_postcard::from_slice(ret_bytes).expect("deserialize echoed blob");
        assert_eq!(echoed, payload);
    }
}
