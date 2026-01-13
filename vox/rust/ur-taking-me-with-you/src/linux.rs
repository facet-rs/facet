//! Linux implementation using prctl(PR_SET_PDEATHSIG)

use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};

/// The pre_exec hook that sets up parent death signal.
/// SAFETY: prctl(PR_SET_PDEATHSIG) is async-signal-safe.
fn set_pdeathsig() -> io::Result<()> {
    let result = unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Configure the current process to receive SIGKILL when parent dies.
pub fn die_with_parent() {
    if let Err(e) = set_pdeathsig() {
        eprintln!(
            "ur-taking-me-with-you: prctl(PR_SET_PDEATHSIG) failed: {}",
            e
        );
    }
}

/// Spawn a child that will die when this process dies.
pub fn spawn_dying_with_parent(mut command: Command) -> io::Result<Child> {
    unsafe { command.pre_exec(set_pdeathsig) };
    command.spawn()
}

/// Spawn a child (async) that will die when this process dies.
#[cfg(feature = "tokio")]
pub fn spawn_dying_with_parent_async(
    mut command: tokio::process::Command,
) -> io::Result<tokio::process::Child> {
    unsafe { command.pre_exec(set_pdeathsig) };
    command.spawn()
}
