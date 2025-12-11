//! # ur-taking-me-with-you
//!
//! Ensure child processes die when their parent dies.
//!
//! On Unix systems, child processes normally continue running even after their parent
//! exits (they get reparented to init/PID 1). This crate provides mechanisms to ensure
//! child processes are terminated when their parent dies, even if the parent is killed
//! with SIGKILL.
//!
//! ## Platform Support
//!
//! - **Linux**: Uses `prctl(PR_SET_PDEATHSIG, SIGKILL)` - the child receives SIGKILL
//!   when its parent thread dies.
//! - **macOS**: Uses a pipe-based approach - the child monitors a pipe from the parent
//!   and exits when the pipe closes (indicating parent death).
//! - **Windows**: Not yet supported.
//!
//! ## Usage
//!
//! ### For the child process (call early in main):
//!
//! ```no_run
//! ur_taking_me_with_you::die_with_parent();
//! ```
//!
//! ### For spawning children with std::process::Command:
//!
//! ```no_run
//! use std::process::Command;
//!
//! let mut cmd = Command::new("my-plugin");
//! cmd.arg("--foo");
//!
//! let child = ur_taking_me_with_you::spawn_dying_with_parent(cmd)
//!     .expect("failed to spawn");
//! ```

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod unsupported;

use std::io;
use std::process::{Child, Command};

/// Configure the current process to die when its parent dies.
///
/// This should be called early in the child process's main function.
///
/// # Platform Behavior
///
/// - **Linux**: Calls `prctl(PR_SET_PDEATHSIG, SIGKILL)`. The process will receive
///   SIGKILL when its parent thread terminates.
/// - **macOS**: This is a no-op. Use `spawn_dying_with_parent` instead, which sets
///   up a pipe-based monitoring mechanism.
/// - **Other platforms**: No-op with a warning.
///
/// # Example
///
/// ```no_run
/// ur_taking_me_with_you::die_with_parent();
/// // ... rest of plugin code
/// ```
pub fn die_with_parent() {
    #[cfg(target_os = "linux")]
    linux::die_with_parent();

    #[cfg(target_os = "macos")]
    macos::die_with_parent();

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    unsupported::die_with_parent();
}

/// Spawn a child process that will die when this (parent) process dies.
///
/// This wraps `Command::spawn()` with platform-specific setup to ensure the child
/// is terminated when the parent exits, even if the parent is killed with SIGKILL.
///
/// # Platform Behavior
///
/// - **Linux**: Uses `pre_exec` to call `prctl(PR_SET_PDEATHSIG, SIGKILL)` in the
///   child before exec.
/// - **macOS**: Creates a pipe and passes the read end to the child. A watchdog
///   thread in the child monitors the pipe and calls `exit()` when it closes.
///
/// # Example
///
/// ```no_run
/// use std::process::Command;
///
/// let mut cmd = Command::new("my-plugin");
/// cmd.arg("--config").arg("/path/to/config");
///
/// let child = ur_taking_me_with_you::spawn_dying_with_parent(cmd)
///     .expect("failed to spawn plugin");
/// ```
pub fn spawn_dying_with_parent(command: Command) -> io::Result<Child> {
    #[cfg(target_os = "linux")]
    return linux::spawn_dying_with_parent(command);

    #[cfg(target_os = "macos")]
    return macos::spawn_dying_with_parent(command);

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    return unsupported::spawn_dying_with_parent(command);
}

/// Environment variable name used on macOS to pass the death-watch pipe FD.
#[cfg(target_os = "macos")]
pub const DEATH_PIPE_ENV: &str = "UR_TAKING_ME_WITH_YOU_FD";
