use std::io::{ErrorKind, Read as _, Write as _};
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use tempfile::TempDir;
use vix::ratchet::{RunError, prepare_source};
use vix::runtime::{
    CanonicalBlobPersistence, ClaimHistory, FixtureStore, FramedNode, MachineCause,
    ObserveCoordinate, ObservedClaim, OriginAdapter, PrimitiveMachineError, PrimitiveServices,
    RuntimeFault, ValueBodyCandidate, ValueId, ValuePersistence,
};
use vix::vir::{ExternKind, Type};
use vix_fetch::HttpBlobOriginAdapter;

const FETCH_AND_EXTRACT: &str = r#"
#[test]
fn pinned_fetch_origin_returns_blob_and_separate_extraction() -> Stream<Check> {
    let blob = fetch(fixture_registry().url("case.crate"));
    let tree = untar(blob);
    yield expect((tree / "Cargo.toml").text().contains("name = \"tokio\""));
    yield fetched(1);
}
"#;

const FETCH_ONLY: &str = r#"
#[test]
fn pinned_fetch_provider_hit_precedes_origin() -> Stream<Check> {
    let blob = fetch(fixture_registry().url("case.crate"));
    yield expect_eq(blob.len(), 4096);
    yield fetched(0);
}
"#;

const FETCH_ONE_FROM_ORIGIN: &str = r#"
#[test]
fn pinned_fetch_falls_through_to_verified_origin() -> Stream<Check> {
    let blob = fetch(fixture_registry().url("case.crate"));
    yield expect_eq(blob.len(), 4096);
    yield fetched(1);
}
"#;

const FETCH_STORE_THEN_REDEMAND: &str = r#"
fn after_first(first: Blob) -> String {
    "second.crate/" + first.len().to_string()
}

#[test]
fn pinned_fetch_local_store_hit_never_contacts_provider_or_origin() -> Stream<Check> {
    let first = fetch(fixture_registry().url("first.crate"));
    yield expect_eq(first.len(), 4096);
    let second = fetch(fixture_registry().url(after_first(first)));
    yield expect_eq(second.len(), 4096);
    yield fetched(1);
}
"#;

const FETCH_MUST_FAIL: &str = r#"
#[test]
fn pinned_fetch_must_fail_before_publishing() -> Stream<Check> {
    let blob = fetch(fixture_registry().url("case.crate"));
    yield expect_eq(blob.len(), 4096);
}
"#;

const FETCH_STORE_THEN_UPSTREAM_CONTRADICTION: &str = r#"
fn after_first(first: Blob) -> String {
    "second.crate/" + first.len().to_string()
}

#[test]
fn pinned_fetch_store_checks_upstream_digest() -> Stream<Check> {
    let first = fetch(fixture_registry().url("first.crate"));
    yield expect_eq(first.len(), 4096);
    let second = fetch(fixture_registry().url(after_first(first)));
    yield expect_eq(second.len(), 4096);
}
"#;

struct BlobServer {
    address: SocketAddr,
    requests: Arc<AtomicUsize>,
    transfers: Arc<AtomicUsize>,
    targets: Arc<Mutex<Vec<String>>>,
    shutdown: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl BlobServer {
    fn start(body: Vec<u8>) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind Blob fixture server");
        listener
            .set_nonblocking(true)
            .expect("make Blob fixture server nonblocking");
        let address = listener.local_addr().expect("read Blob fixture address");
        let requests = Arc::new(AtomicUsize::new(0));
        let transfers = Arc::new(AtomicUsize::new(0));
        let targets = Arc::new(Mutex::new(Vec::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_requests = requests.clone();
        let worker_transfers = transfers.clone();
        let worker_targets = targets.clone();
        let worker_shutdown = shutdown.clone();
        let worker = std::thread::spawn(move || {
            while !worker_shutdown.load(Ordering::Acquire) {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        std::thread::yield_now();
                        continue;
                    }
                    Err(_) => return,
                };
                stream
                    .set_nonblocking(false)
                    .expect("make accepted Blob fixture stream blocking");
                let mut request = [0u8; 4096];
                let read = stream.read(&mut request).unwrap_or(0);
                worker_requests.fetch_add(1, Ordering::AcqRel);
                let path = core::str::from_utf8(&request[..read])
                    .ok()
                    .and_then(|request| request.lines().next())
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or_default()
                    .to_owned();
                worker_targets
                    .lock()
                    .expect("Blob fixture target mutex poisoned")
                    .push(path);
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                if stream.write_all(header.as_bytes()).is_ok() && stream.write_all(&body).is_ok() {
                    worker_transfers.fetch_add(1, Ordering::AcqRel);
                }
            }
        });
        Self {
            address,
            requests,
            transfers,
            targets,
            shutdown,
            worker: Some(worker),
        }
    }

