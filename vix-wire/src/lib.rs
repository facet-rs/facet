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
use vix::oracle::{Oracle, PathDemand, PathMissing, PathPending, Value};
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
pub enum CacheSource {
    /// A process actually ran for this request.
    Fresh,
    /// Joined a LIVE identical run mid-flight (one process, many demands).
    Joined,
    /// Served from a completed run under the same coarse key.
    Tier1,
    /// Coarse key missed; a prior run's read-set verified against the
    /// current mounts — reused without running.
    Tier2 { verified: u64 },
}

#[derive(Facet, Debug, Clone, PartialEq)]
#[repr(u8)]
pub enum WireExecEvent {
    Started,
    /// How this demand is being served (right after Started).
    Serving {
        source: CacheSource,
    },
    /// An output path landed — fetchable NOW via fetch_path, before Finished.
    PathReady {
        path: String,
        content_hash: u64,
    },
    /// The observer's result (oracle::ship bytes of the vix value).
    ObserverResult {
        value: Vec<u8>,
    },
    Finished {
        ok: bool,
        tree: u64,
        read_set_len: u64,
    },
    Failed {
        error: String,
    },
}

// Owned DTOs — streamed channel payloads need a lifetime-shortening view.
vox_schema::impl_reborrow_owned!(WireExecEvent, CacheSource);

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
    /// Output spaces of runs (keyed by run IDENTITY), filled progressively
    /// while the tool executes.
    runs: HashMap<u64, Tree>,
    /// Caller-chosen run ids alias onto run identities (joiners and the
    /// spawner share one output space — one process, one producing tree).
    run_aliases: HashMap<u64, u64>,
}

/// A finished run — what a cache entry points at AFTER the flush. Mid-flight
/// there is no entry: there is a live run to JOIN (the producing-vs-L1
/// answer: serve-before-cache; the journal records only completed runs).
#[derive(Clone)]
struct CompletedRun {
    paths: Vec<(String, u64)>,
    tree: u64,
    read_set: ReadSet,
}

/// A live run: the event log so far (for late joiners) + a broadcast for the
/// rest. Identical demands ATTACH here — one process, many observers.
struct LiveRun {
    log: Vec<WireExecEvent>,
    feed: tokio::sync::broadcast::Sender<WireExecEvent>,
}

enum RunState {
    InFlight(LiveRun),
    Complete(CompletedRun),
    Failed(String),
}

#[derive(Default)]
struct RunTable {
    /// By FULL identity: plan × capability × coarse mounts.
    states: HashMap<u64, RunState>,
    /// Tier-2 candidates by computation identity (plan × capability only —
    /// the mounts are what verification relaxes).
    candidates: HashMap<u64, Vec<CompletedRun>>,
    /// Observer results by (full run identity × observer canonical hash):
    /// one run, many observers, each a distinct memoized VALUE.
    observers: HashMap<(u64, u64), Vec<u8>>,
}

#[derive(Clone)]
pub struct ExecutorService {
    cas: Arc<Mutex<Cas>>,
    runs_table: Arc<Mutex<RunTable>>,
    tools: Arc<HashMap<String, Arc<dyn WireTool>>>,
}

