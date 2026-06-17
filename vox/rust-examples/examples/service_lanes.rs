use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use eyre::{Result, WrapErr, eyre};
use vox::transport::tcp::StreamLink;
use vox::{ConnectionSettings, LaneRejectReason, LaneRejection, Parity};

#[vox::service]
trait CounterLab {
    async fn bump(&self) -> u32;
    async fn echo(&self, value: String) -> String;
    #[vox::context]
    async fn grant_scope(&self) -> String;
    #[vox::context]
    async fn authenticated_peer(&self) -> String;
}

#[vox::service]
trait StringLab {
    async fn shout(&self, value: String) -> String;
}

#[derive(Clone)]
struct CounterLabService {
    count: Arc<AtomicU32>,
}

impl CounterLabService {
    fn new() -> Self {
        Self {
            count: Arc::new(AtomicU32::new(0)),
        }
    }
}

impl CounterLab for CounterLabService {
    async fn bump(&self) -> u32 {
        self.count.fetch_add(1, Ordering::Relaxed) + 1
    }

    async fn echo(&self, value: String) -> String {
        format!("echo:{value}")
    }

    async fn grant_scope(&self, cx: &vox::RequestContext<'_>) -> String {
        use vox::MetadataExt;

        cx.authorization()
            .and_then(|authorization| {
                authorization
                    .lane_grant()
                    .metadata()
                    .meta_str("grant-scope")
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "none".to_owned())
    }

    async fn authenticated_peer(&self, cx: &vox::RequestContext<'_>) -> String {
        cx.authorization()
            .and_then(|authorization| {
                authorization
                    .peer_identity()
                    .bases()
                    .first()
                    .map(|basis| basis.redacted.clone())
            })
            .unwrap_or_else(|| "anonymous".to_owned())
    }
}

#[derive(Clone, Copy)]
struct StringLabService;

impl StringLab for StringLabService {
    async fn shout(&self, value: String) -> String {
        value.to_uppercase()
    }
}

fn require_local_dev_user(
    cx: vox::IdentityResolutionContext<'_>,
) -> std::result::Result<vox::PeerIdentity, vox::Decline> {
    use vox::MetadataExt;

    match cx.claims.meta_str("-#authorization") {
        Some("Bearer local-dev") => Ok(vox::PeerIdentity::from_basis(vox::IdentityBasis::new(
            vox::PeerIdentityForm::ApplicationUser,
            vox::IdentityBasisProvenance::VerifiedClaimBacked,
            "local-dev-user",
        ))),
        _ => Err(vox::Decline::with_metadata(
            vox::EstablishmentRejectReason::Unauthenticated,
            vox::metadata()
                .str("hint", "send -#authorization metadata")
                .build(),
        )),
    }
}

fn authenticated_peer_label<'a>(
    request: &vox::LaneRequest<'a>,
) -> std::result::Result<&'a str, LaneRejection> {
    if request.peer_identity().form() != vox::PeerIdentityForm::ApplicationUser {
        return Err(LaneRejection::with_message(
            LaneRejectReason::PolicyRejected,
            "connection identity is not an authenticated application user",
        ));
    }

    request
        .peer_identity()
        .bases()
        .first()
        .map(|basis| basis.redacted.as_str())
        .ok_or_else(|| {
            LaneRejection::with_message(
                LaneRejectReason::PolicyRejected,
                "authenticated identity has no redacted basis",
            )
        })
}