    fn url(&self) -> String {
        self.url_for("/blob")
    }

    fn url_for(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }

    fn requests(&self) -> usize {
        self.requests.load(Ordering::Acquire)
    }

    fn transfers(&self) -> usize {
        self.transfers.load(Ordering::Acquire)
    }

    fn targets(&self) -> Vec<String> {
        self.targets
            .lock()
            .expect("Blob fixture target mutex poisoned")
            .clone()
    }
}

impl Drop for BlobServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.join().expect("join Blob fixture server");
        }
    }
}

fn archive_bytes() -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../vix/tests/fixtures/registry/tokio-1.52.3.crate"),
    )
    .expect("read pinned archive fixture")
}

fn fixture_store(
    temp: &TempDir,
    url: &str,
    blob: &vix::runtime::ValueId,
    upstream_sha256: &str,
) -> FixtureStore {
    fixture_store_with_manifest(
        temp,
        &format!(
            "case.crate {url} {} {upstream_sha256}\n",
            blob.content.hex()
        ),
    )
}

fn fixture_store_with_manifest(temp: &TempDir, manifest: &str) -> FixtureStore {
    let registry = temp.path().join("registry");
    std::fs::create_dir_all(&registry).expect("create fixture registry");
    std::fs::write(registry.join("manifest"), manifest).expect("write fixture registry manifest");
    FixtureStore::with_root(temp.path().to_path_buf())
}

fn blob_identity(bytes: &[u8]) -> vix::runtime::ValueId {
    FramedNode::leaf(Type::Extern(ExternKind::Blob).schema_ref(), bytes.to_vec()).identity()
}

fn primitive_machine_error(error: RunError) -> PrimitiveMachineError {
    let RunError::Machine(error) = error else {
        panic!("expected machine error, got {error:#?}");
    };
    let MachineCause::Runtime(RuntimeFault::PrimitiveMachine { error }) = error.cause else {
        panic!("expected typed primitive machine error, got {error:#?}");
    };
    error
}

#[derive(Default)]
struct RecordingMissPersistence {
    gets: AtomicUsize,
    puts: AtomicUsize,
    requested: Mutex<Vec<ValueId>>,
}

impl ValuePersistence for RecordingMissPersistence {
    fn get(&self, value: &ValueId) -> Result<Option<ValueBodyCandidate>, PrimitiveMachineError> {
        self.gets.fetch_add(1, Ordering::AcqRel);
        self.requested
            .lock()
            .expect("recording persistence mutex poisoned")
            .push(value.clone());
        Ok(None)
    }

    fn put(&self, _value: &ValueId, _bytes: &[u8]) -> Result<(), PrimitiveMachineError> {
        self.puts.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }
}

struct CorruptThenRecordingPersistence {
    candidate: ValueBodyCandidate,
    gets: AtomicUsize,
    admitted: Mutex<Vec<(ValueId, Vec<u8>)>>,
}

impl ValuePersistence for CorruptThenRecordingPersistence {
    fn get(&self, _value: &ValueId) -> Result<Option<ValueBodyCandidate>, PrimitiveMachineError> {
        self.gets.fetch_add(1, Ordering::AcqRel);
        Ok(Some(self.candidate.clone()))
    }

    fn put(&self, value: &ValueId, bytes: &[u8]) -> Result<(), PrimitiveMachineError> {
        self.admitted
            .lock()
            .expect("corrupt persistence admission mutex poisoned")
            .push((value.clone(), bytes.to_vec()));
        Ok(())
    }
}

