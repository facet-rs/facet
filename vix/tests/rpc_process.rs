#![cfg(all(feature = "runner-rpc", not(target_arch = "wasm32")))]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use exec_protocol::{
    Blake3Hash, ByteLen, CapabilityProbeRequest, CapabilityProbeResult, ContentHash,
    ExecCompletion, ExecDiagnostic, ExecError, ExecErrorKind, ExecExitCode, ExecOutcome, ExecPath,
    ExecReadObservation, ExecReadSet, ExecResult, ExecSubfileCompletion, ExecTree, ExecTreeEvent,
    NodeHash, PlatformArch, PlatformName, Runner, RunnerCapabilities, RunnerDispatcher,
    RunnerPlatform, RunnerTransport, ToolchainCapability, ToolchainKind,
};
use vix::exec::{ExecEvent, ExecPlan, Mount, ReadObservation, Role, Tree, TreeEvent};
use vix::machine::{MachineExecBackend, MachineExecRequest, MachinePathDemand};
use vix::rpc_process::RpcRunnerBackend;

#[derive(Clone)]
struct FakeRunner {
    exec_calls: Arc<AtomicUsize>,
}

impl FakeRunner {
    fn new() -> Self {
        Self {
            exec_calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl Runner for FakeRunner {
    async fn exec(&self, request: exec_protocol::ExecRequest) -> ExecResult {
        self.exec_calls.fetch_add(1, Ordering::SeqCst);
        if request.program.as_str() == "fail" {
            return ExecResult {
                exit_code: ExecExitCode(1),
                stdout: Vec::new(),
                stderr: b"undeclared read".to_vec(),
                completion: ExecCompletion::Failed(ExecError {
                    kind: ExecErrorKind::Verification,
                    diagnostic: ExecDiagnostic("undeclared read /secret.txt".to_string()),
                }),
            };
        }

        let mut outputs = ExecTree::default();
        outputs.insert_bytes("out.txt", b"built by fake runner".to_vec());

        let input_hash = ContentHash(Blake3Hash::from_bytes(b"input"));
        let dir_hash = NodeHash(Blake3Hash::from_bytes(b"dir:/src/include"));
        let mut read_set = ExecReadSet::default();
        read_set.entries.insert(
            ExecPath("/src/input.txt".to_string()),
            ExecReadObservation::File {
                content_hash: input_hash,
                blob_node: None,
                size: ByteLen(5),
            },
        );
        read_set.entries.insert(
            ExecPath("/src/include".to_string()),
            ExecReadObservation::Directory {
                directory_node: dir_hash,
            },
        );
        read_set.entries.insert(
            ExecPath("/src/missing.h".to_string()),
            ExecReadObservation::LookupMiss {
                parent_path: ExecPath("/src".to_string()),
                directory_node: NodeHash(Blake3Hash::from_bytes(b"dir:/src")),
            },
        );

        let completed = ExecSubfileCompletion {
            path: ExecPath("out.txt".to_string()),
            content_hash: ContentHash(Blake3Hash::from_bytes(b"built by fake runner")),
            size: ByteLen(20),
        };

        ExecResult {
            exit_code: ExecExitCode(0),
            stdout: Vec::new(),
            stderr: Vec::new(),
            completion: ExecCompletion::Succeeded(ExecOutcome {
                outputs,
                read_set,
                observation_scopes: Vec::new(),
                tree_events: vec![
                    ExecTreeEvent::SubfileCompleted(completed.clone()),
                    ExecTreeEvent::TreeFinalized(exec_protocol::ExecTreeFinalization {
                        files: vec![completed],
                    }),
                ],
            }),
        }
    }

    async fn capabilities(&self, _request: CapabilityProbeRequest) -> CapabilityProbeResult {
        CapabilityProbeResult {
            capabilities: RunnerCapabilities {
                protocol_version: exec_protocol::RUNNER_PROTOCOL_VERSION,
                platform: RunnerPlatform {
                    os: PlatformName(std::env::consts::OS.to_string()),
                    arch: PlatformArch(std::env::consts::ARCH.to_string()),
                },
                transports: vec![RunnerTransport::VoxWebsocket],
                sandboxes: Vec::new(),
                toolchains: vec![ToolchainCapability {
                    kind: ToolchainKind::Shell,
                    executable: ExecPath("fake".to_string()),
                    content_hash: None,
                }],
            },
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rpc_runner_backend_round_trips_over_vox_websocket() -> Result<(), String> {
    let runner = FakeRunner::new();
    let calls = Arc::clone(&runner.exec_calls);
    let (endpoint, server) = serve_fake_runner(runner).await?;

    let backend = Arc::new(RpcRunnerBackend::new(endpoint));
    let request = exec_request("fake");
    let pending = backend.spawn(request.clone())?;
    let (outcome, event) = pending.flush()?;
    assert_eq!(event, ExecEvent::Ran);
    assert_eq!(
        pending.demand_path("out.txt")?,
        MachinePathDemand::File("built by fake runner".to_string())
    );
    assert_eq!(
        outcome.outputs.entries.get("out.txt").map(String::as_str),
        Some("built by fake runner")
    );
    assert!(matches!(
        outcome.read_set.entries.get("/src/input.txt"),
        Some(ReadObservation::Content(_))
    ));
    assert!(matches!(
        outcome.read_set.entries.get("/src/include/"),
        Some(ReadObservation::Listing(_))
    ));
    assert_eq!(
        outcome.read_set.entries.get("/src/missing.h"),
        Some(&ReadObservation::Absent)
    );
    assert!(matches!(
        outcome.tree_events.as_slice(),
        [
            TreeEvent::SubfileCompleted(completed),
            TreeEvent::TreeFinalized { files }
        ] if completed.path == "out.txt" && files.len() == 1
    ));

    let warm = backend.spawn(request)?;
    let (_, warm_event) = warm.flush()?;
    assert_eq!(warm_event, ExecEvent::Tier1Hit);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    server.abort();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rpc_runner_backend_maps_typed_failures() -> Result<(), String> {
    let runner = FakeRunner::new();
    let (endpoint, server) = serve_fake_runner(runner).await?;

    let backend = Arc::new(RpcRunnerBackend::new(endpoint));
    let err = backend
        .spawn(exec_request("fail"))?
        .flush()
        .expect_err("runner verification failure should be loud");
    assert_eq!(
        err,
        "rpc runner verification failure: undeclared read /secret.txt"
    );

    server.abort();
    Ok(())
}

async fn serve_fake_runner(
    runner: FakeRunner,
) -> Result<(String, tokio::task::JoinHandle<()>), String> {
    let listener = vox::WsListener::bind("127.0.0.1:0")
        .await
        .map_err(|err| format!("bind fake runner: {err}"))?;
    let endpoint = format!(
        "ws://{}",
        listener
            .local_addr()
            .map_err(|err| format!("fake runner addr: {err}"))?
    );
    let server = tokio::spawn(async move {
        vox::serve_listener(listener, RunnerDispatcher::new(runner))
            .await
            .expect("serve fake runner");
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    Ok((endpoint, server))
}

fn exec_request(command: &str) -> MachineExecRequest {
    MachineExecRequest {
        command: command.to_string(),
        plan: ExecPlan {
            argv: vec![
                ("fake".to_string(), Role::Executable),
                ("/src/input.txt".to_string(), Role::Input),
                ("/src/include".to_string(), Role::SearchDir),
                ("out.txt".to_string(), Role::Output),
            ],
        },
        capability: 7,
        mounts: vec![Mount {
            at: "/src".to_string(),
            tree: Tree::of(&[
                ("input.txt", "input"),
                ("include/header.h", "int header(void);"),
            ]),
        }],
        output: "out.txt".to_string(),
        span: None,
        observer: None,
    }
}
