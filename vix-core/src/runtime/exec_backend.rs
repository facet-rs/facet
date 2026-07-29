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
/// materialized argv, the environment assignments the command grammar carved
/// out of the plan, and the output protocol. Everything identity-bearing
/// (plan recipe, capability identity) stays scheduler-side — the backend
/// executes, it never keys. `env` is generic process-boundary vocabulary:
/// WHICH plan elements are environment roles is the capability package's
/// command grammar (`machine.capability.no-argv-dialect`), decided before the
/// invocation reaches any backend; the backend only applies the assignments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecInvocation {
    pub program: String,
    pub argv: Vec<String>,
    /// Ambient variables whose names carry declared command-grammar roles.
    /// They are removed before explicit assignments are applied so the host
    /// process environment cannot add an unkeyed target requirement.
    pub env_remove: Vec<String>,
    /// Environment assignments applied on top of the (host-trusting) ambient
    /// environment, in plan order.
    pub env: Vec<(String, String)>,
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
            env_remove,
            env,
            protocol,
        } = invocation;
        let workspace = ExecWorkspace::create()?;
        let mut command = std::process::Command::new(&program);
        command.args(&argv);
        for name in env_remove {
            command.env_remove(name);
        }
        let mut child = command
            .envs(env)
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

/// Capture a completed workspace as ustar archive bytes — the raw form the exec
/// outcome's canonical `Tree` is derived from. This is domain code the backend
/// side of the process boundary owns (the scheduler never touches the
/// filesystem); it lives here beside the workspace it captures.
pub fn archive_directory(root: &Path) -> Result<Vec<u8>, String> {
    fn collect(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
        let mut entries = std::fs::read_dir(directory)
            .map_err(|error| {
                format!(
                    "read exec output directory `{}`: {error}",
                    directory.display()
                )
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("read exec output entry: {error}"))?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|error| format!("inspect exec output `{}`: {error}", path.display()))?;
            if metadata.file_type().is_symlink() {
                return Err(format!(
                    "exec output symlink `{}` is not yet supported",
                    path.display()
                ));
            }
            if metadata.is_dir() {
                collect(&path, files)?;
            } else if metadata.is_file() {
                files.push(path);
            }
        }
        Ok(())
    }

    fn write_octal(dst: &mut [u8], value: u64) -> Result<(), String> {
        let width = dst
            .len()
            .checked_sub(1)
            .ok_or_else(|| "ustar octal field was empty".to_owned())?;
        let text = format!("{value:0width$o}\0");
        if text.len() != dst.len() {
            return Err(format!(
                "ustar value {value} overflowed {} bytes",
                dst.len()
            ));
        }
        dst.copy_from_slice(text.as_bytes());
        Ok(())
    }

    let mut files = Vec::new();
    collect(root, &mut files)?;
    files.sort();
    let mut archive = Vec::new();
    for file in files {
        let relative = file
            .strip_prefix(root)
            .map_err(|_| format!("exec output `{}` escaped its workspace", file.display()))?;
        let relative = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if relative.len() > 100 {
            return Err(format!("exec output path `{relative}` exceeds ustar v1"));
        }
        let bytes = std::fs::read(&file)
            .map_err(|error| format!("read exec output `{}`: {error}", file.display()))?;
        let mut header = [0u8; 512];
        header[..relative.len()].copy_from_slice(relative.as_bytes());
        header[100..108].copy_from_slice(b"0000644\0");
        header[108..116].copy_from_slice(b"0000000\0");
        header[116..124].copy_from_slice(b"0000000\0");
        write_octal(&mut header[124..136], bytes.len() as u64)?;
        header[136..148].copy_from_slice(b"00000000000\0");
        header[148..156].fill(b' ');
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        let checksum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
        let checksum = format!("{checksum:06o}\0 ");
        header[148..156].copy_from_slice(checksum.as_bytes());
        archive.extend_from_slice(&header);
        archive.extend_from_slice(&bytes);
        archive.resize(archive.len().div_ceil(512) * 512, 0);
    }
    archive.resize(archive.len() + 1024, 0);
    Ok(archive)
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
