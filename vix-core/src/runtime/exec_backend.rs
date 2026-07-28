//! The exec process boundary as an explicit service (`exec-rail.md`, move 2).
//!
//! An authority-crossing backend is a [`PrimitiveServices`](super::PrimitiveServices)
//! entry, never scheduler code: the backend owns spawn, stream, and wait, while
//! the scheduler keeps only what `machine.primitive.registered` permits it —
//! keying, parking, admission, and receipts. The demand key never mentions the
//! backend: identity is "this plan under this capability" now and forever, so a
//! confining backend (`machine.primitive.exec-hermetic-traps`) replaces the
//! host-trusting one behind the same capability identity without re-keying any
//! demand — enforcement upgrades the service and the witness quality, not the
//! memo.
//!
//! r[impl machine.primitive.effect-backend-service]

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// The output protocol the backend streams a command under. Which protocol
/// applies is a property of the capability package
/// (`machine.primitive.command-package`); the backend merely enacts it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecOutputProtocol {
    /// No progressive vocabulary: stdout is opaque bytes until termination.
    ExitOnly,
    /// `vix-ready\t<path>` stdout lines announce immutable products while the
    /// process runs; every other line is ordinary output.
    ProgressiveLinesV1,
}

/// One exec invocation as the backend receives it: the resolved program, the
/// materialized argv, and the output protocol. Everything identity-bearing
/// (plan recipe, capability identity) stays scheduler-side — the backend
/// executes, it never keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecInvocation {
    pub program: String,
    pub argv: Vec<String>,
    pub protocol: ExecOutputProtocol,
}

/// One command-grammar-authorized immutable exec product, snapshotted by the
/// backend the moment the output protocol announces it. Only these bytes cross
/// to the scheduler — never a filesystem readiness guess.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecProduct {
    pub path: String,
    pub bytes: Vec<u8>,
}

/// One event the backend delivers across the scheduler's completion boundary.
/// Termination carries the raw process output; the scheduler thread alone
/// interns and maps it (`machine.primitive.exit-status-is-not-a-value`).
pub enum ExecEvent {
    /// A progressive product announcement (or its per-product read failure).
    Product(Result<ExecProduct, String>),
    /// The process terminated (or the boundary failed before termination).
    Terminated(Result<std::process::Output, String>),
}

/// The backend's one delivery authority: a `Send + Sync` sender the scheduler
/// wraps around its unified completion inbox. The backend may call it from any
/// worker thread; the scheduler alone consumes the events
/// (`machine.scheduler.block-on-event`).
pub type ExecEventSender = Arc<dyn Fn(ExecEvent) + Send + Sync>;

/// A fresh, empty working directory the spawned process runs in and writes its
/// output tree into. The backend creates it; the scheduler archives it at
/// termination and drops it afterwards (removal rides `Drop`).
pub struct ExecWorkspace {
    path: PathBuf,
}