impl ExecutorService {
    pub fn new(tools: HashMap<String, Arc<dyn WireTool>>) -> Self {
        ExecutorService {
            cas: Arc::new(Mutex::new(Cas::default())),
            runs_table: Arc::new(Mutex::new(RunTable::default())),
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
        let identity = cas.run_aliases.get(&run).copied().unwrap_or(run);
        cas.runs
            .get(&identity)
            .and_then(|t| t.entries.get(&path).cloned())
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

        // Run identity: plan × capability × coarse mounts. Computation
        // identity (for tier-2 candidates): plan × capability only.
        let comp_identity = {
            use std::hash::{DefaultHasher, Hash, Hasher};
            let mut h = DefaultHasher::new();
            request.plan.hash().hash(&mut h);
            request.capability.hash(&mut h);
            h.finish()
        };
        let identity = {
            use std::hash::{DefaultHasher, Hash, Hasher};
            let mut h = DefaultHasher::new();
            comp_identity.hash(&mut h);
            for m in &mounts {
                m.at.hash(&mut h);
                m.tree.fingerprint().hash(&mut h);
            }
            h.finish()
        };
        self.cas.lock().await.run_aliases.insert(run, identity);

        // Decide this demand's role under ONE lock: replay a completed run,
        // JOIN a live one, or drive a fresh process. Mid-flight there is no
        // cache entry — there is a run to attach to (serve-before-cache).
        enum ServeRole {
            Replay(CompletedRun, CacheSource),
            Join {
                log: Vec<WireExecEvent>,
                feed: tokio::sync::broadcast::Receiver<WireExecEvent>,
            },
            Drive(tokio::sync::broadcast::Sender<WireExecEvent>),
        }

        let role = {
            let mut table = self.runs_table.lock().await;
            match table.states.get(&identity) {
                Some(RunState::Complete(done)) => {
                    ServeRole::Replay(done.clone(), CacheSource::Tier1)
                }
                Some(RunState::Failed(error)) => {
                    let _ = events
                        .send(WireExecEvent::Failed {
                            error: error.clone(),
                        })
                        .await;
                    return;
                }
                Some(RunState::InFlight(live)) => ServeRole::Join {
                    log: live.log.clone(),
                    feed: live.feed.subscribe(),
                },
                None => {
                    // Tier 2: a prior run of the same COMPUTATION whose
                    // read-set verifies against the current world.
                    let world = MountedWorld::new(&mounts);
                    let cutoff = table.candidates.get(&comp_identity).and_then(|cands| {
                        cands
                            .iter()
                            .find(|c| vix::exec::verify(&c.read_set, &world))
                            .cloned()
                    });
                    match cutoff {
                        Some(done) => {
                            let verified = done.read_set.entries.len() as u64;
                            table
                                .states
                                .insert(identity, RunState::Complete(done.clone()));
                            ServeRole::Replay(done, CacheSource::Tier2 { verified })
                        }
                        None => {
                            let (feed, _) = tokio::sync::broadcast::channel(1024);
                            table.states.insert(
                                identity,
                                RunState::InFlight(LiveRun {
                                    log: Vec::new(),
                                    feed: feed.clone(),
                                }),
                            );
                            ServeRole::Drive(feed)
                        }
                    }
                }
            }
        };

        match role {
            ServeRole::Replay(done, source) => {
                let _ = events.send(WireExecEvent::Serving { source }).await;
                // Re-open the output space so fetch_path works on replays too.
                {
                    let mut cas = self.cas.lock().await;
                    if let Some(tree) = cas.trees.get(&done.tree).cloned() {
                        cas.runs.insert(identity, tree);
                    }
                }
                for (path, content_hash) in &done.paths {
                    let _ = events
                        .send(WireExecEvent::PathReady {
                            path: path.clone(),
                            content_hash: *content_hash,
                        })
                        .await;
                }
                self.finish_with_observer(&request, identity, &done, &events)
                    .await;
            }
            ServeRole::Join { log, mut feed } => {
                let _ = events
                    .send(WireExecEvent::Serving {
                        source: CacheSource::Joined,
                    })
                    .await;
                for event in log {
                    let _ = events.send(event).await;
                }
                loop {
                    match feed.recv().await {
                        Ok(WireExecEvent::Finished { .. }) => {
                            let done = {
                                let table = self.runs_table.lock().await;
                                match table.states.get(&identity) {
                                    Some(RunState::Complete(done)) => done.clone(),
                                    _ => {
                                        drop(table);
                                        let _ = events
                                            .send(WireExecEvent::Failed {
                                                error: "run completed without a record".into(),
                                            })
                                            .await;
                                        return;
                                    }
                                }
                            };
                            self.finish_with_observer(&request, identity, &done, &events)
                                .await;
                            return;
                        }
                        Ok(WireExecEvent::Failed { error }) => {
                            let _ = events.send(WireExecEvent::Failed { error }).await;
                            return;
                        }
                        Ok(event) => {
                            let _ = events.send(event).await;
                        }
                        Err(_) => {
                            let _ = events
                                .send(WireExecEvent::Failed {
                                    error: "live run feed lagged or closed".into(),
                                })
                                .await;
                            return;
                        }
                    }
                }
            }
            ServeRole::Drive(feed) => {
                let _ = events
                    .send(WireExecEvent::Serving {
                        source: CacheSource::Fresh,
                    })
                    .await;
                self.drive(&request, identity, mounts, feed, &events).await;
            }
        }
    }
}

impl ExecutorService {
    /// Append to the live run's log and fan out to joiners — atomically with
    /// respect to joiner subscription (same lock), so no event is lost or
    /// duplicated across the log/feed boundary.
    async fn broadcast(
        &self,
        identity: u64,
        feed: &tokio::sync::broadcast::Sender<WireExecEvent>,
        event: WireExecEvent,
    ) {
        let mut table = self.runs_table.lock().await;
        if let Some(RunState::InFlight(live)) = table.states.get_mut(&identity) {
            live.log.push(event.clone());
        }
        let _ = feed.send(event);
    }

