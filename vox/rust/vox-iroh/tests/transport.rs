use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use iroh::{
    Endpoint, EndpointAddr, EndpointId, RelayMap, RelayMode, endpoint::presets, tls::CaTlsConfig,
};
use vox::{
    ConnectionError, Decline, DriverEvent, EstablishmentEvent, EstablishmentOutcome,
    EstablishmentPhase, EstablishmentRejectReason, IdentityBasis, IdentityBasisProvenance,
    IdentityResolutionContext, LaneAcceptor, LaneRejection, LaneRequest, LinkSource,
    PeerEvidenceItem, PeerIdentity, PeerIdentityForm, PendingLane, PublicKeyAlgorithm, RpcOutcome,
    VoxObserver, identity_resolver_fn,
};
use vox_iroh::{ALPN, IrohLinkSource, IrohListener};

#[vox::service]
trait Echo {
    async fn echo(&self, value: String) -> String;
}

#[derive(Clone)]
struct EchoService;

impl Echo for EchoService {
    async fn echo(&self, value: String) -> String {
        value
    }
}

#[derive(Clone, Default)]
struct RecordingObserver {
    public_key_identities: Arc<AtomicUsize>,
    rejected_handshakes: Arc<AtomicUsize>,
    successful_requests: Arc<AtomicUsize>,
}