#[test]
fn pinned_fetch_origin_returns_blob_and_separate_extraction() {
    let bytes = archive_bytes();
    let identity = blob_identity(&bytes);
    let upstream = vix::fetch::sha256_hex(&bytes);
    let server = BlobServer::start(bytes);
    let fixtures = TempDir::new().expect("create fixture root");
    let services = PrimitiveServices::default()
        .with_fixture_store(fixture_store(
            &fixtures,
            &server.url(),
            &identity,
            &upstream,
        ))
        .with_origin_adapter(Arc::new(HttpBlobOriginAdapter));

    let report = prepare_source(FETCH_AND_EXTRACT)
        .expect("prepare Vix fetch/extract source")
        .execute_with_primitive_services(services)
        .expect("execute Vix fetch/extract source");

    assert!(report.passed(), "fetch/extract report: {report:#?}");
    assert_eq!(server.transfers(), 2, "plain and chaos each transfer once");
    assert_eq!(server.requests(), 2, "plain and chaos each request once");
    for run in [&report.plain, &report.chaos] {
        assert_eq!(run.counters.primitive_invocations, 1, "{run:#?}");
        assert_eq!(run.counters.fetches_performed, 1, "{run:#?}");
    }
}

/// L2.2c production-path certificate: a registered-primitive (pinned fetch)
/// `HostCallYield` parks its frame off the recursive Rust stack in demand-owned
/// pending state, its completion crosses the one unified inbox, and the resumed
/// frame materializes the admitted Blob at its exact frame/PC state. The check
/// passes only when the fetched Blob is realized correctly on resume, so a wrong
/// frame/PC restore is caught by `report.passed()`. The observability counters
/// witness the off-stack park and the single inbox receive authority.
#[test]
fn pinned_fetch_yielded_frame_survives_off_stack_and_crosses_the_inbox() {
    let bytes = archive_bytes();
    let identity = blob_identity(&bytes);
    let upstream = vix::fetch::sha256_hex(&bytes);
    let server = BlobServer::start(bytes);
    let fixtures = TempDir::new().expect("create fixture root");
    let services = PrimitiveServices::default()
        .with_fixture_store(fixture_store(
            &fixtures,
            &server.url(),
            &identity,
            &upstream,
        ))
        .with_origin_adapter(Arc::new(HttpBlobOriginAdapter));

    // A fresh origin fetch: the primitive actually runs (one transfer), so its
    // frame genuinely yields, parks off-stack, and resumes through the inbox.
    let report = prepare_source(FETCH_ONE_FROM_ORIGIN)
        .expect("prepare Vix fetch source")
        .execute_with_primitive_services(services)
        .expect("execute Vix fetch source");

    // The resumed frame produced the correct realized Blob: exact frame/PC state.
    assert!(report.passed(), "fetch report: {report:#?}");
    // The plain run performs the fetch fresh, so its primitive frame yielded,
    // parked off the recursive Rust stack in demand-owned pending state, and
    // resumed only after its completion crossed the one unified inbox.
    let plain = &report.plain;
    assert_eq!(
        plain.counters.primitive_invocations, 1,
        "the fetch began exactly one registered-primitive demand: {plain:#?}"
    );
    assert!(
        plain.counters.peak_primitive_parked_frames >= 1,
        "a yielded primitive frame survived off the recursive stack in pending state: {plain:#?}"
    );
    assert!(
        plain.counters.completion_inbox_receipts >= 1,
        "the primitive completion crossed the unified completion inbox: {plain:#?}"
    );
    assert_eq!(
        plain.counters.stale_completions_ignored, 0,
        "a live single-waiter fetch has no late or duplicate completion: {plain:#?}"
    );
}

#[test]
fn pinned_fetch_provider_hit_precedes_origin() {
    let bytes = archive_bytes();
    let identity = blob_identity(&bytes);
    let upstream = vix::fetch::sha256_hex(&bytes);
    let server = BlobServer::start(bytes.clone());
    let fixtures = TempDir::new().expect("create fixture root");
    let persistence_root = TempDir::new().expect("create Blob persistence root");
    let persistence = Arc::new(CanonicalBlobPersistence::new(persistence_root.path()));
    persistence
        .put(&identity, &bytes)
        .expect("prepopulate canonical Blob persistence");
    let services = PrimitiveServices::default()
        .with_fixture_store(fixture_store(
            &fixtures,
            &server.url(),
            &identity,
            &upstream,
        ))
        .with_value_persistence(persistence)
        .with_origin_adapter(Arc::new(HttpBlobOriginAdapter));

    let report = prepare_source(FETCH_ONLY)
        .expect("prepare Vix provider-hit source")
        .execute_with_primitive_services(services)
        .expect("execute Vix provider-hit source");

    assert!(report.passed(), "provider-hit report: {report:#?}");
    assert_eq!(server.transfers(), 0, "provider hit must precede origin");
    assert_eq!(server.requests(), 0, "provider hit must not contact origin");
    for run in [&report.plain, &report.chaos] {
        assert_eq!(run.counters.primitive_invocations, 1, "{run:#?}");
        assert_eq!(run.counters.fetches_performed, 0, "{run:#?}");
    }
}

