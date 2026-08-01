use std::io::{ErrorKind, Read as _, Write as _};
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use tempfile::TempDir;
use vix::runtime::{
    CanonicalBlobPersistence, FramedNode, MachineCause, PrimitiveMachineError, PrimitiveServices,
    RuntimeFault, ValueBodyCandidate, ValueId, ValuePersistence,
};
use vix::vir::{ExternKind, Type};
use vixen_primitives::HttpBlobOriginAdapter;
use vixen_runtime::fixture::FixtureStore;
use vixen_runtime::ratchet::{RunError, prepare_source};

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
                if stream.write_all(header.as_bytes()).is_ok() {
                    // Count the transfer before streaming the body. The client's
                    // fetch completes as soon as it has read the Content-Length
                    // body; for a body larger than the socket buffer `write_all`
                    // only returns once the client has drained it, so a
                    // post-write increment on this server thread races the
                    // client (and the test) observing completion — surfacing as
                    // a flaky `transfers()` off-by-one on Windows. Incrementing
                    // here orders the count before the bytes the client waits on.
                    worker_transfers.fetch_add(1, Ordering::AcqRel);
                    let _ = stream.write_all(&body);
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
            .join("tests/fixtures/registry/tokio-1.52.3.crate"),
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

/// The two-adapter service set these tests run on: the temp-root fixture
/// store (manifest + `fixture://` coordinates) and the HTTP Blob transport,
/// each installed under its declaration
/// (r[verify machine.primitive.origin-routing]).
fn harness_origins(store: FixtureStore) -> PrimitiveServices {
    fixture_only(store)
        .with_origin(
            HttpBlobOriginAdapter::decl(),
            Arc::new(HttpBlobOriginAdapter),
        )
        .expect("the http adapter's declaration overlaps nothing")
}

/// The fixture adapter alone — no transport serves `http(s)`.
fn fixture_only(store: FixtureStore) -> PrimitiveServices {
    PrimitiveServices::default()
        .with_origin(FixtureStore::origin_decl(), Arc::new(store))
        .expect("the fixture adapter's declaration overlaps nothing")
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
    let upstream = vixen_primitives::sha256_pin(&bytes);
    let server = BlobServer::start(bytes);
    let fixtures = TempDir::new().expect("create fixture root");
    let services = harness_origins(fixture_store(
        &fixtures,
        &server.url(),
        &identity,
        &upstream,
    ));

    let report = prepare_source(FETCH_AND_EXTRACT)
        .expect("prepare Vix fetch/extract source")
        .execute_with_primitive_services(services)
        .expect("execute Vix fetch/extract source");

    assert!(report.passed(), "fetch/extract report: {report:#?}");
    assert_eq!(server.transfers(), 2, "plain and chaos each transfer once");
    assert_eq!(server.requests(), 2, "plain and chaos each request once");
    for run in [&report.plain, &report.chaos] {
        assert_eq!(
            run.counters.primitive_invocations, 4,
            "one registry-url resolve, one fetch, one untar, and one observed \
             tree read — `untar` counts here because it is a registered \
             primitive now rather than a machine op: {run:#?}"
        );
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
    let upstream = vixen_primitives::sha256_pin(&bytes);
    let server = BlobServer::start(bytes);
    let fixtures = TempDir::new().expect("create fixture root");
    let services = harness_origins(fixture_store(
        &fixtures,
        &server.url(),
        &identity,
        &upstream,
    ));

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
        plain.counters.primitive_invocations, 3,
        "the registry-url resolve, the fetch, and the `Blob.len` read each began one registered-primitive demand: {plain:#?}"
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
    let upstream = vixen_primitives::sha256_pin(&bytes);
    let server = BlobServer::start(bytes.clone());
    let fixtures = TempDir::new().expect("create fixture root");
    let persistence_root = TempDir::new().expect("create Blob persistence root");
    let persistence = Arc::new(CanonicalBlobPersistence::new(persistence_root.path()));
    persistence
        .put(&identity, &bytes)
        .expect("prepopulate canonical Blob persistence");
    let services = harness_origins(fixture_store(
        &fixtures,
        &server.url(),
        &identity,
        &upstream,
    ))
    .with_value_persistence(persistence);

    let report = prepare_source(FETCH_ONLY)
        .expect("prepare Vix provider-hit source")
        .execute_with_primitive_services(services)
        .expect("execute Vix provider-hit source");

    assert!(report.passed(), "provider-hit report: {report:#?}");
    assert_eq!(server.transfers(), 0, "provider hit must precede origin");
    assert_eq!(server.requests(), 0, "provider hit must not contact origin");
    for run in [&report.plain, &report.chaos] {
        assert_eq!(run.counters.primitive_invocations, 3, "{run:#?}");
        assert_eq!(run.counters.fetches_performed, 0, "{run:#?}");
    }
}

#[test]
fn pinned_fetch_local_store_hit_never_contacts_provider_or_origin() {
    let bytes = archive_bytes();
    let identity = blob_identity(&bytes);
    let upstream = vixen_primitives::sha256_pin(&bytes);
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
    let services = harness_origins(fixture_store_with_manifest(&fixtures, &manifest))
        .with_value_persistence(persistence.clone());

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
        assert_eq!(run.counters.primitive_invocations, 6, "{run:#?}");
        assert_eq!(run.counters.fetches_performed, 1, "{run:#?}");
    }
}

/// No conjuring: with no adapter claiming the coordinate's scheme, nothing
/// serves an origin read — not even the fixture adapter the services DO
/// carry, because routing is a lookup over declarations and `fixture://` is
/// all it declares. The refusal is loud and typed, it names the coordinate
/// AND the installed declaration set, and zero fetches are performed (the
/// origin server is never contacted). This pins the death of the scheduler's
/// silent fixture-store fallback.
///
/// r[verify machine.primitive.origin-routing]
#[test]
fn no_origin_adapter_refuses_the_origin_read_loudly() {
    let bytes = archive_bytes();
    let identity = blob_identity(&bytes);
    let upstream = vixen_primitives::sha256_pin(&bytes);
    let server = BlobServer::start(bytes);
    let fixtures = TempDir::new().expect("create fixture root");
    // A fixture store, but NO origin adapter: the registry manifest resolves
    // (so the fetch has a real coordinate to ask for), and then the origin
    // read must refuse rather than fall back to anything.
    let services = fixture_only(fixture_store(
        &fixtures,
        &server.url(),
        &identity,
        &upstream,
    ));

    let error = prepare_source(FETCH_MUST_FAIL)
        .expect("prepare no-origin-adapter source")
        .execute_with_primitive_services(services)
        .expect_err("an origin read with no origin adapter installed must refuse");

    let PrimitiveMachineError::OriginUnroutable { detail } = primitive_machine_error(error) else {
        panic!("expected a typed unroutable refusal");
    };
    assert!(
        detail.contains("no installed origin adapter serves scheme \"http\""),
        "the refusal names the unserved scheme: {detail}"
    );
    assert!(
        detail.contains(&server.url()),
        "the refusal names the coordinate: {detail}"
    );
    assert!(
        detail.contains("\"fixture\""),
        "the refusal names what IS installed: {detail}"
    );
    assert_eq!(server.requests(), 0, "zero fetches performed");
}

/// A glob over a tree handle no installed declaration claims is the same
/// loud typed refusal an unclaimed coordinate gets — never a fall-through
/// into content enumeration, whose parse failure would report "malformed
/// bytes" for what is actually a configuration gap. With NO origin adapter
/// installed, the fixture handle is unclaimed, and the refusal names both
/// what was asked (the handle's leading bytes) and what is installed
/// (nothing).
///
/// r[verify machine.primitive.origin-routing]
#[test]
fn glob_on_an_unclaimed_tree_handle_refuses_loudly() {
    const GLOB_UNCLAIMED: &str = r#"
#[test]
fn glob_unclaimed() -> Stream<Check> {
    let tree = fixture_tree("small-crate");
    let sources = tree.glob("src/*.rs").collect().values().sorted();
    yield expect_eq(sources.len(), 2);
}
"#;

    let error = prepare_source(GLOB_UNCLAIMED)
        .expect("prepare unclaimed-glob source")
        .execute_with_primitive_services(PrimitiveServices::default())
        .expect_err("a glob over an unclaimed handle must refuse");

    let PrimitiveMachineError::OriginUnroutable { detail } = primitive_machine_error(error) else {
        panic!("expected a typed unroutable refusal, not a malformed-bytes lie");
    };
    assert!(
        detail.contains("declared namespace"),
        "the refusal says the handle is unclaimed: {detail}"
    );
    assert!(
        detail.contains("no origin adapter is installed"),
        "the refusal names the installed set: {detail}"
    );
}

#[test]
fn pinned_fetch_rejects_vix_identity_mismatch() {
    let bytes = archive_bytes();
    let observed = blob_identity(&bytes);
    let claimed = blob_identity(b"different Blob body");
    let upstream = vixen_primitives::sha256_pin(&bytes);
    let server = BlobServer::start(bytes);
    let fixtures = TempDir::new().expect("create fixture root");
    let services =
        harness_origins(fixture_store(&fixtures, &server.url(), &claimed, &upstream));

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
    // A well-formed pin that simply does not match the bytes: this test is
    // about VERIFICATION failing at each tier, not about parsing, so the pin
    // has to get past the parser to exercise anything.
    let wrong_upstream = format!("sha256:{}", "00".repeat(32));

    let store_server = BlobServer::start(bytes.clone());
    let store_fixtures = TempDir::new().expect("create Store-tier fixture root");
    let store_manifest = format!(
        "first.crate {} {}\nsecond.crate/4096 {} {} {wrong_upstream}\n",
        store_server.url_for("/first"),
        identity.content.hex(),
        store_server.url_for("/must-not-be-contacted"),
        identity.content.hex(),
    );
    let store_services =
        harness_origins(fixture_store_with_manifest(&store_fixtures, &store_manifest));
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
    let provider_services = harness_origins(fixture_store(
        &provider_fixtures,
        &provider_server.url(),
        &identity,
        &wrong_upstream,
    ))
    .with_value_persistence(provider);
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
    let origin_services = harness_origins(fixture_store(
        &origin_fixtures,
        &origin_server.url(),
        &identity,
        &wrong_upstream,
    ));
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
    let upstream = vixen_primitives::sha256_pin(&bytes);
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
    let services = harness_origins(fixture_store(
        &fixtures,
        &server.url(),
        &identity,
        &upstream,
    ))
    .with_value_persistence(provider.clone());

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
        assert_eq!(run.counters.primitive_invocations, 3, "{run:#?}");
        assert_eq!(run.counters.fetches_performed, 1, "{run:#?}");
    }
}