impl VoxObserver for RecordingObserver {
    fn establishment_event(&self, event: EstablishmentEvent) {
        if matches!(
            event,
            EstablishmentEvent::Finished {
                context: vox::EstablishmentContext {
                    phase: EstablishmentPhase::IdentityResolution,
                    ..
                },
                outcome: EstablishmentOutcome::Ok,
                details,
                ..
            } if details.identity_form == Some(PeerIdentityForm::PublicKeyBacked)
        ) {
            self.public_key_identities.fetch_add(1, Ordering::SeqCst);
        }
        if matches!(
            event,
            EstablishmentEvent::Finished {
                context: vox::EstablishmentContext {
                    phase: EstablishmentPhase::ConnectionHandshake,
                    ..
                },
                outcome: EstablishmentOutcome::Rejected,
                ..
            }
        ) {
            self.rejected_handshakes.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn driver_event(&self, event: DriverEvent) {
        if matches!(
            event,
            DriverEvent::RequestFinished {
                outcome: RpcOutcome::Ok,
                ..
            }
        ) {
            self.successful_requests.fetch_add(1, Ordering::SeqCst);
        }
    }
}

struct CountingAcceptor<A> {
    inner: A,
    accepted: Arc<AtomicUsize>,
}

impl<A> CountingAcceptor<A> {
    fn new(inner: A, accepted: Arc<AtomicUsize>) -> Self {
        Self { inner, accepted }
    }
}

impl<A: LaneAcceptor> LaneAcceptor for CountingAcceptor<A> {
    fn accept(
        &self,
        request: &LaneRequest<'_>,
        connection: PendingLane,
    ) -> Result<(), LaneRejection> {
        self.accepted.fetch_add(1, Ordering::SeqCst);
        self.inner.accept(request, connection)
    }
}

fn require_endpoint(
    expected: EndpointId,
    resolutions: Arc<AtomicUsize>,
) -> impl vox::IdentityResolver {
    identity_resolver_fn(move |context: IdentityResolutionContext<'_>| {
        let key = context.evidence.items().iter().find_map(|item| match item {
            PeerEvidenceItem::PublicKey {
                algorithm: PublicKeyAlgorithm::Ed25519,
                bytes,
            } => Some(bytes),
            _ => None,
        });

        if key == Some(expected.as_bytes()) {
            resolutions.fetch_add(1, Ordering::SeqCst);
            Ok(PeerIdentity::from_basis(IdentityBasis::new(
                PeerIdentityForm::PublicKeyBacked,
                IdentityBasisProvenance::EvidenceBacked,
                format!("iroh:{}", expected.fmt_short()),
            )))
        } else {
            Err(Decline::new(EstablishmentRejectReason::Unauthenticated))
        }
    })
}

async fn exercise_authenticated_echo(
    server_endpoint: Endpoint,
    client_endpoint: Endpoint,
    server_addr: EndpointAddr,
) {
    let server_id = server_endpoint.id();
    let client_id = client_endpoint.id();
    let server_resolutions = Arc::new(AtomicUsize::new(0));
    let client_resolutions = Arc::new(AtomicUsize::new(0));
    let accepted_lanes = Arc::new(AtomicUsize::new(0));
    let observer = RecordingObserver::default();

    let server = tokio::spawn(
        vox::serve_listener(
            IrohListener::new(server_endpoint.clone()),
            CountingAcceptor::new(EchoDispatcher::new(EchoService), accepted_lanes.clone()),
        )
        .identity_resolver(require_endpoint(client_id, server_resolutions.clone()))
        .observer(observer.clone())
        .run(),
    );

    let client = vox::initiator(IrohLinkSource::new(client_endpoint.clone(), server_addr))
        .identity_resolver(require_endpoint(server_id, client_resolutions.clone()))
        .observer(observer.clone())
        .establish::<EchoClient>()
        .await
        .expect("establish authenticated Vox connection");

    let value = "authenticated over Iroh".to_owned();
    assert_eq!(client.echo(value.clone()).await.expect("echo"), value);
    assert_eq!(server_resolutions.load(Ordering::SeqCst), 1);
    assert_eq!(client_resolutions.load(Ordering::SeqCst), 1);
    assert_eq!(accepted_lanes.load(Ordering::SeqCst), 1);
    assert_eq!(observer.public_key_identities.load(Ordering::SeqCst), 2);
    assert_eq!(observer.successful_requests.load(Ordering::SeqCst), 1);

    drop(client);
    server.abort();
}

// r[verify transport.iroh.link]
// r[verify transport.iroh.alpn]
// r[verify transport.iroh.evidence]
// r[verify transport.iroh.observability]
#[tokio::test]
async fn direct_path_runs_typed_vox_call_with_endpoint_identity() {
    let server_endpoint = Endpoint::builder(presets::Minimal)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .expect("bind server endpoint");
    let client_endpoint = Endpoint::builder(presets::Minimal)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .expect("bind client endpoint");
    let server_id = server_endpoint.id();
    let server_addr = server_endpoint.addr();

    exercise_authenticated_echo(server_endpoint, client_endpoint.clone(), server_addr).await;

    let remote = client_endpoint
        .remote_info(server_id)
        .await
        .expect("direct remote info");
    assert!(remote.addrs().any(|addr| addr.addr().is_ip()));
    client_endpoint.close().await;
}

// r[verify transport.iroh.alpn]
#[tokio::test]
async fn incompatible_alpn_never_enters_vox_transport() {
    let server_endpoint = Endpoint::builder(presets::Minimal)
        .alpns(vec![b"not-vox/1".to_vec()])
        .bind()
        .await
        .expect("bind incompatible server endpoint");
    let client_endpoint = Endpoint::builder(presets::Minimal)
        .bind()
        .await
        .expect("bind client endpoint");
    let server_addr = server_endpoint.addr();
    let accept_endpoint = server_endpoint.clone();
    let accept = tokio::spawn(async move {
        let incoming = accept_endpoint.accept().await.expect("incoming connection");
        incoming.accept().expect("accept incoming connection").await
    });

    let mut source = IrohLinkSource::new(client_endpoint.clone(), server_addr);
    let result = tokio::time::timeout(Duration::from_secs(5), source.next_link())
        .await
        .expect("ALPN rejection did not arrive");
    assert!(result.is_err());

    accept.abort();
    client_endpoint.close().await;
    server_endpoint.close().await;
}

// r[verify transport.iroh.path-equivalence]
// r[verify transport.iroh.observability]
#[tokio::test]
async fn forced_relay_path_preserves_typed_call_and_endpoint_identity() {
    let (relay_map, relay_url, _relay_server) = iroh::test_utils::run_relay_server()
        .await
        .expect("start relay server");
    let server_endpoint = relay_endpoint(relay_map.clone()).await;
    let client_endpoint = relay_endpoint(relay_map).await;
    server_endpoint.online().await;
    client_endpoint.online().await;

    let server_id = server_endpoint.id();
    let server_addr = EndpointAddr::new(server_id).with_relay_url(relay_url);
    exercise_authenticated_echo(server_endpoint, client_endpoint.clone(), server_addr).await;

    let remote = client_endpoint
        .remote_info(server_id)
        .await
        .expect("relay remote info");
    assert!(remote.addrs().any(|addr| addr.addr().is_relay()));
    assert!(!remote.addrs().any(|addr| addr.addr().is_ip()));
    client_endpoint.close().await;
}

// r[verify transport.iroh.evidence]
#[tokio::test]
async fn unknown_endpoint_is_rejected_before_service_lane_open() {
    let server_endpoint = Endpoint::builder(presets::Minimal)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .expect("bind server endpoint");
    let enrolled_endpoint = Endpoint::builder(presets::Minimal)
        .bind()
        .await
        .expect("bind enrolled endpoint");
    let unknown_endpoint = Endpoint::builder(presets::Minimal)
        .bind()
        .await
        .expect("bind unknown endpoint");
    let accepted_lanes = Arc::new(AtomicUsize::new(0));

    let server = tokio::spawn(
        vox::serve_listener(
            IrohListener::new(server_endpoint.clone()),
            CountingAcceptor::new(EchoDispatcher::new(EchoService), accepted_lanes.clone()),
        )
        .identity_resolver(require_endpoint(
            enrolled_endpoint.id(),
            Arc::new(AtomicUsize::new(0)),
        ))
        .run(),
    );

    let result = tokio::time::timeout(
        Duration::from_secs(5),
        vox::initiator(IrohLinkSource::new(
            unknown_endpoint.clone(),
            server_endpoint.addr(),
        ))
        .establish::<EchoClient>(),
    )
    .await
    .expect("rejection did not arrive");

    match result {
        Err(ConnectionError::EstablishmentRejected(Decline {
            reason: EstablishmentRejectReason::Unauthenticated,
            ..
        })) => {}
        Ok(_) => panic!("unknown endpoint unexpectedly established a service lane"),
        Err(error) => {
            panic!("expected unauthenticated establishment rejection, got {error:?}")
        }
    }
    assert_eq!(accepted_lanes.load(Ordering::SeqCst), 0);

    server.abort();
    unknown_endpoint.close().await;
    enrolled_endpoint.close().await;
    server_endpoint.close().await;
}

// r[verify transport.iroh.evidence]
// r[verify transport.iroh.close]
#[tokio::test]
async fn client_identity_rejection_reaches_server_before_connection_close() {
    let server_endpoint = Endpoint::builder(presets::Minimal)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .expect("bind server endpoint");
    let client_endpoint = Endpoint::builder(presets::Minimal)
        .bind()
        .await
        .expect("bind client endpoint");
    let expected_server = Endpoint::builder(presets::Minimal)
        .bind()
        .await
        .expect("bind expected server endpoint");
    let accepted_lanes = Arc::new(AtomicUsize::new(0));
    let observer = RecordingObserver::default();

    let server = tokio::spawn(
        vox::serve_listener(
            IrohListener::new(server_endpoint.clone()),
            CountingAcceptor::new(EchoDispatcher::new(EchoService), accepted_lanes.clone()),
        )
        .identity_resolver(require_endpoint(
            client_endpoint.id(),
            Arc::new(AtomicUsize::new(0)),
        ))
        .observer(observer.clone())
        .run(),
    );

    let result = vox::initiator(IrohLinkSource::new(
        client_endpoint.clone(),
        server_endpoint.addr(),
    ))
    .identity_resolver(require_endpoint(
        expected_server.id(),
        Arc::new(AtomicUsize::new(0)),
    ))
    .establish::<EchoClient>()
    .await;

    assert!(matches!(
        result,
        Err(ConnectionError::EstablishmentRejected(Decline {
            reason: EstablishmentRejectReason::Unauthenticated,
            ..
        }))
    ));
    tokio::time::timeout(Duration::from_secs(5), async {
        while observer.rejected_handshakes.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("server did not receive the client's typed identity rejection");
    assert_eq!(accepted_lanes.load(Ordering::SeqCst), 0);

    server.abort();
    expected_server.close().await;
    client_endpoint.close().await;
    server_endpoint.close().await;
}

async fn relay_endpoint(relay_map: RelayMap) -> Endpoint {
    Endpoint::builder(presets::Minimal)
        .alpns(vec![ALPN.to_vec()])
        .relay_mode(RelayMode::Custom(relay_map))
        .ca_tls_config(CaTlsConfig::insecure_skip_verify())
        .clear_ip_transports()
        .bind()
        .await
        .expect("bind relay-only endpoint")
}