#[test]
fn pinned_fetch_local_store_hit_never_contacts_provider_or_origin() {
    let bytes = archive_bytes();
    let identity = blob_identity(&bytes);
    let upstream = vix::fetch::sha256_hex(&bytes);
    let server = BlobServer::start(bytes);
    let fixtures = TempDir::new().expect("create fixture root");
    let manifest = format!(
        "first.crate {} {} {upstream}\nsecond.crate/4096 {} {} {upstream}\n",
        server.url_for("/first"),
        identity.content.hex(),
        server.url_for("/must-not-be-contacted"),
        identity.content.hex(),
    );
    let persistence = Arc::new(RecordingMissPersistence::default());
    let services = PrimitiveServices::default()
        .with_fixture_store(fixture_store_with_manifest(&fixtures, &manifest))
        .with_value_persistence(persistence.clone())
        .with_origin_adapter(Arc::new(HttpBlobOriginAdapter));

    let report = prepare_source(FETCH_STORE_THEN_REDEMAND)
        .expect("prepare Vix Store-hit source")
        .execute_with_primitive_services(services)
        .expect("execute Vix Store-hit source");

    assert!(report.passed(), "Store-hit report: {report:#?}");
    assert_eq!(
        server.requests(),
        2,
        "only the first fetch in each lane transfers"
    );
    assert_eq!(
        server.transfers(),
        2,
        "only admitted origin bodies transfer"
    );
    assert_eq!(server.targets(), ["/first", "/first"]);
    assert_eq!(
        persistence.gets.load(Ordering::Acquire),
        2,
        "only the first fetch in each lane consults persistence"
    );
    assert_eq!(
        persistence.puts.load(Ordering::Acquire),
        2,
        "only origin admission is offered to persistence"
    );
    for run in [&report.plain, &report.chaos] {
        assert_eq!(run.counters.primitive_invocations, 2, "{run:#?}");
        assert_eq!(run.counters.fetches_performed, 1, "{run:#?}");
    }
}

#[test]
fn pinned_fetch_rejects_vix_identity_mismatch() {
    let bytes = archive_bytes();
    let observed = blob_identity(&bytes);
    let claimed = blob_identity(b"different Blob body");
    let upstream = vix::fetch::sha256_hex(&bytes);
    let server = BlobServer::start(bytes);
    let fixtures = TempDir::new().expect("create fixture root");
    let services = PrimitiveServices::default()
        .with_fixture_store(fixture_store(&fixtures, &server.url(), &claimed, &upstream))
        .with_origin_adapter(Arc::new(HttpBlobOriginAdapter));

    let error = prepare_source(FETCH_MUST_FAIL)
        .expect("prepare Vix identity-mismatch source")
        .execute_with_primitive_services(services)
        .expect_err("Vix identity mismatch must fail");

    assert_eq!(
        primitive_machine_error(error),
        PrimitiveMachineError::CorruptCandidate { source: observed }
    );
    assert_eq!(server.requests(), 1);
}

