use eyre::{Result, eyre};
use vox::{ConnectionHandle, ConnectionSettings, Metadata, Parity};

#[vox::service]
trait MathText {
    async fn add(&self, a: i32, b: i32) -> Result<i32, String>;
    async fn reverse(&self, value: String) -> Result<String, String>;
}

#[derive(Clone, Copy)]
struct UpstreamMathText;

impl MathText for UpstreamMathText {
    async fn add(&self, a: i32, b: i32) -> Result<i32, String> {
        Ok(a + b)
    }

    async fn reverse(&self, value: String) -> Result<String, String> {
        Ok(value.chars().rev().collect())
    }
}

fn upstream_acceptor(
    request: &vox::LaneRequest,
    connection: vox::PendingLane,
) -> Result<(), Metadata> {
    match request.service() {
        "MathText" => {
            connection.handle_with(MathTextDispatcher::new(UpstreamMathText));
            Ok(())
        }
        _ => Err(vox::metadata().str("error", "unknown service").build()),
    }
}

#[derive(Clone)]
struct ProxyAcceptor {
    upstream_connection: ConnectionHandle,
}

impl vox::LaneAcceptor for ProxyAcceptor {
    fn accept(
        &self,
        request: &vox::LaneRequest,
        connection: vox::PendingLane,
    ) -> Result<(), Metadata> {
        match request.service() {
            "MathText" => {}
            _ => {
                return Err(vox::metadata().str("error", "unknown service").build());
            }
        }

        let upstream_connection = self.upstream_connection.clone();
        let incoming_handle = connection.into_handle();
        tokio::spawn(async move {
            println!("[host] guest-a opened proxy lane; opening upstream lane to guest-b");
            match upstream_connection
                .open_lane_handle(
                    ConnectionSettings {
                        parity: Parity::Odd,
                        max_concurrent_requests: 64,
                        initial_channel_credit: 16,
                    },
                    vox::metadata()
                        .str(vox::VOX_SERVICE_METADATA_KEY, "MathText")
                        .build(),
                )
                .await
            {
                Ok(upstream_conn) => {
                    println!("[host] upstream lane to guest-b is ready");
                    let _ = vox::proxy_lanes(incoming_handle, upstream_conn).await;
                }
                Err(err) => {
                    let msg = format!("failed to open upstream lane: {err:?}");
                    eprintln!("[host] {msg}");
                }
            }
        });
        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let (host_a_link, guest_a_link) = vox::memory_link_pair(64);
    let (host_b_link, guest_b_link) = vox::memory_link_pair(64);

    println!("[guest-b] starting connection");
    let guest_b_task = tokio::spawn(async move {
        let guest_b_root_guard = vox::acceptor_on_link(
            guest_b_link,
            ConnectionSettings {
                parity: Parity::Even,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
        )
        .await
        .expect("guest-b acceptor_on_link")
        .on_connection(vox::lane_acceptor_fn(upstream_acceptor))
        .establish_connection()
        .await
        .expect("guest-b establish");
        let _guest_b_connection = guest_b_root_guard;
        std::future::pending::<()>().await;
    });

    println!("[host] establishing connection to guest-b");
    let host_connection_to_b = vox::initiator_on_link(
        host_b_link,
        ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
            initial_channel_credit: 16,
        },
    )
    .await
    .map_err(|e| eyre!("host<->guest-b initiator_on_link failed: {e:?}"))?
    .establish_connection()
    .await
    .map_err(|e| eyre!("host<->guest-b establish failed: {e:?}"))?;
    println!("[host] host<->guest-b connection ready");

    println!("[host] starting connection for guest-a");
    let proxy_acceptor = ProxyAcceptor {
        upstream_connection: host_connection_to_b,
    };
    let host_for_a_task = tokio::spawn(async move {
        let host_root_for_a_guard = vox::acceptor_on_link(
            host_a_link,
            ConnectionSettings {
                parity: Parity::Even,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
        )
        .await
        .expect("host<->guest-a acceptor_on_link")
        .on_connection(proxy_acceptor)
        .establish_connection()
        .await
        .expect("host<->guest-a establish");
        let _host_connection_for_a = host_root_for_a_guard;
        std::future::pending::<()>().await;
    });

    println!("[guest-a] establishing connection to host");
    let guest_a_connection = vox::initiator_on_link(
        guest_a_link,
        ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
            initial_channel_credit: 16,
        },
    )
    .await
    .map_err(|e| eyre!("guest-a<->host initiator_on_link failed: {e:?}"))?
    .establish_connection()
    .await
    .map_err(|e| eyre!("guest-a<->host establish failed: {e:?}"))?;
    println!("[guest-a] connection ready");

    println!("[guest-a] opening proxy lane to host");
    let proxy_client: MathTextClient = guest_a_connection
        .open_lane_with_settings(ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
            initial_channel_credit: 16,
        })
        .await
        .map_err(|e| eyre!("guest-a open proxy lane failed: {e:?}"))?;

    println!("[guest-a] calling add via host proxy to guest-b");
    let added = proxy_client
        .add(20, 22)
        .await
        .map_err(|e| eyre!("proxy add failed: {e:?}"))?;
    println!("[guest-a] add(20, 22) -> {added}");
    assert_eq!(added, 42);

    println!("[guest-a] calling reverse via host proxy to guest-b");
    let reversed = proxy_client
        .reverse("stressed".to_string())
        .await
        .map_err(|e| eyre!("proxy reverse failed: {e:?}"))?;
    println!("[guest-a] reverse(\"stressed\") -> {reversed}");
    assert_eq!(reversed, "desserts");

    guest_b_task.abort();
    host_for_a_task.abort();
    println!("[demo] memory_proxying: complete");
    Ok(())
}
