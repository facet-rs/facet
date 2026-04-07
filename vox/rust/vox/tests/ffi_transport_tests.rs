use vox::{ConnectionSettings, Parity, SessionHandle};
use vox_ffi::declare_link_endpoint;

#[vox::service]
trait Ping {
    async fn ping(&self, value: u32) -> u32;
}

#[derive(Clone)]
struct PingService;

impl Ping for PingService {
    async fn ping(&self, value: u32) -> u32 {
        value + 1_000
    }
}

#[vox::service]
trait Pong {
    async fn pong(&self, value: u32) -> u32;
}

#[derive(Clone)]
struct PongService;

impl Pong for PongService {
    async fn pong(&self, value: u32) -> u32 {
        value + 2_000
    }
}

declare_link_endpoint!(mod ffi_pair_ab_a { export = vox_ffi_pair_ab_a_v1; });
declare_link_endpoint!(mod ffi_pair_ab_b { export = vox_ffi_pair_ab_b_v1; });
declare_link_endpoint!(mod ffi_pair_ba_a { export = vox_ffi_pair_ba_a_v1; });
declare_link_endpoint!(mod ffi_pair_ba_b { export = vox_ffi_pair_ba_b_v1; });

fn connection_settings() -> ConnectionSettings {
    ConnectionSettings {
        parity: Parity::Odd,
        max_concurrent_requests: 64,
    }
}

async fn open_ping(session: &SessionHandle) -> PingClient {
    session
        .open(connection_settings())
        .await
        .expect("open Ping virtual connection")
}

async fn open_pong(session: &SessionHandle) -> PongClient {
    session
        .open(connection_settings())
        .await
        .expect("open Pong virtual connection")
}

// r[verify rpc.session-setup]
// r[verify rpc.virtual-connection.accept]
// r[verify rpc.request]
// r[verify rpc.response]
#[tokio::test]
async fn ffi_transport_supports_bidirectional_calls_with_two_services_when_a_initiates() {
    let server = tokio::spawn(async move {
        let link = ffi_pair_ab_b::accept().await.expect("accept ffi link");
        let root = vox::acceptor_on(link)
            .on_connection(PongDispatcher::new(PongService))
            .establish::<vox::NoopClient>()
            .await
            .expect("acceptor establish");
        let session = root.session.clone().expect("acceptor session");
        let ping = open_ping(&session).await;
        let response = ping.ping(7).await.expect("Ping call over FFI");
        (response, root)
    });

    let link = ffi_pair_ab_a::connect(ffi_pair_ab_b::vtable()).expect("connect ffi link");
    let root = vox::initiator_on(link, vox::TransportMode::Bare)
        .on_connection(PingDispatcher::new(PingService))
        .establish::<vox::NoopClient>()
        .await
        .expect("initiator establish");
    let session = root.session.clone().expect("initiator session");
    let pong = open_pong(&session).await;

    assert_eq!(pong.pong(11).await.expect("Pong call over FFI"), 2_011);

    let (server_response, _server_root) = server.await.expect("server task");
    assert_eq!(server_response, 1_007);
    drop(root);
}

// r[verify rpc.session-setup]
// r[verify rpc.virtual-connection.accept]
// r[verify rpc.request]
// r[verify rpc.response]
#[tokio::test]
async fn ffi_transport_supports_bidirectional_calls_with_two_services_when_b_initiates() {
    let server = tokio::spawn(async move {
        let link = ffi_pair_ba_a::accept().await.expect("accept ffi link");
        let root = vox::acceptor_on(link)
            .on_connection(PingDispatcher::new(PingService))
            .establish::<vox::NoopClient>()
            .await
            .expect("acceptor establish");
        let session = root.session.clone().expect("acceptor session");
        let pong = open_pong(&session).await;
        let response = pong.pong(13).await.expect("Pong call over FFI");
        (response, root)
    });

    let link = ffi_pair_ba_b::connect(ffi_pair_ba_a::vtable()).expect("connect ffi link");
    let root = vox::initiator_on(link, vox::TransportMode::Bare)
        .on_connection(PongDispatcher::new(PongService))
        .establish::<vox::NoopClient>()
        .await
        .expect("initiator establish");
    let session = root.session.clone().expect("initiator session");
    let ping = open_ping(&session).await;

    assert_eq!(ping.ping(5).await.expect("Ping call over FFI"), 1_005);

    let (server_response, _server_root) = server.await.expect("server task");
    assert_eq!(server_response, 2_013);
    drop(root);
}