#[test]
fn pinned_fetch_rejects_upstream_digest_mismatch_at_every_tier() {
    let bytes = archive_bytes();
    let identity = blob_identity(&bytes);
    let wrong_upstream = "00".repeat(32);

    let store_server = BlobServer::start(bytes.clone());
    let store_fixtures = TempDir::new().expect("create Store-tier fixture root");
    let store_manifest = format!(
        "first.crate {} {}\nsecond.crate/4096 {} {} {wrong_upstream}\n",
        store_server.url_for("/first"),
        identity.content.hex(),
        store_server.url_for("/must-not-be-contacted"),
        identity.content.hex(),
    );
    let store_services = PrimitiveServices::default()
        .with_fixture_store(fixture_store_with_manifest(
            &store_fixtures,
            &store_manifest,
        ))
        .with_origin_adapter(Arc::new(HttpBlobOriginAdapter));
    let store_error = prepare_source(FETCH_STORE_THEN_UPSTREAM_CONTRADICTION)
        .expect("prepare Store-tier upstream source")
        .execute_with_primitive_services(store_services)
        .expect_err("Store-tier upstream mismatch must fail");
    assert!(matches!(
        primitive_machine_error(store_error),
        PrimitiveMachineError::PolicyRejected { .. }
    ));
    assert_eq!(store_server.targets(), ["/first"]);

    let provider_server = BlobServer::start(bytes.clone());
    let provider_fixtures = TempDir::new().expect("create provider-tier fixture root");
    let provider_root = TempDir::new().expect("create provider-tier persistence root");
    let provider = Arc::new(CanonicalBlobPersistence::new(provider_root.path()));
    provider
        .put(&identity, &bytes)
        .expect("prepopulate provider-tier Blob");
    let provider_services = PrimitiveServices::default()
        .with_fixture_store(fixture_store(
            &provider_fixtures,
            &provider_server.url(),
            &identity,
            &wrong_upstream,
        ))
        .with_value_persistence(provider)
        .with_origin_adapter(Arc::new(HttpBlobOriginAdapter));
    let provider_error = prepare_source(FETCH_MUST_FAIL)
        .expect("prepare provider-tier upstream source")
        .execute_with_primitive_services(provider_services)
        .expect_err("provider-tier upstream mismatch must fail");
    assert!(matches!(
        primitive_machine_error(provider_error),
        PrimitiveMachineError::PolicyRejected { .. }
    ));
    assert_eq!(provider_server.requests(), 0);

    let origin_server = BlobServer::start(bytes);
    let origin_fixtures = TempDir::new().expect("create origin-tier fixture root");
    let origin_services = PrimitiveServices::default()
        .with_fixture_store(fixture_store(
            &origin_fixtures,
            &origin_server.url(),
            &identity,
            &wrong_upstream,
        ))
        .with_origin_adapter(Arc::new(HttpBlobOriginAdapter));
    let origin_error = prepare_source(FETCH_MUST_FAIL)
        .expect("prepare origin-tier upstream source")
        .execute_with_primitive_services(origin_services)
        .expect_err("origin-tier upstream mismatch must fail");
    assert!(matches!(
        primitive_machine_error(origin_error),
        PrimitiveMachineError::PolicyRejected { .. }
    ));
    assert_eq!(origin_server.requests(), 1);
}

#[test]
fn pinned_fetch_rejects_corrupt_provider_then_admits_verified_origin() {
    let bytes = archive_bytes();
    let identity = blob_identity(&bytes);
    let upstream = vix::fetch::sha256_hex(&bytes);
    let provider = Arc::new(CorruptThenRecordingPersistence {
        candidate: ValueBodyCandidate {
            claimed: identity.clone(),
            bytes: b"corrupt provider body".to_vec(),
        },
        gets: AtomicUsize::new(0),
        admitted: Mutex::new(Vec::new()),
    });
    let server = BlobServer::start(bytes.clone());
    let fixtures = TempDir::new().expect("create corrupt-provider fixture root");
    let services = PrimitiveServices::default()
        .with_fixture_store(fixture_store(
            &fixtures,
            &server.url(),
            &identity,
            &upstream,
        ))
        .with_value_persistence(provider.clone())
        .with_origin_adapter(Arc::new(HttpBlobOriginAdapter));

    let report = prepare_source(FETCH_ONE_FROM_ORIGIN)
        .expect("prepare corrupt-provider source")
        .execute_with_primitive_services(services)
        .expect("corrupt provider must fall through to verified origin");

    assert!(report.passed(), "corrupt-provider report: {report:#?}");
    assert_eq!(provider.gets.load(Ordering::Acquire), 2);
    assert_eq!(server.requests(), 2);
    assert_eq!(server.transfers(), 2);
    assert_eq!(
        *provider
            .admitted
            .lock()
            .expect("corrupt persistence admission mutex poisoned"),
        [(identity.clone(), bytes.clone()), (identity, bytes),]
    );
    for run in [&report.plain, &report.chaos] {
        assert_eq!(run.counters.primitive_invocations, 1, "{run:#?}");
        assert_eq!(run.counters.fetches_performed, 1, "{run:#?}");
    }
}

