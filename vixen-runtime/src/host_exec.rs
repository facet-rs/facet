//! The host exec backend: `std::process::Command`, explicitly chosen.
//!
//! This is the ASSEMBLY's decision, not the language's. `vix-core` declares the
//! [`ExecBackend`] seam and ships no implementation of it — the same way it
//! declares primitives and ships none, and declares host types and ships none.
//! An effect snapshot with no backend installed cannot cross the process
//! boundary at all: `EffectCtx::exec_backend` fails loudly rather than reaching
//! for `std::process` itself.
//!
//! It used to *look* injected while a default in `PrimitiveServices` quietly
//! substituted this backend for anyone who installed nothing, which meant the
//! machine — not the embedder — decided that vix spawns host processes. The
//! default is gone; every caller that wants a process boundary now says so.
//! [`vixen_runtime::ratchet::harness_services`](crate::ratchet) says so for the
//! test harness, and `vx` says so for the CLI.
//!
//! It is HOST-TRUSTING and declared as such: it interposes no VFS and witnesses
//! no ambient read, so per `machine.primitive.memo-policy` it supports no
//! `Hermetic` claim and the scheduler records its capability witness as
//! `ReadObservation::Unverifiable`. A confining backend (a nix-style sandbox —
//! `machine.primitive.exec-hermetic-traps`) replaces it behind the same demand
//! keys: nothing keys on the backend, so swapping it never re-keys a demand, it
//! changes what a witness is worth.
//!
//! r[impl machine.primitive.effect-backend-service]

use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use vix::runtime::{
    EXEC_STDERR_STREAM, EXEC_STDOUT_STREAM, ExecBackend, ExecEvent, ExecEventSender,
    ExecInvocation, ExecMount, ExecOutputProtocol, ExecProduct, ExecWorkspace,
};

/// The host-trusting backend, EXPLICITLY INSTALLED — `std::process::Command` in
/// a fresh workspace. It is not a default and there is no longer one to be:
/// `vix-core` ships no backend, and an embedder that wants a process boundary
/// names this one (see the module docs above). It interposes no VFS and
/// witnesses no ambient read, so per `machine.primitive.memo-policy` it
/// supports no `Hermetic` claim: the scheduler records its capability witness
/// as `ReadObservation::Unverifiable`. A confining backend replaces this one
/// behind the same demand keys.
pub struct HostExecBackend;

/// Write every mount's files under the workspace. A mount path that escapes the
/// workspace is refused rather than normalized: the paths are derived from the
/// plan (`exec_mount_path`) and a tree's own member names, so an escape means
/// something upstream is wrong and silently clamping it would hide that.
fn materialize_mounts(workspace: &Path, mounts: &[ExecMount]) -> Result<(), String> {
    for mount in mounts {
        // The mount's own directory is created even when the tree holds no
        // files. The argv names this path unconditionally, so an empty tree
        // (an exec that wrote nothing, captured and mounted) would otherwise
        // hand the process an ENOENT where the model promises a directory.
        let root = workspace.join(&mount.path);
        std::fs::create_dir_all(&root)
            .map_err(|error| format!("create `{}`: {error}", root.display()))?;
        for file in &mount.files {
            let joined = format!("{}/{}", mount.path, file.path);
            // Validate through `Path::components`, not by splitting on '/':
            // `Path::join` honours the platform's separators, so a member named
            // `..\\evil` would slip a '/'-only check and escape on Windows. The
            // same discipline `validate_exec_product_path` uses below — a member
            // name is untrusted input, and the paths are otherwise derived from
            // the plan, so an escape means something upstream is wrong and
            // silently normalizing it would hide that.
            if Path::new(&joined)
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
            {
                return Err(format!("mount path `{joined}` escapes the workspace"));
            }
            let target = workspace.join(&joined);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create `{}`: {error}", parent.display()))?;
            }
            std::fs::write(&target, &file.bytes)
                .map_err(|error| format!("write `{}`: {error}", target.display()))?;
            #[cfg(unix)]
            if file.executable {
                use std::os::unix::fs::PermissionsExt as _;
                std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))
                    .map_err(|error| format!("chmod `{}`: {error}", target.display()))?;
            }
        }
    }
    Ok(())
}

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
            mounts,
        } = invocation;
        let workspace = ExecWorkspace::create()?;
        materialize_mounts(workspace.path(), &mounts)?;
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
            let stderr_events = events.clone();
            let stderr_reader = std::thread::spawn(move || {
                let mut bytes = Vec::new();
                let mut stderr = stderr;
                let mut chunk = [0u8; 8192];
                loop {
                    let read = chunk_read(&mut stderr, &mut chunk)
                        .map_err(|error| format!("read `{worker_program}` stderr: {error}"))?;
                    if read == 0 {
                        break;
                    }
                    (*stderr_events)(ExecEvent::Stream {
                        stream: EXEC_STDERR_STREAM,
                        offset: bytes.len() as u64,
                        bytes: chunk[..read].to_vec(),
                    });
                    bytes.extend_from_slice(&chunk[..read]);
                }
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
            (*events)(ExecEvent::Stream {
                stream: EXEC_STDOUT_STREAM,
                offset: output.len() as u64,
                bytes: line.clone(),
            });
            output.extend_from_slice(&line);
        }
    }
    Ok(output)
}

/// One `read` observation, retried through interruptions. Kept out of the
/// reader loops so both streams share the identical transport discipline.
fn chunk_read(source: &mut impl Read, chunk: &mut [u8]) -> std::io::Result<usize> {
    loop {
        match source.read(chunk) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            outcome => return outcome,
        }
    }
}

/// A protocol-announced product path must be relative and normal: the backend
/// snapshots it from inside the workspace, so an absolute path or a `..`
/// component would read a file the process was never granted.
fn validate_exec_product_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("progressive exec product path was empty".to_owned());
    }
    let path = Path::new(path);
    // The mount area is INPUT. Announcing a product from it would replay an
    // input as an output — and capture excludes that area, so the two halves of
    // the protocol would disagree about the same bytes.
    if path
        .components()
        .next()
        .is_some_and(|first| first.as_os_str() == vix::runtime::EXEC_MOUNT_ROOT)
    {
        return Err(format!(
            "progressive exec product `{}` is inside the reserved input-mount area",
            path.display()
        ));
    }
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
