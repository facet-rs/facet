//! Vox RPC exec backend for vix.
//!
//! The runner owns sandboxing, VFS observation, and process execution. This
//! backend owns the client side of the public protocol and maps runner results
//! back into vix's language-owned cache and observation model.

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use exec_protocol as protocol;

use crate::exec::{
    ExecCache, ExecEvent, Mount, MountedWorld, Outcome, ReadObservation, ReadSet, Role, Tree,
    TreeEvent, TreeFileCompletion,
};
use crate::machine::{
    MachineExecBackend, MachineExecRequest, MachinePathDemand, MachinePendingRun,
};

pub struct RpcRunnerBackend {
    endpoint: String,
    cache: Arc<Mutex<ExecCache>>,
}

impl RpcRunnerBackend {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            cache: Arc::new(Mutex::new(ExecCache::new())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RpcRunnerError {
    CachePoisoned,
    Connect(String),
    Capabilities(String),
    UnsupportedProtocol {
        expected: u32,
        actual: u32,
    },
    UnsupportedTransport {
        transports: Vec<protocol::RunnerTransport>,
    },
    Staging(String),
    Spawn(String),
    UnsupportedPlatform(String),
    ProcessExit(String),
    Harvest(String),
    Verification(String),
    Protocol(String),
}

impl RpcRunnerError {
    fn from_exec_error(error: protocol::ExecError) -> Self {
        let diagnostic = error.diagnostic.0;
        match error.kind {
            protocol::ExecErrorKind::Staging => Self::Staging(diagnostic),
            protocol::ExecErrorKind::Spawn => Self::Spawn(diagnostic),
            protocol::ExecErrorKind::UnsupportedPlatform => Self::UnsupportedPlatform(diagnostic),
            protocol::ExecErrorKind::ProcessExit => Self::ProcessExit(diagnostic),
            protocol::ExecErrorKind::Harvest => Self::Harvest(diagnostic),
            protocol::ExecErrorKind::Verification => Self::Verification(diagnostic),
            protocol::ExecErrorKind::Protocol => Self::Protocol(diagnostic),
        }
    }
}

impl std::fmt::Display for RpcRunnerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CachePoisoned => formatter.write_str("rpc runner exec cache poisoned"),
            Self::Connect(message) => write!(formatter, "rpc runner connect failed: {message}"),
            Self::Capabilities(message) => {
                write!(formatter, "rpc runner capability probe failed: {message}")
            }
            Self::UnsupportedProtocol { expected, actual } => write!(
                formatter,
                "rpc runner protocol mismatch: expected {expected}, got {actual}"
            ),
            Self::UnsupportedTransport { transports } => write!(
                formatter,
                "rpc runner does not advertise websocket transport: {transports:?}"
            ),
            Self::Staging(message) => write!(formatter, "rpc runner staging failure: {message}"),
            Self::Spawn(message) => write!(formatter, "rpc runner spawn failure: {message}"),
            Self::UnsupportedPlatform(message) => {
                write!(formatter, "rpc runner unsupported platform: {message}")
            }
            Self::ProcessExit(message) => {
                write!(formatter, "rpc runner process exit failure: {message}")
            }
            Self::Harvest(message) => write!(formatter, "rpc runner harvest failure: {message}"),
            Self::Verification(message) => {
                write!(formatter, "rpc runner verification failure: {message}")
            }
            Self::Protocol(message) => write!(formatter, "rpc runner protocol failure: {message}"),
        }
    }
}

impl std::error::Error for RpcRunnerError {}

impl MachineExecBackend for RpcRunnerBackend {
    fn spawn(&self, request: MachineExecRequest) -> Result<Arc<dyn MachinePendingRun>, String> {
        let endpoint = self.endpoint.clone();
        let cache = Arc::clone(&self.cache);
        let handle = std::thread::spawn(move || {
            run_rpc_request(endpoint, cache, request).map_err(|error| error.to_string())
        });
        Ok(Arc::new(RpcRunnerRun {
            state: Mutex::new(Some(RpcRunnerRunState::Starting(handle))),
        }))
    }
}

struct RpcRunnerRun {
    state: Mutex<Option<RpcRunnerRunState>>,
}

enum RpcRunnerRunState {
    Starting(JoinHandle<Result<(Outcome, ExecEvent), String>>),
    Done(Outcome, ExecEvent),
}

