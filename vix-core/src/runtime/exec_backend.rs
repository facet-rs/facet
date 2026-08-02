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

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::EXEC_MOUNT_ROOT;

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
    /// Input trees to materialize into the workspace before spawning. Each
    /// mount's `path` is workspace-relative and already named by the argv, so a
    /// backend writes them where it is told and invents nothing.
    pub mounts: Vec<ExecMount>,
}

/// One input tree, flattened to its files, destined for a workspace-relative
/// directory. This is a plain file list rather than a tree value on purpose:
/// materializing an input is a filesystem act, and the backend is the only
/// thing that should know a filesystem exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecMount {
    /// Workspace-relative directory the files land under.
    pub path: String,
    /// The tree's files, in tree order.
    pub files: Vec<ExecMountFile>,
}

/// One materialized file of a mount. The executable bit is carried because a
/// tree carries it (`TreeEntry::File::executable`) and dropping it turns a
/// compiler's output into a file the next stage cannot run — the mode is part
/// of what the value says about itself, not a filesystem detail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecMountFile {
    /// Tree-relative path.
    pub path: String,
    pub bytes: Vec<u8>,
    pub executable: bool,
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
    /// One byte-stream extension: `bytes` begins at byte `offset` of the named
    /// response stream, exactly as the process produced them (output-protocol
    /// lines are carved out of the logical stream before offsets are
    /// assigned). The chunking is transport framing — how the reader happened
    /// to observe the bytes — never identity
    /// (`machine.primitive.exec-outcome`).
    Stream {
        stream: &'static str,
        offset: u64,
        bytes: Vec<u8>,
    },
    /// The process terminated (or the boundary failed before termination).
    Terminated(Result<std::process::Output, String>),
}

/// The exec response's stdout stream name, as both the outcome record field
/// and the byte-stream extension address it.
pub const EXEC_STDOUT_STREAM: &str = "stdout";

/// The exec response's stderr stream name.
pub const EXEC_STDERR_STREAM: &str = "stderr";

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

/// Capture a completed workspace as ustar archive bytes — the raw form the exec
/// outcome's canonical `Tree` is derived from.
///
/// This is the one piece of host filesystem code still in `vix-core`, and it is
/// here because the exec PRIMITIVE calls it (`vixen-primitives` cannot reach
/// `vixen-runtime`, where the backend lives). A confining backend would want to
/// capture its own workspace its own way, which is the argument for moving
/// capture behind the [`ExecBackend`] trait — that changes the trait's shape and
/// is deliberately not bundled with the backend's relocation.
pub fn archive_directory(root: &Path) -> Result<Vec<u8>, String> {
    fn collect(
        directory: &Path,
        root: &Path,
        files: &mut Vec<(PathBuf, bool)>,
    ) -> Result<(), String> {
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
            // Inputs are not outputs. Mounts live under one reserved top-level
            // name precisely so capture can drop them here: without this, stage
            // N's tree carries stage N-1's sources, mounting it into stage N+1
            // nests them again, and a chain grows quadratically until it hits
            // the ustar path cap. It would also give two byte-identical
            // products different identities depending on what they were built
            // from, which is the opposite of what an output identity is for.
            if directory == root && path.file_name().is_some_and(|name| name == EXEC_MOUNT_ROOT) {
                continue;
            }
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|error| format!("inspect exec output `{}`: {error}", path.display()))?;
            if metadata.file_type().is_symlink() {
                return Err(format!(
                    "exec output symlink `{}` is not yet supported",
                    path.display()
                ));
            }
            if metadata.is_dir() {
                collect(&path, root, files)?;
            } else if metadata.is_file() {
                // Executability is read from the metadata already in hand.
                // Only the two canonical modes are representable, because the
                // tree model carries a bool, not a mode word — inventing finer
                // permissions would claim an identity the value cannot hold.
                #[cfg(unix)]
                let executable = {
                    use std::os::unix::fs::PermissionsExt as _;
                    // Executable by ANYONE, not just the owner: the tree model
                    // carries a bool, so the question is "is this runnable", and a
                    // mode like 0o605 is.
                    metadata.permissions().mode() & 0o111 != 0
                };
                #[cfg(not(unix))]
                let executable = false;
                files.push((path, executable));
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

    let mut files: Vec<(PathBuf, bool)> = Vec::new();
    collect(root, root, &mut files)?;
    files.sort();
    let mut archive = Vec::new();
    for (file, executable) in files {
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
        header[100..108].copy_from_slice(if executable {
            b"0000755\0"
        } else {
            b"0000644\0"
        });
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
