//! Tests for proxy_connections — transparent call forwarding between sessions.

use vox::{
    AcceptedConnection, ConnectionAcceptor, ConnectionSettings, ConnectionSetup, Driver, Metadata,
    Parity, SessionHandle, memory_link_pair, proxy_connections,
};

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

struct UpstreamEchoAcceptor;

impl ConnectionAcceptor for UpstreamEchoAcceptor {
    fn accept(
        &self,
        _conn_id: vox::ConnectionId,
        peer_settings: &ConnectionSettings,
        _metadata: &[vox::MetadataEntry],
    ) -> Result<AcceptedConnection, Metadata<'static>> {
        Ok(AcceptedConnection {
            settings: ConnectionSettings {
                parity: peer_settings.parity.other(),
                max_concurrent_requests: 64,
            },
            metadata: vec![],
            setup: ConnectionSetup::Handler(Box::new(EchoDispatcher::new(EchoService))),
        })
    }
}

struct ProxyAcceptor {
    upstream_session: SessionHandle,
}

impl ConnectionAcceptor for ProxyAcceptor {
    fn accept(
        &self,
        _conn_id: vox::ConnectionId,
        peer_settings: &ConnectionSettings,
        _metadata: &[vox::MetadataEntry],
    ) -> Result<AcceptedConnection, Metadata<'static>> {
        let upstream_session = self.upstream_session.clone();
        Ok(AcceptedConnection {
            settings: ConnectionSettings {
                parity: peer_settings.parity.other(),
                max_concurrent_requests: 64,
            },
            metadata: vec![],
            setup: ConnectionSetup::Setup(Box::new(move |incoming| {
                tokio::spawn(async move {
                    let upstream = upstream_session
                        .open_connection(
                            ConnectionSettings {
                                parity: Parity::Odd,
                                max_concurrent_requests: 64,
                            },
                            vec![],
                        )
                        .await
                        .expect("open upstream connection");
                    proxy_connections(incoming, upstream).await;
                });
            })),
        })
    }
}

#[tokio::test]
async fn proxy_connections_forwards_calls() {
    // guest-a <-> host <-> guest-b
    // guest-a opens a vconn through host, which proxies to guest-b.
    let (host_b_link, guest_b_link) = memory_link_pair(16);
    let (host_a_link, guest_a_link) = memory_link_pair(16);

    // guest-b: accepts virtual connections with EchoService
    let guest_b_task = tokio::spawn(async move {
        let guard = vox::acceptor_on(guest_b_link)
            .on_connection(UpstreamEchoAcceptor)
            .establish::<vox::NoopClient>(())
            .await
            .expect("guest-b establish");
        let _guard = guard;
        std::future::pending::<()>().await
    });

    // host <-> guest-b root session
    let _host_to_b = vox::initiator_on(host_b_link, vox::TransportMode::Bare)
        .establish::<vox::NoopClient>(())
        .await
        .expect("host<->guest-b establish");
    let host_to_b_session = _host_to_b.session.clone().unwrap();

    // host: accepts connections from guest-a and proxies to guest-b
    let host_for_a_task = tokio::spawn(async move {
        let guard = vox::acceptor_on(host_a_link)
            .on_connection(ProxyAcceptor {
                upstream_session: host_to_b_session,
            })
            .establish::<vox::NoopClient>(())
            .await
            .expect("host<->guest-a establish");
        let _guard = guard;
        std::future::pending::<()>().await
    });

    // guest-a <-> host root session
    let _guest_a_root = vox::initiator_on(guest_a_link, vox::TransportMode::Bare)
        .establish::<vox::NoopClient>(())
        .await
        .expect("guest-a establish");
    let guest_a_session = _guest_a_root.session.clone().unwrap();

    // Open a proxied vconn from guest-a through host to guest-b.
    let proxy_conn = guest_a_session
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            vec![],
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

    guest_a_session
        .close_connection(proxy_conn_id, vec![])
        .await
        .expect("close proxy connection");

    guest_b_task.abort();
    host_for_a_task.abort();
}