impl MachinePendingRun for RpcRunnerRun {
    fn demand_path(&self, path: &str) -> Result<MachinePathDemand, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "rpc runner run state poisoned".to_string())?;
        if let Some(RpcRunnerRunState::Done(outcome, _)) = &*state {
            if let Some(contents) = outcome.outputs.entries.get(path) {
                return Ok(MachinePathDemand::File(contents.clone()));
            }
            if outcome.outputs.blobs.contains_key(path)
                || has_child(&outcome.outputs.entries, path)
                || has_child(&outcome.outputs.blobs, path)
            {
                return Ok(MachinePathDemand::FinishRequired {
                    path: path.to_string(),
                });
            }
            return Ok(MachinePathDemand::Missing {
                path: path.to_string(),
            });
        }
        Ok(MachinePathDemand::FinishRequired {
            path: path.to_string(),
        })
    }

    fn flush(&self) -> Result<(Outcome, ExecEvent), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "rpc runner run state poisoned".to_string())?;
        match state
            .take()
            .ok_or_else(|| "rpc runner run state missing".to_string())?
        {
            RpcRunnerRunState::Done(outcome, event) => {
                *state = Some(RpcRunnerRunState::Done(outcome.clone(), event.clone()));
                Ok((outcome, event))
            }
            RpcRunnerRunState::Starting(handle) => {
                let (outcome, event) = handle
                    .join()
                    .map_err(|_| "rpc runner thread panicked".to_string())??;
                *state = Some(RpcRunnerRunState::Done(outcome.clone(), event.clone()));
                Ok((outcome, event))
            }
        }
    }
}

fn has_child<T>(entries: &std::collections::BTreeMap<String, T>, path: &str) -> bool {
    let prefix = format!("{path}/");
    entries.keys().any(|entry| entry.starts_with(&prefix))
}

#[tracing::instrument(skip_all, fields(endpoint = %endpoint, command = %request.command))]
fn run_rpc_request(
    endpoint: String,
    cache: Arc<Mutex<ExecCache>>,
    request: MachineExecRequest,
) -> Result<(Outcome, ExecEvent), RpcRunnerError> {
    if let Some((outcome, event)) = {
        let mut cache = cache.lock().map_err(|_| RpcRunnerError::CachePoisoned)?;
        cache
            .lookup(&request.plan, request.capability, &request.mounts)
            .map(|outcome| {
                let event = cache
                    .events
                    .last()
                    .cloned()
                    .expect("lookup pushed an event");
                (outcome, event)
            })
    } {
        return Ok((outcome, event));
    }

    let protocol_request = to_protocol_request(&request);
    let protocol_outcome = call_runner(&endpoint, protocol_request)?;
    let outcome = from_protocol_outcome(protocol_outcome, &request.mounts);
    let mut cache = cache.lock().map_err(|_| RpcRunnerError::CachePoisoned)?;
    cache.record_ran(
        &request.plan,
        request.capability,
        &request.mounts,
        outcome.clone(),
    );
    let event = cache
        .events
        .last()
        .cloned()
        .expect("record_ran pushed an event");
    Ok((outcome, event))
}

fn call_runner(
    endpoint: &str,
    request: protocol::ExecRequest,
) -> Result<protocol::ExecOutcome, RpcRunnerError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| RpcRunnerError::Connect(error.to_string()))?;
    runtime.block_on(async move {
        let client: protocol::RunnerClient = vox::connect_lane(endpoint)
            .await
            .map_err(|error| RpcRunnerError::Connect(error.to_string()))?;
        let capabilities = client
            .capabilities(protocol::CapabilityProbeRequest {})
            .await
            .map_err(|error| RpcRunnerError::Capabilities(error.to_string()))?
            .capabilities;
        validate_capabilities(&capabilities)?;
        let result = client
            .exec(request)
            .await
            .map_err(|error| RpcRunnerError::Protocol(error.to_string()))?;
        match result.completion {
            protocol::ExecCompletion::Succeeded(outcome) => Ok(outcome),
            protocol::ExecCompletion::Failed(error) => Err(RpcRunnerError::from_exec_error(error)),
        }
    })
}

fn validate_capabilities(
    capabilities: &protocol::RunnerCapabilities,
) -> Result<(), RpcRunnerError> {
    let expected = protocol::RUNNER_PROTOCOL_VERSION.0;
    let actual = capabilities.protocol_version.0;
    if actual != expected {
        return Err(RpcRunnerError::UnsupportedProtocol { expected, actual });
    }
    if !capabilities
        .transports
        .contains(&protocol::RunnerTransport::VoxWebsocket)
    {
        return Err(RpcRunnerError::UnsupportedTransport {
            transports: capabilities.transports.clone(),
        });
    }
    Ok(())
}

fn to_protocol_request(request: &MachineExecRequest) -> protocol::ExecRequest {
    protocol::ExecRequest {
        program: request.command.clone().into(),
        plan: protocol::ExecPlan {
            argv: request
                .plan
                .argv
                .iter()
                .map(|(value, role)| protocol::ExecArg {
                    value: value.clone().into(),
                    role: to_protocol_role(*role),
                })
                .collect(),
        },
        mounts: request
            .mounts
            .iter()
            .map(|mount| protocol::ExecMount {
                at: mount.at.clone().into(),
                tree: to_protocol_tree(&mount.tree),
            })
            .collect(),
        toolchain_roots: request
            .plan
            .argv
            .iter()
            .filter(|(_, role)| *role == Role::Executable)
            .map(|(value, _)| protocol::ExecPath(value.clone()))
            .collect(),
    }
}