fn lab_acceptor(
    request: &vox::LaneRequest,
    connection: vox::PendingLane,
) -> std::result::Result<(), LaneRejection> {
    let authenticated_peer = authenticated_peer_label(request)?;

    match request.service() {
        "CounterLab" => {
            let grant = vox::LaneGrant::from_metadata(
                vox::metadata()
                    .str("tenant", "lab")
                    .str("grant-scope", "counter:read-write")
                    .str("authenticated-peer", authenticated_peer)
                    .build(),
            );
            connection
                .with_grant(grant)
                .handle_with(CounterLabDispatcher::new(CounterLabService::new()));
            Ok(())
        }
        "StringLab" => {
            let grant = vox::LaneGrant::from_metadata(
                vox::metadata()
                    .str("tenant", "lab")
                    .str("grant-scope", "string:read-write")
                    .str("authenticated-peer", authenticated_peer)
                    .build(),
            );
            connection
                .with_grant(grant)
                .handle_with(StringLabDispatcher::new(StringLabService));
            Ok(())
        }
        _ => Err(LaneRejection::with_message(
            LaneRejectReason::UnknownService,
            "unknown service",
        )),
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    println!("[demo] binding TCP listener");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .wrap_err("binding TCP listener")?;
    let addr = listener.local_addr().wrap_err("reading listener addr")?;
    println!("[demo] listening on {addr}");
    let (server_ready_tx, server_ready_rx) =
        tokio::sync::oneshot::channel::<vox::ConnectionHandle>();

    let server_task = tokio::spawn(async move {
        let mut server_ready_tx = Some(server_ready_tx);

        loop {
            println!("[server] waiting for client");
            let (socket, _) = listener.accept().await.expect("accept");
            println!("[server] client connected; establishing connection");
            let result = vox::acceptor_on(StreamLink::tcp(socket))
                .identity_resolver(vox::identity_resolver_fn(require_local_dev_user))
                .on_lane(vox::lane_acceptor_fn(lab_acceptor))
                .establish_connection()
                .await;

            match result {
                Ok(server_connection) => {
                    if let Some(tx) = server_ready_tx.take() {
                        let _ = tx.send(server_connection.clone());
                    }
                    server_connection.closed().await;
                    break;
                }
                Err(vox::ConnectionError::EstablishmentRejected(decline))
                    if decline.reason == vox::EstablishmentRejectReason::Unauthenticated =>
                {
                    println!(
                        "[server] declined unauthenticated connection; waiting for another client"
                    );
                }
                Err(error) => panic!("server establish failed: {error:?}"),
            }
        }
    });

    println!("[client] connecting without auth metadata; expecting Decline");
    let socket = tokio::net::TcpStream::connect(addr)
        .await
        .wrap_err("connecting unauthenticated client socket")?;
    let rejected = vox::initiator_on(StreamLink::tcp(socket))
        .establish_connection()
        .await;
    match rejected {
        Err(vox::ConnectionError::EstablishmentRejected(decline)) => {
            assert_eq!(
                decline.reason,
                vox::EstablishmentRejectReason::Unauthenticated
            );
            println!("[client] missing auth was declined as unauthenticated");
        }
        Ok(connection) => {
            let _ = connection.shutdown();
            return Err(eyre!("unauthenticated connection unexpectedly succeeded"));
        }
        Err(error) => return Err(eyre!("expected Decline, got {error:?}")),
    }

    println!("[client] connecting with early auth metadata");
    let socket = tokio::net::TcpStream::connect(addr)
        .await
        .wrap_err("connecting client socket")?;
    let connection_handle = vox::initiator_on(StreamLink::tcp(socket))
        .metadata(
            vox::metadata()
                .str("-#authorization", "Bearer local-dev")
                .build(),
        )
        .establish_connection()
        .await
        .map_err(|e| eyre!("failed to establish initiator connection: {e:?}"))?;
    println!("[client] connection established");
    let server_connection = server_ready_rx
        .await
        .map_err(|_| eyre!("server task ended before signaling readiness"))?;
    assert_eq!(
        server_connection.peer_identity().form(),
        vox::PeerIdentityForm::ApplicationUser
    );
    println!("[server] peer identity resolved as ApplicationUser");

    let settings = ConnectionSettings {
        parity: Parity::Odd,
        max_concurrent_requests: 64,
        initial_channel_credit: 16,
    };

    println!("[client] opening counter lane");
    let counter_client: CounterLabClient = connection_handle
        .open_lane_with_settings(settings.clone())
        .await
        .map_err(|e| eyre!("open(CounterLab) failed: {e:?}"))?;

    println!("[client] opening string lane");
    let string_client: StringLabClient = connection_handle
        .open_lane_with_settings(settings)
        .await
        .map_err(|e| eyre!("open(StringLab) failed: {e:?}"))?;

    println!("[client] calling CounterLab::bump twice");
    assert_eq!(
        counter_client
            .bump()
            .await
            .map_err(|e| eyre!("counter_client.bump #1 failed: {e:?}"))?,
        1
    );
    assert_eq!(
        counter_client
            .bump()
            .await
            .map_err(|e| eyre!("counter_client.bump #2 failed: {e:?}"))?,
        2
    );
    println!("[client] CounterLab::bump -> 1, 2");

    println!("[client] calling CounterLab::echo");
    assert_eq!(
        counter_client
            .echo("alpha".to_string())
            .await
            .map_err(|e| eyre!("counter_client.echo failed: {e:?}"))?,
        "echo:alpha"
    );
    println!("[client] CounterLab::echo -> echo:alpha");

    println!("[client] calling CounterLab::grant_scope");
    assert_eq!(
        counter_client
            .grant_scope()
            .await
            .map_err(|e| eyre!("counter_client.grant_scope failed: {e:?}"))?,
        "counter:read-write"
    );
    println!("[client] CounterLab::grant_scope -> counter:read-write");

    println!("[client] calling CounterLab::authenticated_peer");
    assert_eq!(
        counter_client
            .authenticated_peer()
            .await
            .map_err(|e| eyre!("counter_client.authenticated_peer failed: {e:?}"))?,
        "local-dev-user"
    );
    println!("[client] CounterLab::authenticated_peer -> local-dev-user");

    println!("[client] calling StringLab::shout");
    assert_eq!(
        string_client
            .shout("beta".to_string())
            .await
            .map_err(|e| eyre!("string_client.shout failed: {e:?}"))?,
        "BETA"
    );
    println!("[client] StringLab::shout -> BETA");

    println!("[client] shutting down connection");
    connection_handle.shutdown().expect("connection shutdown");
    server_connection
        .shutdown()
        .expect("server connection shutdown");
    server_task.await.wrap_err("joining server_task")?;
    println!("[demo] service_lanes: complete");

    Ok(())
}