#[derive(Default)]
struct RecordingClaimHistory {
    appended: Mutex<Vec<ObservedClaim>>,
}

impl RecordingClaimHistory {
    fn appended(&self) -> Vec<ObservedClaim> {
        self.appended
            .lock()
            .expect("claim history mutex poisoned")
            .clone()
    }
}

impl ClaimHistory for RecordingClaimHistory {
    fn head(
        &self,
        coordinate: &ObserveCoordinate,
    ) -> Result<Option<ObservedClaim>, PrimitiveMachineError> {
        Ok(self
            .appended
            .lock()
            .expect("claim history mutex poisoned")
            .iter()
            .rev()
            .find(|claim| &claim.coordinate == coordinate)
            .cloned())
    }

    fn append(&self, claim: &ObservedClaim) -> Result<(), PrimitiveMachineError> {
        self.appended
            .lock()
            .expect("claim history mutex poisoned")
            .push(claim.clone());
        Ok(())
    }

    fn history(
        &self,
        coordinate: &ObserveCoordinate,
    ) -> Result<Vec<ObservedClaim>, PrimitiveMachineError> {
        Ok(self
            .appended
            .lock()
            .expect("claim history mutex poisoned")
            .iter()
            .filter(|claim| &claim.coordinate == coordinate)
            .cloned()
            .collect())
    }
}

#[derive(Default)]
struct RecordingCasPersistence {
    puts: Mutex<Vec<(ValueId, Vec<u8>)>>,
}

impl ValuePersistence for RecordingCasPersistence {
    fn get(&self, _value: &ValueId) -> Result<Option<ValueBodyCandidate>, PrimitiveMachineError> {
        Ok(None)
    }

    fn put(&self, value: &ValueId, bytes: &[u8]) -> Result<(), PrimitiveMachineError> {
        self.puts
            .lock()
            .expect("cas persistence mutex poisoned")
            .push((value.clone(), bytes.to_vec()));
        Ok(())
    }
}

const OBSERVE_ONCE: &str = r#"
#[test]
fn observe_records_the_coordinate_claim() -> Stream<Check> {
    let blob = observe(fixture_registry().coordinate("case.crate"));
    yield expect_eq(blob.len(), 4096);
    yield fetched(1);
}
"#;

#[test]
fn observe_records_appends_and_persists_the_verified_observation() {
    let bytes = archive_bytes();
    let identity = blob_identity(&bytes);
    let upstream = vix::fetch::sha256_hex(&bytes);
    let server = BlobServer::start(bytes.clone());
    let fixtures = TempDir::new().expect("create observe fixture root");
    let claims = Arc::new(RecordingClaimHistory::default());
    let persistence = Arc::new(RecordingCasPersistence::default());
    let services = PrimitiveServices::default()
        .with_fixture_store(fixture_store(
            &fixtures,
            &server.url(),
            &identity,
            &upstream,
        ))
        .with_value_persistence(persistence.clone())
        .with_claim_history(claims.clone())
        .with_origin_adapter(Arc::new(HttpBlobOriginAdapter));

    let report = prepare_source(OBSERVE_ONCE)
        .expect("prepare observe source")
        .execute_with_primitive_services(services)
        .expect("observe records a coordinate claim");

    assert!(report.passed(), "observe report: {report:#?}");
    // One observation per lane across two lanes: the origin is read to discover
    // an identity not known before the read.
    assert_eq!(server.requests(), 2);
    assert_eq!(server.transfers(), 2);
    assert_eq!(server.targets(), ["/blob", "/blob"]);

    // Each observation appends a claim binding the coordinate to the identity it
    // resolved to; the observed identity is the self-verifying identity of the
    // arriving bytes (append-only claim log, one head per lane).
    let appended = claims.appended();
    assert_eq!(appended.len(), 2);
    for claim in &appended {
        assert_eq!(claim.observed, identity);
        assert_eq!(claim.coordinate.coordinate, server.url());
    }

    // The verified observation body is persisted into the CAS under its
    // self-verifying identity.
    let persisted = persistence
        .puts
        .lock()
        .expect("cas persistence mutex poisoned")
        .clone();
    assert_eq!(persisted.len(), 2);
    for entry in &persisted {
        assert_eq!(*entry, (identity.clone(), bytes.clone()));
    }

    for run in [&report.plain, &report.chaos] {
        assert_eq!(run.counters.primitive_invocations, 1, "{run:#?}");
        assert_eq!(run.counters.fetches_performed, 1, "{run:#?}");
    }
}

