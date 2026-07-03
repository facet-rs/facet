//! The executor over the real wire.
//!
//! What crosses it (all facet data):
//!   - plans + mount HASHES inbound (bytes must already be in the executor's
//!     local CAS — `put_tree`/`pull_from` get them there);
//!   - an event STREAM outbound: `PathReady` per output path as the tool
//!     writes it (the PRODUCING handle — a consumer can fetch and act on
//!     `lib.rmeta` while `lib.rlib` is still in flight), then `Finished`;
//!   - the OBSERVER's result: a shipped vix closure, evaluated HERE against
//!     the `Run` value, generic over its return type — only its result
//!     crosses back, never the world it observed.
//!
//! Trees move EXECUTOR→EXECUTOR (`pull_from`): the orchestrator handles
//! handles, not bytes — gravity-first scheduling's data path.
//!
//! Deliberately not wired yet: the two-tier cache on the producing path (the
//! recorded open question — what does an L1 entry point at before the output
//! hash exists?), and seatbelt confinement (runtime side).

use std::collections::HashMap;
use std::sync::Arc;

use facet::Facet;
use tokio::sync::Mutex;
use vix::exec::{ExecPlan, MountedWorld, ObservedWorld, ReadSet, Role, Tree};
use vix::oracle::{Oracle, Value};
use vox::Tx;

// ---------------------------------------------------------------------------
// Wire types.
// ---------------------------------------------------------------------------

#[derive(Facet, Debug, Clone)]
pub struct WireMount {
    pub at: String,
    /// Tree hash — the bytes must already live in the executor's CAS.
    pub tree: u64,
}

#[derive(Facet, Debug, Clone)]
pub struct WireExecRequest {
    pub plan: ExecPlan,
    pub mounts: Vec<WireMount>,
    pub capability: u64,
    /// Which tool to run (the command grammar's name — toy registry for now).
    pub command: String,
    /// A shipped vix closure (oracle::ship bytes) evaluated executor-side
    /// against the Run value; its result is the only thing that crosses.
    pub observer: Option<Vec<u8>>,
    /// The vix module the observer closure was born in (v0: source travels;
    /// later: canonical module bytes from the CAS).
    pub module: String,
}

#[derive(Facet, Debug, Clone, PartialEq)]
#[repr(u8)]
pub enum WireExecEvent {
    Started,
    /// An output path landed — fetchable NOW via fetch_path, before Finished.
    PathReady { path: String, content_hash: u64 },
    /// The observer's result (oracle::ship bytes of the vix value).
    ObserverResult { value: Vec<u8> },
    Finished { ok: bool, tree: u64, read_set_len: u64 },
    Failed { error: String },
}

// Owned DTO — the streamed channel payload needs a lifetime-shortening view.
vox_schema::impl_reborrow_owned!(WireExecEvent);

// ---------------------------------------------------------------------------
// The service.
// ---------------------------------------------------------------------------

#[vox::service]
pub trait Executor {
    /// Ingest a tree (postcard bytes) into the local CAS; returns its hash.
    async fn put_tree(&self, bytes: Vec<u8>) -> u64;
    /// Which of these trees are already local?
    async fn have(&self, hashes: Vec<u64>) -> Vec<bool>;
    /// Whole tree out of the CAS (postcard bytes). Chunking comes later.
    async fn fetch_tree(&self, hash: u64) -> Option<Vec<u8>>;
    /// One path out of a (possibly still PRODUCING) run's output space.
    async fn fetch_path(&self, run: u64, path: String) -> Option<String>;
    /// Pull a tree from a peer executor into the local CAS — the
    /// executor→executor data path; the orchestrator never carries bytes.
    async fn pull_from(&self, peer: String, hash: u64) -> bool;
    /// Run a plan; events stream as they happen.
    async fn exec(&self, request: WireExecRequest, run: u64, events: Tx<WireExecEvent>);
}

/// A tool the wire executor can run — PROGRESSIVE: it emits each output path
/// as it produces it (the seam difference from vix::exec::Tool, whose outputs
/// are atomic; those adapt by emitting everything at the end).
pub trait WireTool: Send + Sync {
    fn run(
        &self,
        plan: &ExecPlan,
        world: &mut ObservedWorld<'_>,
        emit: &mut dyn FnMut(&str, &str),
    ) -> Result<(), String>;
}

/// Adapt an atomic vix::exec tool (FakeCc, FakeAr) to the progressive seam.
pub struct Atomic<T: vix::exec::Tool + Send + Sync>(pub T);