/// A pin the manifest got wrong fails AT THE MANIFEST, naming itself.
///
/// The failure mode this guards against is the one that cost real debugging
/// time while writing this change: an unparseable pin used to ride to the wire
/// and surface three layers away as an opaque `InvalidRequest` against a
/// synthesized value id, which tells you nothing about which row of which
/// manifest is wrong.
///
/// The three cases are the three ways a pin goes wrong, and they must stay
/// distinguishable: untagged (the shape self-describing pins abolish), refused
/// (an algorithm we can compute and decline to trust), and unknown.
///
/// r[verify vixen.pins.self-describing]
/// r[verify vixen.pins.algorithm-strength]
#[test]
fn a_manifest_pin_that_is_not_a_pin_fails_at_the_manifest() {
    let bytes = archive_bytes();
    let identity = blob_identity(&bytes);
    for bad in [
        &"00".repeat(32),                     // untagged: which algorithm?
        &format!("md5:{}", "00".repeat(16)),  // refused, not unknown
        &format!("sha3:{}", "00".repeat(32)), // genuinely unknown
    ] {
        let server = BlobServer::start(bytes.clone());
        let fixtures = TempDir::new().expect("create fixture root");
        let services =
            harness_origins(fixture_store(&fixtures, &server.url(), &identity, bad));
        let error = prepare_source(FETCH_ONLY)
            .expect("prepare source")
            .execute_with_primitive_services(services)
            .expect_err("a malformed manifest pin must fail");
        let error = primitive_machine_error(error);
        // Same variant the manifest's other malformed-value cases use (a
        // non-UTF-8 manifest, an absent artifact): the registry served
        // something it had no authority to call a pin.
        let PrimitiveMachineError::AuthorityViolation { detail } = &error else {
            panic!("a malformed pin is a manifest authority violation, got {error:#?}");
        };
        assert!(
            detail.contains("is not a pin"),
            "the diagnostic must say the manifest pin is at fault: {detail}"
        );
        assert_eq!(
            server.transfers(),
            0,
            "nothing is fetched on the strength of a pin we could not read"
        );
    }
}