impl ExecWorkspace {
    pub fn create() -> Result<Self, String> {
        static NEXT_WORKSPACE: AtomicU64 = AtomicU64::new(0);
        let ordinal = NEXT_WORKSPACE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("vix-exec-{}-{ordinal}", std::process::id()));
        std::fs::create_dir(&path)
            .map_err(|error| format!("create exec workspace `{}`: {error}", path.display()))?;
        Ok(Self { path })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ExecWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The exec backend service: owns the process boundary — workspace, spawn,
/// stream, wait — and delivers every outcome through the given sender. `begin`
/// returns once the invocation is launched; a synchronous `Err` is a boundary
/// failure before any event could flow (workspace or spawn failure).
///
/// r[impl machine.primitive.effect-backend-service]
pub trait ExecBackend: Send + Sync {
    fn begin(
        &self,
        invocation: ExecInvocation,
        events: ExecEventSender,
    ) -> Result<ExecWorkspace, String>;
}

/// The default backend: the current behavior, verbatim — `std::process::Command`
/// in a fresh workspace, explicitly HOST-TRUSTING. It interposes no VFS and
/// witnesses no ambient read, so per `machine.primitive.memo-policy` it
/// supports no `Hermetic` claim: the scheduler records its capability witness
/// as `ReadObservation::Unverifiable`. A confining backend replaces this one
/// behind the same demand keys.
pub struct HostExecBackend;

impl ExecBackend for HostExecBackend {
    fn begin(
        &self,
        invocation: ExecInvocation,
        events: ExecEventSender,
    ) -> Result<ExecWorkspace, String> {
        let ExecInvocation {
            program,
            argv,
            protocol,
        } = invocation;
        let workspace = ExecWorkspace::create()?;
        let mut child = std::process::Command::new(&program)
            .args(&argv)
            .current_dir(workspace.path())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|error| format!("spawn `{program}`: {error}"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| format!("`{program}` stdout was not piped"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| format!("`{program}` stderr was not piped"))?;
        let workspace_path = workspace.path().to_path_buf();
        // `wait_with_output` lives ONLY inside this worker-thread closure; the
        // scheduler thread never waits on the process boundary.
        std::thread::spawn(move || {
            let progress_events = events.clone();
            let stdout_reader = std::thread::spawn(move || {
                read_exec_stdout(stdout, protocol, &workspace_path, &progress_events)
            });
            let worker_program = program.clone();
            let stderr_reader = std::thread::spawn(move || {
                let mut bytes = Vec::new();
                let mut stderr = stderr;
                stderr
                    .read_to_end(&mut bytes)
                    .map_err(|error| format!("read `{worker_program}` stderr: {error}"))?;
                Ok::<_, String>(bytes)
            });
            let output = (|| {
                let status = child
                    .wait()
                    .map_err(|error| format!("wait `{program}`: {error}"))?;
                let stdout = stdout_reader
                    .join()
                    .map_err(|_| format!("read `{program}` stdout worker panicked"))??;
                let stderr = stderr_reader
                    .join()
                    .map_err(|_| format!("read `{program}` stderr worker panicked"))??;
                Ok(std::process::Output {
                    status,
                    stdout,
                    stderr,
                })
            })();
            (*events)(ExecEvent::Terminated(output));
        });
        Ok(workspace)
    }
}

fn read_exec_stdout(
    stdout: impl Read,
    protocol: ExecOutputProtocol,
    workspace: &Path,
    events: &ExecEventSender,
) -> Result<Vec<u8>, String> {
    const READY_PREFIX: &[u8] = b"vix-ready\t";
    let mut reader = BufReader::new(stdout);
    let mut output = Vec::new();
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = reader
            .read_until(b'\n', &mut line)
            .map_err(|error| format!("read exec stdout: {error}"))?;
        if read == 0 {
            break;
        }
        if protocol == ExecOutputProtocol::ProgressiveLinesV1 && line.starts_with(READY_PREFIX) {
            let mut path = &line[READY_PREFIX.len()..];
            if path.ends_with(b"\n") {
                path = &path[..path.len() - 1];
            }
            if path.ends_with(b"\r") {
                path = &path[..path.len() - 1];
            }
            let path = core::str::from_utf8(path)
                .map_err(|_| "progressive exec path was not UTF-8".to_owned())?;
            validate_exec_product_path(path)?;
            let bytes = std::fs::read(workspace.join(path)).map_err(|error| {
                format!("read progressive exec product `{path}` after readiness: {error}")
            });
            (*events)(ExecEvent::Product(bytes.map(|bytes| ExecProduct {
                path: path.to_owned(),
                bytes,
            })));
        } else {
            output.extend_from_slice(&line);
        }
    }
    Ok(output)
}

pub(crate) fn validate_exec_product_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("progressive exec product path was empty".to_owned());
    }
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(format!(
            "progressive exec product `{}` was not a relative normal path",
            path.display()
        ));
    }
    Ok(())
}