impl<T: vix::exec::Tool + Send + Sync> WireTool for Atomic<T> {
    fn run(
        &self,
        plan: &ExecPlan,
        world: &mut ObservedWorld<'_>,
        emit: &mut dyn FnMut(&str, &str),
    ) -> Result<(), String> {
        let out = self.0.run(plan, world)?;
        for (path, contents) in &out.entries {
            emit(path, contents);
        }
        Ok(())
    }
}

#[derive(Default)]
struct Cas {
    trees: HashMap<u64, Tree>,
    /// Output spaces of runs, filled progressively while the tool executes.
    runs: HashMap<u64, Tree>,
}

#[derive(Clone)]
pub struct ExecutorService {
    cas: Arc<Mutex<Cas>>,
    tools: Arc<HashMap<String, Arc<dyn WireTool>>>,
}

impl ExecutorService {
    pub fn new(tools: HashMap<String, Arc<dyn WireTool>>) -> Self {
        ExecutorService {
            cas: Arc::new(Mutex::new(Cas::default())),
            tools: Arc::new(tools),
        }
    }

    /// The default registry: the fake compiler + archiver, adapted.
    pub fn with_default_tools() -> Self {
        let mut tools: HashMap<String, Arc<dyn WireTool>> = HashMap::new();
        tools.insert("cc".into(), Arc::new(Atomic(vix::exec::FakeCc)));
        tools.insert("ar".into(), Arc::new(Atomic(vix::exec::FakeAr)));
        Self::new(tools)
    }

    pub fn with_tool(mut self, name: &str, tool: Arc<dyn WireTool>) -> Self {
        Arc::get_mut(&mut self.tools)
            .expect("add tools before cloning the service")
            .insert(name.to_string(), tool);
        self
    }
}

fn tree_hash(tree: &Tree) -> u64 {
    tree.fingerprint()
}

fn content_hash(s: &str) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

impl Executor for ExecutorService {
    async fn put_tree(&self, bytes: Vec<u8>) -> u64 {
        let tree = tree_from_bytes(&bytes).expect("tree deserializes");
        let hash = tree_hash(&tree);
        self.cas.lock().await.trees.insert(hash, tree);
        hash
    }

    async fn have(&self, hashes: Vec<u64>) -> Vec<bool> {
        let cas = self.cas.lock().await;
        hashes.iter().map(|h| cas.trees.contains_key(h)).collect()
    }

    async fn fetch_tree(&self, hash: u64) -> Option<Vec<u8>> {
        let cas = self.cas.lock().await;
        cas.trees.get(&hash).map(tree_to_bytes)
    }

    async fn fetch_path(&self, run: u64, path: String) -> Option<String> {
        let cas = self.cas.lock().await;
        cas.runs.get(&run).and_then(|t| t.entries.get(&path).cloned())
    }

    async fn pull_from(&self, peer: String, hash: u64) -> bool {
        let Ok(client) = vox::connect_lane::<ExecutorClient>(&peer).await else {
            return false;
        };
        let Ok(Some(bytes)) = client.fetch_tree(hash).await else {
            return false;
        };
        self.put_tree(bytes).await == hash
    }

