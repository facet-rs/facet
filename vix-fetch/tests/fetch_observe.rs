use std::io::{ErrorKind, Read as _, Write as _};
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use tempfile::TempDir;
use vix::ratchet::prepare_source;
use vix::runtime::{
    CanonicalBlobPersistence, FixtureStore, FramedNode, PrimitiveMachineError, PrimitiveServices,
    ValueBodyCandidate, ValueId, ValuePersistence,
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

const FETCH_STORE_THEN_REDEMAND: &str = r#"
#[test]
fn pinned_fetch_local_store_hit_never_contacts_provider_or_origin() -> Stream<Check> {
    let first = fetch(fixture_registry().url("first.crate"));
    yield expect_eq(first.len(), 4096);
    let second = fetch(fixture_registry().url("second.crate"));
    yield expect_eq(second.len(), 4096);
    yield fetched(1);
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
        "first.crate {} {} {upstream}\nsecond.crate {} {} {upstream}\n",
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