fn to_protocol_role(role: Role) -> protocol::ExecRole {
    match role {
        Role::Executable => protocol::ExecRole::Executable,
        Role::Input => protocol::ExecRole::Input,
        Role::InputFlag => protocol::ExecRole::InputFlag,
        Role::Output => protocol::ExecRole::Output,
        Role::OutputFlag => protocol::ExecRole::OutputFlag,
        Role::OutputDir => protocol::ExecRole::OutputDir,
        Role::Stdout => protocol::ExecRole::Stdout,
        Role::Env => protocol::ExecRole::Env,
        Role::SearchDir => protocol::ExecRole::SearchDir,
        Role::SearchDirFlag => protocol::ExecRole::SearchDirFlag,
        Role::Flag => protocol::ExecRole::Flag,
    }
}

fn to_protocol_tree(tree: &Tree) -> protocol::ExecTree {
    protocol::ExecTree {
        entries: tree
            .entries
            .iter()
            .map(|(path, contents)| {
                (
                    protocol::ExecPath(path.clone()),
                    protocol::ExecText(contents.clone()),
                )
            })
            .collect(),
        blobs: tree
            .blobs
            .iter()
            .map(|(path, contents)| (protocol::ExecPath(path.clone()), contents.clone()))
            .collect(),
    }
}

fn from_protocol_outcome(outcome: protocol::ExecOutcome, mounts: &[Mount]) -> Outcome {
    let outputs = from_protocol_tree(outcome.outputs);
    let world = MountedWorld::new(mounts);
    Outcome {
        outputs,
        read_set: from_protocol_read_set(outcome.read_set, &world),
        tree_events: outcome
            .tree_events
            .into_iter()
            .map(from_protocol_tree_event)
            .collect(),
    }
}

fn from_protocol_tree(tree: protocol::ExecTree) -> Tree {
    Tree {
        entries: tree
            .entries
            .into_iter()
            .map(|(path, contents)| (path.0, contents.0))
            .collect(),
        blobs: tree
            .blobs
            .into_iter()
            .map(|(path, contents)| (path.0, contents))
            .collect(),
    }
}

fn from_protocol_read_set(read_set: protocol::ExecReadSet, world: &MountedWorld<'_>) -> ReadSet {
    let mut entries = std::collections::BTreeMap::new();
    for (path, observation) in read_set.entries {
        let path = path.0;
        match observation {
            protocol::ExecReadObservation::File { content_hash, .. } => {
                if let Some(observation) = local_file_observation(&path, world) {
                    entries.insert(path, observation);
                } else {
                    entries.insert(
                        path,
                        ReadObservation::Content(crate::exec::Blake3Hash::from(content_hash.0.0)),
                    );
                }
            }
            protocol::ExecReadObservation::Directory { directory_node } => {
                if let Some((path, observation)) = local_directory_observation(&path, world) {
                    entries.insert(path, observation);
                } else {
                    entries.insert(
                        format!("{path}/"),
                        ReadObservation::Listing(crate::exec::Blake3Hash::from(directory_node.0.0)),
                    );
                }
            }
            protocol::ExecReadObservation::LookupMiss { .. } => {
                entries.insert(path, ReadObservation::Absent);
            }
        }
    }
    ReadSet { entries }
}

fn local_file_observation(path: &str, world: &MountedWorld<'_>) -> Option<ReadObservation> {
    let mut observed = crate::exec::ObservedWorld::new(world);
    observed.read_bytes(path)?;
    observed.into_read_set().entries.remove(path)
}

fn local_directory_observation(
    path: &str,
    world: &MountedWorld<'_>,
) -> Option<(String, ReadObservation)> {
    let mut observed = crate::exec::ObservedWorld::new(world);
    observed.list(path)?;
    let key = format!("{path}/");
    observed.into_read_set().entries.remove_entry(&key)
}

fn from_protocol_tree_event(event: protocol::ExecTreeEvent) -> TreeEvent {
    match event {
        protocol::ExecTreeEvent::SubfileCompleted(completion) => {
            TreeEvent::SubfileCompleted(from_protocol_file_completion(completion))
        }
        protocol::ExecTreeEvent::TreeFinalized(finalization) => TreeEvent::TreeFinalized {
            files: finalization
                .files
                .into_iter()
                .map(from_protocol_file_completion)
                .collect(),
        },
    }
}

fn from_protocol_file_completion(
    completion: protocol::ExecSubfileCompletion,
) -> TreeFileCompletion {
    TreeFileCompletion {
        path: completion.path.0,
        content_hash: crate::exec::Blake3Hash::from(completion.content_hash.0.0),
        size: completion.size.0,
    }
}
