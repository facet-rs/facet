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

/// One input tree, flattened to its entries, destined for a workspace-relative
/// directory. This is a plain entry list rather than a tree value on purpose:
/// materializing an input is a filesystem act, and the backend is the only
/// thing that should know a filesystem exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecMount {
    /// Workspace-relative directory the entries land under.
    pub path: String,
    /// The tree's entries, in tree order.
    pub entries: Vec<ExecMountEntry>,
}

/// One materialized entry of a mount — EVERY entry kind a `TreeEntry` has, not
/// just files.
///
/// Directories and symlinks are carried because they participate in tree
/// identity: an empty directory is explicitly representable in the tree model
/// (it is how one process hands structure to a later one), and a symlink's
/// target is preserved verbatim. Flattening to files alone would mount a tree
/// that is not the value the request named — a silent difference between what
/// the identity says and what the process sees.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecMountEntry {
    /// The executable bit is carried because a tree carries it
    /// (`TreeEntry::File::executable`) and dropping it turns a compiler's
    /// output into a file the next stage cannot run.
    File {
        path: String,
        bytes: Vec<u8>,
        executable: bool,
    },
    Dir {
        path: String,
    },
    Symlink {
        path: String,
        target: String,
    },
}

impl ExecMountEntry {
    /// The entry's tree-relative path, whatever its kind.
    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            Self::File { path, .. } | Self::Dir { path } | Self::Symlink { path, .. } => path,
        }
    }
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
pub fn archive_directory(root: &Path, mount_count: usize) -> Result<Vec<u8>, String> {
    /// One captured entry: everything the tree model can hold, so a workspace
    /// round-trips to the value it actually is. Files-only capture lost empty
    /// directories (which the tree model represents deliberately — it is how one
    /// process hands structure to a later one) and refused symlinks outright.
    enum Captured {
        File { path: PathBuf, executable: bool },
        Dir { path: PathBuf },
        Symlink { path: PathBuf, target: String },
    }

    impl Captured {
        fn path(&self) -> &Path {
            match self {
                Self::File { path, .. } | Self::Dir { path } | Self::Symlink { path, .. } => path,
            }
        }
    }

    fn collect(
        directory: &Path,
        root: &Path,
        captured: &mut Vec<Captured>,
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
            //
            // KNOWN AND BOUNDED: this drops the whole reserved subtree, not just
            // what was materialized into it. A process that mounted something
            // AND wrote new files under `.vix-mounts/` loses those files with no
            // diagnostic — the same silent-loss class the zero-mount refusal
            // below exists to prevent, and this walk cannot tell the two apart
            // without the materialized manifest to compare against. The
            // reservation is what makes it defensible: `.vix-mounts` is not a
            // name a program may write, and the zero-mount case (where the
            // process could not have known that) IS refused. Closing it properly
            // means threading the materialized file list into capture.
            if directory == root && path.file_name().is_some_and(|name| name == EXEC_MOUNT_ROOT) {
                continue;
            }
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|error| format!("inspect exec output `{}`: {error}", path.display()))?;
            if metadata.file_type().is_symlink() {
                // The target is preserved VERBATIM — dangling targets and `..`
                // are representable, and resolution is the materializer's
                // problem, not the value's
                // (`machine.identity.tree-canonicalization`).
                let target = std::fs::read_link(&path)
                    .map_err(|error| format!("read link `{}`: {error}", path.display()))?;
                let target = target
                    .to_str()
                    .ok_or_else(|| {
                        format!("exec output symlink `{}` has a non-UTF-8 target", path.display())
                    })?
                    .to_owned();
                captured.push(Captured::Symlink { path, target });
            } else if metadata.is_dir() {
                // The directory is recorded in its own right, not merely implied
                // by what is under it, so an EMPTY one survives capture.
                captured.push(Captured::Dir { path: path.clone() });
                collect(&path, root, captured)?;
            } else if metadata.is_file() {
                // Executability is read from the metadata already in hand.
                // Only the two canonical modes are representable, because the
                // tree model carries a bool, not a mode word — inventing finer
                // permissions would claim an identity the value cannot hold.
                #[cfg(unix)]
                let executable = {
                    use std::os::unix::fs::PermissionsExt as _;
                    // Executable by ANYONE, not just the owner: the tree model
                    // carries a bool, so the question is "is this runnable", and
                    // a mode like 0o605 is.
                    metadata.permissions().mode() & 0o111 != 0
                };
                #[cfg(not(unix))]
                let executable = false;
                captured.push(Captured::File { path, executable });
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

    // A process that mounted nothing and wrote a top-level `.vix-mounts` is
    // naming the reservation itself. The skip in the walk cannot tell that
    // output from an input area, and silently dropping it would give two
    // different runs the same output identity — refuse where the cause is
    // legible instead.
    if mount_count == 0 {
        match std::fs::symlink_metadata(root.join(EXEC_MOUNT_ROOT)) {
            Ok(_) => {
                return Err(format!(
                    "`{EXEC_MOUNT_ROOT}` is reserved for exec input mounts; this invocation \
                     mounted nothing, so a top-level entry by that name is an output the \
                     capture cannot represent"
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "inspect reserved exec mount path `{}`: {error}",
                    root.join(EXEC_MOUNT_ROOT).display()
                ));
            }
        }
    }
    let mut captured: Vec<Captured> = Vec::new();
    collect(root, root, &mut captured)?;
    captured.sort_by(|left, right| left.path().cmp(right.path()));
    let mut archive = Vec::new();
    for entry in captured {
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|_| format!("exec output `{}` escaped its workspace", entry.path().display()))?;
        let relative = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if relative.len() > 100 {
            return Err(format!("exec output path `{relative}` exceeds ustar v1"));
        }
        let bytes = match &entry {
            Captured::File { path, .. } => std::fs::read(path)
                .map_err(|error| format!("read exec output `{}`: {error}", path.display()))?,
            Captured::Dir { .. } | Captured::Symlink { .. } => Vec::new(),
        };
        let mut header = [0u8; 512];
        header[..relative.len()].copy_from_slice(relative.as_bytes());
        header[100..108].copy_from_slice(match &entry {
            Captured::File { executable: true, .. } => b"0000755\0",
            Captured::File { .. } => b"0000644\0",
            Captured::Dir { .. } => b"0000755\0",
            Captured::Symlink { .. } => b"0000777\0",
        });
        header[108..116].copy_from_slice(b"0000000\0");
        header[116..124].copy_from_slice(b"0000000\0");
        write_octal(&mut header[124..136], bytes.len() as u64)?;
        header[136..148].copy_from_slice(b"00000000000\0");
        header[148..156].fill(b' ');
        // The typeflags `parse_ustar` admits: regular file, directory, symlink.
        header[156] = match &entry {
            Captured::File { .. } => b'0',
            Captured::Dir { .. } => b'5',
            Captured::Symlink { .. } => b'2',
        };
        if let Captured::Symlink { target, .. } = &entry {
            if target.len() > 100 {
                return Err(format!("exec output symlink target `{target}` exceeds ustar v1"));
            }
            header[157..157 + target.len()].copy_from_slice(target.as_bytes());
        }
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