    /// Evaluate THIS demand's observer (memoized by run identity × observer
    /// hash) and finish the stream. One run, many observers, each its own
    /// memoized value — the L1-proof discipline: distinct observers never
    /// alias.
    async fn finish_with_observer(
        &self,
        request: &WireExecRequest,
        identity: u64,
        done: &CompletedRun,
        events: &Tx<WireExecEvent>,
    ) {
        if let Some(observer_bytes) = &request.observer {
            let obs_hash = {
                use std::hash::{DefaultHasher, Hash, Hasher};
                let mut h = DefaultHasher::new();
                observer_bytes.hash(&mut h);
                h.finish()
            };
            let memo = self
                .runs_table
                .lock()
                .await
                .observers
                .get(&(identity, obs_hash))
                .cloned();
            let value = match memo {
                Some(v) => Ok(v),
                None => {
                    let outputs = self
                        .cas
                        .lock()
                        .await
                        .trees
                        .get(&done.tree)
                        .cloned()
                        .unwrap_or_default();
                    let module = request.module.clone();
                    let bytes = observer_bytes.clone();
                    let evaluated = tokio::task::spawn_blocking(move || {
                        evaluate_observer(&module, &bytes, &outputs)
                    })
                    .await
                    .expect("observer task joins");
                    if let Ok(v) = &evaluated {
                        self.runs_table
                            .lock()
                            .await
                            .observers
                            .insert((identity, obs_hash), v.clone());
                    }
                    evaluated
                }
            };
            match value {
                Ok(v) => {
                    let _ = events
                        .send(WireExecEvent::ObserverResult { value: v })
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
                tree: done.tree,
                read_set_len: done.read_set.entries.len() as u64,
            })
            .await;
    }

    /// Actually run the tool: the one process behind however many demands.
    async fn drive(
        &self,
        request: &WireExecRequest,
        identity: u64,
        mounts: Vec<vix::exec::Mount>,
        feed: tokio::sync::broadcast::Sender<WireExecEvent>,
        events: &Tx<WireExecEvent>,
    ) {
        let comp_identity = {
            use std::hash::{DefaultHasher, Hash, Hasher};
            let mut h = DefaultHasher::new();
            request.plan.hash().hash(&mut h);
            request.capability.hash(&mut h);
            h.finish()
        };

        let fail = |error: String| async {
            self.runs_table
                .lock()
                .await
                .states
                .insert(identity, RunState::Failed(error.clone()));
            let _ = feed.send(WireExecEvent::Failed {
                error: error.clone(),
            });
            let _ = events.send(WireExecEvent::Failed { error }).await;
        };

        let Some(tool) = self.tools.get(&request.command).cloned() else {
            fail(format!("no tool for `{}`", request.command)).await;
            return;
        };

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
        let mut paths = Vec::new();
        while let Some((path, contents)) = path_rx.recv().await {
            let hash = content_hash(&contents);
            produced.entries.insert(path.clone(), contents.clone());
            paths.push((path.clone(), hash));
            self.cas
                .lock()
                .await
                .runs
                .entry(identity)
                .or_default()
                .entries
                .insert(path.clone(), contents);
            let event = WireExecEvent::PathReady {
                path,
                content_hash: hash,
            };
            self.broadcast(identity, &feed, event.clone()).await;
            let _ = events.send(event).await;
        }

        let (result, read_set): (Result<(), String>, ReadSet) =
            tool_task.await.expect("tool task joins");
        if let Err(error) = result {
            fail(error).await;
            return;
        }

        // FLUSH: only now does a cache entry exist. The journal records
        // completed runs; in-flight state evaporates on failure/crash.
        let final_hash = tree_hash(&produced);
        self.cas.lock().await.trees.insert(final_hash, produced);
        let done = CompletedRun {
            paths,
            tree: final_hash,
            read_set,
        };
        {
            let mut table = self.runs_table.lock().await;
            table
                .states
                .insert(identity, RunState::Complete(done.clone()));
            table
                .candidates
                .entry(comp_identity)
                .or_default()
                .push(done.clone());
        }
        // Wake the joiners (state is Complete BEFORE the marker lands).
        let _ = feed.send(WireExecEvent::Finished {
            ok: true,
            tree: done.tree,
            read_set_len: done.read_set.entries.len() as u64,
        });

        self.finish_with_observer(request, identity, &done, events)
            .await;
    }
}

// ---------------------------------------------------------------------------
// The fleet: the oracle's ExecBackend over N wire executors. Placement is
// gravity-first (run where the inputs already live); missing mounts move
// EXECUTOR→EXECUTOR when any peer has them; the orchestrator uploads only
// trees it created itself. v0 gap, on purpose: result trees materialize back
// at the orchestrator (Value::Tree carries bytes) — remote-handle Trees in
// the language are the next step; observers are already the way around it.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transfer {
    /// Orchestrator → executor (a tree born at the orchestrator).
    Upload { to: usize, tree: u64 },
    /// Executor → executor (gravity: bytes never touch the orchestrator).
    GravityPull { from: usize, to: usize, tree: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Run where the most input trees already live (ties: fewest runs so far).
    Gravity,
    /// Alternate executors (tests: forces cross-machine traffic).
    RoundRobin,
}

pub struct FleetBackend {
    handle: tokio::runtime::Handle,
    executors: Vec<(String, ExecutorClient)>,
    placement: Placement,
    state: std::sync::Mutex<FleetState>,
}

#[derive(Default)]
struct FleetState {
    /// Which executors hold which trees (learned from puts/pulls/outputs).
    locations: HashMap<u64, Vec<usize>>,
    /// Every byte movement — the scheduler's future food.
    pub transfers: Vec<Transfer>,
    placements: u64,
    run_ids: u64,
}

impl FleetBackend {
    pub async fn connect(placement: Placement, addrs: &[String]) -> Result<Self, String> {
        let mut executors = Vec::new();
        for addr in addrs {
            let client: ExecutorClient = vox::connect_lane(addr)
                .await
                .map_err(|e| format!("connect {addr}: {e:?}"))?;
            executors.push((addr.clone(), client));
        }
        Ok(FleetBackend {
            handle: tokio::runtime::Handle::current(),
            executors,
            placement,
            state: std::sync::Mutex::new(FleetState::default()),
        })
    }

