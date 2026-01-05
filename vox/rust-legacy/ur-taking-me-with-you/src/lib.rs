#![doc = include_str!("../README.md")]
// This crate requires unsafe for platform-specific FFI (libc calls for pipe/process management)
#![allow(unsafe_code)]

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "windows")]
mod windows;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
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
/// - **macOS**: Checks for a death-watch pipe passed via environment variable and
///   starts a watchdog thread if present. Use `spawn_dying_with_parent` to set this up.
/// - **Windows**: Creates a job object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` and
///   assigns the current process to it.
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

    #[cfg(target_os = "windows")]
    windows::die_with_parent();

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
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
/// - **macOS**: Creates a pipe and passes the read end to the child via environment.
///   The child must call `die_with_parent()` to start the watchdog thread.
/// - **Windows**: Creates a job object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` and
///   assigns the child process to it. Note: small race window between spawn and assignment.
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

    #[cfg(target_os = "windows")]
    return windows::spawn_dying_with_parent(command);

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    return unsupported::spawn_dying_with_parent(command);
}

/// Environment variable name used on macOS to pass the death-watch pipe FD.
#[cfg(target_os = "macos")]
pub const DEATH_PIPE_ENV: &str = "UR_TAKING_ME_WITH_YOU_FD";

/// Spawn a child process (async) that will die when this (parent) process dies.
///
/// Same as `spawn_dying_with_parent` but takes a `tokio::process::Command` and
/// returns a `tokio::process::Child` with async `wait()`.
#[cfg(feature = "tokio")]
pub fn spawn_dying_with_parent_async(
    command: tokio::process::Command,
) -> io::Result<tokio::process::Child> {
    #[cfg(target_os = "linux")]
    return linux::spawn_dying_with_parent_async(command);

    #[cfg(target_os = "macos")]
    return macos::spawn_dying_with_parent_async(command);

    #[cfg(target_os = "windows")]
    return windows::spawn_dying_with_parent_async(command);

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    return unsupported::spawn_dying_with_parent_async(command);
}
