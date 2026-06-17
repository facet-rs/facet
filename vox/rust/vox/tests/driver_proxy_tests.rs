//! Tests for proxy_lanes — transparent call forwarding between lanes.

use vox::{ConnectionHandle, ConnectionSettings, Driver, LaneRejection, Parity, memory_link_pair};

#[vox::service]
trait Echo {
    async fn echo(&self, value: u32) -> u32;
}

#[derive(Clone)]
struct EchoService;

impl Echo for EchoService {
    async fn echo(&self, value: u32) -> u32 {
        value
    }
}

struct ProxyAcceptor {
    upstream_connection: ConnectionHandle,
}

impl vox::LaneAcceptor for ProxyAcceptor {
    fn accept(
        &self,
        request: &vox::LaneRequest,
        connection: vox::PendingLane,
    ) -> Result<(), LaneRejection> {
        if request.service() == "Noop" {
            connection.handle_with(());
            return Ok(());
        }
        let upstream_connection = self.upstream_connection.clone();
        let incoming = connection.into_handle();
        tokio::spawn(async move {
            let upstream = upstream_connection
                .open_lane_handle(
                    ConnectionSettings {
                        parity: Parity::Odd,
                        max_concurrent_requests: 64,
                        initial_channel_credit: 16,
                    },
                    vox_types::metadata().str("vox-service", "Echo").build(),
                )
                .await
                .expect("open upstream connection");
            let _ = vox::proxy_lanes(incoming, upstream).await;
        });
        Ok(())
    }
}

#[tokio::test]
async fn proxy_lanes_forwards_calls() {
    // guest-a <-> host <-> guest-b
    // guest-a opens a service lane through host, which proxies to guest-b.
    let (host_b_link, guest_b_link) = memory_link_pair(16);
    let (host_a_link, guest_a_link) = memory_link_pair(16);

    // guest-b: accepts service lanes with EchoService
    let guest_b_task = tokio::spawn(async move {
        let guard = vox::acceptor_on(guest_b_link)
            .on_lane(EchoDispatcher::new(EchoService))
            .establish_connection()
            .await
            .expect("guest-b establish");
        let _guard = guard;
        std::future::pending::<()>().await
    });

    // host <-> guest-b control lane
    let _host_to_b = vox::initiator_on(host_b_link)
        .establish_connection()
        .await
        .expect("host<->guest-b establish");
    let host_to_b_connection = _host_to_b.clone();

    // host: accepts connections from guest-a and proxies to guest-b
    let host_for_a_task = tokio::spawn(async move {
        let guard = vox::acceptor_on(host_a_link)
            .on_lane(ProxyAcceptor {
                upstream_connection: host_to_b_connection,
            })
            .establish_connection()
            .await
            .expect("host<->guest-a establish");
        let _guard = guard;
        std::future::pending::<()>().await
    });

    // guest-a <-> host control lane
    let _guest_a_connection = vox::initiator_on(guest_a_link)
        .establish_connection()
        .await
        .expect("guest-a establish");
    let guest_a_connection = _guest_a_connection.clone();

    // Open a proxied service lane from guest-a through host to guest-b.
    let proxy_conn = guest_a_connection
        .open_lane_handle(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            vox_types::metadata().str("vox-service", "Echo").build(),
        )
        .await
        .expect("open proxy connection");

    let proxy_conn_id = proxy_conn.connection_id();
    let mut proxy_driver = Driver::new(proxy_conn, ());
    let proxy_caller = vox::Caller::new(proxy_driver.caller());
    tokio::spawn(async move { proxy_driver.run().await });

    let client = EchoClient::new(proxy_caller);
    let result = client.echo(777).await.expect("proxied echo");
    assert_eq!(result, 777);

    guest_a_connection
        .close_lane(proxy_conn_id, Default::default())
        .await
        .expect("close proxy connection");

    guest_b_task.abort();
    host_for_a_task.abort();
}