const OBSERVE_THEN_REFRESH: &str = r#"
fn observed_alias(blob: Blob) -> String {
    "case-" + blob.len().to_string() + ".crate"
}

#[test]
fn refresh_forces_a_fresh_observation_past_the_memo() -> Stream<Check> {
    let first = observe(fixture_registry().coordinate("case.crate"));
    yield expect_eq(first.len(), 4096);
    let second = observe(fixture_registry().coordinate("case.crate"));
    yield expect_eq(second.len(), 4096);
    let refreshed = refresh(fixture_registry().coordinate(observed_alias(second)));
    yield expect_eq(refreshed.len(), 4096);
    yield fetched(2);
}
"#;

#[test]
fn refresh_reobserves_past_the_memo_and_appends_a_new_head() {
    let bytes = archive_bytes();
    let identity = blob_identity(&bytes);
    let upstream = vix::fetch::sha256_hex(&bytes);
    let server = BlobServer::start(bytes.clone());
    let fixtures = TempDir::new().expect("create refresh fixture root");
    let claims = Arc::new(RecordingClaimHistory::default());
    let persistence = Arc::new(RecordingCasPersistence::default());
    let services = PrimitiveServices::default()
        .with_fixture_store(fixture_store_with_manifest(
            &fixtures,
            &format!(
                "case.crate {url} {content} {upstream}\n\
                 case-4096.crate {url} {content} {upstream}\n",
                url = server.url(),
                content = identity.content.hex(),
            ),
        ))
        .with_value_persistence(persistence.clone())
        .with_claim_history(claims.clone())
        .with_origin_adapter(Arc::new(HttpBlobOriginAdapter));

    let report = prepare_source(OBSERVE_THEN_REFRESH)
        .expect("prepare refresh source")
        .execute_with_primitive_services(services)
        .expect("refresh re-observes the coordinate");

    assert!(report.passed(), "refresh report: {report:#?}");
    // Per lane: the two identical observes collapse to one demand, but refresh is
    // a distinct demand that reads the origin again (2 reads per lane).
    assert_eq!(server.requests(), 4);
    assert_eq!(server.transfers(), 4);

    // Every observation appends its own head; the same identity observed twice is
    // appended twice, never deduped away (append-only claim log).
    let appended = claims.appended();
    assert_eq!(appended.len(), 4);
    for claim in &appended {
        assert_eq!(claim.observed, identity);
        assert_eq!(claim.coordinate.coordinate, server.url());
    }

    for run in [&report.plain, &report.chaos] {
        // One observe demand plus one refresh demand.
        assert_eq!(run.counters.primitive_invocations, 2, "{run:#?}");
        assert_eq!(run.counters.fetches_performed, 2, "{run:#?}");
    }
}

/// A claim history whose head advances underneath a reader: it returns `None`
/// on even-numbered `head` calls and a fixed seeded claim on odd-numbered calls.
/// A refresh samples the head twice (before and after reading the origin), so
/// this deterministically simulates a concurrent observer that advanced the head
/// mid-flight, driving the optimistic-concurrency `RefreshConflict` path.
struct ConcurrentlyAdvancingClaimHistory {
    seeded: ObservedClaim,
    calls: AtomicUsize,
    appended: Mutex<Vec<ObservedClaim>>,
}

impl ClaimHistory for ConcurrentlyAdvancingClaimHistory {
    fn head(
        &self,
        _coordinate: &ObserveCoordinate,
    ) -> Result<Option<ObservedClaim>, PrimitiveMachineError> {
        let call = self.calls.fetch_add(1, Ordering::AcqRel);
        Ok((call % 2 == 1).then(|| self.seeded.clone()))
    }

    fn append(&self, claim: &ObservedClaim) -> Result<(), PrimitiveMachineError> {
        self.appended
            .lock()
            .expect("claim history mutex poisoned")
            .push(claim.clone());
        Ok(())
    }