    async fn exec(&self, request: WireExecRequest, run: u64, events: Tx<WireExecEvent>) {
        let _ = events.send(WireExecEvent::Started).await;

        // Materialize mounts from the local CAS: bytes must already be here.
        let mut mounts = Vec::new();
        {
            let cas = self.cas.lock().await;
            for m in &request.mounts {
                let Some(tree) = cas.trees.get(&m.tree) else {
                    let _ = events
                        .send(WireExecEvent::Failed {
                            error: format!("tree {:x} not in local CAS (pull it first)", m.tree),
                        })
                        .await;
                    return;
                };
                mounts.push(vix::exec::Mount {
                    at: m.at.clone(),
                    tree: tree.clone(),
                });
            }
        }

        let Some(tool) = self.tools.get(&request.command).cloned() else {
            let _ = events
                .send(WireExecEvent::Failed {
                    error: format!("no tool for `{}`", request.command),
                })
                .await;
            return;
        };

        // Run the tool on a blocking thread; forward each produced path into
        // the run's PRODUCING output space + the event stream immediately.
        let (path_tx, mut path_rx) = tokio::sync::mpsc::unbounded_channel::<(String, String)>();
        let plan = request.plan.clone();
        let tool_task = tokio::task::spawn_blocking(move || {
            let world = MountedWorld::new(&mounts);
            let mut observed = ObservedWorld::new(&world);
            let mut emit = |path: &str, contents: &str| {
                let _ = path_tx.send((path.to_string(), contents.to_string()));
            };
            let result = tool.run(&plan, &mut observed, &mut emit);
            (result, observed.into_read_set())
        });

        let mut produced = Tree::default();
        while let Some((path, contents)) = path_rx.recv().await {
            let hash = content_hash(&contents);
            produced.entries.insert(path.clone(), contents.clone());
            self.cas
                .lock()
                .await
                .runs
                .entry(run)
                .or_default()
                .entries
                .insert(path.clone(), contents);
            let _ = events
                .send(WireExecEvent::PathReady {
                    path,
                    content_hash: hash,
                })
                .await;
        }

        let (result, read_set): (Result<(), String>, ReadSet) =
            tool_task.await.expect("tool task joins");
        if let Err(error) = result {
            let _ = events.send(WireExecEvent::Failed { error }).await;
            return;
        }

        // Flush: the finished output space becomes an immutable CAS tree.
        let final_hash = tree_hash(&produced);
        self.cas.lock().await.trees.insert(final_hash, produced.clone());

        // The observer: a shipped vix closure, evaluated HERE against the Run
        // value. Only its result crosses.
        if let Some(observer_bytes) = &request.observer {
            match evaluate_observer(&request.module, observer_bytes, &produced) {
                Ok(value_bytes) => {
                    let _ = events
                        .send(WireExecEvent::ObserverResult { value: value_bytes })
                        .await;
                }
                Err(error) => {
                    let _ = events.send(WireExecEvent::Failed { error }).await;
                    return;
                }
            }
        }

        let _ = events
            .send(WireExecEvent::Finished {
                ok: true,
                tree: final_hash,
                read_set_len: read_set.entries.len() as u64,
            })
            .await;
    }
}

/// Executor-side observer evaluation: reconstitute the closure, bind the Run
/// value, invoke, ship the result. The observer is generic over its return
/// type — whatever it returns is what the exec node IS to the graph.
fn evaluate_observer(module: &str, observer: &[u8], outputs: &Tree) -> Result<Vec<u8>, String> {
    let oracle = Oracle::load(module)?;
    let closure = vix::oracle::receive(observer)?;
    let run = Value::Struct {
        name: "Run".to_string(),
        fields: vec![
            ("ok".to_string(), Value::Bool(true)),
            ("out".to_string(), Value::Tree(outputs.clone())),
        ],
    };
    let result = oracle.invoke(closure, vec![run])?;
    vix::oracle::ship(&result)
}

// v0 helpers: whole-tree phon bytes (chunking later).
pub fn tree_to_bytes(tree: &Tree) -> Vec<u8> {
    phon::api::encode(tree).expect("tree serializes")
}

pub fn tree_from_bytes(bytes: &[u8]) -> Result<Tree, String> {
    phon::api::decode(bytes).map_err(|e| format!("wire decode: {e}"))
}

/// A progressive fake rustc: writes `lib.rmeta`, then WAITS for a green light
/// (the test's stand-in for "codegen takes a while"), then writes `lib.rlib`.
/// The canonical pipelining probe: a consumer must be able to fetch the rmeta
/// while this tool is still blocked.
pub struct FakeRustc {
    pub proceed: Arc<std::sync::Condvar>,
    pub gate: Arc<std::sync::Mutex<bool>>,
}

impl FakeRustc {
    pub fn gated() -> (Arc<Self>, impl Fn()) {
        let tool = Arc::new(FakeRustc {
            proceed: Arc::new(std::sync::Condvar::new()),
            gate: Arc::new(std::sync::Mutex::new(false)),
        });
        let t = tool.clone();
        let open = move || {
            *t.gate.lock().unwrap() = true;
            t.proceed.notify_all();
        };
        (tool, open)
    }
}

impl WireTool for FakeRustc {
    fn run(
        &self,
        plan: &ExecPlan,
        world: &mut ObservedWorld<'_>,
        emit: &mut dyn FnMut(&str, &str),
    ) -> Result<(), String> {
        let mut digest: u64 = 0;
        for (input, _) in plan.argv.iter().filter(|(_, r)| *r == Role::Input) {
            let src = world
                .read(input)
                .ok_or_else(|| format!("rustc: cannot read `{input}`"))?;
            digest = digest.wrapping_mul(31).wrapping_add(content_hash(&src));
        }
        // Metadata is done at end-of-typecheck: emit it NOW.
        emit("lib.rmeta", &format!("rmeta({digest:016x})"));
        // Codegen "runs" until the test opens the gate.
        let mut opened = self.gate.lock().unwrap();
        while !*opened {
            opened = self.proceed.wait(opened).unwrap();
        }
        emit("lib.rlib", &format!("rlib({digest:016x})"));
        Ok(())
    }
}
