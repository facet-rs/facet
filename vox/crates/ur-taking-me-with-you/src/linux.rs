//! Linux implementation using prctl(PR_SET_PDEATHSIG)

use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};

/// Configure the current process to receive SIGKILL when parent dies.
pub fn die_with_parent() {
    unsafe {
        let result = libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
        if result != 0 {
            eprintln!(
                "ur-taking-me-with-you: prctl(PR_SET_PDEATHSIG) failed: {}",
                io::Error::last_os_error()
            );
        }
    }
}

/// Spawn a child that will die when this process dies.
pub fn spawn_dying_with_parent(mut command: Command) -> io::Result<Child> {
    // SAFETY: prctl(PR_SET_PDEATHSIG) is async-signal-safe
    unsafe {
        command.pre_exec(|| {
            let result = libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            if result != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }

    command.spawn()
}
