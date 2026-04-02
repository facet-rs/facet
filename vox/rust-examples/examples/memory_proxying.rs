use eyre::{Result, eyre};
use vox::{ConnectionSettings, Metadata, MetadataEntry, Parity, SessionHandle};

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
    request: &vox::ConnectionRequest,
    connection: vox::PendingConnection,
) -> Result<(), Metadata<'static>> {
    match request.service() {
        "Noop" => {
            connection.handle_with(());
            Ok(())
        }
        "MathText" => {
            connection.handle_with(MathTextDispatcher::new(UpstreamMathText));
            Ok(())
        }
        _ => Err(vec![MetadataEntry::str("error", "unknown service")]),
    }
}

#[derive(Clone)]
struct ProxyAcceptor {
    upstream_session: SessionHandle,
}

impl vox::ConnectionAcceptor for ProxyAcceptor {
    fn accept(
        &self,
        request: &vox::ConnectionRequest,
        connection: vox::PendingConnection,
    ) -> Result<(), Metadata<'static>> {
        match request.service() {
            "Noop" => {
                connection.handle_with(());
                return Ok(());
            }
            "MathText" => {}
            _ => {
                return Err(vec![MetadataEntry::str("error", "unknown service")]);
            }
        }

        let upstream_session = self.upstream_session.clone();
        let incoming_handle = connection.into_handle();
        tokio::spawn(async move {
            println!("[host] guest-a opened proxy vconn; opening upstream vconn to guest-b");
            match upstream_session
                .open_connection(
                    ConnectionSettings {
                        parity: Parity::Odd,
                        max_concurrent_requests: 64,
                    },
                    vec![MetadataEntry::str(
                        vox::VOX_SERVICE_METADATA_KEY,
                        "MathText",
                    )],
                )
                .await
            {
                Ok(upstream_conn) => {
                    println!("[host] upstream vconn to guest-b is ready");
                    let _ = vox::proxy_connections(incoming_handle, upstream_conn).await;
                }
                Err(err) => {
                    let msg = format!("failed to open upstream vconn: {err:?}");
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

    println!("[guest-b] starting root session");
    let guest_b_task = tokio::spawn(async move {
        let guest_b_root_guard = vox::acceptor_on_link(
            guest_b_link,
            ConnectionSettings {
                parity: Parity::Even,
                max_concurrent_requests: 64,
            },
        )
        .await
        .expect("guest-b acceptor_on_link")
        .on_connection(vox::acceptor_fn(upstream_acceptor))
        .establish::<vox::NoopClient>()
        .await
        .expect("guest-b establish");
        let _guest_b_root_guard = guest_b_root_guard;
        std::future::pending::<()>().await;
    });

    println!("[host] establishing session to guest-b");
    let _host_root_to_b_guard = vox::initiator_on_link(
        host_b_link,
        ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
        },
    )
    .await
    .map_err(|e| eyre!("host<->guest-b initiator_on_link failed: {e:?}"))?
    .establish::<vox::NoopClient>()
    .await
    .map_err(|e| eyre!("host<->guest-b establish failed: {e:?}"))?;
    let upstream_session_handle = _host_root_to_b_guard.session.clone().unwrap();
    println!("[host] host<->guest-b root session ready");

    println!("[host] starting root session for guest-a");
    let proxy_acceptor = ProxyAcceptor {
        upstream_session: upstream_session_handle,
    };
    let host_for_a_task = tokio::spawn(async move {
        let host_root_for_a_guard = vox::acceptor_on_link(
            host_a_link,
            ConnectionSettings {
                parity: Parity::Even,
                max_concurrent_requests: 64,
            },
        )
        .await
        .expect("host<->guest-a acceptor_on_link")
        .on_connection(proxy_acceptor)
        .establish::<vox::NoopClient>()
        .await
        .expect("host<->guest-a establish");
        let _host_root_for_a_guard = host_root_for_a_guard;
        std::future::pending::<()>().await;
    });

    println!("[guest-a] establishing root session to host");
    let _guest_a_root_guard = vox::initiator_on_link(
        guest_a_link,
        ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
        },
    )
    .await
    .map_err(|e| eyre!("guest-a<->host initiator_on_link failed: {e:?}"))?
    .establish::<vox::NoopClient>()
    .await
    .map_err(|e| eyre!("guest-a<->host establish failed: {e:?}"))?;
    let guest_a_session_handle = _guest_a_root_guard.session.clone().unwrap();
    println!("[guest-a] root session ready");

    println!("[guest-a] opening proxy vconn to host");
    let proxy_client: MathTextClient = guest_a_session_handle
        .open(ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
        })
        .await
        .map_err(|e| eyre!("guest-a open proxy vconn failed: {e:?}"))?;

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