/// A **real** registry artifact is a gzipped tar, and `untar` parses plain
/// ustar. Until `Blob.gunzip()` existed, the only archive this system had ever
/// unpacked was an uncompressed fixture that merely had a `.crate` name — so the
/// whole fetch→extract leg worked exclusively against a shape no registry
/// serves. This is that leg over bytes shaped like the real thing.
///
/// r[verify machine.primitive.fetch-returns-a-blob]
#[test]
fn a_gzipped_crate_unpacks_through_gunzip_then_untar() {
    const SOURCE: &str = r#"
#[test]
fn gzipped_crate() -> Stream<Check> {
    let archive = fetch(fixture_registry().url("tokio-1.52.3-gzipped.crate"));
    let tree = untar(archive.gunzip());
    let manifest = (tree / "Cargo.toml").text();
    yield expect(manifest.contains("name = \"tokio\""));
}
"#;
    let report = prepare_source(SOURCE)
        .expect("prepare gzipped-crate source")
        .execute()
        .expect("execute gzipped-crate source");
    assert!(report.passed(), "gzipped crate report: {report:#?}");
}

/// The compressed and uncompressed forms of one archive are two **Blobs** and one
/// **Tree**: `machine.primitive.fetch-returns-a-blob` says so outright, and it is
/// the property that makes recompressing a mirror's artifacts harmless.
///
/// r[verify machine.primitive.fetch-returns-a-blob]
#[test]
fn gzipping_changes_the_blob_but_not_the_tree() {
    const SOURCE: &str = r#"
#[test]
fn same_tree_either_way() -> Stream<Check> {
    let plain = fetch(fixture_registry().url("tokio-1.52.3.crate"));
    let gzipped = fetch(fixture_registry().url("tokio-1.52.3-gzipped.crate"));
    yield expect(plain.len() != gzipped.len());
    yield expect_eq((untar(plain) / "Cargo.toml").text(), (untar(gzipped.gunzip()) / "Cargo.toml").text());
}
"#;
    let report = prepare_source(SOURCE)
        .expect("prepare recompression source")
        .execute()
        .expect("execute recompression source");
    assert!(report.passed(), "recompression report: {report:#?}");
}