    pub fn transfers(&self) -> Vec<Transfer> {
        self.state.lock().unwrap().transfers.clone()
    }

    fn choose(&self, mount_hashes: &[u64]) -> usize {
        let mut state = self.state.lock().unwrap();
        let chosen = match self.placement {
            Placement::RoundRobin => (state.placements as usize) % self.executors.len(),
            Placement::Gravity => (0..self.executors.len())
                .max_by_key(|idx| {
                    mount_hashes
                        .iter()
                        .filter(|h| {
                            state
                                .locations
                                .get(h)
                                .is_some_and(|locs| locs.contains(idx))
                        })
                        .count()
                })
                .unwrap_or(0),
        };
        state.placements += 1;
        chosen
    }

    async fn ensure_mount(&self, target: usize, tree: &Tree) -> Result<u64, String> {
        let hash = tree.fingerprint();
        let holders: Vec<usize> = {
            let state = self.state.lock().unwrap();
            state.locations.get(&hash).cloned().unwrap_or_default()
        };
        if holders.contains(&target) {
            return Ok(hash);
        }
        let client = &self.executors[target].1;
        if let Some(&from) = holders.first() {
            // Gravity: the peer has it; bytes go executor→executor.
            let peer_addr = self.executors[from].0.clone();
            let ok = client
                .pull_from(peer_addr, hash)
                .await
                .map_err(|e| format!("pull_from: {e:?}"))?;
            if ok {
                let mut state = self.state.lock().unwrap();
                state.locations.entry(hash).or_default().push(target);
                state.transfers.push(Transfer::GravityPull {
                    from,
                    to: target,
                    tree: hash,
                });
                return Ok(hash);
            }
        }
        // Orchestrator-born (fetch/extract/merge results): upload once.
        let put = client
            .put_tree(tree_to_bytes(tree))
            .await
            .map_err(|e| format!("put_tree: {e:?}"))?;
        let mut state = self.state.lock().unwrap();
        state.locations.entry(hash).or_default().push(target);
        state.transfers.push(Transfer::Upload {
            to: target,
            tree: hash,
        });
        Ok(put)
    }
}

/// A live wire run the oracle can DEMAND from: paths land as PathReady events
/// arrive; demand_path blocks until its path exists (or the run ends without
/// it); flush blocks until Finished and materializes the tree (the flagged
/// v0 gap). std sync primitives on purpose — the oracle blocks a thread, the
/// event feeder runs on the fleet's runtime.
struct FleetRun {
    fleet_run_id: u64,
    client: ExecutorClient,
    handle: tokio::runtime::Handle,
    state: std::sync::Mutex<FleetRunState>,
    wake: std::sync::Condvar,
}

#[derive(Default)]
struct FleetRunState {
    ready_paths: std::collections::HashSet<String>,
    source: Option<vix::exec::ExecEvent>,
    finished: Option<Result<u64, String>>,
    flushed: Option<Tree>,
}

impl FleetRun {
    fn note(&self, event: WireExecEvent) {
        let mut state = self.state.lock().unwrap();
        match event {
            WireExecEvent::Serving { source } => {
                state.source = Some(match source {
                    CacheSource::Fresh => vix::exec::ExecEvent::Ran,
                    CacheSource::Joined => vix::exec::ExecEvent::Joined,
                    CacheSource::Tier1 => vix::exec::ExecEvent::Tier1Hit,
                    CacheSource::Tier2 { verified } => vix::exec::ExecEvent::Tier2Cutoff {
                        verified: verified as usize,
                    },
                });
            }
            WireExecEvent::PathReady { path, .. } => {
                state.ready_paths.insert(path);
            }
            WireExecEvent::Finished { ok, tree, .. } => {
                state.finished = Some(if ok {
                    Ok(tree)
                } else {
                    Err("run failed".to_string())
                });
            }
            WireExecEvent::Failed { error } => {
                state.finished = Some(Err(error));
            }
            _ => {}
        }
        self.wake.notify_all();
    }
}

impl vix::oracle::PendingRun for FleetRun {
    fn demand_path(&self, path: &str) -> Result<PathDemand, String> {
        // Wait only for THIS path — the language-level rmeta move.
        {
            let mut state = self.state.lock().unwrap();
            loop {
                if state.ready_paths.contains(path) {
                    break;
                }
                if let Some(finished) = &state.finished {
                    finished.clone()?;
                    drop(state);
                    let (tree, _) = self.flush()?;
                    let prefix = format!("{path}/");
                    return if tree.entries.keys().any(|entry| entry.starts_with(&prefix)) {
                        Ok(PathDemand::FinishRequired(PathPending {
                            path: path.to_string(),
                        }))
                    } else {
                        Ok(PathDemand::Missing(PathMissing {
                            path: path.to_string(),
                        }))
                    };
                }
                state = self.wake.wait(state).unwrap();
            }
        }
        let contents = tokio::task::block_in_place(|| {
            self.handle.clone().block_on(async {
                self.client
                    .fetch_path(self.fleet_run_id, path.to_string())
                    .await
                    .map_err(|e| format!("fetch_path: {e:?}"))?
                    .ok_or_else(|| format!("`{path}` vanished from the producing space"))
            })
        })?;
        Ok(PathDemand::File(contents))
    }