    fn history(
        &self,
        _coordinate: &ObserveCoordinate,
    ) -> Result<Vec<ObservedClaim>, PrimitiveMachineError> {
        Ok(self
            .appended
            .lock()
            .expect("claim history mutex poisoned")
            .clone())
    }
}

const REFRESH_ONLY: &str = r#"
#[test]
fn refresh_conflict_aborts_before_publishing() -> Stream<Check> {
    let blob = refresh(fixture_registry().coordinate("case.crate"));
    yield expect_eq(blob.len(), 4096);
}
"#;

#[test]
fn refresh_rejects_a_head_advanced_underneath_it() {
    let bytes = archive_bytes();
    let identity = blob_identity(&bytes);
    let upstream = vix::fetch::sha256_hex(&bytes);
    let advanced = blob_identity(b"a newer observation from a concurrent observer");
    let server = BlobServer::start(bytes.clone());
    let fixtures = TempDir::new().expect("create refresh-conflict fixture root");
    let claims = Arc::new(ConcurrentlyAdvancingClaimHistory {
        seeded: ObservedClaim {
            coordinate: ObserveCoordinate {
                capability: identity.clone(),
                coordinate: server.url(),
            },
            observed: advanced.clone(),
        },
        calls: AtomicUsize::new(0),
        appended: Mutex::new(Vec::new()),
    });
    let services = PrimitiveServices::default()
        .with_fixture_store(fixture_store(
            &fixtures,
            &server.url(),
            &identity,
            &upstream,
        ))
        .with_claim_history(claims.clone())
        .with_origin_adapter(Arc::new(HttpBlobOriginAdapter));

    let error = prepare_source(REFRESH_ONLY)
        .expect("prepare refresh-conflict source")
        .execute_with_primitive_services(services)
        .expect_err("refresh must reject a concurrently advanced head");

    // The refresh observed a fresh body, but the head moved underneath it before
    // the append, so the append is rejected and the current head is surfaced.
    assert_eq!(
        primitive_machine_error(error),
        PrimitiveMachineError::RefreshConflict { current: advanced }
    );
    // The conflict is detected only after the origin read (the body is valid CAS);
    // it is the append that is refused, so nothing is appended.
    assert_eq!(server.requests(), 1);
    assert!(
        claims
            .appended
            .lock()
            .expect("claim history mutex poisoned")
            .is_empty()
    );
}

/// An origin that is transiently unreachable: every read fails with a typed
/// retryable machine error rather than a body.
struct TransientOriginAdapter;

impl OriginAdapter for TransientOriginAdapter {
    fn read(
        &self,
        _capability: &ValueId,
        _coordinate: &str,
    ) -> Result<Vec<u8>, PrimitiveMachineError> {
        Err(PrimitiveMachineError::Unavailable {
            detail: "origin is temporarily unreachable".to_owned(),
        })
    }
}

#[test]
fn observe_surfaces_a_transient_origin_failure_as_a_typed_error() {
    let bytes = archive_bytes();
    let identity = blob_identity(&bytes);
    let upstream = vix::fetch::sha256_hex(&bytes);
    let fixtures = TempDir::new().expect("create transient-origin fixture root");
    let claims = Arc::new(RecordingClaimHistory::default());
    let persistence = Arc::new(RecordingCasPersistence::default());
    let services = PrimitiveServices::default()
        .with_fixture_store(fixture_store(
            &fixtures,
            "http://127.0.0.1:1/blob",
            &identity,
            &upstream,
        ))
        .with_value_persistence(persistence.clone())
        .with_claim_history(claims.clone())
        .with_origin_adapter(Arc::new(TransientOriginAdapter));

    let error = prepare_source(OBSERVE_ONCE)
        .expect("prepare transient-origin source")
        .execute_with_primitive_services(services)
        .expect_err("a transient origin failure must surface, not be swallowed");

    // The transient failure is preserved as a typed machine error, never
    // collapsed to a formatted string or silently retried away.
    assert_eq!(
        primitive_machine_error(error),
        PrimitiveMachineError::Unavailable {
            detail: "origin is temporarily unreachable".to_owned(),
        }
    );
    // A failed observation names no value: nothing is admitted, persisted, or
    // appended to the claim log.
    assert!(claims.appended().is_empty());
    assert!(
        persistence
            .puts
            .lock()
            .expect("cas persistence mutex poisoned")
            .is_empty()
    );
}