    fn flush(&self) -> Result<(Tree, vix::exec::ExecEvent), String> {
        let (tree_hash, source) = {
            let mut state = self.state.lock().unwrap();
            loop {
                if let Some(finished) = &state.finished {
                    let hash = finished.clone()?;
                    if let Some(tree) = &state.flushed {
                        return Ok((
                            tree.clone(),
                            state.source.clone().unwrap_or(vix::exec::ExecEvent::Ran),
                        ));
                    }
                    break (
                        hash,
                        state.source.clone().unwrap_or(vix::exec::ExecEvent::Ran),
                    );
                }
                state = self.wake.wait(state).unwrap();
            }
        };
        // v0: materialize the result back (the flagged gap — remote-handle
        // Trees in the language are the next step).
        let tree = tokio::task::block_in_place(|| {
            self.handle.clone().block_on(async {
                let bytes = self
                    .client
                    .fetch_tree(tree_hash)
                    .await
                    .map_err(|e| format!("fetch_tree: {e:?}"))?
                    .ok_or("finished tree missing from CAS")?;
                tree_from_bytes(&bytes)
            })
        })?;
        self.state.lock().unwrap().flushed = Some(tree.clone());
        Ok((tree, source))
    }
}

impl vix::oracle::ExecBackend for FleetBackend {
    fn spawn(
        &self,
        command: &str,
        plan: &ExecPlan,
        capability: u64,
        mounts: &[vix::exec::Mount],
    ) -> Result<Arc<dyn vix::oracle::PendingRun>, String> {
        // The oracle is sync; bridge onto the runtime this fleet was born on.
        tokio::task::block_in_place(|| {
            self.handle.clone().block_on(async {
                let mount_hashes: Vec<u64> = mounts.iter().map(|m| m.tree.fingerprint()).collect();
                let target = self.choose(&mount_hashes);
                let client = self.executors[target].1.clone();

                let mut wire_mounts = Vec::new();
                for m in mounts {
                    let hash = self.ensure_mount(target, &m.tree).await?;
                    wire_mounts.push(WireMount {
                        at: m.at.clone(),
                        tree: hash,
                    });
                }

                let run_id = {
                    let mut state = self.state.lock().unwrap();
                    state.run_ids += 1;
                    state.run_ids
                };
                let request = WireExecRequest {
                    plan: plan.clone(),
                    mounts: wire_mounts,
                    capability,
                    command: command.to_string(),
                    observer: None,
                    module: String::new(),
                };

                let run = Arc::new(FleetRun {
                    fleet_run_id: run_id,
                    client: client.clone(),
                    handle: self.handle.clone(),
                    state: std::sync::Mutex::new(FleetRunState::default()),
                    wake: std::sync::Condvar::new(),
                });

                // Dispatch; the feeder task keeps the run state live while
                // evaluation continues elsewhere. THIS is the demand shape.
                let (tx, mut rx) = vox::channel::<WireExecEvent>();
                let exec_client = client.clone();
                tokio::spawn(async move {
                    let _ = exec_client.exec(request, run_id, tx).await;
                });
                let feeder_run = run.clone();
                tokio::spawn(async move {
                    while let Ok(Some(event)) = rx.recv().await {
                        let event = event.get().clone();
                        let done = matches!(
                            event,
                            WireExecEvent::Finished { .. } | WireExecEvent::Failed { .. }
                        );
                        feeder_run.note(event);
                        if done {
                            break;
                        }
                    }
                });

                Ok(run as Arc<dyn vix::oracle::PendingRun>)
            })
        })
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
